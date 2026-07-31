use crate::pallet::Config as EterraSlotsConfig;
use crate::{
    mock::*, ActiveCard, CardArtworkCollectionId, CardCapacityBonus, CardEquipmentAttachments,
    CardMagicLoadout, CardMagicLoadouts, CardPrices, CardProgressions, Cards, CardsByOwner,
    CollectionCard, Element, ElementProfile, Error, Event, GearItem, GearItemTemplates,
    GearSlotType, GearTier, ListedByOwner, NextCardId, NextStarterGrantId, NexusAccountStates,
    NexusCardKind, NexusCardOrigin, NexusCollectionCards, NexusGearItems, NexusOverflowCards,
    NexusOverflowSubjectCounts, NexusPrizeKind, NexusPrizePools, NexusPrizeTemplate,
    NexusSpellbook, NexusStorageLocation, NexusSubjectCopyCounts, PackCardInProgress,
    PackInProgress, PlayerPacks, ProgressionNode, ProgressionNodeKind, ProgressionNodeStatus,
    ProgressionTreeBySubject, ProgressionTreeIds, ProgressionTreeUseCounts, ProgressionTrees,
    RankStyleLabel, RankValue, SeasonCollectionIds, SeasonCollectionStatus, SeasonCollections,
    SpellEntry, SpellSlotEntry, StarterCardTemplate, StarterGrants, StarterPath,
    StarterTeamConfigs,
};
use frame_support::traits::Get;
use frame_support::{assert_noop, assert_ok, BoundedBTreeSet, BoundedVec};
use log::{debug, Level, Metadata, Record};
use parity_scale_codec::{Decode, Encode};
use sp_runtime::traits::AccountIdConversion;
use std::sync::Once;

static INIT: Once = Once::new();

pub struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            println!(
                "[{}] {}: {}",
                record.level(),
                record.target(),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

static LOGGER: SimpleLogger = SimpleLogger;

pub fn init_logger() {
    INIT.call_once(|| {
        // Tests can run in parallel across crates; if another test has already
        // installed a logger, don't fail.
        let _ = log::set_logger(&LOGGER);
        log::set_max_level(log::LevelFilter::Debug);
    });
}

fn assert_event_found<F>(matcher: F, event_name: &str)
where
    F: Fn(&RuntimeEvent) -> bool,
{
    let events = frame_system::Pallet::<Test>::events();
    let found = events.iter().any(|record| matcher(&record.event));

    assert!(
        found,
        "Expected {} event but did not find it. Events seen: {:?}",
        event_name, events
    );
}

fn attest_v16_migration() {
    let state =
        crate::TcgMigrationStateStorageV16::<Test>::get().expect("migration awaits attestation");
    assert_eq!(state.phase, crate::MigrationPhaseV16::AwaitingVerification);
    assert_ok!(EterraSlots::complete_legacy_migration_v16(
        RuntimeOrigin::root(),
        state.cards_seen,
        state.anomalies,
        [0xA5; 32],
    ));
}

/// Advances the block number to `n` to ensure event processing occurs.
fn run_to_block(n: u64) {
    while frame_system::Pallet::<Test>::block_number() < n {
        frame_system::Pallet::<Test>::set_block_number(
            frame_system::Pallet::<Test>::block_number() + 1,
        );
        frame_system::Pallet::<Test>::finalize();
        frame_system::Pallet::<Test>::initialize(
            &frame_system::Pallet::<Test>::block_number(),
            &Default::default(),
            &Default::default(),
        );
    }
}

fn seed_owned_card_index(owner: u64, count: u32, id_offset: u32) {
    let mut ids = BoundedBTreeSet::<u32, MaxOwnedCards>::new();
    for id in id_offset..id_offset.saturating_add(count) {
        assert!(ids.try_insert(id).is_ok());
    }
    CardsByOwner::<Test>::insert(owner, ids);
}

fn progression_node(
    node_id: u32,
    required_level: u16,
    required_item_template_id: u32,
    power_delta: u16,
) -> ProgressionNode {
    ProgressionNode {
        node_id,
        node_kind: ProgressionNodeKind::Weapon,
        required_level,
        required_item_template_id,
        gear_slot_type: Some(GearSlotType::Weapon),
        power_delta,
        config_version: 1,
    }
}

fn set_default_progression_tree() {
    assert_ok!(EterraSlots::set_progression_tree(
        RuntimeOrigin::root(),
        1,
        2,
        None,
        vec![progression_node(1, 1, 77, 5)],
        1
    ));
}

fn mint_progression_card(owner: u64) -> u32 {
    set_default_progression_tree();
    let card_id = NextCardId::<Test>::get();
    assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(owner)));
    card_id
}

fn seed_progression_gear(owner: u64, gear_id: u32, item_template_id: u32) {
    NexusGearItems::<Test>::insert(
        gear_id,
        GearItem {
            owner,
            gear_id,
            slot_type: GearSlotType::Weapon,
            tier: GearTier::Basic,
            power: 1,
            spell_slots: BoundedVec::<SpellSlotEntry, MaxNexusSpellSlotsPerCard>::default(),
            equipped_card_id: None,
            season_id: 1,
            config_version: 1,
        },
    );
    GearItemTemplates::<Test>::insert(gear_id, item_template_id);
}

fn seed_spell(owner: u64, spell_id: u32, power: u16) {
    NexusSpellbook::<Test>::insert(
        spell_id,
        SpellEntry {
            owner,
            spell_id,
            element: Element::Fire,
            power,
            slotted_to: None,
            config_version: 1,
        },
    );
}

fn insert_magic_loadout(card_id: u32, spells: Vec<u32>) {
    let bounded_spells: BoundedVec<u32, MaxMagicSlotsPerCard> =
        spells.try_into().expect("test loadout fits");
    CardMagicLoadouts::<Test>::insert(
        card_id,
        CardMagicLoadout {
            card_id,
            spells: bounded_spells,
            config_version: 1,
        },
    );
}

fn seed_collection_card(owner: u64, card_id: u32, power: u16) {
    NexusCollectionCards::<Test>::insert(
        card_id,
        CollectionCard {
            owner,
            subject_id: 2,
            kind: NexusCardKind::Echo,
            origin: NexusCardOrigin::Pull,
            base_ranks: [RankValue::Number(1); 4],
            apex_side: None,
            genes: Default::default(),
            element_profile: ElementProfile {
                main: Element::Fire,
                minor: None,
                resistance: None,
                weakness: None,
            },
            card_power: power,
            location: NexusStorageLocation::Collection,
            account_bound: false,
            acquired_at: System::block_number(),
            config_version: 1,
        },
    );
}

fn starter_template(subject_id: u32, power: u16) -> StarterCardTemplate {
    StarterCardTemplate {
        subject_id,
        base_ranks: [
            RankValue::Number(5),
            RankValue::Number(4),
            RankValue::Number(5),
            RankValue::Number(4),
        ],
        apex_side: None,
        style_label: RankStyleLabel::Balanced,
        genes: Default::default(),
        element_profile: ElementProfile {
            main: Element::Fire,
            minor: None,
            resistance: None,
            weakness: None,
        },
        card_power: power,
        config_version: 1,
    }
}

fn starter_team() -> Vec<StarterCardTemplate> {
    (0..NexusTeamSize::get())
        .map(|offset| starter_template(2, 18 + offset as u16))
        .collect()
}

fn set_default_starter_team(path: StarterPath) {
    assert_ok!(EterraSlots::set_starter_team_config(
        RuntimeOrigin::root(),
        path,
        starter_team(),
        1
    ));
}

#[test]
fn nexus_config_defaults_use_season_1_constants() {
    new_test_ext().execute_with(|| {
        let config = EterraSlots::current_nexus_config();

        assert_eq!(config.config_version, 1);
        assert_eq!(config.subject_copy_cap, 5);
        assert_eq!(config.overflow_total_capacity, 30);
        assert_eq!(config.overflow_per_subject_capacity, 2);
        assert_eq!(config.base_vault_capacity, 20);
        assert_eq!(config.team_size, 5);
        assert_eq!(config.updated_at, System::block_number());
    });
}

#[test]
fn root_can_set_valid_starter_team_config() {
    new_test_ext().execute_with(|| {
        assert_ok!(EterraSlots::set_starter_team_config(
            RuntimeOrigin::root(),
            StarterPath::Fire,
            starter_team(),
            1
        ));

        let config = StarterTeamConfigs::<Test>::get(StarterPath::Fire)
            .expect("starter team config should exist");
        assert_eq!(config.len(), NexusTeamSize::get() as usize);

        assert_event_found(
            |event| {
                matches!(
                    event,
                    RuntimeEvent::EterraSlots(Event::StarterTeamConfigSet {
                        path,
                        card_count,
                        config_version,
                    }) if *path == StarterPath::Fire
                        && *card_count == NexusTeamSize::get()
                        && *config_version == 1
                )
            },
            "StarterTeamConfigSet",
        );
    });
}

#[test]
fn invalid_starter_team_config_is_rejected() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            EterraSlots::set_starter_team_config(
                RuntimeOrigin::signed(2),
                StarterPath::Fire,
                starter_team(),
                1
            ),
            sp_runtime::DispatchError::BadOrigin
        );

        let mut too_short = starter_team();
        too_short.pop();
        assert_noop!(
            EterraSlots::set_starter_team_config(
                RuntimeOrigin::root(),
                StarterPath::Fire,
                too_short,
                1
            ),
            Error::<Test>::InvalidStarterTeamConfig
        );

        let mut bad_rank = starter_team();
        bad_rank[0].base_ranks[0] = RankValue::Apex;
        assert_noop!(
            EterraSlots::set_starter_team_config(
                RuntimeOrigin::root(),
                StarterPath::Fire,
                bad_rank,
                1
            ),
            Error::<Test>::InvalidStarterTeamConfig
        );

        let mut bad_version = starter_team();
        bad_version[0].config_version = 2;
        assert_noop!(
            EterraSlots::set_starter_team_config(
                RuntimeOrigin::root(),
                StarterPath::Fire,
                bad_version,
                1
            ),
            Error::<Test>::InvalidStarterTeamConfig
        );
    });
}

#[test]
fn nexus_prize_pack_fulfills_atomically_and_routes_sixth_copy_to_overflow() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let template = NexusPrizeTemplate {
            kind: NexusCardKind::Echo,
            card: starter_template(2, 18),
        };
        assert_ok!(EterraSlots::set_nexus_prize_pool(
            RuntimeOrigin::root(),
            9,
            vec![template],
            1,
        ));
        assert_noop!(
            EterraSlots::set_nexus_prize_pool(RuntimeOrigin::root(), 9, vec![template], 2,),
            Error::<Test>::NexusPrizePoolAlreadyExists
        );
        let card_ids = EterraSlots::try_fulfill_nexus_prize(
            &player,
            NexusPrizeKind::RandomPack,
            9,
            None,
            [2u8; 32],
            NexusCardOrigin::Claim,
        )
        .expect("pack fulfillment succeeds");
        assert_eq!(card_ids.len(), CardsPerPack::get() as usize);
        assert_eq!(NexusSubjectCopyCounts::<Test>::get(player, 2), 5);
        assert_eq!(NexusOverflowSubjectCounts::<Test>::get(player, 2), 1);
        assert_eq!(NexusOverflowCards::<Test>::get(player).len(), 1);
        for (index, card_id) in card_ids.iter().enumerate() {
            let card = NexusCollectionCards::<Test>::get(card_id).expect("collection record");
            assert_eq!(card.subject_id, 2);
            assert_eq!(card.origin, NexusCardOrigin::Claim);
            assert!(!card.account_bound);
            assert_eq!(
                card.location,
                if index < 5 {
                    NexusStorageLocation::Collection
                } else {
                    NexusStorageLocation::Overflow
                }
            );
            assert!(Cards::<Test>::get(card_id).expect("card").is_finalized());
        }
    });
}

#[test]
fn nexus_prize_pool_rejects_oversized_legacy_vec_before_duplicate_scan() {
    new_test_ext().execute_with(|| {
        let templates = (0..=MaxSubjects::get())
            .map(|subject_id| NexusPrizeTemplate {
                kind: NexusCardKind::Echo,
                card: starter_template(subject_id, 18),
            })
            .collect();

        assert_noop!(
            EterraSlots::set_nexus_prize_pool(RuntimeOrigin::root(), 10, templates, 1),
            Error::<Test>::InvalidNexusPrizePool
        );
        assert!(!NexusPrizePools::<Test>::contains_key(10));
    });
}

#[test]
fn featured_prize_guarantees_subject_but_resolves_traits_once() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let baseline = starter_template(2, 18);
        assert_ok!(EterraSlots::set_nexus_prize_pool(
            RuntimeOrigin::root(),
            11,
            vec![NexusPrizeTemplate {
                kind: NexusCardKind::Monster,
                card: baseline,
            }],
            1,
        ));
        let card_ids = EterraSlots::try_fulfill_nexus_prize(
            &player,
            NexusPrizeKind::FeaturedSubject,
            11,
            Some(2),
            [2u8; 32],
            NexusCardOrigin::Pull,
        )
        .expect("featured fulfillment succeeds");
        assert_eq!(card_ids.len(), 1);
        let card = NexusCollectionCards::<Test>::get(card_ids[0]).expect("collection record");
        assert_eq!(card.subject_id, 2);
        assert_eq!(card.kind, NexusCardKind::Monster);
        assert_eq!(card.origin, NexusCardOrigin::Pull);
        assert_ne!(card.base_ranks, baseline.base_ranks);
        assert_ne!(card.genes, baseline.genes);
        assert!(card.apex_side.is_none());
        assert!(NexusPrizePools::<Test>::contains_key(11));
    });
}

#[test]
fn claim_starter_grant_mints_account_bound_starter_team() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        set_default_starter_team(StarterPath::Fire);

        assert_ok!(EterraSlots::claim_starter_grant(
            RuntimeOrigin::signed(player),
            StarterPath::Fire
        ));

        let account_state =
            NexusAccountStates::<Test>::get(player).expect("account state should exist");
        assert!(account_state.starter_claimed);
        assert_eq!(account_state.starter_path, Some(StarterPath::Fire));
        assert_eq!(account_state.vault_capacity, 20);
        assert_eq!(account_state.config_version, 1);

        let grant = StarterGrants::<Test>::get(player).expect("starter grant should exist");
        assert_eq!(grant.path, StarterPath::Fire);
        assert_eq!(grant.grant_id, 0);
        assert_eq!(NextStarterGrantId::<Test>::get(), 1);

        let owned_cards = CardsByOwner::<Test>::get(player);
        assert_eq!(owned_cards.len(), NexusTeamSize::get() as usize);
        for card_id in owned_cards.iter() {
            let card = Cards::<Test>::get(card_id).expect("starter card should exist");
            assert!(card.is_finalized());
            assert_eq!(card.get_owner(), &player);
            assert_eq!(card.get_slot_values(), Some([5, 4, 5, 4]));

            let collection_card =
                NexusCollectionCards::<Test>::get(card_id).expect("collection card should exist");
            assert_eq!(collection_card.owner, player);
            assert_eq!(collection_card.subject_id, 2);
            assert_eq!(collection_card.origin, NexusCardOrigin::StarterGrant);
            assert_eq!(collection_card.location, NexusStorageLocation::Collection);
            assert!(collection_card.account_bound);
            assert_eq!(
                collection_card.base_ranks,
                [
                    RankValue::Number(5),
                    RankValue::Number(4),
                    RankValue::Number(5),
                    RankValue::Number(4),
                ]
            );

            let art = EterraSlots::card_artwork(card_id).expect("artwork should exist");
            assert_eq!(art.subject_media_id, 2);
        }

        assert_eq!(EterraSlots::unique_minter_count(), 1);

        assert_event_found(
            |event| {
                matches!(
                    event,
                    RuntimeEvent::EterraSlots(Event::StarterGrantClaimed {
                        account_id,
                        path,
                        grant_id,
                        config_version
                    }) if *account_id == player
                        && *path == StarterPath::Fire
                        && *grant_id == 0
                        && *config_version == 1
                )
            },
            "StarterGrantClaimed",
        );
        assert_event_found(
            |event| {
                matches!(
                    event,
                    RuntimeEvent::EterraSlots(Event::NexusCardClaimed {
                        account_id,
                        source,
                        ..
                    }) if *account_id == player && *source == NexusCardOrigin::StarterGrant
                )
            },
            "NexusCardClaimed",
        );
    });
}

#[test]
fn claim_starter_grant_rejects_duplicates() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        set_default_starter_team(StarterPath::Water);

        assert_ok!(EterraSlots::claim_starter_grant(
            RuntimeOrigin::signed(player),
            StarterPath::Water
        ));
        assert_noop!(
            EterraSlots::claim_starter_grant(RuntimeOrigin::signed(player), StarterPath::Wind),
            Error::<Test>::NexusStarterGrantAlreadyClaimed
        );

        let grant = StarterGrants::<Test>::get(player).expect("starter grant should remain");
        assert_eq!(grant.path, StarterPath::Water);
        assert_eq!(NextStarterGrantId::<Test>::get(), 1);
    });
}

#[test]
fn claim_starter_grant_rejects_missing_team_config() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            EterraSlots::claim_starter_grant(RuntimeOrigin::signed(2), StarterPath::Fire),
            Error::<Test>::StarterTeamConfigMissing
        );
    });
}

#[test]
fn starter_cards_cannot_be_listed_transferred_or_converted() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let other = 3u64;
        set_default_starter_team(StarterPath::Fire);
        assert_ok!(EterraSlots::claim_starter_grant(
            RuntimeOrigin::signed(player),
            StarterPath::Fire
        ));
        let card_id = *CardsByOwner::<Test>::get(player)
            .iter()
            .next()
            .expect("starter card id exists");

        assert_noop!(
            EterraSlots::set_price(RuntimeOrigin::signed(player), card_id, 500),
            Error::<Test>::AccountBoundCardLocked
        );
        assert_noop!(
            EterraSlots::transfer_card(RuntimeOrigin::signed(player), card_id, other),
            Error::<Test>::AccountBoundCardLocked
        );

        assert_ok!(EterraSlots::init_card_nft_collection(
            RuntimeOrigin::signed(1),
            1
        ));
        assert_noop!(
            EterraSlots::convert_to_nft(RuntimeOrigin::signed(player), card_id),
            Error::<Test>::AccountBoundCardLocked
        );
    });
}

#[test]
fn nexus_copy_cap_routes_sixth_subject_copy_to_overflow() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let subject_id = 42u32;

        let initial_location =
            EterraSlots::classify_nexus_card_location(&player, subject_id).unwrap();
        assert_eq!(initial_location, NexusStorageLocation::Collection);

        NexusSubjectCopyCounts::<Test>::insert(player, subject_id, 5);

        let capped_location =
            EterraSlots::classify_nexus_card_location(&player, subject_id).unwrap();
        assert_eq!(capped_location, NexusStorageLocation::Overflow);
    });
}

#[test]
fn nexus_overflow_enforces_per_subject_and_total_caps() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let subject_id = 42u32;

        NexusSubjectCopyCounts::<Test>::insert(player, subject_id, 5);
        NexusOverflowSubjectCounts::<Test>::insert(player, subject_id, 2);

        assert!(matches!(
            EterraSlots::classify_nexus_card_location(&player, subject_id),
            Err(Error::<Test>::NexusOverflowSubjectCapacityExceeded)
        ));

        NexusOverflowSubjectCounts::<Test>::insert(player, subject_id, 0);
        let overflow: BoundedVec<u32, NexusOverflowTotalCapacity> =
            (0u32..30u32).collect::<Vec<_>>().try_into().unwrap();
        NexusOverflowCards::<Test>::insert(player, overflow);

        assert!(matches!(
            EterraSlots::classify_nexus_card_location(&player, subject_id),
            Err(Error::<Test>::NexusOverflowCapacityExceeded)
        ));
    });
}

#[test]
fn nexus_team_validation_requires_exactly_five_cards() {
    new_test_ext().execute_with(|| {
        assert_ok!(EterraSlots::validate_nexus_team_size(5));
        assert_noop!(
            EterraSlots::validate_nexus_team_size(4),
            Error::<Test>::NexusTeamSizeInvalid
        );
        assert_noop!(
            EterraSlots::validate_nexus_team_size(6),
            Error::<Test>::NexusTeamSizeInvalid
        );
    });
}

#[test]
fn admin_can_set_valid_progression_tree() {
    new_test_ext().execute_with(|| {
        assert_ok!(EterraSlots::set_progression_tree(
            RuntimeOrigin::root(),
            1,
            2,
            None,
            vec![progression_node(1, 1, 77, 5)],
            1
        ));

        let tree = ProgressionTrees::<Test>::get(1).expect("tree should be stored");
        assert_eq!(tree.tree_id, 1);
        assert_eq!(tree.subject_id, 2);
        assert_eq!(tree.nodes.len(), 1);
        assert_eq!(
            ProgressionTreeBySubject::<Test>::get(2, None::<u8>),
            Some(1)
        );
        assert_eq!(ProgressionTreeIds::<Test>::get().to_vec(), vec![1]);
    });
}

#[test]
fn progression_tree_rejects_signed_empty_oversized_duplicate_and_bad_version() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            EterraSlots::set_progression_tree(
                RuntimeOrigin::signed(1),
                1,
                2,
                None,
                vec![progression_node(1, 1, 77, 5)],
                1
            ),
            sp_runtime::DispatchError::BadOrigin
        );

        assert_noop!(
            EterraSlots::set_progression_tree(RuntimeOrigin::root(), 1, 2, None, vec![], 1),
            Error::<Test>::InvalidProgressionTree
        );

        let oversized = (0..=MaxProgressionNodesPerTree::get())
            .map(|id| progression_node(id, 1, 77 + id, 1))
            .collect::<Vec<_>>();
        assert_noop!(
            EterraSlots::set_progression_tree(RuntimeOrigin::root(), 1, 2, None, oversized, 1),
            Error::<Test>::InvalidProgressionTree
        );

        assert_noop!(
            EterraSlots::set_progression_tree(
                RuntimeOrigin::root(),
                1,
                2,
                None,
                vec![progression_node(1, 1, 77, 5), progression_node(1, 2, 78, 5)],
                1
            ),
            Error::<Test>::InvalidProgressionTree
        );

        let mut bad_version = progression_node(1, 1, 77, 5);
        bad_version.config_version = 2;
        assert_noop!(
            EterraSlots::set_progression_tree(
                RuntimeOrigin::root(),
                1,
                2,
                None,
                vec![bad_version],
                1
            ),
            Error::<Test>::InvalidProgressionTree
        );

        assert_noop!(
            EterraSlots::set_progression_tree(
                RuntimeOrigin::root(),
                1,
                2,
                Some(1),
                vec![progression_node(1, 1, 77, 5)],
                1
            ),
            Error::<Test>::InvalidProgressionTree
        );
    });
}

#[test]
fn unused_progression_tree_can_be_replaced() {
    new_test_ext().execute_with(|| {
        assert_ok!(EterraSlots::set_progression_tree(
            RuntimeOrigin::root(),
            1,
            2,
            None,
            vec![progression_node(1, 1, 77, 5)],
            1
        ));
        assert_ok!(EterraSlots::set_progression_tree(
            RuntimeOrigin::root(),
            1,
            3,
            None,
            vec![progression_node(2, 1, 78, 6)],
            1
        ));

        assert_eq!(ProgressionTreeBySubject::<Test>::get(2, None::<u8>), None);
        assert_eq!(
            ProgressionTreeBySubject::<Test>::get(3, None::<u8>),
            Some(1)
        );
        let tree = ProgressionTrees::<Test>::get(1).expect("tree exists");
        assert_eq!(tree.subject_id, 3);
        assert_eq!(tree.nodes[0].node_id, 2);
    });
}

#[test]
fn tree_replacement_rejected_after_assignment() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let _card_id = mint_progression_card(player);

        assert_noop!(
            EterraSlots::set_progression_tree(
                RuntimeOrigin::root(),
                1,
                2,
                None,
                vec![progression_node(2, 1, 78, 6)],
                1
            ),
            Error::<Test>::ProgressionTreeAlreadyInUse
        );
    });
}

#[test]
fn new_minted_card_gets_matching_progression_tree_assigned() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let card_id = mint_progression_card(player);

        let progression =
            CardProgressions::<Test>::get(card_id).expect("progression should be initialized");
        assert_eq!(progression.card_id, card_id);
        assert_eq!(progression.tree_id, 1);
        assert_eq!(progression.level, 1);
        assert_eq!(progression.experience, 0);
        assert_eq!(ProgressionTreeUseCounts::<Test>::get(1), 1);
    });
}

#[test]
fn authorized_card_xp_grant_updates_level_deterministically() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let issuer = 1u64;
        let card_id = mint_progression_card(player);

        assert_ok!(EterraSlots::grant_card_experience(
            RuntimeOrigin::signed(issuer),
            10,
            7,
            8,
            card_id,
            250
        ));

        let progression = CardProgressions::<Test>::get(card_id).expect("progression exists");
        assert_eq!(progression.experience, 250);
        assert_eq!(progression.level, 3);
        System::assert_last_event(RuntimeEvent::EterraSlots(Event::CardExperienceGranted {
            issuer,
            authority_id: 99,
            game_id: 10,
            version_id: 7,
            event_type_id: 8,
            card_id,
            amount: 250,
            experience: 250,
            level: 3,
            config_version: 1,
        }));
    });
}

#[test]
fn v16_pause_and_unknown_custody_block_all_legacy_card_progression_mutations() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let issuer = 1u64;
        let card_id = mint_progression_card(player);
        seed_progression_gear(player, 100, 77);
        seed_spell(player, 200, 3);

        crate::LegacyWritesPausedV16::<Test>::put(true);
        assert_noop!(
            EterraSlots::assign_progression_tree_to_card(RuntimeOrigin::root(), card_id, 1),
            Error::<Test>::LegacyWritesPaused
        );
        assert_noop!(
            EterraSlots::grant_card_experience(
                RuntimeOrigin::signed(issuer),
                10,
                7,
                8,
                card_id,
                100
            ),
            Error::<Test>::LegacyWritesPaused
        );
        assert_noop!(
            EterraSlots::forge_progression_node(RuntimeOrigin::signed(player), card_id, 1, 100),
            Error::<Test>::LegacyWritesPaused
        );
        assert_noop!(
            EterraSlots::set_card_magic_loadout(RuntimeOrigin::signed(player), card_id, vec![200]),
            Error::<Test>::LegacyWritesPaused
        );
        assert_eq!(
            CardProgressions::<Test>::get(card_id)
                .expect("progression")
                .experience,
            0
        );
        assert!(NexusGearItems::<Test>::contains_key(100));
        assert!(CardMagicLoadouts::<Test>::get(card_id).is_none());

        crate::LegacyWritesPausedV16::<Test>::put(false);
        crate::LegacyCardClassifications::<Test>::insert(
            card_id,
            crate::LegacyCardClassification {
                beneficial_owner: None,
                custody: crate::LegacyCustodyKind::UnknownFrozen,
                frozen: true,
                record_hash: [7; 32],
            },
        );
        assert_noop!(
            EterraSlots::assign_progression_tree_to_card(RuntimeOrigin::root(), card_id, 1),
            Error::<Test>::LegacyCardFrozen
        );
        assert_noop!(
            EterraSlots::grant_card_experience(
                RuntimeOrigin::signed(issuer),
                10,
                7,
                8,
                card_id,
                100
            ),
            Error::<Test>::LegacyCardFrozen
        );
        assert_noop!(
            EterraSlots::forge_progression_node(RuntimeOrigin::signed(player), card_id, 1, 100),
            Error::<Test>::LegacyCardFrozen
        );
        assert_noop!(
            EterraSlots::set_card_magic_loadout(RuntimeOrigin::signed(player), card_id, vec![200]),
            Error::<Test>::LegacyCardFrozen
        );
    });
}

#[test]
fn unauthorized_card_xp_grant_is_rejected() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let card_id = mint_progression_card(player);

        assert_noop!(
            EterraSlots::grant_card_experience(RuntimeOrigin::signed(1), 10, 7, 9, card_id, 100),
            Error::<Test>::NotAuthorizedProgressionIssuer
        );
    });
}

#[test]
fn xp_grant_rejects_amount_above_cap() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let issuer = 1u64;
        let card_id = mint_progression_card(player);

        assert_noop!(
            EterraSlots::grant_card_experience(
                RuntimeOrigin::signed(issuer),
                10,
                7,
                8,
                card_id,
                MaxCardXpGrantAmount::get() + 1
            ),
            Error::<Test>::CardXpGrantTooLarge
        );
    });
}

#[test]
fn owner_can_forge_unlocked_progression_node_with_matching_item() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let card_id = mint_progression_card(player);
        seed_progression_gear(player, 100, 77);

        assert_ok!(EterraSlots::forge_progression_node(
            RuntimeOrigin::signed(player),
            card_id,
            1,
            100
        ));

        let progression = CardProgressions::<Test>::get(card_id).expect("progression exists");
        assert_eq!(progression.completed_nodes.to_vec(), vec![1]);
        assert!(CardEquipmentAttachments::<Test>::get(card_id, 1).is_some());
        assert_eq!(
            EterraSlots::progression_node_status(card_id, 1).unwrap(),
            ProgressionNodeStatus::Completed
        );
        assert!(NexusGearItems::<Test>::get(100).is_none());
        assert!(GearItemTemplates::<Test>::get(100).is_none());
    });
}

#[test]
fn alpha_seeded_progression_gear_can_be_forged_into_node() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let card_id = mint_progression_card(player);

        assert_ok!(EterraSlots::seed_alpha_progression_gear(
            RuntimeOrigin::root(),
            player,
            200,
            77,
            GearSlotType::Weapon,
            GearTier::Basic,
            2,
            1,
            1
        ));
        assert_eq!(GearItemTemplates::<Test>::get(200), Some(77));
        assert!(NexusGearItems::<Test>::get(200).is_some());

        assert_ok!(EterraSlots::forge_progression_node(
            RuntimeOrigin::signed(player),
            card_id,
            1,
            200
        ));

        assert!(CardEquipmentAttachments::<Test>::get(card_id, 1).is_some());
        assert!(NexusGearItems::<Test>::get(200).is_none());
        assert!(GearItemTemplates::<Test>::get(200).is_none());
    });
}

#[test]
fn alpha_seeded_spell_can_be_used_as_removable_magic() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let card_id = mint_progression_card(player);

        assert_ok!(EterraSlots::seed_alpha_spell(
            RuntimeOrigin::root(),
            player,
            201,
            Element::Fire,
            4,
            1
        ));
        assert!(NexusSpellbook::<Test>::get(201).is_some());

        assert_ok!(EterraSlots::set_card_magic_loadout(
            RuntimeOrigin::signed(player),
            card_id,
            vec![201]
        ));

        let loadout = CardMagicLoadouts::<Test>::get(card_id).expect("loadout exists");
        assert_eq!(loadout.spells.to_vec(), vec![201]);
        assert_eq!(EterraSlots::nexus_card_total_power(card_id), 4);
    });
}

#[test]
fn alpha_seed_helpers_reject_signed_origin_and_duplicates() {
    new_test_ext().execute_with(|| {
        let player = 2u64;

        assert_noop!(
            EterraSlots::seed_alpha_progression_gear(
                RuntimeOrigin::signed(player),
                player,
                200,
                77,
                GearSlotType::Weapon,
                GearTier::Basic,
                2,
                1,
                1
            ),
            sp_runtime::DispatchError::BadOrigin
        );

        assert_ok!(EterraSlots::seed_alpha_progression_gear(
            RuntimeOrigin::root(),
            player,
            200,
            77,
            GearSlotType::Weapon,
            GearTier::Basic,
            2,
            1,
            1
        ));
        assert_noop!(
            EterraSlots::seed_alpha_progression_gear(
                RuntimeOrigin::root(),
                player,
                200,
                77,
                GearSlotType::Weapon,
                GearTier::Basic,
                2,
                1,
                1
            ),
            Error::<Test>::AlphaGearAlreadyExists
        );

        assert_ok!(EterraSlots::seed_alpha_spell(
            RuntimeOrigin::root(),
            player,
            201,
            Element::Fire,
            4,
            1
        ));
        assert_noop!(
            EterraSlots::seed_alpha_spell(RuntimeOrigin::root(), player, 201, Element::Fire, 4, 1),
            Error::<Test>::AlphaSpellAlreadyExists
        );
    });
}

#[test]
fn owner_cannot_forge_locked_progression_node() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        assert_ok!(EterraSlots::set_progression_tree(
            RuntimeOrigin::root(),
            1,
            2,
            None,
            vec![progression_node(1, 2, 77, 5)],
            1
        ));
        let card_id = NextCardId::<Test>::get();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(player)));
        seed_progression_gear(player, 100, 77);

        assert_noop!(
            EterraSlots::forge_progression_node(RuntimeOrigin::signed(player), card_id, 1, 100),
            Error::<Test>::ProgressionNodeLocked
        );
    });
}

#[test]
fn owner_cannot_forge_completed_progression_node_twice() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let card_id = mint_progression_card(player);
        seed_progression_gear(player, 100, 77);

        assert_ok!(EterraSlots::forge_progression_node(
            RuntimeOrigin::signed(player),
            card_id,
            1,
            100
        ));
        assert_noop!(
            EterraSlots::forge_progression_node(RuntimeOrigin::signed(player), card_id, 1, 100),
            Error::<Test>::ProgressionNodeAlreadyCompleted
        );
    });
}

#[test]
fn owner_cannot_forge_with_wrong_item_template() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let card_id = mint_progression_card(player);
        seed_progression_gear(player, 100, 999);

        assert_noop!(
            EterraSlots::forge_progression_node(RuntimeOrigin::signed(player), card_id, 1, 100),
            Error::<Test>::RequiredItemMismatch
        );
    });
}

#[test]
fn non_owner_cannot_forge_another_players_card() {
    new_test_ext().execute_with(|| {
        let owner = 2u64;
        let other = 3u64;
        let card_id = mint_progression_card(owner);
        seed_progression_gear(other, 100, 77);

        assert_noop!(
            EterraSlots::forge_progression_node(RuntimeOrigin::signed(other), card_id, 1, 100),
            Error::<Test>::NotCardOwner
        );
    });
}

#[test]
fn magic_loadout_can_change_without_changing_completed_nodes() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let card_id = mint_progression_card(player);
        seed_progression_gear(player, 100, 77);
        seed_spell(player, 200, 3);
        seed_spell(player, 201, 4);

        assert_ok!(EterraSlots::forge_progression_node(
            RuntimeOrigin::signed(player),
            card_id,
            1,
            100
        ));
        let before = CardProgressions::<Test>::get(card_id)
            .expect("progression exists")
            .completed_nodes
            .to_vec();

        assert_ok!(EterraSlots::set_card_magic_loadout(
            RuntimeOrigin::signed(player),
            card_id,
            vec![200]
        ));
        assert_ok!(EterraSlots::set_card_magic_loadout(
            RuntimeOrigin::signed(player),
            card_id,
            vec![201]
        ));

        let after = CardProgressions::<Test>::get(card_id)
            .expect("progression exists")
            .completed_nodes
            .to_vec();
        assert_eq!(before, after);
        assert_eq!(
            CardMagicLoadouts::<Test>::get(card_id)
                .expect("loadout exists")
                .spells
                .to_vec(),
            vec![201]
        );
    });
}

#[test]
fn magic_loadout_rejects_spell_not_owned() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let card_id = mint_progression_card(player);
        seed_spell(3, 200, 3);

        assert_noop!(
            EterraSlots::set_card_magic_loadout(RuntimeOrigin::signed(player), card_id, vec![200]),
            Error::<Test>::SpellNotOwned
        );
    });
}

#[test]
fn total_card_power_includes_base_progression_and_magic_power() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let card_id = mint_progression_card(player);
        seed_collection_card(player, card_id, 7);
        seed_progression_gear(player, 100, 77);
        seed_spell(player, 200, 3);

        assert_ok!(EterraSlots::forge_progression_node(
            RuntimeOrigin::signed(player),
            card_id,
            1,
            100
        ));
        assert_ok!(EterraSlots::set_card_magic_loadout(
            RuntimeOrigin::signed(player),
            card_id,
            vec![200]
        ));

        assert_eq!(EterraSlots::nexus_card_total_power(card_id), 15);
    });
}

#[test]
fn magic_loadout_rejects_duplicate_spell_ids() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let card_id = mint_progression_card(player);
        seed_spell(player, 200, 3);

        assert_noop!(
            EterraSlots::set_card_magic_loadout(
                RuntimeOrigin::signed(player),
                card_id,
                vec![200, 200]
            ),
            Error::<Test>::DuplicateSpellInLoadout
        );
    });
}

#[test]
fn total_power_does_not_double_count_duplicate_spells() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let card_id = mint_progression_card(player);
        seed_collection_card(player, card_id, 7);
        seed_spell(player, 200, 3);
        insert_magic_loadout(card_id, vec![200, 200]);

        assert_eq!(EterraSlots::nexus_card_total_power(card_id), 10);
    });
}

#[test]
fn magic_loadout_clears_on_transfer_card() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let new_owner = 3u64;
        let card_id = mint_progression_card(player);
        seed_spell(player, 200, 3);

        assert_ok!(EterraSlots::set_card_magic_loadout(
            RuntimeOrigin::signed(player),
            card_id,
            vec![200]
        ));
        assert_ok!(EterraSlots::transfer_card(
            RuntimeOrigin::signed(player),
            card_id,
            new_owner
        ));

        assert!(CardMagicLoadouts::<Test>::get(card_id).is_none());
        System::assert_has_event(RuntimeEvent::EterraSlots(Event::CardMagicLoadoutCleared {
            card_id,
            old_owner: player,
            new_owner,
            config_version: 1,
        }));
    });
}

#[test]
fn magic_loadout_clears_on_market_purchase() {
    new_test_ext().execute_with(|| {
        let seller = 2u64;
        let buyer = 3u64;
        let card_id = mint_progression_card(seller);
        seed_spell(seller, 200, 3);

        assert_ok!(EterraSlots::set_card_magic_loadout(
            RuntimeOrigin::signed(seller),
            card_id,
            vec![200]
        ));
        assert_ok!(EterraSlots::set_price(
            RuntimeOrigin::signed(seller),
            card_id,
            10
        ));
        assert_ok!(EterraSlots::buy_card(RuntimeOrigin::signed(buyer), card_id));

        assert!(CardMagicLoadouts::<Test>::get(card_id).is_none());
        assert_eq!(EterraSlots::nexus_card_total_power(card_id), 0);
    });
}

#[test]
fn magic_loadout_clears_on_nft_unwrap() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let new_owner = 3u64;
        let card_id = mint_progression_card(player);
        seed_spell(player, 200, 3);

        assert_ok!(EterraSlots::init_card_nft_collection(
            RuntimeOrigin::signed(1),
            1
        ));
        let collection_id = EterraSlots::card_nft_collection_id().expect("collection id set");
        assert_ok!(EterraSlots::set_card_magic_loadout(
            RuntimeOrigin::signed(player),
            card_id,
            vec![200]
        ));
        assert_ok!(EterraSlots::convert_to_nft(
            RuntimeOrigin::signed(player),
            card_id
        ));
        assert!(CardMagicLoadouts::<Test>::get(card_id).is_none());

        assert_ok!(Nfts::transfer(
            RuntimeOrigin::signed(player),
            collection_id,
            card_id,
            new_owner
        ));
        assert_ok!(EterraSlots::unwrap_from_nft(
            RuntimeOrigin::signed(new_owner),
            card_id
        ));

        assert!(CardMagicLoadouts::<Test>::get(card_id).is_none());
        assert_eq!(EterraSlots::nexus_card_total_power(card_id), 0);
    });
}

#[test]
fn stale_spell_power_not_counted_after_card_owner_changes() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let new_owner = 3u64;
        let card_id = mint_progression_card(player);
        seed_collection_card(new_owner, card_id, 7);
        seed_spell(player, 200, 3);

        assert_ok!(EterraSlots::transfer_card(
            RuntimeOrigin::signed(player),
            card_id,
            new_owner
        ));
        insert_magic_loadout(card_id, vec![200]);

        assert_eq!(EterraSlots::nexus_card_total_power(card_id), 7);
    });
}

#[test]
fn forge_progression_rejected_for_listed_card() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let card_id = mint_progression_card(player);
        seed_progression_gear(player, 100, 77);

        assert_ok!(EterraSlots::set_price(
            RuntimeOrigin::signed(player),
            card_id,
            10
        ));
        assert_noop!(
            EterraSlots::forge_progression_node(RuntimeOrigin::signed(player), card_id, 1, 100),
            Error::<Test>::CardBuildLocked
        );
    });
}

#[test]
fn forge_progression_rejected_for_current_hand_card() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let card_id = mint_progression_card(player);
        seed_progression_gear(player, 100, 77);
        set_mock_current_hand(player, card_id);

        assert_noop!(
            EterraSlots::forge_progression_node(RuntimeOrigin::signed(player), card_id, 1, 100),
            Error::<Test>::CardInCurrentHand
        );
    });
}

#[test]
fn set_magic_loadout_rejected_for_listed_card() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let card_id = mint_progression_card(player);
        seed_spell(player, 200, 3);

        assert_ok!(EterraSlots::set_price(
            RuntimeOrigin::signed(player),
            card_id,
            10
        ));
        assert_noop!(
            EterraSlots::set_card_magic_loadout(RuntimeOrigin::signed(player), card_id, vec![200]),
            Error::<Test>::CardBuildLocked
        );
    });
}

#[test]
fn set_magic_loadout_rejected_for_current_hand_card() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let card_id = mint_progression_card(player);
        seed_spell(player, 200, 3);
        set_mock_current_hand(player, card_id);

        assert_noop!(
            EterraSlots::set_card_magic_loadout(RuntimeOrigin::signed(player), card_id, vec![200]),
            Error::<Test>::CardInCurrentHand
        );
    });
}

#[test]
fn forge_progression_consumes_gear_and_attachment_survives_transfer() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let new_owner = 3u64;
        let card_id = mint_progression_card(player);
        seed_collection_card(player, card_id, 7);
        seed_progression_gear(player, 100, 77);

        assert_ok!(EterraSlots::forge_progression_node(
            RuntimeOrigin::signed(player),
            card_id,
            1,
            100
        ));

        assert!(NexusGearItems::<Test>::get(100).is_none());
        assert!(GearItemTemplates::<Test>::get(100).is_none());
        assert!(CardEquipmentAttachments::<Test>::get(card_id, 1).is_some());
        assert_ok!(EterraSlots::transfer_card(
            RuntimeOrigin::signed(player),
            card_id,
            new_owner
        ));
        assert_eq!(EterraSlots::nexus_card_total_power(card_id), 12);
    });
}

#[test]
fn wrong_owner_cannot_mutate_attached_progression() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let new_owner = 3u64;
        let card_id = mint_progression_card(player);
        seed_progression_gear(player, 100, 77);
        seed_progression_gear(player, 101, 78);
        seed_spell(player, 200, 3);

        assert_ok!(EterraSlots::forge_progression_node(
            RuntimeOrigin::signed(player),
            card_id,
            1,
            100
        ));
        assert_ok!(EterraSlots::transfer_card(
            RuntimeOrigin::signed(player),
            card_id,
            new_owner
        ));

        assert_noop!(
            EterraSlots::forge_progression_node(RuntimeOrigin::signed(player), card_id, 2, 101),
            Error::<Test>::NotCardOwner
        );
        assert_noop!(
            EterraSlots::set_card_magic_loadout(RuntimeOrigin::signed(player), card_id, vec![200]),
            Error::<Test>::NotCardOwner
        );
    });
}

#[test]
fn mint_fails_without_active_season() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        let receiver = <Test as EterraSlotsConfig>::MintCardPriceReceiver::get();
        let player_before = Balances::free_balance(player);
        let receiver_before = Balances::free_balance(receiver);

        // Close the active season (created by the mock genesis helper).
        assert_ok!(EterraSeasons::close_season(RuntimeOrigin::signed(1), 1));

        assert_noop!(
            EterraSlots::mint_card(RuntimeOrigin::signed(player)),
            Error::<Test>::NoActiveSeason
        );

        // Transactional rollback: fee transfer must not occur.
        assert_eq!(Balances::free_balance(player), player_before);
        assert_eq!(Balances::free_balance(receiver), receiver_before);
    });
}

#[test]
fn activate_season_fails_when_published_pool_is_missing() {
    new_test_ext().execute_with(|| {
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S2".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D2".to_vec().try_into().unwrap();
        let collection_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"DraftOnly".to_vec().try_into().unwrap();

        assert_ok!(EterraSeasons::create_season(
            RuntimeOrigin::signed(1),
            name,
            desc
        ));
        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            collection_name
        ));

        assert_noop!(
            EterraSeasons::activate_season(RuntimeOrigin::signed(1), 2),
            Error::<Test>::NoPublishedSeasonCollection
        );
    });
}

#[test]
fn mint_card_writes_card_artwork() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let next_before = NextCardId::<Test>::get();

        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(player)));

        let art = EterraSlots::card_artwork(next_before).expect("card artwork written");
        assert_eq!(art.season_id, 1);
        assert_eq!(art.border_media_id, 0);
        assert_eq!(art.background_media_id, 1);
        assert_eq!(art.subject_media_id, 2);
        assert_eq!(art.back_media_id, 3);

        let mint_info = EterraSlots::card_mint_info(next_before).expect("card mint info written");
        assert_eq!(mint_info.minter, player);
        assert_eq!(mint_info.minted_at, System::block_number());
    });
}

#[test]
fn unique_minter_count_tracks_distinct_accounts_only_once() {
    new_test_ext().execute_with(|| {
        let first = 2u64;
        let second = 3u64;

        assert_eq!(EterraSlots::unique_minter_count(), 0);

        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(first)));
        assert_eq!(EterraSlots::unique_minter_count(), 1);
        assert!(EterraSlots::has_minted(first).is_some());

        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(first)));
        assert_eq!(EterraSlots::unique_minter_count(), 1);

        assert_ok!(EterraSlots::mint_pro(RuntimeOrigin::signed(second)));
        assert_eq!(EterraSlots::unique_minter_count(), 2);
        assert!(EterraSlots::has_minted(second).is_some());
    });
}

#[test]
fn publish_season_collection_requires_at_least_one_art_layer() {
    new_test_ext().execute_with(|| {
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S2".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D2".to_vec().try_into().unwrap();
        let collection_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Core".to_vec().try_into().unwrap();

        assert_ok!(EterraSeasons::create_season(
            RuntimeOrigin::signed(1),
            name,
            desc
        ));
        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            collection_name
        ));

        assert_noop!(
            EterraSlots::publish_season_collection(RuntimeOrigin::signed(1), 2, 0),
            Error::<Test>::SeasonCollectionIncomplete
        );
    });
}

#[test]
fn first_published_collection_requires_a_back_layer() {
    new_test_ext().execute_with(|| {
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S2".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D2".to_vec().try_into().unwrap();
        let collection_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Core".to_vec().try_into().unwrap();
        let ct: BoundedVec<u8, MaxMediaContentTypeLen> = b"image/png".to_vec().try_into().unwrap();

        assert_ok!(EterraSeasons::create_season(
            RuntimeOrigin::signed(1),
            name,
            desc
        ));

        for suffix in [
            b"border".as_slice(),
            b"background".as_slice(),
            b"subject".as_slice(),
            b"packaging-front".as_slice(),
            b"packaging-back".as_slice(),
        ] {
            let mut uri_bytes = b"ipfs://season2-".to_vec();
            uri_bytes.extend_from_slice(suffix);
            let uri: BoundedVec<u8, MaxMediaUriLen> = uri_bytes.try_into().unwrap();
            assert_ok!(EterraMedia::register_media(
                RuntimeOrigin::signed(1),
                None,
                uri,
                ct.clone(),
                pallet_eterra_media::MediaClass::CoreAsset,
                pallet_eterra_media::Delivery::RemoteIpfs,
                None,
            ));
        }

        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            collection_name
        ));
        for (kind, media_id) in [
            (crate::AssetKind::Border, 6u64),
            (crate::AssetKind::Background, 7u64),
            (crate::AssetKind::Subject, 8u64),
            (crate::AssetKind::PackagingFront, 9u64),
            (crate::AssetKind::PackagingBack, 10u64),
        ] {
            assert_ok!(EterraSlots::add_season_collection_asset(
                RuntimeOrigin::signed(1),
                2,
                0,
                kind,
                media_id
            ));
        }

        assert_noop!(
            EterraSlots::publish_season_collection(RuntimeOrigin::signed(1), 2, 0),
            Error::<Test>::SeasonCollectionIncomplete
        );
    });
}

#[test]
fn first_published_collection_requires_packaging_pair() {
    new_test_ext().execute_with(|| {
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S2".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D2".to_vec().try_into().unwrap();
        let collection_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Core".to_vec().try_into().unwrap();
        let ct: BoundedVec<u8, MaxMediaContentTypeLen> = b"image/png".to_vec().try_into().unwrap();

        assert_ok!(EterraSeasons::create_season(
            RuntimeOrigin::signed(1),
            name,
            desc
        ));

        for suffix in [
            b"border".as_slice(),
            b"background".as_slice(),
            b"subject".as_slice(),
            b"back".as_slice(),
        ] {
            let mut uri_bytes = b"ipfs://season2-".to_vec();
            uri_bytes.extend_from_slice(suffix);
            let uri: BoundedVec<u8, MaxMediaUriLen> = uri_bytes.try_into().unwrap();
            assert_ok!(EterraMedia::register_media(
                RuntimeOrigin::signed(1),
                None,
                uri,
                ct.clone(),
                pallet_eterra_media::MediaClass::CoreAsset,
                pallet_eterra_media::Delivery::RemoteIpfs,
                None,
            ));
        }

        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            collection_name
        ));
        for (kind, media_id) in [
            (crate::AssetKind::Border, 6u64),
            (crate::AssetKind::Background, 7u64),
            (crate::AssetKind::Subject, 8u64),
            (crate::AssetKind::Back, 9u64),
        ] {
            assert_ok!(EterraSlots::add_season_collection_asset(
                RuntimeOrigin::signed(1),
                2,
                0,
                kind,
                media_id
            ));
        }

        assert_noop!(
            EterraSlots::publish_season_collection(RuntimeOrigin::signed(1), 2, 0),
            Error::<Test>::SeasonCollectionIncomplete
        );
    });
}

#[test]
fn set_season_collection_asset_weights_validates_count_and_total() {
    new_test_ext().execute_with(|| {
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S2".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D2".to_vec().try_into().unwrap();
        let collection_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Weights".to_vec().try_into().unwrap();
        let ct: BoundedVec<u8, MaxMediaContentTypeLen> = b"image/png".to_vec().try_into().unwrap();

        assert_ok!(EterraSeasons::create_season(
            RuntimeOrigin::signed(1),
            name,
            desc
        ));

        for suffix in [b"border-a".as_slice(), b"border-b".as_slice()] {
            let mut uri_bytes = b"ipfs://season2-".to_vec();
            uri_bytes.extend_from_slice(suffix);
            let uri: BoundedVec<u8, MaxMediaUriLen> = uri_bytes.try_into().unwrap();
            assert_ok!(EterraMedia::register_media(
                RuntimeOrigin::signed(1),
                None,
                uri,
                ct.clone(),
                pallet_eterra_media::MediaClass::CoreAsset,
                pallet_eterra_media::Delivery::RemoteIpfs,
                None,
            ));
        }

        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            collection_name
        ));
        for media_id in [6u64, 7u64] {
            assert_ok!(EterraSlots::add_season_collection_asset(
                RuntimeOrigin::signed(1),
                2,
                0,
                crate::AssetKind::Border,
                media_id
            ));
        }

        assert_noop!(
            EterraSlots::set_season_collection_asset_weights(
                RuntimeOrigin::signed(1),
                2,
                0,
                crate::AssetWeightKind::Border,
                vec![100],
                vec![100]
            ),
            Error::<Test>::AssetWeightCountMismatch
        );

        assert_noop!(
            EterraSlots::set_season_collection_asset_weights(
                RuntimeOrigin::signed(1),
                2,
                0,
                crate::AssetWeightKind::Border,
                vec![60, 30],
                vec![100, 100]
            ),
            Error::<Test>::AssetWeightTotalInvalid
        );
    });
}

#[test]
fn set_season_collection_asset_weights_clear_when_assets_change() {
    new_test_ext().execute_with(|| {
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S2".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D2".to_vec().try_into().unwrap();
        let collection_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Weights".to_vec().try_into().unwrap();
        let ct: BoundedVec<u8, MaxMediaContentTypeLen> = b"image/png".to_vec().try_into().unwrap();

        assert_ok!(EterraSeasons::create_season(
            RuntimeOrigin::signed(1),
            name,
            desc
        ));

        for suffix in [
            b"border-a".as_slice(),
            b"border-b".as_slice(),
            b"border-c".as_slice(),
        ] {
            let mut uri_bytes = b"ipfs://season2-".to_vec();
            uri_bytes.extend_from_slice(suffix);
            let uri: BoundedVec<u8, MaxMediaUriLen> = uri_bytes.try_into().unwrap();
            assert_ok!(EterraMedia::register_media(
                RuntimeOrigin::signed(1),
                None,
                uri,
                ct.clone(),
                pallet_eterra_media::MediaClass::CoreAsset,
                pallet_eterra_media::Delivery::RemoteIpfs,
                None,
            ));
        }

        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            collection_name
        ));
        for media_id in [6u64, 7u64] {
            assert_ok!(EterraSlots::add_season_collection_asset(
                RuntimeOrigin::signed(1),
                2,
                0,
                crate::AssetKind::Border,
                media_id
            ));
        }

        assert_ok!(EterraSlots::set_season_collection_asset_weights(
            RuntimeOrigin::signed(1),
            2,
            0,
            crate::AssetWeightKind::Border,
            vec![70, 30],
            vec![100, 250]
        ));

        let before_add = EterraSlots::season_collection_assets(2, 0);
        assert_eq!(before_add.border_weights.weights.to_vec(), vec![70, 30]);
        assert_eq!(
            before_add.border_weights.multipliers.to_vec(),
            vec![100, 250]
        );

        assert_ok!(EterraSlots::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            2,
            0,
            crate::AssetKind::Border,
            8
        ));

        let after_add = EterraSlots::season_collection_assets(2, 0);
        assert!(after_add.border_weights.weights.is_empty());
        assert!(after_add.border_weights.multipliers.is_empty());
    });
}

#[test]
fn weighted_mint_uses_percentages_and_multipliers() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S2".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D2".to_vec().try_into().unwrap();
        let collection_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Weighted".to_vec().try_into().unwrap();
        let ct: BoundedVec<u8, MaxMediaContentTypeLen> = b"image/png".to_vec().try_into().unwrap();

        assert_ok!(EterraSeasons::create_season(
            RuntimeOrigin::signed(1),
            name,
            desc
        ));

        for suffix in [
            b"border-a".as_slice(),
            b"border-b".as_slice(),
            b"background".as_slice(),
            b"subject".as_slice(),
            b"back".as_slice(),
            b"packaging-front".as_slice(),
            b"packaging-back".as_slice(),
        ] {
            let mut uri_bytes = b"ipfs://season2-".to_vec();
            uri_bytes.extend_from_slice(suffix);
            let uri: BoundedVec<u8, MaxMediaUriLen> = uri_bytes.try_into().unwrap();
            assert_ok!(EterraMedia::register_media(
                RuntimeOrigin::signed(1),
                None,
                uri,
                ct.clone(),
                pallet_eterra_media::MediaClass::CoreAsset,
                pallet_eterra_media::Delivery::RemoteIpfs,
                None,
            ));
        }

        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            collection_name
        ));
        for (kind, media_id) in [
            (crate::AssetKind::Border, 6u64),
            (crate::AssetKind::Border, 7u64),
            (crate::AssetKind::Background, 8u64),
            (crate::AssetKind::Subject, 9u64),
            (crate::AssetKind::Back, 10u64),
            (crate::AssetKind::PackagingFront, 11u64),
            (crate::AssetKind::PackagingBack, 12u64),
        ] {
            assert_ok!(EterraSlots::add_season_collection_asset(
                RuntimeOrigin::signed(1),
                2,
                0,
                kind,
                media_id
            ));
        }

        assert_ok!(EterraSlots::set_season_collection_asset_weights(
            RuntimeOrigin::signed(1),
            2,
            0,
            crate::AssetWeightKind::Border,
            vec![50, 50],
            vec![200, 0]
        ));

        assert_ok!(EterraSlots::publish_season_collection(
            RuntimeOrigin::signed(1),
            2,
            0
        ));
        assert_ok!(EterraSeasons::activate_season(RuntimeOrigin::signed(1), 2));

        let card_id = NextCardId::<Test>::get();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(player)));

        let art = EterraSlots::card_artwork(card_id).expect("card artwork written");
        assert_eq!(art.season_id, 2);
        assert_eq!(art.border_media_id, 6);
        assert_eq!(art.background_media_id, 8);
        assert_eq!(art.subject_media_id, 9);
        assert_eq!(art.back_media_id, 10);
    });
}

#[test]
fn published_season_collection_is_used_for_minting() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S2".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D2".to_vec().try_into().unwrap();
        let collection_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Core".to_vec().try_into().unwrap();
        let ct: BoundedVec<u8, MaxMediaContentTypeLen> = b"image/png".to_vec().try_into().unwrap();

        assert_ok!(EterraSeasons::create_season(
            RuntimeOrigin::signed(1),
            name,
            desc
        ));

        for suffix in [
            b"border".as_slice(),
            b"background".as_slice(),
            b"subject".as_slice(),
            b"back".as_slice(),
            b"packaging-front".as_slice(),
            b"packaging-back".as_slice(),
        ] {
            let mut uri_bytes = b"ipfs://season2-".to_vec();
            uri_bytes.extend_from_slice(suffix);
            let uri: BoundedVec<u8, MaxMediaUriLen> = uri_bytes.try_into().unwrap();
            assert_ok!(EterraMedia::register_media(
                RuntimeOrigin::signed(1),
                None,
                uri,
                ct.clone(),
                pallet_eterra_media::MediaClass::CoreAsset,
                pallet_eterra_media::Delivery::RemoteIpfs,
                None,
            ));
        }

        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            collection_name
        ));
        assert_ok!(EterraSlots::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            2,
            0,
            crate::AssetKind::Border,
            6
        ));
        assert_ok!(EterraSlots::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            2,
            0,
            crate::AssetKind::Background,
            7
        ));
        assert_ok!(EterraSlots::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            2,
            0,
            crate::AssetKind::Subject,
            8
        ));
        assert_ok!(EterraSlots::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            2,
            0,
            crate::AssetKind::Back,
            9
        ));
        assert_ok!(EterraSlots::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            2,
            0,
            crate::AssetKind::PackagingFront,
            10
        ));
        assert_ok!(EterraSlots::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            2,
            0,
            crate::AssetKind::PackagingBack,
            11
        ));
        assert_ok!(EterraSlots::publish_season_collection(
            RuntimeOrigin::signed(1),
            2,
            0
        ));
        assert_ok!(EterraSeasons::activate_season(RuntimeOrigin::signed(1), 2));

        let card_id = NextCardId::<Test>::get();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(player)));

        let art = EterraSlots::card_artwork(card_id).expect("card artwork written");
        assert_eq!(art.season_id, 2);
        assert_eq!(art.border_media_id, 6);
        assert_eq!(art.background_media_id, 7);
        assert_eq!(art.subject_media_id, 8);
        assert_eq!(art.back_media_id, 9);
        assert_eq!(CardArtworkCollectionId::<Test>::get(card_id), Some(0));
    });
}

#[test]
fn published_partial_collections_contribute_to_the_shared_season_pool() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S2".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D2".to_vec().try_into().unwrap();
        let core_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Core".to_vec().try_into().unwrap();
        let subject_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Subject Drop".to_vec().try_into().unwrap();
        let ct: BoundedVec<u8, MaxMediaContentTypeLen> = b"image/png".to_vec().try_into().unwrap();

        assert_ok!(EterraSeasons::create_season(
            RuntimeOrigin::signed(1),
            name,
            desc
        ));

        for suffix in [
            b"core-border".as_slice(),
            b"core-background".as_slice(),
            b"core-back".as_slice(),
            b"core-packaging-front".as_slice(),
            b"core-packaging-back".as_slice(),
            b"subject-drop".as_slice(),
        ] {
            let mut uri_bytes = b"ipfs://season2-".to_vec();
            uri_bytes.extend_from_slice(suffix);
            let uri: BoundedVec<u8, MaxMediaUriLen> = uri_bytes.try_into().unwrap();
            assert_ok!(EterraMedia::register_media(
                RuntimeOrigin::signed(1),
                None,
                uri,
                ct.clone(),
                pallet_eterra_media::MediaClass::CoreAsset,
                pallet_eterra_media::Delivery::RemoteIpfs,
                None,
            ));
        }

        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            core_name
        ));
        for (kind, media_id) in [
            (crate::AssetKind::Border, 6u64),
            (crate::AssetKind::Background, 7u64),
            (crate::AssetKind::Back, 8u64),
            (crate::AssetKind::PackagingFront, 9u64),
            (crate::AssetKind::PackagingBack, 10u64),
        ] {
            assert_ok!(EterraSlots::add_season_collection_asset(
                RuntimeOrigin::signed(1),
                2,
                0,
                kind,
                media_id
            ));
        }
        assert_ok!(EterraSlots::publish_season_collection(
            RuntimeOrigin::signed(1),
            2,
            0
        ));

        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            subject_name
        ));
        assert_ok!(EterraSlots::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            2,
            1,
            crate::AssetKind::Subject,
            11
        ));
        assert_ok!(EterraSlots::publish_season_collection(
            RuntimeOrigin::signed(1),
            2,
            1
        ));
        assert_ok!(EterraSeasons::activate_season(RuntimeOrigin::signed(1), 2));

        let card_id = NextCardId::<Test>::get();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(player)));

        let art = EterraSlots::card_artwork(card_id).expect("card artwork written");
        assert_eq!(art.season_id, 2);
        assert_eq!(art.border_media_id, 6);
        assert_eq!(art.background_media_id, 7);
        assert_eq!(art.subject_media_id, 11);
        assert_eq!(art.back_media_id, 8);
        assert_eq!(CardArtworkCollectionId::<Test>::get(card_id), Some(1));
    });
}

#[test]
fn active_season_can_publish_new_collection_without_mutating_old_one() {
    new_test_ext().execute_with(|| {
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S2".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D2".to_vec().try_into().unwrap();
        let core_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Core".to_vec().try_into().unwrap();
        let ct: BoundedVec<u8, MaxMediaContentTypeLen> = b"image/png".to_vec().try_into().unwrap();

        assert_ok!(EterraSeasons::create_season(
            RuntimeOrigin::signed(1),
            name,
            desc
        ));

        for suffix in [
            b"c-border".as_slice(),
            b"c-background".as_slice(),
            b"c-subject".as_slice(),
            b"c-back".as_slice(),
            b"c-packaging-front".as_slice(),
            b"c-packaging-back".as_slice(),
            b"e-border".as_slice(),
            b"e-background".as_slice(),
            b"e-subject".as_slice(),
        ] {
            let mut uri_bytes = b"ipfs://season2-".to_vec();
            uri_bytes.extend_from_slice(suffix);
            let uri: BoundedVec<u8, MaxMediaUriLen> = uri_bytes.try_into().unwrap();
            assert_ok!(EterraMedia::register_media(
                RuntimeOrigin::signed(1),
                None,
                uri,
                ct.clone(),
                pallet_eterra_media::MediaClass::CoreAsset,
                pallet_eterra_media::Delivery::RemoteIpfs,
                None,
            ));
        }

        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            core_name
        ));
        for (kind, media_id) in [
            (crate::AssetKind::Border, 6u64),
            (crate::AssetKind::Background, 7u64),
            (crate::AssetKind::Subject, 8u64),
            (crate::AssetKind::Back, 9u64),
            (crate::AssetKind::PackagingFront, 10u64),
            (crate::AssetKind::PackagingBack, 11u64),
        ] {
            assert_ok!(EterraSlots::add_season_collection_asset(
                RuntimeOrigin::signed(1),
                2,
                0,
                kind,
                media_id
            ));
        }
        assert_ok!(EterraSlots::publish_season_collection(
            RuntimeOrigin::signed(1),
            2,
            0
        ));
        assert_ok!(EterraSeasons::activate_season(RuntimeOrigin::signed(1), 2));

        let expansion_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Expansion".to_vec().try_into().unwrap();
        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            expansion_name
        ));
        for (kind, media_id) in [
            (crate::AssetKind::Border, 12u64),
            (crate::AssetKind::Background, 13u64),
            (crate::AssetKind::Subject, 14u64),
        ] {
            assert_ok!(EterraSlots::add_season_collection_asset(
                RuntimeOrigin::signed(1),
                2,
                1,
                kind,
                media_id
            ));
        }
        assert_ok!(EterraSlots::publish_season_collection(
            RuntimeOrigin::signed(1),
            2,
            1
        ));

        assert_eq!(SeasonCollectionIds::<Test>::get(2).to_vec(), vec![0, 1]);
        assert_eq!(
            SeasonCollections::<Test>::get(2, 0).map(|collection| collection.status),
            Some(SeasonCollectionStatus::Published)
        );
        assert_eq!(
            SeasonCollections::<Test>::get(2, 1).map(|collection| collection.status),
            Some(SeasonCollectionStatus::Published)
        );
    });
}

#[test]
fn draft_collection_is_not_used_until_published() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S2".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D2".to_vec().try_into().unwrap();
        let ct: BoundedVec<u8, MaxMediaContentTypeLen> = b"image/png".to_vec().try_into().unwrap();

        assert_ok!(EterraSeasons::create_season(
            RuntimeOrigin::signed(1),
            name,
            desc
        ));

        for suffix in [
            b"core-border".as_slice(),
            b"core-background".as_slice(),
            b"core-subject".as_slice(),
            b"core-back".as_slice(),
            b"core-packaging-front".as_slice(),
            b"core-packaging-back".as_slice(),
            b"draft-border".as_slice(),
            b"draft-background".as_slice(),
            b"draft-subject".as_slice(),
        ] {
            let mut uri_bytes = b"ipfs://season2-".to_vec();
            uri_bytes.extend_from_slice(suffix);
            let uri: BoundedVec<u8, MaxMediaUriLen> = uri_bytes.try_into().unwrap();
            assert_ok!(EterraMedia::register_media(
                RuntimeOrigin::signed(1),
                None,
                uri,
                ct.clone(),
                pallet_eterra_media::MediaClass::CoreAsset,
                pallet_eterra_media::Delivery::RemoteIpfs,
                None,
            ));
        }

        let core_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Core".to_vec().try_into().unwrap();
        let draft_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Draft".to_vec().try_into().unwrap();

        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            core_name
        ));
        for (kind, media_id) in [
            (crate::AssetKind::Border, 6u64),
            (crate::AssetKind::Background, 7u64),
            (crate::AssetKind::Subject, 8u64),
            (crate::AssetKind::Back, 9u64),
            (crate::AssetKind::PackagingFront, 10u64),
            (crate::AssetKind::PackagingBack, 11u64),
        ] {
            assert_ok!(EterraSlots::add_season_collection_asset(
                RuntimeOrigin::signed(1),
                2,
                0,
                kind,
                media_id
            ));
        }
        assert_ok!(EterraSlots::publish_season_collection(
            RuntimeOrigin::signed(1),
            2,
            0
        ));

        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            draft_name
        ));
        for (kind, media_id) in [
            (crate::AssetKind::Border, 12u64),
            (crate::AssetKind::Background, 13u64),
            (crate::AssetKind::Subject, 14u64),
        ] {
            assert_ok!(EterraSlots::add_season_collection_asset(
                RuntimeOrigin::signed(1),
                2,
                1,
                kind,
                media_id
            ));
        }

        assert_ok!(EterraSeasons::activate_season(RuntimeOrigin::signed(1), 2));
        let card_id = NextCardId::<Test>::get();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(player)));

        let art = EterraSlots::card_artwork(card_id).expect("card artwork written");
        assert_eq!(art.border_media_id, 6);
        assert_eq!(art.background_media_id, 7);
        assert_eq!(art.subject_media_id, 8);
        assert_eq!(art.back_media_id, 9);
        assert_eq!(CardArtworkCollectionId::<Test>::get(card_id), Some(0));
    });
}

#[test]
fn init_card_nft_collection_creates_collection_and_sets_storage() {
    new_test_ext().execute_with(|| {
        assert_ok!(EterraSlots::init_card_nft_collection(
            RuntimeOrigin::signed(1),
            1
        ));

        assert_eq!(EterraSlots::card_nft_collection_id(), Some(0));
        assert!(pallet_nfts::Collection::<Test>::contains_key(0));
    });
}

#[test]
fn convert_to_nft_escrows_card_and_mints_item() {
    new_test_ext().execute_with(|| {
        let player = 2u64;

        assert_ok!(EterraSlots::init_card_nft_collection(
            RuntimeOrigin::signed(1),
            1
        ));
        let collection_id = EterraSlots::card_nft_collection_id().expect("collection id set");

        let card_id = NextCardId::<Test>::get();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(player)));

        assert_ok!(EterraSlots::convert_to_nft(
            RuntimeOrigin::signed(player),
            card_id
        ));

        let escrow: u64 = frame_support::PalletId(*b"et/tcgsc").into_account_truncating();
        let card = EterraSlots::cards(card_id).expect("card exists");
        assert_eq!(card.get_owner(), &escrow);
        assert!(EterraSlots::converted(card_id).is_some());

        assert_eq!(
            pallet_nfts::Pallet::<Test>::owner(collection_id, card_id),
            Some(player)
        );
    });
}

#[test]
fn nft_transfer_allows_new_owner_to_unwrap() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let new_owner = 3u64;

        assert_ok!(EterraSlots::init_card_nft_collection(
            RuntimeOrigin::signed(1),
            1
        ));
        let collection_id = EterraSlots::card_nft_collection_id().expect("collection id set");

        let card_id = NextCardId::<Test>::get();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(player)));
        assert_ok!(EterraSlots::convert_to_nft(
            RuntimeOrigin::signed(player),
            card_id
        ));

        assert_ok!(Nfts::transfer(
            RuntimeOrigin::signed(player),
            collection_id,
            card_id,
            new_owner
        ));

        assert_eq!(
            pallet_nfts::Pallet::<Test>::owner(collection_id, card_id),
            Some(new_owner)
        );

        assert_ok!(EterraSlots::unwrap_from_nft(
            RuntimeOrigin::signed(new_owner),
            card_id
        ));

        let card = EterraSlots::cards(card_id).expect("card exists");
        assert_eq!(card.get_owner(), &new_owner);
        assert!(EterraSlots::converted(card_id).is_none());
        assert_eq!(
            pallet_nfts::Pallet::<Test>::owner(collection_id, card_id),
            None
        );
    });
}

#[test]
fn v16_wrapped_nft_transfer_updates_custody_indexes_and_unwraps_for_new_owner() {
    use frame_support::traits::{Hooks, StorageVersion};

    new_test_ext().execute_with(|| {
        let player = 2u64;
        let new_owner = 3u64;
        assert_ok!(EterraSlots::init_card_nft_collection(
            RuntimeOrigin::signed(1),
            1
        ));
        let collection_id = EterraSlots::card_nft_collection_id().expect("collection id set");
        let card_id = NextCardId::<Test>::get();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(player)));
        assert_ok!(EterraSlots::convert_to_nft(
            RuntimeOrigin::signed(player),
            card_id
        ));

        StorageVersion::new(15).put::<EterraSlots>();
        <EterraSlots as Hooks<u64>>::on_runtime_upgrade();
        assert_eq!(EterraSlots::migrate_v16_batch(100), 1);
        attest_v16_migration();

        assert_ok!(EterraSlots::transfer_wrapped_card_nft_v16(
            RuntimeOrigin::signed(player),
            card_id,
            new_owner
        ));
        assert_eq!(
            pallet_nfts::Pallet::<Test>::owner(collection_id, card_id),
            Some(new_owner)
        );
        let wrapped =
            crate::LegacyCardClassifications::<Test>::get(card_id).expect("classification");
        assert_eq!(wrapped.custody, crate::LegacyCustodyKind::NftWrapped);
        assert_eq!(wrapped.beneficial_owner, Some(new_owner));
        assert!(!crate::RepairedLegacyCardsByOwnerV16::<Test>::get(
            player, card_id
        ));
        assert!(crate::RepairedLegacyCardsByOwnerV16::<Test>::get(
            new_owner, card_id
        ));

        assert_ok!(EterraSlots::unwrap_from_nft(
            RuntimeOrigin::signed(new_owner),
            card_id
        ));
        assert_eq!(
            EterraSlots::cards(card_id)
                .expect("card remains")
                .get_owner(),
            &new_owner
        );
        let unwrapped =
            crate::LegacyCardClassifications::<Test>::get(card_id).expect("classification");
        assert_eq!(unwrapped.custody, crate::LegacyCustodyKind::Ordinary);
        assert_eq!(unwrapped.beneficial_owner, Some(new_owner));
    });
}

#[test]
fn convert_to_nft_fails_when_card_not_finalized() {
    new_test_ext().execute_with(|| {
        let player = 2u64;

        assert_ok!(EterraSlots::init_card_nft_collection(
            RuntimeOrigin::signed(1),
            1
        ));

        // Pro mint creates a non-finalized card until accepted.
        assert_ok!(EterraSlots::mint_pro(RuntimeOrigin::signed(player)));
        let card_id = EterraSlots::pro_in_progress(player).expect("pro in progress");

        assert_noop!(
            EterraSlots::convert_to_nft(RuntimeOrigin::signed(player), card_id),
            Error::<Test>::CardNotFinalized
        );
    });
}

#[test]
fn test_mint_pack_simple_storage_check() {
    new_test_ext().execute_with(|| {
        let player = 1;

        // Clear any old data
        PlayerPacks::<Test>::remove(&player);
        ActiveCard::<Test>::remove(&player);
        System::reset_events();
        System::set_block_number(42); // or any number you prefer

        // Mint the pack
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        // Verify the minted pack is in storage
        let packs = EterraSlots::player_packs(player);
        assert_eq!(packs.len(), 1, "Should have exactly 1 pack minted");

        // The newly minted pack should have ID = 42 (the current block)
        let minted_pack = &packs[0];
        assert_eq!(minted_pack.get_id(), 42);
    });
}

#[test]
fn test_mint_pack_check_event_directly() {
    new_test_ext().execute_with(|| {
        let player = 1;

        // Ensure a known block number
        System::set_block_number(100);
        System::reset_events();

        // Dispatch extrinsic
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        // Check that PackMinted event with pack_id=100 was indeed emitted
        System::assert_has_event(
            RuntimeEvent::EterraSlots(Event::PackMinted {
                player,
                pack_id: 100,
            })
            .into(),
        );
    });
}

#[test]
fn test_mint_pack_inspect_events() {
    new_test_ext().execute_with(|| {
        let player = 1;
        System::set_block_number(7);
        System::reset_events();

        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        let all_events = System::events();
        assert!(!all_events.is_empty(), "No events were recorded!");

        let minted_event_found = all_events.iter().any(|r| match &r.event {
            RuntimeEvent::EterraSlots(Event::PackMinted {
                player: who,
                pack_id,
            }) => *who == player && *pack_id == 7,
            _ => false,
        });
        assert!(
            minted_event_found,
            "Expected PackMinted for player={}, pack_id=7, but not found.",
            player
        );
    });
}

#[test]
fn test_mint_pack_storage_and_events() {
    new_test_ext().execute_with(|| {
        let player = 1;
        System::set_block_number(8);
        System::reset_events();

        // 1) Mint the pack
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        // 2) Check storage updated
        let packs = EterraSlots::player_packs(player);
        assert_eq!(packs.len(), 1, "Should have 1 pack minted now.");
        let minted_pack = &packs[0];
        assert_eq!(minted_pack.get_id(), 8);

        // 3) Check event with direct assertion
        System::assert_has_event(
            RuntimeEvent::EterraSlots(Event::PackMinted { player, pack_id: 8 }).into(),
        );
    });
}

#[test]
fn mint_pack_rolls_back_when_card_ids_exhausted() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        let receiver = <Test as EterraSlotsConfig>::PackPriceReceiver::get();
        let price: u128 = <Test as EterraSlotsConfig>::PackPrice::get();
        let player_before = Balances::free_balance(player);
        let receiver_before = Balances::free_balance(receiver);

        NextCardId::<Test>::put(u32::MAX - 1);

        assert_noop!(
            EterraSlots::mint_pack(RuntimeOrigin::signed(player)),
            Error::<Test>::CardIdExhausted
        );

        // Ensure transactional rollback: no partial cards or pack state persisted.
        assert_eq!(NextCardId::<Test>::get(), u32::MAX - 1);
        assert!(Cards::<Test>::get(u32::MAX - 1).is_none());
        assert!(Cards::<Test>::get(u32::MAX).is_none());
        assert!(PlayerPacks::<Test>::get(player).is_empty());
        assert_eq!(ActiveCard::<Test>::get(player), None);

        // Fee transfer must also be rolled back.
        assert_eq!(Balances::free_balance(player), player_before);
        assert_eq!(Balances::free_balance(receiver), receiver_before);
        // Sanity check: price is non-zero in this mock.
        assert!(price > 0);
    });
}

#[test]
fn mint_pack_charges_price_and_mints_expected_card_count() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        let receiver = <Test as EterraSlotsConfig>::PackPriceReceiver::get();
        let price: u128 = <Test as EterraSlotsConfig>::PackPrice::get();
        let cards_per_pack: u8 = <Test as EterraSlotsConfig>::CardsPerPack::get();

        let player_before = Balances::free_balance(player);
        let receiver_before = Balances::free_balance(receiver);

        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        // Price charged to player and sent to receiver.
        assert_eq!(Balances::free_balance(player), player_before - price);
        assert_eq!(Balances::free_balance(receiver), receiver_before + price);

        // Pack contains the expected number of unique cards (unique IDs).
        let packs = EterraSlots::player_packs(player);
        let pack = packs.last().expect("pack exists");
        assert_eq!(pack.get_card_ids().len(), cards_per_pack as usize);

        // Ensure the card IDs within the pack are unique.
        let mut ids: sp_std::vec::Vec<u32> = pack.get_card_ids().iter().copied().collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), cards_per_pack as usize);
    });
}

#[test]
fn mint_pro_charges_price_and_starts_in_progress() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        let receiver = <Test as EterraSlotsConfig>::ProPriceReceiver::get();
        let price: u128 = <Test as EterraSlotsConfig>::ProPrice::get();

        let player_before = Balances::free_balance(player);
        let receiver_before = Balances::free_balance(receiver);

        System::reset_events();
        assert_ok!(EterraSlots::mint_pro(RuntimeOrigin::signed(player)));

        // Price charged to player and sent to receiver.
        assert_eq!(Balances::free_balance(player), player_before - price);
        assert_eq!(Balances::free_balance(receiver), receiver_before + price);

        // Pro mint should create an in-progress card (no spin yet).
        let card_id = EterraSlots::pro_in_progress(player).expect("pro in progress");
        let card = EterraSlots::cards(card_id).expect("card exists");
        assert!(!card.is_finalized());
        assert!(card.get_slot_values().is_none());
        assert_eq!(EterraSlots::card_attempts(card_id), 0);
        assert!(EterraSlots::cards_by_owner(player).contains(&card_id));

        // Events: should include ProMintStarted.
        assert_event_found(
            |e| matches!(e, RuntimeEvent::EterraSlots(Event::ProMintStarted { player: who, card_id: id }) if *who == player && *id == card_id),
            "ProMintStarted",
        );
    });
}

#[test]
fn pro_card_stays_visible_in_owner_index_after_finalize() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        assert_ok!(EterraSlots::mint_pro(RuntimeOrigin::signed(player)));
        let card_id = EterraSlots::pro_in_progress(player).expect("pro in progress");
        assert!(EterraSlots::cards_by_owner(player).contains(&card_id));

        assert_ok!(EterraSlots::spin_pro(RuntimeOrigin::signed(player)));
        assert_ok!(EterraSlots::accept_pro(RuntimeOrigin::signed(player)));

        assert!(EterraSlots::cards_by_owner(player).contains(&card_id));
    });
}

#[test]
fn mint_pro_fails_when_already_in_progress() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        assert_ok!(EterraSlots::mint_pro(RuntimeOrigin::signed(player)));
        assert_noop!(
            EterraSlots::mint_pro(RuntimeOrigin::signed(player)),
            Error::<Test>::ProMintAlreadyInProgress
        );
    });
}

#[test]
fn spin_pro_increments_spins() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        assert_ok!(EterraSlots::mint_pro(RuntimeOrigin::signed(player)));
        let card_id = EterraSlots::pro_in_progress(player).expect("pro in progress");

        assert_eq!(EterraSlots::card_attempts(card_id), 0);
        assert_ok!(EterraSlots::spin_pro(RuntimeOrigin::signed(player)));
        assert_eq!(EterraSlots::card_attempts(card_id), 1);
    });
}

#[test]
fn accept_pro_finalizes_and_clears_progress() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        assert_ok!(EterraSlots::mint_pro(RuntimeOrigin::signed(player)));
        let card_id = EterraSlots::pro_in_progress(player).expect("pro in progress");

        assert_ok!(EterraSlots::spin_pro(RuntimeOrigin::signed(player)));
        assert_ok!(EterraSlots::accept_pro(RuntimeOrigin::signed(player)));

        let card = EterraSlots::cards(card_id).expect("card exists");
        assert!(card.is_finalized());
        assert_eq!(EterraSlots::card_attempts(card_id), 0);
        assert!(EterraSlots::pro_in_progress(player).is_none());
    });
}

#[test]
fn accept_pro_fails_when_not_spun() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        assert_ok!(EterraSlots::mint_pro(RuntimeOrigin::signed(player)));
        assert_noop!(
            EterraSlots::accept_pro(RuntimeOrigin::signed(player)),
            Error::<Test>::ProCardNotSpun
        );
    });
}

#[test]
fn spin_pro_forces_finalize_on_last_spin() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        let max_spins: u8 = <Test as EterraSlotsConfig>::MaxProSpins::get();
        assert!(max_spins > 1);

        assert_ok!(EterraSlots::mint_pro(RuntimeOrigin::signed(player)));
        let card_id = EterraSlots::pro_in_progress(player).expect("pro in progress");

        for _ in 0..max_spins {
            assert_ok!(EterraSlots::spin_pro(RuntimeOrigin::signed(player)));
        }

        let card = EterraSlots::cards(card_id).expect("card exists");
        assert!(card.is_finalized());
        assert!(EterraSlots::pro_in_progress(player).is_none());

        // Further spins should fail since the pro mint has been finalized/cleared.
        assert_noop!(
            EterraSlots::spin_pro(RuntimeOrigin::signed(player)),
            Error::<Test>::NoProMintInProgress
        );
    });
}

#[test]
fn test_generate_slot_success() {
    init_logger();
    new_test_ext().execute_with(|| {
        let player = 1;

        debug!("Ensuring fresh state for player {}", player);
        PlayerPacks::<Test>::remove(&player);
        ActiveCard::<Test>::remove(&player);
        System::reset_events();
        assert!(
            EterraSlots::player_packs(player).is_empty(),
            "Player should start with no packs"
        );

        debug!(
            "Minting a pack for player {} before generating a slot.",
            player
        );
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        debug!("Running to next block...");
        run_to_block(frame_system::Pallet::<Test>::block_number() + 1);

        // Check active card
        let active_card = ActiveCard::<Test>::get(player);
        assert!(
            active_card.is_some(),
            "Expected an active card but found None"
        );

        debug!("Generate slot for the active card");
        System::reset_events();
        assert_ok!(EterraSlots::generate_slot(RuntimeOrigin::signed(player)));

        run_to_block(frame_system::Pallet::<Test>::block_number() + 1);

        // We only have `SlotGenerated { card_id, values }` now
        // So let's confirm that event by checking it has the correct type:
        assert_event_found(
            |e| {
                matches!(
                    e,
                    RuntimeEvent::EterraSlots(Event::SlotGenerated { values, .. })
                        if values.len() == 4
                )
            },
            "SlotGenerated",
        );
    });
}

#[test]
fn test_accept_slot_success() {
    init_logger();
    new_test_ext().execute_with(|| {
        let player = 1;

        debug!("Minting a pack and generating a slot for player {}", player);
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));
        run_to_block(System::block_number() + 1);

        // Generate a slot
        assert_ok!(EterraSlots::generate_slot(RuntimeOrigin::signed(player)));
        run_to_block(System::block_number() + 1);

        debug!("Accepting slot...");
        System::reset_events();
        assert_ok!(EterraSlots::accept_slot(RuntimeOrigin::signed(player)));
        run_to_block(System::block_number() + 1);

        // The event is now `SlotAccepted { card_id }`, no player field
        assert_event_found(
            |e| matches!(e, RuntimeEvent::EterraSlots(Event::SlotAccepted { .. })),
            "SlotAccepted",
        );
    });
}

#[test]
fn mint_pack_fails_when_card_capacity_would_be_exceeded_without_charging_fee() {
    init_logger();
    new_test_ext().execute_with(|| {
        let player = 1u64;
        let receiver = <Test as EterraSlotsConfig>::PackPriceReceiver::get();
        let player_before = Balances::free_balance(player);
        let receiver_before = Balances::free_balance(receiver);

        seed_owned_card_index(player, 495, 10_000);

        assert_noop!(
            EterraSlots::mint_pack(RuntimeOrigin::signed(player)),
            Error::<Test>::CardCapacityExceeded
        );

        assert_eq!(Balances::free_balance(player), player_before);
        assert_eq!(Balances::free_balance(receiver), receiver_before);
        assert!(PlayerPacks::<Test>::get(player).is_empty());
    });
}

#[test]
fn test_generate_slot_fail_when_no_active_card() {
    init_logger();
    new_test_ext().execute_with(|| {
        let player = 1;

        debug!("Attempt to generate slot with no pack at all");
        assert_noop!(
            EterraSlots::generate_slot(RuntimeOrigin::signed(player)),
            Error::<Test>::NoPackFound
        );
    });
}

#[test]
fn test_accept_slot_fail_when_slot_not_rolled() {
    init_logger();
    new_test_ext().execute_with(|| {
        let player = 1;

        debug!("Minting pack but not generating a slot yet");
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        debug!("Try to accept slot before rolling one");
        let result = EterraSlots::accept_slot(RuntimeOrigin::signed(player));
        assert!(
            result == Err(Error::<Test>::NoActiveCard.into()),
            "Expected NoActiveCard but got {:?}",
            result
        );
    });
}

#[test]
fn active_card_advances_after_finalize() {
    new_test_ext().execute_with(|| {
        let player = 1;
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));
        assert_eq!(ActiveCard::<Test>::get(player), Some(0));

        let max_attempts: u8 = <Test as EterraSlotsConfig>::MaxAttempts::get();
        for _ in 0..max_attempts {
            assert_ok!(EterraSlots::generate_slot(RuntimeOrigin::signed(player)));
        }

        assert_eq!(ActiveCard::<Test>::get(player), Some(1));
        let packs = EterraSlots::player_packs(player);
        let pack = packs.last().expect("pack exists");
        assert_eq!(pack.get_active_card_index(), 1);

        let first_id = *pack.get_card_ids().first().expect("card exists");
        let card = EterraSlots::cards(first_id).expect("card exists");
        assert!(card.is_finalized());
    });
}

#[test]
fn pack_completed_clears_active_card_and_emits_event() {
    new_test_ext().execute_with(|| {
        let player = 1;
        System::set_block_number(1);
        System::reset_events();
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        let max_attempts: u8 = <Test as EterraSlotsConfig>::MaxAttempts::get();
        let cards_per_pack: u8 = <Test as EterraSlotsConfig>::CardsPerPack::get();

        for _ in 0..cards_per_pack {
            for _ in 0..max_attempts {
                assert_ok!(EterraSlots::generate_slot(RuntimeOrigin::signed(player)));
            }
        }

        assert_eq!(ActiveCard::<Test>::get(player), None);
        assert!(PackInProgress::<Test>::get(player).is_none());
        assert!(PackCardInProgress::<Test>::get(player).is_none());
        let packs = EterraSlots::player_packs(player);
        assert!(packs.is_empty());

        System::assert_has_event(RuntimeEvent::EterraSlots(Event::PackCompleted {
            player,
            pack_id: 1,
        }));
    });
}

#[test]
fn test_attempts_removed_after_generating_max_times() {
    new_test_ext().execute_with(|| {
        let player = 1;

        // Mint a pack
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        // We want to see which card_id was created.
        let packs = EterraSlots::player_packs(player);
        let last_pack = packs.last().expect("Pack should exist");
        let card_id = last_pack
            .get_card_ids()
            .first()
            .copied()
            .expect("Should have at least one card ID in the pack");

        // Check the MaxAttempts
        let max_attempts: u8 = <Test as EterraSlotsConfig>::MaxAttempts::get();

        // Generate slots until we hit max
        for _ in 0..max_attempts {
            assert_ok!(EterraSlots::generate_slot(RuntimeOrigin::signed(player)));
        }

        // After final generation, that card should be finalized => attempts removed
        let attempts_after = EterraSlots::card_attempts(card_id);
        assert_eq!(
            attempts_after, 0,
            "Expected attempts to be removed after finalization."
        );
    });
}

#[test]
fn test_attempts_removed_after_accept_slot() {
    new_test_ext().execute_with(|| {
        let player = 1;

        // Mint a pack
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        // Grab the first card_id
        let packs = EterraSlots::player_packs(player);
        let last_pack = packs.last().unwrap();
        let card_id = *last_pack.get_card_ids().first().unwrap();

        // Generate one slot
        assert_ok!(EterraSlots::generate_slot(RuntimeOrigin::signed(player)));

        // Should now have attempts = 1
        let attempts_before = EterraSlots::card_attempts(card_id);
        assert_eq!(attempts_before, 1);

        // Accept slot => finalize the card => attempts removed
        assert_ok!(EterraSlots::accept_slot(RuntimeOrigin::signed(player)));

        let attempts_after = EterraSlots::card_attempts(card_id);
        assert_eq!(
            attempts_after, 0,
            "Expected attempts to be removed after finalization."
        );
    });
}

#[test]
fn test_transfer_card_not_owner_fails() {
    new_test_ext().execute_with(|| {
        let owner = 1;
        let non_owner = 2;
        let malicious_user = 3;

        // 1) Mint a pack for `owner`
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(owner)));

        // 2) Retrieve the first card
        let packs = EterraSlots::player_packs(owner);
        let card_id = *packs[0]
            .get_card_ids()
            .first()
            .expect("At least one card expected");

        // 3) Attempt to transfer it as `non_owner` or `malicious_user`
        let result =
            EterraSlots::transfer_card(RuntimeOrigin::signed(non_owner), card_id, malicious_user);

        // 4) Confirm it fails with the expected NotCardOwner error
        assert_noop!(result, Error::<Test>::NotCardOwner);
    });
}

#[test]
fn test_transfer_card_no_such_card_fails() {
    new_test_ext().execute_with(|| {
        let sender = 1;
        let receiver = 2;

        // Don’t mint anything, so no cards exist
        let card_id_that_does_not_exist = 9999;

        // Attempt transfer
        let result = EterraSlots::transfer_card(
            RuntimeOrigin::signed(sender),
            card_id_that_does_not_exist,
            receiver,
        );

        assert_noop!(result, Error::<Test>::NoSuchCard);
    });
}

#[test]
fn test_transfer_card_success() {
    new_test_ext().execute_with(|| {
        let original_owner = 1;
        let new_owner = 2;

        // 1) Mint a pack for `original_owner` to create some cards.
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(
            original_owner
        )));

        // 2) Grab the first pack and its first card_id.
        let packs = EterraSlots::player_packs(original_owner);
        let pack = packs.first().expect("Expected at least one pack minted");
        let card_id = pack
            .get_card_ids()
            .first()
            .copied()
            .expect("Expected at least one card in the pack");

        // Log which card ID we’re transferring
        println!("[TEST] Minted card_id: {}", card_id);

        // 3) Finalize the card before transferring
        System::reset_events(); // Clear old events

        assert_ok!(EterraSlots::generate_slot(RuntimeOrigin::signed(
            original_owner
        )));
        assert_ok!(EterraSlots::accept_slot(RuntimeOrigin::signed(
            original_owner
        )));

        // 4) Transfer the finalized card to `new_owner`
        let result =
            EterraSlots::transfer_card(RuntimeOrigin::signed(original_owner), card_id, new_owner);

        assert_ok!(result);

        // 5) Confirm the card's ownership changed in storage
        let card_info = EterraSlots::cards(card_id).expect("Card must still exist");
        println!("[TEST] card_info after transfer: {:?}", card_info);
        assert_eq!(
            *card_info.get_owner(),
            new_owner,
            "Storage shows the card owner didn't update!"
        );
        assert!(!CardsByOwner::<Test>::get(original_owner).contains(&card_id));
        assert!(CardsByOwner::<Test>::get(new_owner).contains(&card_id));

        // 6) Attempt to find a CardTransferred event.
        let events = System::events();
        println!("[TEST] Events after transfer: {:?}", events);

        let found_event = events.iter().any(|r| {
            matches!(
                r.event,
                RuntimeEvent::EterraSlots(Event::CardTransferred {
                    from,
                    to,
                    card_id: c_id
                }) if from == original_owner && to == new_owner && c_id == card_id
            )
        });
        if !found_event {
            println!(
                "[WARN] No CardTransferred event found for card_id={}, but ownership DID update.",
                card_id
            );
        } else {
            println!("[TEST] Found the CardTransferred event as expected!");
        }
    });
}

#[test]
fn transfer_card_fails_when_recipient_card_capacity_is_full() {
    new_test_ext().execute_with(|| {
        let original_owner = 1u64;
        let new_owner = 2u64;

        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(
            original_owner
        )));
        let card_id = NextCardId::<Test>::get().saturating_sub(1);

        seed_owned_card_index(new_owner, BaseCardCapacity::get(), 30_000);

        assert_noop!(
            EterraSlots::transfer_card(RuntimeOrigin::signed(original_owner), card_id, new_owner),
            Error::<Test>::CardCapacityExceeded
        );

        let card = EterraSlots::cards(card_id).expect("card exists");
        assert_eq!(*card.get_owner(), original_owner);
    });
}

#[test]
fn mint_card_charges_price_and_mints() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);

        let player = 1u64;
        let receiver = <Test as EterraSlotsConfig>::MintCardPriceReceiver::get();
        let price: u128 = <Test as EterraSlotsConfig>::MintCardPrice::get();

        let player_before = Balances::free_balance(player);
        let receiver_before = Balances::free_balance(receiver);

        System::reset_events();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(player)));

        let card_id = NextCardId::<Test>::get().saturating_sub(1);
        let card = EterraSlots::cards(card_id).expect("card exists");
        assert_eq!(*card.get_owner(), player);
        assert!(card.is_finalized());
        assert!(card.get_slot_values().is_some());
        assert!(EterraSlots::cards_by_owner(player).contains(&card_id));

        assert_eq!(Balances::free_balance(player), player_before - price);
        assert_eq!(Balances::free_balance(receiver), receiver_before + price);

        System::assert_has_event(RuntimeEvent::EterraSlots(Event::CardMinted {
            player,
            card_id,
        }));
    });
}

#[test]
fn mint_card_fails_when_card_capacity_is_full_without_charging_fee() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        let receiver = <Test as EterraSlotsConfig>::MintCardPriceReceiver::get();
        let player_before = Balances::free_balance(player);
        let receiver_before = Balances::free_balance(receiver);

        seed_owned_card_index(player, BaseCardCapacity::get(), 20_000);

        assert_noop!(
            EterraSlots::mint_card(RuntimeOrigin::signed(player)),
            Error::<Test>::CardCapacityExceeded
        );

        assert_eq!(Balances::free_balance(player), player_before);
        assert_eq!(Balances::free_balance(receiver), receiver_before);
    });
}

#[test]
fn buy_card_capacity_increases_capacity_and_charges_price() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let receiver = <Test as EterraSlotsConfig>::CardCapacityUpgradePriceReceiver::get();
        let price = <Test as EterraSlotsConfig>::CardCapacityUpgradePrice::get();

        let player_before = Balances::free_balance(player);
        let receiver_before = Balances::free_balance(receiver);

        assert_ok!(EterraSlots::buy_card_capacity(RuntimeOrigin::signed(
            player
        )));

        assert_eq!(
            CardCapacityBonus::<Test>::get(player),
            CardCapacityUpgradeAmount::get()
        );
        assert_eq!(Balances::free_balance(player), player_before - price);
        assert_eq!(Balances::free_balance(receiver), receiver_before + price);

        System::assert_has_event(RuntimeEvent::EterraSlots(Event::CardCapacityUpgraded {
            player,
            added_slots: CardCapacityUpgradeAmount::get(),
            new_capacity: BaseCardCapacity::get() + CardCapacityUpgradeAmount::get(),
            price_paid: price,
        }));
    });
}

#[test]
fn set_and_remove_price_updates_storage_and_events() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        let owner = 1u64;

        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(owner)));
        let card_id = NextCardId::<Test>::get().saturating_sub(1);

        // List for sale
        System::reset_events();
        assert_ok!(EterraSlots::set_price(
            RuntimeOrigin::signed(owner),
            card_id,
            500
        ));
        assert_eq!(CardPrices::<Test>::get(card_id), Some(500));
        assert!(ListedByOwner::<Test>::get(&owner).contains(&card_id));
        System::assert_has_event(RuntimeEvent::EterraSlots(Event::CardListed {
            owner,
            card_id,
            price: 500,
        }));

        // Unlist
        System::reset_events();
        assert_ok!(EterraSlots::remove_price(
            RuntimeOrigin::signed(owner),
            card_id
        ));
        assert_eq!(CardPrices::<Test>::get(card_id), None);
        assert!(!ListedByOwner::<Test>::get(&owner).contains(&card_id));
        System::assert_has_event(RuntimeEvent::EterraSlots(Event::CardUnlisted {
            owner,
            card_id,
        }));
    });
}

#[test]
fn transfer_card_auto_unlists() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        let owner = 1u64;
        let to = 2u64;

        // Mint and list
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(owner)));
        let card_id = NextCardId::<Test>::get().saturating_sub(1);
        assert_ok!(EterraSlots::set_price(
            RuntimeOrigin::signed(owner),
            card_id,
            777
        ));
        assert!(CardPrices::<Test>::get(card_id).is_some());

        // Transfer to `to`; should unlist
        assert_ok!(EterraSlots::transfer_card(
            RuntimeOrigin::signed(owner),
            card_id,
            to
        ));
        let card = EterraSlots::cards(card_id).unwrap();
        assert_eq!(*card.get_owner(), to);

        // Listing removed
        assert_eq!(CardPrices::<Test>::get(card_id), None);
        assert!(!ListedByOwner::<Test>::get(&owner).contains(&card_id));
    });
}

#[test]
fn buy_card_transfers_funds_and_ownership_then_unlists() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        let seller = 1u64;
        let buyer = 2u64;

        // Seller mints, lists at 200
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(seller)));
        let card_id = NextCardId::<Test>::get().saturating_sub(1);
        assert_ok!(EterraSlots::set_price(
            RuntimeOrigin::signed(seller),
            card_id,
            200
        ));

        let seller_before = Balances::free_balance(seller);
        let buyer_before = Balances::free_balance(buyer);

        // Buyer buys
        System::reset_events();
        assert_ok!(EterraSlots::buy_card(RuntimeOrigin::signed(buyer), card_id));

        // Ownership moved to buyer
        let card = EterraSlots::cards(card_id).unwrap();
        assert_eq!(*card.get_owner(), buyer);
        assert!(CardsByOwner::<Test>::get(&buyer).contains(&card_id));
        assert!(!CardsByOwner::<Test>::get(&seller).contains(&card_id));

        // Listing removed
        assert_eq!(CardPrices::<Test>::get(card_id), None);
        assert!(!ListedByOwner::<Test>::get(&seller).contains(&card_id));

        // Funds moved: buyer -200, seller +200
        let seller_after = Balances::free_balance(seller);
        let buyer_after = Balances::free_balance(buyer);
        assert_eq!(seller_after, seller_before + 200);
        assert_eq!(buyer_after, buyer_before - 200);

        System::assert_has_event(RuntimeEvent::EterraSlots(Event::CardBought {
            buyer,
            seller,
            card_id,
            price: 200,
        }));
    });
}

#[test]
fn buy_card_fails_if_not_listed() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        let seller = 1u64;
        let buyer = 2u64;

        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(seller)));
        let card_id = NextCardId::<Test>::get().saturating_sub(1);
        assert_noop!(
            EterraSlots::buy_card(RuntimeOrigin::signed(buyer), card_id),
            Error::<Test>::NotForSale
        );
    });
}

#[test]
fn mint_card_fails_when_card_ids_exhausted_without_charging_fee() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        let player = 1u64;
        let receiver = <Test as EterraSlotsConfig>::MintCardPriceReceiver::get();

        NextCardId::<Test>::put(u32::MAX);

        let receiver_before = Balances::free_balance(receiver);
        let player_before = Balances::free_balance(player);

        assert_noop!(
            EterraSlots::mint_card(RuntimeOrigin::signed(player)),
            Error::<Test>::CardIdExhausted
        );

        // Transactional rollback: no fee transfer should happen on ID exhaustion.
        assert_eq!(Balances::free_balance(receiver), receiver_before);
        assert_eq!(Balances::free_balance(player), player_before);
    });
}

#[test]
fn transfer_card_fails_when_not_finalized() {
    new_test_ext().execute_with(|| {
        let owner = 1u64;
        let to = 2u64;

        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(owner)));

        let packs = EterraSlots::player_packs(owner);
        let card_id = *packs[0]
            .get_card_ids()
            .first()
            .expect("At least one card expected");

        assert_noop!(
            EterraSlots::transfer_card(RuntimeOrigin::signed(owner), card_id, to),
            Error::<Test>::CardNotFinalized
        );
    });
}

#[test]
fn v2_profile_publication_rejects_directional_rank_regression_atomically() {
    use eterra_nexus_primitives::{
        CardRarity, ConversionPolicy, Element as V2Element, ElementProfile as V2ElementProfile,
        SubjectDefinitionV2, SubjectRarityProfile, SubjectRole,
    };

    new_test_ext().execute_with(|| {
        assert_ok!(EterraSlots::publish_subject_definition_v2(
            RuntimeOrigin::root(),
            SubjectDefinitionV2 {
                subject_definition_id: 77,
                subject_id: 77,
                subject_version: 1,
                role: SubjectRole::Hero,
                conversion_policy: ConversionPolicy::PlayableEmbodiment,
                element_profile: V2ElementProfile {
                    main: V2Element::Fire,
                    minor: None,
                    resistance: None,
                    weakness: Some(V2Element::Water),
                },
                display_metadata_id: 77,
                definition_hash: [77; 32],
                catalog_version: 1,
            }
        ));
        let rows = [
            (CardRarity::Common, [6, 5, 4, 3], None),
            // Individually valid (total 21), but north regresses 6 -> 5.
            (CardRarity::Rare, [5, 6, 5, 5], None),
            (CardRarity::Epic, [6, 7, 6, 5], None),
            (CardRarity::Legendary, [7, 8, 7, 5], None),
            (CardRarity::Mythical, [10, 8, 7, 5], Some(0)),
        ];
        let profiles = core::array::from_fn(|index| {
            let (rarity, base_ranks, apex_side) = rows[index];
            SubjectRarityProfile {
                profile_id: 770 + index as u32,
                subject_id: 77,
                subject_version: 1,
                rarity,
                base_ranks,
                apex_side,
                rarity_load: rarity.rarity_load(),
                profile_version: 1,
                profile_hash: [index as u8; 32],
            }
        });
        assert!(profiles.iter().all(SubjectRarityProfile::validate));

        assert_noop!(
            EterraSlots::publish_subject_rarity_profiles_v2(
                RuntimeOrigin::root(),
                77,
                1,
                profiles,
                1,
            ),
            Error::<Test>::V2InvalidProfiles
        );
        assert_eq!(crate::SubjectRarityProfilesV2::<Test>::iter().count(), 0);
        assert_eq!(
            crate::SubjectRarityProfileByKeyV2::<Test>::iter().count(),
            0
        );
    });
}

fn publish_v2_test_catalog() {
    use eterra_nexus_primitives::{
        CardRarity, ConversionPolicy, DiscoveryPolicy, Element as V2Element,
        ElementProfile as V2ElementProfile, MediaDefinitionV2, PackSkuVersion, SubjectDefinitionV2,
        SubjectRarityProfile, SubjectRole,
    };

    let rarities = [
        (CardRarity::Common, [5, 5, 4, 4], None),
        (CardRarity::Rare, [6, 5, 5, 5], None),
        (CardRarity::Epic, [6, 6, 6, 6], None),
        (CardRarity::Legendary, [7, 7, 7, 6], None),
        (CardRarity::Mythical, [10, 7, 7, 6], Some(0)),
    ];
    let mut profile_ids = Vec::new();
    let mut pose_ids = Vec::new();
    for subject_id in 1..=2u32 {
        let definition_id = subject_id;
        assert_ok!(EterraSlots::publish_subject_definition_v2(
            RuntimeOrigin::root(),
            SubjectDefinitionV2 {
                subject_definition_id: definition_id,
                subject_id,
                subject_version: 1,
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
        ));
        assert_ok!(EterraSlots::set_subject_activation_v2(
            RuntimeOrigin::root(),
            eterra_nexus_primitives::SubjectActivationState {
                subject_definition_id: definition_id,
                mint_enabled: true,
                conversion_enabled: true,
            }
        ));
        let profiles = core::array::from_fn(|index| {
            let (rarity, ranks, apex_side) = rarities[index];
            let profile_id = subject_id * 10 + index as u32;
            profile_ids.push(profile_id);
            SubjectRarityProfile {
                profile_id,
                subject_id,
                subject_version: 1,
                rarity,
                base_ranks: ranks,
                apex_side,
                rarity_load: rarity.rarity_load(),
                profile_version: 1,
                profile_hash: [profile_id as u8; 32],
            }
        });
        assert_ok!(EterraSlots::publish_subject_rarity_profiles_v2(
            RuntimeOrigin::root(),
            subject_id,
            1,
            profiles,
            1,
        ));
        for pose in 0..3u32 {
            let definition_id = subject_id * 10 + pose;
            pose_ids.push(definition_id);
            assert_ok!(EterraSlots::publish_pose_definition_v2(
                RuntimeOrigin::root(),
                MediaDefinitionV2 {
                    definition_id,
                    subject_id: Some(subject_id),
                    media_id: definition_id,
                    release_epoch: 1,
                    definition_hash: [definition_id as u8; 32],
                }
            ));
        }
    }
    let background_ids = vec![1000, 1001, 1002, 1003, 1004];
    for definition_id in background_ids.iter().copied() {
        assert_ok!(EterraSlots::publish_background_definition_v2(
            RuntimeOrigin::root(),
            MediaDefinitionV2 {
                definition_id,
                subject_id: None,
                media_id: definition_id,
                release_epoch: 1,
                definition_hash: [definition_id as u8; 32],
            }
        ));
    }
    assert_ok!(EterraSlots::publish_acquisition_pool_v2(
        RuntimeOrigin::root(),
        1,
        1,
        1,
        profile_ids,
        pose_ids,
        background_ids,
        [9; 32],
    ));
    assert_ok!(EterraSlots::publish_pack_sku_version_v2(
        RuntimeOrigin::root(),
        PackSkuVersion {
            pack_sku: 1,
            version: 1,
            card_count: 6,
            set_id: 1,
            pool_id: 1,
            pool_version: 1,
            rarity_weights: [6_800, 2_200, 750, 200, 50],
            discovery_policy: DiscoveryPolicy::Earned,
            odds_metadata_hash: [8; 32],
            immutable_config_hash: [9; 32],
            active_from: 1u64,
            active_until: None,
        }
    ));
    assert_ok!(EterraSlots::set_v2_feature_enabled(
        RuntimeOrigin::root(),
        crate::V2Feature::Packs,
        true,
    ));
    assert_ok!(EterraSlots::set_v2_feature_enabled(
        RuntimeOrigin::root(),
        crate::V2Feature::Conversion,
        true,
    ));
}

fn v2_draw_transcript(tag: u8) -> crate::V2DrawTranscript {
    crate::V2DrawTranscript {
        request_id: [tag; 32],
        immutable_config_hash: [tag.wrapping_add(1); 32],
        account_commitment: [tag.wrapping_add(2); 32],
        verified_randomness_output: [tag.wrapping_add(3); 32],
    }
}

fn seed_v2_conversion_safety_team(owner: u64, candidate_card_id: u64) -> Vec<u64> {
    use eterra_nexus_primitives::{
        CardInstanceV2, CardOriginV2, CardRarity, CardStateV2, ConversionPolicy,
        Element as V2Element, ElementProfile as V2ElementProfile, MediaDefinitionV2,
        SubjectDefinitionV2, SubjectRarityProfile, SubjectRole,
    };

    let candidate = crate::CardsV2::<Test>::get(candidate_card_id).expect("candidate exists");
    let rarities = [
        (CardRarity::Common, [5, 5, 4, 4], None),
        (CardRarity::Rare, [6, 5, 5, 5], None),
        (CardRarity::Epic, [6, 6, 6, 6], None),
        (CardRarity::Legendary, [7, 7, 7, 6], None),
        (CardRarity::Mythical, [10, 7, 7, 6], Some(0)),
    ];
    let mut card_ids = Vec::new();
    for subject_id in 3..=7u32 {
        assert_ok!(EterraSlots::publish_subject_definition_v2(
            RuntimeOrigin::root(),
            SubjectDefinitionV2 {
                subject_definition_id: subject_id,
                subject_id,
                subject_version: 1,
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
            },
        ));
        assert_ok!(EterraSlots::set_subject_activation_v2(
            RuntimeOrigin::root(),
            eterra_nexus_primitives::SubjectActivationState {
                subject_definition_id: subject_id,
                mint_enabled: true,
                conversion_enabled: true,
            },
        ));
        let profiles = core::array::from_fn(|index| {
            let (rarity, ranks, apex_side) = rarities[index];
            let profile_id = subject_id * 10 + index as u32;
            SubjectRarityProfile {
                profile_id,
                subject_id,
                subject_version: 1,
                rarity,
                base_ranks: ranks,
                apex_side,
                rarity_load: rarity.rarity_load(),
                profile_version: 1,
                profile_hash: [profile_id as u8; 32],
            }
        });
        assert_ok!(EterraSlots::publish_subject_rarity_profiles_v2(
            RuntimeOrigin::root(),
            subject_id,
            1,
            profiles,
            1,
        ));
        let pose_definition_id = subject_id * 10;
        assert_ok!(EterraSlots::publish_pose_definition_v2(
            RuntimeOrigin::root(),
            MediaDefinitionV2 {
                definition_id: pose_definition_id,
                subject_id: Some(subject_id),
                media_id: pose_definition_id,
                release_epoch: 1,
                definition_hash: [pose_definition_id as u8; 32],
            },
        ));

        let card_id = crate::NextCardIdV2::<Test>::get().max(1);
        crate::NextCardIdV2::<Test>::put(card_id + 1);
        crate::CardsV2::<Test>::insert(
            card_id,
            CardInstanceV2 {
                card_id,
                owner,
                set_id: candidate.set_id,
                season_id: candidate.season_id,
                subject_id,
                subject_version: 1,
                rarity: CardRarity::Common,
                profile_id: subject_id * 10,
                pose_definition_id,
                background_definition_id: 1000,
                serial_number: 1,
                economic_realm: candidate.economic_realm,
                origin: CardOriginV2::Tutorial {
                    tutorial_id: [subject_id as u8; 32],
                },
                acquisition_id: [subject_id as u8; 32],
                pool_id: candidate.pool_id,
                pool_version: candidate.pool_version,
                state: CardStateV2::Active,
                acquired_at: System::block_number(),
            },
        );
        crate::LiveSupplyBySubjectRarityV2::<Test>::mutate(
            (subject_id, CardRarity::Common, candidate.economic_realm),
            |count| *count += 1,
        );
        card_ids.push(card_id);
    }
    crate::V2OwnerCardCount::<Test>::mutate(owner, |count| *count += 5);
    crate::V2OwnerActiveCardCount::<Test>::mutate(owner, |count| *count += 5);
    assert_ok!(EterraSlots::publish_competitive_format_v2(
        RuntimeOrigin::root(),
        crate::CompetitiveFormatV2 {
            format_id: 9001,
            version: 1,
            set_id: candidate.set_id,
            team_size: 5,
            rarity_load_budget: 15,
            max_mythical: 1,
            max_legendary_or_better: 2,
            rules_hash: [0x90; 32],
        },
    ));
    assert_ok!(EterraSlots::set_v2_feature_enabled(
        RuntimeOrigin::root(),
        crate::V2Feature::Ranked,
        true,
    ));
    assert_ok!(EterraSlots::save_competitive_team_v2(
        RuntimeOrigin::signed(owner),
        9001,
        9001,
        1,
        card_ids.clone().try_into().expect("five cards fit"),
    ));
    card_ids
}

fn open_v2_training_pack(
    owner: u64,
    tag: u8,
) -> BoundedVec<u64, frame_support::traits::ConstU32<6>> {
    assert_ok!(EterraSlots::issue_training_pack_credit_v2(
        RuntimeOrigin::root(),
        owner,
        1,
        1,
        [tag; 32],
    ));
    assert_ok!(EterraSlots::request_pack_open_v2(
        RuntimeOrigin::signed(owner),
        1,
        1,
        eterra_nexus_primitives::EconomicRealm::Training,
        [tag.wrapping_add(1); 32],
    ));
    let opening_id = crate::PendingPackOpeningsV2::<Test>::iter_keys()
        .next()
        .expect("pending opening");
    finalize_random_request(last_random_request(), [tag.wrapping_add(2); 32]);
    assert_ok!(EterraSlots::finalize_pack_open_v2(
        RuntimeOrigin::signed(owner),
        opening_id,
    ));
    crate::ProcessedAcquisitionsV2::<Test>::get(opening_id).expect("processed opening")
}

const ASCENSION_SEASON_ID: u32 = 1;
const ASCENSION_SUBJECT_ID: u32 = 1;
const ASCENSION_ELIGIBILITY_ID: [u8; 32] = [0x41; 32];

fn configure_v2_ascension(accounts: &[u64]) {
    publish_v2_test_catalog();
    assert_ok!(EterraSlots::configure_mythical_ascension_season_v2(
        RuntimeOrigin::root(),
        crate::MythicalAscensionSeasonConfig {
            season_id: ASCENSION_SEASON_ID,
            set_id: 1,
            pool_id: 1,
            pool_version: 1,
            starts_at: 10,
            ends_at: 100,
            required_mastery: 10,
            required_marks: 10,
            available_weeks: 12,
            config_hash: [0x51; 32],
        },
    ));
    assert_ok!(EterraSlots::configure_mythical_ascension_subject_v2(
        RuntimeOrigin::root(),
        crate::MythicalAscensionSubjectConfig {
            season_id: ASCENSION_SEASON_ID,
            subject_id: ASCENSION_SUBJECT_ID,
            subject_version: 1,
            foundation_pose_definition_id: 10,
            foundation_background_definition_id: 1000,
            config_hash: [0x52; 32],
        },
    ));
    for account in accounts {
        assert_ok!(EterraSlots::link_season_eligibility_v2(
            RuntimeOrigin::root(),
            *account,
            ASCENSION_SEASON_ID,
            ASCENSION_ELIGIBILITY_ID,
        ));
    }
}

fn record_v2_ascension_progress(last_week: u8) {
    for week in 0..=last_week {
        System::set_block_number(10 + u64::from(week) * 7);
        assert_ok!(EterraSlots::record_mythical_ascension_progress_v2(
            RuntimeOrigin::root(),
            ASCENSION_ELIGIBILITY_ID,
            ASCENSION_SEASON_ID,
            ASCENSION_SUBJECT_ID,
            eterra_nexus_primitives::EconomicRealm::Production,
            (week == 0).then_some(10),
            Some(week),
            week == 0,
            [0x60u8.saturating_add(week); 32],
        ));
    }
}

fn seed_v2_production_legendary(owner: u64) -> u64 {
    use eterra_nexus_primitives::{
        CardInstanceV2, CardOriginV2, CardRarity, CardStateV2, EconomicRealm,
    };

    let card_id = crate::NextCardIdV2::<Test>::get().max(1);
    crate::NextCardIdV2::<Test>::put(card_id + 1);
    crate::NextSerialV2::<Test>::insert((ASCENSION_SUBJECT_ID, CardRarity::Legendary), 1);
    crate::CardsV2::<Test>::insert(
        card_id,
        CardInstanceV2 {
            card_id,
            owner,
            set_id: 1,
            season_id: 1,
            subject_id: ASCENSION_SUBJECT_ID,
            subject_version: 1,
            rarity: CardRarity::Legendary,
            profile_id: 13,
            pose_definition_id: 11,
            background_definition_id: 1001,
            serial_number: 1,
            economic_realm: EconomicRealm::Production,
            origin: CardOriginV2::Pack {
                opening_id: [0x71; 32],
                slot: 0,
            },
            acquisition_id: [0x72; 32],
            pool_id: 1,
            pool_version: 1,
            state: CardStateV2::Active,
            acquired_at: 1,
        },
    );
    crate::V2OwnerCardCount::<Test>::insert(owner, 1);
    crate::V2OwnerActiveCardCount::<Test>::insert(owner, 1);
    crate::LiveSupplyBySubjectRarityV2::<Test>::insert(
        (
            ASCENSION_SUBJECT_ID,
            CardRarity::Legendary,
            EconomicRealm::Production,
        ),
        1,
    );
    card_id
}

#[test]
fn v2_mythical_ascension_foundation_path_is_atomic_production_and_idempotent() {
    use eterra_nexus_primitives::{CardOriginV2, CardRarity, EconomicRealm};

    new_test_ext().execute_with(|| {
        configure_v2_ascension(&[1]);
        record_v2_ascension_progress(9);
        assert!(!crate::V2FeatureEnabled::<Test>::get(
            crate::V2Feature::MythicalAscension
        ));
        assert_noop!(
            EterraSlots::ascend_mythical_v2(
                RuntimeOrigin::signed(1),
                ASCENSION_SEASON_ID,
                ASCENSION_SUBJECT_ID,
                crate::MythicalAscensionInput::LegendaryFoundation,
            ),
            Error::<Test>::V2FeatureDisabled
        );
        assert_ok!(EterraSlots::set_v2_feature_enabled(
            RuntimeOrigin::root(),
            crate::V2Feature::MythicalAscension,
            true,
        ));

        assert_ok!(EterraSlots::ascend_mythical_v2(
            RuntimeOrigin::signed(1),
            ASCENSION_SEASON_ID,
            ASCENSION_SUBJECT_ID,
            crate::MythicalAscensionInput::LegendaryFoundation,
        ));
        let output_id = 1;
        let output = crate::CardsV2::<Test>::get(output_id).expect("mythical output");
        assert_eq!(output.owner, 1);
        assert_eq!(output.rarity, CardRarity::Mythical);
        assert_eq!(output.economic_realm, EconomicRealm::Production);
        assert_eq!(output.pose_definition_id, 10);
        assert_eq!(output.background_definition_id, 1000);
        assert!(matches!(
            output.origin,
            CardOriginV2::MythicalAscension { .. }
        ));
        assert_eq!(
            crate::V2CardAccountBoundUntil::<Test>::get(output_id),
            Some(100)
        );
        assert_eq!(crate::V2OwnerCardCount::<Test>::get(1), 1);
        assert_eq!(crate::V2OwnerActiveCardCount::<Test>::get(1), 1);
        assert_eq!(
            crate::LiveSupplyBySubjectRarityV2::<Test>::get((
                ASCENSION_SUBJECT_ID,
                CardRarity::Mythical,
                EconomicRealm::Production,
            )),
            1
        );
        assert!(!crate::LegendaryFoundationsV2::<Test>::get((
            ASCENSION_ELIGIBILITY_ID,
            ASCENSION_SEASON_ID,
            ASCENSION_SUBJECT_ID,
        )));
        assert_eq!(
            crate::ConvergenceProgressV2::<Test>::get((
                ASCENSION_ELIGIBILITY_ID,
                ASCENSION_SEASON_ID,
            )),
            crate::ConvergenceProgress::default()
        );
        assert!(!crate::MythicCatalystsV2::<Test>::get((
            ASCENSION_ELIGIBILITY_ID,
            ASCENSION_SEASON_ID,
        )));

        let profile_bitmap = crate::PackProtectionHistoryBitmapsV2::<Test>::get(1, 1);
        assert_ne!(profile_bitmap[0] & (1 << 4), 0);
        let cosmetic_bit = EterraSlots::cosmetic_protection_bit(
            1,
            ASCENSION_SUBJECT_ID,
            CardRarity::Mythical,
            10,
            1000,
        )
        .unwrap();
        let cosmetic_bitmap = crate::CosmeticProtectionBitmapsV2::<Test>::get(1, 1);
        assert_ne!(
            cosmetic_bitmap[cosmetic_bit / 8] & (1 << (cosmetic_bit % 8)),
            0
        );

        let ascension_id = crate::MythicalAscensionByEligibilityV2::<Test>::get((
            ASCENSION_ELIGIBILITY_ID,
            ASCENSION_SEASON_ID,
        ))
        .expect("ascension recorded");
        let receipt =
            crate::MythicalAscensionReceiptsV2::<Test>::get(ascension_id).expect("receipt");
        assert_eq!(receipt.output_card_id, output_id);
        assert_eq!(
            receipt.input,
            crate::MythicalAscensionInput::LegendaryFoundation
        );

        assert_ok!(EterraSlots::ascend_mythical_v2(
            RuntimeOrigin::signed(1),
            ASCENSION_SEASON_ID,
            ASCENSION_SUBJECT_ID,
            crate::MythicalAscensionInput::LegendaryFoundation,
        ));
        assert_eq!(crate::V2OwnerCardCount::<Test>::get(1), 1);
        assert_eq!(
            crate::LiveSupplyBySubjectRarityV2::<Test>::get((
                ASCENSION_SUBJECT_ID,
                CardRarity::Mythical,
                EconomicRealm::Production,
            )),
            1
        );
        assert_eq!(
            crate::MythicalAscensionReceiptsV2::<Test>::iter().count(),
            1
        );
    });
}

#[test]
fn v2_mythical_ascension_respects_pending_pack_capacity_without_consuming_inputs() {
    new_test_ext().execute_with(|| {
        configure_v2_ascension(&[1]);
        record_v2_ascension_progress(9);
        assert_ok!(EterraSlots::set_v2_feature_enabled(
            RuntimeOrigin::root(),
            crate::V2Feature::MythicalAscension,
            true,
        ));
        crate::V2OwnerCardCount::<Test>::insert(1, 9_999);
        crate::ReservedV2PackCardCount::<Test>::insert(1, 1);

        assert_noop!(
            EterraSlots::ascend_mythical_v2(
                RuntimeOrigin::signed(1),
                ASCENSION_SEASON_ID,
                ASCENSION_SUBJECT_ID,
                crate::MythicalAscensionInput::LegendaryFoundation,
            ),
            Error::<Test>::V2OperationalCardLimitReached
        );
        assert!(crate::LegendaryFoundationsV2::<Test>::get((
            ASCENSION_ELIGIBILITY_ID,
            ASCENSION_SEASON_ID,
            ASCENSION_SUBJECT_ID,
        )));
        assert!(crate::MythicCatalystsV2::<Test>::get((
            ASCENSION_ELIGIBILITY_ID,
            ASCENSION_SEASON_ID,
        )));
        assert_eq!(
            crate::MythicalAscensionReceiptsV2::<Test>::iter().count(),
            0
        );
        assert_eq!(crate::CardsV2::<Test>::iter().count(), 0);
    });
}

#[test]
fn v2_mythical_ascension_legendary_path_tombstones_and_carries_cosmetics() {
    use eterra_nexus_primitives::{CardRarity, CardStateV2, EconomicRealm};

    new_test_ext().execute_with(|| {
        configure_v2_ascension(&[1]);
        let source_id = seed_v2_production_legendary(1);
        record_v2_ascension_progress(9);
        assert_ok!(EterraSlots::set_v2_feature_enabled(
            RuntimeOrigin::root(),
            crate::V2Feature::MythicalAscension,
            true,
        ));

        assert_ok!(EterraSlots::ascend_mythical_v2(
            RuntimeOrigin::signed(1),
            ASCENSION_SEASON_ID,
            ASCENSION_SUBJECT_ID,
            crate::MythicalAscensionInput::LegendaryCard { card_id: source_id },
        ));
        let output_id = 2;
        let source = crate::CardsV2::<Test>::get(source_id).unwrap();
        assert_eq!(
            source.state,
            CardStateV2::MythicalAscended {
                output_card_id: output_id,
            }
        );
        let output = crate::CardsV2::<Test>::get(output_id).unwrap();
        assert_eq!(output.pose_definition_id, source.pose_definition_id);
        assert_eq!(
            output.background_definition_id,
            source.background_definition_id
        );
        assert_eq!(output.economic_realm, EconomicRealm::Production);
        assert_eq!(crate::V2OwnerCardCount::<Test>::get(1), 2);
        assert_eq!(crate::V2OwnerActiveCardCount::<Test>::get(1), 1);
        assert_eq!(
            crate::LiveSupplyBySubjectRarityV2::<Test>::get((
                ASCENSION_SUBJECT_ID,
                CardRarity::Legendary,
                EconomicRealm::Production,
            )),
            0
        );
        assert_eq!(
            crate::LiveSupplyBySubjectRarityV2::<Test>::get((
                ASCENSION_SUBJECT_ID,
                CardRarity::Mythical,
                EconomicRealm::Production,
            )),
            1
        );
    });
}

#[test]
fn v2_mythical_ascension_is_shared_across_linked_wallets() {
    new_test_ext().execute_with(|| {
        configure_v2_ascension(&[1, 2]);
        record_v2_ascension_progress(9);
        assert_ok!(EterraSlots::set_v2_feature_enabled(
            RuntimeOrigin::root(),
            crate::V2Feature::MythicalAscension,
            true,
        ));
        assert_ok!(EterraSlots::ascend_mythical_v2(
            RuntimeOrigin::signed(1),
            ASCENSION_SEASON_ID,
            ASCENSION_SUBJECT_ID,
            crate::MythicalAscensionInput::LegendaryFoundation,
        ));
        assert_noop!(
            EterraSlots::ascend_mythical_v2(
                RuntimeOrigin::signed(2),
                ASCENSION_SEASON_ID,
                ASCENSION_SUBJECT_ID,
                crate::MythicalAscensionInput::LegendaryFoundation,
            ),
            Error::<Test>::V2AscensionAlreadyCompleted
        );
        assert_eq!(crate::V2OwnerCardCount::<Test>::get(2), 0);
    });
}

#[test]
fn v2_ascension_progress_rejects_training_wrong_duplicate_and_out_of_range_weeks() {
    new_test_ext().execute_with(|| {
        configure_v2_ascension(&[1]);
        System::set_block_number(10);
        assert_noop!(
            EterraSlots::record_mythical_ascension_progress_v2(
                RuntimeOrigin::root(),
                ASCENSION_ELIGIBILITY_ID,
                ASCENSION_SEASON_ID,
                ASCENSION_SUBJECT_ID,
                eterra_nexus_primitives::EconomicRealm::Training,
                Some(10),
                Some(0),
                true,
                [0x80; 32],
            ),
            Error::<Test>::V2AscensionNotActive
        );
        assert_noop!(
            EterraSlots::record_mythical_ascension_progress_v2(
                RuntimeOrigin::root(),
                ASCENSION_ELIGIBILITY_ID,
                ASCENSION_SEASON_ID,
                ASCENSION_SUBJECT_ID,
                eterra_nexus_primitives::EconomicRealm::Production,
                None,
                Some(1),
                false,
                [0x81; 32],
            ),
            Error::<Test>::V2AscensionWeekInvalid
        );
        assert_ok!(EterraSlots::record_mythical_ascension_progress_v2(
            RuntimeOrigin::root(),
            ASCENSION_ELIGIBILITY_ID,
            ASCENSION_SEASON_ID,
            ASCENSION_SUBJECT_ID,
            eterra_nexus_primitives::EconomicRealm::Production,
            Some(10),
            Some(0),
            true,
            [0x82; 32],
        ));
        assert_noop!(
            EterraSlots::record_mythical_ascension_progress_v2(
                RuntimeOrigin::root(),
                ASCENSION_ELIGIBILITY_ID,
                ASCENSION_SEASON_ID,
                ASCENSION_SUBJECT_ID,
                eterra_nexus_primitives::EconomicRealm::Production,
                None,
                Some(0),
                false,
                [0x83; 32],
            ),
            Error::<Test>::V2AscensionWeekAlreadyCredited
        );
        System::set_block_number(94);
        assert_noop!(
            EterraSlots::record_mythical_ascension_progress_v2(
                RuntimeOrigin::root(),
                ASCENSION_ELIGIBILITY_ID,
                ASCENSION_SEASON_ID,
                ASCENSION_SUBJECT_ID,
                eterra_nexus_primitives::EconomicRealm::Production,
                None,
                Some(12),
                false,
                [0x84; 32],
            ),
            Error::<Test>::V2AscensionWeekInvalid
        );
    });
}

#[test]
fn v2_ascension_week_and_season_boundaries_are_chain_derived() {
    new_test_ext().execute_with(|| {
        configure_v2_ascension(&[1]);
        System::set_block_number(9);
        assert_noop!(
            EterraSlots::record_mythical_ascension_progress_v2(
                RuntimeOrigin::root(),
                ASCENSION_ELIGIBILITY_ID,
                ASCENSION_SEASON_ID,
                ASCENSION_SUBJECT_ID,
                eterra_nexus_primitives::EconomicRealm::Production,
                Some(10),
                None,
                false,
                [0x85; 32],
            ),
            Error::<Test>::V2AscensionNotActive
        );
        System::set_block_number(87);
        assert_ok!(EterraSlots::record_mythical_ascension_progress_v2(
            RuntimeOrigin::root(),
            ASCENSION_ELIGIBILITY_ID,
            ASCENSION_SEASON_ID,
            ASCENSION_SUBJECT_ID,
            eterra_nexus_primitives::EconomicRealm::Production,
            Some(10),
            Some(11),
            true,
            [0x86; 32],
        ));
        System::set_block_number(100);
        assert_noop!(
            EterraSlots::record_mythical_ascension_progress_v2(
                RuntimeOrigin::root(),
                ASCENSION_ELIGIBILITY_ID,
                ASCENSION_SEASON_ID,
                ASCENSION_SUBJECT_ID,
                eterra_nexus_primitives::EconomicRealm::Production,
                None,
                None,
                true,
                [0x87; 32],
            ),
            Error::<Test>::V2AscensionNotActive
        );
    });
}

#[test]
fn v2_ascension_failure_rolls_back_all_inputs() {
    new_test_ext().execute_with(|| {
        configure_v2_ascension(&[1]);
        record_v2_ascension_progress(9);
        assert_ok!(EterraSlots::set_v2_feature_enabled(
            RuntimeOrigin::root(),
            crate::V2Feature::MythicalAscension,
            true,
        ));
        crate::NextCardIdV2::<Test>::put(u64::MAX);

        assert_noop!(
            EterraSlots::ascend_mythical_v2(
                RuntimeOrigin::signed(1),
                ASCENSION_SEASON_ID,
                ASCENSION_SUBJECT_ID,
                crate::MythicalAscensionInput::LegendaryFoundation,
            ),
            Error::<Test>::V2CardIdExhausted
        );
        assert!(crate::LegendaryFoundationsV2::<Test>::get((
            ASCENSION_ELIGIBILITY_ID,
            ASCENSION_SEASON_ID,
            ASCENSION_SUBJECT_ID,
        )));
        assert_eq!(
            crate::ConvergenceProgressV2::<Test>::get((
                ASCENSION_ELIGIBILITY_ID,
                ASCENSION_SEASON_ID,
            ))
            .marks_earned,
            10
        );
        assert!(crate::MythicCatalystsV2::<Test>::get((
            ASCENSION_ELIGIBILITY_ID,
            ASCENSION_SEASON_ID,
        )));
        assert_eq!(crate::CardsV2::<Test>::iter().count(), 0);
    });
}

#[test]
fn v2_ascension_requires_mastery_distinct_marks_catalyst_and_foundation() {
    new_test_ext().execute_with(|| {
        configure_v2_ascension(&[1]);
        System::set_block_number(10);
        assert_ok!(EterraSlots::set_v2_feature_enabled(
            RuntimeOrigin::root(),
            crate::V2Feature::MythicalAscension,
            true,
        ));
        let mastery_key = (
            ASCENSION_ELIGIBILITY_ID,
            ASCENSION_SEASON_ID,
            ASCENSION_SUBJECT_ID,
        );
        let convergence_key = (ASCENSION_ELIGIBILITY_ID, ASCENSION_SEASON_ID);
        crate::LegendaryFoundationsV2::<Test>::insert(mastery_key, true);
        crate::MythicCatalystsV2::<Test>::insert(convergence_key, true);
        crate::ConvergenceProgressV2::<Test>::insert(
            convergence_key,
            crate::ConvergenceProgress {
                marks_earned: 10,
                credited_week_bitmap: 0x03ff,
            },
        );
        crate::MythicalSubjectMasteryV2::<Test>::insert(mastery_key, 9);
        assert_noop!(
            EterraSlots::ascend_mythical_v2(
                RuntimeOrigin::signed(1),
                ASCENSION_SEASON_ID,
                ASCENSION_SUBJECT_ID,
                crate::MythicalAscensionInput::LegendaryFoundation,
            ),
            Error::<Test>::V2AscensionRequirementsMissing
        );

        crate::MythicalSubjectMasteryV2::<Test>::insert(mastery_key, 10);
        crate::ConvergenceProgressV2::<Test>::insert(
            convergence_key,
            crate::ConvergenceProgress {
                marks_earned: 9,
                credited_week_bitmap: 0x01ff,
            },
        );
        assert_noop!(
            EterraSlots::ascend_mythical_v2(
                RuntimeOrigin::signed(1),
                ASCENSION_SEASON_ID,
                ASCENSION_SUBJECT_ID,
                crate::MythicalAscensionInput::LegendaryFoundation,
            ),
            Error::<Test>::V2AscensionRequirementsMissing
        );

        crate::ConvergenceProgressV2::<Test>::insert(
            convergence_key,
            crate::ConvergenceProgress {
                marks_earned: 10,
                credited_week_bitmap: 0x01ff,
            },
        );
        assert_noop!(
            EterraSlots::ascend_mythical_v2(
                RuntimeOrigin::signed(1),
                ASCENSION_SEASON_ID,
                ASCENSION_SUBJECT_ID,
                crate::MythicalAscensionInput::LegendaryFoundation,
            ),
            Error::<Test>::V2AscensionRequirementsMissing
        );

        crate::ConvergenceProgressV2::<Test>::insert(
            convergence_key,
            crate::ConvergenceProgress {
                marks_earned: 10,
                credited_week_bitmap: 0x03ff,
            },
        );
        crate::MythicCatalystsV2::<Test>::insert(convergence_key, false);
        assert_noop!(
            EterraSlots::ascend_mythical_v2(
                RuntimeOrigin::signed(1),
                ASCENSION_SEASON_ID,
                ASCENSION_SUBJECT_ID,
                crate::MythicalAscensionInput::LegendaryFoundation,
            ),
            Error::<Test>::V2AscensionRequirementsMissing
        );

        crate::MythicCatalystsV2::<Test>::insert(convergence_key, true);
        crate::LegendaryFoundationsV2::<Test>::insert(mastery_key, false);
        assert_noop!(
            EterraSlots::ascend_mythical_v2(
                RuntimeOrigin::signed(1),
                ASCENSION_SEASON_ID,
                ASCENSION_SUBJECT_ID,
                crate::MythicalAscensionInput::LegendaryFoundation,
            ),
            Error::<Test>::V2AscensionFoundationMissing
        );
        assert_eq!(crate::CardsV2::<Test>::iter().count(), 0);
    });
}

#[test]
fn v2_ascension_is_active_only_inside_season_and_output_is_bound_through_end() {
    new_test_ext().execute_with(|| {
        set_mock_randomness_mode(pallet_eterra_randomness::RandomnessMode::DrandQuicknet);
        configure_v2_ascension(&[1]);
        record_v2_ascension_progress(9);
        assert_ok!(EterraSlots::set_v2_feature_enabled(
            RuntimeOrigin::root(),
            crate::V2Feature::MythicalAscension,
            true,
        ));
        System::set_block_number(9);
        assert_noop!(
            EterraSlots::ascend_mythical_v2(
                RuntimeOrigin::signed(1),
                ASCENSION_SEASON_ID,
                ASCENSION_SUBJECT_ID,
                crate::MythicalAscensionInput::LegendaryFoundation,
            ),
            Error::<Test>::V2AscensionNotActive
        );
        System::set_block_number(100);
        assert_noop!(
            EterraSlots::ascend_mythical_v2(
                RuntimeOrigin::signed(1),
                ASCENSION_SEASON_ID,
                ASCENSION_SUBJECT_ID,
                crate::MythicalAscensionInput::LegendaryFoundation,
            ),
            Error::<Test>::V2AscensionNotActive
        );
        System::set_block_number(99);
        assert_ok!(EterraSlots::ascend_mythical_v2(
            RuntimeOrigin::signed(1),
            ASCENSION_SEASON_ID,
            ASCENSION_SUBJECT_ID,
            crate::MythicalAscensionInput::LegendaryFoundation,
        ));
        let output_id = 1;
        assert_noop!(
            EterraSlots::request_conversion_v2(RuntimeOrigin::signed(1), output_id, 1, [0x91; 32],),
            Error::<Test>::V2ConversionNotAllowed
        );

        // The account binding expires at the exclusive season end.
        System::set_block_number(100);
        seed_v2_conversion_safety_team(1, output_id);
        assert_ok!(EterraSlots::request_conversion_v2(
            RuntimeOrigin::signed(1),
            output_id,
            1,
            [0x92; 32],
        ));
    });
}

#[test]
fn v2_ascension_progress_evidence_is_idempotent_but_conflicts_fail_closed() {
    new_test_ext().execute_with(|| {
        configure_v2_ascension(&[1]);
        System::set_block_number(10);
        let evidence_id = [0x93; 32];
        assert_ok!(EterraSlots::record_mythical_ascension_progress_v2(
            RuntimeOrigin::root(),
            ASCENSION_ELIGIBILITY_ID,
            ASCENSION_SEASON_ID,
            ASCENSION_SUBJECT_ID,
            eterra_nexus_primitives::EconomicRealm::Production,
            Some(10),
            Some(0),
            true,
            evidence_id,
        ));
        assert_ok!(EterraSlots::record_mythical_ascension_progress_v2(
            RuntimeOrigin::root(),
            ASCENSION_ELIGIBILITY_ID,
            ASCENSION_SEASON_ID,
            ASCENSION_SUBJECT_ID,
            eterra_nexus_primitives::EconomicRealm::Production,
            Some(10),
            Some(0),
            true,
            evidence_id,
        ));
        assert_noop!(
            EterraSlots::record_mythical_ascension_progress_v2(
                RuntimeOrigin::root(),
                ASCENSION_ELIGIBILITY_ID,
                ASCENSION_SEASON_ID,
                ASCENSION_SUBJECT_ID,
                eterra_nexus_primitives::EconomicRealm::Production,
                Some(9),
                None,
                false,
                evidence_id,
            ),
            Error::<Test>::V2AscensionProgressEvidenceConflict
        );
        assert_eq!(
            crate::ConvergenceProgressV2::<Test>::get((
                ASCENSION_ELIGIBILITY_ID,
                ASCENSION_SEASON_ID,
            ))
            .marks_earned,
            1
        );
    });
}

#[test]
fn v2_ascension_season_configuration_requires_the_exact_duration() {
    new_test_ext().execute_with(|| {
        publish_v2_test_catalog();
        for (season_id, ends_at) in [(2, 99), (3, 101)] {
            assert_noop!(
                EterraSlots::configure_mythical_ascension_season_v2(
                    RuntimeOrigin::root(),
                    crate::MythicalAscensionSeasonConfig {
                        season_id,
                        set_id: 1,
                        pool_id: 1,
                        pool_version: 1,
                        starts_at: 10,
                        ends_at,
                        required_mastery: 10,
                        required_marks: 10,
                        available_weeks: 12,
                        config_hash: [season_id as u8; 32],
                    },
                ),
                Error::<Test>::V2AscensionConfigInvalid
            );
        }
    });
}

#[test]
fn v2_protection_layout_survives_pool_reordering_and_is_rarity_scoped() {
    new_test_ext().execute_with(|| {
        publish_v2_test_catalog();
        assert_eq!(crate::SubjectProtectionSlotsV2::<Test>::get(1, 1), Some(0));
        assert_eq!(crate::SubjectProtectionSlotsV2::<Test>::get(1, 2), Some(1));
        assert_eq!(crate::PoseProtectionSlotsV2::<Test>::get(1, 10), Some(0));
        assert_eq!(
            crate::BackgroundProtectionSlotsV2::<Test>::get(1, 1000),
            Some(0)
        );

        let mut profile_ids = vec![10, 11, 12, 13, 14, 20, 21, 22, 23, 24];
        profile_ids.reverse();
        let mut pose_ids = vec![10, 11, 12, 20, 21, 22];
        pose_ids.reverse();
        let mut background_ids = vec![1000, 1001, 1002, 1003, 1004];
        background_ids.reverse();
        assert_ok!(EterraSlots::publish_acquisition_pool_v2(
            RuntimeOrigin::root(),
            1,
            2,
            1,
            profile_ids,
            pose_ids,
            background_ids,
            [7; 32],
        ));

        assert_eq!(crate::SubjectProtectionSlotsV2::<Test>::get(1, 1), Some(0));
        assert_eq!(crate::SubjectProtectionSlotsV2::<Test>::get(1, 2), Some(1));
        assert_eq!(
            EterraSlots::cosmetic_protection_bit(
                1,
                2,
                eterra_nexus_primitives::CardRarity::Common,
                20,
                1000,
            )
            .unwrap(),
            75
        );
        let subject_one_legendary_cosmetic_bit = EterraSlots::cosmetic_protection_bit(
            1,
            1,
            eterra_nexus_primitives::CardRarity::Legendary,
            10,
            1000,
        )
        .unwrap();
        assert_eq!(subject_one_legendary_cosmetic_bit, 45);

        // A later pool may omit a subject without recycling or renumbering any
        // of that subject's stable protection slots.
        assert_ok!(EterraSlots::publish_acquisition_pool_v2(
            RuntimeOrigin::root(),
            1,
            3,
            1,
            vec![20, 21, 22, 23, 24],
            vec![22, 20, 21],
            vec![1004, 1002, 1000, 1003, 1001],
            [6; 32],
        ));
        assert_eq!(crate::SubjectProtectionSlotsV2::<Test>::get(1, 1), Some(0));
        assert_eq!(crate::PoseProtectionSlotsV2::<Test>::get(1, 10), Some(0));
        assert_eq!(
            EterraSlots::cosmetic_protection_bit(
                1,
                1,
                eterra_nexus_primitives::CardRarity::Legendary,
                10,
                1000,
            )
            .unwrap(),
            subject_one_legendary_cosmetic_bit,
        );

        // Re-adding the omitted subject in another order retains all 5×3×5
        // rarity/cosmetic coordinates.
        assert_ok!(EterraSlots::publish_acquisition_pool_v2(
            RuntimeOrigin::root(),
            1,
            4,
            1,
            vec![24, 14, 23, 13, 22, 12, 21, 11, 20, 10],
            vec![21, 12, 20, 11, 22, 10],
            vec![1001, 1003, 1000, 1004, 1002],
            [5; 32],
        ));
        assert_eq!(crate::NextSubjectProtectionSlotV2::<Test>::get(1), 2);
        assert_eq!(crate::NextPoseProtectionSlotV2::<Test>::get((1, 1)), 3);
        assert_eq!(crate::NextBackgroundProtectionSlotV2::<Test>::get(1), 5);
        assert_eq!(
            EterraSlots::cosmetic_protection_bit(
                1,
                1,
                eterra_nexus_primitives::CardRarity::Legendary,
                10,
                1000,
            )
            .unwrap(),
            subject_one_legendary_cosmetic_bit,
        );

        let pool = crate::AcquisitionPoolVersionsV2::<Test>::get((1, 2)).unwrap();
        let mut protection = crate::PackProtectionHistoryBitmapsV2::<Test>::get(1, 1);
        while protection.len() < 2 {
            protection.try_push(0).unwrap();
        }
        // Subject 1 Common is known, while Subject 2 Legendary is known.
        // A Legendary discovery slot must still choose Subject 1 because
        // Legendary/Mythical protection is scoped to the rolled rarity row.
        protection[0] |= 1 << 0;
        protection[1] |= 1 << 0;
        let transcript = v2_draw_transcript(3);
        let (_, selected) = EterraSlots::select_profile_for_pack(
            &pool,
            1,
            eterra_nexus_primitives::CardRarity::Legendary,
            &transcript,
            0,
            &[],
            &protection,
            true,
        )
        .unwrap();
        assert_eq!(selected.subject_id, 1);
        assert_eq!(
            EterraSlots::profile_protection_bit(1, &selected).unwrap(),
            3
        );
    });
}

#[test]
fn v2_protection_layout_requires_three_poses_and_five_backgrounds() {
    new_test_ext().execute_with(|| {
        publish_v2_test_catalog();
        let profiles = vec![10, 11, 12, 13, 14];
        assert_noop!(
            EterraSlots::publish_acquisition_pool_v2(
                RuntimeOrigin::root(),
                2,
                1,
                1,
                profiles.clone(),
                vec![10, 11],
                vec![1000, 1001, 1002, 1003, 1004],
                [4; 32],
            ),
            Error::<Test>::V2InvalidPool
        );
        assert_noop!(
            EterraSlots::publish_acquisition_pool_v2(
                RuntimeOrigin::root(),
                2,
                2,
                1,
                profiles,
                vec![10, 11, 12],
                vec![1000, 1001, 1002, 1003],
                [3; 32],
            ),
            Error::<Test>::V2InvalidPool
        );
    });
}

#[test]
fn v2_pool_requires_a_pinned_background_eligible_for_every_subject() {
    new_test_ext().execute_with(|| {
        use eterra_nexus_primitives::MediaDefinitionV2;

        publish_v2_test_catalog();
        let background_ids = (1100..1105).collect::<Vec<_>>();
        for definition_id in background_ids.iter().copied() {
            assert_ok!(EterraSlots::publish_background_definition_v2(
                RuntimeOrigin::root(),
                MediaDefinitionV2 {
                    definition_id,
                    subject_id: Some(1),
                    media_id: definition_id,
                    release_epoch: 1,
                    definition_hash: [definition_id as u8; 32],
                },
            ));
        }

        assert_noop!(
            EterraSlots::publish_acquisition_pool_v2(
                RuntimeOrigin::root(),
                2,
                1,
                2,
                vec![10, 11, 12, 13, 14, 20, 21, 22, 23, 24],
                vec![10, 11, 12, 20, 21, 22],
                background_ids,
                [0x61; 32],
            ),
            Error::<Test>::V2InvalidPool
        );
        assert!(!crate::AcquisitionPoolVersionsV2::<Test>::contains_key((
            2, 1
        )));
    });
}

#[test]
fn v2_duplicate_protection_uses_one_ordinary_reroll_and_full_top_tier_row() {
    new_test_ext().execute_with(|| {
        use eterra_nexus_primitives::CardRarity;

        publish_v2_test_catalog();
        let mut pool = crate::AcquisitionPoolVersionsV2::<Test>::get((1, 1)).unwrap();
        crate::SubjectProtectionSlotsV2::<Test>::insert(1, 3, 2);
        for (source_profile_id, profile_id) in [(23, 33), (24, 34)] {
            let mut profile =
                crate::SubjectRarityProfilesV2::<Test>::get(source_profile_id).unwrap();
            profile.profile_id = profile_id;
            profile.subject_id = 3;
            crate::SubjectRarityProfilesV2::<Test>::insert(profile_id, profile);
            pool.profiles
                .try_push(crate::PoolProfileEntry { profile_id })
                .unwrap();
        }
        let transcript = (0..=u8::MAX)
            .map(v2_draw_transcript)
            .find(|transcript| {
                EterraSlots::uniform_index(b"ETERRA_PACK_SUBJECT_V3", transcript, 0, 2) == Ok(0)
                    && EterraSlots::uniform_index(
                        b"ETERRA_PACK_SUBJECT_REROLL_V3",
                        transcript,
                        0,
                        2,
                    ) == Ok(1)
            })
            .expect("a transcript exercises the independent reroll");
        for rarity in [CardRarity::Common, CardRarity::Rare, CardRarity::Epic] {
            let mut protection = crate::PackProtectionHistoryBitmapsV2::<Test>::get(1, 1);
            while protection.len() < 2 {
                protection.try_push(0).unwrap();
            }
            // Both ordinary-tier outcomes are already known. The first draw
            // selects subject 1; exactly one independently seeded reroll selects
            // subject 2 and is accepted even though it is also known.
            protection[0] |= 1 << rarity.index();
            protection[0] |= 1 << (5 + rarity.index());
            let (_, ordinary) = EterraSlots::select_profile_for_pack(
                &pool,
                1,
                rarity,
                &transcript,
                0,
                &[],
                &protection,
                false,
            )
            .unwrap();
            assert_eq!(ordinary.subject_id, 2);
        }

        for rarity in [CardRarity::Legendary, CardRarity::Mythical] {
            let mut protection = crate::PackProtectionHistoryBitmapsV2::<Test>::get(1, 1);
            while protection.len() < 2 {
                protection.try_push(0).unwrap();
            }
            // Subjects 1 and 2 are known. Both the initial draw and an
            // independently seeded single reroll would remain duplicates, so
            // full-row protection must continue to undiscovered subject 3.
            protection[0] |= 1 << rarity.index();
            let subject_two_bit = 5 + rarity.index();
            protection[subject_two_bit / 8] |= 1 << (subject_two_bit % 8);
            let (_, top_tier) = EterraSlots::select_profile_for_pack(
                &pool,
                1,
                rarity,
                &transcript,
                0,
                &[],
                &protection,
                false,
            )
            .unwrap();
            assert_eq!(top_tier.subject_id, 3);
        }
    });
}

#[test]
fn v2_pack_open_is_credit_backed_two_phase_and_uses_fixed_profiles() {
    new_test_ext().execute_with(|| {
        publish_v2_test_catalog();
        assert_ok!(EterraSlots::issue_training_pack_credit_v2(
            RuntimeOrigin::root(),
            1,
            1,
            1,
            [3; 32],
        ));
        assert_ok!(EterraSlots::request_pack_open_v2(
            RuntimeOrigin::signed(1),
            1,
            1,
            eterra_nexus_primitives::EconomicRealm::Training,
            [4; 32],
        ));
        let opening_id = crate::PendingPackOpeningsV2::<Test>::iter()
            .next()
            .expect("pending opening")
            .0;
        assert_eq!(
            crate::PendingPackOpeningsV2::<Test>::get(opening_id)
                .expect("pending opening")
                .expected_randomness_provenance,
            pallet_eterra_randomness::RandomnessMode::DeterministicPrivateAlpha
        );
        assert_noop!(
            EterraSlots::finalize_pack_open_v2(RuntimeOrigin::signed(2), opening_id),
            Error::<Test>::V2PackOpeningNotReady
        );
        finalize_random_request(last_random_request(), [5; 32]);
        assert_ok!(EterraSlots::finalize_pack_open_v2(
            RuntimeOrigin::signed(2),
            opening_id,
        ));
        assert_noop!(
            EterraSlots::timeout_pack_open_v2(RuntimeOrigin::signed(2), opening_id),
            Error::<Test>::V2PackOpeningTerminalConflict
        );
        let card_ids =
            crate::ProcessedAcquisitionsV2::<Test>::get(opening_id).expect("processed opening");
        assert_eq!(card_ids.len(), 6);
        for (slot, card_id) in card_ids.iter().copied().enumerate() {
            let card = crate::CardsV2::<Test>::get(card_id).expect("card exists");
            let profile = crate::SubjectRarityProfilesV2::<Test>::get(card.profile_id)
                .expect("profile exists");
            assert_eq!(profile.subject_id, card.subject_id);
            assert_eq!(profile.subject_version, card.subject_version);
            assert_eq!(profile.rarity, card.rarity);
            assert_eq!(
                profile.base_ranks.iter().copied().sum::<u8>(),
                card.rarity.target_rank_total()
            );
            if slot == 5 {
                assert_eq!(card.rarity, eterra_nexus_primitives::CardRarity::Common);
                let definition_id = crate::SubjectDefinitionByKeyV2::<Test>::get((
                    card.subject_id,
                    card.subject_version,
                ))
                .expect("subject definition key");
                assert!(crate::SubjectDefinitionsV2::<Test>::get(definition_id)
                    .expect("subject definition")
                    .conversion_policy
                    .permits_conversion());
            }
        }
        assert!(!crate::TutorialConversionProfileIdsV2::<Test>::contains_key(opening_id));
        assert_eq!(crate::V2OwnerActiveCardCount::<Test>::get(1), 6);
        assert_eq!(
            crate::OutstandingPackCreditCountV2::<Test>::get(
                1,
                (1, 1, eterra_nexus_primitives::EconomicRealm::Training)
            ),
            0
        );
    });
}

#[test]
fn v2_pack_request_exact_retry_is_a_noop_before_dequeuing_another_credit() {
    new_test_ext().execute_with(|| {
        use eterra_nexus_primitives::EconomicRealm;

        publish_v2_test_catalog();
        for tutorial_id in [[0xE1; 32], [0xE2; 32]] {
            assert_ok!(EterraSlots::issue_training_pack_credit_v2(
                RuntimeOrigin::root(),
                1,
                1,
                1,
                tutorial_id,
            ));
        }
        let commitment = [0xE3; 32];
        assert_ok!(EterraSlots::request_pack_open_v2(
            RuntimeOrigin::signed(1),
            1,
            1,
            EconomicRealm::Training,
            commitment,
        ));
        let opening_id = crate::PendingPackOpeningsV2::<Test>::iter_keys()
            .next()
            .expect("first request committed");
        let first_randomness_request = last_random_request();
        let available_after_first =
            crate::AvailablePackCreditIdsV2::<Test>::get(1, (1, 1, EconomicRealm::Training));
        assert_eq!(available_after_first.len(), 1);
        assert_eq!(
            crate::PackOpeningRequestReceiptsV2::<Test>::get(1, commitment)
                .expect("permanent replay receipt")
                .opening_id,
            opening_id
        );

        assert_ok!(EterraSlots::set_v2_feature_enabled(
            RuntimeOrigin::root(),
            crate::V2Feature::Packs,
            false,
        ));
        assert_ok!(EterraSlots::request_pack_open_v2(
            RuntimeOrigin::signed(1),
            1,
            1,
            EconomicRealm::Training,
            commitment,
        ));
        assert_eq!(last_random_request(), first_randomness_request);
        assert_eq!(crate::PendingPackOpeningsV2::<Test>::iter().count(), 1);
        assert_eq!(
            crate::AvailablePackCreditIdsV2::<Test>::get(1, (1, 1, EconomicRealm::Training)),
            available_after_first
        );
        assert_eq!(
            crate::OutstandingPackCreditCountV2::<Test>::get(1, (1, 1, EconomicRealm::Training)),
            2
        );

        assert_noop!(
            EterraSlots::request_pack_open_v2(
                RuntimeOrigin::signed(1),
                2,
                1,
                EconomicRealm::Training,
                commitment,
            ),
            Error::<Test>::V2PackOpeningRequestConflict
        );
    });
}

#[test]
fn v2_tutorial_credit_grant_is_globally_idempotent_and_conflict_safe() {
    new_test_ext().execute_with(|| {
        use eterra_nexus_primitives::EconomicRealm;

        publish_v2_test_catalog();
        assert_noop!(
            EterraSlots::issue_training_pack_credit_v2(
                RuntimeOrigin::root(),
                1,
                1,
                1,
                [0; 32],
            ),
            Error::<Test>::V2TutorialIdRequired
        );
        assert!(!crate::TutorialPackCreditGrantReceiptsV2::<Test>::contains_key([0; 32]));
        assert_noop!(
            EterraSlots::issue_training_pack_credit_v2(
                RuntimeOrigin::root(),
                1,
                99,
                1,
                [0xE0; 32],
            ),
            Error::<Test>::V2PackSkuMissing
        );
        assert!(
            !crate::TutorialPackCreditGrantReceiptsV2::<Test>::contains_key([0xE0; 32])
        );
        let tutorial_id = [0xE4; 32];
        assert_ok!(EterraSlots::issue_training_pack_credit_v2(
            RuntimeOrigin::root(),
            1,
            1,
            1,
            tutorial_id,
        ));
        let next_credit_id = crate::NextPackCreditIdV2::<Test>::get();
        let queue =
            crate::AvailablePackCreditIdsV2::<Test>::get(1, (1, 1, EconomicRealm::Training));
        let receipt = crate::TutorialPackCreditGrantReceiptsV2::<Test>::get(tutorial_id)
            .expect("permanent tutorial grant receipt");

        assert_ok!(EterraSlots::issue_training_pack_credit_v2(
            RuntimeOrigin::root(),
            1,
            1,
            1,
            tutorial_id,
        ));
        assert_eq!(crate::NextPackCreditIdV2::<Test>::get(), next_credit_id);
        assert_eq!(
            crate::AvailablePackCreditIdsV2::<Test>::get(1, (1, 1, EconomicRealm::Training)),
            queue
        );
        assert_eq!(
            crate::TutorialPackCreditGrantReceiptsV2::<Test>::get(tutorial_id),
            Some(receipt)
        );

        assert_noop!(
            EterraSlots::issue_training_pack_credit_v2(
                RuntimeOrigin::root(),
                2,
                1,
                1,
                tutorial_id,
            ),
            Error::<Test>::V2TutorialPackCreditGrantConflict
        );
        assert_noop!(
            EterraSlots::issue_training_pack_credit_v2(
                RuntimeOrigin::root(),
                1,
                2,
                1,
                tutorial_id,
            ),
            Error::<Test>::V2TutorialPackCreditGrantConflict
        );
    });
}

#[test]
fn v2_pack_commitment_must_be_nonzero_without_consuming_credit() {
    new_test_ext().execute_with(|| {
        use eterra_nexus_primitives::EconomicRealm;

        publish_v2_test_catalog();
        assert_ok!(EterraSlots::issue_training_pack_credit_v2(
            RuntimeOrigin::root(),
            1,
            1,
            1,
            [0xE5; 32],
        ));
        let available =
            crate::AvailablePackCreditIdsV2::<Test>::get(1, (1, 1, EconomicRealm::Training));
        assert_noop!(
            EterraSlots::request_pack_open_v2(
                RuntimeOrigin::signed(1),
                1,
                1,
                EconomicRealm::Training,
                [0; 32],
            ),
            Error::<Test>::V2EntropyCommitmentRequired
        );
        assert_eq!(
            crate::AvailablePackCreditIdsV2::<Test>::get(1, (1, 1, EconomicRealm::Training)),
            available
        );
        assert_eq!(crate::PendingPackOpeningsV2::<Test>::iter().count(), 0);
        assert_eq!(crate::ReservedV2PackCardCount::<Test>::get(1), 0);
    });
}

#[test]
fn v2_operational_card_limit_queues_credits_and_reserves_pending_capacity() {
    new_test_ext().execute_with(|| {
        use eterra_nexus_primitives::EconomicRealm;

        publish_v2_test_catalog();
        for tutorial_id in [[0xE6; 32], [0xE7; 32], [0xE8; 32]] {
            assert_ok!(EterraSlots::issue_training_pack_credit_v2(
                RuntimeOrigin::root(),
                1,
                1,
                1,
                tutorial_id,
            ));
        }
        crate::V2OwnerCardCount::<Test>::insert(1, 9_988);

        for commitment in [[0xE9; 32], [0xEA; 32]] {
            assert_ok!(EterraSlots::request_pack_open_v2(
                RuntimeOrigin::signed(1),
                1,
                1,
                EconomicRealm::Training,
                commitment,
            ));
        }
        assert_eq!(crate::ReservedV2PackCardCount::<Test>::get(1), 12);
        let available =
            crate::AvailablePackCreditIdsV2::<Test>::get(1, (1, 1, EconomicRealm::Training));
        assert_eq!(available.len(), 1);

        assert_noop!(
            EterraSlots::request_pack_open_v2(
                RuntimeOrigin::signed(1),
                1,
                1,
                EconomicRealm::Training,
                [0xEB; 32],
            ),
            Error::<Test>::V2OperationalCardLimitReached
        );
        assert_eq!(
            crate::AvailablePackCreditIdsV2::<Test>::get(1, (1, 1, EconomicRealm::Training)),
            available
        );
        assert_eq!(crate::PendingPackOpeningsV2::<Test>::iter().count(), 2);

        let openings = crate::PendingPackOpeningsV2::<Test>::iter_values().collect::<Vec<_>>();
        for opening in openings {
            timeout_random_request(opening.randomness_request_id);
            assert_ok!(EterraSlots::timeout_pack_open_v2(
                RuntimeOrigin::signed(2),
                opening.opening_id,
            ));
        }
        assert_eq!(crate::ReservedV2PackCardCount::<Test>::get(1), 0);
        assert_eq!(
            crate::AvailablePackCreditIdsV2::<Test>::get(1, (1, 1, EconomicRealm::Training)).len(),
            3
        );
    });
}

#[test]
fn v2_over_limit_pack_request_has_no_side_effects() {
    new_test_ext().execute_with(|| {
        use eterra_nexus_primitives::EconomicRealm;

        publish_v2_test_catalog();
        assert_ok!(EterraSlots::issue_training_pack_credit_v2(
            RuntimeOrigin::root(),
            1,
            1,
            1,
            [0xEF; 32],
        ));
        crate::V2OwnerCardCount::<Test>::insert(1, 9_995);
        let key = (1, 1, EconomicRealm::Training);
        let available = crate::AvailablePackCreditIdsV2::<Test>::get(1, key);
        let outstanding = crate::OutstandingPackCreditCountV2::<Test>::get(1, key);

        assert_noop!(
            EterraSlots::request_pack_open_v2(
                RuntimeOrigin::signed(1),
                1,
                1,
                EconomicRealm::Training,
                [0xF0; 32],
            ),
            Error::<Test>::V2OperationalCardLimitReached
        );
        assert_eq!(
            crate::AvailablePackCreditIdsV2::<Test>::get(1, key),
            available
        );
        assert_eq!(
            crate::OutstandingPackCreditCountV2::<Test>::get(1, key),
            outstanding
        );
        assert_eq!(crate::ReservedV2PackCardCount::<Test>::get(1), 0);
        assert_eq!(crate::PendingPackOpeningsV2::<Test>::iter().count(), 0);
        assert_eq!(
            crate::PackOpeningRequestReceiptsV2::<Test>::iter().count(),
            0
        );
        assert!(!has_random_request());
    });
}

#[test]
fn v2_pack_crossing_nine_thousand_emits_operational_warning_once() {
    new_test_ext().execute_with(|| {
        use eterra_nexus_primitives::EconomicRealm;

        publish_v2_test_catalog();
        crate::V2OwnerCardCount::<Test>::insert(1, 8_995);
        assert_ok!(EterraSlots::issue_training_pack_credit_v2(
            RuntimeOrigin::root(),
            1,
            1,
            1,
            [0xEC; 32],
        ));
        assert_ok!(EterraSlots::request_pack_open_v2(
            RuntimeOrigin::signed(1),
            1,
            1,
            EconomicRealm::Training,
            [0xED; 32],
        ));
        let opening = crate::PendingPackOpeningsV2::<Test>::iter_values()
            .next()
            .expect("opening");
        finalize_random_request(opening.randomness_request_id, [0xEE; 32]);
        assert_ok!(EterraSlots::finalize_pack_open_v2(
            RuntimeOrigin::signed(2),
            opening.opening_id,
        ));
        assert_eq!(crate::V2OwnerCardCount::<Test>::get(1), 9_001);
        let warnings = frame_system::Pallet::<Test>::events()
            .into_iter()
            .filter(|record| {
                matches!(
                    record.event,
                    RuntimeEvent::EterraSlots(crate::Event::V2OwnerCardOperationalWarning {
                        owner: 1,
                        lifetime_card_count: 9_000,
                        unopened_limit: 10_000,
                    })
                )
            })
            .count();
        assert_eq!(warnings, 1);
    });
}

#[test]
fn v2_pack_may_fill_the_operational_card_limit_exactly() {
    new_test_ext().execute_with(|| {
        use eterra_nexus_primitives::EconomicRealm;

        publish_v2_test_catalog();
        crate::V2OwnerCardCount::<Test>::insert(1, 9_994);
        assert_ok!(EterraSlots::issue_training_pack_credit_v2(
            RuntimeOrigin::root(),
            1,
            1,
            1,
            [0xF1; 32],
        ));
        assert_ok!(EterraSlots::request_pack_open_v2(
            RuntimeOrigin::signed(1),
            1,
            1,
            EconomicRealm::Training,
            [0xF2; 32],
        ));
        let opening = crate::PendingPackOpeningsV2::<Test>::iter_values()
            .next()
            .expect("opening");
        assert_eq!(crate::ReservedV2PackCardCount::<Test>::get(1), 6);
        finalize_random_request(opening.randomness_request_id, [0xF3; 32]);
        assert_ok!(EterraSlots::finalize_pack_open_v2(
            RuntimeOrigin::signed(2),
            opening.opening_id,
        ));
        assert_eq!(crate::V2OwnerCardCount::<Test>::get(1), 10_000);
        assert_eq!(crate::ReservedV2PackCardCount::<Test>::get(1), 0);
    });
}

#[test]
fn v2_production_pack_request_fails_closed_without_drand_and_preserves_credit() {
    new_test_ext().execute_with(|| {
        use eterra_nexus_primitives::{EconomicRealm, PackCredit, PackCreditSource};

        publish_v2_test_catalog();
        crate::PackCreditsV2::<Test>::insert(
            1,
            PackCredit {
                credit_id: 1,
                owner: 1,
                pack_sku: 1,
                sku_version: 1,
                economic_realm: EconomicRealm::Production,
                source: PackCreditSource::Founder {
                    entitlement_id: [0xD1; 32],
                },
                amount: 1,
            },
        );
        crate::AvailablePackCreditIdsV2::<Test>::mutate(
            1,
            (1, 1, EconomicRealm::Production),
            |queue| queue.try_push(1).expect("one test credit fits"),
        );
        crate::OutstandingPackCreditCountV2::<Test>::insert(
            1,
            (1, 1, EconomicRealm::Production),
            1,
        );
        let available_before =
            crate::AvailablePackCreditIdsV2::<Test>::get(1, (1, 1, EconomicRealm::Production));

        assert_noop!(
            EterraSlots::request_pack_open_v2(
                RuntimeOrigin::signed(1),
                1,
                1,
                EconomicRealm::Production,
                [0xD2; 32],
            ),
            sp_runtime::DispatchError::Other("mock randomness realm or provenance mismatch")
        );
        assert_eq!(
            crate::AvailablePackCreditIdsV2::<Test>::get(1, (1, 1, EconomicRealm::Production)),
            available_before
        );
        assert_eq!(crate::PendingPackOpeningsV2::<Test>::iter().count(), 0);
    });
}

#[test]
fn v2_pack_activation_gates_new_requests_but_not_committed_finalization() {
    new_test_ext().execute_with(|| {
        use eterra_nexus_primitives::{EconomicRealm, SubjectActivationState};

        publish_v2_test_catalog();
        assert_ok!(EterraSlots::issue_training_pack_credit_v2(
            RuntimeOrigin::root(),
            1,
            1,
            1,
            [0x71; 32],
        ));
        assert_ok!(EterraSlots::request_pack_open_v2(
            RuntimeOrigin::signed(1),
            1,
            1,
            EconomicRealm::Training,
            [0x72; 32],
        ));
        let opening_id = crate::PendingPackOpeningsV2::<Test>::iter_keys()
            .next()
            .expect("opening committed while pool subjects are active");
        let randomness_request_id = last_random_request();

        for subject_definition_id in [1, 2] {
            assert_ok!(EterraSlots::set_subject_activation_v2(
                RuntimeOrigin::root(),
                SubjectActivationState {
                    subject_definition_id,
                    mint_enabled: false,
                    conversion_enabled: false,
                },
            ));
        }
        finalize_random_request(randomness_request_id, [0x73; 32]);
        assert_ok!(EterraSlots::finalize_pack_open_v2(
            RuntimeOrigin::signed(2),
            opening_id,
        ));
        assert_eq!(
            crate::ProcessedAcquisitionsV2::<Test>::get(opening_id)
                .expect("committed opening finalizes from its pinned pool")
                .len(),
            6
        );

        assert_ok!(EterraSlots::issue_training_pack_credit_v2(
            RuntimeOrigin::root(),
            1,
            1,
            1,
            [0x74; 32],
        ));
        let available_before =
            crate::AvailablePackCreditIdsV2::<Test>::get(1, (1, 1, EconomicRealm::Training));
        assert_noop!(
            EterraSlots::request_pack_open_v2(
                RuntimeOrigin::signed(1),
                1,
                1,
                EconomicRealm::Training,
                [0x75; 32],
            ),
            Error::<Test>::V2NoEligibleProfile
        );
        assert_eq!(
            crate::AvailablePackCreditIdsV2::<Test>::get(1, (1, 1, EconomicRealm::Training)),
            available_before
        );
    });
}

#[test]
fn v2_tutorial_conversion_candidate_is_filtered_and_frozen_at_commitment() {
    new_test_ext().execute_with(|| {
        use eterra_nexus_primitives::{CardRarity, EconomicRealm, SubjectActivationState};

        publish_v2_test_catalog();
        assert_ok!(EterraSlots::set_subject_activation_v2(
            RuntimeOrigin::root(),
            SubjectActivationState {
                subject_definition_id: 2,
                mint_enabled: true,
                conversion_enabled: false,
            },
        ));
        assert_ok!(EterraSlots::issue_training_pack_credit_v2(
            RuntimeOrigin::root(),
            1,
            1,
            1,
            [0xA1; 32],
        ));
        assert_ok!(EterraSlots::request_pack_open_v2(
            RuntimeOrigin::signed(1),
            1,
            1,
            EconomicRealm::Training,
            [0xA2; 32],
        ));
        let opening_id = crate::PendingPackOpeningsV2::<Test>::iter_keys()
            .next()
            .expect("tutorial opening");
        assert_eq!(
            crate::TutorialConversionProfileIdsV2::<Test>::get(opening_id)
                .expect("frozen tutorial candidates")
                .to_vec(),
            vec![10]
        );
        let randomness_request_id = last_random_request();

        // Mutable activation gates only new commitments. The already-frozen
        // candidate set remains live for permissionless finalization.
        assert_ok!(EterraSlots::set_subject_activation_v2(
            RuntimeOrigin::root(),
            SubjectActivationState {
                subject_definition_id: 1,
                mint_enabled: true,
                conversion_enabled: false,
            },
        ));
        finalize_random_request(randomness_request_id, [0xA3; 32]);
        assert_ok!(EterraSlots::finalize_pack_open_v2(
            RuntimeOrigin::signed(2),
            opening_id,
        ));
        let cards = crate::ProcessedAcquisitionsV2::<Test>::get(opening_id)
            .expect("opening finalizes from frozen candidates");
        let conversion_card = crate::CardsV2::<Test>::get(cards[5]).expect("sixth card");
        assert_eq!(conversion_card.subject_id, 1);
        assert_eq!(conversion_card.rarity, CardRarity::Common);

        assert_ok!(EterraSlots::issue_training_pack_credit_v2(
            RuntimeOrigin::root(),
            1,
            1,
            1,
            [0xA4; 32],
        ));
        let available_before =
            crate::AvailablePackCreditIdsV2::<Test>::get(1, (1, 1, EconomicRealm::Training));
        assert_noop!(
            EterraSlots::request_pack_open_v2(
                RuntimeOrigin::signed(1),
                1,
                1,
                EconomicRealm::Training,
                [0xA5; 32],
            ),
            Error::<Test>::V2TutorialConversionCardUnavailable
        );
        assert_eq!(
            crate::AvailablePackCreditIdsV2::<Test>::get(1, (1, 1, EconomicRealm::Training)),
            available_before
        );
        assert_eq!(last_random_request(), randomness_request_id);
    });
}

#[test]
fn v2_pack_timeout_restores_the_exact_credit_without_minting() {
    new_test_ext().execute_with(|| {
        publish_v2_test_catalog();
        assert_ok!(EterraSlots::issue_training_pack_credit_v2(
            RuntimeOrigin::root(),
            1,
            1,
            1,
            [6; 32],
        ));
        let credit_id = crate::NextPackCreditIdV2::<Test>::get() - 1;
        assert_ok!(EterraSlots::request_pack_open_v2(
            RuntimeOrigin::signed(1),
            1,
            1,
            eterra_nexus_primitives::EconomicRealm::Training,
            [7; 32],
        ));
        let opening_id = crate::PendingPackOpeningsV2::<Test>::iter()
            .next()
            .expect("pending opening")
            .0;
        timeout_random_request(last_random_request());
        assert_ok!(EterraSlots::timeout_pack_open_v2(
            RuntimeOrigin::signed(2),
            opening_id,
        ));
        let restored_queue = crate::AvailablePackCreditIdsV2::<Test>::get(
            1,
            (1, 1, eterra_nexus_primitives::EconomicRealm::Training),
        );
        assert_ok!(EterraSlots::timeout_pack_open_v2(
            RuntimeOrigin::signed(3),
            opening_id,
        ));
        assert_noop!(
            EterraSlots::finalize_pack_open_v2(RuntimeOrigin::signed(3), opening_id),
            Error::<Test>::V2PackOpeningTerminalConflict
        );
        assert_ok!(EterraSlots::request_pack_open_v2(
            RuntimeOrigin::signed(1),
            1,
            1,
            eterra_nexus_primitives::EconomicRealm::Training,
            [7; 32],
        ));
        assert!(crate::PackCreditsV2::<Test>::contains_key(credit_id));
        assert_eq!(
            crate::AvailablePackCreditIdsV2::<Test>::get(
                1,
                (1, 1, eterra_nexus_primitives::EconomicRealm::Training)
            ),
            restored_queue
        );
        assert!(restored_queue.contains(&credit_id));
        assert_eq!(
            crate::TimedOutPackOpeningsV2::<Test>::get(opening_id),
            Some(credit_id)
        );
        assert_eq!(crate::ReservedV2PackCardCount::<Test>::get(1), 0);
        assert_eq!(crate::NextCardIdV2::<Test>::get(), 0);
    });
}

#[test]
fn v2_conversion_is_irreversible_and_timeout_creates_stasis() {
    new_test_ext().execute_with(|| {
        publish_v2_test_catalog();
        assert_noop!(
            EterraSlots::request_conversion_v2(RuntimeOrigin::signed(1), 999, 1, [0; 32]),
            Error::<Test>::V2EntropyCommitmentRequired
        );
        assert_ok!(EterraSlots::issue_training_pack_credit_v2(
            RuntimeOrigin::root(),
            1,
            1,
            1,
            [10; 32],
        ));
        assert_ok!(EterraSlots::request_pack_open_v2(
            RuntimeOrigin::signed(1),
            1,
            1,
            eterra_nexus_primitives::EconomicRealm::Training,
            [11; 32],
        ));
        let opening_id = crate::PendingPackOpeningsV2::<Test>::iter()
            .next()
            .unwrap()
            .0;
        finalize_random_request(last_random_request(), [12; 32]);
        assert_ok!(EterraSlots::finalize_pack_open_v2(
            RuntimeOrigin::signed(1),
            opening_id,
        ));
        let cards = crate::ProcessedAcquisitionsV2::<Test>::get(opening_id).unwrap();

        let first_card = cards[0];
        seed_v2_conversion_safety_team(1, first_card);
        assert_ok!(EterraSlots::request_conversion_v2(
            RuntimeOrigin::signed(1),
            first_card,
            1,
            [13; 32],
        ));
        let first_request = crate::ConversionRequestByCard::<Test>::get(first_card).unwrap();
        assert_eq!(
            crate::CardConversionTombstones::<Test>::get(first_request)
                .expect("conversion tombstone")
                .expected_randomness_provenance,
            pallet_eterra_randomness::RandomnessMode::DeterministicPrivateAlpha
        );
        let first_conversion_randomness = last_random_request();
        assert_ok!(EterraSlots::set_v2_feature_enabled(
            RuntimeOrigin::root(),
            crate::V2Feature::Conversion,
            false,
        ));
        assert_ok!(EterraSlots::request_conversion_v2(
            RuntimeOrigin::signed(1),
            first_card,
            1,
            [13; 32],
        ));
        assert_eq!(last_random_request(), first_conversion_randomness);
        assert_eq!(crate::PendingConversionCountByAccountV2::<Test>::get(1), 1);
        assert_noop!(
            EterraSlots::request_conversion_v2(RuntimeOrigin::signed(1), first_card, 1, [14; 32]),
            Error::<Test>::V2ConversionRequestConflict
        );
        assert_ok!(EterraSlots::set_v2_feature_enabled(
            RuntimeOrigin::root(),
            crate::V2Feature::Conversion,
            true,
        ));
        finalize_random_request(last_random_request(), [15; 32]);
        assert_ok!(EterraSlots::finalize_conversion_v2(
            RuntimeOrigin::signed(2),
            first_request,
        ));
        assert_noop!(
            EterraSlots::timeout_conversion_v2(RuntimeOrigin::signed(2), first_request),
            Error::<Test>::V2ConversionTerminalConflict
        );
        assert!(matches!(
            crate::CardsV2::<Test>::get(first_card).unwrap().state,
            eterra_nexus_primitives::CardStateV2::Converted { .. }
        ));
        assert_ok!(EterraSlots::request_conversion_v2(
            RuntimeOrigin::signed(1),
            first_card,
            1,
            [13; 32],
        ));
        assert_eq!(crate::PendingConversionCountByAccountV2::<Test>::get(1), 0);

        let second_card = cards[1];
        assert_ok!(EterraSlots::request_conversion_v2(
            RuntimeOrigin::signed(1),
            second_card,
            1,
            [16; 32],
        ));
        let second_request = crate::ConversionRequestByCard::<Test>::get(second_card).unwrap();
        timeout_random_request(last_random_request());
        assert_ok!(EterraSlots::timeout_conversion_v2(
            RuntimeOrigin::signed(3),
            second_request,
        ));
        assert_ok!(EterraSlots::timeout_conversion_v2(
            RuntimeOrigin::signed(4),
            second_request,
        ));
        assert_noop!(
            EterraSlots::finalize_conversion_v2(RuntimeOrigin::signed(4), second_request),
            Error::<Test>::V2ConversionTerminalConflict
        );
        let entities = created_entities();
        assert_eq!(entities.len(), 2);
        assert!(!entities[0].stasis_genome);
        assert!(entities[1].stasis_genome);
        assert_eq!(
            crate::CardConversionTombstones::<Test>::get(second_request)
                .unwrap()
                .resolution,
            crate::ConversionResolution::StasisTimeout
        );
        assert_eq!(crate::V2OwnerActiveCardCount::<Test>::get(1), 9);
    });
}

#[test]
fn v2_conversion_profile_activation_is_checked_only_before_commitment() {
    new_test_ext().execute_with(|| {
        use eterra_nexus_primitives::EconomicRealm;

        publish_v2_test_catalog();
        assert_ok!(EterraSlots::issue_training_pack_credit_v2(
            RuntimeOrigin::root(),
            1,
            1,
            1,
            [0x81; 32],
        ));
        assert_ok!(EterraSlots::request_pack_open_v2(
            RuntimeOrigin::signed(1),
            1,
            1,
            EconomicRealm::Training,
            [0x82; 32],
        ));
        let opening_id = crate::PendingPackOpeningsV2::<Test>::iter_keys()
            .next()
            .expect("pending opening");
        finalize_random_request(last_random_request(), [0x83; 32]);
        assert_ok!(EterraSlots::finalize_pack_open_v2(
            RuntimeOrigin::signed(1),
            opening_id,
        ));
        let card_id =
            crate::ProcessedAcquisitionsV2::<Test>::get(opening_id).expect("opened cards")[0];
        seed_v2_conversion_safety_team(1, card_id);

        set_mock_conversion_profile_active(false);
        assert_noop!(
            EterraSlots::request_conversion_v2(RuntimeOrigin::signed(1), card_id, 1, [0x84; 32],),
            sp_runtime::DispatchError::Other("mock conversion profile inactive")
        );
        assert_eq!(
            crate::CardsV2::<Test>::get(card_id)
                .expect("rejected conversion leaves card")
                .state,
            eterra_nexus_primitives::CardStateV2::Active
        );
        assert!(!crate::ConversionRequestByCard::<Test>::contains_key(
            card_id
        ));

        set_mock_conversion_profile_active(true);
        assert_ok!(EterraSlots::request_conversion_v2(
            RuntimeOrigin::signed(1),
            card_id,
            1,
            [0x85; 32],
        ));
        let request_id =
            crate::ConversionRequestByCard::<Test>::get(card_id).expect("conversion committed");

        // Mutable profile activation may stop later requests, but cannot strand
        // this already non-cancellable conversion.
        set_mock_conversion_profile_active(false);
        finalize_random_request(last_random_request(), [0x86; 32]);
        assert_ok!(EterraSlots::finalize_conversion_v2(
            RuntimeOrigin::signed(2),
            request_id,
        ));
        assert_eq!(
            crate::CardConversionTombstones::<Test>::get(request_id)
                .expect("conversion tombstone")
                .resolution,
            crate::ConversionResolution::Created
        );
        assert_eq!(created_entities().len(), 1);
    });
}

#[test]
fn v2_conversion_requires_a_current_same_realm_bring_five_excluding_candidate() {
    new_test_ext().execute_with(|| {
        use eterra_nexus_primitives::{CardStateV2, EconomicRealm};

        publish_v2_test_catalog();
        assert_noop!(
            EterraSlots::publish_competitive_format_v2(
                RuntimeOrigin::root(),
                crate::CompetitiveFormatV2 {
                    format_id: 8000,
                    version: 1,
                    set_id: 1,
                    team_size: 4,
                    rarity_load_budget: 15,
                    max_mythical: 1,
                    max_legendary_or_better: 2,
                    rules_hash: [0x80; 32],
                },
            ),
            Error::<Test>::V2InvalidFormat
        );
        let cards = open_v2_training_pack(1, 0xB0);
        let candidate_id = cards[5];
        let last_pack_randomness = last_random_request();

        // Raw inventory count alone is never a playable-roster proof.
        assert_noop!(
            EterraSlots::request_conversion_v2(
                RuntimeOrigin::signed(1),
                candidate_id,
                1,
                [0xB1; 32],
            ),
            Error::<Test>::V2PlayableRosterTooSmall
        );
        assert_eq!(last_random_request(), last_pack_randomness);

        let safety_cards = seed_v2_conversion_safety_team(1, candidate_id);

        // A Training roster cannot authorize conversion of a Production card.
        let mut production_candidate =
            crate::CardsV2::<Test>::get(candidate_id).expect("candidate");
        let production_card_id = crate::NextCardIdV2::<Test>::get();
        crate::NextCardIdV2::<Test>::put(production_card_id + 1);
        production_candidate.card_id = production_card_id;
        production_candidate.economic_realm = EconomicRealm::Production;
        production_candidate.acquisition_id = [0xB2; 32];
        crate::CardsV2::<Test>::insert(production_card_id, production_candidate);
        crate::V2OwnerCardCount::<Test>::mutate(1, |count| *count += 1);
        crate::V2OwnerActiveCardCount::<Test>::mutate(1, |count| *count += 1);
        assert_noop!(
            EterraSlots::request_conversion_v2(
                RuntimeOrigin::signed(1),
                production_card_id,
                1,
                [0xB3; 32],
            ),
            Error::<Test>::V2PlayableRosterTooSmall
        );

        // The latest saved safety team must exclude the candidate itself.
        let mut containing_candidate = vec![candidate_id];
        containing_candidate.extend_from_slice(&safety_cards[..4]);
        assert_ok!(EterraSlots::save_competitive_team_v2(
            RuntimeOrigin::signed(1),
            9001,
            9001,
            1,
            containing_candidate.try_into().expect("five cards fit"),
        ));
        assert_noop!(
            EterraSlots::request_conversion_v2(
                RuntimeOrigin::signed(1),
                candidate_id,
                1,
                [0xB4; 32],
            ),
            Error::<Test>::V2PlayableRosterTooSmall
        );

        assert_ok!(EterraSlots::save_competitive_team_v2(
            RuntimeOrigin::signed(1),
            9001,
            9001,
            1,
            safety_cards.clone().try_into().expect("five cards fit"),
        ));
        let mut stale = crate::CardsV2::<Test>::get(safety_cards[0]).expect("roster card");
        stale.state = CardStateV2::ConversionCommitted {
            request_id: [0xB5; 32],
        };
        crate::CardsV2::<Test>::insert(safety_cards[0], stale.clone());
        assert_noop!(
            EterraSlots::request_conversion_v2(
                RuntimeOrigin::signed(1),
                candidate_id,
                1,
                [0xB6; 32],
            ),
            Error::<Test>::V2PlayableRosterTooSmall
        );
        stale.state = CardStateV2::Active;
        crate::CardsV2::<Test>::insert(safety_cards[0], stale);

        assert_ok!(EterraSlots::request_conversion_v2(
            RuntimeOrigin::signed(1),
            candidate_id,
            1,
            [0xB7; 32],
        ));
        assert_eq!(crate::PendingConversionCountByAccountV2::<Test>::get(1), 1);
    });
}

#[test]
fn v2_pending_conversion_bound_releases_on_created_and_stasis_terminals() {
    new_test_ext().execute_with(|| {
        publish_v2_test_catalog();
        let cards = open_v2_training_pack(1, 0xC0);
        seed_v2_conversion_safety_team(1, cards[5]);

        let mut requests = Vec::new();
        let mut randomness_requests = Vec::new();
        for (index, card_id) in cards.iter().copied().take(2).enumerate() {
            assert_ok!(EterraSlots::request_conversion_v2(
                RuntimeOrigin::signed(1),
                card_id,
                1,
                [0xC1 + index as u8; 32],
            ));
            requests.push(
                crate::ConversionRequestByCard::<Test>::get(card_id)
                    .expect("conversion request"),
            );
            randomness_requests.push(last_random_request());
        }
        assert_eq!(crate::PendingConversionCountByAccountV2::<Test>::get(1), 2);
        let third_card = cards[2];
        assert_noop!(
            EterraSlots::request_conversion_v2(
                RuntimeOrigin::signed(1),
                third_card,
                1,
                [0xC3; 32],
            ),
            Error::<Test>::V2PendingConversionLimitReached
        );
        assert_eq!(
            crate::CardsV2::<Test>::get(third_card)
                .expect("rejected candidate")
                .state,
            eterra_nexus_primitives::CardStateV2::Active
        );
        assert!(!crate::ConversionRequestByCard::<Test>::contains_key(
            third_card
        ));

        finalize_random_request(randomness_requests[0], [0xC4; 32]);
        assert_ok!(EterraSlots::finalize_conversion_v2(
            RuntimeOrigin::signed(2),
            requests[0],
        ));
        assert_eq!(crate::PendingConversionCountByAccountV2::<Test>::get(1), 1);
        assert_ok!(EterraSlots::finalize_conversion_v2(
            RuntimeOrigin::signed(2),
            requests[0],
        ));
        assert_eq!(crate::PendingConversionCountByAccountV2::<Test>::get(1), 1);

        timeout_random_request(randomness_requests[1]);
        assert_ok!(EterraSlots::timeout_conversion_v2(
            RuntimeOrigin::signed(3),
            requests[1],
        ));
        assert_eq!(crate::PendingConversionCountByAccountV2::<Test>::get(1), 0);
        assert_noop!(
            EterraSlots::finalize_conversion_v2(RuntimeOrigin::signed(2), requests[1]),
            Error::<Test>::V2ConversionTerminalConflict
        );
        assert_eq!(crate::PendingConversionCountByAccountV2::<Test>::get(1), 0);

        assert_ok!(EterraSlots::request_conversion_v2(
            RuntimeOrigin::signed(1),
            third_card,
            1,
            [0xC5; 32],
        ));
        assert_eq!(crate::PendingConversionCountByAccountV2::<Test>::get(1), 1);
    });
}

#[test]
fn v2_draw_and_genome_transcripts_are_genesis_and_domain_separated() {
    new_test_ext().execute_with(|| {
        let transcript = v2_draw_transcript(0xD0);
        let domain: &[u8] = b"ETERRA_PACK_RARITY_V3";
        let genesis = <MockV2ChainDomain as crate::V2ChainDomainProvider>::genesis_hash();
        assert_eq!(genesis, [0xA5; 32]);
        let hash = EterraSlots::draw_hash(domain, &transcript, 7);
        assert_eq!(
            hash,
            sp_io::hashing::blake2_256(
                &(
                    domain,
                    genesis,
                    transcript.request_id,
                    transcript.immutable_config_hash,
                    transcript.account_commitment,
                    transcript.verified_randomness_output,
                    7u32,
                )
                    .encode()
            )
        );

        let mut mutations = Vec::new();
        let mut changed = transcript;
        changed.request_id[0] ^= 1;
        mutations.push(EterraSlots::draw_hash(domain, &changed, 7));
        changed = transcript;
        changed.immutable_config_hash[0] ^= 1;
        mutations.push(EterraSlots::draw_hash(domain, &changed, 7));
        changed = transcript;
        changed.account_commitment[0] ^= 1;
        mutations.push(EterraSlots::draw_hash(domain, &changed, 7));
        changed = transcript;
        changed.verified_randomness_output[0] ^= 1;
        mutations.push(EterraSlots::draw_hash(domain, &changed, 7));
        mutations.push(EterraSlots::draw_hash(domain, &transcript, 8));
        mutations.push(EterraSlots::draw_hash(
            b"ETERRA_PACK_SUBJECT_V3",
            &transcript,
            7,
        ));
        assert!(mutations.iter().all(|changed_hash| *changed_hash != hash));

        let genome =
            EterraSlots::conversion_genome_hash([1; 32], [2; 32], 3, [4; 32], [5; 32], 6, 1);
        let genome_domain: &[u8] = b"ETERRA_ENTITY_GENOME_V1";
        assert_eq!(
            genome,
            sp_io::hashing::blake2_256(
                &(
                    genome_domain,
                    genesis,
                    [1u8; 32],
                    [2u8; 32],
                    3u64,
                    [4u8; 32],
                    [5u8; 32],
                    6u32,
                    1u16,
                )
                    .encode()
            )
        );
        set_mock_v2_genesis_hash([0xA6; 32]);
        assert_ne!(EterraSlots::draw_hash(domain, &transcript, 7), hash);
        assert_ne!(
            EterraSlots::conversion_genome_hash([1; 32], [2; 32], 3, [4; 32], [5; 32], 6, 1,),
            genome
        );
    });
}

#[test]
fn v2_rejection_sampler_has_exact_u32_boundaries() {
    new_test_ext().execute_with(|| {
        assert_eq!(EterraSlots::unbiased_index_for_sample(0, 0), None);
        assert_eq!(EterraSlots::unbiased_index_for_sample(u32::MAX, 1), Some(0));
        assert_eq!(EterraSlots::unbiased_index_for_sample(u32::MAX, 2), Some(1));
        assert_eq!(
            EterraSlots::unbiased_index_for_sample(u32::MAX - 1, 3),
            Some(2)
        );
        assert_eq!(EterraSlots::unbiased_index_for_sample(u32::MAX, 3), None);
        assert_eq!(
            EterraSlots::unbiased_index_for_sample(4_294_967_291, 6),
            Some(5)
        );
        assert_eq!(
            EterraSlots::unbiased_index_for_sample(4_294_967_292, 6),
            None
        );
        assert_eq!(
            EterraSlots::unbiased_index_for_sample(4_294_959_999, 10_000),
            Some(9_999)
        );
        assert_eq!(
            EterraSlots::unbiased_index_for_sample(4_294_960_000, 10_000),
            None
        );
        assert_eq!(
            EterraSlots::unbiased_index_for_sample(u32::MAX, u32::MAX),
            None
        );
        let mut calls = 0u32;
        assert_eq!(
            EterraSlots::rejection_sample_with(3, |attempt| {
                calls += 1;
                Ok(if attempt == 0 { u32::MAX } else { 5 })
            })
            .unwrap(),
            2
        );
        assert_eq!(calls, 2);
        assert_eq!(
            EterraSlots::rejection_sample_with(3, |_| Ok(u32::MAX)),
            Err(Error::<Test>::V2RandomSamplingExhausted.into())
        );
        assert_eq!(
            EterraSlots::uniform_index(b"ETERRA_PACK_RARITY_V3", &v2_draw_transcript(1), 0, 1,)
                .unwrap(),
            0
        );
        assert_eq!(
            EterraSlots::uniform_index(
                b"ETERRA_PACK_RARITY_V3",
                &v2_draw_transcript(1),
                u32::MAX,
                1,
            ),
            Err(Error::<Test>::V2ArithmeticOverflow.into())
        );
    });
}

#[test]
fn v14_to_v16_migration_builds_sidecars_without_rewriting_legacy_storage() {
    use frame_support::traits::{Hooks, StorageVersion};

    new_test_ext().execute_with(|| {
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(1)));
        let card_id = NextCardId::<Test>::get() - 1;
        let card_before = Cards::<Test>::get(card_id).unwrap().encode();
        let owner_index_before = CardsByOwner::<Test>::get(1).encode();
        let next_card_id_before = NextCardId::<Test>::get();

        StorageVersion::new(14).put::<EterraSlots>();
        <EterraSlots as Hooks<u64>>::on_runtime_upgrade();

        let running = crate::TcgMigrationStateStorageV16::<Test>::get().expect("migration started");
        assert_eq!(running.phase, crate::MigrationPhaseV16::Running);
        assert_eq!(running.from_storage_version, 14);
        assert_eq!(running.upper_bound, next_card_id_before);
        assert_eq!(
            StorageVersion::get::<EterraSlots>(),
            StorageVersion::new(16)
        );
        assert_eq!(Cards::<Test>::get(card_id).unwrap().encode(), card_before);
        assert_eq!(CardsByOwner::<Test>::get(1).encode(), owner_index_before);
        assert_eq!(NextCardId::<Test>::get(), next_card_id_before);

        assert_eq!(EterraSlots::migrate_v16_batch(100), 1);
        let awaiting =
            crate::TcgMigrationStateStorageV16::<Test>::get().expect("migration scanned");
        assert_eq!(
            awaiting.phase,
            crate::MigrationPhaseV16::AwaitingVerification
        );
        assert!(crate::LegacyWritesPausedV16::<Test>::get());
        assert_noop!(
            EterraSlots::transfer_card(RuntimeOrigin::signed(1), card_id, 2),
            Error::<Test>::LegacyWritesPaused
        );
        assert_noop!(
            EterraSlots::complete_legacy_migration_v16(
                RuntimeOrigin::root(),
                awaiting.cards_seen,
                awaiting.anomalies,
                [0; 32],
            ),
            Error::<Test>::V16MigrationInvariantFailed
        );
        assert_noop!(
            EterraSlots::complete_legacy_migration_v16(
                RuntimeOrigin::root(),
                awaiting.cards_seen.saturating_add(1),
                awaiting.anomalies,
                [1; 32],
            ),
            Error::<Test>::V16MigrationInvariantFailed
        );
        assert!(crate::LegacyWritesPausedV16::<Test>::get());
        attest_v16_migration();
        let completed =
            crate::TcgMigrationStateStorageV16::<Test>::get().expect("migration attested");
        assert_eq!(completed.phase, crate::MigrationPhaseV16::Completed);
        assert_eq!(completed.from_storage_version, 14);
        assert_eq!(completed.cards_seen, 1);
        assert_eq!(crate::LegacyCardClassifications::<Test>::iter().count(), 1);
        assert_eq!(Cards::<Test>::get(card_id).unwrap().encode(), card_before);
        assert_eq!(CardsByOwner::<Test>::get(1).encode(), owner_index_before);
        assert_eq!(NextCardId::<Test>::get(), next_card_id_before);
        assert_event_found(
            |event| {
                matches!(
                    event,
                    RuntimeEvent::EterraSlots(Event::LegacyMigrationStarted {
                        from_storage_version: 14,
                        upper_bound,
                    }) if *upper_bound == next_card_id_before
                )
            },
            "LegacyMigrationStarted(V14)",
        );
    });
}

#[test]
fn v16_migration_pause_blocks_every_remaining_legacy_writer_class() {
    new_test_ext().execute_with(|| {
        crate::LegacyWritesPausedV16::<Test>::put(true);
        let player_balance = Balances::free_balance(1);
        let collection_count = SeasonCollectionIds::<Test>::get(1).len();
        let gear_count = NexusGearItems::<Test>::iter().count();
        let spell_count = NexusSpellbook::<Test>::iter().count();
        let starter_config_count = StarterTeamConfigs::<Test>::iter().count();
        let prize_pool_count = NexusPrizePools::<Test>::iter().count();
        let collection_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"paused".to_vec().try_into().expect("bounded");

        assert_noop!(
            EterraSlots::create_season_collection(RuntimeOrigin::signed(1), 1, collection_name),
            Error::<Test>::LegacyWritesPaused
        );
        assert_noop!(
            EterraSlots::publish_season_collection(RuntimeOrigin::signed(1), 1, 0),
            Error::<Test>::LegacyWritesPaused
        );
        assert_noop!(
            EterraSlots::remove_season_collection(RuntimeOrigin::signed(1), 1, 0),
            Error::<Test>::LegacyWritesPaused
        );
        assert_noop!(
            EterraSlots::add_season_collection_asset(
                RuntimeOrigin::signed(1),
                1,
                0,
                crate::AssetKind::Subject,
                1
            ),
            Error::<Test>::LegacyWritesPaused
        );
        assert_noop!(
            EterraSlots::remove_season_collection_asset(
                RuntimeOrigin::signed(1),
                1,
                0,
                crate::AssetKind::Subject,
                1
            ),
            Error::<Test>::LegacyWritesPaused
        );
        assert_noop!(
            EterraSlots::move_season_collection_asset(
                RuntimeOrigin::signed(1),
                1,
                0,
                crate::AssetKind::Subject,
                1,
                0
            ),
            Error::<Test>::LegacyWritesPaused
        );
        assert_noop!(
            EterraSlots::set_season_collection_asset_weights(
                RuntimeOrigin::signed(1),
                1,
                0,
                crate::AssetWeightKind::Subject,
                vec![],
                vec![]
            ),
            Error::<Test>::LegacyWritesPaused
        );
        assert_noop!(
            EterraSlots::buy_card_capacity(RuntimeOrigin::signed(1)),
            Error::<Test>::LegacyWritesPaused
        );
        assert_eq!(Balances::free_balance(1), player_balance);
        assert_noop!(
            EterraSlots::init_card_nft_collection(RuntimeOrigin::signed(1), 1),
            Error::<Test>::LegacyWritesPaused
        );
        assert_noop!(
            EterraSlots::set_progression_tree(RuntimeOrigin::root(), 1, 1, None, vec![], 1),
            Error::<Test>::LegacyWritesPaused
        );
        assert_noop!(
            EterraSlots::seed_alpha_progression_gear(
                RuntimeOrigin::root(),
                1,
                1,
                1,
                GearSlotType::Weapon,
                GearTier::Basic,
                1,
                1,
                1
            ),
            Error::<Test>::LegacyWritesPaused
        );
        assert_noop!(
            EterraSlots::seed_alpha_spell(RuntimeOrigin::root(), 1, 1, Element::Fire, 1, 1),
            Error::<Test>::LegacyWritesPaused
        );
        assert_noop!(
            EterraSlots::set_starter_team_config(
                RuntimeOrigin::root(),
                StarterPath::Fire,
                vec![],
                1
            ),
            Error::<Test>::LegacyWritesPaused
        );
        assert_noop!(
            EterraSlots::set_nexus_prize_pool(RuntimeOrigin::root(), 1, vec![], 1),
            Error::<Test>::LegacyWritesPaused
        );

        assert_eq!(SeasonCollectionIds::<Test>::get(1).len(), collection_count);
        assert_eq!(NexusGearItems::<Test>::iter().count(), gear_count);
        assert_eq!(NexusSpellbook::<Test>::iter().count(), spell_count);
        assert_eq!(
            StarterTeamConfigs::<Test>::iter().count(),
            starter_config_count
        );
        assert_eq!(NexusPrizePools::<Test>::iter().count(), prize_pool_count);
    });
}

#[test]
fn v15_to_v16_migration_builds_sidecars_without_rewriting_legacy_card() {
    use frame_support::traits::{Hooks, StorageVersion};

    new_test_ext().execute_with(|| {
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(1)));
        let card_id = NextCardId::<Test>::get() - 1;
        let before = Cards::<Test>::get(card_id).unwrap();
        StorageVersion::new(15).put::<EterraSlots>();
        <EterraSlots as Hooks<u64>>::on_runtime_upgrade();
        let running = crate::TcgMigrationStateStorageV16::<Test>::get().expect("migration started");
        assert_eq!(running.from_storage_version, 15);
        assert!(crate::LegacyWritesPausedV16::<Test>::get());
        assert_noop!(
            EterraSlots::mint_card(RuntimeOrigin::signed(1)),
            Error::<Test>::LegacyWritesPaused
        );
        assert_eq!(EterraSlots::migrate_v16_batch(100), 1);
        assert_eq!(
            crate::TcgMigrationStateStorageV16::<Test>::get()
                .unwrap()
                .phase,
            crate::MigrationPhaseV16::AwaitingVerification
        );
        assert!(crate::LegacyWritesPausedV16::<Test>::get());
        attest_v16_migration();
        let classification =
            crate::LegacyCardClassifications::<Test>::get(card_id).expect("classified");
        assert_eq!(classification.custody, crate::LegacyCustodyKind::Ordinary);
        assert_eq!(classification.beneficial_owner, Some(1));
        assert!(!classification.frozen);
        assert_eq!(Cards::<Test>::get(card_id).unwrap(), before);
        assert!(!crate::LegacyWritesPausedV16::<Test>::get());
        assert!(crate::LegacyCreationSealedV16::<Test>::get());
        assert_noop!(
            EterraSlots::mint_card(RuntimeOrigin::signed(1)),
            Error::<Test>::LegacyCreationSealed
        );
        assert_eq!(
            crate::TcgMigrationStateStorageV16::<Test>::get()
                .unwrap()
                .phase,
            crate::MigrationPhaseV16::Completed
        );
        assert_eq!(crate::LegacyCardClassifications::<Test>::iter().count(), 1);
    });
}

#[test]
fn v14_to_v16_migration_resumes_without_reinitializing_bounded_progress() {
    use frame_support::traits::{Hooks, StorageVersion};

    new_test_ext().execute_with(|| {
        for owner in [1, 2, 3] {
            assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(owner)));
        }
        StorageVersion::new(14).put::<EterraSlots>();
        <EterraSlots as Hooks<u64>>::on_runtime_upgrade();

        assert_eq!(EterraSlots::migrate_v16_batch(1), 1);
        let interrupted =
            crate::TcgMigrationStateStorageV16::<Test>::get().expect("migration running");
        assert_eq!(interrupted.phase, crate::MigrationPhaseV16::Running);
        assert_eq!(interrupted.cursor, 1);
        assert_eq!(interrupted.cards_seen, 1);

        // A runtime restart/re-entry observes V16 and must not reset bounded progress.
        <EterraSlots as Hooks<u64>>::on_runtime_upgrade();
        assert_eq!(
            crate::TcgMigrationStateStorageV16::<Test>::get(),
            Some(interrupted)
        );

        assert_eq!(EterraSlots::migrate_v16_batch(1), 1);
        assert_eq!(EterraSlots::migrate_v16_batch(100), 1);
        attest_v16_migration();
        let completed =
            crate::TcgMigrationStateStorageV16::<Test>::get().expect("migration completed");
        assert_eq!(completed.phase, crate::MigrationPhaseV16::Completed);
        assert_eq!(completed.from_storage_version, 14);
        assert_eq!(completed.cursor, 3);
        assert_eq!(completed.cards_seen, 3);
        assert_eq!(crate::LegacyCardClassifications::<Test>::iter().count(), 3);
    });
}

#[test]
fn unsupported_pre_v16_migration_source_remains_fail_closed() {
    use frame_support::traits::{Hooks, StorageVersion};

    new_test_ext().execute_with(|| {
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(1)));
        let card_before = Cards::<Test>::get(0).unwrap().encode();
        for feature in [
            crate::V2Feature::Packs,
            crate::V2Feature::Conversion,
            crate::V2Feature::Ranked,
            crate::V2Feature::MythicalAscension,
        ] {
            crate::V2FeatureEnabled::<Test>::insert(feature, true);
        }

        StorageVersion::new(13).put::<EterraSlots>();
        <EterraSlots as Hooks<u64>>::on_runtime_upgrade();

        assert_eq!(
            StorageVersion::get::<EterraSlots>(),
            StorageVersion::new(13)
        );
        let state =
            crate::TcgMigrationStateStorageV16::<Test>::get().expect("rejection state recorded");
        assert_eq!(state.phase, crate::MigrationPhaseV16::UnsupportedSource);
        assert_eq!(state.from_storage_version, 13);
        assert!(crate::LegacyWritesPausedV16::<Test>::get());
        assert!(crate::LegacyCreationSealedV16::<Test>::get());
        assert_eq!(EterraSlots::migrate_v16_batch(100), 0);
        assert_eq!(Cards::<Test>::get(0).unwrap().encode(), card_before);
        for feature in [
            crate::V2Feature::Packs,
            crate::V2Feature::Conversion,
            crate::V2Feature::Ranked,
            crate::V2Feature::MythicalAscension,
        ] {
            assert!(!crate::V2FeatureEnabled::<Test>::get(feature));
        }
        assert_event_found(
            |event| {
                matches!(
                    event,
                    RuntimeEvent::EterraSlots(Event::LegacyMigrationSourceRejectedV16 {
                        from_storage_version: 13,
                    })
                )
            },
            "LegacyMigrationSourceRejectedV16",
        );
    });
}

#[cfg(feature = "try-runtime")]
#[test]
fn v14_and_v15_try_runtime_evidence_proves_start_and_completion_invariants() {
    use frame_support::traits::{Hooks, StorageVersion};

    for source_version in [14, 15] {
        new_test_ext().execute_with(|| {
            assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(1)));
            crate::V2FeatureEnabled::<Test>::insert(crate::V2Feature::Packs, true);
            StorageVersion::new(source_version).put::<EterraSlots>();

            let evidence =
                <EterraSlots as Hooks<u64>>::pre_upgrade().expect("pre-upgrade evidence");
            <EterraSlots as Hooks<u64>>::on_runtime_upgrade();
            <EterraSlots as Hooks<u64>>::post_upgrade(evidence)
                .expect("post-upgrade evidence reconciles");
            <EterraSlots as Hooks<u64>>::try_state(System::block_number())
                .expect("running state is valid");

            assert_eq!(EterraSlots::migrate_v16_batch(1), 1);
            <EterraSlots as Hooks<u64>>::try_state(System::block_number())
                .expect("awaiting-verification state is valid");
            attest_v16_migration();
            <EterraSlots as Hooks<u64>>::try_state(System::block_number())
                .expect("attested completed state is valid");
        });
    }
}

#[cfg(feature = "try-runtime")]
#[test]
fn unsupported_source_try_state_accepts_only_the_sealed_rejection_state() {
    use frame_support::traits::{Hooks, StorageVersion};

    new_test_ext().execute_with(|| {
        StorageVersion::new(13).put::<EterraSlots>();
        <EterraSlots as Hooks<u64>>::on_runtime_upgrade();
        <EterraSlots as Hooks<u64>>::try_state(System::block_number())
            .expect("unsupported source remains safely sealed");
    });
}

#[cfg(feature = "try-runtime")]
#[test]
fn v2_try_state_reconciles_pending_pack_capacity_and_locked_credit() {
    use frame_support::traits::Hooks;

    new_test_ext().execute_with(|| {
        publish_v2_test_catalog();
        assert_ok!(EterraSlots::issue_training_pack_credit_v2(
            RuntimeOrigin::root(),
            1,
            1,
            1,
            [0x61; 32],
        ));
        assert_ok!(EterraSlots::request_pack_open_v2(
            RuntimeOrigin::signed(1),
            1,
            1,
            eterra_nexus_primitives::EconomicRealm::Training,
            [0x62; 32],
        ));
        <EterraSlots as Hooks<u64>>::try_state(System::block_number())
            .expect("pending opening reconciles");

        crate::ReservedV2PackCardCount::<Test>::insert(1, 5);
        assert!(
            <EterraSlots as Hooks<u64>>::try_state(System::block_number()).is_err(),
            "corrupt reservation must fail try-state"
        );
        crate::ReservedV2PackCardCount::<Test>::insert(1, 6);
        let opening_id = crate::PendingPackOpeningsV2::<Test>::iter_keys()
            .next()
            .expect("opening");
        crate::LockedPackCreditsV2::<Test>::remove(opening_id);
        assert!(
            <EterraSlots as Hooks<u64>>::try_state(System::block_number()).is_err(),
            "missing locked credit must fail try-state"
        );
    });
}

#[test]
fn v15_to_v16_migration_classifies_all_custody_paths_and_freezes_unknowns() {
    use frame_support::traits::{Hooks, StorageVersion};

    new_test_ext().execute_with(|| {
        let escrow: u64 = frame_support::PalletId(*b"et/tcgsc").into_account_truncating();
        let external_escrow = MOCK_LEGACY_ESCROW_CUSTODIAN;
        assert_ok!(EterraSlots::init_card_nft_collection(
            RuntimeOrigin::signed(1),
            1
        ));

        let ordinary = NextCardId::<Test>::get();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(2)));

        let nft_wrapped = NextCardId::<Test>::get();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(2)));
        assert_ok!(EterraSlots::convert_to_nft(
            RuntimeOrigin::signed(2),
            nft_wrapped
        ));

        let known_escrow = NextCardId::<Test>::get();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(3)));
        assert_ok!(EterraSlots::transfer_card(
            RuntimeOrigin::signed(3),
            known_escrow,
            external_escrow
        ));
        set_mock_legacy_escrow_owner(known_escrow, 3);

        let stale_external_entry = NextCardId::<Test>::get();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(3)));
        set_mock_legacy_escrow_owner(stale_external_entry, 3);

        let missing_external_entry = NextCardId::<Test>::get();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(3)));
        assert_ok!(EterraSlots::transfer_card(
            RuntimeOrigin::signed(3),
            missing_external_entry,
            external_escrow
        ));

        let unknown_escrow = NextCardId::<Test>::get();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(1)));
        assert_ok!(EterraSlots::transfer_card(
            RuntimeOrigin::signed(1),
            unknown_escrow,
            escrow
        ));

        StorageVersion::new(15).put::<EterraSlots>();
        <EterraSlots as Hooks<u64>>::on_runtime_upgrade();
        assert_eq!(EterraSlots::migrate_v16_batch(100), 6);
        assert_eq!(
            crate::TcgMigrationStateStorageV16::<Test>::get()
                .unwrap()
                .phase,
            crate::MigrationPhaseV16::AwaitingVerification
        );
        attest_v16_migration();

        let ordinary_class =
            crate::LegacyCardClassifications::<Test>::get(ordinary).expect("ordinary classified");
        assert_eq!(ordinary_class.custody, crate::LegacyCustodyKind::Ordinary);
        assert_eq!(ordinary_class.beneficial_owner, Some(2));
        assert!(!ordinary_class.frozen);

        let nft_class = crate::LegacyCardClassifications::<Test>::get(nft_wrapped)
            .expect("wrapped card classified");
        assert_eq!(nft_class.custody, crate::LegacyCustodyKind::NftWrapped);
        assert_eq!(nft_class.beneficial_owner, Some(2));
        assert!(!nft_class.frozen);

        let known_class = crate::LegacyCardClassifications::<Test>::get(known_escrow)
            .expect("known escrow classified");
        assert_eq!(known_class.custody, crate::LegacyCustodyKind::KnownEscrow);
        assert_eq!(known_class.beneficial_owner, Some(3));
        assert!(!known_class.frozen);

        for anomaly_id in [stale_external_entry, missing_external_entry, unknown_escrow] {
            let classification = crate::LegacyCardClassifications::<Test>::get(anomaly_id)
                .expect("anomaly classified");
            assert_eq!(
                classification.custody,
                crate::LegacyCustodyKind::UnknownFrozen
            );
            assert_eq!(classification.beneficial_owner, None);
            assert!(classification.frozen);
            assert!(crate::TcgMigrationAnomaliesV16::<Test>::contains_key(
                anomaly_id
            ));
        }

        let unknown_class = crate::LegacyCardClassifications::<Test>::get(unknown_escrow)
            .expect("unknown escrow classified");
        assert_eq!(
            unknown_class.custody,
            crate::LegacyCustodyKind::UnknownFrozen
        );
        assert_eq!(unknown_class.beneficial_owner, None);
        assert!(unknown_class.frozen);
        assert!(crate::TcgMigrationAnomaliesV16::<Test>::contains_key(
            unknown_escrow
        ));

        let state = crate::TcgMigrationStateStorageV16::<Test>::get().expect("migration state");
        assert_eq!(state.phase, crate::MigrationPhaseV16::Completed);
        assert_eq!(state.cards_seen, 6);
        assert_eq!(state.ordinary, 1);
        assert_eq!(state.nft_wrapped, 1);
        assert_eq!(state.known_escrow, 1);
        assert_eq!(state.anomalies, 3);
        assert!(crate::RepairedLegacyCardsByOwnerV16::<Test>::get(
            2, ordinary
        ));
        assert!(crate::RepairedLegacyCardsByOwnerV16::<Test>::get(
            2,
            nft_wrapped
        ));
        assert!(crate::RepairedLegacyCardsByOwnerV16::<Test>::get(
            3,
            known_escrow
        ));
        assert!(!crate::RepairedLegacyCardsByOwnerV16::<Test>::get(
            1,
            unknown_escrow
        ));

        assert_noop!(
            EterraSlots::set_price(RuntimeOrigin::signed(escrow), unknown_escrow, 1),
            Error::<Test>::LegacyCardFrozen
        );
        assert_noop!(
            EterraSlots::transfer_card(RuntimeOrigin::signed(escrow), unknown_escrow, 4),
            Error::<Test>::LegacyCardFrozen
        );

        assert_ok!(EterraSlots::transfer_card(
            RuntimeOrigin::signed(2),
            ordinary,
            4
        ));
        let moved =
            crate::LegacyCardClassifications::<Test>::get(ordinary).expect("sidecar retained");
        assert_eq!(moved.beneficial_owner, Some(4));
        assert_eq!(moved.custody, crate::LegacyCustodyKind::Ordinary);
        assert!(!crate::RepairedLegacyCardsByOwnerV16::<Test>::get(
            2, ordinary
        ));
        assert!(crate::RepairedLegacyCardsByOwnerV16::<Test>::get(
            4, ordinary
        ));

        assert_ok!(EterraSlots::unwrap_from_nft(
            RuntimeOrigin::signed(2),
            nft_wrapped
        ));
        let unwrapped =
            crate::LegacyCardClassifications::<Test>::get(nft_wrapped).expect("sidecar retained");
        assert_eq!(unwrapped.beneficial_owner, Some(2));
        assert_eq!(unwrapped.custody, crate::LegacyCustodyKind::Ordinary);
    });
}

#[test]
fn v15_to_v16_migration_preserves_vault_owner_and_safe_exit() {
    use frame_support::traits::{Hooks, StorageVersion};

    new_test_ext().execute_with(|| {
        let owner = 1;
        let recipient = 2;
        let card_id = NextCardId::<Test>::get();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(owner)));
        seed_collection_card(owner, card_id, 10);
        NexusCollectionCards::<Test>::mutate(card_id, |record| {
            record.as_mut().expect("seeded card").location = NexusStorageLocation::Vault;
        });
        NexusSubjectCopyCounts::<Test>::insert(owner, 2, 1);
        let metadata_uri = b"ipfs://nexus-v2-vault-variant"
            .to_vec()
            .try_into()
            .expect("bounded URI");
        crate::VaultVariants::<Test>::insert(
            7,
            crate::VaultVariant {
                variant_id: 7,
                card_record_id: card_id,
                subject_id: 2,
                sealed_at: System::block_number(),
                metadata_uri,
                trade_eligible: true,
                config_version: 1,
            },
        );

        StorageVersion::new(15).put::<EterraSlots>();
        <EterraSlots as Hooks<u64>>::on_runtime_upgrade();
        assert_eq!(EterraSlots::migrate_v16_batch(100), 1);
        attest_v16_migration();

        let classification =
            crate::LegacyCardClassifications::<Test>::get(card_id).expect("classified");
        assert_eq!(classification.custody, crate::LegacyCustodyKind::Ordinary);
        assert_eq!(classification.beneficial_owner, Some(owner));
        assert!(!classification.frozen);
        assert!(!crate::TcgMigrationAnomaliesV16::<Test>::contains_key(
            card_id
        ));

        assert_ok!(EterraSlots::transfer_card(
            RuntimeOrigin::signed(owner),
            card_id,
            recipient
        ));
        let moved =
            crate::LegacyCardClassifications::<Test>::get(card_id).expect("sidecar retained");
        assert_eq!(moved.beneficial_owner, Some(recipient));
        assert_eq!(moved.custody, crate::LegacyCustodyKind::Ordinary);
        assert_eq!(
            NexusCollectionCards::<Test>::get(card_id)
                .expect("Nexus record retained")
                .owner,
            recipient
        );
        assert_eq!(
            crate::VaultVariants::<Test>::get(7)
                .expect("Vault metadata retained")
                .card_record_id,
            card_id
        );
    });
}

#[test]
fn v16_on_idle_respects_both_ref_time_and_proof_size_limits() {
    use frame_support::traits::{Hooks, StorageVersion};
    use frame_support::weights::Weight;

    new_test_ext().execute_with(|| {
        for owner in 1..=3 {
            assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(owner)));
        }
        StorageVersion::new(14).put::<EterraSlots>();
        <EterraSlots as Hooks<u64>>::on_runtime_upgrade();

        let proof_constrained = Weight::from_parts(u64::MAX / 4, 64 * 1024);
        let used = <EterraSlots as Hooks<u64>>::on_idle(1, proof_constrained);
        assert!(used.all_lte(proof_constrained));
        assert_eq!(
            crate::TcgMigrationStateStorageV16::<Test>::get()
                .unwrap()
                .cursor,
            0
        );

        let generous = Weight::from_parts(u64::MAX / 4, u64::MAX / 4);
        let used = <EterraSlots as Hooks<u64>>::on_idle(2, generous);
        assert!(used.all_lte(generous));
        assert_eq!(
            crate::TcgMigrationStateStorageV16::<Test>::get()
                .unwrap()
                .phase,
            crate::MigrationPhaseV16::AwaitingVerification
        );
        assert!(crate::LegacyWritesPausedV16::<Test>::get());
        attest_v16_migration();
    });
}

#[test]
fn legacy_and_v2_call_indices_are_scale_frozen() {
    let calls = [
        (
            crate::Call::<Test>::transfer_card { card_id: 7, to: 2 }.encode()[0],
            3,
        ),
        (
            crate::Call::<Test>::finalize_pack_open_v2 {
                opening_id: [1; 32],
            }
            .encode()[0],
            45,
        ),
        (
            crate::Call::<Test>::timeout_pack_open_v2 {
                opening_id: [2; 32],
            }
            .encode()[0],
            46,
        ),
        (
            crate::Call::<Test>::set_v2_feature_enabled {
                feature: crate::V2Feature::Conversion,
                enabled: false,
            }
            .encode()[0],
            49,
        ),
        (
            crate::Call::<Test>::finalize_conversion_v2 {
                request_id: [3; 32],
            }
            .encode()[0],
            51,
        ),
        (
            crate::Call::<Test>::timeout_conversion_v2 {
                request_id: [4; 32],
            }
            .encode()[0],
            52,
        ),
        (
            crate::Call::<Test>::configure_mythical_ascension_season_v2 {
                config: crate::MythicalAscensionSeasonConfig {
                    season_id: 1,
                    set_id: 1,
                    pool_id: 1,
                    pool_version: 1,
                    starts_at: 10,
                    ends_at: 100,
                    required_mastery: 10,
                    required_marks: 10,
                    available_weeks: 12,
                    config_hash: [5; 32],
                },
            }
            .encode()[0],
            53,
        ),
        (
            crate::Call::<Test>::configure_mythical_ascension_subject_v2 {
                config: crate::MythicalAscensionSubjectConfig {
                    season_id: 1,
                    subject_id: 1,
                    subject_version: 1,
                    foundation_pose_definition_id: 10,
                    foundation_background_definition_id: 1000,
                    config_hash: [6; 32],
                },
            }
            .encode()[0],
            54,
        ),
        (
            crate::Call::<Test>::link_season_eligibility_v2 {
                account: 1,
                season_id: 1,
                season_eligibility_id: [7; 32],
            }
            .encode()[0],
            55,
        ),
        (
            crate::Call::<Test>::record_mythical_ascension_progress_v2 {
                season_eligibility_id: [8; 32],
                season_id: 1,
                subject_id: 1,
                economic_realm: eterra_nexus_primitives::EconomicRealm::Production,
                mastery_level: Some(10),
                convergence_week: Some(0),
                grant_catalyst: true,
                evidence_id: [9; 32],
            }
            .encode()[0],
            56,
        ),
        (
            crate::Call::<Test>::ascend_mythical_v2 {
                season_id: 1,
                subject_id: 1,
                input: crate::MythicalAscensionInput::LegendaryFoundation,
            }
            .encode()[0],
            57,
        ),
        (
            crate::Call::<Test>::complete_legacy_migration_v16 {
                expected_cards_seen: 1,
                expected_anomalies: 0,
                verification_hash: [10; 32],
            }
            .encode()[0],
            58,
        ),
        (
            crate::Call::<Test>::transfer_wrapped_card_nft_v16 {
                card_id: 1,
                new_owner: 2,
            }
            .encode()[0],
            59,
        ),
    ];
    for (encoded, expected) in calls {
        assert_eq!(encoded, expected);
    }
}

#[test]
fn v2_team_call_rejects_oversized_vectors_during_scale_decode() {
    let mut encoded = vec![48u8];
    let oversized = (1..=u64::from(MaxV2TeamSize::get()) + 1).collect::<Vec<_>>();
    encoded.extend((1u32, 1u32, 1u32, oversized).encode());
    assert!(
        crate::Call::<Test>::decode(&mut encoded.as_slice()).is_err(),
        "the call boundary must reject more than MaxV2TeamSize before dispatch"
    );
}

#[test]
fn legacy_magic_vec_weight_scales_with_decoded_length() {
    let short = <() as crate::weights::WeightInfo>::set_card_magic_loadout(3);
    let oversized = <() as crate::weights::WeightInfo>::set_card_magic_loadout(1_000_000);
    assert!(short.ref_time() < oversized.ref_time());
    assert!(short.proof_size() < oversized.proof_size());
}
