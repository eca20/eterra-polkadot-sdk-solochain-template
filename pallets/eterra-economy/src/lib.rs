//! Eterra Economy pallet MVP scaffold.
//!
//! Purpose: products, entitlements, credits, developer sponsor pools, and
//! revenue accounting. Production integration should wire real token movement
//! through the runtime's chosen fungible asset/currency traits.
#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::too_many_arguments)]

pub use pallet::*;
pub mod weights;
pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame_support::pallet]
pub mod pallet {
    use super::weights::WeightInfo;
    use frame_support::{
        dispatch::DispatchResult, pallet_prelude::*, traits::StorageVersion, transactional,
    };
    use frame_system::pallet_prelude::*;

    pub type GameId = u64;
    pub type ProductId = u64;
    pub type EntitlementId = u32;
    pub type CreditTypeId = u32;
    pub type Balance = u128;

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
    pub enum ProductType {
        SeasonPass,
        BattlePass,
        ArcadeCreditPack,
        DungeonTicket,
        PremiumQuest,
        TournamentEntry,
        Subscription,
        Cosmetic,
        Bundle,
        CrossGamePass,
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
    pub enum ProductStatus {
        Draft,
        Active,
        Paused,
        Retired,
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
    pub struct ProductRecord<Hash> {
        pub product_type: ProductType,
        pub status: ProductStatus,
        pub price: Balance,
        pub grants_entitlement: Option<EntitlementId>,
        pub grants_credit: Option<(CreditTypeId, u64)>,
        pub metadata_hash: Hash,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type WeightInfo: WeightInfo;
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;
    }

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub type Products<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        GameId,
        Blake2_128Concat,
        ProductId,
        ProductRecord<T::Hash>,
    >;

    #[pallet::storage]
    pub type Entitlements<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, GameId>,
            NMapKey<Blake2_128Concat, T::AccountId>,
            NMapKey<Blake2_128Concat, EntitlementId>,
        ),
        bool,
        ValueQuery,
    >;

    #[pallet::storage]
    pub type Credits<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, GameId>,
            NMapKey<Blake2_128Concat, T::AccountId>,
            NMapKey<Blake2_128Concat, CreditTypeId>,
        ),
        u64,
        ValueQuery,
    >;

    #[pallet::storage]
    pub type SponsorPools<T: Config> = StorageMap<_, Blake2_128Concat, GameId, Balance, ValueQuery>;

    #[pallet::storage]
    pub type RevenueEscrow<T: Config> =
        StorageMap<_, Blake2_128Concat, GameId, Balance, ValueQuery>;

    #[pallet::storage]
    pub type FulfilledReceipts<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, GameId>,
            NMapKey<Blake2_128Concat, T::Hash>,
        ),
        bool,
        ValueQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        ProductCreated {
            game_id: GameId,
            product_id: ProductId,
        },
        ProductStatusChanged {
            game_id: GameId,
            product_id: ProductId,
            status: ProductStatus,
        },
        EntitlementGranted {
            game_id: GameId,
            account: T::AccountId,
            entitlement_id: EntitlementId,
        },
        EntitlementRevoked {
            game_id: GameId,
            account: T::AccountId,
            entitlement_id: EntitlementId,
        },
        CreditGranted {
            game_id: GameId,
            account: T::AccountId,
            credit_type: CreditTypeId,
            amount: u64,
        },
        CreditConsumed {
            game_id: GameId,
            account: T::AccountId,
            credit_type: CreditTypeId,
            amount: u64,
        },
        SponsorFundsDeposited {
            game_id: GameId,
            amount: Balance,
        },
        RevenueRecorded {
            game_id: GameId,
            amount: Balance,
        },
        ProductFulfilled {
            game_id: GameId,
            product_id: ProductId,
            account: T::AccountId,
            receipt_hash: T::Hash,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        ProductAlreadyExists,
        ProductNotFound,
        ProductNotActive,
        InsufficientCredit,
        InsufficientSponsorFunds,
        ReceiptAlreadyFulfilled,
        ArithmeticOverflow,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::create_product())]
        pub fn create_product(
            origin: OriginFor<T>,
            game_id: GameId,
            product_id: ProductId,
            product_type: ProductType,
            price: Balance,
            grants_entitlement: Option<EntitlementId>,
            grants_credit: Option<(CreditTypeId, u64)>,
            metadata_hash: T::Hash,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                !Products::<T>::contains_key(game_id, product_id),
                Error::<T>::ProductAlreadyExists
            );
            Products::<T>::insert(
                game_id,
                product_id,
                ProductRecord {
                    product_type,
                    status: ProductStatus::Draft,
                    price,
                    grants_entitlement,
                    grants_credit,
                    metadata_hash,
                },
            );
            Self::deposit_event(Event::ProductCreated {
                game_id,
                product_id,
            });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::set_product_status())]
        pub fn set_product_status(
            origin: OriginFor<T>,
            game_id: GameId,
            product_id: ProductId,
            status: ProductStatus,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Products::<T>::try_mutate(game_id, product_id, |maybe_product| -> DispatchResult {
                let product = maybe_product.as_mut().ok_or(Error::<T>::ProductNotFound)?;
                product.status = status.clone();
                Ok(())
            })?;
            Self::deposit_event(Event::ProductStatusChanged {
                game_id,
                product_id,
                status,
            });
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::grant_entitlement())]
        pub fn grant_entitlement(
            origin: OriginFor<T>,
            game_id: GameId,
            account: T::AccountId,
            entitlement_id: EntitlementId,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Self::try_grant_entitlement(&account, game_id, entitlement_id)
        }

        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::revoke_entitlement())]
        pub fn revoke_entitlement(
            origin: OriginFor<T>,
            game_id: GameId,
            account: T::AccountId,
            entitlement_id: EntitlementId,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Self::try_revoke_entitlement(&account, game_id, entitlement_id)
        }

        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::grant_credit())]
        pub fn grant_credit(
            origin: OriginFor<T>,
            game_id: GameId,
            account: T::AccountId,
            credit_type: CreditTypeId,
            amount: u64,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Self::try_grant_credit(&account, game_id, credit_type, amount)
        }

        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::consume_credit())]
        pub fn consume_credit(
            origin: OriginFor<T>,
            game_id: GameId,
            credit_type: CreditTypeId,
            amount: u64,
        ) -> DispatchResult {
            let account = ensure_signed(origin)?;
            Self::try_consume_credit(&account, game_id, credit_type, amount)
        }

        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::deposit_sponsor_funds())]
        pub fn deposit_sponsor_funds(
            origin: OriginFor<T>,
            game_id: GameId,
            amount: Balance,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Self::try_deposit_sponsor_funds(game_id, amount)
        }

        #[pallet::call_index(7)]
        #[pallet::weight(T::WeightInfo::record_revenue())]
        pub fn record_revenue(
            origin: OriginFor<T>,
            game_id: GameId,
            amount: Balance,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Self::try_record_revenue(game_id, amount)
        }

        #[pallet::call_index(8)]
        #[pallet::weight(T::WeightInfo::fulfill_product())]
        #[transactional]
        pub fn fulfill_product(
            origin: OriginFor<T>,
            game_id: GameId,
            product_id: ProductId,
            account: T::AccountId,
            receipt_hash: T::Hash,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Self::try_fulfill_product(&account, game_id, product_id, receipt_hash)
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn has_entitlement(
            account: &T::AccountId,
            game_id: GameId,
            entitlement_id: EntitlementId,
        ) -> bool {
            Entitlements::<T>::get((game_id, account, entitlement_id))
        }

        pub fn credit_balance(
            account: &T::AccountId,
            game_id: GameId,
            credit_type: CreditTypeId,
        ) -> u64 {
            Credits::<T>::get((game_id, account, credit_type))
        }

        pub fn spend_sponsor_funds(game_id: GameId, amount: Balance) -> DispatchResult {
            Self::try_spend_sponsor_funds(game_id, amount)
        }

        #[transactional]
        pub fn try_fulfill_product(
            account: &T::AccountId,
            game_id: GameId,
            product_id: ProductId,
            receipt_hash: T::Hash,
        ) -> DispatchResult {
            let product =
                Products::<T>::get(game_id, product_id).ok_or(Error::<T>::ProductNotFound)?;
            ensure!(
                product.status == ProductStatus::Active,
                Error::<T>::ProductNotActive
            );
            ensure!(
                !FulfilledReceipts::<T>::get((game_id, receipt_hash)),
                Error::<T>::ReceiptAlreadyFulfilled
            );

            if let Some(entitlement_id) = product.grants_entitlement {
                Self::try_grant_entitlement(account, game_id, entitlement_id)?;
            }
            if let Some((credit_type, amount)) = product.grants_credit {
                Self::try_grant_credit(account, game_id, credit_type, amount)?;
            }
            Self::try_record_revenue(game_id, product.price)?;
            FulfilledReceipts::<T>::insert((game_id, receipt_hash), true);
            Self::deposit_event(Event::ProductFulfilled {
                game_id,
                product_id,
                account: account.clone(),
                receipt_hash,
            });
            Ok(())
        }

        pub fn try_record_revenue(game_id: GameId, amount: Balance) -> DispatchResult {
            RevenueEscrow::<T>::try_mutate(game_id, |balance| -> DispatchResult {
                *balance = balance
                    .checked_add(amount)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                Ok(())
            })?;
            Self::deposit_event(Event::RevenueRecorded { game_id, amount });
            Ok(())
        }

        pub fn try_grant_entitlement(
            account: &T::AccountId,
            game_id: GameId,
            entitlement_id: EntitlementId,
        ) -> DispatchResult {
            Entitlements::<T>::insert((game_id, account.clone(), entitlement_id), true);
            Self::deposit_event(Event::EntitlementGranted {
                game_id,
                account: account.clone(),
                entitlement_id,
            });
            Ok(())
        }

        pub fn try_revoke_entitlement(
            account: &T::AccountId,
            game_id: GameId,
            entitlement_id: EntitlementId,
        ) -> DispatchResult {
            Entitlements::<T>::remove((game_id, account.clone(), entitlement_id));
            Self::deposit_event(Event::EntitlementRevoked {
                game_id,
                account: account.clone(),
                entitlement_id,
            });
            Ok(())
        }

        pub fn try_grant_credit(
            account: &T::AccountId,
            game_id: GameId,
            credit_type: CreditTypeId,
            amount: u64,
        ) -> DispatchResult {
            Credits::<T>::try_mutate(
                (game_id, account.clone(), credit_type),
                |balance| -> DispatchResult {
                    *balance = balance
                        .checked_add(amount)
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    Ok(())
                },
            )?;
            Self::deposit_event(Event::CreditGranted {
                game_id,
                account: account.clone(),
                credit_type,
                amount,
            });
            Ok(())
        }

        pub fn try_consume_credit(
            account: &T::AccountId,
            game_id: GameId,
            credit_type: CreditTypeId,
            amount: u64,
        ) -> DispatchResult {
            Credits::<T>::try_mutate(
                (game_id, account.clone(), credit_type),
                |balance| -> DispatchResult {
                    ensure!(*balance >= amount, Error::<T>::InsufficientCredit);
                    *balance = balance
                        .checked_sub(amount)
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    Ok(())
                },
            )?;
            Self::deposit_event(Event::CreditConsumed {
                game_id,
                account: account.clone(),
                credit_type,
                amount,
            });
            Ok(())
        }

        pub fn try_deposit_sponsor_funds(game_id: GameId, amount: Balance) -> DispatchResult {
            SponsorPools::<T>::try_mutate(game_id, |balance| -> DispatchResult {
                *balance = balance
                    .checked_add(amount)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                Ok(())
            })?;
            Self::deposit_event(Event::SponsorFundsDeposited { game_id, amount });
            Ok(())
        }

        pub fn try_spend_sponsor_funds(game_id: GameId, amount: Balance) -> DispatchResult {
            SponsorPools::<T>::try_mutate(game_id, |balance| -> DispatchResult {
                ensure!(*balance >= amount, Error::<T>::InsufficientSponsorFunds);
                *balance = balance
                    .checked_sub(amount)
                    .ok_or(Error::<T>::ArithmeticOverflow)?;
                Ok(())
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_support::{
        assert_noop, assert_ok, construct_runtime, parameter_types,
        traits::{ConstU32, Everything, GetStorageVersion, StorageVersion},
    };
    use frame_system as system;
    use sp_core::H256;
    use sp_runtime::{
        traits::{BlakeTwo256, IdentityLookup},
        BuildStorage,
    };

    type AccountId = u64;
    type Block = system::mocking::MockBlock<Test>;

    construct_runtime!(
        pub enum Test {
            System: system,
            EterraEconomy: crate,
        }
    );

    parameter_types! {
        pub const BlockHashCount: u64 = 250;
    }

    impl system::Config for Test {
        type BaseCallFilter = Everything;
        type BlockWeights = ();
        type BlockLength = ();
        type DbWeight = ();
        type RuntimeOrigin = RuntimeOrigin;
        type RuntimeCall = RuntimeCall;
        type RuntimeEvent = RuntimeEvent;
        type Block = Block;
        type Hash = H256;
        type Hashing = BlakeTwo256;
        type AccountId = AccountId;
        type Lookup = IdentityLookup<Self::AccountId>;
        type BlockHashCount = BlockHashCount;
        type Version = ();
        type PalletInfo = PalletInfo;
        type AccountData = ();
        type OnNewAccount = ();
        type OnKilledAccount = ();
        type SystemWeightInfo = ();
        type SS58Prefix = ();
        type OnSetCode = ();
        type MaxConsumers = ConstU32<16>;
        type Nonce = u64;
        type SingleBlockMigrations = ();
        type MultiBlockMigrator = ();
        type PreInherents = ();
        type PostInherents = ();
        type PostTransactions = ();
        type RuntimeTask = ();
    }

    impl Config for Test {
        type RuntimeEvent = RuntimeEvent;
        type WeightInfo = ();
        type AdminOrigin = frame_system::EnsureRoot<AccountId>;
    }

    pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
        let storage = system::GenesisConfig::<Test>::default()
            .build_storage()
            .expect("frame-system storage build should not fail");
        let mut ext = sp_io::TestExternalities::new(storage);
        ext.execute_with(|| System::set_block_number(1));
        ext
    }

    #[test]
    fn product_fulfillment_requires_active_product_and_rejects_duplicate_receipt() {
        new_test_ext().execute_with(|| {
            assert_ok!(EterraEconomy::create_product(
                RuntimeOrigin::root(),
                10,
                77,
                ProductType::SeasonPass,
                500,
                Some(33),
                Some((9, 3)),
                H256::repeat_byte(1),
            ));

            let receipt = H256::repeat_byte(9);
            assert_noop!(
                EterraEconomy::fulfill_product(RuntimeOrigin::root(), 10, 77, 42, receipt),
                Error::<Test>::ProductNotActive
            );

            assert_ok!(EterraEconomy::set_product_status(
                RuntimeOrigin::root(),
                10,
                77,
                ProductStatus::Active,
            ));
            assert_ok!(EterraEconomy::fulfill_product(
                RuntimeOrigin::root(),
                10,
                77,
                42,
                receipt,
            ));

            assert!(EterraEconomy::has_entitlement(&42, 10, 33));
            assert_eq!(EterraEconomy::credit_balance(&42, 10, 9), 3);
            assert_eq!(RevenueEscrow::<Test>::get(10), 500);
            assert!(FulfilledReceipts::<Test>::get((10, receipt)));

            assert_noop!(
                EterraEconomy::fulfill_product(RuntimeOrigin::root(), 10, 77, 42, receipt),
                Error::<Test>::ReceiptAlreadyFulfilled
            );
        });
    }

    #[test]
    fn sponsor_and_credit_balances_cannot_underflow() {
        new_test_ext().execute_with(|| {
            assert_ok!(EterraEconomy::try_deposit_sponsor_funds(10, 3));
            assert_noop!(
                EterraEconomy::try_spend_sponsor_funds(10, 4),
                Error::<Test>::InsufficientSponsorFunds
            );
            assert_eq!(SponsorPools::<Test>::get(10), 3);
            assert_ok!(EterraEconomy::try_spend_sponsor_funds(10, 3));
            assert_eq!(SponsorPools::<Test>::get(10), 0);

            assert_noop!(
                EterraEconomy::try_consume_credit(&42, 10, 9, 1),
                Error::<Test>::InsufficientCredit
            );
            assert_ok!(EterraEconomy::try_grant_credit(&42, 10, 9, 2));
            assert_ok!(EterraEconomy::try_consume_credit(&42, 10, 9, 1));
            assert_eq!(EterraEconomy::credit_balance(&42, 10, 9), 1);
        });
    }

    #[test]
    fn storage_version_is_declared_without_migration_requirement() {
        new_test_ext().execute_with(|| {
            assert_eq!(
                Pallet::<Test>::in_code_storage_version(),
                StorageVersion::new(1)
            );
        });
    }

    #[test]
    fn dispatchables_emit_events_and_reject_duplicate_or_missing_state() {
        new_test_ext().execute_with(|| {
            assert_ok!(EterraEconomy::create_product(
                RuntimeOrigin::root(),
                10,
                77,
                ProductType::ArcadeCreditPack,
                200,
                None,
                Some((9, 2)),
                H256::repeat_byte(1),
            ));
            System::assert_last_event(RuntimeEvent::EterraEconomy(Event::ProductCreated {
                game_id: 10,
                product_id: 77,
            }));
            assert_noop!(
                EterraEconomy::create_product(
                    RuntimeOrigin::root(),
                    10,
                    77,
                    ProductType::ArcadeCreditPack,
                    200,
                    None,
                    Some((9, 2)),
                    H256::repeat_byte(1),
                ),
                Error::<Test>::ProductAlreadyExists
            );
            assert_noop!(
                EterraEconomy::set_product_status(
                    RuntimeOrigin::root(),
                    10,
                    88,
                    ProductStatus::Active,
                ),
                Error::<Test>::ProductNotFound
            );

            assert_ok!(EterraEconomy::grant_entitlement(
                RuntimeOrigin::root(),
                10,
                42,
                33,
            ));
            System::assert_last_event(RuntimeEvent::EterraEconomy(Event::EntitlementGranted {
                game_id: 10,
                account: 42,
                entitlement_id: 33,
            }));
            assert_ok!(EterraEconomy::revoke_entitlement(
                RuntimeOrigin::root(),
                10,
                42,
                33,
            ));
            assert!(!EterraEconomy::has_entitlement(&42, 10, 33));

            assert_ok!(EterraEconomy::grant_credit(
                RuntimeOrigin::root(),
                10,
                42,
                9,
                5,
            ));
            assert_ok!(EterraEconomy::consume_credit(
                RuntimeOrigin::signed(42),
                10,
                9,
                2,
            ));
            assert_eq!(EterraEconomy::credit_balance(&42, 10, 9), 3);

            assert_ok!(EterraEconomy::deposit_sponsor_funds(
                RuntimeOrigin::root(),
                10,
                100,
            ));
            assert_eq!(SponsorPools::<Test>::get(10), 100);
            assert_ok!(EterraEconomy::record_revenue(RuntimeOrigin::root(), 10, 50,));
            assert_eq!(RevenueEscrow::<Test>::get(10), 50);
        });
    }

    #[test]
    fn checked_accounting_overflow_rolls_back_product_fulfillment() {
        new_test_ext().execute_with(|| {
            RevenueEscrow::<Test>::insert(10, u128::MAX);
            assert_ok!(EterraEconomy::create_product(
                RuntimeOrigin::root(),
                10,
                77,
                ProductType::SeasonPass,
                1,
                Some(33),
                Some((9, 3)),
                H256::repeat_byte(1),
            ));
            assert_ok!(EterraEconomy::set_product_status(
                RuntimeOrigin::root(),
                10,
                77,
                ProductStatus::Active,
            ));

            let receipt = H256::repeat_byte(9);
            assert_noop!(
                EterraEconomy::fulfill_product(RuntimeOrigin::root(), 10, 77, 42, receipt),
                Error::<Test>::ArithmeticOverflow
            );
            assert_eq!(RevenueEscrow::<Test>::get(10), u128::MAX);
            assert!(!EterraEconomy::has_entitlement(&42, 10, 33));
            assert_eq!(EterraEconomy::credit_balance(&42, 10, 9), 0);
            assert!(!FulfilledReceipts::<Test>::get((10, receipt)));

            SponsorPools::<Test>::insert(10, u128::MAX);
            assert_noop!(
                EterraEconomy::deposit_sponsor_funds(RuntimeOrigin::root(), 10, 1),
                Error::<Test>::ArithmeticOverflow
            );
            Credits::<Test>::insert((10, 42, 9), u64::MAX);
            assert_noop!(
                EterraEconomy::grant_credit(RuntimeOrigin::root(), 10, 42, 9, 1),
                Error::<Test>::ArithmeticOverflow
            );
        });
    }
}
