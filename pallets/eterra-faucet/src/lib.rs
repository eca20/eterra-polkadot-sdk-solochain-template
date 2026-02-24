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

use frame_support::traits::Currency;

/// Helper to get the balance type from the configured Currency
pub type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{
        dispatch::DispatchResult,
        pallet_prelude::*,
        sp_runtime::traits::{Saturating, Zero},
        traits::{tokens::ExistenceRequirement, BuildGenesisConfig, StorageVersion},
    };
    use frame_system::pallet_prelude::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The aggregated runtime event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The currency used for faucet payouts.
        type Currency: Currency<Self::AccountId>;

        /// Minimum number of blocks between claims by the same account.
        /// Set to 0 to disable cooldown.
        #[pallet::constant]
        type ClaimCooldownBlocks: Get<BlockNumberFor<Self>>;

        /// Max number of fee-sponsored claims in one sponsorship window.
        /// Set to 0 to disable sponsorship entirely.
        #[pallet::constant]
        type SponsoredClaimMaxCount: Get<u32>;

        /// Sponsorship window size in blocks.
        #[pallet::constant]
        type SponsoredClaimWindowBlocks: Get<BlockNumberFor<Self>>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;
    }

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

    #[pallet::pallet]
    #[pallet::without_storage_info]
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
    }

    /// Faucet account id, set via genesis
    #[pallet::storage]
    #[pallet::getter(fn faucet_account)]
    pub type FaucetAccount<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

    /// Fixed payout amount per claim, set via genesis
    #[pallet::storage]
    #[pallet::getter(fn payout_amount)]
    pub type PayoutAmount<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    /// Last block at which a given account claimed from the faucet.
    #[pallet::storage]
    #[pallet::getter(fn last_claim)]
    pub type LastClaim<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BlockNumberFor<T>, OptionQuery>;

    /// Start block of the current sponsorship window for an account.
    #[pallet::storage]
    #[pallet::getter(fn sponsored_window_start)]
    pub type SponsoredWindowStart<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BlockNumberFor<T>, OptionQuery>;

    /// Number of sponsored claims used by the account in the current window.
    #[pallet::storage]
    #[pallet::getter(fn sponsored_claims_used)]
    pub type SponsoredClaimsUsed<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

    #[pallet::genesis_config]
    #[derive(frame_support::DefaultNoBound)]
    pub struct GenesisConfig<T: Config> {
        pub faucet_account: Option<T::AccountId>,
        pub payout_amount: BalanceOf<T>,
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            if let Some(ref acc) = self.faucet_account {
                FaucetAccount::<T>::put(acc);
            }
            PayoutAmount::<T>::put(&self.payout_amount);
        }
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// A faucet claim was paid.
        /// (who, amount)
        Claimed {
            who: T::AccountId,
            amount: BalanceOf<T>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// The faucet account does not have enough free balance to pay the claim.
        InsufficientFaucetBalance,
        /// Transfer failed for another reason.
        TransferFailed,
        /// Faucet was not configured in genesis.
        NotConfigured,
        /// Destination claimed too recently (cooldown not elapsed).
        TooFrequent,
        /// Destination must be the caller.
        InvalidDestination,
    }

    impl<T: Config> Pallet<T> {
        /// Returns true if `who` can receive sponsorship at block `now`.
        pub fn can_receive_sponsored_claim(who: &T::AccountId, now: BlockNumberFor<T>) -> bool {
            let max = T::SponsoredClaimMaxCount::get();
            if max == 0 {
                return false;
            }

            let window = T::SponsoredClaimWindowBlocks::get();
            if window.is_zero() {
                return false;
            }
            match SponsoredWindowStart::<T>::get(who) {
                None => true,
                Some(start) => {
                    let in_window = now < start.saturating_add(window);
                    if !in_window {
                        return true;
                    }
                    SponsoredClaimsUsed::<T>::get(who) < max
                }
            }
        }

        /// Pre-dispatch sponsorship check used by transaction-fee charging.
        /// This intentionally includes cooldown and faucet liquidity checks so
        /// failing claims are not repeatedly fee-sponsored.
        pub fn can_receive_sponsored_claim_pre_dispatch(
            who: &T::AccountId,
            now: BlockNumberFor<T>,
        ) -> bool {
            if !Self::can_receive_sponsored_claim(who, now) {
                return false;
            }

            if let Some(last) = LastClaim::<T>::get(who) {
                let next_allowed = last.saturating_add(T::ClaimCooldownBlocks::get());
                if now < next_allowed {
                    return false;
                }
            }

            let Some(faucet) = FaucetAccount::<T>::get() else {
                return false;
            };
            let amount: BalanceOf<T> = PayoutAmount::<T>::get();
            T::Currency::free_balance(&faucet) >= amount
        }

        /// Records one sponsored claim usage for `who` at block `now`.
        fn note_sponsored_claim(who: &T::AccountId, now: BlockNumberFor<T>) {
            let window = T::SponsoredClaimWindowBlocks::get();
            let max = T::SponsoredClaimMaxCount::get();
            if max == 0 || window.is_zero() {
                return;
            }

            match SponsoredWindowStart::<T>::get(who) {
                Some(start) if now < start.saturating_add(window) => {
                    SponsoredClaimsUsed::<T>::mutate(who, |count| {
                        *count = count.saturating_add(1);
                    });
                }
                _ => {
                    SponsoredWindowStart::<T>::insert(who, now);
                    SponsoredClaimsUsed::<T>::insert(who, 1);
                }
            }
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Claim faucet funds. Transfers `PayoutAmount` from `FaucetAccount` to `dest`.
        ///
        /// This is a **signed** extrinsic. Rate-limited by `ClaimCooldownBlocks` per `dest`.
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::claim())]
        pub fn claim(origin: OriginFor<T>, dest: T::AccountId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(who == dest, Error::<T>::InvalidDestination);
            let was_zero_balance = T::Currency::free_balance(&who).is_zero();

            // Basic rate limit: one claim per configured cooldown interval per destination.
            let now = frame_system::Pallet::<T>::block_number();
            if let Some(last) = LastClaim::<T>::get(&dest) {
                let next_allowed = last.saturating_add(T::ClaimCooldownBlocks::get());
                ensure!(now >= next_allowed, Error::<T>::TooFrequent);
            }

            let faucet = FaucetAccount::<T>::get().ok_or(Error::<T>::NotConfigured)?;
            let amount: BalanceOf<T> = PayoutAmount::<T>::get();

            // Ensure faucet has enough balance
            let free = T::Currency::free_balance(&faucet);
            ensure!(free >= amount, Error::<T>::InsufficientFaucetBalance);

            // Transfer, allowing account creation for `dest`
            T::Currency::transfer(&faucet, &dest, amount, ExistenceRequirement::AllowDeath)
                .map_err(|_| Error::<T>::TransferFailed)?;

            // Record the claim block
            LastClaim::<T>::insert(&dest, now);

            // Track sponsored usage on successful claims from zero-balance accounts.
            if was_zero_balance && Self::can_receive_sponsored_claim(&dest, now) {
                Self::note_sponsored_claim(&dest, now);
            }

            Self::deposit_event(Event::Claimed { who: dest, amount });
            Ok(())
        }
    }
}
