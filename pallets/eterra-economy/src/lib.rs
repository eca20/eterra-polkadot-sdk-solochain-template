//! Eterra Economy pallet.
//!
//! Purpose: products, entitlements, credits, developer sponsor pools, revenue
//! accounting, Arcade Tickets, and the chain-authoritative arcade prize catalog.
#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::too_many_arguments)]

pub use pallet::*;
pub mod weights;
pub use weights::WeightInfo;

use codec::{Decode, Encode, MaxEncodedLen};
use eterra_nexus_primitives::{EconomicRealm, PackCreditSource};
use frame_support::dispatch::DispatchResult;
use scale_info::TypeInfo;
use sp_runtime::{DispatchError, RuntimeDebug};
use sp_std::vec::Vec;

pub type TicketBalance = u128;
pub type AssetId = u32;
pub type SubjectId = u32;
pub type PoolId = u32;

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum PrizeFulfillmentKind {
    RandomSingle,
    RandomPack,
    FeaturedSubject,
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum PrizeAcquisitionSource {
    TicketClaim,
    NativePull,
}

/// Runtime adapter over the configured `pallet-assets` ticket class.
pub trait TicketAssetProvider<AccountId> {
    fn asset_exists(asset_id: AssetId) -> bool;
    fn decimals(asset_id: AssetId) -> u8;
    fn balance(asset_id: AssetId, account: &AccountId) -> TicketBalance;
    fn mint(asset_id: AssetId, account: &AccountId, amount: TicketBalance) -> DispatchResult;
    fn burn(asset_id: AssetId, account: &AccountId, amount: TicketBalance) -> DispatchResult;
    fn transfer(
        asset_id: AssetId,
        from: &AccountId,
        to: &AccountId,
        amount: TicketBalance,
    ) -> DispatchResult;
}

/// Runtime adapter that transfers native-token vending revenue to Treasury.
pub trait NativePaymentProvider<AccountId> {
    fn pay_treasury(account: &AccountId, amount: u128) -> DispatchResult;
}

/// Runtime adapter into the Nexus V2 non-transferable Pack Credit issuer.
///
/// The economy pallet owns ticket pricing, redemption limits, and replay
/// protection. The TCG pallet remains authoritative for the referenced pack
/// catalog and the resulting credit.
pub trait V2PackCreditIssuer<AccountId> {
    fn validate_target(pack_sku: u32, sku_version: u32, realm: EconomicRealm) -> DispatchResult;

    fn issue_pack_credit(
        owner: &AccountId,
        pack_sku: u32,
        sku_version: u32,
        realm: EconomicRealm,
        source: PackCreditSource,
    ) -> DispatchResult;

    #[cfg(feature = "runtime-benchmarks")]
    fn prepare_benchmark_target(_pack_sku: u32, _sku_version: u32, _realm: EconomicRealm) {}
}

impl<AccountId> V2PackCreditIssuer<AccountId> for () {
    fn validate_target(_pack_sku: u32, _sku_version: u32, _realm: EconomicRealm) -> DispatchResult {
        Err(DispatchError::Other("V2 pack credit provider unavailable"))
    }

    fn issue_pack_credit(
        _owner: &AccountId,
        _pack_sku: u32,
        _sku_version: u32,
        _realm: EconomicRealm,
        _source: PackCreditSource,
    ) -> DispatchResult {
        Err(DispatchError::Other("V2 pack credit provider unavailable"))
    }
}

/// Runtime adapter into the Nexus TCG acquisition implementation.
pub trait PrizeFulfillmentProvider<AccountId> {
    fn validate_pool(pool_id: PoolId, featured_subjects: &[SubjectId]) -> DispatchResult;

    fn fulfill(
        account: &AccountId,
        kind: PrizeFulfillmentKind,
        pool_id: PoolId,
        subject_id: Option<SubjectId>,
        entropy: [u8; 32],
        source: PrizeAcquisitionSource,
    ) -> Result<Vec<u32>, DispatchError>;
}

impl<AccountId> PrizeFulfillmentProvider<AccountId> for () {
    fn validate_pool(_pool_id: PoolId, _featured_subjects: &[SubjectId]) -> DispatchResult {
        Ok(())
    }

    fn fulfill(
        _account: &AccountId,
        _kind: PrizeFulfillmentKind,
        _pool_id: PoolId,
        _subject_id: Option<SubjectId>,
        _entropy: [u8; 32],
        _source: PrizeAcquisitionSource,
    ) -> Result<Vec<u32>, DispatchError> {
        Ok(Vec::new())
    }
}

pub trait AccountEligibilityProvider<AccountId> {
    fn eligible(account: &AccountId) -> bool;

    #[cfg(feature = "runtime-benchmarks")]
    fn prepare_benchmark_account(_account: &AccountId) {}
}

impl<AccountId> AccountEligibilityProvider<AccountId> for () {
    fn eligible(_account: &AccountId) -> bool {
        true
    }
}

/// Replaceable randomness boundary. Alpha may use consensus entropy; valuable
/// production catalogs must supply a reviewed manipulation-resistant provider.
pub trait ArcadeRandomnessProvider {
    fn random(domain: &[u8], payload: &[u8]) -> [u8; 32];
}

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame_support::pallet]
pub mod pallet {
    use super::{
        weights::WeightInfo, AccountEligibilityProvider, ArcadeRandomnessProvider, AssetId,
        NativePaymentProvider, PoolId, PrizeAcquisitionSource, PrizeFulfillmentKind,
        PrizeFulfillmentProvider, SubjectId, TicketAssetProvider, TicketBalance,
        V2PackCreditIssuer,
    };
    use eterra_nexus_primitives::{EconomicRealm, Hash32, PackCreditSource};
    use frame_support::{
        dispatch::DispatchResult,
        pallet_prelude::*,
        traits::{BuildGenesisConfig, Get, StorageVersion},
        transactional,
        weights::Weight,
    };
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::{SaturatedConversion, Saturating, Zero};
    use sp_std::vec::Vec;

    pub type GameId = u64;
    pub type ProductId = u64;
    pub type EntitlementId = u32;
    pub type CreditTypeId = u32;
    pub type Balance = u128;
    pub type SkuId = u64;
    pub type RotationId = u64;

    pub type ScoreTiersOf<T> = BoundedVec<ScoreTier, <T as Config>::MaxScoreTiers>;
    pub type EligibleRewardModesOf<T> =
        BoundedVec<TicketRewardMode, <T as Config>::MaxEligibleRewardModes>;
    pub type EligibleEndedReasonsOf<T> = BoundedVec<u8, <T as Config>::MaxEligibleEndedReasons>;
    pub type EligibleSubjectsOf<T> = BoundedVec<SubjectId, <T as Config>::MaxFeaturedPoolSubjects>;
    pub type FeaturedSubjectsOf<T> = BoundedVec<SubjectId, <T as Config>::MaxFeaturedSlots>;
    pub type FeaturedOffersOf<T> = BoundedVec<FeaturedOffer, <T as Config>::MaxFeaturedSlots>;
    pub type PrizeCardIdsOf<T> = BoundedVec<u32, <T as Config>::MaxPrizeCards>;

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

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
    pub struct TicketAssetConfig {
        pub asset_id: AssetId,
        pub config_version: u32,
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
    pub struct ScoreTier {
        pub min_score: u64,
        pub tickets: TicketBalance,
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
    pub enum TicketRewardMode {
        Ranked,
        Unranked,
    }

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct TicketRewardPolicy<T: Config> {
        pub enabled: bool,
        pub eligible_modes: EligibleRewardModesOf<T>,
        pub eligible_ended_reasons: EligibleEndedReasonsOf<T>,
        pub score_tiers: ScoreTiersOf<T>,
        pub per_result_cap: TicketBalance,
        pub window_blocks: BlockNumberFor<T>,
        pub per_account_window_cap: TicketBalance,
        pub config_version: u32,
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
    pub enum PauseDomain {
        TicketEarning,
        TicketTransfers,
        TicketRedemption,
        RandomVending,
        FeaturedVending,
        PackCreditRedemptionV2,
    }

    impl PauseDomain {
        pub const ALL: [Self; 6] = [
            Self::TicketEarning,
            Self::TicketTransfers,
            Self::TicketRedemption,
            Self::RandomVending,
            Self::FeaturedVending,
            Self::PackCreditRedemptionV2,
        ];
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
    pub enum PrizeKind {
        RandomSingle,
        RandomPack,
    }

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct PrizeSku<T: Config> {
        pub kind: PrizeKind,
        pub pool_id: PoolId,
        pub ticket_price: Option<TicketBalance>,
        pub native_price: Option<Balance>,
        pub enabled: bool,
        pub total_cap: Option<u64>,
        pub per_account_window_cap: u32,
        pub window_blocks: BlockNumberFor<T>,
        pub config_version: u32,
    }

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct FeaturedRotationConfig<T: Config> {
        pub enabled: bool,
        pub pool_id: PoolId,
        pub eligible_subjects: EligibleSubjectsOf<T>,
        pub period_blocks: BlockNumberFor<T>,
        pub native_price: Balance,
        pub per_slot_cap: u32,
        pub per_account_limit: u32,
        pub config_version: u32,
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
    pub struct FeaturedOffer {
        pub subject_id: SubjectId,
        pub native_price: Balance,
        pub stock_cap: u32,
        pub sold: u32,
        pub config_version: u32,
    }

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct FeaturedRotation<T: Config> {
        pub rotation_id: RotationId,
        pub starts_at: BlockNumberFor<T>,
        pub ends_at: BlockNumberFor<T>,
        pub pool_id: PoolId,
        pub per_account_limit: u32,
        pub offers: FeaturedOffersOf<T>,
        pub config_version: u32,
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
    pub enum PurchaseTarget {
        CatalogSku(SkuId),
        FeaturedSlot { rotation_id: RotationId, slot: u8 },
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
    pub enum FeaturedRotationFailureReason {
        UnsafeOrInvalidConfiguration,
    }

    /// Versioned Prize Counter offer that redeems Tickets for one Nexus V2
    /// Pack Credit. Alpha offers are deliberately restricted to Training.
    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
    pub struct ArcadePackCreditSkuV2<BlockNumber> {
        pub pack_sku: u32,
        pub pack_sku_version: u32,
        pub economic_realm: EconomicRealm,
        pub ticket_price: TicketBalance,
        pub policy_version: u32,
        pub enabled: bool,
        pub total_cap: Option<u64>,
        pub per_account_window_cap: u32,
        pub window_blocks: BlockNumber,
        pub config_version: u32,
    }

    /// Globally keyed replay receipt for an Arcade Prize Pack Credit
    /// redemption. A retry is a no-op only when its complete caller-supplied
    /// request identity matches this receipt.
    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct ArcadePackCreditRedemptionReceiptV2<T: Config> {
        pub account: T::AccountId,
        pub sku_id: SkuId,
        pub sku_config_version: u32,
        pub pack_sku: u32,
        pub pack_sku_version: u32,
        pub economic_realm: EconomicRealm,
        pub ticket_amount: TicketBalance,
        pub policy_version: u32,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type WeightInfo: WeightInfo;
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;
        type TicketAssets: TicketAssetProvider<Self::AccountId>;
        type NativePayments: NativePaymentProvider<Self::AccountId>;
        type PrizeFulfillment: PrizeFulfillmentProvider<Self::AccountId>;
        type PackCreditIssuer: V2PackCreditIssuer<Self::AccountId>;
        type AccountEligibility: AccountEligibilityProvider<Self::AccountId>;
        type RandomnessProvider: ArcadeRandomnessProvider;

        #[pallet::constant]
        type ArcadeCreditFaucetGameId: Get<GameId>;
        #[pallet::constant]
        type ArcadeCreditFaucetType: Get<CreditTypeId>;
        #[pallet::constant]
        type ArcadeCreditFaucetAmount: Get<u64>;
        #[pallet::constant]
        type MaxScoreTiers: Get<u32>;
        #[pallet::constant]
        type MaxEligibleRewardModes: Get<u32>;
        #[pallet::constant]
        type MaxEligibleEndedReasons: Get<u32>;
        #[pallet::constant]
        type MaxFeaturedPoolSubjects: Get<u32>;
        #[pallet::constant]
        type MaxFeaturedSlots: Get<u32>;
        #[pallet::constant]
        type FeaturedSlotCount: Get<u32>;
        #[pallet::constant]
        type MaxPrizeCards: Get<u32>;
    }

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(3);

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub ticket_asset: Option<(AssetId, u32)>,
        pub paused: bool,
        pub _phantom: PhantomData<T>,
    }

    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self {
                ticket_asset: None,
                paused: true,
                _phantom: PhantomData,
            }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            if let Some((asset_id, config_version)) = self.ticket_asset {
                TicketAsset::<T>::put(TicketAssetConfig {
                    asset_id,
                    config_version,
                });
            }
            for domain in PauseDomain::ALL {
                PausedDomains::<T>::insert(domain, self.paused);
            }
        }
    }

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

    #[pallet::storage]
    #[pallet::getter(fn ticket_asset_config)]
    pub type TicketAsset<T: Config> = StorageValue<_, TicketAssetConfig, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn ticket_reward_policy)]
    pub type TicketRewardPolicies<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        GameId,
        Blake2_128Concat,
        u32,
        TicketRewardPolicy<T>,
        OptionQuery,
    >;

    #[pallet::storage]
    pub type TicketEarningWindows<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, GameId>,
            NMapKey<Blake2_128Concat, u32>,
            NMapKey<Blake2_128Concat, T::AccountId>,
            NMapKey<Blake2_128Concat, u64>,
        ),
        TicketBalance,
        ValueQuery,
    >;

    #[pallet::storage]
    pub type TicketRewardedResults<T: Config> =
        StorageMap<_, Blake2_128Concat, T::Hash, bool, ValueQuery>;

    #[pallet::type_value]
    pub fn DefaultPaused() -> bool {
        true
    }

    #[pallet::storage]
    #[pallet::getter(fn domain_paused)]
    pub type PausedDomains<T: Config> =
        StorageMap<_, Blake2_128Concat, PauseDomain, bool, ValueQuery, DefaultPaused>;

    #[pallet::storage]
    #[pallet::getter(fn account_restricted)]
    pub type RestrictedAccounts<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, bool, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn prize_sku)]
    pub type PrizeSkus<T: Config> =
        StorageMap<_, Blake2_128Concat, SkuId, PrizeSku<T>, OptionQuery>;

    #[pallet::storage]
    pub type PrizeSkuSold<T: Config> = StorageMap<_, Blake2_128Concat, SkuId, u64, ValueQuery>;

    #[pallet::storage]
    pub type PrizeSkuAccountWindows<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, SkuId>,
            NMapKey<Blake2_128Concat, T::AccountId>,
            NMapKey<Blake2_128Concat, u64>,
        ),
        u32,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn arcade_pack_credit_sku_v2)]
    pub type ArcadePackCreditSkusV2<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        SkuId,
        ArcadePackCreditSkuV2<BlockNumberFor<T>>,
        OptionQuery,
    >;

    #[pallet::storage]
    pub type ArcadePackCreditSkuSoldV2<T: Config> =
        StorageMap<_, Blake2_128Concat, SkuId, u64, ValueQuery>;

    #[pallet::storage]
    pub type ArcadePackCreditSkuAccountWindowsV2<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, SkuId>,
            NMapKey<Blake2_128Concat, T::AccountId>,
            NMapKey<Blake2_128Concat, u64>,
        ),
        u32,
        ValueQuery,
    >;

    /// The redemption ID namespace is global across every account and every
    /// Prize Counter SKU.
    #[pallet::storage]
    #[pallet::getter(fn arcade_pack_credit_redemption_receipt_v2)]
    pub type ArcadePackCreditRedemptionReceiptsV2<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        Hash32,
        ArcadePackCreditRedemptionReceiptV2<T>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn featured_rotation_config)]
    pub type FeaturedRotationSettings<T: Config> =
        StorageValue<_, FeaturedRotationConfig<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn current_featured_rotation)]
    pub type CurrentFeaturedRotation<T: Config> = StorageValue<_, FeaturedRotation<T>, OptionQuery>;

    #[pallet::storage]
    pub type NextFeaturedRotationId<T: Config> = StorageValue<_, RotationId, ValueQuery>;

    #[pallet::storage]
    pub type FeaturedAccountPurchases<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, RotationId>,
            NMapKey<Blake2_128Concat, u8>,
            NMapKey<Blake2_128Concat, T::AccountId>,
        ),
        u32,
        ValueQuery,
    >;

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_runtime_upgrade() -> Weight {
            let mut weight = T::DbWeight::get().reads(1);
            let on_chain = StorageVersion::get::<Pallet<T>>();
            if on_chain < StorageVersion::new(2) {
                for domain in PauseDomain::ALL {
                    PausedDomains::<T>::insert(domain, true);
                }
                STORAGE_VERSION.put::<Pallet<T>>();
                weight = weight
                    .saturating_add(T::DbWeight::get().writes(PauseDomain::ALL.len() as u64 + 1));
            } else if on_chain < STORAGE_VERSION {
                // V2 state keeps every prior pause decision. The new Pack
                // Credit bridge alone starts explicitly fail-closed.
                PausedDomains::<T>::insert(PauseDomain::PackCreditRedemptionV2, true);
                STORAGE_VERSION.put::<Pallet<T>>();
                weight = weight.saturating_add(T::DbWeight::get().writes(2));
            }
            weight
        }

        fn on_initialize(now: BlockNumberFor<T>) -> Weight {
            let Some(config) = FeaturedRotationSettings::<T>::get() else {
                return T::DbWeight::get().reads(1);
            };
            if !config.enabled {
                return T::DbWeight::get().reads(1);
            }
            let due = CurrentFeaturedRotation::<T>::get()
                .map(|rotation| now >= rotation.ends_at)
                .unwrap_or(true);
            if !due {
                return T::DbWeight::get().reads(2);
            }
            if Self::try_rotate_featured(now, &config).is_err() {
                PausedDomains::<T>::insert(PauseDomain::FeaturedVending, true);
                Self::deposit_event(Event::FeaturedRotationFailed {
                    rotation_id: NextFeaturedRotationId::<T>::get().saturating_add(1),
                    attempted_at: now,
                    reason: FeaturedRotationFailureReason::UnsafeOrInvalidConfiguration,
                    config_version: config.config_version,
                });
            }
            T::WeightInfo::rotate_featured()
        }
    }

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
        ArcadeCreditFaucetClaimed {
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
        TicketAssetConfigured {
            asset_id: AssetId,
            config_version: u32,
        },
        TicketRewardPolicyUpdated {
            game_id: GameId,
            ruleset_version: u32,
            config_version: u32,
            enabled: bool,
        },
        GameplayTicketsGranted {
            account: T::AccountId,
            game_id: GameId,
            ruleset_version: u32,
            result_id_hash: T::Hash,
            amount: TicketBalance,
            window_index: u64,
            config_version: u32,
        },
        TicketsTransferred {
            from: T::AccountId,
            to: T::AccountId,
            amount: TicketBalance,
        },
        ArcadeEconomyPauseChanged {
            domain: PauseDomain,
            paused: bool,
        },
        ArcadeEconomyAccountRestrictionChanged {
            account: T::AccountId,
            restricted: bool,
        },
        PrizeSkuUpdated {
            sku_id: SkuId,
            kind: PrizeKind,
            config_version: u32,
            enabled: bool,
        },
        FeaturedRotationConfigured {
            config_version: u32,
            period_blocks: BlockNumberFor<T>,
            slot_count: u32,
        },
        FeaturedRotationAdvanced {
            rotation_id: RotationId,
            starts_at: BlockNumberFor<T>,
            ends_at: BlockNumberFor<T>,
            subject_ids: FeaturedSubjectsOf<T>,
            config_version: u32,
        },
        FeaturedRotationFailed {
            rotation_id: RotationId,
            attempted_at: BlockNumberFor<T>,
            reason: FeaturedRotationFailureReason,
            config_version: u32,
        },
        PrizeRedeemed {
            account: T::AccountId,
            sku_id: SkuId,
            ticket_amount: TicketBalance,
            card_ids: PrizeCardIdsOf<T>,
            config_version: u32,
        },
        PrizePurchased {
            account: T::AccountId,
            target: PurchaseTarget,
            native_amount: Balance,
            card_ids: PrizeCardIdsOf<T>,
            config_version: u32,
        },
        ArcadePackCreditSkuUpdatedV2 {
            sku_id: SkuId,
            pack_sku: u32,
            pack_sku_version: u32,
            economic_realm: EconomicRealm,
            policy_version: u32,
            config_version: u32,
            enabled: bool,
        },
        ArcadePackCreditRedeemedV2 {
            account: T::AccountId,
            sku_id: SkuId,
            redemption_id: Hash32,
            pack_sku: u32,
            pack_sku_version: u32,
            economic_realm: EconomicRealm,
            ticket_amount: TicketBalance,
            policy_version: u32,
            config_version: u32,
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
        TicketAssetNotConfigured,
        TicketAssetDoesNotExist,
        TicketAssetMustBeIndivisible,
        InvalidRewardPolicy,
        RewardResultAlreadyProcessed,
        SubsystemPaused,
        AccountNotEligible,
        AccountRestricted,
        InvalidAmount,
        InvalidConfigVersion,
        PrizeSkuNotFound,
        PrizeSkuDisabled,
        PrizePaymentNotSupported,
        PrizeSoldOut,
        PrizeAccountLimitReached,
        InvalidFeaturedRotationConfig,
        FeaturedRotationUnavailable,
        FeaturedSlotNotFound,
        StaleRotation,
        TooManyPrizeCards,
        PrizeFulfillmentFailed,
        ArcadePackCreditSkuNotFound,
        ArcadePackCreditSkuDisabled,
        ArcadePackCreditProductionDisabled,
        InvalidArcadePackCreditSku,
        InvalidArcadePackCreditRedemptionId,
        ArcadePackCreditRedemptionConflict,
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

        #[pallet::call_index(9)]
        #[pallet::weight(T::WeightInfo::claim_arcade_credit())]
        pub fn claim_arcade_credit(origin: OriginFor<T>) -> DispatchResult {
            let account = ensure_signed(origin)?;
            let game_id = T::ArcadeCreditFaucetGameId::get();
            let credit_type = T::ArcadeCreditFaucetType::get();
            let amount = T::ArcadeCreditFaucetAmount::get();
            Self::try_grant_credit(&account, game_id, credit_type, amount)?;
            Self::deposit_event(Event::ArcadeCreditFaucetClaimed {
                game_id,
                account,
                credit_type,
                amount,
            });
            Ok(())
        }

        #[pallet::call_index(10)]
        #[pallet::weight(T::WeightInfo::set_ticket_asset())]
        pub fn set_ticket_asset(
            origin: OriginFor<T>,
            asset_id: AssetId,
            config_version: u32,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(config_version > 0, Error::<T>::InvalidConfigVersion);
            ensure!(
                T::TicketAssets::asset_exists(asset_id),
                Error::<T>::TicketAssetDoesNotExist
            );
            ensure!(
                T::TicketAssets::decimals(asset_id) == 0,
                Error::<T>::TicketAssetMustBeIndivisible
            );
            if let Some(current) = TicketAsset::<T>::get() {
                ensure!(
                    config_version > current.config_version,
                    Error::<T>::InvalidConfigVersion
                );
            }
            TicketAsset::<T>::put(TicketAssetConfig {
                asset_id,
                config_version,
            });
            Self::deposit_event(Event::TicketAssetConfigured {
                asset_id,
                config_version,
            });
            Ok(())
        }

        #[pallet::call_index(11)]
        #[pallet::weight(T::WeightInfo::set_ticket_reward_policy())]
        pub fn set_ticket_reward_policy(
            origin: OriginFor<T>,
            game_id: GameId,
            ruleset_version: u32,
            policy: TicketRewardPolicy<T>,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Self::validate_reward_policy(&policy)?;
            if let Some(current) = TicketRewardPolicies::<T>::get(game_id, ruleset_version) {
                ensure!(
                    policy.config_version > current.config_version,
                    Error::<T>::InvalidConfigVersion
                );
            }
            let config_version = policy.config_version;
            let enabled = policy.enabled;
            TicketRewardPolicies::<T>::insert(game_id, ruleset_version, policy);
            Self::deposit_event(Event::TicketRewardPolicyUpdated {
                game_id,
                ruleset_version,
                config_version,
                enabled,
            });
            Ok(())
        }

        #[pallet::call_index(12)]
        #[pallet::weight(T::WeightInfo::set_arcade_economy_pause())]
        pub fn set_arcade_economy_pause(
            origin: OriginFor<T>,
            domain: PauseDomain,
            paused: bool,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            PausedDomains::<T>::insert(domain, paused);
            Self::deposit_event(Event::ArcadeEconomyPauseChanged { domain, paused });
            Ok(())
        }

        #[pallet::call_index(13)]
        #[pallet::weight(T::WeightInfo::set_arcade_account_restriction())]
        pub fn set_arcade_account_restriction(
            origin: OriginFor<T>,
            account: T::AccountId,
            restricted: bool,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            RestrictedAccounts::<T>::insert(&account, restricted);
            Self::deposit_event(Event::ArcadeEconomyAccountRestrictionChanged {
                account,
                restricted,
            });
            Ok(())
        }

        #[pallet::call_index(14)]
        #[pallet::weight(T::WeightInfo::transfer_tickets())]
        #[transactional]
        pub fn transfer_tickets(
            origin: OriginFor<T>,
            to: T::AccountId,
            amount: TicketBalance,
        ) -> DispatchResult {
            let from = ensure_signed(origin)?;
            ensure!(
                !PausedDomains::<T>::get(PauseDomain::TicketTransfers),
                Error::<T>::SubsystemPaused
            );
            ensure!(amount > 0, Error::<T>::InvalidAmount);
            Self::ensure_account_eligible(&from)?;
            Self::ensure_account_eligible(&to)?;
            let asset = TicketAsset::<T>::get().ok_or(Error::<T>::TicketAssetNotConfigured)?;
            T::TicketAssets::transfer(asset.asset_id, &from, &to, amount)?;
            Self::deposit_event(Event::TicketsTransferred { from, to, amount });
            Ok(())
        }

        #[pallet::call_index(15)]
        #[pallet::weight(T::WeightInfo::upsert_prize_sku())]
        pub fn upsert_prize_sku(
            origin: OriginFor<T>,
            sku_id: SkuId,
            sku: PrizeSku<T>,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Self::validate_prize_sku(&sku)?;
            T::PrizeFulfillment::validate_pool(sku.pool_id, &[])?;
            if let Some(current) = PrizeSkus::<T>::get(sku_id) {
                ensure!(
                    sku.config_version > current.config_version,
                    Error::<T>::InvalidConfigVersion
                );
            }
            let kind = sku.kind;
            let config_version = sku.config_version;
            let enabled = sku.enabled;
            PrizeSkus::<T>::insert(sku_id, sku);
            Self::deposit_event(Event::PrizeSkuUpdated {
                sku_id,
                kind,
                config_version,
                enabled,
            });
            Ok(())
        }

        #[pallet::call_index(16)]
        #[pallet::weight(T::WeightInfo::set_featured_rotation_config())]
        pub fn set_featured_rotation_config(
            origin: OriginFor<T>,
            config: FeaturedRotationConfig<T>,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Self::validate_featured_rotation_config(&config)?;
            T::PrizeFulfillment::validate_pool(
                config.pool_id,
                config.eligible_subjects.as_slice(),
            )?;
            if let Some(current) = FeaturedRotationSettings::<T>::get() {
                ensure!(
                    config.config_version > current.config_version,
                    Error::<T>::InvalidConfigVersion
                );
            }
            let config_version = config.config_version;
            let period_blocks = config.period_blocks;
            FeaturedRotationSettings::<T>::put(config);
            Self::deposit_event(Event::FeaturedRotationConfigured {
                config_version,
                period_blocks,
                slot_count: T::FeaturedSlotCount::get(),
            });
            Ok(())
        }

        /// LegacyV1 direct-card Ticket redemption retained for SCALE
        /// compatibility. Nexus V2 Prize Counter clients must use call 20.
        #[pallet::call_index(17)]
        #[pallet::weight(T::WeightInfo::redeem_prize_with_tickets())]
        #[transactional]
        pub fn redeem_prize_with_tickets(
            origin: OriginFor<T>,
            sku_id: SkuId,
            expected_version: u32,
        ) -> DispatchResult {
            let account = ensure_signed(origin)?;
            Self::try_redeem_prize_with_tickets(&account, sku_id, expected_version)
        }

        #[pallet::call_index(18)]
        #[pallet::weight(T::WeightInfo::purchase_prize_with_native())]
        #[transactional]
        pub fn purchase_prize_with_native(
            origin: OriginFor<T>,
            target: PurchaseTarget,
            expected_version: u32,
        ) -> DispatchResult {
            let account = ensure_signed(origin)?;
            Self::try_purchase_prize_with_native(&account, target, expected_version)
        }

        /// Configure a versioned Training-only Prize Counter Pack Credit offer.
        #[pallet::call_index(19)]
        #[pallet::weight(T::WeightInfo::upsert_arcade_pack_credit_sku_v2())]
        pub fn upsert_arcade_pack_credit_sku_v2(
            origin: OriginFor<T>,
            sku_id: SkuId,
            sku: ArcadePackCreditSkuV2<BlockNumberFor<T>>,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Self::validate_arcade_pack_credit_sku_v2(&sku)?;
            T::PackCreditIssuer::validate_target(
                sku.pack_sku,
                sku.pack_sku_version,
                sku.economic_realm,
            )?;
            if let Some(current) = ArcadePackCreditSkusV2::<T>::get(sku_id) {
                ensure!(
                    sku.config_version > current.config_version,
                    Error::<T>::InvalidConfigVersion
                );
            }
            let event = Event::ArcadePackCreditSkuUpdatedV2 {
                sku_id,
                pack_sku: sku.pack_sku,
                pack_sku_version: sku.pack_sku_version,
                economic_realm: sku.economic_realm,
                policy_version: sku.policy_version,
                config_version: sku.config_version,
                enabled: sku.enabled,
            };
            ArcadePackCreditSkusV2::<T>::insert(sku_id, sku);
            Self::deposit_event(event);
            Ok(())
        }

        /// Atomically burn Tickets and issue one non-transferable Nexus V2 Pack
        /// Credit. `redemption_id` is globally replay protected.
        #[pallet::call_index(20)]
        #[pallet::weight(T::WeightInfo::redeem_arcade_pack_credit_with_tickets_v2())]
        #[transactional]
        pub fn redeem_arcade_pack_credit_with_tickets_v2(
            origin: OriginFor<T>,
            sku_id: SkuId,
            expected_version: u32,
            redemption_id: Hash32,
        ) -> DispatchResult {
            let account = ensure_signed(origin)?;
            ensure!(
                redemption_id != [0u8; 32],
                Error::<T>::InvalidArcadePackCreditRedemptionId
            );
            if let Some(receipt) = ArcadePackCreditRedemptionReceiptsV2::<T>::get(redemption_id) {
                ensure!(
                    receipt.account == account
                        && receipt.sku_id == sku_id
                        && receipt.sku_config_version == expected_version,
                    Error::<T>::ArcadePackCreditRedemptionConflict
                );
                return Ok(());
            }
            Self::try_redeem_arcade_pack_credit_with_tickets_v2(
                &account,
                sku_id,
                expected_version,
                redemption_id,
            )
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

        pub fn ticket_balance(account: &T::AccountId) -> TicketBalance {
            TicketAsset::<T>::get()
                .map(|config| T::TicketAssets::balance(config.asset_id, account))
                .unwrap_or_default()
        }

        #[transactional]
        pub fn try_grant_gameplay_tickets(
            account: &T::AccountId,
            game_id: GameId,
            ruleset_version: u32,
            result_id_hash: T::Hash,
            score: u64,
            ranked: bool,
            ended_reason: u8,
        ) -> Result<TicketBalance, sp_runtime::DispatchError> {
            if PausedDomains::<T>::get(PauseDomain::TicketEarning) {
                return Ok(0);
            }
            let Some(policy) = TicketRewardPolicies::<T>::get(game_id, ruleset_version) else {
                return Ok(0);
            };
            let reward_mode = if ranked {
                TicketRewardMode::Ranked
            } else {
                TicketRewardMode::Unranked
            };
            if !policy.enabled
                || !policy.eligible_modes.contains(&reward_mode)
                || !policy.eligible_ended_reasons.contains(&ended_reason)
                || RestrictedAccounts::<T>::get(account)
                || !T::AccountEligibility::eligible(account)
            {
                return Ok(0);
            }
            if TicketRewardedResults::<T>::get(result_id_hash) {
                return Ok(0);
            }

            let tier_award = policy
                .score_tiers
                .iter()
                .rev()
                .find(|tier| score >= tier.min_score)
                .map(|tier| tier.tickets)
                .unwrap_or_default();
            let desired = tier_award.min(policy.per_result_cap);
            let window_index = Self::window_index(policy.window_blocks);
            let issued =
                TicketEarningWindows::<T>::get((game_id, ruleset_version, account, window_index));
            let remaining = policy.per_account_window_cap.saturating_sub(issued);
            let amount = desired.min(remaining);

            TicketRewardedResults::<T>::insert(result_id_hash, true);
            if amount == 0 {
                return Ok(0);
            }
            let asset = TicketAsset::<T>::get().ok_or(Error::<T>::TicketAssetNotConfigured)?;
            T::TicketAssets::mint(asset.asset_id, account, amount)?;
            TicketEarningWindows::<T>::insert(
                (game_id, ruleset_version, account, window_index),
                issued
                    .checked_add(amount)
                    .ok_or(Error::<T>::ArithmeticOverflow)?,
            );
            Self::deposit_event(Event::GameplayTicketsGranted {
                account: account.clone(),
                game_id,
                ruleset_version,
                result_id_hash,
                amount,
                window_index,
                config_version: policy.config_version,
            });
            Ok(amount)
        }

        fn validate_reward_policy(policy: &TicketRewardPolicy<T>) -> DispatchResult {
            ensure!(policy.config_version > 0, Error::<T>::InvalidConfigVersion);
            ensure!(
                policy.window_blocks.saturated_into::<u64>() > 0
                    && policy.per_result_cap > 0
                    && policy.per_account_window_cap >= policy.per_result_cap
                    && !policy.score_tiers.is_empty()
                    && !policy.eligible_modes.is_empty()
                    && !policy.eligible_ended_reasons.is_empty(),
                Error::<T>::InvalidRewardPolicy
            );
            for (index, mode) in policy.eligible_modes.iter().enumerate() {
                ensure!(
                    !policy
                        .eligible_modes
                        .iter()
                        .skip(index + 1)
                        .any(|other| other == mode),
                    Error::<T>::InvalidRewardPolicy
                );
            }
            let mut previous = None;
            for tier in policy.score_tiers.iter() {
                ensure!(tier.tickets > 0, Error::<T>::InvalidRewardPolicy);
                if let Some(min_score) = previous {
                    ensure!(tier.min_score > min_score, Error::<T>::InvalidRewardPolicy);
                }
                previous = Some(tier.min_score);
            }
            Ok(())
        }

        fn validate_prize_sku(sku: &PrizeSku<T>) -> DispatchResult {
            ensure!(sku.config_version > 0, Error::<T>::InvalidConfigVersion);
            let ticket_valid = sku.ticket_price.map(|price| price > 0).unwrap_or(false);
            let native_valid = sku.native_price.map(|price| price > 0).unwrap_or(false);
            ensure!(ticket_valid || native_valid, Error::<T>::InvalidAmount);
            ensure!(
                sku.window_blocks.saturated_into::<u64>() > 0
                    && sku.per_account_window_cap > 0
                    && sku.total_cap.map(|cap| cap > 0).unwrap_or(true),
                Error::<T>::InvalidAmount
            );
            Ok(())
        }

        fn validate_featured_rotation_config(config: &FeaturedRotationConfig<T>) -> DispatchResult {
            ensure!(config.config_version > 0, Error::<T>::InvalidConfigVersion);
            let slot_count = T::FeaturedSlotCount::get();
            ensure!(
                slot_count > 0
                    && slot_count <= T::MaxFeaturedSlots::get()
                    && config.eligible_subjects.len() as u32 >= slot_count
                    && config.period_blocks.saturated_into::<u64>() > 0
                    && config.native_price > 0
                    && config.per_slot_cap > 0
                    && config.per_account_limit > 0,
                Error::<T>::InvalidFeaturedRotationConfig
            );
            for (index, subject) in config.eligible_subjects.iter().enumerate() {
                ensure!(
                    !config
                        .eligible_subjects
                        .iter()
                        .skip(index + 1)
                        .any(|other| other == subject),
                    Error::<T>::InvalidFeaturedRotationConfig
                );
            }
            Ok(())
        }

        fn ensure_account_eligible(account: &T::AccountId) -> DispatchResult {
            ensure!(
                !RestrictedAccounts::<T>::get(account),
                Error::<T>::AccountRestricted
            );
            ensure!(
                T::AccountEligibility::eligible(account),
                Error::<T>::AccountNotEligible
            );
            Ok(())
        }

        fn window_index(window_blocks: BlockNumberFor<T>) -> u64 {
            let now: u64 = frame_system::Pallet::<T>::block_number().saturated_into();
            let window: u64 = window_blocks.saturated_into::<u64>().max(1);
            now / window
        }

        fn fulfillment_kind(kind: PrizeKind) -> PrizeFulfillmentKind {
            match kind {
                PrizeKind::RandomSingle => PrizeFulfillmentKind::RandomSingle,
                PrizeKind::RandomPack => PrizeFulfillmentKind::RandomPack,
            }
        }

        fn bounded_card_ids(card_ids: Vec<u32>) -> Result<PrizeCardIdsOf<T>, DispatchError> {
            ensure!(!card_ids.is_empty(), Error::<T>::PrizeFulfillmentFailed);
            PrizeCardIdsOf::<T>::try_from(card_ids)
                .map_err(|_| Error::<T>::TooManyPrizeCards.into())
        }

        fn entropy(account: &T::AccountId, target: &PurchaseTarget, nonce: u64) -> [u8; 32] {
            let payload = (
                account,
                target,
                nonce,
                frame_system::Pallet::<T>::block_number(),
            )
                .encode();
            T::RandomnessProvider::random(b"eterra/arcade-prize/v1", &payload)
        }

        fn ensure_sku_capacity(
            account: &T::AccountId,
            sku_id: SkuId,
            sku: &PrizeSku<T>,
        ) -> Result<(u64, u32, u64), DispatchError> {
            let sold = PrizeSkuSold::<T>::get(sku_id);
            if let Some(cap) = sku.total_cap {
                ensure!(sold < cap, Error::<T>::PrizeSoldOut);
            }
            let window = Self::window_index(sku.window_blocks);
            let account_count = PrizeSkuAccountWindows::<T>::get((sku_id, account, window));
            ensure!(
                account_count < sku.per_account_window_cap,
                Error::<T>::PrizeAccountLimitReached
            );
            Ok((sold, account_count, window))
        }

        fn validate_arcade_pack_credit_sku_v2(
            sku: &ArcadePackCreditSkuV2<BlockNumberFor<T>>,
        ) -> DispatchResult {
            ensure!(
                sku.economic_realm == EconomicRealm::Training,
                Error::<T>::ArcadePackCreditProductionDisabled
            );
            ensure!(
                sku.pack_sku > 0
                    && sku.pack_sku_version > 0
                    && sku.ticket_price > 0
                    && sku.policy_version > 0
                    && sku.per_account_window_cap > 0
                    && !sku.window_blocks.is_zero()
                    && sku.config_version > 0
                    && sku.total_cap.map(|cap| cap > 0).unwrap_or(true),
                Error::<T>::InvalidArcadePackCreditSku
            );
            Ok(())
        }

        fn ensure_arcade_pack_credit_sku_capacity_v2(
            account: &T::AccountId,
            sku_id: SkuId,
            sku: &ArcadePackCreditSkuV2<BlockNumberFor<T>>,
        ) -> Result<(u64, u32, u64), DispatchError> {
            let sold = ArcadePackCreditSkuSoldV2::<T>::get(sku_id);
            if let Some(cap) = sku.total_cap {
                ensure!(sold < cap, Error::<T>::PrizeSoldOut);
            }
            let window = Self::window_index(sku.window_blocks);
            let account_count =
                ArcadePackCreditSkuAccountWindowsV2::<T>::get((sku_id, account, window));
            ensure!(
                account_count < sku.per_account_window_cap,
                Error::<T>::PrizeAccountLimitReached
            );
            Ok((sold, account_count, window))
        }

        #[transactional]
        fn try_redeem_arcade_pack_credit_with_tickets_v2(
            account: &T::AccountId,
            sku_id: SkuId,
            expected_version: u32,
            redemption_id: Hash32,
        ) -> DispatchResult {
            ensure!(
                !PausedDomains::<T>::get(PauseDomain::PackCreditRedemptionV2),
                Error::<T>::SubsystemPaused
            );
            Self::ensure_account_eligible(account)?;
            let sku = ArcadePackCreditSkusV2::<T>::get(sku_id)
                .ok_or(Error::<T>::ArcadePackCreditSkuNotFound)?;
            ensure!(sku.enabled, Error::<T>::ArcadePackCreditSkuDisabled);
            ensure!(
                sku.config_version == expected_version,
                Error::<T>::InvalidConfigVersion
            );
            Self::validate_arcade_pack_credit_sku_v2(&sku)?;
            T::PackCreditIssuer::validate_target(
                sku.pack_sku,
                sku.pack_sku_version,
                sku.economic_realm,
            )?;
            let (sold, account_count, window) =
                Self::ensure_arcade_pack_credit_sku_capacity_v2(account, sku_id, &sku)?;
            let next_sold = sold.checked_add(1).ok_or(Error::<T>::ArithmeticOverflow)?;
            let next_account_count = account_count
                .checked_add(1)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let asset = TicketAsset::<T>::get().ok_or(Error::<T>::TicketAssetNotConfigured)?;
            let source = PackCreditSource::ArcadePrize {
                policy_version: sku.policy_version,
                redemption_id,
            };
            T::PackCreditIssuer::issue_pack_credit(
                account,
                sku.pack_sku,
                sku.pack_sku_version,
                sku.economic_realm,
                source,
            )?;
            T::TicketAssets::burn(asset.asset_id, account, sku.ticket_price)?;
            ArcadePackCreditSkuSoldV2::<T>::insert(sku_id, next_sold);
            ArcadePackCreditSkuAccountWindowsV2::<T>::insert(
                (sku_id, account, window),
                next_account_count,
            );
            ArcadePackCreditRedemptionReceiptsV2::<T>::insert(
                redemption_id,
                ArcadePackCreditRedemptionReceiptV2::<T> {
                    account: account.clone(),
                    sku_id,
                    sku_config_version: sku.config_version,
                    pack_sku: sku.pack_sku,
                    pack_sku_version: sku.pack_sku_version,
                    economic_realm: sku.economic_realm,
                    ticket_amount: sku.ticket_price,
                    policy_version: sku.policy_version,
                },
            );
            Self::deposit_event(Event::ArcadePackCreditRedeemedV2 {
                account: account.clone(),
                sku_id,
                redemption_id,
                pack_sku: sku.pack_sku,
                pack_sku_version: sku.pack_sku_version,
                economic_realm: sku.economic_realm,
                ticket_amount: sku.ticket_price,
                policy_version: sku.policy_version,
                config_version: sku.config_version,
            });
            Ok(())
        }

        #[transactional]
        fn try_redeem_prize_with_tickets(
            account: &T::AccountId,
            sku_id: SkuId,
            expected_version: u32,
        ) -> DispatchResult {
            ensure!(
                !PausedDomains::<T>::get(PauseDomain::TicketRedemption),
                Error::<T>::SubsystemPaused
            );
            Self::ensure_account_eligible(account)?;
            let sku = PrizeSkus::<T>::get(sku_id).ok_or(Error::<T>::PrizeSkuNotFound)?;
            ensure!(sku.enabled, Error::<T>::PrizeSkuDisabled);
            ensure!(
                sku.config_version == expected_version,
                Error::<T>::InvalidConfigVersion
            );
            let price = sku
                .ticket_price
                .ok_or(Error::<T>::PrizePaymentNotSupported)?;
            let (sold, account_count, window) = Self::ensure_sku_capacity(account, sku_id, &sku)?;
            let target = PurchaseTarget::CatalogSku(sku_id);
            let cards = T::PrizeFulfillment::fulfill(
                account,
                Self::fulfillment_kind(sku.kind),
                sku.pool_id,
                None,
                Self::entropy(account, &target, sold),
                PrizeAcquisitionSource::TicketClaim,
            )?;
            let card_ids = Self::bounded_card_ids(cards)?;
            let asset = TicketAsset::<T>::get().ok_or(Error::<T>::TicketAssetNotConfigured)?;
            T::TicketAssets::burn(asset.asset_id, account, price)?;
            PrizeSkuSold::<T>::insert(
                sku_id,
                sold.checked_add(1).ok_or(Error::<T>::ArithmeticOverflow)?,
            );
            PrizeSkuAccountWindows::<T>::insert(
                (sku_id, account, window),
                account_count
                    .checked_add(1)
                    .ok_or(Error::<T>::ArithmeticOverflow)?,
            );
            Self::deposit_event(Event::PrizeRedeemed {
                account: account.clone(),
                sku_id,
                ticket_amount: price,
                card_ids,
                config_version: sku.config_version,
            });
            Ok(())
        }

        #[transactional]
        fn try_purchase_prize_with_native(
            account: &T::AccountId,
            target: PurchaseTarget,
            expected_version: u32,
        ) -> DispatchResult {
            Self::ensure_account_eligible(account)?;
            match target.clone() {
                PurchaseTarget::CatalogSku(sku_id) => {
                    ensure!(
                        !PausedDomains::<T>::get(PauseDomain::RandomVending),
                        Error::<T>::SubsystemPaused
                    );
                    let sku = PrizeSkus::<T>::get(sku_id).ok_or(Error::<T>::PrizeSkuNotFound)?;
                    ensure!(sku.enabled, Error::<T>::PrizeSkuDisabled);
                    ensure!(
                        sku.config_version == expected_version,
                        Error::<T>::InvalidConfigVersion
                    );
                    let price = sku
                        .native_price
                        .ok_or(Error::<T>::PrizePaymentNotSupported)?;
                    let (sold, account_count, window) =
                        Self::ensure_sku_capacity(account, sku_id, &sku)?;
                    let cards = T::PrizeFulfillment::fulfill(
                        account,
                        Self::fulfillment_kind(sku.kind),
                        sku.pool_id,
                        None,
                        Self::entropy(account, &target, sold),
                        PrizeAcquisitionSource::NativePull,
                    )?;
                    let card_ids = Self::bounded_card_ids(cards)?;
                    T::NativePayments::pay_treasury(account, price)?;
                    PrizeSkuSold::<T>::insert(
                        sku_id,
                        sold.checked_add(1).ok_or(Error::<T>::ArithmeticOverflow)?,
                    );
                    PrizeSkuAccountWindows::<T>::insert(
                        (sku_id, account, window),
                        account_count
                            .checked_add(1)
                            .ok_or(Error::<T>::ArithmeticOverflow)?,
                    );
                    Self::deposit_event(Event::PrizePurchased {
                        account: account.clone(),
                        target,
                        native_amount: price,
                        card_ids,
                        config_version: sku.config_version,
                    });
                }
                PurchaseTarget::FeaturedSlot { rotation_id, slot } => {
                    ensure!(
                        !PausedDomains::<T>::get(PauseDomain::FeaturedVending),
                        Error::<T>::SubsystemPaused
                    );
                    let mut rotation = CurrentFeaturedRotation::<T>::get()
                        .ok_or(Error::<T>::FeaturedRotationUnavailable)?;
                    ensure!(
                        rotation.rotation_id == rotation_id,
                        Error::<T>::StaleRotation
                    );
                    ensure!(
                        rotation.config_version == expected_version,
                        Error::<T>::InvalidConfigVersion
                    );
                    let now = frame_system::Pallet::<T>::block_number();
                    ensure!(
                        now >= rotation.starts_at && now < rotation.ends_at,
                        Error::<T>::StaleRotation
                    );
                    let rotation_pool_id = rotation.pool_id;
                    let rotation_account_limit = rotation.per_account_limit;
                    let offer = rotation
                        .offers
                        .get_mut(slot as usize)
                        .ok_or(Error::<T>::FeaturedSlotNotFound)?;
                    ensure!(offer.sold < offer.stock_cap, Error::<T>::PrizeSoldOut);
                    let account_count =
                        FeaturedAccountPurchases::<T>::get((rotation_id, slot, account));
                    ensure!(
                        account_count < rotation_account_limit,
                        Error::<T>::PrizeAccountLimitReached
                    );
                    let subject_id = offer.subject_id;
                    let price = offer.native_price;
                    let offer_version = offer.config_version;
                    let cards = T::PrizeFulfillment::fulfill(
                        account,
                        PrizeFulfillmentKind::FeaturedSubject,
                        rotation_pool_id,
                        Some(subject_id),
                        Self::entropy(account, &target, u64::from(offer.sold)),
                        PrizeAcquisitionSource::NativePull,
                    )?;
                    let card_ids = Self::bounded_card_ids(cards)?;
                    T::NativePayments::pay_treasury(account, price)?;
                    offer.sold = offer
                        .sold
                        .checked_add(1)
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    CurrentFeaturedRotation::<T>::put(rotation);
                    FeaturedAccountPurchases::<T>::insert(
                        (rotation_id, slot, account),
                        account_count
                            .checked_add(1)
                            .ok_or(Error::<T>::ArithmeticOverflow)?,
                    );
                    Self::deposit_event(Event::PrizePurchased {
                        account: account.clone(),
                        target,
                        native_amount: price,
                        card_ids,
                        config_version: offer_version,
                    });
                }
            }
            Ok(())
        }

        fn try_rotate_featured(
            now: BlockNumberFor<T>,
            config: &FeaturedRotationConfig<T>,
        ) -> DispatchResult {
            Self::validate_featured_rotation_config(config)?;
            let rotation_id = NextFeaturedRotationId::<T>::get()
                .checked_add(1)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let seed = T::RandomnessProvider::random(
                b"eterra/featured-rotation/v1",
                &(rotation_id, now, config.config_version).encode(),
            );
            let mut offers = FeaturedOffersOf::<T>::default();
            let mut chosen: Vec<SubjectId> = Vec::new();
            let pool_len = config.eligible_subjects.len();
            for slot in 0..T::FeaturedSlotCount::get() {
                let slot_entropy = T::RandomnessProvider::random(
                    b"eterra/featured-slot/v1",
                    &(seed, rotation_id, slot).encode(),
                );
                let mut index = u32::from_le_bytes([
                    slot_entropy[0],
                    slot_entropy[1],
                    slot_entropy[2],
                    slot_entropy[3],
                ]) as usize
                    % pool_len;
                for _ in 0..pool_len {
                    let subject = config.eligible_subjects[index];
                    if !chosen.contains(&subject) {
                        chosen.push(subject);
                        offers
                            .try_push(FeaturedOffer {
                                subject_id: subject,
                                native_price: config.native_price,
                                stock_cap: config.per_slot_cap,
                                sold: 0,
                                config_version: config.config_version,
                            })
                            .map_err(|_| Error::<T>::InvalidFeaturedRotationConfig)?;
                        break;
                    }
                    index = (index + 1) % pool_len;
                }
            }
            ensure!(
                offers.len() as u32 == T::FeaturedSlotCount::get(),
                Error::<T>::InvalidFeaturedRotationConfig
            );
            let subject_ids = FeaturedSubjectsOf::<T>::try_from(chosen)
                .map_err(|_| Error::<T>::InvalidFeaturedRotationConfig)?;
            let ends_at = now.saturating_add(config.period_blocks);
            CurrentFeaturedRotation::<T>::put(FeaturedRotation::<T> {
                rotation_id,
                starts_at: now,
                ends_at,
                pool_id: config.pool_id,
                per_account_limit: config.per_account_limit,
                offers,
                config_version: config.config_version,
            });
            NextFeaturedRotationId::<T>::put(rotation_id);
            Self::deposit_event(Event::FeaturedRotationAdvanced {
                rotation_id,
                starts_at: now,
                ends_at,
                subject_ids,
                config_version: config.config_version,
            });
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_support::{
        assert_noop, assert_ok, construct_runtime, parameter_types,
        traits::{ConstU32, Everything, GetStorageVersion, Hooks, StorageVersion},
    };
    use frame_system as system;
    use sp_core::H256;
    use sp_runtime::{
        traits::{BlakeTwo256, IdentityLookup},
        BuildStorage, TokenError,
    };
    use std::{cell::RefCell, collections::BTreeMap};

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
        type TicketAssets = MockTicketAssets;
        type NativePayments = MockNativePayments;
        type PrizeFulfillment = MockPrizeFulfillment;
        type PackCreditIssuer = MockPackCreditIssuer;
        type AccountEligibility = MockAccountEligibility;
        type RandomnessProvider = MockRandomness;
        type ArcadeCreditFaucetGameId = ArcadeCreditFaucetGameId;
        type ArcadeCreditFaucetType = ArcadeCreditFaucetType;
        type ArcadeCreditFaucetAmount = ArcadeCreditFaucetAmount;
        type MaxScoreTiers = ConstU32<16>;
        type MaxEligibleRewardModes = ConstU32<2>;
        type MaxEligibleEndedReasons = ConstU32<16>;
        type MaxFeaturedPoolSubjects = ConstU32<128>;
        type MaxFeaturedSlots = ConstU32<12>;
        type FeaturedSlotCount = ConstU32<12>;
        type MaxPrizeCards = ConstU32<6>;
    }

    pub struct MockTicketAssets;
    thread_local! {
        static MOCK_TICKET_BALANCES: RefCell<BTreeMap<AccountId, TicketBalance>> =
            const { RefCell::new(BTreeMap::new()) };
        static MOCK_NATIVE_PAYMENTS: RefCell<Vec<(AccountId, u128)>> = const { RefCell::new(Vec::new()) };
        static MOCK_MINT_FAILS: RefCell<bool> = const { RefCell::new(false) };
        static MOCK_NATIVE_PAYMENT_FAILS: RefCell<bool> = const { RefCell::new(false) };
        static MOCK_FULFILLMENT_FAILS: RefCell<bool> = const { RefCell::new(false) };
        static MOCK_PACK_CREDIT_ISSUANCE_FAILS: RefCell<bool> = const { RefCell::new(false) };
    }

    type MockIssuedPackCredit = (AccountId, u32, u32, EconomicRealm, PackCreditSource);
    const MOCK_ISSUED_PACK_CREDITS_KEY: &[u8] = b":eterra-economy:test:issued-pack-credits";

    fn mock_issued_pack_credits() -> Vec<MockIssuedPackCredit> {
        sp_io::storage::get(MOCK_ISSUED_PACK_CREDITS_KEY)
            .and_then(|encoded| Vec::<MockIssuedPackCredit>::decode(&mut &encoded[..]).ok())
            .unwrap_or_default()
    }

    pub struct MockPackCreditIssuer;
    impl V2PackCreditIssuer<AccountId> for MockPackCreditIssuer {
        fn validate_target(
            pack_sku: u32,
            sku_version: u32,
            realm: EconomicRealm,
        ) -> DispatchResult {
            if pack_sku == 1 && sku_version == 1 && realm == EconomicRealm::Training {
                Ok(())
            } else {
                Err(DispatchError::Other("mock pack target missing"))
            }
        }

        fn issue_pack_credit(
            owner: &AccountId,
            pack_sku: u32,
            sku_version: u32,
            realm: EconomicRealm,
            source: PackCreditSource,
        ) -> DispatchResult {
            if MOCK_PACK_CREDIT_ISSUANCE_FAILS.with(|fails| *fails.borrow()) {
                return Err(DispatchError::Other("mock pack credit issuance failed"));
            }
            Self::validate_target(pack_sku, sku_version, realm)?;
            let mut issued = mock_issued_pack_credits();
            issued.push((*owner, pack_sku, sku_version, realm, source));
            sp_io::storage::set(MOCK_ISSUED_PACK_CREDITS_KEY, &issued.encode());
            Ok(())
        }
    }

    impl TicketAssetProvider<AccountId> for MockTicketAssets {
        fn asset_exists(asset_id: AssetId) -> bool {
            matches!(asset_id, 3 | 4)
        }
        fn decimals(asset_id: AssetId) -> u8 {
            if asset_id == 3 {
                0
            } else {
                2
            }
        }
        fn balance(_asset_id: AssetId, account: &AccountId) -> TicketBalance {
            MOCK_TICKET_BALANCES
                .with(|balances| balances.borrow().get(account).copied().unwrap_or_default())
        }
        fn mint(_asset_id: AssetId, account: &AccountId, amount: TicketBalance) -> DispatchResult {
            if MOCK_MINT_FAILS.with(|fails| *fails.borrow()) {
                return Err(TokenError::FundsUnavailable.into());
            }
            MOCK_TICKET_BALANCES.with(|balances| {
                let current = balances.borrow().get(account).copied().unwrap_or_default();
                let next = current
                    .checked_add(amount)
                    .ok_or(Error::<Test>::ArithmeticOverflow)?;
                balances.borrow_mut().insert(*account, next);
                Ok(())
            })
        }
        fn burn(_asset_id: AssetId, account: &AccountId, amount: TicketBalance) -> DispatchResult {
            MOCK_TICKET_BALANCES.with(|balances| {
                let current = balances.borrow().get(account).copied().unwrap_or_default();
                let next = current
                    .checked_sub(amount)
                    .ok_or(TokenError::FundsUnavailable)?;
                balances.borrow_mut().insert(*account, next);
                Ok(())
            })
        }
        fn transfer(
            _asset_id: AssetId,
            from: &AccountId,
            to: &AccountId,
            amount: TicketBalance,
        ) -> DispatchResult {
            Self::burn(3, from, amount)?;
            Self::mint(3, to, amount)
        }
    }

    pub struct MockNativePayments;
    impl NativePaymentProvider<AccountId> for MockNativePayments {
        fn pay_treasury(account: &AccountId, amount: u128) -> DispatchResult {
            if MOCK_NATIVE_PAYMENT_FAILS.with(|fails| *fails.borrow()) {
                return Err(TokenError::FundsUnavailable.into());
            }
            MOCK_NATIVE_PAYMENTS.with(|payments| payments.borrow_mut().push((*account, amount)));
            Ok(())
        }
    }

    pub struct MockPrizeFulfillment;
    impl PrizeFulfillmentProvider<AccountId> for MockPrizeFulfillment {
        fn validate_pool(_pool_id: PoolId, _featured_subjects: &[SubjectId]) -> DispatchResult {
            Ok(())
        }

        fn fulfill(
            _account: &AccountId,
            kind: PrizeFulfillmentKind,
            _pool_id: PoolId,
            _subject_id: Option<SubjectId>,
            _entropy: [u8; 32],
            _source: PrizeAcquisitionSource,
        ) -> Result<Vec<u32>, sp_runtime::DispatchError> {
            if MOCK_FULFILLMENT_FAILS.with(|fails| *fails.borrow()) {
                return Err(Error::<Test>::PrizeFulfillmentFailed.into());
            }
            Ok(match kind {
                PrizeFulfillmentKind::RandomPack => vec![1, 2, 3, 4, 5, 6],
                _ => vec![1],
            })
        }
    }

    pub struct MockAccountEligibility;
    impl AccountEligibilityProvider<AccountId> for MockAccountEligibility {
        fn eligible(account: &AccountId) -> bool {
            *account != 99
        }
    }

    pub struct MockRandomness;
    impl ArcadeRandomnessProvider for MockRandomness {
        fn random(domain: &[u8], payload: &[u8]) -> [u8; 32] {
            let mut bytes = domain.to_vec();
            bytes.extend_from_slice(payload);
            sp_io::hashing::blake2_256(&bytes)
        }
    }

    parameter_types! {
        pub const ArcadeCreditFaucetGameId: GameId = 1000;
        pub const ArcadeCreditFaucetType: CreditTypeId = 1;
        pub const ArcadeCreditFaucetAmount: u64 = 1000;
    }

    pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
        let storage = system::GenesisConfig::<Test>::default()
            .build_storage()
            .expect("frame-system storage build should not fail");
        let mut ext = sp_io::TestExternalities::new(storage);
        ext.execute_with(|| {
            System::set_block_number(1);
            MOCK_TICKET_BALANCES.with(|balances| balances.borrow_mut().clear());
            MOCK_NATIVE_PAYMENTS.with(|payments| payments.borrow_mut().clear());
            MOCK_MINT_FAILS.with(|fails| *fails.borrow_mut() = false);
            MOCK_NATIVE_PAYMENT_FAILS.with(|fails| *fails.borrow_mut() = false);
            MOCK_FULFILLMENT_FAILS.with(|fails| *fails.borrow_mut() = false);
            MOCK_PACK_CREDIT_ISSUANCE_FAILS.with(|fails| *fails.borrow_mut() = false);
            sp_io::storage::clear(MOCK_ISSUED_PACK_CREDITS_KEY);
        });
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
    fn arcade_credit_faucet_grants_configured_alpha_amount() {
        new_test_ext().execute_with(|| {
            assert_eq!(EterraEconomy::credit_balance(&42, 1000, 1), 0);
            assert_ok!(EterraEconomy::claim_arcade_credit(RuntimeOrigin::signed(
                42
            )));
            assert_eq!(EterraEconomy::credit_balance(&42, 1000, 1), 1000);
            assert_ok!(EterraEconomy::claim_arcade_credit(RuntimeOrigin::signed(
                42
            )));
            assert_eq!(EterraEconomy::credit_balance(&42, 1000, 1), 2000);
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
    fn economy_migration_adds_v2_bridge_pause_without_reopening_or_repausing_legacy() {
        new_test_ext().execute_with(|| {
            StorageVersion::new(1).put::<Pallet<Test>>();
            for domain in PauseDomain::ALL {
                PausedDomains::<Test>::insert(domain, false);
                assert!(!PausedDomains::<Test>::get(domain));
            }

            <Pallet<Test> as Hooks<u64>>::on_runtime_upgrade();
            assert_eq!(
                Pallet::<Test>::on_chain_storage_version(),
                StorageVersion::new(3)
            );
            for domain in PauseDomain::ALL {
                assert!(PausedDomains::<Test>::get(domain));
            }

            // A live storage-V2 chain keeps all prior legacy pause choices;
            // only the newly introduced V2 bridge is materialized as paused.
            StorageVersion::new(2).put::<Pallet<Test>>();
            PausedDomains::<Test>::insert(PauseDomain::TicketTransfers, false);
            PausedDomains::<Test>::insert(PauseDomain::TicketRedemption, false);
            PausedDomains::<Test>::remove(PauseDomain::PackCreditRedemptionV2);
            assert!(!PausedDomains::<Test>::contains_key(
                PauseDomain::PackCreditRedemptionV2
            ));
            <Pallet<Test> as Hooks<u64>>::on_runtime_upgrade();
            assert!(!PausedDomains::<Test>::get(PauseDomain::TicketTransfers));
            assert!(!PausedDomains::<Test>::get(PauseDomain::TicketRedemption));
            assert!(PausedDomains::<Test>::get(
                PauseDomain::PackCreditRedemptionV2
            ));
            assert_eq!(
                Pallet::<Test>::on_chain_storage_version(),
                StorageVersion::new(3)
            );

            PausedDomains::<Test>::insert(PauseDomain::PackCreditRedemptionV2, false);
            <Pallet<Test> as Hooks<u64>>::on_runtime_upgrade();
            assert!(!PausedDomains::<Test>::get(
                PauseDomain::PackCreditRedemptionV2
            ));
        });
    }

    #[test]
    fn default_genesis_starts_every_arcade_economy_domain_paused() {
        let mut storage = system::GenesisConfig::<Test>::default()
            .build_storage()
            .expect("frame-system storage build should not fail");
        GenesisConfig::<Test>::default()
            .assimilate_storage(&mut storage)
            .expect("economy genesis should assimilate");
        sp_io::TestExternalities::new(storage).execute_with(|| {
            for domain in PauseDomain::ALL {
                assert!(PausedDomains::<Test>::get(domain));
            }
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

    fn reward_policy() -> TicketRewardPolicy<Test> {
        TicketRewardPolicy::<Test> {
            enabled: true,
            eligible_modes: vec![TicketRewardMode::Ranked]
                .try_into()
                .expect("mode bound"),
            eligible_ended_reasons: vec![0u8].try_into().expect("reason bound"),
            score_tiers: vec![
                ScoreTier {
                    min_score: 100,
                    tickets: 10,
                },
                ScoreTier {
                    min_score: 1_000,
                    tickets: 40,
                },
            ]
            .try_into()
            .expect("tier bound"),
            per_result_cap: 50,
            window_blocks: 100,
            per_account_window_cap: 70,
            config_version: 1,
        }
    }

    fn configure_ticket_rewards() {
        assert_ok!(EterraEconomy::set_ticket_asset(RuntimeOrigin::root(), 3, 1));
        assert_ok!(EterraEconomy::set_ticket_reward_policy(
            RuntimeOrigin::root(),
            1003,
            1,
            reward_policy(),
        ));
        assert_ok!(EterraEconomy::set_arcade_economy_pause(
            RuntimeOrigin::root(),
            PauseDomain::TicketEarning,
            false,
        ));
    }

    fn random_pack_sku(config_version: u32) -> PrizeSku<Test> {
        PrizeSku::<Test> {
            kind: PrizeKind::RandomPack,
            pool_id: 9,
            ticket_price: Some(20),
            native_price: Some(500),
            enabled: true,
            total_cap: Some(10),
            per_account_window_cap: 2,
            window_blocks: 100,
            config_version,
        }
    }

    fn arcade_pack_credit_sku_v2(config_version: u32) -> ArcadePackCreditSkuV2<u64> {
        ArcadePackCreditSkuV2 {
            pack_sku: 1,
            pack_sku_version: 1,
            economic_realm: EconomicRealm::Training,
            ticket_price: 20,
            policy_version: 7,
            enabled: true,
            total_cap: Some(10),
            per_account_window_cap: 2,
            window_blocks: 100,
            config_version,
        }
    }

    fn featured_config(config_version: u32) -> FeaturedRotationConfig<Test> {
        FeaturedRotationConfig::<Test> {
            enabled: true,
            pool_id: 5,
            eligible_subjects: (1u32..=20)
                .collect::<Vec<_>>()
                .try_into()
                .expect("pool bound"),
            period_blocks: 5,
            native_price: 250,
            per_slot_cap: 2,
            per_account_limit: 1,
            config_version,
        }
    }

    #[test]
    fn ticket_asset_configuration_requires_zero_decimals() {
        new_test_ext().execute_with(|| {
            assert_noop!(
                EterraEconomy::set_ticket_asset(RuntimeOrigin::root(), 3, 0),
                Error::<Test>::InvalidConfigVersion
            );
            assert_noop!(
                EterraEconomy::set_ticket_asset(RuntimeOrigin::root(), 99, 1),
                Error::<Test>::TicketAssetDoesNotExist
            );
            assert_noop!(
                EterraEconomy::set_ticket_asset(RuntimeOrigin::root(), 4, 1),
                Error::<Test>::TicketAssetMustBeIndivisible
            );
            assert_ok!(EterraEconomy::set_ticket_asset(RuntimeOrigin::root(), 3, 1));
            assert_noop!(
                EterraEconomy::set_ticket_asset(RuntimeOrigin::root(), 3, 1),
                Error::<Test>::InvalidConfigVersion
            );
        });
    }

    #[test]
    fn chain_configuration_rejects_zero_stale_and_malformed_versions() {
        new_test_ext().execute_with(|| {
            let mut policy = reward_policy();
            policy.config_version = 0;
            assert_noop!(
                EterraEconomy::set_ticket_reward_policy(RuntimeOrigin::root(), 1003, 1, policy),
                Error::<Test>::InvalidConfigVersion
            );

            let mut policy = reward_policy();
            policy.eligible_modes = vec![TicketRewardMode::Ranked, TicketRewardMode::Ranked]
                .try_into()
                .expect("mode bound");
            assert_noop!(
                EterraEconomy::set_ticket_reward_policy(RuntimeOrigin::root(), 1003, 1, policy),
                Error::<Test>::InvalidRewardPolicy
            );
            assert_ok!(EterraEconomy::set_ticket_reward_policy(
                RuntimeOrigin::root(),
                1003,
                1,
                reward_policy(),
            ));
            assert_noop!(
                EterraEconomy::set_ticket_reward_policy(
                    RuntimeOrigin::root(),
                    1003,
                    1,
                    reward_policy(),
                ),
                Error::<Test>::InvalidConfigVersion
            );

            assert_noop!(
                EterraEconomy::upsert_prize_sku(RuntimeOrigin::root(), 77, random_pack_sku(0)),
                Error::<Test>::InvalidConfigVersion
            );
            let mut no_price = random_pack_sku(1);
            no_price.ticket_price = None;
            no_price.native_price = None;
            assert_noop!(
                EterraEconomy::upsert_prize_sku(RuntimeOrigin::root(), 77, no_price),
                Error::<Test>::InvalidAmount
            );
            assert_ok!(EterraEconomy::upsert_prize_sku(
                RuntimeOrigin::root(),
                77,
                random_pack_sku(1),
            ));
            assert_noop!(
                EterraEconomy::upsert_prize_sku(RuntimeOrigin::root(), 77, random_pack_sku(1),),
                Error::<Test>::InvalidConfigVersion
            );

            assert_noop!(
                EterraEconomy::set_featured_rotation_config(
                    RuntimeOrigin::root(),
                    featured_config(0),
                ),
                Error::<Test>::InvalidConfigVersion
            );
            let mut duplicate_pool = featured_config(1);
            duplicate_pool.eligible_subjects = vec![7u32; 12]
                .try_into()
                .expect("duplicate pool is bounded");
            assert_noop!(
                EterraEconomy::set_featured_rotation_config(RuntimeOrigin::root(), duplicate_pool,),
                Error::<Test>::InvalidFeaturedRotationConfig
            );
        });
    }

    #[test]
    fn verified_score_tiers_issue_integer_tickets_once_and_apply_window_cap() {
        new_test_ext().execute_with(|| {
            configure_ticket_rewards();
            let first = H256::repeat_byte(1);
            assert_eq!(
                EterraEconomy::try_grant_gameplay_tickets(&42, 1003, 1, first, 1_500, true, 0)
                    .expect("reward succeeds"),
                40
            );
            assert_eq!(EterraEconomy::ticket_balance(&42), 40);
            assert_eq!(
                EterraEconomy::try_grant_gameplay_tickets(&42, 1003, 1, first, 1_500, true, 0)
                    .expect("duplicate is ignored"),
                0
            );
            assert_eq!(
                EterraEconomy::try_grant_gameplay_tickets(
                    &42,
                    1003,
                    1,
                    H256::repeat_byte(2),
                    1_500,
                    true,
                    0,
                )
                .expect("second reward succeeds"),
                30
            );
            assert_eq!(EterraEconomy::ticket_balance(&42), 70);
            assert_eq!(
                EterraEconomy::try_grant_gameplay_tickets(
                    &42,
                    1003,
                    1,
                    H256::repeat_byte(3),
                    1_500,
                    false,
                    0,
                )
                .expect("unranked result is ineligible"),
                0
            );
        });
    }

    #[test]
    fn earning_enforces_eligibility_and_rolls_windows_forward() {
        new_test_ext().execute_with(|| {
            configure_ticket_rewards();
            let ineligible_cases = [
                (99, true, 0, H256::repeat_byte(10)),
                (42, false, 0, H256::repeat_byte(11)),
                (42, true, 9, H256::repeat_byte(12)),
            ];
            for (account, ranked, ended_reason, result) in ineligible_cases {
                assert_eq!(
                    EterraEconomy::try_grant_gameplay_tickets(
                        &account,
                        1003,
                        1,
                        result,
                        1_500,
                        ranked,
                        ended_reason,
                    )
                    .expect("ineligible results are ignored"),
                    0
                );
                assert!(!TicketRewardedResults::<Test>::get(result));
            }

            assert_ok!(EterraEconomy::set_arcade_account_restriction(
                RuntimeOrigin::root(),
                42,
                true,
            ));
            let restricted = H256::repeat_byte(13);
            assert_eq!(
                EterraEconomy::try_grant_gameplay_tickets(
                    &42,
                    1003,
                    1,
                    restricted,
                    1_500,
                    true,
                    0,
                )
                .expect("restricted result is ignored"),
                0
            );
            assert!(!TicketRewardedResults::<Test>::get(restricted));
            assert_ok!(EterraEconomy::set_arcade_account_restriction(
                RuntimeOrigin::root(),
                42,
                false,
            ));

            System::set_block_number(99);
            assert_eq!(
                EterraEconomy::try_grant_gameplay_tickets(
                    &42,
                    1003,
                    1,
                    H256::repeat_byte(14),
                    1_500,
                    true,
                    0,
                )
                .expect("first window reward"),
                40
            );
            System::set_block_number(100);
            assert_eq!(
                EterraEconomy::try_grant_gameplay_tickets(
                    &42,
                    1003,
                    1,
                    H256::repeat_byte(15),
                    1_500,
                    true,
                    0,
                )
                .expect("next window reward"),
                40
            );
            assert_eq!(TicketEarningWindows::<Test>::get((1003, 1, 42, 0)), 40);
            assert_eq!(TicketEarningWindows::<Test>::get((1003, 1, 42, 1)), 40);
        });
    }

    #[test]
    fn failed_ticket_mint_rolls_back_replay_marker_and_cap_accounting() {
        new_test_ext().execute_with(|| {
            configure_ticket_rewards();
            let result = H256::repeat_byte(20);
            MOCK_MINT_FAILS.with(|fails| *fails.borrow_mut() = true);
            assert_eq!(
                EterraEconomy::try_grant_gameplay_tickets(&42, 1003, 1, result, 1_500, true, 0,),
                Err(TokenError::FundsUnavailable.into())
            );
            assert!(!TicketRewardedResults::<Test>::get(result));
            assert_eq!(TicketEarningWindows::<Test>::get((1003, 1, 42, 0)), 0);
            assert_eq!(EterraEconomy::ticket_balance(&42), 0);

            MOCK_MINT_FAILS.with(|fails| *fails.borrow_mut() = false);
            assert_eq!(
                EterraEconomy::try_grant_gameplay_tickets(&42, 1003, 1, result, 1_500, true, 0,)
                    .expect("retry succeeds"),
                40
            );
            assert!(TicketRewardedResults::<Test>::get(result));
        });
    }

    #[test]
    fn direct_ticket_transfer_uses_asset_balance_without_changing_earned_window() {
        new_test_ext().execute_with(|| {
            configure_ticket_rewards();
            assert_eq!(
                EterraEconomy::try_grant_gameplay_tickets(
                    &42,
                    1003,
                    1,
                    H256::repeat_byte(4),
                    1_500,
                    true,
                    0,
                )
                .expect("reward succeeds"),
                40
            );
            assert_ok!(EterraEconomy::set_arcade_economy_pause(
                RuntimeOrigin::root(),
                PauseDomain::TicketTransfers,
                false,
            ));
            assert_ok!(EterraEconomy::transfer_tickets(
                RuntimeOrigin::signed(42),
                7,
                15,
            ));
            assert_eq!(EterraEconomy::ticket_balance(&42), 25);
            assert_eq!(EterraEconomy::ticket_balance(&7), 15);
            assert_eq!(TicketEarningWindows::<Test>::get((1003, 1, 42, 0)), 40);
            assert_eq!(TicketEarningWindows::<Test>::get((1003, 1, 7, 0)), 0);
        });
    }

    #[test]
    fn ticket_transfers_honor_pause_amount_and_account_controls() {
        new_test_ext().execute_with(|| {
            assert_ok!(EterraEconomy::set_ticket_asset(RuntimeOrigin::root(), 3, 1));
            assert_ok!(MockTicketAssets::mint(3, &42, 100));
            assert_noop!(
                EterraEconomy::transfer_tickets(RuntimeOrigin::signed(42), 7, 1),
                Error::<Test>::SubsystemPaused
            );
            assert_ok!(EterraEconomy::set_arcade_economy_pause(
                RuntimeOrigin::root(),
                PauseDomain::TicketTransfers,
                false,
            ));
            assert_noop!(
                EterraEconomy::transfer_tickets(RuntimeOrigin::signed(42), 7, 0),
                Error::<Test>::InvalidAmount
            );
            assert_noop!(
                EterraEconomy::transfer_tickets(RuntimeOrigin::signed(42), 99, 1),
                Error::<Test>::AccountNotEligible
            );
            assert_ok!(EterraEconomy::set_arcade_account_restriction(
                RuntimeOrigin::root(),
                7,
                true,
            ));
            assert_noop!(
                EterraEconomy::transfer_tickets(RuntimeOrigin::signed(42), 7, 1),
                Error::<Test>::AccountRestricted
            );
            assert_eq!(EterraEconomy::ticket_balance(&42), 100);
            assert_eq!(EterraEconomy::ticket_balance(&7), 0);
        });
    }

    #[test]
    fn arcade_pack_credit_catalog_is_versioned_and_training_only() {
        new_test_ext().execute_with(|| {
            let mut invalid = arcade_pack_credit_sku_v2(1);
            invalid.economic_realm = EconomicRealm::Production;
            assert_noop!(
                EterraEconomy::upsert_arcade_pack_credit_sku_v2(
                    RuntimeOrigin::root(),
                    7001,
                    invalid,
                ),
                Error::<Test>::ArcadePackCreditProductionDisabled
            );

            let mut invalid = arcade_pack_credit_sku_v2(1);
            invalid.policy_version = 0;
            assert_noop!(
                EterraEconomy::upsert_arcade_pack_credit_sku_v2(
                    RuntimeOrigin::root(),
                    7001,
                    invalid,
                ),
                Error::<Test>::InvalidArcadePackCreditSku
            );

            let mut missing_target = arcade_pack_credit_sku_v2(1);
            missing_target.pack_sku = 2;
            assert_eq!(
                EterraEconomy::upsert_arcade_pack_credit_sku_v2(
                    RuntimeOrigin::root(),
                    7001,
                    missing_target,
                ),
                Err(DispatchError::Other("mock pack target missing"))
            );

            assert_ok!(EterraEconomy::upsert_arcade_pack_credit_sku_v2(
                RuntimeOrigin::root(),
                7001,
                arcade_pack_credit_sku_v2(1),
            ));
            assert_noop!(
                EterraEconomy::upsert_arcade_pack_credit_sku_v2(
                    RuntimeOrigin::root(),
                    7001,
                    arcade_pack_credit_sku_v2(1),
                ),
                Error::<Test>::InvalidConfigVersion
            );
            let mut next = arcade_pack_credit_sku_v2(2);
            next.enabled = false;
            assert_ok!(EterraEconomy::upsert_arcade_pack_credit_sku_v2(
                RuntimeOrigin::root(),
                7001,
                next,
            ));
            assert_eq!(
                EterraEconomy::arcade_pack_credit_sku_v2(7001)
                    .expect("V2 arcade SKU exists")
                    .config_version,
                2
            );
        });
    }

    #[test]
    fn arcade_prize_redemption_issues_exact_pack_credit_and_burns_tickets() {
        new_test_ext().execute_with(|| {
            assert_ok!(EterraEconomy::set_ticket_asset(RuntimeOrigin::root(), 3, 1));
            assert_ok!(EterraEconomy::upsert_arcade_pack_credit_sku_v2(
                RuntimeOrigin::root(),
                7001,
                arcade_pack_credit_sku_v2(1),
            ));
            assert_ok!(EterraEconomy::set_arcade_economy_pause(
                RuntimeOrigin::root(),
                PauseDomain::PackCreditRedemptionV2,
                false,
            ));
            assert_ok!(MockTicketAssets::mint(3, &42, 100));
            let redemption_id = [0xA5; 32];

            assert_ok!(EterraEconomy::redeem_arcade_pack_credit_with_tickets_v2(
                RuntimeOrigin::signed(42),
                7001,
                1,
                redemption_id,
            ));

            assert_eq!(EterraEconomy::ticket_balance(&42), 80);
            assert_eq!(ArcadePackCreditSkuSoldV2::<Test>::get(7001), 1);
            assert_eq!(
                ArcadePackCreditSkuAccountWindowsV2::<Test>::get((7001, 42, 0)),
                1
            );
            assert_eq!(
                mock_issued_pack_credits(),
                vec![(
                    42,
                    1,
                    1,
                    EconomicRealm::Training,
                    PackCreditSource::ArcadePrize {
                        policy_version: 7,
                        redemption_id,
                    },
                )]
            );
            assert_eq!(
                EterraEconomy::arcade_pack_credit_redemption_receipt_v2(redemption_id),
                Some(ArcadePackCreditRedemptionReceiptV2::<Test> {
                    account: 42,
                    sku_id: 7001,
                    sku_config_version: 1,
                    pack_sku: 1,
                    pack_sku_version: 1,
                    economic_realm: EconomicRealm::Training,
                    ticket_amount: 20,
                    policy_version: 7,
                })
            );
            System::assert_last_event(RuntimeEvent::EterraEconomy(
                Event::ArcadePackCreditRedeemedV2 {
                    account: 42,
                    sku_id: 7001,
                    redemption_id,
                    pack_sku: 1,
                    pack_sku_version: 1,
                    economic_realm: EconomicRealm::Training,
                    ticket_amount: 20,
                    policy_version: 7,
                    config_version: 1,
                },
            ));
        });
    }

    #[test]
    fn arcade_prize_redemption_id_is_globally_idempotent_with_exact_conflicts() {
        new_test_ext().execute_with(|| {
            assert_ok!(EterraEconomy::set_ticket_asset(RuntimeOrigin::root(), 3, 1));
            assert_ok!(EterraEconomy::upsert_arcade_pack_credit_sku_v2(
                RuntimeOrigin::root(),
                7001,
                arcade_pack_credit_sku_v2(1),
            ));
            assert_ok!(EterraEconomy::set_arcade_economy_pause(
                RuntimeOrigin::root(),
                PauseDomain::PackCreditRedemptionV2,
                false,
            ));
            assert_ok!(MockTicketAssets::mint(3, &42, 20));
            let redemption_id = [0xB6; 32];
            assert_ok!(EterraEconomy::redeem_arcade_pack_credit_with_tickets_v2(
                RuntimeOrigin::signed(42),
                7001,
                1,
                redemption_id,
            ));

            // An exact retry is a no-op even if mutable gates and catalog
            // state have changed since the finalized redemption.
            PausedDomains::<Test>::insert(PauseDomain::PackCreditRedemptionV2, true);
            RestrictedAccounts::<Test>::insert(42, true);
            TicketAsset::<Test>::kill();
            ArcadePackCreditSkusV2::<Test>::remove(7001);
            assert_ok!(EterraEconomy::redeem_arcade_pack_credit_with_tickets_v2(
                RuntimeOrigin::signed(42),
                7001,
                1,
                redemption_id,
            ));
            assert_eq!(mock_issued_pack_credits().len(), 1);
            assert_eq!(ArcadePackCreditSkuSoldV2::<Test>::get(7001), 1);

            assert_noop!(
                EterraEconomy::redeem_arcade_pack_credit_with_tickets_v2(
                    RuntimeOrigin::signed(7),
                    7001,
                    1,
                    redemption_id,
                ),
                Error::<Test>::ArcadePackCreditRedemptionConflict
            );
            assert_noop!(
                EterraEconomy::redeem_arcade_pack_credit_with_tickets_v2(
                    RuntimeOrigin::signed(42),
                    7002,
                    1,
                    redemption_id,
                ),
                Error::<Test>::ArcadePackCreditRedemptionConflict
            );
            assert_noop!(
                EterraEconomy::redeem_arcade_pack_credit_with_tickets_v2(
                    RuntimeOrigin::signed(42),
                    7001,
                    2,
                    redemption_id,
                ),
                Error::<Test>::ArcadePackCreditRedemptionConflict
            );
        });
    }

    #[test]
    fn arcade_prize_redemption_failures_roll_back_all_chain_state() {
        new_test_ext().execute_with(|| {
            assert_ok!(EterraEconomy::set_ticket_asset(RuntimeOrigin::root(), 3, 1));
            assert_ok!(EterraEconomy::upsert_arcade_pack_credit_sku_v2(
                RuntimeOrigin::root(),
                7001,
                arcade_pack_credit_sku_v2(1),
            ));
            assert_ok!(EterraEconomy::set_arcade_economy_pause(
                RuntimeOrigin::root(),
                PauseDomain::PackCreditRedemptionV2,
                false,
            ));
            assert_ok!(MockTicketAssets::mint(3, &42, 20));

            let issuance_failure_id = [0xC7; 32];
            MOCK_PACK_CREDIT_ISSUANCE_FAILS.with(|fails| *fails.borrow_mut() = true);
            assert_eq!(
                EterraEconomy::redeem_arcade_pack_credit_with_tickets_v2(
                    RuntimeOrigin::signed(42),
                    7001,
                    1,
                    issuance_failure_id,
                ),
                Err(DispatchError::Other("mock pack credit issuance failed"))
            );
            assert_eq!(EterraEconomy::ticket_balance(&42), 20);
            assert!(mock_issued_pack_credits().is_empty());
            assert!(!ArcadePackCreditRedemptionReceiptsV2::<Test>::contains_key(
                issuance_failure_id
            ));

            // Issuance happens before the asset burn, so this also proves the
            // cross-pallet write is rolled back when payment fails.
            MOCK_PACK_CREDIT_ISSUANCE_FAILS.with(|fails| *fails.borrow_mut() = false);
            MOCK_TICKET_BALANCES.with(|balances| balances.borrow_mut().insert(42, 0));
            let payment_failure_id = [0xD8; 32];
            assert_eq!(
                EterraEconomy::redeem_arcade_pack_credit_with_tickets_v2(
                    RuntimeOrigin::signed(42),
                    7001,
                    1,
                    payment_failure_id,
                ),
                Err(TokenError::FundsUnavailable.into())
            );
            assert!(mock_issued_pack_credits().is_empty());
            assert_eq!(ArcadePackCreditSkuSoldV2::<Test>::get(7001), 0);
            assert_eq!(
                ArcadePackCreditSkuAccountWindowsV2::<Test>::get((7001, 42, 0)),
                0
            );
            assert!(!ArcadePackCreditRedemptionReceiptsV2::<Test>::contains_key(
                payment_failure_id
            ));

            ArcadePackCreditSkusV2::<Test>::mutate(7001, |maybe_sku| {
                maybe_sku.as_mut().expect("SKU exists").total_cap = None;
            });
            ArcadePackCreditSkuSoldV2::<Test>::insert(7001, u64::MAX);
            assert_ok!(MockTicketAssets::mint(3, &42, 20));
            let overflow_id = [0xE9; 32];
            assert_noop!(
                EterraEconomy::redeem_arcade_pack_credit_with_tickets_v2(
                    RuntimeOrigin::signed(42),
                    7001,
                    1,
                    overflow_id,
                ),
                Error::<Test>::ArithmeticOverflow
            );
            assert_eq!(EterraEconomy::ticket_balance(&42), 20);
            assert!(mock_issued_pack_credits().is_empty());
            assert!(!ArcadePackCreditRedemptionReceiptsV2::<Test>::contains_key(
                overflow_id
            ));
        });
    }

    #[test]
    fn arcade_prize_redemption_rejects_zero_ids_pauses_access_and_caps() {
        new_test_ext().execute_with(|| {
            assert_ok!(EterraEconomy::set_ticket_asset(RuntimeOrigin::root(), 3, 1));
            let mut sku = arcade_pack_credit_sku_v2(1);
            sku.total_cap = Some(2);
            sku.per_account_window_cap = 1;
            assert_ok!(EterraEconomy::upsert_arcade_pack_credit_sku_v2(
                RuntimeOrigin::root(),
                7001,
                sku,
            ));
            assert_ok!(MockTicketAssets::mint(3, &42, 60));
            assert_ok!(MockTicketAssets::mint(3, &7, 20));

            assert_noop!(
                EterraEconomy::redeem_arcade_pack_credit_with_tickets_v2(
                    RuntimeOrigin::signed(42),
                    7001,
                    1,
                    [0u8; 32],
                ),
                Error::<Test>::InvalidArcadePackCreditRedemptionId
            );
            assert_ok!(EterraEconomy::set_arcade_economy_pause(
                RuntimeOrigin::root(),
                PauseDomain::TicketRedemption,
                false,
            ));
            assert_noop!(
                EterraEconomy::redeem_arcade_pack_credit_with_tickets_v2(
                    RuntimeOrigin::signed(42),
                    7001,
                    1,
                    [1u8; 32],
                ),
                Error::<Test>::SubsystemPaused
            );
            assert_ok!(EterraEconomy::set_arcade_economy_pause(
                RuntimeOrigin::root(),
                PauseDomain::PackCreditRedemptionV2,
                false,
            ));
            assert_noop!(
                EterraEconomy::redeem_arcade_pack_credit_with_tickets_v2(
                    RuntimeOrigin::signed(99),
                    7001,
                    1,
                    [2u8; 32],
                ),
                Error::<Test>::AccountNotEligible
            );
            assert_ok!(EterraEconomy::redeem_arcade_pack_credit_with_tickets_v2(
                RuntimeOrigin::signed(42),
                7001,
                1,
                [3u8; 32],
            ));
            assert_noop!(
                EterraEconomy::redeem_arcade_pack_credit_with_tickets_v2(
                    RuntimeOrigin::signed(42),
                    7001,
                    1,
                    [4u8; 32],
                ),
                Error::<Test>::PrizeAccountLimitReached
            );
            assert_ok!(EterraEconomy::redeem_arcade_pack_credit_with_tickets_v2(
                RuntimeOrigin::signed(7),
                7001,
                1,
                [5u8; 32],
            ));
            assert_noop!(
                EterraEconomy::redeem_arcade_pack_credit_with_tickets_v2(
                    RuntimeOrigin::signed(8),
                    7001,
                    1,
                    [6u8; 32],
                ),
                Error::<Test>::PrizeSoldOut
            );
        });
    }

    #[test]
    fn arcade_pack_credit_bridge_only_appends_scale_discriminants() {
        new_test_ext().execute_with(|| {
            assert_eq!(
                Call::<Test>::redeem_prize_with_tickets {
                    sku_id: 1,
                    expected_version: 1,
                }
                .encode()[0],
                17
            );
            assert_eq!(
                Call::<Test>::purchase_prize_with_native {
                    target: PurchaseTarget::CatalogSku(1),
                    expected_version: 1,
                }
                .encode()[0],
                18
            );
            assert_eq!(
                Call::<Test>::upsert_arcade_pack_credit_sku_v2 {
                    sku_id: 1,
                    sku: arcade_pack_credit_sku_v2(1),
                }
                .encode()[0],
                19
            );
            assert_eq!(
                Call::<Test>::redeem_arcade_pack_credit_with_tickets_v2 {
                    sku_id: 1,
                    expected_version: 1,
                    redemption_id: [1u8; 32],
                }
                .encode()[0],
                20
            );
            assert_eq!(
                Event::<Test>::PrizePurchased {
                    account: 42,
                    target: PurchaseTarget::CatalogSku(1),
                    native_amount: 1,
                    card_ids: vec![1].try_into().expect("bounded card ids"),
                    config_version: 1,
                }
                .encode()[0],
                21
            );
            assert_eq!(
                Event::<Test>::ArcadePackCreditSkuUpdatedV2 {
                    sku_id: 1,
                    pack_sku: 1,
                    pack_sku_version: 1,
                    economic_realm: EconomicRealm::Training,
                    policy_version: 1,
                    config_version: 1,
                    enabled: true,
                }
                .encode()[0],
                22
            );
            assert_eq!(Error::<Test>::PrizeFulfillmentFailed.encode()[0], 27);
            assert_eq!(
                Error::<Test>::ArcadePackCreditRedemptionConflict.encode()[0],
                33
            );
            assert_eq!(
                PackCreditSource::ArcadePrize {
                    policy_version: 1,
                    redemption_id: [1u8; 32],
                }
                .encode()[0],
                3
            );
            assert_eq!(PauseDomain::TicketRedemption.encode()[0], 2);
            assert_eq!(PauseDomain::PackCreditRedemptionV2.encode()[0], 5);
        });
    }

    #[test]
    fn ticket_and_native_catalog_payments_are_atomic_and_capped() {
        new_test_ext().execute_with(|| {
            configure_ticket_rewards();
            assert_ok!(MockTicketAssets::mint(3, &42, 100));
            let sku = PrizeSku::<Test> {
                kind: PrizeKind::RandomPack,
                pool_id: 9,
                ticket_price: Some(20),
                native_price: Some(500),
                enabled: true,
                total_cap: Some(2),
                per_account_window_cap: 2,
                window_blocks: 100,
                config_version: 1,
            };
            assert_ok!(EterraEconomy::upsert_prize_sku(
                RuntimeOrigin::root(),
                77,
                sku
            ));
            assert_ok!(EterraEconomy::set_arcade_economy_pause(
                RuntimeOrigin::root(),
                PauseDomain::TicketRedemption,
                false,
            ));
            assert_ok!(EterraEconomy::set_arcade_economy_pause(
                RuntimeOrigin::root(),
                PauseDomain::RandomVending,
                false,
            ));
            assert_ok!(EterraEconomy::redeem_prize_with_tickets(
                RuntimeOrigin::signed(42),
                77,
                1,
            ));
            assert_eq!(EterraEconomy::ticket_balance(&42), 80);
            assert_ok!(EterraEconomy::purchase_prize_with_native(
                RuntimeOrigin::signed(42),
                PurchaseTarget::CatalogSku(77),
                1,
            ));
            MOCK_NATIVE_PAYMENTS.with(|payments| {
                assert_eq!(payments.borrow().as_slice(), &[(42, 500)]);
            });
            assert_noop!(
                EterraEconomy::redeem_prize_with_tickets(RuntimeOrigin::signed(42), 77, 1),
                Error::<Test>::PrizeSoldOut
            );
        });
    }

    #[test]
    fn failed_catalog_fulfillment_and_payment_leave_supply_and_limits_untouched() {
        new_test_ext().execute_with(|| {
            assert_ok!(EterraEconomy::set_ticket_asset(RuntimeOrigin::root(), 3, 1));
            assert_ok!(EterraEconomy::upsert_prize_sku(
                RuntimeOrigin::root(),
                77,
                random_pack_sku(1),
            ));
            for domain in [PauseDomain::TicketRedemption, PauseDomain::RandomVending] {
                assert_ok!(EterraEconomy::set_arcade_economy_pause(
                    RuntimeOrigin::root(),
                    domain,
                    false,
                ));
            }

            MOCK_FULFILLMENT_FAILS.with(|fails| *fails.borrow_mut() = true);
            assert_noop!(
                EterraEconomy::redeem_prize_with_tickets(RuntimeOrigin::signed(42), 77, 1),
                Error::<Test>::PrizeFulfillmentFailed
            );
            assert_eq!(PrizeSkuSold::<Test>::get(77), 0);
            assert_eq!(PrizeSkuAccountWindows::<Test>::get((77, 42, 0)), 0);

            MOCK_FULFILLMENT_FAILS.with(|fails| *fails.borrow_mut() = false);
            assert_eq!(
                EterraEconomy::redeem_prize_with_tickets(RuntimeOrigin::signed(42), 77, 1),
                Err(TokenError::FundsUnavailable.into())
            );
            assert_eq!(PrizeSkuSold::<Test>::get(77), 0);

            MOCK_NATIVE_PAYMENT_FAILS.with(|fails| *fails.borrow_mut() = true);
            assert_eq!(
                EterraEconomy::purchase_prize_with_native(
                    RuntimeOrigin::signed(42),
                    PurchaseTarget::CatalogSku(77),
                    1,
                ),
                Err(TokenError::FundsUnavailable.into())
            );
            assert_eq!(PrizeSkuSold::<Test>::get(77), 0);
            assert_eq!(PrizeSkuAccountWindows::<Test>::get((77, 42, 0)), 0);
            MOCK_NATIVE_PAYMENTS.with(|payments| assert!(payments.borrow().is_empty()));
        });
    }

    #[test]
    fn catalog_rejects_stale_disabled_unsupported_and_over_limit_purchases() {
        new_test_ext().execute_with(|| {
            assert_ok!(EterraEconomy::set_ticket_asset(RuntimeOrigin::root(), 3, 1));
            let mut sku = random_pack_sku(1);
            sku.native_price = None;
            sku.per_account_window_cap = 1;
            assert_ok!(EterraEconomy::upsert_prize_sku(
                RuntimeOrigin::root(),
                77,
                sku,
            ));
            assert_ok!(EterraEconomy::set_arcade_economy_pause(
                RuntimeOrigin::root(),
                PauseDomain::TicketRedemption,
                false,
            ));
            assert_ok!(EterraEconomy::set_arcade_economy_pause(
                RuntimeOrigin::root(),
                PauseDomain::RandomVending,
                false,
            ));
            assert_noop!(
                EterraEconomy::redeem_prize_with_tickets(RuntimeOrigin::signed(42), 77, 2),
                Error::<Test>::InvalidConfigVersion
            );
            assert_noop!(
                EterraEconomy::purchase_prize_with_native(
                    RuntimeOrigin::signed(42),
                    PurchaseTarget::CatalogSku(77),
                    1,
                ),
                Error::<Test>::PrizePaymentNotSupported
            );
            assert_ok!(MockTicketAssets::mint(3, &42, 40));
            assert_ok!(EterraEconomy::redeem_prize_with_tickets(
                RuntimeOrigin::signed(42),
                77,
                1,
            ));
            assert_noop!(
                EterraEconomy::redeem_prize_with_tickets(RuntimeOrigin::signed(42), 77, 1),
                Error::<Test>::PrizeAccountLimitReached
            );

            let mut disabled = random_pack_sku(2);
            disabled.enabled = false;
            assert_ok!(EterraEconomy::upsert_prize_sku(
                RuntimeOrigin::root(),
                77,
                disabled,
            ));
            assert_noop!(
                EterraEconomy::redeem_prize_with_tickets(RuntimeOrigin::signed(7), 77, 2),
                Error::<Test>::PrizeSkuDisabled
            );
        });
    }

    #[test]
    fn featured_rotation_has_twelve_unique_subjects_and_rejects_stale_epoch() {
        new_test_ext().execute_with(|| {
            let config = FeaturedRotationConfig::<Test> {
                enabled: true,
                pool_id: 5,
                eligible_subjects: (1u32..=20)
                    .collect::<Vec<_>>()
                    .try_into()
                    .expect("pool bound"),
                period_blocks: 5,
                native_price: 250,
                per_slot_cap: 3,
                per_account_limit: 1,
                config_version: 1,
            };
            assert_ok!(EterraEconomy::set_featured_rotation_config(
                RuntimeOrigin::root(),
                config
            ));
            System::set_block_number(2);
            EterraEconomy::on_initialize(2);
            let first = CurrentFeaturedRotation::<Test>::get().expect("rotation exists");
            assert_eq!(first.offers.len(), 12);
            let mut subjects = first
                .offers
                .iter()
                .map(|offer| offer.subject_id)
                .collect::<Vec<_>>();
            subjects.sort_unstable();
            subjects.dedup();
            assert_eq!(subjects.len(), 12);
            assert_eq!(first.pool_id, 5);
            assert_eq!(first.per_account_limit, 1);
            assert_eq!(first.offers[0].native_price, 250);
            assert_ok!(EterraEconomy::set_featured_rotation_config(
                RuntimeOrigin::root(),
                FeaturedRotationConfig::<Test> {
                    enabled: true,
                    pool_id: 6,
                    eligible_subjects: (21u32..=40)
                        .collect::<Vec<_>>()
                        .try_into()
                        .expect("next pool bound"),
                    period_blocks: 7,
                    native_price: 300,
                    per_slot_cap: 4,
                    per_account_limit: 2,
                    config_version: 2,
                }
            ));
            assert_ok!(EterraEconomy::set_arcade_economy_pause(
                RuntimeOrigin::root(),
                PauseDomain::FeaturedVending,
                false,
            ));
            assert_ok!(EterraEconomy::purchase_prize_with_native(
                RuntimeOrigin::signed(42),
                PurchaseTarget::FeaturedSlot {
                    rotation_id: first.rotation_id,
                    slot: 0
                },
                1,
            ));
            System::set_block_number(first.ends_at);
            EterraEconomy::on_initialize(first.ends_at);
            let second = CurrentFeaturedRotation::<Test>::get().expect("next rotation exists");
            assert_eq!(second.config_version, 2);
            assert_eq!(second.pool_id, 6);
            assert_eq!(second.per_account_limit, 2);
            assert_eq!(second.offers[0].native_price, 300);
            assert_noop!(
                EterraEconomy::purchase_prize_with_native(
                    RuntimeOrigin::signed(7),
                    PurchaseTarget::FeaturedSlot {
                        rotation_id: first.rotation_id,
                        slot: 0
                    },
                    1,
                ),
                Error::<Test>::StaleRotation
            );
        });
    }

    #[test]
    fn featured_vending_enforces_slot_stock_account_and_version_limits() {
        new_test_ext().execute_with(|| {
            assert_ok!(EterraEconomy::set_featured_rotation_config(
                RuntimeOrigin::root(),
                featured_config(1),
            ));
            System::set_block_number(2);
            EterraEconomy::on_initialize(2);
            let rotation = CurrentFeaturedRotation::<Test>::get().expect("rotation exists");
            assert_ok!(EterraEconomy::set_arcade_economy_pause(
                RuntimeOrigin::root(),
                PauseDomain::FeaturedVending,
                false,
            ));
            assert_noop!(
                EterraEconomy::purchase_prize_with_native(
                    RuntimeOrigin::signed(42),
                    PurchaseTarget::FeaturedSlot {
                        rotation_id: rotation.rotation_id,
                        slot: 12,
                    },
                    1,
                ),
                Error::<Test>::FeaturedSlotNotFound
            );
            assert_noop!(
                EterraEconomy::purchase_prize_with_native(
                    RuntimeOrigin::signed(42),
                    PurchaseTarget::FeaturedSlot {
                        rotation_id: rotation.rotation_id,
                        slot: 0,
                    },
                    2,
                ),
                Error::<Test>::InvalidConfigVersion
            );
            assert_ok!(EterraEconomy::purchase_prize_with_native(
                RuntimeOrigin::signed(42),
                PurchaseTarget::FeaturedSlot {
                    rotation_id: rotation.rotation_id,
                    slot: 0,
                },
                1,
            ));
            assert_noop!(
                EterraEconomy::purchase_prize_with_native(
                    RuntimeOrigin::signed(42),
                    PurchaseTarget::FeaturedSlot {
                        rotation_id: rotation.rotation_id,
                        slot: 0,
                    },
                    1,
                ),
                Error::<Test>::PrizeAccountLimitReached
            );
            assert_ok!(EterraEconomy::purchase_prize_with_native(
                RuntimeOrigin::signed(7),
                PurchaseTarget::FeaturedSlot {
                    rotation_id: rotation.rotation_id,
                    slot: 0,
                },
                1,
            ));
            assert_noop!(
                EterraEconomy::purchase_prize_with_native(
                    RuntimeOrigin::signed(8),
                    PurchaseTarget::FeaturedSlot {
                        rotation_id: rotation.rotation_id,
                        slot: 0,
                    },
                    1,
                ),
                Error::<Test>::PrizeSoldOut
            );
            MOCK_NATIVE_PAYMENTS.with(|payments| {
                assert_eq!(payments.borrow().as_slice(), &[(42, 250), (7, 250)]);
            });
        });
    }

    #[test]
    fn failed_rotation_pauses_featured_sales_and_keeps_prior_roster() {
        new_test_ext().execute_with(|| {
            assert_ok!(EterraEconomy::set_featured_rotation_config(
                RuntimeOrigin::root(),
                featured_config(1),
            ));
            System::set_block_number(2);
            EterraEconomy::on_initialize(2);
            let prior = CurrentFeaturedRotation::<Test>::get().expect("rotation exists");
            assert_ok!(EterraEconomy::set_arcade_economy_pause(
                RuntimeOrigin::root(),
                PauseDomain::FeaturedVending,
                false,
            ));

            let mut invalid = featured_config(2);
            invalid.eligible_subjects = vec![1u32; 12].try_into().expect("invalid pool is bounded");
            FeaturedRotationSettings::<Test>::put(invalid);
            System::set_block_number(prior.ends_at);
            EterraEconomy::on_initialize(prior.ends_at);

            assert_eq!(CurrentFeaturedRotation::<Test>::get(), Some(prior));
            assert!(PausedDomains::<Test>::get(PauseDomain::FeaturedVending));
            assert_eq!(NextFeaturedRotationId::<Test>::get(), 1);
            assert!(matches!(
                System::events().last().map(|record| &record.event),
                Some(RuntimeEvent::EterraEconomy(
                    Event::FeaturedRotationFailed { .. }
                ))
            ));
        });
    }
}
