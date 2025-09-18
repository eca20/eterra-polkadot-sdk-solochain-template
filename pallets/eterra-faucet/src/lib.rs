

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

use frame_support::{
    dispatch::DispatchResult,
    pallet_prelude::*,
    traits::tokens::ExistenceRequirement,
    traits::Currency,
};
use frame_system::pallet_prelude::*;

/// Helper to get the balance type from the configured Currency
pub type BalanceOf<T> = <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The aggregated runtime event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The currency used for faucet payouts.
        type Currency: Currency<Self::AccountId>;

        /// The account that funds faucet payouts (should be set to Alice in dev).
        type FaucetAccount: frame_support::traits::Get<Self::AccountId>;

        /// The fixed payout amount per claim.
        type PayoutAmount: frame_support::traits::Get<BalanceOf<Self>>;
    }

    #[pallet::pallet]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// A faucet claim was paid.
        /// (who, amount)
        Claimed { who: T::AccountId, amount: BalanceOf<T> },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// The faucet account does not have enough free balance to pay the claim.
        InsufficientFaucetBalance,
        /// Transfer failed for another reason.
        TransferFailed,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Claim faucet funds. Transfers `PayoutAmount` from `FaucetAccount` to the signer.
        ///
        /// * Development note:* Configure `FaucetAccount` to be Alice in your runtime, and
        /// prefund it in genesis.
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)]
        pub fn claim(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let faucet = T::FaucetAccount::get();
            let amount: BalanceOf<T> = T::PayoutAmount::get();

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