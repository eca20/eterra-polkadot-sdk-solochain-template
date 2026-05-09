use crate::pallet::Config as EterraSlotsConfig;
use crate::{
    mock::*, ActiveCard, ApexSide, CardArtworkCollectionId, CardCapacityBonus, CardPrices, Cards,
    CardsByOwner, CollectionCard, Element, ElementProfile, Error, Event, ForgeBranch, GearSlotType,
    GearTier, GeneProfile, ListedByOwner, MatchMode, MatchStatus, NextCardId, NextNexusGearId,
    NextNexusMatchId, NextStarterGrantId, NextVaultVariantId, NexusAccountStates, NexusCardKind,
    NexusCardOrigin, NexusCollectionCards, NexusEquippedGear, NexusGearItems, NexusMatchBoards,
    NexusMatches, NexusOverflowCards, NexusOverflowSubjectCounts, NexusResources, NexusSpellbook,
    NexusStorageLocation, NexusSubjectCopyCounts, NexusTeams, NexusTrials,
    NexusVaultVariantsByOwner, PackCardInProgress, PackInProgress, PlayerPacks, RankValue,
    ResourceKind, SeasonCollectionIds, SeasonCollectionStatus, SeasonCollections, SpellSlotKind,
    StarterGrants, StarterPath, TrialStatus, VaultVariants,
};
use frame_support::traits::Get;
use frame_support::{assert_noop, assert_ok, BoundedBTreeSet, BoundedVec};
use log::{debug, Level, Metadata, Record};
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

fn ranks(top: RankValue, right: RankValue, bottom: RankValue, left: RankValue) -> [RankValue; 4] {
    [top, right, bottom, left]
}

fn all_number(value: u8) -> [RankValue; 4] {
    [RankValue::Number(value); 4]
}

fn profile(main: Element, weakness: Option<Element>) -> ElementProfile {
    ElementProfile {
        main,
        minor: None,
        resistance: None,
        weakness,
    }
}

fn seed_nexus_card(
    owner: u64,
    card_id: u32,
    subject_id: u32,
    base_ranks: [RankValue; 4],
    card_power: u16,
    location: NexusStorageLocation,
    element_profile: ElementProfile,
) {
    NexusCollectionCards::<Test>::insert(
        card_id,
        CollectionCard {
            owner,
            subject_id,
            kind: NexusCardKind::Echo,
            origin: NexusCardOrigin::Claim,
            base_ranks,
            apex_side: base_ranks
                .iter()
                .position(|rank| *rank == RankValue::Apex)
                .map(|index| match index {
                    0 => ApexSide::Top,
                    1 => ApexSide::Right,
                    2 => ApexSide::Bottom,
                    _ => ApexSide::Left,
                }),
            genes: GeneProfile::default(),
            element_profile,
            card_power,
            location,
            account_bound: false,
            acquired_at: System::block_number(),
            config_version: 1,
        },
    );
}

fn seed_workshop_resources(owner: u64, amount: u32) {
    for kind in [
        ResourceKind::GearParts,
        ResourceKind::ElementShards,
        ResourceKind::EchoCoreFragments,
        ResourceKind::EchoCores,
        ResourceKind::ForgeStars,
    ] {
        NexusResources::<Test>::insert(owner, kind, amount);
    }
}

fn seed_nexus_team(owner: u64, first_card_id: u32) -> Vec<u32> {
    let mut ids = Vec::new();
    for offset in 0..5u32 {
        let card_id = first_card_id + offset;
        seed_nexus_card(
            owner,
            card_id,
            card_id,
            all_number(1),
            1,
            NexusStorageLocation::Collection,
            profile(Element::Fire, Some(Element::Water)),
        );
        ids.push(card_id);
    }
    ids
}

fn save_seeded_team(owner: u64, team_id: u32, card_ids: Vec<u32>) {
    assert_ok!(EterraSlots::save_nexus_team(
        RuntimeOrigin::signed(owner),
        team_id,
        card_ids
    ));
}

fn start_seeded_match(
    player: u64,
    opponent: u64,
    board_id: u32,
    mode: MatchMode,
) -> (u32, u64, u64) {
    assert_ok!(EterraSlots::start_nexus_match(
        RuntimeOrigin::signed(player),
        opponent,
        mode,
        board_id,
        1,
        1
    ));
    let match_id = NextNexusMatchId::<Test>::get() - 1;
    let state = NexusMatches::<Test>::get(match_id).expect("match state exists");
    let first = state.first_player.expect("first player set");
    let second = if first == player { opponent } else { player };
    (match_id, first, second)
}

fn card_for(player: u64, player_one: u64, first_base: u32, second_base: u32, offset: u32) -> u32 {
    if player == player_one {
        first_base + offset
    } else {
        second_base + offset
    }
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
fn claim_starter_grant_initializes_nexus_account_state() {
    new_test_ext().execute_with(|| {
        let player = 2u64;

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
    });
}

#[test]
fn claim_starter_grant_rejects_duplicates() {
    new_test_ext().execute_with(|| {
        let player = 2u64;

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
fn save_nexus_team_validates_ownership_duplicates_and_overflow() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let cards = seed_nexus_team(player, 100);

        assert_ok!(EterraSlots::save_nexus_team(
            RuntimeOrigin::signed(player),
            1,
            cards.clone()
        ));
        let team = NexusTeams::<Test>::get(player, 1).expect("team should be stored");
        assert_eq!(team.card_ids.to_vec(), cards);
        assert_eq!(team.team_power, 5);

        assert_event_found(
            |event| {
                matches!(
                    event,
                    RuntimeEvent::EterraSlots(Event::TeamSaved {
                        account_id,
                        team_id,
                        team_power,
                        ..
                    }) if *account_id == player && *team_id == 1 && *team_power == 5
                )
            },
            "TeamSaved",
        );

        assert_noop!(
            EterraSlots::save_nexus_team(
                RuntimeOrigin::signed(player),
                2,
                vec![100, 101, 102, 103]
            ),
            Error::<Test>::NexusTeamSizeInvalid
        );
        assert_noop!(
            EterraSlots::save_nexus_team(
                RuntimeOrigin::signed(player),
                2,
                vec![100, 100, 101, 102, 103]
            ),
            Error::<Test>::NexusTeamDuplicateCard
        );

        seed_nexus_card(
            3,
            200,
            200,
            all_number(1),
            1,
            NexusStorageLocation::Collection,
            profile(Element::Earth, None),
        );
        assert_noop!(
            EterraSlots::save_nexus_team(
                RuntimeOrigin::signed(player),
                2,
                vec![100, 101, 102, 103, 200]
            ),
            Error::<Test>::NotCardOwner
        );

        seed_nexus_card(
            player,
            201,
            201,
            all_number(1),
            1,
            NexusStorageLocation::Overflow,
            profile(Element::Earth, None),
        );
        assert_noop!(
            EterraSlots::save_nexus_team(
                RuntimeOrigin::signed(player),
                2,
                vec![100, 101, 102, 103, 201]
            ),
            Error::<Test>::NexusCardNotPlayable
        );
    });
}

#[test]
fn salvage_nexus_card_grants_deterministic_workshop_resources() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        seed_nexus_card(
            player,
            100,
            42,
            all_number(2),
            6,
            NexusStorageLocation::Collection,
            profile(Element::Fire, None),
        );
        NexusSubjectCopyCounts::<Test>::insert(player, 42, 1);

        assert_ok!(EterraSlots::salvage_nexus_card(
            RuntimeOrigin::signed(player),
            100
        ));

        let card = NexusCollectionCards::<Test>::get(100).expect("card remains as record");
        assert_eq!(card.location, NexusStorageLocation::Salvaged);
        assert_eq!(NexusSubjectCopyCounts::<Test>::get(player, 42), 0);
        assert_eq!(
            NexusResources::<Test>::get(player, ResourceKind::GearParts),
            7
        );
        assert_eq!(
            NexusResources::<Test>::get(player, ResourceKind::ElementShards),
            1
        );
        assert_eq!(
            NexusResources::<Test>::get(player, ResourceKind::EonCoins),
            0
        );

        assert_event_found(
            |event| {
                matches!(
                    event,
                    RuntimeEvent::EterraSlots(Event::CardSalvaged {
                        account_id,
                        card_record_id,
                        outputs,
                        salvage_table_version
                    }) if *account_id == player
                        && *card_record_id == 100
                        && outputs.gear_parts == 7
                        && outputs.element_shards == 1
                        && outputs.eon_coins == 0
                        && *salvage_table_version == 1
                )
            },
            "CardSalvaged",
        );

        assert_noop!(
            EterraSlots::salvage_nexus_card(RuntimeOrigin::signed(player), 100),
            Error::<Test>::NexusCardNotInCollection
        );
    });
}

#[test]
fn seal_nexus_card_creates_vault_variant_and_enforces_capacity() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        assert_ok!(EterraSlots::claim_starter_grant(
            RuntimeOrigin::signed(player),
            StarterPath::Fire
        ));
        seed_nexus_card(
            player,
            100,
            42,
            all_number(2),
            3,
            NexusStorageLocation::Collection,
            profile(Element::Fire, None),
        );

        assert_ok!(EterraSlots::seal_nexus_card(
            RuntimeOrigin::signed(player),
            100,
            b"ipfs://nexus/card-100".to_vec()
        ));

        let card = NexusCollectionCards::<Test>::get(100).expect("card remains as record");
        assert_eq!(card.location, NexusStorageLocation::Vault);
        assert_eq!(NextVaultVariantId::<Test>::get(), 1);
        assert_eq!(
            NexusVaultVariantsByOwner::<Test>::get(player).to_vec(),
            vec![0]
        );
        let variant = VaultVariants::<Test>::get(0).expect("vault variant stored");
        assert_eq!(variant.card_record_id, 100);
        assert_eq!(variant.subject_id, 42);
        assert_eq!(
            variant.metadata_uri.to_vec(),
            b"ipfs://nexus/card-100".to_vec()
        );
        assert!(!variant.trade_eligible);

        NexusAccountStates::<Test>::mutate(player, |state| {
            state.as_mut().expect("account state exists").vault_capacity = 1;
        });
        seed_nexus_card(
            player,
            101,
            43,
            all_number(2),
            3,
            NexusStorageLocation::Collection,
            profile(Element::Earth, None),
        );
        assert_noop!(
            EterraSlots::seal_nexus_card(
                RuntimeOrigin::signed(player),
                101,
                b"ipfs://nexus/card-101".to_vec()
            ),
            Error::<Test>::NexusVaultCapacityExceeded
        );
    });
}

#[test]
fn craft_and_equip_nexus_gear_updates_card_build_power() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let cards = seed_nexus_team(player, 100);
        seed_workshop_resources(player, 100);

        assert_ok!(EterraSlots::craft_nexus_gear(
            RuntimeOrigin::signed(player),
            1
        ));
        assert_eq!(NextNexusGearId::<Test>::get(), 1);
        let gear = NexusGearItems::<Test>::get(0).expect("gear stored");
        assert_eq!(gear.slot_type, GearSlotType::Weapon);
        assert_eq!(gear.tier, GearTier::Common);
        assert_eq!(gear.power, 2);
        assert_eq!(
            NexusResources::<Test>::get(player, ResourceKind::GearParts),
            90
        );
        assert_eq!(
            NexusResources::<Test>::get(player, ResourceKind::ElementShards),
            98
        );

        assert_ok!(EterraSlots::equip_nexus_gear(
            RuntimeOrigin::signed(player),
            100,
            0
        ));
        assert_eq!(
            NexusEquippedGear::<Test>::get(100, GearSlotType::Weapon),
            Some(0)
        );

        assert_ok!(EterraSlots::save_nexus_team(
            RuntimeOrigin::signed(player),
            1,
            cards
        ));
        let team = NexusTeams::<Test>::get(player, 1).expect("team stored");
        assert_eq!(team.team_power, 7);

        assert_ok!(EterraSlots::craft_nexus_gear(
            RuntimeOrigin::signed(player),
            1
        ));
        assert_noop!(
            EterraSlots::equip_nexus_gear(RuntimeOrigin::signed(player), 100, 1),
            Error::<Test>::NexusGearSlotOccupied
        );
    });
}

#[test]
fn spell_slotting_validates_element_open_and_locked_slots() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        seed_nexus_card(
            player,
            100,
            42,
            all_number(2),
            3,
            NexusStorageLocation::Collection,
            profile(Element::Fire, None),
        );
        seed_workshop_resources(player, 100);

        assert_ok!(EterraSlots::craft_nexus_gear(
            RuntimeOrigin::signed(player),
            1
        ));
        assert_ok!(EterraSlots::equip_nexus_gear(
            RuntimeOrigin::signed(player),
            100,
            0
        ));
        assert_ok!(EterraSlots::craft_nexus_spell(
            RuntimeOrigin::signed(player),
            3
        ));
        assert_noop!(
            EterraSlots::slot_nexus_spell(RuntimeOrigin::signed(player), 100, 0, 0, 0),
            Error::<Test>::NexusSpellElementMismatch
        );

        assert_ok!(EterraSlots::craft_nexus_spell(
            RuntimeOrigin::signed(player),
            1
        ));
        assert_ok!(EterraSlots::slot_nexus_spell(
            RuntimeOrigin::signed(player),
            100,
            0,
            0,
            1
        ));
        assert_eq!(
            NexusSpellbook::<Test>::get(1)
                .expect("fire spell stored")
                .slotted_to,
            Some((0, 0))
        );

        assert_ok!(EterraSlots::slot_nexus_spell(
            RuntimeOrigin::signed(player),
            100,
            0,
            1,
            0
        ));
        assert_ok!(EterraSlots::craft_nexus_spell(
            RuntimeOrigin::signed(player),
            2
        ));
        assert_noop!(
            EterraSlots::slot_nexus_spell(RuntimeOrigin::signed(player), 100, 0, 2, 2),
            Error::<Test>::NexusSpellSlotLocked
        );

        assert_ok!(EterraSlots::unslot_nexus_spell(
            RuntimeOrigin::signed(player),
            100,
            0,
            0,
            1
        ));
        assert_eq!(
            NexusSpellbook::<Test>::get(1)
                .expect("fire spell stored")
                .slotted_to,
            None
        );
        let gear = NexusGearItems::<Test>::get(0).expect("gear stored");
        assert_eq!(gear.spell_slots[0].spell_id, None);
        assert_eq!(
            gear.spell_slots[0].slot_kind,
            SpellSlotKind::Element(Element::Fire)
        );
    });
}

#[test]
fn trials_grant_forge_stars_and_gate_weapon_forge_paths() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        NexusResources::<Test>::insert(player, ResourceKind::GearParts, 200);
        NexusResources::<Test>::insert(player, ResourceKind::ElementShards, 200);

        assert_ok!(EterraSlots::craft_nexus_gear(
            RuntimeOrigin::signed(player),
            1
        ));
        assert_ok!(EterraSlots::forge_nexus_weapon(
            RuntimeOrigin::signed(player),
            0,
            ForgeBranch::Sword
        ));
        assert_eq!(
            NexusGearItems::<Test>::get(0).expect("gear stored").tier,
            GearTier::Rare
        );
        assert_ok!(EterraSlots::forge_nexus_weapon(
            RuntimeOrigin::signed(player),
            0,
            ForgeBranch::Sword
        ));
        assert_eq!(
            NexusGearItems::<Test>::get(0).expect("gear stored").tier,
            GearTier::Epic
        );
        assert_noop!(
            EterraSlots::forge_nexus_weapon(RuntimeOrigin::signed(player), 0, ForgeBranch::Sword),
            Error::<Test>::NexusForgeGateMissing
        );

        assert_ok!(EterraSlots::start_nexus_trial(
            RuntimeOrigin::signed(player),
            1
        ));
        assert_noop!(
            EterraSlots::start_nexus_trial(RuntimeOrigin::signed(player), 1),
            Error::<Test>::NexusTrialAlreadyStarted
        );
        assert_ok!(EterraSlots::complete_nexus_trial(
            RuntimeOrigin::signed(player),
            1,
            true
        ));
        let trial = NexusTrials::<Test>::get(player, 1).expect("trial stored");
        assert_eq!(trial.status, TrialStatus::Completed);
        assert_eq!(
            NexusResources::<Test>::get(player, ResourceKind::ForgeStars),
            1
        );

        assert_ok!(EterraSlots::forge_nexus_weapon(
            RuntimeOrigin::signed(player),
            0,
            ForgeBranch::Sword
        ));
        let gear = NexusGearItems::<Test>::get(0).expect("gear stored");
        assert_eq!(gear.tier, GearTier::Legendary);
        assert_eq!(gear.power, 8);
        assert_eq!(
            NexusResources::<Test>::get(player, ResourceKind::ForgeStars),
            0
        );
    });
}

#[test]
fn start_nexus_match_stores_board_hands_and_events() {
    new_test_ext().execute_with(|| {
        let p1_cards = seed_nexus_team(2, 100);
        let p2_cards = seed_nexus_team(3, 200);
        save_seeded_team(2, 1, p1_cards.clone());
        save_seeded_team(3, 1, p2_cards.clone());

        assert_ok!(EterraSlots::start_nexus_match(
            RuntimeOrigin::signed(2),
            3,
            MatchMode::Quick,
            2,
            1,
            1
        ));

        let state = NexusMatches::<Test>::get(0).expect("match state should exist");
        assert_eq!(state.match_id, 0);
        assert_eq!(state.mode, MatchMode::Quick);
        assert_eq!(state.board_id, 2);
        assert_eq!(state.status, MatchStatus::Active);
        assert_eq!(state.turn_index, 0);
        assert!(state.first_player == Some(2) || state.first_player == Some(3));
        assert_eq!(state.players.to_vec(), vec![2, 3]);

        let board = NexusMatchBoards::<Test>::get(0).expect("board should exist");
        assert_eq!(board.locked_cells, (1u16 << 3) | (1u16 << 12));
        assert_eq!(board.mana_wells, (1u16 << 5) | (1u16 << 10));
        assert_eq!(board.cells.len(), 16);

        assert_eq!(EterraSlots::nexus_match_hand(0, 2).to_vec(), p1_cards);
        assert_eq!(EterraSlots::nexus_match_hand(0, 3).to_vec(), p2_cards);

        assert_event_found(
            |event| {
                matches!(
                    event,
                    RuntimeEvent::EterraSlots(Event::MatchStarted {
                        match_id,
                        mode,
                        board_id,
                        ..
                    }) if *match_id == 0 && *mode == MatchMode::Quick && *board_id == 2
                )
            },
            "MatchStarted",
        );
    });
}

#[test]
fn play_nexus_match_card_rejects_illegal_moves() {
    new_test_ext().execute_with(|| {
        let p1_cards = seed_nexus_team(2, 100);
        let p2_cards = seed_nexus_team(3, 200);
        save_seeded_team(2, 1, p1_cards);
        save_seeded_team(3, 1, p2_cards);
        let (match_id, first, second) = start_seeded_match(2, 3, 2, MatchMode::Quick);
        let first_card = card_for(first, 2, 100, 200, 0);
        let second_card = card_for(second, 2, 100, 200, 0);

        assert_noop!(
            EterraSlots::play_nexus_match_card(
                RuntimeOrigin::signed(second),
                match_id,
                second_card,
                4,
                None
            ),
            Error::<Test>::NexusNotPlayerTurn
        );
        assert_noop!(
            EterraSlots::play_nexus_match_card(
                RuntimeOrigin::signed(first),
                match_id,
                first_card,
                3,
                None
            ),
            Error::<Test>::NexusCellLocked
        );
        assert_noop!(
            EterraSlots::play_nexus_match_card(
                RuntimeOrigin::signed(first),
                match_id,
                first_card,
                16,
                None
            ),
            Error::<Test>::NexusCellOutOfBounds
        );

        assert_ok!(EterraSlots::play_nexus_match_card(
            RuntimeOrigin::signed(first),
            match_id,
            first_card,
            4,
            None
        ));
        assert_noop!(
            EterraSlots::play_nexus_match_card(
                RuntimeOrigin::signed(second),
                match_id,
                second_card,
                4,
                None
            ),
            Error::<Test>::NexusCellOccupied
        );
        assert_noop!(
            EterraSlots::play_nexus_match_card(
                RuntimeOrigin::signed(second),
                match_id,
                first_card,
                5,
                None
            ),
            Error::<Test>::NexusCardNotInHand
        );
    });
}

#[test]
fn apex_capture_flips_adjacent_cards_without_chains() {
    new_test_ext().execute_with(|| {
        let first_base = 100;
        let second_base = 200;
        seed_nexus_team(2, first_base);
        seed_nexus_team(3, second_base);
        save_seeded_team(2, 1, (first_base..first_base + 5).collect());
        save_seeded_team(3, 1, (second_base..second_base + 5).collect());
        let (match_id, first, second) = start_seeded_match(2, 3, 0, MatchMode::Quick);

        let first_a = card_for(first, 2, first_base, second_base, 0);
        let first_c = card_for(first, 2, first_base, second_base, 1);
        let second_filler = card_for(second, 2, first_base, second_base, 0);
        let second_attacker = card_for(second, 2, first_base, second_base, 1);

        seed_nexus_card(
            first,
            first_a,
            first_a,
            ranks(
                RankValue::Number(1),
                RankValue::Number(9),
                RankValue::Number(1),
                RankValue::Number(1),
            ),
            1,
            NexusStorageLocation::Collection,
            profile(Element::Fire, Some(Element::Water)),
        );
        seed_nexus_card(
            second,
            second_attacker,
            second_attacker,
            ranks(
                RankValue::Number(1),
                RankValue::Apex,
                RankValue::Number(1),
                RankValue::Number(1),
            ),
            1,
            NexusStorageLocation::Collection,
            profile(Element::Earth, None),
        );

        assert_ok!(EterraSlots::play_nexus_match_card(
            RuntimeOrigin::signed(first),
            match_id,
            first_a,
            5,
            None
        ));
        assert_ok!(EterraSlots::play_nexus_match_card(
            RuntimeOrigin::signed(second),
            match_id,
            second_filler,
            15,
            None
        ));
        assert_ok!(EterraSlots::play_nexus_match_card(
            RuntimeOrigin::signed(first),
            match_id,
            first_c,
            6,
            None
        ));
        assert_ok!(EterraSlots::play_nexus_match_card(
            RuntimeOrigin::signed(second),
            match_id,
            second_attacker,
            4,
            None
        ));

        let board = NexusMatchBoards::<Test>::get(match_id).expect("board exists");
        assert_eq!(board.cells[5].card.as_ref().unwrap().controller, second);
        assert_eq!(
            board.cells[6].card.as_ref().unwrap().controller,
            first,
            "direct capture must not chain into the next adjacent card"
        );
        assert_event_found(
            |event| {
                matches!(
                    event,
                    RuntimeEvent::EterraSlots(Event::CardCaptured {
                        match_id: seen_match_id,
                        attacker_card_id,
                        captured_card_id,
                        side,
                        ..
                    }) if *seen_match_id == match_id
                        && *attacker_card_id == second_attacker
                        && *captured_card_id == first_a
                        && *side == ApexSide::Right
                )
            },
            "CardCaptured",
        );
    });
}

#[test]
fn apex_ties_and_equal_numeric_ranks_do_not_capture() {
    new_test_ext().execute_with(|| {
        let first_base = 300;
        let second_base = 400;
        seed_nexus_team(2, first_base);
        seed_nexus_team(3, second_base);
        save_seeded_team(2, 1, (first_base..first_base + 5).collect());
        save_seeded_team(3, 1, (second_base..second_base + 5).collect());
        let (match_id, first, second) = start_seeded_match(2, 3, 0, MatchMode::Quick);

        let defender = card_for(first, 2, first_base, second_base, 0);
        let attacker = card_for(second, 2, first_base, second_base, 0);
        seed_nexus_card(
            first,
            defender,
            defender,
            ranks(
                RankValue::Number(1),
                RankValue::Number(1),
                RankValue::Number(1),
                RankValue::Apex,
            ),
            1,
            NexusStorageLocation::Collection,
            profile(Element::Fire, None),
        );
        seed_nexus_card(
            second,
            attacker,
            attacker,
            ranks(
                RankValue::Number(1),
                RankValue::Apex,
                RankValue::Number(1),
                RankValue::Number(1),
            ),
            1,
            NexusStorageLocation::Collection,
            profile(Element::Earth, None),
        );

        assert_ok!(EterraSlots::play_nexus_match_card(
            RuntimeOrigin::signed(first),
            match_id,
            defender,
            5,
            None
        ));
        assert_ok!(EterraSlots::play_nexus_match_card(
            RuntimeOrigin::signed(second),
            match_id,
            attacker,
            4,
            None
        ));

        let board = NexusMatchBoards::<Test>::get(match_id).expect("board exists");
        assert_eq!(board.cells[5].card.as_ref().unwrap().controller, first);
    });
}

#[test]
fn rune_cell_triggers_before_capture_and_respects_mana_well_rules() {
    new_test_ext().execute_with(|| {
        let first_base = 500;
        let second_base = 600;
        seed_nexus_team(2, first_base);
        seed_nexus_team(3, second_base);
        save_seeded_team(2, 1, (first_base..first_base + 5).collect());
        save_seeded_team(3, 1, (second_base..second_base + 5).collect());
        let (match_id, first, second) = start_seeded_match(2, 3, 2, MatchMode::Quick);

        let caster = card_for(first, 2, first_base, second_base, 0);
        let trigger_card = card_for(second, 2, first_base, second_base, 0);
        seed_nexus_card(
            first,
            caster,
            caster,
            ranks(
                RankValue::Number(1),
                RankValue::Number(5),
                RankValue::Number(1),
                RankValue::Number(1),
            ),
            1,
            NexusStorageLocation::Collection,
            profile(Element::Earth, None),
        );
        seed_nexus_card(
            second,
            trigger_card,
            trigger_card,
            ranks(
                RankValue::Number(1),
                RankValue::Number(1),
                RankValue::Number(1),
                RankValue::Number(5),
            ),
            1,
            NexusStorageLocation::Collection,
            profile(Element::Fire, Some(Element::Water)),
        );

        assert_ok!(EterraSlots::play_nexus_match_card(
            RuntimeOrigin::signed(first),
            match_id,
            caster,
            4,
            Some((5, Element::Fire))
        ));
        assert_ok!(EterraSlots::play_nexus_match_card(
            RuntimeOrigin::signed(second),
            match_id,
            trigger_card,
            5,
            None
        ));

        let board = NexusMatchBoards::<Test>::get(match_id).expect("board exists");
        assert!(board.rune_cells.is_empty());
        let triggered = board.cells[5].card.as_ref().expect("trigger card placed");
        assert_eq!(triggered.ranks[3], RankValue::Number(6));
        assert_eq!(
            board.cells[4].card.as_ref().unwrap().controller,
            second,
            "Rune bonus must apply before directional capture"
        );

        assert_event_found(
            |event| {
                matches!(
                    event,
                    RuntimeEvent::EterraSlots(Event::RuneTriggered {
                        match_id: seen_match_id,
                        card_id,
                        well_cell,
                        element,
                        effect,
                        ..
                    }) if *seen_match_id == match_id
                        && *card_id == trigger_card
                        && *well_cell == 5
                        && *element == Element::Fire
                        && *effect == 1
                )
            },
            "RuneTriggered",
        );
    });
}

#[test]
fn nexus_match_ends_after_both_five_card_hands_are_played() {
    new_test_ext().execute_with(|| {
        let first_base = 700;
        let second_base = 800;
        seed_nexus_team(2, first_base);
        seed_nexus_team(3, second_base);
        save_seeded_team(2, 1, (first_base..first_base + 5).collect());
        save_seeded_team(3, 1, (second_base..second_base + 5).collect());
        let (match_id, first, second) = start_seeded_match(2, 3, 0, MatchMode::Quick);

        let cells = [0u8, 15, 1, 14, 2, 13, 3, 12, 4, 11];
        for turn in 0..10u32 {
            let player = if turn % 2 == 0 { first } else { second };
            let offset = turn / 2;
            let card_id = card_for(player, 2, first_base, second_base, offset);
            assert_ok!(EterraSlots::play_nexus_match_card(
                RuntimeOrigin::signed(player),
                match_id,
                card_id,
                cells[turn as usize],
                None
            ));
        }

        let state = NexusMatches::<Test>::get(match_id).expect("match exists");
        assert_eq!(state.status, MatchStatus::Complete);
        assert_eq!(state.turn_index, 10);
        assert_eq!(state.winner, None);
        assert_event_found(
            |event| {
                matches!(
                    event,
                    RuntimeEvent::EterraSlots(Event::MatchEnded {
                        match_id: seen_match_id,
                        winner,
                        score,
                        duration,
                        reward_status
                    }) if *seen_match_id == match_id
                        && winner.is_none()
                        && *score == [5, 5]
                        && *duration == 10
                        && !*reward_status
                )
            },
            "MatchEnded",
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
