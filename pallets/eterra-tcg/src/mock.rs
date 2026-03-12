use crate as pallet_eterra_slots;
use frame_support::{
    dispatch::DispatchResult,
    parameter_types,
    traits::{ConstU128, ConstU32, ConstU64, ConstU8, Everything},
    BoundedVec,
};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage,
};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
    pub struct Test {
        System: frame_system,
        Balances: pallet_balances,
        EterraMedia: pallet_eterra_media,
        EterraSeasons: pallet_eterra_seasons,
        Nfts: pallet_nfts,
        EterraSlots: pallet_eterra_slots,
    }
);

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const ExistentialDeposit: u128 = 1;
    pub const MaxAttempts: u8 = 3;
    pub const CardsPerPack: u8 = 6;
    pub const MaxOwnedCards: u32 = 5_000;
    pub const BaseCardCapacity: u32 = 500;
    pub const CardCapacityUpgradeAmount: u32 = 100;
    pub const CardCapacityUpgradePrice: u128 = 100;
    pub const CardCapacityUpgradePriceReceiver: u64 = 999;
    pub const PackPrice: u128 = 500;
    pub const PackPriceReceiver: u64 = 999;
    pub const ProPrice: u128 = 200;
    pub const ProPriceReceiver: u64 = 999;
    pub const MintCardPrice: u128 = 100;
    pub const MintCardPriceReceiver: u64 = 999;
    pub const MaxProSpins: u8 = 5;
    pub const MaxBorders: u32 = 32;
    pub const MaxBackgrounds: u32 = 32;
    pub const MaxSubjects: u32 = 128;
    pub const MaxBacks: u32 = 32;
    pub const MaxPackagingFronts: u32 = 16;
    pub const MaxPackagingBacks: u32 = 16;
    pub const MaxSeasonCollections: u32 = 32;
    pub const MaxSeasonCollectionNameLen: u32 = 64;

    pub const MaxMediaUriLen: u32 = 256;
    pub const MaxMediaContentTypeLen: u32 = 64;
    pub const MaxMediaNameLen: u32 = 64;
    pub const MaxMediaDescriptionLen: u32 = 256;
    pub const MaxMediaRolesPerAccount: u32 = 8;
    pub const DefaultMediaCollectionId: u32 = 0;

    pub const MaxSeasonNameLen: u32 = 64;
    pub const MaxSeasonDescLen: u32 = 256;
}

pub struct TcgSeasonActivationValidator;

impl pallet_eterra_seasons::SeasonActivationValidator<u32> for TcgSeasonActivationValidator {
    fn ensure_can_activate(season_id: u32) -> DispatchResult {
        pallet_eterra_slots::Pallet::<Test>::ensure_season_ready_for_activation(season_id)
    }
}

impl system::Config for Test {
    type BaseCallFilter = Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type RuntimeEvent = RuntimeEvent;
    type Block = Block;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<u128>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();

    type Nonce = u64;
    type RuntimeTask = ();
    type MaxConsumers = ConstU32<16>;
    type SingleBlockMigrations = ();
    type MultiBlockMigrator = ();
    type PreInherents = ();
    type PostInherents = ();
    type PostTransactions = ();
    type BlockHashCount = ConstU64<250>;
}

impl pallet_balances::Config for Test {
    type Balance = u128;
    type DustRemoval = ();
    type RuntimeEvent = RuntimeEvent;
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = ();
    type MaxLocks = ConstU32<50>;
    type MaxReserves = ConstU32<0>;
    type ReserveIdentifier = [u8; 8];
    type FreezeIdentifier = [u8; 8];
    type RuntimeHoldReason = ();
    type RuntimeFreezeReason = ();
    type MaxFreezes = ConstU32<0>;
}

parameter_types! {
    pub NftsFeatures: pallet_nfts::PalletFeatures = pallet_nfts::PalletFeatures::all_enabled();
}

impl pallet_nfts::Config for Test {
    type RuntimeEvent = RuntimeEvent;

    type CollectionId = u32;
    type ItemId = u32;

    type Currency = Balances;
    type ForceOrigin = frame_system::EnsureRoot<u64>;
    type CreateOrigin =
        frame_support::traits::AsEnsureOriginWithArg<frame_system::EnsureSigned<u64>>;
    type Locker = ();

    type CollectionDeposit = ConstU128<0>;
    type ItemDeposit = ConstU128<0>;
    type MetadataDepositBase = ConstU128<0>;
    type AttributeDepositBase = ConstU128<0>;
    type DepositPerByte = ConstU128<0>;

    type StringLimit = ConstU32<256>;
    type KeyLimit = ConstU32<64>;
    type ValueLimit = ConstU32<256>;

    type ApprovalsLimit = ConstU32<20>;
    type ItemAttributesApprovalsLimit = ConstU32<20>;
    type MaxTips = ConstU32<10>;
    type MaxDeadlineDuration = ConstU64<100_000>;
    type MaxAttributesPerCall = ConstU32<10>;

    type Features = NftsFeatures;

    type OffchainSignature = sp_runtime::testing::TestSignature;
    type OffchainPublic = sp_runtime::testing::UintAuthorityId;

    #[cfg(feature = "runtime-benchmarks")]
    type Helper = ();

    type WeightInfo = ();
}

impl pallet_eterra_seasons::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type AdminOrigin = frame_system::EnsureRoot<u64>;
    type MaxSeasonNameLen = MaxSeasonNameLen;
    type MaxSeasonDescLen = MaxSeasonDescLen;
    type SeasonActivationValidator = TcgSeasonActivationValidator;
    type WeightInfo = ();
}

impl pallet_eterra_media::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxUriLen = MaxMediaUriLen;
    type MaxContentTypeLen = MaxMediaContentTypeLen;
    type MaxNameLen = MaxMediaNameLen;
    type MaxDescriptionLen = MaxMediaDescriptionLen;
    type MaxRolesPerAccount = MaxMediaRolesPerAccount;
    type DefaultCollectionId = DefaultMediaCollectionId;
    type DefaultCollectionOwner = MintCardPriceReceiver;
    type WeightInfo = ();
}

impl pallet_eterra_slots::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type PaymentCurrency = Balances;
    type HandChecker = ();
    type PackPrice = PackPrice;
    type PackPriceReceiver = PackPriceReceiver;
    type ProPrice = ProPrice;
    type ProPriceReceiver = ProPriceReceiver;
    type MintCardPrice = MintCardPrice;
    type MintCardPriceReceiver = MintCardPriceReceiver;
    type MaxProSpins = MaxProSpins;
    type MaxAttempts = ConstU8<3>;
    type CardsPerPack = ConstU8<6>;
    type MaxOwnedCards = MaxOwnedCards;
    type BaseCardCapacity = BaseCardCapacity;
    type CardCapacityUpgradeAmount = CardCapacityUpgradeAmount;
    type CardCapacityUpgradePrice = CardCapacityUpgradePrice;
    type CardCapacityUpgradePriceReceiver = CardCapacityUpgradePriceReceiver;
    type MaxBorders = MaxBorders;
    type MaxBackgrounds = MaxBackgrounds;
    type MaxSubjects = MaxSubjects;
    type MaxBacks = MaxBacks;
    type MaxPackagingFronts = MaxPackagingFronts;
    type MaxPackagingBacks = MaxPackagingBacks;
    type MaxSeasonCollections = MaxSeasonCollections;
    type MaxSeasonCollectionNameLen = MaxSeasonCollectionNameLen;

    type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut storage = system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("system genesis builds");

    // Fund common test accounts so they can mint packs (and pay transaction fees if enabled).
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(1, 1_000_000), (2, 1_000_000), (3, 1_000_000)],
    }
    .assimilate_storage(&mut storage)
    .expect("balances genesis assimilates");

    pallet_eterra_seasons::GenesisConfig::<Test> {
        admins: vec![1],
        initial_draft_season: None,
        initial_active_season: None,
    }
    .assimilate_storage(&mut storage)
    .expect("seasons genesis assimilates");

    pallet_eterra_media::GenesisConfig::<Test> {
        create_default_collection: true,
        default_collection_name: b"Default".to_vec(),
        default_collection_description: b"Default".to_vec(),
        default_collection_owner: Some(1),
    }
    .assimilate_storage(&mut storage)
    .expect("media genesis assimilates");

    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1);

        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S1".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D1".to_vec().try_into().unwrap();
        pallet_eterra_seasons::Pallet::<Test>::create_season(RuntimeOrigin::signed(1), name, desc)
            .expect("create season");

        let uri: BoundedVec<u8, MaxMediaUriLen> = b"ipfs://b".to_vec().try_into().unwrap();
        let ct: BoundedVec<u8, MaxMediaContentTypeLen> = b"image/png".to_vec().try_into().unwrap();
        pallet_eterra_media::Pallet::<Test>::register_media(
            RuntimeOrigin::signed(1),
            None,
            uri.clone(),
            ct.clone(),
            pallet_eterra_media::MediaClass::CoreAsset,
            pallet_eterra_media::Delivery::RemoteIpfs,
            None,
        )
        .expect("register border");
        pallet_eterra_media::Pallet::<Test>::register_media(
            RuntimeOrigin::signed(1),
            None,
            uri.clone(),
            ct.clone(),
            pallet_eterra_media::MediaClass::CoreAsset,
            pallet_eterra_media::Delivery::RemoteIpfs,
            None,
        )
        .expect("register background");
        pallet_eterra_media::Pallet::<Test>::register_media(
            RuntimeOrigin::signed(1),
            None,
            uri.clone(),
            ct.clone(),
            pallet_eterra_media::MediaClass::CoreAsset,
            pallet_eterra_media::Delivery::RemoteIpfs,
            None,
        )
        .expect("register subject");
        pallet_eterra_media::Pallet::<Test>::register_media(
            RuntimeOrigin::signed(1),
            None,
            uri,
            ct,
            pallet_eterra_media::MediaClass::CoreAsset,
            pallet_eterra_media::Delivery::RemoteIpfs,
            None,
        )
        .expect("register back");
        let uri: BoundedVec<u8, MaxMediaUriLen> = b"ipfs://pf".to_vec().try_into().unwrap();
        let ct: BoundedVec<u8, MaxMediaContentTypeLen> = b"image/png".to_vec().try_into().unwrap();
        pallet_eterra_media::Pallet::<Test>::register_media(
            RuntimeOrigin::signed(1),
            None,
            uri.clone(),
            ct.clone(),
            pallet_eterra_media::MediaClass::CoreAsset,
            pallet_eterra_media::Delivery::RemoteIpfs,
            None,
        )
        .expect("register packaging front");
        pallet_eterra_media::Pallet::<Test>::register_media(
            RuntimeOrigin::signed(1),
            None,
            uri,
            ct,
            pallet_eterra_media::MediaClass::CoreAsset,
            pallet_eterra_media::Delivery::RemoteIpfs,
            None,
        )
        .expect("register packaging back");

        let collection_name: frame_support::BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Core Set".to_vec().try_into().unwrap();
        pallet_eterra_slots::Pallet::<Test>::create_season_collection(
            RuntimeOrigin::signed(1),
            1,
            collection_name,
        )
        .expect("create season collection");
        pallet_eterra_slots::Pallet::<Test>::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            1,
            0,
            pallet_eterra_slots::AssetKind::Border,
            0,
        )
        .expect("add border asset");
        pallet_eterra_slots::Pallet::<Test>::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            1,
            0,
            pallet_eterra_slots::AssetKind::Background,
            1,
        )
        .expect("add background asset");
        pallet_eterra_slots::Pallet::<Test>::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            1,
            0,
            pallet_eterra_slots::AssetKind::Subject,
            2,
        )
        .expect("add subject asset");
        pallet_eterra_slots::Pallet::<Test>::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            1,
            0,
            pallet_eterra_slots::AssetKind::Back,
            3,
        )
        .expect("add back asset");
        pallet_eterra_slots::Pallet::<Test>::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            1,
            0,
            pallet_eterra_slots::AssetKind::PackagingFront,
            4,
        )
        .expect("add packaging front asset");
        pallet_eterra_slots::Pallet::<Test>::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            1,
            0,
            pallet_eterra_slots::AssetKind::PackagingBack,
            5,
        )
        .expect("add packaging back asset");
        pallet_eterra_slots::Pallet::<Test>::publish_season_collection(
            RuntimeOrigin::signed(1),
            1,
            0,
        )
        .expect("publish season collection");

        pallet_eterra_seasons::Pallet::<Test>::activate_season(RuntimeOrigin::signed(1), 1)
            .expect("activate season");

        System::reset_events();
    });
    ext
}
