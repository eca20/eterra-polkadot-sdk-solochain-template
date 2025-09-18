#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

use frame_support::{
    dispatch::DispatchResult,
    pallet_prelude::*,
    traits::{
        tokens::ExistenceRequirement,
        BuildGenesisConfig,
        Currency,
    },
};
use frame_system::pallet_prelude::*;

/// Helper to get the balance type from the configured Currency
pub type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The aggregated runtime event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The currency used for faucet payouts.
        type Currency: Currency<Self::AccountId>;
    }

    #[pallet::pallet]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);

    /// Faucet account id, set via genesis
    #[pallet::storage]
    #[pallet::getter(fn faucet_account)]
    pub type FaucetAccount<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

    /// Fixed payout amount per claim, set via genesis
    #[pallet::storage]
    #[pallet::getter(fn payout_amount)]
    pub type PayoutAmount<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

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
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Claim faucet funds. Transfers `PayoutAmount` from `FaucetAccount` to the signer.
        ///
        /// * Development note:* Configure `FaucetAccount` to be Alice in your runtime, and
        /// prefund it in genesis.
        #[pallet::call_index(0)]
        #[pallet::weight((0, frame_support::dispatch::DispatchClass::Normal, frame_support::dispatch::Pays::No))]
        pub fn claim(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let faucet = FaucetAccount::<T>::get().ok_or(Error::<T>::NotConfigured)?;
            let amount: BalanceOf<T> = PayoutAmount::<T>::get();

            // Ensure faucet has enough balance
            let free = T::Currency::free_balance(&faucet);
            ensure!(free >= amount, Error::<T>::InsufficientFaucetBalance);

            T::Currency::transfer(&faucet, &who, amount, ExistenceRequirement::AllowDeath)
                .map_err(|_| Error::<T>::TransferFailed)?;

            Self::deposit_event(Event::Claimed { who, amount });
            Ok(())
        }
    }
}
