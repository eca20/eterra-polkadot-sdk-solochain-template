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
use sp_runtime::codec::Encode;
use sp_runtime::transaction_validity::{InvalidTransaction, TransactionSource, TransactionValidity, ValidTransaction};

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
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Claim faucet funds. Transfers `PayoutAmount` from `FaucetAccount` to `dest`.
        ///
        /// This is an **unsigned** extrinsic, validated via `ValidateUnsigned` so brand-new
        /// accounts (with no balance/nonce) can claim. Rate-limited to once per block per `dest`.
        #[pallet::call_index(0)]
        #[pallet::weight((0, frame_support::dispatch::DispatchClass::Normal, frame_support::dispatch::Pays::No))]
        pub fn claim(origin: OriginFor<T>, dest: T::AccountId) -> DispatchResult {
            // Unsigned call; no nonce/fee required
            ensure_none(origin)?;

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

    #[pallet::validate_unsigned]
    impl<T: Config> sp_runtime::traits::ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;

        fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
            match call {
                // Whitelist our unsigned faucet claim. Use provides=(dest, block) so duplicates
                // in the same block are rejected by the pool. Dispatch also enforces it on-chain.
                Call::claim { dest } => {
                    let now = frame_system::Pallet::<T>::block_number();
                    ValidTransaction::with_tag_prefix("EterraFaucet")
                        .priority(0)
                        .longevity(1)
                        .propagate(true)
                        .and_provides((dest, now).encode())
                        .build()
                }
                _ => InvalidTransaction::Call.into(),
            }
        }
    }
}
