#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

pub mod weights;
pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use frame_support::{
    dispatch::DispatchResult,
    pallet_prelude::*,
    traits::{tokens::ExistenceRequirement, BuildGenesisConfig, Currency},
};
use frame_system::pallet_prelude::*;

/// Helper to get the balance type from the configured Currency
pub type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;
    use frame_support::traits::StorageVersion;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The aggregated runtime event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The currency used for faucet payouts.
        type Currency: Currency<Self::AccountId>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;
    }

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

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
        /// Destination already claimed this block (rate limit).
        TooFrequent,
        /// Destination must be the caller.
        InvalidDestination,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Claim faucet funds. Transfers `PayoutAmount` from `FaucetAccount` to `dest`.
        ///
        /// This is a **signed** extrinsic. Rate-limited to once per block per `dest`.
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::claim())]
        pub fn claim(origin: OriginFor<T>, dest: T::AccountId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(who == dest, Error::<T>::InvalidDestination);

            // Basic rate limit: once per block per destination
            let now = frame_system::Pallet::<T>::block_number();
            if let Some(last) = LastClaim::<T>::get(&dest) {
                // If already claimed this exact block, reject
                if last == now {
                    return Err(Error::<T>::TooFrequent.into());
                }
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

            Self::deposit_event(Event::Claimed { who: dest, amount });
            Ok(())
        }
    }
}
