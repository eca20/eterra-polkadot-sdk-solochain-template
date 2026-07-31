#![cfg(feature = "runtime-benchmarks")]

use super::*;
use eterra_nexus_primitives::{
    Element as V2Element, ElementProfile as V2ElementProfile, SubjectRole,
};
use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::BoundedVec;
use frame_system::pallet_prelude::BlockNumberFor;
use frame_system::RawOrigin;
use sp_runtime::traits::{Saturating, Zero};

const BENCHMARK_SEASON_ID: SeasonId = 1;
const BENCHMARK_COLLECTION_ID: SeasonCollectionId = 0;
const V2_BENCHMARK_SUBJECT_ID: SubjectId = 1;
const V2_BENCHMARK_SUBJECT_VERSION: u32 = 1;
const V2_BENCHMARK_SET_ID: u32 = 1;
const V2_BENCHMARK_POOL_ID: u32 = 1;
const V2_BENCHMARK_POOL_VERSION: u32 = 1;
const V2_BENCHMARK_PACK_SKU: u32 = 1;
const V2_BENCHMARK_PACK_SKU_VERSION: u32 = 1;
const V2_BENCHMARK_POSE_ID: u32 = 100;
const V2_BENCHMARK_BACKGROUND_ID: u32 = 1_000;
const V2_BENCHMARK_ASCENSION_SEASON_ID: u32 = 7;
const V2_BENCHMARK_ELIGIBILITY_ID: Hash32 = [81; 32];

fn ensure_benchmark_season<T: Config>() {
    LegacyCreationSealedV16::<T>::put(false);
    let now = <frame_system::Pallet<T>>::block_number();
    let season_name: BoundedVec<u8, <T as pallet_eterra_seasons::Config>::MaxSeasonNameLen> =
        b"Benchmark Season"
            .to_vec()
            .try_into()
            .expect("benchmark season name fits");
    let season_desc: BoundedVec<u8, <T as pallet_eterra_seasons::Config>::MaxSeasonDescLen> =
        b"Benchmark"
            .to_vec()
            .try_into()
            .expect("benchmark season description fits");

    pallet_eterra_seasons::Seasons::<T>::insert(
        BENCHMARK_SEASON_ID,
        pallet_eterra_seasons::SeasonInfo {
            name: season_name,
            description: season_desc,
            status: pallet_eterra_seasons::SeasonStatus::Active,
            created_at: now,
            activated_at: Some(now),
            closed_at: None,
        },
    );
    pallet_eterra_seasons::ActiveSeasonId::<T>::put(Some(BENCHMARK_SEASON_ID));
    pallet_eterra_seasons::NextSeasonId::<T>::mutate(|next| {
        if *next <= BENCHMARK_SEASON_ID {
            *next = BENCHMARK_SEASON_ID.saturating_add(1);
        }
    });

    let mut collection_ids: BoundedVec<SeasonCollectionId, T::MaxSeasonCollections> =
        BoundedVec::default();
    collection_ids
        .try_push(BENCHMARK_COLLECTION_ID)
        .expect("benchmark collection id fits");
    SeasonCollectionIds::<T>::insert(BENCHMARK_SEASON_ID, collection_ids);

    let collection_name: BoundedVec<u8, T::MaxSeasonCollectionNameLen> = b"Benchmark Set"
        .to_vec()
        .try_into()
        .expect("benchmark collection name fits");
    SeasonCollections::<T>::insert(
        BENCHMARK_SEASON_ID,
        BENCHMARK_COLLECTION_ID,
        SeasonCollectionInfo {
            name: collection_name,
            status: SeasonCollectionStatus::Published,
            created_at: now,
            published_at: Some(now),
        },
    );

    let mut assets = SeasonCollectionAssets::<T>::get(BENCHMARK_SEASON_ID, BENCHMARK_COLLECTION_ID);
    assets.borders = BoundedVec::try_from(sp_std::vec![0]).expect("benchmark border fits");
    assets.backgrounds = BoundedVec::try_from(sp_std::vec![1]).expect("benchmark background fits");
    assets.subjects = BoundedVec::try_from(sp_std::vec![2]).expect("benchmark subject fits");
    assets.backs = BoundedVec::try_from(sp_std::vec![3]).expect("benchmark back fits");
    assets.packaging_fronts =
        BoundedVec::try_from(sp_std::vec![4]).expect("benchmark packaging front fits");
    assets.packaging_backs =
        BoundedVec::try_from(sp_std::vec![5]).expect("benchmark packaging back fits");
    SeasonCollectionAssets::<T>::insert(BENCHMARK_SEASON_ID, BENCHMARK_COLLECTION_ID, assets);
}

fn fund<T: Config>(who: &T::AccountId) {
    ensure_benchmark_season::<T>();

    // Ensure the caller can pay either price and still satisfy `KeepAlive`.
    let pack_price = T::PackPrice::get();
    let pro_price = T::ProPrice::get();
    let mint_price = T::MintCardPrice::get();
    let storage_price = T::CardCapacityUpgradePrice::get();
    let amount = pack_price
        .saturating_add(pack_price)
        .saturating_add(pro_price)
        .saturating_add(pro_price)
        .saturating_add(mint_price)
        .saturating_add(mint_price)
        .saturating_add(storage_price)
        .saturating_add(storage_price);
    let _ = T::PaymentCurrency::deposit_creating(who, amount);
}

fn setup_finalized_card<T: Config>(player: &T::AccountId) -> u32 {
    fund::<T>(player);
    Pallet::<T>::mint_card(RawOrigin::Signed(player.clone()).into()).expect("mint card succeeds");
    // NextCardId is incremented after minting; the minted id is previous value.
    NextCardId::<T>::get().saturating_sub(1)
}

fn active_card_id<T: Config>(player: &T::AccountId) -> u32 {
    let packs = PlayerPacks::<T>::get(player);
    let pack = packs.last().expect("pack exists");
    let idx = ActiveCard::<T>::get(player).expect("active card index");
    *pack
        .get_card_ids()
        .get(idx as usize)
        .expect("card id exists")
}

fn setup_pack<T: Config>(player: &T::AccountId) -> u32 {
    fund::<T>(player);
    Pallet::<T>::mint_pack(RawOrigin::Signed(player.clone()).into()).expect("mint pack succeeds");
    active_card_id::<T>(player)
}

fn setup_generated_slot<T: Config>(player: &T::AccountId) -> u32 {
    let card_id = setup_pack::<T>(player);
    Pallet::<T>::generate_slot(RawOrigin::Signed(player.clone()).into())
        .expect("generate slot succeeds");
    card_id
}

fn setup_pro<T: Config>(player: &T::AccountId) -> u32 {
    fund::<T>(player);
    Pallet::<T>::mint_pro(RawOrigin::Signed(player.clone()).into()).expect("mint pro succeeds");
    ProInProgress::<T>::get(player).expect("pro in progress")
}

fn sample_progression_node() -> ProgressionNode {
    ProgressionNode {
        node_id: 1,
        node_kind: ProgressionNodeKind::Weapon,
        required_level: 1,
        required_item_template_id: 77,
        gear_slot_type: Some(GearSlotType::Weapon),
        power_delta: 5,
        config_version: 1,
    }
}

fn sample_starter_template(subject_id: SubjectId) -> StarterCardTemplate {
    StarterCardTemplate {
        subject_id,
        base_ranks: [
            RankValue::Number(5),
            RankValue::Number(5),
            RankValue::Number(5),
            RankValue::Number(5),
        ],
        apex_side: None,
        style_label: RankStyleLabel::Balanced,
        genes: GeneProfile {
            strength: 5,
            agility: 5,
            vitality: 5,
            defense: 5,
            magic: 5,
            resist: 5,
        },
        element_profile: ElementProfile {
            main: Element::Fire,
            minor: None,
            resistance: None,
            weakness: None,
        },
        card_power: 20,
        config_version: 1,
    }
}

fn sample_starter_team() -> Vec<StarterCardTemplate> {
    (0..5).map(|_| sample_starter_template(2)).collect()
}

fn setup_progression_tree<T: Config>() {
    Pallet::<T>::set_progression_tree(
        RawOrigin::Root.into(),
        1,
        2,
        None,
        sp_std::vec![sample_progression_node()],
        1,
    )
    .expect("set progression tree succeeds");
}

fn setup_progression_card<T: Config>(player: &T::AccountId) -> u32 {
    setup_progression_tree::<T>();
    let card_id = setup_finalized_card::<T>(player);
    if CardProgressions::<T>::get(card_id).is_none() {
        Pallet::<T>::assign_progression_tree_to_card(RawOrigin::Root.into(), card_id, 1)
            .expect("assign progression succeeds");
    }
    card_id
}

fn setup_progression_gear<T: Config>(owner: &T::AccountId, gear_id: GearId) {
    NexusGearItems::<T>::insert(
        gear_id,
        GearItem {
            owner: owner.clone(),
            gear_id,
            slot_type: GearSlotType::Weapon,
            tier: GearTier::Basic,
            power: 1,
            spell_slots: BoundedVec::<SpellSlotEntry, T::MaxNexusSpellSlotsPerCard>::default(),
            equipped_card_id: None,
            season_id: BENCHMARK_SEASON_ID,
            config_version: 1,
        },
    );
    GearItemTemplates::<T>::insert(gear_id, 77);
}

fn setup_progression_spell<T: Config>(owner: &T::AccountId, spell_id: SpellId) {
    NexusSpellbook::<T>::insert(
        spell_id,
        SpellEntry {
            owner: owner.clone(),
            spell_id,
            element: Element::Fire,
            power: 3,
            slotted_to: None,
            config_version: 1,
        },
    );
}

fn v2_subject_definition(subject_id: SubjectId) -> SubjectDefinitionV2 {
    SubjectDefinitionV2 {
        subject_definition_id: subject_id,
        subject_id,
        subject_version: V2_BENCHMARK_SUBJECT_VERSION,
        role: SubjectRole::Hero,
        conversion_policy: ConversionPolicy::PlayableEmbodiment,
        element_profile: V2ElementProfile {
            main: V2Element::Fire,
            minor: None,
            resistance: None,
            weakness: Some(V2Element::Water),
        },
        display_metadata_id: subject_id,
        definition_hash: [subject_id as u8; 32],
        catalog_version: 1,
    }
}

fn v2_profile(subject_id: SubjectId, rarity: CardRarity) -> SubjectRarityProfile {
    let (base_ranks, apex_side) = match rarity {
        CardRarity::Common => ([5, 5, 4, 4], None),
        CardRarity::Rare => ([6, 5, 5, 5], None),
        CardRarity::Epic => ([6, 6, 6, 6], None),
        CardRarity::Legendary => ([7, 7, 7, 6], None),
        CardRarity::Mythical => ([10, 7, 7, 6], Some(0)),
    };
    let profile_id = subject_id
        .saturating_mul(10)
        .saturating_add(rarity.index() as u32);
    SubjectRarityProfile {
        profile_id,
        subject_id,
        subject_version: V2_BENCHMARK_SUBJECT_VERSION,
        rarity,
        base_ranks,
        apex_side,
        rarity_load: rarity.rarity_load(),
        profile_version: 1,
        profile_hash: [profile_id as u8; 32],
    }
}

fn v2_profiles(subject_id: SubjectId) -> [SubjectRarityProfile; 5] {
    [
        v2_profile(subject_id, CardRarity::Common),
        v2_profile(subject_id, CardRarity::Rare),
        v2_profile(subject_id, CardRarity::Epic),
        v2_profile(subject_id, CardRarity::Legendary),
        v2_profile(subject_id, CardRarity::Mythical),
    ]
}

fn v2_pose(subject_id: SubjectId, offset: u32) -> MediaDefinitionV2 {
    let definition_id = V2_BENCHMARK_POSE_ID
        .saturating_add(subject_id.saturating_mul(10))
        .saturating_add(offset);
    MediaDefinitionV2 {
        definition_id,
        subject_id: Some(subject_id),
        media_id: definition_id,
        release_epoch: 1,
        definition_hash: [definition_id as u8; 32],
    }
}

fn v2_background(offset: u32) -> MediaDefinitionV2 {
    let definition_id = V2_BENCHMARK_BACKGROUND_ID.saturating_add(offset);
    MediaDefinitionV2 {
        definition_id,
        subject_id: None,
        media_id: definition_id,
        release_epoch: 1,
        definition_hash: [offset as u8; 32],
    }
}

fn seed_v2_subject<T: Config>(subject_id: SubjectId) {
    let definition = v2_subject_definition(subject_id);
    SubjectDefinitionsV2::<T>::insert(definition.subject_definition_id, definition);
    SubjectDefinitionByKeyV2::<T>::insert(
        (subject_id, V2_BENCHMARK_SUBJECT_VERSION),
        definition.subject_definition_id,
    );
    SubjectActivationStatesV2::<T>::insert(
        definition.subject_definition_id,
        SubjectActivationState {
            subject_definition_id: definition.subject_definition_id,
            mint_enabled: true,
            conversion_enabled: true,
        },
    );
}

fn seed_v2_profiles<T: Config>(subject_id: SubjectId) -> Vec<u32> {
    let profiles = v2_profiles(subject_id);
    for profile in profiles {
        SubjectRarityProfilesV2::<T>::insert(profile.profile_id, profile);
        SubjectRarityProfileByKeyV2::<T>::insert(
            (subject_id, V2_BENCHMARK_SUBJECT_VERSION),
            profile.rarity,
            profile.profile_id,
        );
    }
    profiles.iter().map(|profile| profile.profile_id).collect()
}

fn seed_v2_media<T: Config>(subject_id: SubjectId) -> (Vec<u32>, Vec<u32>) {
    let mut poses = Vec::new();
    for offset in 0..3 {
        let definition = v2_pose(subject_id, offset);
        poses.push(definition.definition_id);
        PoseDefinitionsV2::<T>::insert(definition.definition_id, definition);
    }
    let mut backgrounds = Vec::new();
    for offset in 0..5 {
        let definition = v2_background(offset);
        backgrounds.push(definition.definition_id);
        BackgroundDefinitionsV2::<T>::insert(definition.definition_id, definition);
    }
    (poses, backgrounds)
}

fn publish_v2_pool<T: Config>() {
    seed_v2_subject::<T>(V2_BENCHMARK_SUBJECT_ID);
    let profile_ids = seed_v2_profiles::<T>(V2_BENCHMARK_SUBJECT_ID);
    let (pose_ids, background_ids) = seed_v2_media::<T>(V2_BENCHMARK_SUBJECT_ID);
    Pallet::<T>::publish_acquisition_pool_v2(
        RawOrigin::Root.into(),
        V2_BENCHMARK_POOL_ID,
        V2_BENCHMARK_POOL_VERSION,
        V2_BENCHMARK_SET_ID,
        profile_ids,
        pose_ids,
        background_ids,
        [9; 32],
    )
    .expect("benchmark V2 pool is valid");
}

fn v2_pack_sku<T: Config>() -> PackSkuVersion<BlockNumberFor<T>> {
    PackSkuVersion {
        pack_sku: V2_BENCHMARK_PACK_SKU,
        version: V2_BENCHMARK_PACK_SKU_VERSION,
        card_count: 6,
        set_id: V2_BENCHMARK_SET_ID,
        pool_id: V2_BENCHMARK_POOL_ID,
        pool_version: V2_BENCHMARK_POOL_VERSION,
        rarity_weights: [6_800, 2_200, 750, 200, 50],
        discovery_policy: DiscoveryPolicy::Earned,
        odds_metadata_hash: [8; 32],
        immutable_config_hash: [9; 32],
        active_from: Zero::zero(),
        active_until: None,
    }
}

fn publish_v2_catalog<T: Config>() {
    publish_v2_pool::<T>();
    T::V2BenchmarkHelper::prepare_conversion_entity_profile(
        V2_BENCHMARK_SUBJECT_ID,
        V2_BENCHMARK_SUBJECT_VERSION,
        CardRarity::Common,
    );
    Pallet::<T>::publish_pack_sku_version_v2(RawOrigin::Root.into(), v2_pack_sku::<T>())
        .expect("benchmark V2 SKU is valid");
}

fn issue_v2_credit<T: Config>(owner: &T::AccountId) {
    Pallet::<T>::issue_training_pack_credit_v2(
        RawOrigin::Root.into(),
        owner.clone(),
        V2_BENCHMARK_PACK_SKU,
        V2_BENCHMARK_PACK_SKU_VERSION,
        [11; 32],
    )
    .expect("benchmark training credit is valid");
}

fn request_v2_open<T: Config>(owner: &T::AccountId, commitment: Hash32) -> Hash32 {
    T::V2BenchmarkHelper::prepare_randomness();
    V2FeatureEnabled::<T>::insert(V2Feature::Packs, true);
    Pallet::<T>::request_pack_open_v2(
        RawOrigin::Signed(owner.clone()).into(),
        V2_BENCHMARK_PACK_SKU,
        V2_BENCHMARK_PACK_SKU_VERSION,
        EconomicRealm::Training,
        commitment,
    )
    .expect("benchmark pack request is valid");
    PendingPackOpeningsV2::<T>::iter_keys()
        .next()
        .expect("benchmark opening exists")
}

fn v2_card<T: Config>(
    owner: &T::AccountId,
    card_id: CardIdV2,
    subject_id: SubjectId,
    rarity: CardRarity,
) -> CardInstanceV2<T::AccountId, BlockNumberFor<T>> {
    let profile = v2_profile(subject_id, rarity);
    CardInstanceV2 {
        card_id,
        owner: owner.clone(),
        set_id: V2_BENCHMARK_SET_ID,
        season_id: V2_BENCHMARK_SET_ID,
        subject_id,
        subject_version: V2_BENCHMARK_SUBJECT_VERSION,
        rarity,
        profile_id: profile.profile_id,
        pose_definition_id: v2_pose(subject_id, 0).definition_id,
        background_definition_id: v2_background(0).definition_id,
        serial_number: u64::from(card_id),
        economic_realm: EconomicRealm::Production,
        origin: CardOriginV2::Pack {
            opening_id: [12; 32],
            slot: 0,
        },
        acquisition_id: sp_io::hashing::blake2_256(&(b"BENCHMARK_V2_CARD", card_id).encode()),
        pool_id: V2_BENCHMARK_POOL_ID,
        pool_version: V2_BENCHMARK_POOL_VERSION,
        state: CardStateV2::Active,
        acquired_at: frame_system::Pallet::<T>::block_number(),
    }
}

fn seed_v2_conversion_cards<T: Config>(owner: &T::AccountId) -> CardIdV2 {
    publish_v2_catalog::<T>();
    let source_id = 1;
    CardsV2::<T>::insert(
        source_id,
        v2_card::<T>(
            owner,
            source_id,
            V2_BENCHMARK_SUBJECT_ID,
            CardRarity::Common,
        ),
    );
    let mut safety_cards = Vec::new();
    for offset in 1..=5u32 {
        let subject_id = V2_BENCHMARK_SUBJECT_ID.saturating_add(offset);
        seed_v2_subject::<T>(subject_id);
        seed_v2_profiles::<T>(subject_id);
        seed_v2_media::<T>(subject_id);
        let card_id = u64::from(offset).saturating_add(1);
        CardsV2::<T>::insert(
            card_id,
            v2_card::<T>(owner, card_id, subject_id, CardRarity::Common),
        );
        LiveSupplyBySubjectRarityV2::<T>::insert(
            (subject_id, CardRarity::Common, EconomicRealm::Production),
            1,
        );
        safety_cards.push(card_id);
    }
    NextCardIdV2::<T>::put(7);
    V2OwnerCardCount::<T>::insert(owner, 6);
    V2OwnerActiveCardCount::<T>::insert(owner, 6);
    LiveSupplyBySubjectRarityV2::<T>::insert(
        (
            V2_BENCHMARK_SUBJECT_ID,
            CardRarity::Common,
            EconomicRealm::Production,
        ),
        1,
    );
    let format = v2_format();
    CompetitiveFormatsV2::<T>::insert((format.format_id, format.version), format);
    let safety_cards = BoundedVec::<CardIdV2, T::MaxV2TeamSize>::try_from(safety_cards)
        .expect("five safety cards fit");
    CompetitiveTeamsV2::<T>::insert(
        owner,
        1,
        CompetitiveTeamV2 {
            owner: owner.clone(),
            team_id: 1,
            format_id: format.format_id,
            format_version: format.version,
            cards: safety_cards,
            rarity_load: 5,
        },
    );
    ConversionSafetyTeamByRealmSetV2::<T>::insert(
        owner,
        (V2_BENCHMARK_SET_ID, EconomicRealm::Production),
        1,
    );
    V2FeatureEnabled::<T>::insert(V2Feature::Conversion, true);
    T::V2BenchmarkHelper::prepare_randomness();
    T::V2BenchmarkHelper::prepare_conversion_entity_profile(
        V2_BENCHMARK_SUBJECT_ID,
        V2_BENCHMARK_SUBJECT_VERSION,
        CardRarity::Common,
    );
    source_id
}

fn request_v2_conversion<T: Config>(owner: &T::AccountId) -> Hash32 {
    let source_id = seed_v2_conversion_cards::<T>(owner);
    Pallet::<T>::request_conversion_v2(
        RawOrigin::Signed(owner.clone()).into(),
        source_id,
        1,
        [13; 32],
    )
    .expect("benchmark conversion request is valid");
    ConversionRequestByCard::<T>::get(source_id).expect("benchmark conversion tombstone exists")
}

fn v2_format() -> CompetitiveFormatV2 {
    CompetitiveFormatV2 {
        format_id: 1,
        version: 1,
        set_id: V2_BENCHMARK_SET_ID,
        team_size: 5,
        rarity_load_budget: 10,
        max_mythical: 1,
        max_legendary_or_better: 2,
        rules_hash: [14; 32],
    }
}

fn v2_ascension_season<T: Config>() -> MythicalAscensionSeasonConfig<BlockNumberFor<T>> {
    let starts_at = frame_system::Pallet::<T>::block_number();
    MythicalAscensionSeasonConfig {
        season_id: V2_BENCHMARK_ASCENSION_SEASON_ID,
        set_id: V2_BENCHMARK_SET_ID,
        pool_id: V2_BENCHMARK_POOL_ID,
        pool_version: V2_BENCHMARK_POOL_VERSION,
        starts_at,
        ends_at: starts_at.saturating_add(T::MythicalAscensionSeasonDurationBlocks::get()),
        required_mastery: 10,
        required_marks: 10,
        available_weeks: 12,
        config_hash: [15; 32],
    }
}

fn v2_ascension_subject() -> MythicalAscensionSubjectConfig {
    MythicalAscensionSubjectConfig {
        season_id: V2_BENCHMARK_ASCENSION_SEASON_ID,
        subject_id: V2_BENCHMARK_SUBJECT_ID,
        subject_version: V2_BENCHMARK_SUBJECT_VERSION,
        foundation_pose_definition_id: v2_pose(V2_BENCHMARK_SUBJECT_ID, 0).definition_id,
        foundation_background_definition_id: v2_background(0).definition_id,
        config_hash: [16; 32],
    }
}

fn seed_v2_ascension<T: Config>(owner: &T::AccountId) {
    publish_v2_catalog::<T>();
    MythicalAscensionSeasonConfigsV2::<T>::insert(
        V2_BENCHMARK_ASCENSION_SEASON_ID,
        v2_ascension_season::<T>(),
    );
    MythicalAscensionSubjectConfigsV2::<T>::insert(
        V2_BENCHMARK_ASCENSION_SEASON_ID,
        V2_BENCHMARK_SUBJECT_ID,
        v2_ascension_subject(),
    );
    SeasonEligibilityByAccountV2::<T>::insert(
        owner,
        V2_BENCHMARK_ASCENSION_SEASON_ID,
        V2_BENCHMARK_ELIGIBILITY_ID,
    );
    RegisteredSeasonEligibilityV2::<T>::insert(
        V2_BENCHMARK_ASCENSION_SEASON_ID,
        V2_BENCHMARK_ELIGIBILITY_ID,
        true,
    );
}

benchmarks! {
    mint_pack {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        let packs = PlayerPacks::<T>::get(&caller);
        assert!(!packs.is_empty());
        assert!(PackInProgress::<T>::get(&caller).is_some());
        assert!(PackCardInProgress::<T>::get(&caller).is_some());
        assert!(!CardsByOwner::<T>::get(&caller).is_empty());
    }

    mint_pro {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        let card_id = ProInProgress::<T>::get(&caller).expect("pro in progress");
        let card = Cards::<T>::get(card_id).expect("card exists");
        assert!(card.get_slot_values().is_none());
        assert_eq!(CardAttempts::<T>::get(card_id), 0);
        assert!(CardsByOwner::<T>::get(&caller).contains(&card_id));
    }

    generate_slot {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = setup_pack::<T>(&caller);
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        assert!(CardAttempts::<T>::get(card_id) > 0);
    }

    spin_pro {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = setup_pro::<T>(&caller);

        // Pre-spin up to just before the last allowed spin so the benchmarked
        // call exercises the "forced finalize" path (worst case).
        let max = T::MaxProSpins::get();
        let pre_spins = max.saturating_sub(1);
        for _ in 0..pre_spins {
            Pallet::<T>::spin_pro(RawOrigin::Signed(caller.clone()).into()).expect("pre spin succeeds");
        }
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        let card = Cards::<T>::get(card_id).expect("card exists");
        assert!(card.is_finalized());
        assert!(ProInProgress::<T>::get(&caller).is_none());
    }

    accept_slot {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = setup_generated_slot::<T>(&caller);
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        let card = Cards::<T>::get(card_id).expect("card exists");
        assert!(card.is_finalized());
    }

    accept_pro {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = setup_pro::<T>(&caller);
        Pallet::<T>::spin_pro(RawOrigin::Signed(caller.clone()).into()).expect("spin pro succeeds");
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        let card = Cards::<T>::get(card_id).expect("card exists");
        assert!(card.is_finalized());
        assert!(ProInProgress::<T>::get(&caller).is_none());
    }

    transfer_card {
        let from: T::AccountId = whitelisted_caller();
        let to: T::AccountId = account("to", 0, 0);
        let card_id = setup_generated_slot::<T>(&from);
        Pallet::<T>::accept_slot(RawOrigin::Signed(from.clone()).into())
            .expect("accept slot succeeds");
    }: _(RawOrigin::Signed(from.clone()), card_id, to.clone())
    verify {
        let card = Cards::<T>::get(card_id).expect("card exists");
        assert_eq!(card.get_owner(), &to);
        assert!(CardsByOwner::<T>::get(&to).contains(&card_id));
    }

    mint_card {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        let card_id = NextCardId::<T>::get().saturating_sub(1);
        let card = Cards::<T>::get(card_id).expect("card exists");
        assert!(card.is_finalized());
        assert!(card.get_slot_values().is_some());
        assert!(CardsByOwner::<T>::get(&caller).contains(&card_id));
    }

    claim_starter_grant {
        let caller: T::AccountId = whitelisted_caller();
        ensure_benchmark_season::<T>();
        Pallet::<T>::set_starter_team_config(
            RawOrigin::Root.into(),
            StarterPath::Fire,
            sample_starter_team(),
            1
        ).expect("set starter team succeeds");
    }: _(RawOrigin::Signed(caller.clone()), StarterPath::Fire)
    verify {
        assert!(StarterGrants::<T>::get(&caller).is_some());
        assert_eq!(CardsByOwner::<T>::get(&caller).len(), T::NexusTeamSize::get() as usize);
    }

    set_starter_team_config {
        let team = sample_starter_team();
    }: _(RawOrigin::Root, StarterPath::Fire, team, 1)
    verify {
        assert!(StarterTeamConfigs::<T>::get(StarterPath::Fire).is_some());
    }

    set_price {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = setup_finalized_card::<T>(&caller);
        let price = T::MintCardPrice::get(); // arbitrary non-zero
    }: _(RawOrigin::Signed(caller.clone()), card_id, price)
    verify {
        assert_eq!(CardPrices::<T>::get(card_id), Some(price));
        assert!(ListedByOwner::<T>::get(&caller).contains(&card_id));
    }

    remove_price {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = setup_finalized_card::<T>(&caller);
        let price = T::MintCardPrice::get();
        Pallet::<T>::set_price(RawOrigin::Signed(caller.clone()).into(), card_id, price)
            .expect("set price succeeds");
    }: _(RawOrigin::Signed(caller.clone()), card_id)
    verify {
        assert!(CardPrices::<T>::get(card_id).is_none());
        assert!(!ListedByOwner::<T>::get(&caller).contains(&card_id));
    }

    buy_card_capacity {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        assert_eq!(
            CardCapacityBonus::<T>::get(&caller),
            T::CardCapacityUpgradeAmount::get()
        );
    }

    buy_card {
        let seller: T::AccountId = whitelisted_caller();
        let buyer: T::AccountId = account("buyer", 0, 0);
        fund::<T>(&seller);
        fund::<T>(&buyer);

        let card_id = setup_finalized_card::<T>(&seller);
        let price = T::MintCardPrice::get();
        Pallet::<T>::set_price(RawOrigin::Signed(seller.clone()).into(), card_id, price)
            .expect("set price succeeds");
    }: _(RawOrigin::Signed(buyer.clone()), card_id)
    verify {
        let card = Cards::<T>::get(card_id).expect("card exists");
        assert_eq!(card.get_owner(), &buyer);
        assert!(CardPrices::<T>::get(card_id).is_none());
    }

    set_progression_tree {
        let node = sample_progression_node();
    }: _(RawOrigin::Root, 1, 2, None, sp_std::vec![node], 1)
    verify {
        assert!(ProgressionTrees::<T>::get(1).is_some());
    }

    assign_progression_tree_to_card {
        let caller: T::AccountId = whitelisted_caller();
        setup_progression_tree::<T>();
        let card_id = setup_finalized_card::<T>(&caller);
    }: _(RawOrigin::Root, card_id, 1)
    verify {
        assert!(CardProgressions::<T>::get(card_id).is_some());
    }

    grant_card_experience {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = setup_progression_card::<T>(&caller);
    }: _(RawOrigin::Signed(caller.clone()), 10, 7, 8, card_id, 100)
    verify {
        let progression = CardProgressions::<T>::get(card_id).expect("progression exists");
        assert!(progression.experience >= 100);
        assert!(progression.level >= 2);
    }

    forge_progression_node {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = setup_progression_card::<T>(&caller);
        setup_progression_gear::<T>(&caller, 100);
    }: _(RawOrigin::Signed(caller.clone()), card_id, 1, 100)
    verify {
        assert!(CardEquipmentAttachments::<T>::get(card_id, 1).is_some());
        assert!(NexusGearItems::<T>::get(100).is_none());
        assert!(GearItemTemplates::<T>::get(100).is_none());
    }

    set_card_magic_loadout {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = setup_progression_card::<T>(&caller);
        setup_progression_spell::<T>(&caller, 200);
    }: _(RawOrigin::Signed(caller.clone()), card_id, sp_std::vec![200])
    verify {
        let loadout = CardMagicLoadouts::<T>::get(card_id).expect("loadout exists");
        assert_eq!(loadout.spells.len(), 1);
    }

    seed_alpha_progression_gear {
        let caller: T::AccountId = whitelisted_caller();
    }: _(
        RawOrigin::Root,
        caller.clone(),
        100,
        77,
        GearSlotType::Weapon,
        GearTier::Basic,
        1,
        BENCHMARK_SEASON_ID,
        1
    )
    verify {
        assert!(NexusGearItems::<T>::get(100).is_some());
        assert_eq!(GearItemTemplates::<T>::get(100), Some(77));
    }

    seed_alpha_spell {
        let caller: T::AccountId = whitelisted_caller();
    }: _(RawOrigin::Root, caller.clone(), 200, Element::Fire, 3, 1)
    verify {
        assert!(NexusSpellbook::<T>::get(200).is_some());
    }

    publish_subject_definition_v2 {
        let definition = v2_subject_definition(V2_BENCHMARK_SUBJECT_ID);
    }: _(RawOrigin::Root, definition)
    verify {
        assert!(SubjectDefinitionsV2::<T>::contains_key(V2_BENCHMARK_SUBJECT_ID));
    }

    set_subject_activation_v2 {
        seed_v2_subject::<T>(V2_BENCHMARK_SUBJECT_ID);
        let state = SubjectActivationState {
            subject_definition_id: V2_BENCHMARK_SUBJECT_ID,
            mint_enabled: false,
            conversion_enabled: false,
        };
    }: _(RawOrigin::Root, state)
    verify {
        assert_eq!(
            SubjectActivationStatesV2::<T>::get(V2_BENCHMARK_SUBJECT_ID),
            Some(state)
        );
    }

    publish_subject_rarity_profiles_v2 {
        seed_v2_subject::<T>(V2_BENCHMARK_SUBJECT_ID);
        for profile in v2_profiles(V2_BENCHMARK_SUBJECT_ID) {
            SubjectRarityProfilesV2::<T>::remove(profile.profile_id);
            SubjectRarityProfileByKeyV2::<T>::remove(
                (V2_BENCHMARK_SUBJECT_ID, V2_BENCHMARK_SUBJECT_VERSION),
                profile.rarity,
            );
        }
        let profiles = v2_profiles(V2_BENCHMARK_SUBJECT_ID);
    }: _(
        RawOrigin::Root,
        V2_BENCHMARK_SUBJECT_ID,
        V2_BENCHMARK_SUBJECT_VERSION,
        profiles,
        1
    )
    verify {
        assert!(SubjectRarityProfileByKeyV2::<T>::contains_key(
            (V2_BENCHMARK_SUBJECT_ID, V2_BENCHMARK_SUBJECT_VERSION),
            CardRarity::Mythical,
        ));
    }

    publish_pose_definition_v2 {
        let definition = v2_pose(V2_BENCHMARK_SUBJECT_ID, 0);
    }: _(RawOrigin::Root, definition)
    verify {
        assert!(PoseDefinitionsV2::<T>::contains_key(definition.definition_id));
    }

    publish_background_definition_v2 {
        let definition = v2_background(0);
    }: _(RawOrigin::Root, definition)
    verify {
        assert!(BackgroundDefinitionsV2::<T>::contains_key(definition.definition_id));
    }

    publish_acquisition_pool_v2 {
        seed_v2_subject::<T>(V2_BENCHMARK_SUBJECT_ID);
        let profile_ids = seed_v2_profiles::<T>(V2_BENCHMARK_SUBJECT_ID);
        let (pose_definition_ids, background_definition_ids) =
            seed_v2_media::<T>(V2_BENCHMARK_SUBJECT_ID);
    }: _(
        RawOrigin::Root,
        V2_BENCHMARK_POOL_ID,
        V2_BENCHMARK_POOL_VERSION,
        V2_BENCHMARK_SET_ID,
        profile_ids,
        pose_definition_ids,
        background_definition_ids,
        [9; 32]
    )
    verify {
        assert!(AcquisitionPoolVersionsV2::<T>::contains_key((
            V2_BENCHMARK_POOL_ID,
            V2_BENCHMARK_POOL_VERSION,
        )));
        assert_eq!(NextPoseProtectionSlotV2::<T>::get((
            V2_BENCHMARK_SET_ID,
            V2_BENCHMARK_SUBJECT_ID,
        )), 3);
        assert_eq!(NextBackgroundProtectionSlotV2::<T>::get(V2_BENCHMARK_SET_ID), 5);
    }

    publish_pack_sku_version_v2 {
        publish_v2_pool::<T>();
        let sku = v2_pack_sku::<T>();
    }: _(RawOrigin::Root, sku)
    verify {
        assert!(PackSkuVersionsV2::<T>::contains_key((
            V2_BENCHMARK_PACK_SKU,
            V2_BENCHMARK_PACK_SKU_VERSION,
        )));
    }

    issue_training_pack_credit_v2 {
        let caller: T::AccountId = whitelisted_caller();
        publish_v2_catalog::<T>();
    }: _(
        RawOrigin::Root,
        caller.clone(),
        V2_BENCHMARK_PACK_SKU,
        V2_BENCHMARK_PACK_SKU_VERSION,
        [11; 32]
    )
    verify {
        assert_eq!(
            OutstandingPackCreditCountV2::<T>::get(
                &caller,
                (
                    V2_BENCHMARK_PACK_SKU,
                    V2_BENCHMARK_PACK_SKU_VERSION,
                    EconomicRealm::Training,
                ),
            ),
            1
        );
        assert!(TutorialPackCreditGrantReceiptsV2::<T>::contains_key([11; 32]));
    }

    request_pack_open_v2 {
        let caller: T::AccountId = whitelisted_caller();
        publish_v2_catalog::<T>();
        issue_v2_credit::<T>(&caller);
        T::V2BenchmarkHelper::prepare_randomness();
        V2FeatureEnabled::<T>::insert(V2Feature::Packs, true);
    }: _(
        RawOrigin::Signed(caller.clone()),
        V2_BENCHMARK_PACK_SKU,
        V2_BENCHMARK_PACK_SKU_VERSION,
        EconomicRealm::Training,
        [21; 32]
    )
    verify {
        assert_eq!(PendingPackOpeningsV2::<T>::iter().count(), 1);
        assert_eq!(LockedPackCreditsV2::<T>::iter().count(), 1);
        assert!(PackOpeningRequestReceiptsV2::<T>::contains_key(
            &caller,
            [21; 32],
        ));
        assert_eq!(ReservedV2PackCardCount::<T>::get(&caller), 6);
    }

    finalize_pack_open_v2 {
        let caller: T::AccountId = whitelisted_caller();
        publish_v2_catalog::<T>();
        issue_v2_credit::<T>(&caller);
        let opening_id = request_v2_open::<T>(&caller, [22; 32]);
        let opening = PendingPackOpeningsV2::<T>::get(opening_id)
            .expect("benchmark opening exists");
        T::V2BenchmarkHelper::seed_finalized_randomness(
            opening.randomness_request_id,
            [23; 32],
        );
    }: _(RawOrigin::Signed(caller.clone()), opening_id)
    verify {
        assert!(ProcessedAcquisitionsV2::<T>::contains_key(opening_id));
        assert!(!PendingPackOpeningsV2::<T>::contains_key(opening_id));
        assert_eq!(V2OwnerCardCount::<T>::get(&caller), 6);
        assert_eq!(ReservedV2PackCardCount::<T>::get(&caller), 0);
    }

    timeout_pack_open_v2 {
        let caller: T::AccountId = whitelisted_caller();
        publish_v2_catalog::<T>();
        issue_v2_credit::<T>(&caller);
        let opening_id = request_v2_open::<T>(&caller, [24; 32]);
        let opening = PendingPackOpeningsV2::<T>::get(opening_id)
            .expect("benchmark opening exists");
        T::V2BenchmarkHelper::seed_timed_out_randomness(opening.randomness_request_id);
    }: _(RawOrigin::Signed(caller.clone()), opening_id)
    verify {
        assert!(!PendingPackOpeningsV2::<T>::contains_key(opening_id));
        assert_eq!(
            AvailablePackCreditIdsV2::<T>::get(
                &caller,
                (
                    V2_BENCHMARK_PACK_SKU,
                    V2_BENCHMARK_PACK_SKU_VERSION,
                    EconomicRealm::Training,
                ),
            )
            .len(),
            1
        );
        assert!(TimedOutPackOpeningsV2::<T>::contains_key(opening_id));
        assert_eq!(ReservedV2PackCardCount::<T>::get(&caller), 0);
    }

    publish_competitive_format_v2 {
        let format = v2_format();
    }: _(RawOrigin::Root, format)
    verify {
        assert!(CompetitiveFormatsV2::<T>::contains_key((format.format_id, format.version)));
    }

    save_competitive_team_v2 {
        let caller: T::AccountId = whitelisted_caller();
        let format = v2_format();
        CompetitiveFormatsV2::<T>::insert((format.format_id, format.version), format);
        V2FeatureEnabled::<T>::insert(V2Feature::Ranked, true);
        let mut card_ids = Vec::new();
        for offset in 0..5 {
            let card_id = (offset + 1) as CardIdV2;
            let subject_id = V2_BENCHMARK_SUBJECT_ID.saturating_add(offset);
            seed_v2_subject::<T>(subject_id);
            seed_v2_profiles::<T>(subject_id);
            seed_v2_media::<T>(subject_id);
            card_ids.push(card_id);
            CardsV2::<T>::insert(
                card_id,
                v2_card::<T>(
                    &caller,
                    card_id,
                    subject_id,
                    CardRarity::Common,
                ),
            );
        }
    }: _(
        RawOrigin::Signed(caller.clone()),
        1,
        format.format_id,
        format.version,
        card_ids.try_into().expect("five benchmark cards fit")
    )
    verify {
        assert!(CompetitiveTeamsV2::<T>::contains_key(&caller, 1));
    }

    set_v2_feature_enabled {
    }: _(RawOrigin::Root, V2Feature::Packs, true)
    verify {
        assert!(V2FeatureEnabled::<T>::get(V2Feature::Packs));
    }

    request_conversion_v2 {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = seed_v2_conversion_cards::<T>(&caller);
    }: _(RawOrigin::Signed(caller.clone()), card_id, 1, [31; 32])
    verify {
        assert!(ConversionRequestByCard::<T>::contains_key(card_id));
        assert!(matches!(
            CardsV2::<T>::get(card_id).expect("benchmark card exists").state,
            CardStateV2::ConversionCommitted { .. },
        ));
    }

    finalize_conversion_v2 {
        let caller: T::AccountId = whitelisted_caller();
        let request_id = request_v2_conversion::<T>(&caller);
        let tombstone = CardConversionTombstones::<T>::get(request_id)
            .expect("benchmark tombstone exists");
        T::V2BenchmarkHelper::seed_finalized_randomness(
            tombstone.randomness_request_id,
            [32; 32],
        );
    }: _(RawOrigin::Signed(caller.clone()), request_id)
    verify {
        assert_eq!(
            CardConversionTombstones::<T>::get(request_id)
                .expect("benchmark tombstone exists")
                .resolution,
            ConversionResolution::Created,
        );
        assert_eq!(PendingConversionCountByAccountV2::<T>::get(&caller), 0);
    }

    timeout_conversion_v2 {
        let caller: T::AccountId = whitelisted_caller();
        let request_id = request_v2_conversion::<T>(&caller);
        let tombstone = CardConversionTombstones::<T>::get(request_id)
            .expect("benchmark tombstone exists");
        T::V2BenchmarkHelper::seed_timed_out_randomness(tombstone.randomness_request_id);
    }: _(RawOrigin::Signed(caller.clone()), request_id)
    verify {
        assert_eq!(
            CardConversionTombstones::<T>::get(request_id)
                .expect("benchmark tombstone exists")
                .resolution,
            ConversionResolution::StasisTimeout,
        );
        assert_eq!(PendingConversionCountByAccountV2::<T>::get(&caller), 0);
    }

    configure_mythical_ascension_season_v2 {
        publish_v2_pool::<T>();
        let config = v2_ascension_season::<T>();
    }: _(RawOrigin::Root, config)
    verify {
        assert_eq!(
            MythicalAscensionSeasonConfigsV2::<T>::get(V2_BENCHMARK_ASCENSION_SEASON_ID),
            Some(config),
        );
    }

    configure_mythical_ascension_subject_v2 {
        publish_v2_pool::<T>();
        MythicalAscensionSeasonConfigsV2::<T>::insert(
            V2_BENCHMARK_ASCENSION_SEASON_ID,
            v2_ascension_season::<T>(),
        );
        let config = v2_ascension_subject();
    }: _(RawOrigin::Root, config)
    verify {
        assert_eq!(
            MythicalAscensionSubjectConfigsV2::<T>::get(
                V2_BENCHMARK_ASCENSION_SEASON_ID,
                V2_BENCHMARK_SUBJECT_ID,
            ),
            Some(config),
        );
    }

    link_season_eligibility_v2 {
        let caller: T::AccountId = whitelisted_caller();
        publish_v2_pool::<T>();
        MythicalAscensionSeasonConfigsV2::<T>::insert(
            V2_BENCHMARK_ASCENSION_SEASON_ID,
            v2_ascension_season::<T>(),
        );
    }: _(
        RawOrigin::Root,
        caller.clone(),
        V2_BENCHMARK_ASCENSION_SEASON_ID,
        V2_BENCHMARK_ELIGIBILITY_ID
    )
    verify {
        assert_eq!(
            SeasonEligibilityByAccountV2::<T>::get(
                &caller,
                V2_BENCHMARK_ASCENSION_SEASON_ID,
            ),
            Some(V2_BENCHMARK_ELIGIBILITY_ID),
        );
    }

    record_mythical_ascension_progress_v2 {
        let caller: T::AccountId = whitelisted_caller();
        seed_v2_ascension::<T>(&caller);
    }: _(
        RawOrigin::Root,
        V2_BENCHMARK_ELIGIBILITY_ID,
        V2_BENCHMARK_ASCENSION_SEASON_ID,
        V2_BENCHMARK_SUBJECT_ID,
        EconomicRealm::Production,
        Some(10),
        Some(0),
        true,
        [41; 32]
    )
    verify {
        assert_eq!(
            MythicalSubjectMasteryV2::<T>::get((
                V2_BENCHMARK_ELIGIBILITY_ID,
                V2_BENCHMARK_ASCENSION_SEASON_ID,
                V2_BENCHMARK_SUBJECT_ID,
            )),
            10,
        );
        assert_eq!(
            ConvergenceProgressV2::<T>::get((
                V2_BENCHMARK_ELIGIBILITY_ID,
                V2_BENCHMARK_ASCENSION_SEASON_ID,
            ))
            .marks_earned,
            1,
        );
    }

    ascend_mythical_v2 {
        let caller: T::AccountId = whitelisted_caller();
        seed_v2_ascension::<T>(&caller);
        MythicalSubjectMasteryV2::<T>::insert(
            (
                V2_BENCHMARK_ELIGIBILITY_ID,
                V2_BENCHMARK_ASCENSION_SEASON_ID,
                V2_BENCHMARK_SUBJECT_ID,
            ),
            10,
        );
        LegendaryFoundationsV2::<T>::insert(
            (
                V2_BENCHMARK_ELIGIBILITY_ID,
                V2_BENCHMARK_ASCENSION_SEASON_ID,
                V2_BENCHMARK_SUBJECT_ID,
            ),
            true,
        );
        ConvergenceProgressV2::<T>::insert(
            (
                V2_BENCHMARK_ELIGIBILITY_ID,
                V2_BENCHMARK_ASCENSION_SEASON_ID,
            ),
            ConvergenceProgress {
                marks_earned: 10,
                credited_week_bitmap: 0x03ff,
            },
        );
        MythicCatalystsV2::<T>::insert(
            (
                V2_BENCHMARK_ELIGIBILITY_ID,
                V2_BENCHMARK_ASCENSION_SEASON_ID,
            ),
            true,
        );
        V2FeatureEnabled::<T>::insert(V2Feature::MythicalAscension, true);
    }: _(
        RawOrigin::Signed(caller.clone()),
        V2_BENCHMARK_ASCENSION_SEASON_ID,
        V2_BENCHMARK_SUBJECT_ID,
        MythicalAscensionInput::LegendaryFoundation
    )
    verify {
        assert_eq!(MythicalAscensionReceiptsV2::<T>::iter().count(), 1);
        let output_id = NextCardIdV2::<T>::get().saturating_sub(1);
        let output = CardsV2::<T>::get(output_id).expect("benchmark ascension output exists");
        assert_eq!(output.owner, caller);
        assert_eq!(output.rarity, CardRarity::Mythical);
        assert_eq!(output.economic_realm, EconomicRealm::Production);
    }

    complete_legacy_migration_v16 {
        let verification_hash = [0x5a; 32];
        let expected_cards_seen = 1;
        let expected_anomalies = 0;
        NextCardId::<T>::put(expected_cards_seen);
        LegacyWritesPausedV16::<T>::put(true);
        TcgMigrationStateStorageV16::<T>::put(TcgMigrationStateV16 {
            phase: MigrationPhaseV16::AwaitingVerification,
            from_storage_version: 15,
            cursor: expected_cards_seen,
            upper_bound: expected_cards_seen,
            cards_seen: expected_cards_seen,
            ordinary: expected_cards_seen,
            nft_wrapped: 0,
            known_escrow: 0,
            anomalies: expected_anomalies,
            max_card_id_seen: Some(0),
        });
    }: _(
        RawOrigin::Root,
        expected_cards_seen,
        expected_anomalies,
        verification_hash
    )
    verify {
        assert_eq!(
            TcgMigrationStateStorageV16::<T>::get()
                .expect("migration state exists")
                .phase,
            MigrationPhaseV16::Completed,
        );
        assert_eq!(
            V16MigrationVerificationHash::<T>::get(),
            Some(verification_hash),
        );
        assert!(!LegacyWritesPausedV16::<T>::get());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_benchmarking::impl_benchmark_test_suite;

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
