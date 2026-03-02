#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

pub mod weights;
pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod types;

pub use crate::types::GameId;
use frame_support::ensure;
use frame_support::pallet_prelude::ConstU32;
use frame_support::traits::Get;
use frame_support::BoundedVec;
use frame_system::pallet_prelude::BlockNumberFor;
use parity_scale_codec::Encode;
use sp_runtime::{traits::Hash, Saturating};
pub use types::board::Board;
pub use types::card::Card;
pub use types::card::Possession as Player; // PlayerOne / PlayerTwo
pub use types::game::*;

use eterra_card_ai_adapter::eterra_adapter as ai;
use pallet_eterra_monte_carlo_ai as mc_ai; // reserved for future use

#[frame_support::pallet]
pub mod pallet {
    use crate::weights::WeightInfo;
    use frame_support::pallet_prelude::ConstU32;
    use frame_support::traits::StorageVersion;
    use frame_support::traits::{Currency, fungibles};
    use frame_support::BoundedVec;
    use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::Hash;
    use sp_runtime::Saturating;
    use sp_runtime::traits::Zero;

    pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
    pub type BalanceOf<T> =
        <<T as pallet_eterra_simple_tcg::pallet::Config>::Currency as Currency<
            <T as frame_system::Config>::AccountId,
        >>::Balance;

    use crate::types::board::Board;
    use crate::types::card::Card;

    use crate::types::game::Move;
    use crate::types::game::*;
    use crate::types::GameId;
    // Alias the simple TCG pallet so we can read card ownership & stats

    use pallet_eterra_monte_carlo_ai as mc_ai;
    use pallet_eterra_simple_tcg as cards; // reserved for future use

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_runtime_upgrade() -> Weight {
            let mut weight = T::DbWeight::get().reads(1);
            if StorageVersion::get::<Pallet<T>>() < STORAGE_VERSION {
                STORAGE_VERSION.put::<Pallet<T>>();
                weight = weight.saturating_add(T::DbWeight::get().writes(1));
            }
            weight
        }

        fn on_initialize(n: BlockNumberFor<T>) -> Weight {
            // If enabled, adjust AI difficulty based on PvE payouts in the configured window.
            Self::maybe_adjust_ai_difficulty(n);
            <T as Config>::WeightInfo::on_initialize()
        }
    }

    #[pallet::config]
    pub trait Config: frame_system::Config + cards::pallet::Config + mc_ai::pallet::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        // Exact number of players that can join a single game
        #[pallet::constant]
        type NumPlayers: Get<u32> + Clone + TypeInfo;
        #[pallet::constant]
        type MaxRounds: Get<u8>;
        #[pallet::constant]
        type BlocksToPlayLimit: Get<u8>;
        /// Exactly how many cards a submitted hand must contain
        #[pallet::constant]
        type HandSize: Get<u32>;
        /// Special account representing the AI opponent in PvE games
        type AiAccount: Get<Self::AccountId>;
        /// Default AI difficulty (0..=100)
        type AiDifficulty: Get<u8>;

        /// Origin permitted to change AI difficulty/controller settings.
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Block counts for controller window options (hour/day/week/month).
        #[pallet::constant]
        type BlocksPerHour: Get<BlockNumberFor<Self>>;
        #[pallet::constant]
        type BlocksPerDay: Get<BlockNumberFor<Self>>;
        #[pallet::constant]
        type BlocksPerWeek: Get<BlockNumberFor<Self>>;
        #[pallet::constant]
        type BlocksPerMonth: Get<BlockNumberFor<Self>>;

        /// Multi-currency assets interface used to reward devCOIN/betaCOIN.
        type Assets: fungibles::Mutate<Self::AccountId, AssetId = u32, Balance = BalanceOf<Self>>;

        /// Experience manager used to reward XP on wins.
        type ExperienceManager: pallet_eterra_gamer::ExperienceManager<Self::AccountId>;

        /// `pallet-assets` id for devCOIN.
        #[pallet::constant]
        type DevCoinAssetId: Get<u32>;

        /// `pallet-assets` id for betaCOIN.
        #[pallet::constant]
        type BetaCoinAssetId: Get<u32>;

        /// COIN minted to the winner on each win.
        #[pallet::constant]
        type WinRewardCoin: Get<BalanceOf<Self>>;

        /// devCOIN minted to the winner on each win.
        #[pallet::constant]
        type WinRewardDevCoin: Get<BalanceOf<Self>>;

        /// betaCOIN minted to the winner on each win.
        #[pallet::constant]
        type WinRewardBetaCoin: Get<BalanceOf<Self>>;

        /// Experience granted to the winner on each win.
        #[pallet::constant]
        type WinRewardExperience: Get<u128>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: crate::weights::WeightInfo;
    }

    /// Which fungible payout the AI controller should target.
    #[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub enum TrackedPayoutCurrency {
        Coin,
        DevCoin,
        BetaCoin,
    }

    /// Rolling window length used to compare payouts against a target.
    #[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub enum PayoutWindow {
        Hour,
        Day,
        Week,
        Month,
    }

    /// Configuration for the on-chain AI difficulty controller.
    ///
    /// The controller adjusts `CurrentAiDifficulty` to target a payout schedule for PvE wins.
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub struct AiDifficultyControllerConfig<BlockNumber, Balance> {
        pub tracked_currency: TrackedPayoutCurrency,
        pub window: PayoutWindow,
        pub target_per_window: Balance,
        /// How frequently (in blocks) to run adjustments in `on_initialize`.
        pub adjust_period: BlockNumber,
        pub min_difficulty: u8,
        pub max_difficulty: u8,
        /// Step size to change difficulty by (0 disables adjustments).
        pub step: u8,
        /// Absolute deadband around the expected payout (prevents oscillation).
        pub deadband: Balance,
    }

    /// Tracks payouts in the current controller window.
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub struct PayoutWindowState<BlockNumber, Balance> {
        pub window_start: BlockNumber,
        pub last_adjust: BlockNumber,
        pub paid_coin: Balance,
        pub paid_dev_coin: Balance,
        pub paid_beta_coin: Balance,
    }

    #[pallet::type_value]
    pub fn DefaultAiDifficulty<T: Config>() -> u8 {
        T::AiDifficulty::get()
    }

    /// Current AI difficulty used for PvE suggestions (0..=100).
    #[pallet::storage]
    #[pallet::getter(fn current_ai_difficulty)]
    pub type CurrentAiDifficulty<T: Config> =
        StorageValue<_, u8, ValueQuery, DefaultAiDifficulty<T>>;

    /// Optional controller configuration. When `None`, difficulty remains whatever was set manually.
    #[pallet::storage]
    #[pallet::getter(fn ai_difficulty_controller)]
    pub type AiDifficultyController<T: Config> = StorageValue<
        _,
        AiDifficultyControllerConfig<BlockNumberFor<T>, BalanceOf<T>>,
        OptionQuery,
    >;

    #[pallet::type_value]
    pub fn DefaultPayoutWindowState<T: Config>() -> PayoutWindowState<BlockNumberFor<T>, BalanceOf<T>> {
        PayoutWindowState {
            window_start: Zero::zero(),
            last_adjust: Zero::zero(),
            paid_coin: Zero::zero(),
            paid_dev_coin: Zero::zero(),
            paid_beta_coin: Zero::zero(),
        }
    }

    /// Controller window state (payout counters + timing).
    #[pallet::storage]
    #[pallet::getter(fn ai_payout_window_state)]
    pub type AiPayoutWindowState<T: Config> = StorageValue<
        _,
        PayoutWindowState<BlockNumberFor<T>, BalanceOf<T>>,
        ValueQuery,
        DefaultPayoutWindowState<T>,
    >;

    #[pallet::storage]
    #[pallet::getter(fn game_board)]
    pub type GameStorage<T: Config> = StorageMap<
        _, // Explicit prefix using the pallet type
        Blake2_128Concat,
        GameId<T>,
        Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers>, // Store the complete game struct
    >;

    #[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, MaxEncodedLen, Debug)]
    pub enum GameMode {
        PvP,
        PvE,
    }

    #[pallet::storage]
    #[pallet::getter(fn game_mode_of)]
    pub type GameModes<T: Config> =
        StorageMap<_, Blake2_128Concat, GameId<T>, GameMode, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn active_game_of)]
    /// Tracks if an account is currently in an active game. A player may have at most one.
    pub type ActiveGameOf<T: Config> =
        StorageMap<_, Blake2_128Concat, AccountIdOf<T>, GameId<T>, OptionQuery>;

    /// Recent games for each player (most-recent first, bounded).
    #[pallet::storage]
    #[pallet::getter(fn player_games)]
    pub type PlayerGames<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        AccountIdOf<T>,
        BoundedVec<GameId<T>, ConstU32<10>>, // keep last 10
        ValueQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        GameCreated {
            game_id: GameId<T>,
        },
        MovePlayed {
            game_id: GameId<T>,
            player: T::AccountId,
            x: u8,
            y: u8,
        },
        GameFinished {
            game_id: GameId<T>,
            winner: Option<T::AccountId>,
        },
        /// Rewards were issued to a winner (if any) as part of finishing a game.
        WinRewardsIssued {
            game_id: GameId<T>,
            winner: T::AccountId,
            coin: BalanceOf<T>,
            dev_coin: BalanceOf<T>,
            beta_coin: BalanceOf<T>,
            exp: u128,
        },
        /// AI difficulty was set manually by an admin origin.
        AiDifficultySet {
            old: u8,
            new: u8,
        },
        /// AI difficulty controller configuration was updated (and the window was reset).
        AiDifficultyControllerSet {
            tracked_currency: TrackedPayoutCurrency,
            window: PayoutWindow,
            target_per_window: BalanceOf<T>,
            adjust_period: BlockNumberFor<T>,
            min_difficulty: u8,
            max_difficulty: u8,
            step: u8,
            deadband: BalanceOf<T>,
        },
        /// AI difficulty controller was disabled (config removed).
        AiDifficultyControllerDisabled,
        /// AI difficulty was adjusted automatically by the controller.
        AiDifficultyAdjusted {
            old: u8,
            new: u8,
            tracked_currency: TrackedPayoutCurrency,
            paid: BalanceOf<T>,
            expected: BalanceOf<T>,
        },
        //New Turn
        NewTurn {
            game_id: GameId<T>,
            next_player: AccountIdOf<T>,
        },
        TurnForceFinished {
            game_id: GameId<T>,
            player: AccountIdOf<T>,
        },
        HandSubmitted {
            game_id: GameId<T>,
            player: AccountIdOf<T>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        GameNotFound,
        InvalidMove,
        NotYourTurn,
        CellOccupied,
        InvalidNumberOfPlayers,
        InternalError,
        BlocksToPlayLimitNotPassed,
        CurrentPlayerCannotForceFinishTurn,
        PlayerNotInGame,
        CreatorMustBeInGame,
        // Hand / deck errors
        HandSizeInvalid,
        DuplicateCardInHand,
        HandAlreadySubmitted,
        HandNotSubmitted,
        HandIndexOutOfRange,
        CardAlreadyUsed,
        CardDoesNotExist,
        CardNotOwned,
        PlayerAlreadyInGame,
        PresetHandMissing,
        GameNotActive,
        InvalidAiDifficulty,
        InvalidControllerConfig,
    }

    /// Limit of cards per hand (defaults to 5 via Config::HandSize)
    pub type HandLimit = ConstU32<5>;

    /// A single entry in a player's submitted hand
    #[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, MaxEncodedLen, Debug)]
    pub struct HandEntry {
        pub card_id: u32,
        pub north: u8,
        pub east: u8,
        pub south: u8,
        pub west: u8,
        pub used: bool,
    }

    /// Stores each player's hand for a given game.
    /// Keyed by (game_id, account_id) -> bounded vec of exactly HandSize entries.
    #[pallet::storage]
    #[pallet::getter(fn game_hands)]
    pub type HandsOfGame<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        GameId<T>,
        Blake2_128Concat,
        AccountIdOf<T>,
        BoundedVec<HandEntry, HandLimit>,
        OptionQuery,
    >;

    /// The player's current hand configuration (card IDs only). This is editable by the user in the UI.
    #[pallet::storage]
    #[pallet::getter(fn current_hand_of)]
    pub type CurrentHandOf<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        AccountIdOf<T>,
        BoundedVec<u32, HandLimit>, // exactly HandLimit entries expected by the UI flow
        OptionQuery,
    >;

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::create_game())]
        pub fn create_game(
            origin: OriginFor<T>,
            mut players: BoundedVec<AccountIdOf<T>, T::NumPlayers>,
            game_mode: GameMode,
        ) -> DispatchResult {
            let who: AccountIdOf<T> = ensure_signed(origin)?;

            // Require the creator to have a current hand before starting a game
            ensure!(
                CurrentHandOf::<T>::contains_key(&who),
                Error::<T>::PresetHandMissing
            );

            // Normalize players vector depending on mode
            match game_mode {
                GameMode::PvP => {
                    // For PvP, the caller must have included themselves and exactly one opponent.
                    ensure!(players.contains(&who), Error::<T>::CreatorMustBeInGame);
                    ensure!(
                        players.len()
                            == <u32 as sp_runtime::traits::SaturatedConversion>::saturated_into::<
                                usize,
                            >(T::NumPlayers::get()),
                        Error::<T>::InvalidNumberOfPlayers
                    );
                    // Ensure distinct players; also normalize order to [creator, opponent]
                    // so downstream logic is predictable.
                    ensure!(players[0] != players[1], Error::<T>::InvalidMove);
                    if players[0] != who {
                        // Put creator in slot 0 while keeping a bounded structure.
                        if players[1] == who {
                            let opponent = players[0].clone();
                            let mut normalized = BoundedVec::<AccountIdOf<T>, T::NumPlayers>::new();
                            normalized
                                .try_push(who.clone())
                                .map_err(|_| Error::<T>::InvalidNumberOfPlayers)?;
                            normalized
                                .try_push(opponent)
                                .map_err(|_| Error::<T>::InvalidNumberOfPlayers)?;
                            players = normalized;
                        } else {
                            // Shouldn’t happen because of the contains() check, but be safe.
                            return Err(Error::<T>::CreatorMustBeInGame.into());
                        }
                    }
                }
                GameMode::PvE => {
                    // For PvE, ignore whatever was passed and force [creator, AI].
                    let ai_acc = T::AiAccount::get();
                    // Also guard against creator == AI account (shouldn’t happen for sane config).
                    ensure!(who != ai_acc, Error::<T>::InvalidMove);
                    let mut forced = BoundedVec::<AccountIdOf<T>, T::NumPlayers>::new();
                    forced
                        .try_push(who.clone())
                        .map_err(|_| Error::<T>::InvalidNumberOfPlayers)?;
                    forced
                        .try_push(ai_acc)
                        .map_err(|_| Error::<T>::InvalidNumberOfPlayers)?;
                    players = forced;
                }
            }

            // From here on, `players` is normalized for both modes.
            let number_of_players = players.len();
            ensure!(
                number_of_players
                    == <u32 as sp_runtime::traits::SaturatedConversion>::saturated_into::<usize>(
                        T::NumPlayers::get()
                    ),
                Error::<T>::InvalidNumberOfPlayers
            );

            let creator = players[0].clone();
            let opponent = players[1].clone();

            // Redundant after normalization, but keep as a safety net.
            ensure!(creator != opponent, Error::<T>::InvalidMove);

            // Enforce: a wallet may participate in at most one active game, scoped by mode.
            match game_mode {
                GameMode::PvP => {
                    ensure!(
                        ActiveGameOf::<T>::get(&creator).is_none(),
                        Error::<T>::PlayerAlreadyInGame
                    );
                    ensure!(
                        ActiveGameOf::<T>::get(&opponent).is_none(),
                        Error::<T>::PlayerAlreadyInGame
                    );
                }
                GameMode::PvE => {
                    // Only the human creator is restricted in PvE; the AI may participate in many games.
                    ensure!(
                        ActiveGameOf::<T>::get(&creator).is_none(),
                        Error::<T>::PlayerAlreadyInGame
                    );
                }
            }

            let current_block_number = <frame_system::Pallet<T>>::block_number();
            let game_id =
                T::Hashing::hash_of(&(creator.clone(), opponent.clone(), current_block_number));

            // Ensure the game_id isn’t already in use (collision check)
            ensure!(
                !GameStorage::<T>::contains_key(&game_id),
                Error::<T>::GameNotFound
            );

            let initial_board: Board = Default::default();
            let initial_scores = (5, 5);

            let mut game: Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers> = Game {
                state: GameState::Playing,
                last_played_block: current_block_number,
                players: players.clone(),
                player_turn: 0,
                round: 0,
                max_rounds: T::MaxRounds::get(),
                board: initial_board.clone(),
                scores: initial_scores,
            };

            GameModes::<T>::insert(&game_id, game_mode.clone());
            // Mark participants as busy with this game
            match game_mode {
                GameMode::PvP => {
                    ActiveGameOf::<T>::insert(&creator, game_id);
                    ActiveGameOf::<T>::insert(&opponent, game_id);
                }
                GameMode::PvE => {
                    // Only mark the human creator as active; AI is allowed to be in many games simultaneously.
                    ActiveGameOf::<T>::insert(&creator, game_id);
                }
            }

            // Update per-player recent game lists (most-recent first, dedup, prune to 10)
            let push_recent = |acct: &AccountIdOf<T>| {
                PlayerGames::<T>::mutate(acct, |list| {
                    if let Some(pos) = list.iter().position(|g| *g == game_id) {
                        list.remove(pos);
                    }
                    // Try to insert at the front; if full, pop last first
                    if list.len() as u32 >= <ConstU32<10> as sp_runtime::traits::Get<u32>>::get() {
                        let _ = list.pop();
                    }
                    // Insert at front by rebuilding (BoundedVec has no direct insert at 0)
                    let mut tmp = list.to_vec();
                    tmp.insert(0, game_id);
                    *list = BoundedVec::try_from(tmp).expect("<= 10; qed");
                });
            };
            push_recent(&creator);
            push_recent(&opponent);

            // If PvE, create AI hand immediately so UI can render it.
            if matches!(game_mode, GameMode::PvE) {
                let ai_acc = T::AiAccount::get();
                if HandsOfGame::<T>::get(&game_id, &ai_acc).is_none() {
                    if let Some(ai_hand) = Self::generate_ai_hand_default(&game_id) {
                        HandsOfGame::<T>::insert(&game_id, &ai_acc, ai_hand);
                    }
                }
            }

            // Set starting player: PvE -> creator always starts; PvP -> keep randomized start
            if matches!(game_mode, GameMode::PvE) {
                // players[0] is guaranteed to be the creator after normalization above
                game.set_player_turn(0);
            } else {
                // PvP: randomize starting player based on creator hash
                game.set_player_turn(
                    if sp_io::hashing::blake2_128(&creator.encode())[0] % 2 == 0 {
                        0
                    } else {
                        1
                    },
                );
            }

            GameStorage::<T>::insert(&game_id, game.clone());
            Self::deposit_event(Event::GameCreated { game_id });
            Ok(())
        }
        /// Dev-only: play using a raw `Move`. Production uses `play_from_hand`.
        #[cfg(any(test, feature = "dev-play"))]
        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::play())]
        pub fn play(origin: OriginFor<T>, game_id: GameId<T>, player_move: Move) -> DispatchResult {
            let who = ensure_signed(origin)?;

            log::debug!(
                "Player {:?} is attempting to play on game_id {:?} at {:?}",
                who,
                game_id,
                player_move
            );

            let mut game = GameStorage::<T>::get(&game_id).ok_or(Error::<T>::GameNotFound)?;
            ensure!(
                matches!(game.state, GameState::Playing),
                Error::<T>::GameNotActive
            );

            // Validate the current player's turn and move
            Self::validate_player_turn(&game, &who)?;
            Self::validate_move(&game, &player_move)?;

            // Determine the current player's index (0 or 1)
            let player_ix = Self::get_current_player_index(&game, &who);

            // Place the card on the board
            Self::place_card_on_board(&mut game, &player_move, player_ix);

            // Capture logic
            Self::apply_capture_logic(&mut game, &player_move, player_ix);

            // Update the last_played_block to the current block number
            let current_block = <frame_system::Pallet<T>>::block_number();
            game.last_played_block = current_block;

            // Check if the game is won
            // if let Some(winner) = Self::is_game_won(&game_id, &game) {
            //     Self::end_game(&game_id, winner);
            //     return Ok(());
            // }

            // Update to the next turn
            game.next_turn();

            log::debug!(
                "Saving game state after next_turn. Current round: {}, player_turn: {}",
                game.round,
                game.player_turn
            );

            // Emit a NewTurn event for the new current player
            let next_player = game.players[game.get_player_turn() as usize].clone();
            Self::deposit_event(Event::NewTurn {
                game_id,
                next_player,
            });

            // Save the updated game
            GameStorage::<T>::insert(&game_id, game.clone());

            // Check if the game is won after updating the round
            if let Some(winner) = Self::is_game_won(&game_id, &game) {
                Self::end_game(&game_id, winner);
                return Ok(());
            }

            log::debug!(
                "Next turn belongs to: {:?}",
                game.players[game.get_player_turn() as usize]
            );

            Self::deposit_event(Event::MovePlayed {
                game_id,
                player: who,
                x: player_move.place_index_x,
                y: player_move.place_index_y,
            });

            // If this is a PvE game and it's now the AI's turn, let the AI act immediately.
            if matches!(GameModes::<T>::get(&game_id), Some(GameMode::PvE)) {
                if let Some(mut g) = GameStorage::<T>::get(&game_id) {
                    Self::maybe_ai_take_turn(&game_id, &mut g);
                }
            }

            Ok(())
        }

        /// Submit your current 5-card hand for this game. The submitted hand is always loaded from your current hand configuration.
        /// The `card_ids` argument is ignored and exists for ABI compatibility only.
        #[pallet::call_index(2)]
        #[pallet::weight(<T as Config>::WeightInfo::submit_hand())]
        pub fn submit_hand(
            origin: OriginFor<T>,
            game_id: GameId<T>,
            _card_ids: BoundedVec<u32, HandLimit>,
        ) -> DispatchResult {
            let who: AccountIdOf<T> = ensure_signed(origin)?;

            // Ensure the game exists and the caller is a player in it
            let game = GameStorage::<T>::get(&game_id).ok_or(Error::<T>::GameNotFound)?;
            ensure!(
                matches!(game.state, GameState::Playing),
                Error::<T>::GameNotActive
            );
            ensure!(game.players.contains(&who), Error::<T>::PlayerNotInGame);

            // Prevent resubmission for this game
            ensure!(
                HandsOfGame::<T>::get(&game_id, &who).is_none(),
                Error::<T>::HandAlreadySubmitted
            );

            // Load the caller's current hand configuration and snapshot it into the game
            let current_ids = CurrentHandOf::<T>::get(&who).ok_or(Error::<T>::PresetHandMissing)?;
            ensure!(
                current_ids.len() as u32 == T::HandSize::get(),
                Error::<T>::HandSizeInvalid
            );

            // Validate uniqueness (defense in depth)
            for i in 0..current_ids.len() {
                for j in (i + 1)..current_ids.len() {
                    ensure!(
                        current_ids[i] != current_ids[j],
                        Error::<T>::DuplicateCardInHand
                    );
                }
            }

            // Build hand entries from the cards pallet; validate ownership & existence
            let mut hand: BoundedVec<HandEntry, HandLimit> = BoundedVec::default();
            for &card_id in current_ids.iter() {
                let info =
                    cards::pallet::Cards::<T>::get(card_id).ok_or(Error::<T>::CardDoesNotExist)?;
                ensure!(info.owner == who, Error::<T>::CardNotOwned);
                let entry = HandEntry {
                    card_id,
                    north: info.north,
                    east: info.east,
                    south: info.south,
                    west: info.west,
                    used: false,
                };
                hand.try_push(entry)
                    .map_err(|_| Error::<T>::HandSizeInvalid)?;
            }

            HandsOfGame::<T>::insert(&game_id, &who, hand);
            Self::deposit_event(Event::HandSubmitted {
                game_id,
                player: who.clone(),
            });

            // PvE: submitting player is always the human. Generate AI hand right away,
            // and if it's AI's turn (e.g., AI won first move), let it act immediately.
            if matches!(GameModes::<T>::get(&game_id), Some(GameMode::PvE)) {
                let ai_acc = T::AiAccount::get();
                if HandsOfGame::<T>::get(&game_id, &ai_acc).is_none() {
                    if let Some(ai_hand) = Self::generate_ai_hand_for_game(&game_id, &who) {
                        HandsOfGame::<T>::insert(&game_id, &ai_acc, ai_hand);
                    }
                }
                // If AI is up next, take its turn now that it has a hand.
                if let Some(mut game) = GameStorage::<T>::get(&game_id) {
                    Self::maybe_ai_take_turn(&game_id, &mut game);
                }
            }
            Ok(())
        }

        /// Play a card by referencing its index in the submitted hand (0..HandSize-1).
        #[pallet::call_index(3)]
        #[pallet::weight(<T as Config>::WeightInfo::play_from_hand())]
        pub fn play_from_hand(
            origin: OriginFor<T>,
            game_id: GameId<T>,
            hand_index: u8,
            x: u8,
            y: u8,
        ) -> DispatchResult {
            let who: AccountIdOf<T> = ensure_signed(origin)?;

            // Load game
            let mut game = GameStorage::<T>::get(&game_id).ok_or(Error::<T>::GameNotFound)?;
            ensure!(
                matches!(game.state, GameState::Playing),
                Error::<T>::GameNotActive
            );

            // Validate it's the caller's turn and the target cell is open
            Self::validate_player_turn(&game, &who)?;
            ensure!(x < 4 && y < 4, Error::<T>::InvalidMove);
            ensure!(
                game.board[x as usize][y as usize].is_none(),
                Error::<T>::CellOccupied
            );

            // Get caller's hand
            let mut hand =
                HandsOfGame::<T>::get(&game_id, &who).ok_or(Error::<T>::HandNotSubmitted)?;
            let idx = hand_index as usize;
            ensure!(idx < hand.len(), Error::<T>::HandIndexOutOfRange);
            ensure!(!hand[idx].used, Error::<T>::CardAlreadyUsed);

            // Build the placed card from the saved stats
            let player_ix = Self::get_current_player_index(&game, &who);
            let h = hand[idx].clone();
            let placed = Card {
                top: h.north,
                right: h.east,
                bottom: h.south,
                left: h.west,
                possession: None,
            };
            let mv = Move {
                place_card: placed,
                place_index_x: x,
                place_index_y: y,
            };

            // Place the card and resolve capture logic (mirrors `play`)
            Self::place_card_on_board(&mut game, &mv, player_ix);
            Self::apply_capture_logic(&mut game, &mv, player_ix);

            // Mark card as used and persist the hand
            hand[idx].used = true;
            HandsOfGame::<T>::insert(&game_id, &who, hand);

            // Update timing and turn
            let current_block = <frame_system::Pallet<T>>::block_number();
            game.last_played_block = current_block;
            game.next_turn();

            // Emit events and save game
            let next_player = game.players[game.get_player_turn() as usize].clone();
            Self::deposit_event(Event::NewTurn {
                game_id,
                next_player,
            });
            GameStorage::<T>::insert(&game_id, game.clone());

            Self::deposit_event(Event::MovePlayed {
                game_id,
                player: who,
                x,
                y,
            });

            // Check for win condition after saving
            if let Some(winner) = Self::is_game_won(&game_id, &game) {
                Self::end_game(&game_id, winner);
                return Ok(());
            }

            // If this is a PvE game and it's now the AI's turn, let the AI act immediately.
            if matches!(GameModes::<T>::get(&game_id), Some(GameMode::PvE)) {
                if let Some(mut g) = GameStorage::<T>::get(&game_id) {
                    Self::maybe_ai_take_turn(&game_id, &mut g);
                }
            }

            Ok(())
        }

        #[pallet::call_index(4)]
        #[pallet::weight(<T as Config>::WeightInfo::force_finish_turn())]
        pub fn force_finish_turn(origin: OriginFor<T>, game_id: GameId<T>) -> DispatchResult {
            let who: AccountIdOf<T> = ensure_signed(origin)?;

            // Retrieve the game from storage
            let mut game = GameStorage::<T>::get(&game_id).ok_or(Error::<T>::GameNotFound)?;
            ensure!(
                matches!(game.state, GameState::Playing),
                Error::<T>::GameNotActive
            );

            // Ensure the caller is a player in the game
            ensure!(game.players.contains(&who), Error::<T>::PlayerNotInGame);

            // Ensure the caller is not the current player
            let current_player = game.players[game.player_turn as usize].clone();
            ensure!(
                current_player != who,
                Error::<T>::CurrentPlayerCannotForceFinishTurn
            );

            // Check if the BlocksToPlayLimit has passed (use saturating math and inclusive deadline)
            let current_block = <frame_system::Pallet<T>>::block_number();
            let limit: BlockNumberFor<T> = T::BlocksToPlayLimit::get().into();
            let deadline = game.last_played_block.saturating_add(limit);
            ensure!(
                current_block >= deadline,
                Error::<T>::BlocksToPlayLimitNotPassed
            );

            // Force finish the current turn
            game.next_turn();
            game.last_played_block = current_block;

            log::debug!(
                "Force finish turn: game_id {:?}, current round: {}, max rounds: {}",
                game_id,
                game.round,
                game.max_rounds
            );

            // ✅ Check if game is won after forcing turn
            if let Some(winner) = Self::is_game_won(&game_id, &game) {
                // End game clears storage and ActiveGameOf markers; early return is fine.
                Self::end_game(&game_id, winner);
                return Ok(());
            }

            // Persist updated game state before emitting events
            GameStorage::<T>::insert(&game_id, game.clone());

            // Emit events
            let next_player = game.players[game.player_turn as usize].clone();
            Self::deposit_event(Event::TurnForceFinished {
                game_id,
                player: current_player,
            });
            Self::deposit_event(Event::NewTurn {
                game_id,
                next_player,
            });

            Ok(())
        }

        /// Save/update your "current hand" (card IDs only) that the UI will use for future games.
        /// The hand must contain exactly `HandSize` unique cards owned by the caller.
        #[pallet::call_index(5)]
        #[pallet::weight(<T as Config>::WeightInfo::set_current_hand())]
        pub fn set_current_hand(
            origin: OriginFor<T>,
            card_ids: BoundedVec<u32, HandLimit>,
        ) -> DispatchResult {
            let who: AccountIdOf<T> = ensure_signed(origin)?;

            // Enforce exact hand size and uniqueness
            ensure!(
                card_ids.len() as u32 == T::HandSize::get(),
                Error::<T>::HandSizeInvalid
            );
            for i in 0..card_ids.len() {
                for j in (i + 1)..card_ids.len() {
                    ensure!(card_ids[i] != card_ids[j], Error::<T>::DuplicateCardInHand);
                }
            }

            // Validate ownership and that each card exists
            for &card_id in &card_ids {
                let info =
                    cards::pallet::Cards::<T>::get(card_id).ok_or(Error::<T>::CardDoesNotExist)?;
                ensure!(info.owner == who, Error::<T>::CardNotOwned);
            }

            // Persist as a bounded vec
            CurrentHandOf::<T>::insert(&who, card_ids);
            Ok(())
        }

        /// Deprecated alias for backwards compatibility. Calls `set_current_hand`.
        #[pallet::call_index(6)]
        #[pallet::weight(<T as Config>::WeightInfo::set_preset_hand())]
        pub fn set_preset_hand(
            origin: OriginFor<T>,
            card_ids: BoundedVec<u32, HandLimit>,
        ) -> DispatchResult {
            Self::set_current_hand(origin, card_ids)
        }

        /// Manually set the AI difficulty used for PvE games.
        ///
        /// This is an on-chain setting (stored in `CurrentAiDifficulty`) so all nodes agree.
        #[pallet::call_index(7)]
        #[pallet::weight(<T as Config>::WeightInfo::set_ai_difficulty())]
        pub fn set_ai_difficulty(origin: OriginFor<T>, new: u8) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(new <= 100, Error::<T>::InvalidAiDifficulty);
            let old = CurrentAiDifficulty::<T>::get();
            CurrentAiDifficulty::<T>::put(new);
            Self::deposit_event(Event::AiDifficultySet { old, new });
            Ok(())
        }

        /// Configure the automatic AI difficulty controller.
        ///
        /// Only PvE wins against the AI are counted toward payouts in the window.
        #[pallet::call_index(8)]
        #[pallet::weight(<T as Config>::WeightInfo::set_ai_difficulty_controller())]
        pub fn set_ai_difficulty_controller(
            origin: OriginFor<T>,
            tracked_currency: TrackedPayoutCurrency,
            window: PayoutWindow,
            target_per_window: BalanceOf<T>,
            adjust_period: BlockNumberFor<T>,
            min_difficulty: u8,
            max_difficulty: u8,
            step: u8,
            deadband: BalanceOf<T>,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;

            // Basic sanity checks.
            ensure!(!adjust_period.is_zero(), Error::<T>::InvalidControllerConfig);
            ensure!(min_difficulty <= max_difficulty, Error::<T>::InvalidControllerConfig);
            ensure!(max_difficulty <= 100, Error::<T>::InvalidControllerConfig);

            // Validate that the selected window has a non-zero length.
            let win_len = Self::window_len_blocks(window);
            ensure!(!win_len.is_zero(), Error::<T>::InvalidControllerConfig);

            let cfg = AiDifficultyControllerConfig {
                tracked_currency,
                window,
                target_per_window,
                adjust_period,
                min_difficulty,
                max_difficulty,
                step,
                deadband,
            };
            AiDifficultyController::<T>::put(cfg.clone());

            // Reset the rolling window.
            let now = <frame_system::Pallet<T>>::block_number();
            AiPayoutWindowState::<T>::put(PayoutWindowState {
                window_start: now,
                last_adjust: now,
                paid_coin: Zero::zero(),
                paid_dev_coin: Zero::zero(),
                paid_beta_coin: Zero::zero(),
            });

            Self::deposit_event(Event::AiDifficultyControllerSet {
                tracked_currency,
                window,
                target_per_window,
                adjust_period,
                min_difficulty,
                max_difficulty,
                step,
                deadband,
            });

            Ok(())
        }

        /// Disable (remove) the automatic AI difficulty controller config.
        #[pallet::call_index(9)]
        #[pallet::weight(<T as Config>::WeightInfo::disable_ai_difficulty_controller())]
        pub fn disable_ai_difficulty_controller(origin: OriginFor<T>) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            AiDifficultyController::<T>::kill();
            AiPayoutWindowState::<T>::put(PayoutWindowState {
                window_start: Zero::zero(),
                last_adjust: Zero::zero(),
                paid_coin: Zero::zero(),
                paid_dev_coin: Zero::zero(),
                paid_beta_coin: Zero::zero(),
            });
            Self::deposit_event(Event::AiDifficultyControllerDisabled);
            Ok(())
        }
    }
}

// Helper methods
impl<T: Config> Pallet<T> {
    /// Create a PvP game between two accounts without a signed origin.
    /// Intended to be called from the matchmaking pallet via the `GameCreator` trait.
    fn do_create_pvp_game(
        a: &AccountIdOf<T>,
        b: &AccountIdOf<T>,
    ) -> Result<GameId<T>, sp_runtime::DispatchError> {
        // Sanity checks
        ensure!(a != b, Error::<T>::InvalidMove);
        ensure!(
            T::NumPlayers::get() == 2,
            Error::<T>::InvalidNumberOfPlayers
        );

        // Both players must have a preset/current hand (defense in depth; the matchmaker checks this too)
        ensure!(
            CurrentHandOf::<T>::contains_key(a),
            Error::<T>::PresetHandMissing
        );
        ensure!(
            CurrentHandOf::<T>::contains_key(b),
            Error::<T>::PresetHandMissing
        );

        // Neither is currently in another game
        ensure!(
            ActiveGameOf::<T>::get(a).is_none(),
            Error::<T>::PlayerAlreadyInGame
        );
        ensure!(
            ActiveGameOf::<T>::get(b).is_none(),
            Error::<T>::PlayerAlreadyInGame
        );

        // Create a deterministic game id from (a,b,block)
        let current_block_number = <frame_system::Pallet<T>>::block_number();
        let game_id = T::Hashing::hash_of(&(a.clone(), b.clone(), current_block_number));

        // Collision check (extremely unlikely)
        ensure!(
            !GameStorage::<T>::contains_key(&game_id),
            Error::<T>::GameNotFound
        );

        // Build initial game struct
        let initial_board: Board = Default::default();
        let initial_scores = (5, 5);
        let players_vec = sp_std::vec![a.clone(), b.clone()];

        let mut game: Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers> = Game {
            state: GameState::Playing,
            last_played_block: current_block_number,
            players: players_vec
                .clone()
                .try_into()
                .map_err(|_| Error::<T>::InternalError)?,
            player_turn: 0,
            round: 0,
            max_rounds: T::MaxRounds::get(),
            board: initial_board.clone(),
            scores: initial_scores,
        };

        // Mark this as a PvP game and set active game markers
        GameModes::<T>::insert(&game_id, GameMode::PvP);
        ActiveGameOf::<T>::insert(a, game_id);
        ActiveGameOf::<T>::insert(b, game_id);

        // Push into recent lists for each player (most-recent first, bounded to 10)
        let push_recent = |acct: &AccountIdOf<T>| {
            PlayerGames::<T>::mutate(acct, |list| {
                if let Some(pos) = list.iter().position(|g| *g == game_id) {
                    list.remove(pos);
                }
                if list.len() as u32 >= <ConstU32<10> as sp_runtime::traits::Get<u32>>::get() {
                    let _ = list.pop();
                }
                let mut tmp = list.to_vec();
                tmp.insert(0, game_id);
                *list = BoundedVec::try_from(tmp).expect("<= 10; qed");
            });
        };
        push_recent(a);
        push_recent(b);

        // Randomize starting player using `a` as seed (keep behavior similar to create_game PvP)
        game.set_player_turn(if sp_io::hashing::blake2_128(&a.encode())[0] % 2 == 0 {
            0
        } else {
            1
        });

        GameStorage::<T>::insert(&game_id, game.clone());
        Self::deposit_event(Event::GameCreated { game_id });

        Ok(game_id)
    }
    fn map_card_to_ai(c: &Card) -> ai::Card {
        ai::Card {
            top: c.top,
            right: c.right,
            bottom: c.bottom,
            left: c.left,
            possession: c.possession.as_ref().map(|p| match p {
                Player::PlayerOne => ai::Possession::PlayerOne,
                Player::PlayerTwo => ai::Possession::PlayerTwo,
            }),
        }
    }

    fn window_len_blocks(window: PayoutWindow) -> BlockNumberFor<T> {
        match window {
            PayoutWindow::Hour => T::BlocksPerHour::get(),
            PayoutWindow::Day => T::BlocksPerDay::get(),
            PayoutWindow::Week => T::BlocksPerWeek::get(),
            PayoutWindow::Month => T::BlocksPerMonth::get(),
        }
    }

    fn paid_for_currency(
        s: &PayoutWindowState<BlockNumberFor<T>, BalanceOf<T>>,
        c: TrackedPayoutCurrency,
    ) -> BalanceOf<T> {
        match c {
            TrackedPayoutCurrency::Coin => s.paid_coin,
            TrackedPayoutCurrency::DevCoin => s.paid_dev_coin,
            TrackedPayoutCurrency::BetaCoin => s.paid_beta_coin,
        }
    }

    /// Add payouts from a PvE win to the rolling controller window (if enabled).
    fn note_pve_payout(
        game_id: &GameId<T>,
        paid_coin: BalanceOf<T>,
        paid_dev: BalanceOf<T>,
        paid_beta: BalanceOf<T>,
    ) {
        // Only track PvE wins vs the AI.
        if !matches!(GameModes::<T>::get(game_id), Some(GameMode::PvE)) {
            return;
        }

        // Only track if the controller is enabled.
        if AiDifficultyController::<T>::get().is_none() {
            return;
        }

        AiPayoutWindowState::<T>::mutate(|s| {
            s.paid_coin = s.paid_coin.saturating_add(paid_coin);
            s.paid_dev_coin = s.paid_dev_coin.saturating_add(paid_dev);
            s.paid_beta_coin = s.paid_beta_coin.saturating_add(paid_beta);
        });
    }

    /// Adjust the AI difficulty (if controller is configured) to target payouts in the current window.
    fn maybe_adjust_ai_difficulty(now: BlockNumberFor<T>) {
        let cfg = match AiDifficultyController::<T>::get() {
            Some(c) => c,
            None => return,
        };
        if cfg.step == 0 {
            return;
        }

        let window_len = Self::window_len_blocks(cfg.window);
        if sp_runtime::traits::Zero::is_zero(&window_len) {
            return;
        }

        AiPayoutWindowState::<T>::mutate(|s| {
            // Initialize window on first use.
            if sp_runtime::traits::Zero::is_zero(&s.window_start) {
                s.window_start = now;
                s.last_adjust = now;
                return;
            }

            // If the window elapsed, settle against the full target and reset.
            let elapsed = now.saturating_sub(s.window_start);
            if elapsed >= window_len {
                let paid = Self::paid_for_currency(s, cfg.tracked_currency);
                let expected = cfg.target_per_window;
                Self::adjust_difficulty(&cfg, paid, expected);

                // Start a fresh window.
                s.window_start = now;
                s.last_adjust = now;
                s.paid_coin = sp_runtime::traits::Zero::zero();
                s.paid_dev_coin = sp_runtime::traits::Zero::zero();
                s.paid_beta_coin = sp_runtime::traits::Zero::zero();
                return;
            }

            // Throttle adjustments.
            if now.saturating_sub(s.last_adjust) < cfg.adjust_period {
                return;
            }

            let paid = Self::paid_for_currency(s, cfg.tracked_currency);

            // expected = target * elapsed / window_len
            let elapsed_u128: u128 =
                sp_runtime::traits::SaturatedConversion::saturated_into(elapsed);
            let window_u128: u128 =
                sp_runtime::traits::SaturatedConversion::saturated_into(window_len);
            let target_u128: u128 =
                sp_runtime::traits::SaturatedConversion::saturated_into(cfg.target_per_window);
            let expected_u128: u128 = if window_u128 == 0 {
                target_u128
            } else {
                target_u128
                    .saturating_mul(elapsed_u128)
                    .saturating_div(window_u128)
            };
            let expected: BalanceOf<T> =
                sp_runtime::traits::SaturatedConversion::saturated_into(expected_u128);

            Self::adjust_difficulty(&cfg, paid, expected);
            s.last_adjust = now;
        });
    }

    fn adjust_difficulty(
        cfg: &AiDifficultyControllerConfig<BlockNumberFor<T>, BalanceOf<T>>,
        paid: BalanceOf<T>,
        expected: BalanceOf<T>,
    ) {
        // Clamp the current setting into the configured range first.
        let old_raw = CurrentAiDifficulty::<T>::get();
        let old = old_raw.clamp(cfg.min_difficulty, cfg.max_difficulty);
        if old != old_raw {
            CurrentAiDifficulty::<T>::put(old);
        }

        let up_threshold = expected.saturating_add(cfg.deadband);
        let down_threshold = expected.saturating_sub(cfg.deadband);

        let mut new = old;
        if paid > up_threshold {
            new = old.saturating_add(cfg.step).min(cfg.max_difficulty);
        } else if paid < down_threshold {
            new = old.saturating_sub(cfg.step).max(cfg.min_difficulty);
        }

        if new != old {
            CurrentAiDifficulty::<T>::put(new);
            Self::deposit_event(Event::AiDifficultyAdjusted {
                old,
                new,
                tracked_currency: cfg.tracked_currency,
                paid,
                expected,
            });
        }
    }
    /// If the next player is the AI in a PvE game, let the AI take its move immediately.
    fn maybe_ai_take_turn(
        game_id: &GameId<T>,
        game: &mut Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers>,
    ) {
        // Only PvE
        if !matches!(GameModes::<T>::get(game_id), Some(GameMode::PvE)) {
            return;
        }
        let ai_acc = T::AiAccount::get();
        let turn_acc = game.players[game.get_player_turn() as usize].clone();
        if turn_acc != ai_acc {
            return;
        }

        // Build AI adapter state from on-chain state
        let state = match Self::build_ai_state(game_id, game) {
            Some(s) => s,
            None => return,
        };
        let diff = CurrentAiDifficulty::<T>::get();

        if let Some(action) = mc_ai::pallet::Pallet::<T>::suggest::<ai::Adapter>(&state, diff) {
            let x = action.x;
            let y = action.y;
            let idx = action.hand_index as usize;

            // Play as AI (mirror play_from_hand)
            if let Some(mut ai_hand) = HandsOfGame::<T>::get(game_id, &ai_acc) {
                // Safely access the chosen hand entry without using idx < len comparisons
                if let Some(slot) = ai_hand.get_mut(idx) {
                    let xi: usize = x as usize;
                    let yi: usize = y as usize;
                    if !slot.used {
                        if let Some(col) = game.board.get(xi) {
                            if let Some(cell) = col.get(yi) {
                                if cell.is_none() {
                                    let h = slot.clone();
                                    let placed = Card {
                                        top: h.north,
                                        right: h.east,
                                        bottom: h.south,
                                        left: h.west,
                                        possession: None,
                                    };
                                    let mv = Move {
                                        place_card: placed,
                                        place_index_x: x,
                                        place_index_y: y,
                                    };

                                    let player_ix = Self::get_current_player_index(game, &ai_acc);
                                    Self::place_card_on_board(game, &mv, player_ix);
                                    Self::apply_capture_logic(game, &mv, player_ix);

                                    slot.used = true;
                                    HandsOfGame::<T>::insert(game_id, &ai_acc, ai_hand);

                                    let current_block = <frame_system::Pallet<T>>::block_number();
                                    game.last_played_block = current_block;
                                    game.next_turn();

                                    let next_player =
                                        game.players[game.get_player_turn() as usize].clone();
                                    Self::deposit_event(Event::NewTurn {
                                        game_id: *game_id,
                                        next_player,
                                    });
                                    GameStorage::<T>::insert(game_id, game.clone());

                                    if let Some(winner) = Self::is_game_won(game_id, game) {
                                        Self::end_game(game_id, winner);
                                        return;
                                    }

                                    Self::deposit_event(Event::MovePlayed {
                                        game_id: *game_id,
                                        player: ai_acc,
                                        x,
                                        y,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn build_ai_state(
        game_id: &GameId<T>,
        game: &Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers>,
    ) -> Option<ai::State> {
        let p0 = game.players.get(0)?.clone();
        let p1 = game.players.get(1)?.clone();
        let hand0 = HandsOfGame::<T>::get(game_id, &p0)?;
        let hand1 = HandsOfGame::<T>::get(game_id, &p1)?;

        let map_hand = |h: &BoundedVec<HandEntry, HandLimit>| -> ai::Hand {
            let mut arr: [ai::HandEntry; 5] = core::array::from_fn(|_| ai::HandEntry {
                north: 1,
                east: 1,
                south: 1,
                west: 1,
                used: true,
            });
            for (i, he) in h.iter().enumerate().take(5) {
                arr[i] = ai::HandEntry {
                    north: he.north,
                    east: he.east,
                    south: he.south,
                    west: he.west,
                    used: he.used,
                };
            }
            ai::Hand { entries: arr }
        };

        let hands = [map_hand(&hand0), map_hand(&hand1)];

        // Map on-chain board (card::Card) to adapter board (ai::Card)
        let mut board_ai: [[Option<ai::Card>; 4]; 4] =
            core::array::from_fn(|_| core::array::from_fn(|_| None));
        for x in 0..4 {
            for y in 0..4 {
                if let Some(ref c) = game.board[x][y] {
                    board_ai[x][y] = Some(Self::map_card_to_ai(c));
                }
            }
        }

        Some(ai::State {
            board: board_ai,
            scores: game.scores,
            player_turn: game.player_turn,
            round: game.round,
            max_rounds: game.max_rounds,
            hands,
        })
    }

    /// Build an AI hand whose average ranks are slightly below the human's submitted hand.
    fn generate_ai_hand_for_game(
        game_id: &GameId<T>,
        human: &T::AccountId,
    ) -> Option<BoundedVec<HandEntry, HandLimit>> {
        let human_hand = HandsOfGame::<T>::get(game_id, human)?;
        let mut sum: u32 = 0;
        for h in human_hand.iter() {
            sum += h.north as u32 + h.east as u32 + h.south as u32 + h.west as u32;
        }
        let avg = (sum as f32) / ((human_hand.len() as f32) * 4.0);
        let target = (avg - 0.5).max(1.0); // slightly easier than human

        // Deterministic pseudo-randomization from (game_id, human)
        let seed_hash = <T as frame_system::Config>::Hashing::hash_of(&(game_id, human));
        let bytes = seed_hash.as_ref();

        let mk_val = |i: usize| -> u8 {
            // clamp to [1.0, 9.0] then round without relying on FloatCore/round
            let jitter = ((bytes.get(i % bytes.len()).copied().unwrap_or(0) as i8 % 3) - 1) as f32;
            let mut base = target + jitter;
            if base < 1.0 {
                base = 1.0;
            }
            if base > 9.0 {
                base = 9.0;
            }
            let b_int = base as i32;
            let frac = base - (b_int as f32);
            let mut clamped = if frac >= 0.5 {
                (b_int + 1) as u8
            } else {
                b_int as u8
            };
            if clamped < 1 {
                clamped = 1;
            }
            if clamped > 9 {
                clamped = 9;
            }
            clamped
        };

        let mut out: BoundedVec<HandEntry, HandLimit> = BoundedVec::default();
        for i in 0..HandLimit::get() {
            let e = HandEntry {
                card_id: 0,
                north: mk_val(i as usize),
                east: mk_val(i as usize + 1),
                south: mk_val(i as usize + 2),
                west: mk_val(i as usize + 3),
                used: false,
            };
            let _ = out.try_push(e);
        }
        Some(out)
    }

    fn is_game_won(
        game_id: &GameId<T>,
        game: &Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers>,
    ) -> Option<Option<T::AccountId>> {
        log::debug!(
            "Checking if game is won. Current round: {}, Max rounds: {}",
            game.round,
            game.max_rounds
        );

        // Ensure game is still in storage before checking win conditions
        if !GameStorage::<T>::contains_key(game_id) {
            log::warn!(
                "Warning: Attempted to check is_game_won() on a removed game: {:?}",
                game_id
            );
            return None;
        }

        if game.round >= game.max_rounds {
            log::debug!("Max rounds reached. Determining winner...");
        } else {
            log::debug!("Game continues. Not at max rounds yet.");
            return None;
        }

        // Determine winner
        let (score_player_0, score_player_1) = game.scores;
        let winner = if score_player_0 > score_player_1 {
            Some(game.players[0].clone())
        } else if score_player_1 > score_player_0 {
            Some(game.players[1].clone())
        } else {
            None // Draw
        };

        log::debug!(
            "Game ID: {:?}, Scores: {:?}, Winner: {:?}",
            game_id,
            game.scores,
            winner
        );

        Some(winner)
    }
    fn validate_player_turn(
        game: &Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers>,
        who: &AccountIdOf<T>,
    ) -> Result<(), Error<T>> {
        let current_turn_index = game.get_player_turn();
        let current_turn = game.players[current_turn_index as usize].clone();
        ensure!(current_turn == *who, Error::<T>::NotYourTurn);
        Ok(())
    }

    #[cfg(any(test, feature = "dev-play"))]
    fn validate_move(
        game: &Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers>,
        player_move: &Move,
    ) -> Result<(), Error<T>> {
        ensure!(
            player_move.place_index_x < 4 && player_move.place_index_y < 4,
            Error::<T>::InvalidMove
        );
        ensure!(
            game.board[player_move.place_index_x as usize][player_move.place_index_y as usize]
                .is_none(),
            Error::<T>::CellOccupied
        );
        Ok(())
    }

    fn get_current_player_index(
        game: &Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers>,
        who: &AccountIdOf<T>,
    ) -> u8 {
        if who == &game.players[0] {
            0
        } else {
            1
        }
    }

    fn place_card_on_board(
        game: &mut Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers>,
        player_move: &Move,
        player_ix: u8,
    ) {
        let placed_card = player_move
            .place_card
            .clone()
            .with_possession(match player_ix {
                0 => Player::PlayerOne,
                _ => Player::PlayerTwo,
            });
        game.board[player_move.place_index_x as usize][player_move.place_index_y as usize] =
            Some(placed_card);
    }

    fn apply_capture_logic(
        game: &mut Game<AccountIdOf<T>, BlockNumberFor<T>, T::NumPlayers>,
        player_move: &Move,
        player_ix: u8,
    ) {
        // For each of the 4 orthogonal directions, compare the placed card's edge
        // against the opposite edge of the neighboring card. Capture only if:
        //  - There is a card
        //  - It is owned by the opponent
        //  - Our edge strictly beats their opposing edge (ties do NOT capture)
        for &(dx, dy, my_rank) in &[
            (0, -1, player_move.place_card.top), // Top: compare vs neighbor's bottom
            (1, 0, player_move.place_card.right), // Right: compare vs neighbor's left
            (0, 1, player_move.place_card.bottom), // Bottom: compare vs neighbor's top
            (-1, 0, player_move.place_card.left), // Left: compare vs neighbor's right
        ] {
            let nx = player_move.place_index_x as isize + dx;
            let ny = player_move.place_index_y as isize + dy;
            if nx < 0 || nx >= 4 || ny < 0 || ny >= 4 {
                continue;
            }

            let xi = nx as usize;
            let yi = ny as usize;

            if let Some(mut neighbor) = game.board[xi][yi].clone() {
                // Only attempt to capture if the neighbor is owned by the opponent
                let is_opponent_owned = match (neighbor.possession.as_ref(), player_ix) {
                    (Some(Player::PlayerOne), 1) => true,
                    (Some(Player::PlayerTwo), 0) => true,
                    _ => false,
                };
                if !is_opponent_owned {
                    continue;
                }

                // Determine neighbor's opposing edge rank based on direction
                let opp_rank = match (dx, dy) {
                    (0, -1) => neighbor.bottom,
                    (1, 0) => neighbor.left,
                    (0, 1) => neighbor.top,
                    (-1, 0) => neighbor.right,
                    _ => 0,
                };

                log::debug!(
                    "[CaptureCheck] at ({},{}) vs neighbor ({},{}): my_edge={}, opp_edge={}",
                    player_move.place_index_x,
                    player_move.place_index_y,
                    xi,
                    yi,
                    my_rank,
                    opp_rank
                );

                // Strictly greater captures; ties do not capture
                if my_rank > opp_rank {
                    // Adjust scores: remove point from previous owner (opponent), give to current
                    match neighbor.possession.as_ref() {
                        Some(Player::PlayerOne) => {
                            game.scores.0 = game.scores.0.saturating_sub(1);
                        }
                        Some(Player::PlayerTwo) => {
                            game.scores.1 = game.scores.1.saturating_sub(1);
                        }
                        _ => {}
                    }

                    match player_ix {
                        0 => {
                            game.scores.0 = game.scores.0.saturating_add(1);
                            neighbor.possession = Some(Player::PlayerOne);
                        }
                        _ => {
                            game.scores.1 = game.scores.1.saturating_add(1);
                            neighbor.possession = Some(Player::PlayerTwo);
                        }
                    }

                    log::debug!(
                        "[Captured] neighbor ({},{}) now owned by player {}",
                        xi,
                        yi,
                        player_ix
                    );

                    // Persist flipped neighbor back to the board
                    game.board[xi][yi] = Some(neighbor);
                }
            }
        }
    }

    /// Create a default AI hand at game creation time so UI can display it even before human submits.
    /// This hand uses deterministic pseudo-random stats (1..=9) derived from the game_id seed.
    fn generate_ai_hand_default(game_id: &GameId<T>) -> Option<BoundedVec<HandEntry, HandLimit>> {
        // Derive bytes from the game_id itself for reproducible pseudo-randomness
        let h = <T as frame_system::Config>::Hashing::hash_of(game_id);
        let bytes = h.as_ref();
        if bytes.is_empty() {
            return None;
        }

        let mut at = 0usize;
        let mut next = || -> u8 {
            let b = bytes[at % bytes.len()];
            at = at.wrapping_add(1);
            // Map 0..=255 -> 1..=9
            (b % 9).saturating_add(1)
        };

        let mut out: BoundedVec<HandEntry, HandLimit> = BoundedVec::default();
        for _ in 0..HandLimit::get() {
            let e = HandEntry {
                card_id: 0,
                north: next(),
                east: next(),
                south: next(),
                west: next(),
                used: false,
            };
            let _ = out.try_push(e);
        }
        Some(out)
    }

    fn end_game(game_id: &GameId<T>, winner: Option<T::AccountId>) {
        // Read and update game in storage to persist final state
        if let Some(mut g) = GameStorage::<T>::get(game_id) {
            // Emit before we change pointers
            Self::deposit_event(Event::GameFinished {
                game_id: *game_id,
                winner: winner.clone(),
            });

            // If there is a winner, reward them (skip AI account).
            if let Some(ref winner_acc) = winner {
                if *winner_acc != T::AiAccount::get() {
                    let coin = T::WinRewardCoin::get();
                    let dev = T::WinRewardDevCoin::get();
                    let beta = T::WinRewardBetaCoin::get();
                    let exp = T::WinRewardExperience::get();

                    let mut paid_coin: BalanceOf<T> = sp_runtime::traits::Zero::zero();
                    let mut paid_dev: BalanceOf<T> = sp_runtime::traits::Zero::zero();
                    let mut paid_beta: BalanceOf<T> = sp_runtime::traits::Zero::zero();

                    if !sp_runtime::traits::Zero::is_zero(&coin) {
                        let _ = <<T as pallet_eterra_simple_tcg::pallet::Config>::Currency as frame_support::traits::Currency<AccountIdOf<T>>>::deposit_creating(winner_acc, coin);
                        paid_coin = coin;
                    }
                    if !sp_runtime::traits::Zero::is_zero(&dev) {
                        if let Err(e) = <T::Assets as frame_support::traits::fungibles::Mutate<AccountIdOf<T>>>::mint_into(T::DevCoinAssetId::get(), winner_acc, dev) {
                            log::warn!(
                                target: "runtime::eterra",
                                "failed to mint devCOIN reward for {:?}: {:?}",
                                winner_acc,
                                e
                            );
                        } else {
                            paid_dev = dev;
                        }
                    }
                    if !sp_runtime::traits::Zero::is_zero(&beta) {
                        if let Err(e) = <T::Assets as frame_support::traits::fungibles::Mutate<AccountIdOf<T>>>::mint_into(T::BetaCoinAssetId::get(), winner_acc, beta) {
                            log::warn!(
                                target: "runtime::eterra",
                                "failed to mint betaCOIN reward for {:?}: {:?}",
                                winner_acc,
                                e
                            );
                        } else {
                            paid_beta = beta;
                        }
                    }
                    if exp != 0 {
                        <T::ExperienceManager as pallet_eterra_gamer::ExperienceManager<_>>::grant_experience(winner_acc, exp);
                    }

                    Self::deposit_event(Event::WinRewardsIssued {
                        game_id: *game_id,
                        winner: winner_acc.clone(),
                        coin,
                        dev_coin: dev,
                        beta_coin: beta,
                        exp,
                    });

                    // Track PvE payouts for the AI controller (if enabled).
                    Self::note_pve_payout(game_id, paid_coin, paid_dev, paid_beta);
                }
            }

            // Clear active-game markers for human participants
            if let Some(a) = g.players.get(0).cloned() {
                ActiveGameOf::<T>::remove(&a);
            }
            if let Some(b) = g.players.get(1).cloned() {
                ActiveGameOf::<T>::remove(&b);
            }

            // Map AccountId winner to player index (0/1) to match GameState::Finished { winner: Option<u8> }
            let winner_ix: Option<u8> = match winner.as_ref() {
                Some(acc) if *acc == g.players[0] => Some(0),
                Some(acc) if *acc == g.players[1] => Some(1),
                _ => None,
            };
            g.state = GameState::Finished { winner: winner_ix };
            GameStorage::<T>::insert(game_id, g);
        } else {
            // If the game wasn't found (should not happen), still emit the event
            Self::deposit_event(Event::GameFinished {
                game_id: *game_id,
                winner,
            });
        }
    }
}

// Expose GameCreator for the matchmaker pallet
impl<T: Config> pallet_eterra_simple_matchmaker::GameCreator<AccountIdOf<T>> for Pallet<T> {
    fn create_from_matchmaking(
        a: &AccountIdOf<T>,
        b: &AccountIdOf<T>,
    ) -> sp_runtime::DispatchResult {
        // Create the game and ignore the returned GameId; the matchmaker
        // only needs to know whether the creation succeeded.
        let _ = Self::do_create_pvp_game(a, b)?;
        Ok(())
    }
}
