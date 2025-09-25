#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use scale_info::TypeInfo;
use parity_scale_codec::{Encode, Decode, MaxEncodedLen};

/// A generic, no_std-friendly adapter for any 2-player, turn-based, perfect-information game.
pub trait GameAdapter {
    /// Game state snapshot for search.
    type State: Clone + TypeInfo + Encode + Decode + MaxEncodedLen + core::fmt::Debug + PartialEq + Eq;
    /// An action that the current player can take.
    type Action: Clone + TypeInfo + Encode + Decode + MaxEncodedLen + core::fmt::Debug + PartialEq + Eq;
    /// Identifier for a player (you can use u8 {0,1}, AccountId, etc.)
    type Player: Copy + Eq + TypeInfo + Encode + Decode;

    /// List legal actions for `state`. Fill into `out` and return count used.
    fn list_actions<const MAX: usize>(
        state: &Self::State,
        out: &mut [Option<Self::Action>; MAX],
    ) -> usize;

    /// Apply `action` to `state`, producing the next state.
    fn apply(state: &Self::State, action: &Self::Action) -> Self::State;

    /// Whether this state is terminal.
    fn is_terminal(state: &Self::State) -> bool;

    /// Which player is to move.
    fn current_player(state: &Self::State) -> Self::Player;

    /// Terminal scoring from the perspective of `for_player`. Higher is better.
    /// Non-terminal states may return heuristic estimates.
    fn score(state: &Self::State, for_player: Self::Player) -> i32;

    /// Uniform-ish random legal action for playouts (return None if none).
    /// Use `seed` deterministically to stay consensus-safe on-chain.
    fn random_action(state: &Self::State, seed: u64) -> Option<Self::Action>;
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*, dispatch::DispatchResultWithPostInfo};
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::Hash;
    use parity_scale_codec::Encode;
    use frame_support::sp_runtime::traits::Hash as HashTrait;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Provide the concrete adapter to use for AI.
        type Adapter: GameAdapter;

        /// Maximum branching factor the adapter promises (upper-bound on legal actions).
        #[pallet::constant]
        type MaxActions: Get<u32>;

        /// Base number of iterations per move evaluation (scaled by difficulty).
        #[pallet::constant]
        type BaseIterations: Get<u32>;

        /// Maximum playout depth to avoid long games during rollouts.
        #[pallet::constant]
        type MaxPlayoutDepth: Get<u16>;

        /// Seed used for deterministic PRNG inside the pallet.
        #[pallet::constant]
        type RandomnessSeed: Get<u64>;
    }

    #[pallet::storage]
    /// Simple deterministic nonce for PRNG.
    pub type Nonce<T: Config> = StorageValue<_, u64, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// A suggested action has been computed for the given state hash.
        Suggested {
            state_hash: T::Hash,
            difficulty: u8,
            iterations: u32,
            /// The suggested action (SCALE-encoded by RPC users if needed).
            action: <T::Adapter as GameAdapter>::Action,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        NoLegalMoves,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Ask the AI to suggest the best action by Monte-Carlo (rollout) search.
        /// `difficulty` in 0..=100 scales the iterations.
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn suggest_move(
            origin: OriginFor<T>,
            state: <T::Adapter as GameAdapter>::State,
            difficulty: u8,
        ) -> DispatchResultWithPostInfo {
            let _ = ensure_signed(origin)?; // optionally allow unsigned

            let action = Self::suggest::<T::Adapter>(&state, difficulty)
                .ok_or(Error::<T>::NoLegalMoves)?;

            let state_hash: T::Hash = <T::Hashing as HashTrait>::hash_of(&state);
            let iters = Self::scaled_iterations::<T>(difficulty);
            Self::deposit_event(Event::Suggested {
                state_hash,
                difficulty,
                iterations: iters,
                action: action.clone(),
            });

            Ok(Pays::No.into())
        }
    }

    impl<T: Config> Pallet<T> {
        #[inline]
        pub fn scaled_iterations<C: Config>(difficulty: u8) -> u32 {
            let base = C::BaseIterations::get().max(1);
            // map 0..100 to 0.5x .. 3x the base (tune as needed)
            let mult_num = 50 + 2 * difficulty as u32; // 50..250
            (base * mult_num) / 100
        }

        #[inline]
        pub fn prng_u64<C: Config>(salt: u64) -> u64 {
            // Deterministic, cheap PRNG for no_std.
            let n = Nonce::<C>::get();
            let seed = C::RandomnessSeed::get();
            let mix = seed ^ n ^ salt;

            // SplitMix64-ish
            let mut z = mix.wrapping_add(0x9E3779B97F4A7C15);
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
            let out = z ^ (z >> 31);

            Nonce::<C>::put(n.wrapping_add(1));
            out
        }

        /// Monte-Carlo rollout suggestor (per-action averaging).
        pub fn suggest<A: GameAdapter>(
            state: &A::State,
            difficulty: u8,
        ) -> Option<A::Action> {
            if A::is_terminal(state) {
                return None;
            }

            // Collect legal actions into a fixed-size buffer
            const MAX_BUF: usize = 128;
            // Ensure compile-time buffer bound is >= runtime bound
            // (Build-time safety: MaxActions <= 128 in tests/runtime config.)
            let mut actions: [Option<A::Action>; MAX_BUF] = core::array::from_fn(|_| None);

            let n = A::list_actions::<MAX_BUF>(state, &mut actions);
            if n == 0 { return None; }

            let iters = Self::scaled_iterations::<T>(difficulty).max(n as u32);
            let sims_per_action = (iters / n as u32).max(1);

            let me = A::current_player(state);

            let mut best_idx = 0usize;
            let mut best_score = i64::MIN;

            for i in 0..n {
                let action = actions[i].as_ref().unwrap();
                let mut accum: i64 = 0;
                for j in 0..sims_per_action {
                    let seed = Self::prng_u64::<T>((i as u64) << 32 | j as u64);
                    let s1 = A::apply(state, action);
                    let outcome = Self::random_playout::<A>(&s1, me, seed);
                    accum += outcome as i64;
                }
                let avg = accum / sims_per_action as i64;
                if avg > best_score {
                    best_score = avg;
                    best_idx = i;
                }
            }

            actions[best_idx].clone()
        }

        fn random_playout<A: GameAdapter>(
            start: &A::State,
            me: A::Player,
            mut seed: u64,
        ) -> i32 {
            let mut s = start.clone();
            let mut depth = 0u16;
            while !A::is_terminal(&s) && depth < T::MaxPlayoutDepth::get() {
                if let Some(a) = A::random_action(&s, seed) {
                    s = A::apply(&s, &a);
                } else {
                    break;
                }
                depth = depth.saturating_add(1);
                seed = seed.wrapping_add(0x9E37_79B9); // nudge seed
            }
            A::score(&s, me)
        }
    }
}
