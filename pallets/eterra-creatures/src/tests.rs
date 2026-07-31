use crate::{mock::*, ConversionEntityInput, CpLevelCurve, Entities, EntityManager, Error};
use eterra_nexus_primitives::{
    CardRarity, EconomicRealm, Element, EntityLeagueFormat, EntityProfile, EntityRole,
    MoveDefinition,
};
use frame_support::{assert_noop, assert_ok};

fn curve() -> CpLevelCurve {
    let mut ratios = [0u16; 50];
    for (index, value) in ratios.iter_mut().enumerate() {
        *value = 500 + ((9_500u32 * index as u32) / 49) as u16;
    }
    ratios[49] = 10_000;
    CpLevelCurve {
        version: 1,
        ratios_bps: ratios,
        curve_hash: [1; 32],
    }
}

fn move_definition(move_id: u32, unlock_level: u8) -> MoveDefinition {
    MoveDefinition {
        move_id,
        element: Element::Fire,
        unlock_level,
        essence_cost: 5,
        competitive_load: 2,
        tier: 1,
        tags: 1,
        cooldown_turns: 1,
        resource_cost: 10,
        rules_hash: [move_id as u8; 32],
    }
}

fn configure() {
    assert_ok!(Creatures::publish_cp_level_curve(
        RuntimeOrigin::root(),
        curve()
    ));
    for id in 1..=3 {
        assert_ok!(Creatures::publish_move(
            RuntimeOrigin::root(),
            move_definition(id, if id == 3 { 2 } else { 1 })
        ));
    }
    let profile = EntityProfile {
        profile_id: 7,
        subject_id: 42,
        subject_version: 1,
        rarity: CardRarity::Epic,
        role: EntityRole::Creature,
        base_combat_stats: [10; 6],
        base_max_cp: 1_000,
        genetic_cp_span: 500,
        starter_moves: [1, 2],
        formula_version: 1,
        definition_hash: [9; 32],
    };
    assert_ok!(Creatures::publish_entity_profile(
        RuntimeOrigin::root(),
        profile,
        vec![1, 2, 3]
    ));
    assert_ok!(Creatures::set_profile_activation(
        RuntimeOrigin::root(),
        7,
        true
    ));
}

#[test]
fn conversion_reservation_is_one_way_and_genes_are_immutable() {
    new_test_ext().execute_with(|| {
        configure();
        let entity_id = <Creatures as EntityManager<u64, u64>>::reserve_entity_id().unwrap();
        assert_eq!(entity_id, 1);
        let input = ConversionEntityInput {
            entity_id,
            owner: 1,
            economic_realm: EconomicRealm::Production,
            source_card_id: 99,
            source_rarity: CardRarity::Epic,
            subject_id: 42,
            subject_version: 1,
            genome_seed: [31; 32],
            stasis_genome: false,
        };
        assert_ok!(<Creatures as EntityManager<u64, u64>>::create_from_conversion(input.clone()));
        assert_noop!(
            <Creatures as EntityManager<u64, u64>>::create_from_conversion(input),
            Error::<Test>::EntityAlreadyExists
        );
        let entity = Entities::<Test>::get(entity_id).unwrap();
        assert_eq!(entity.genes.vitality, 31);
        assert!(entity.current_cp <= entity.max_cp);
    });
}

#[test]
fn conversion_profile_activation_gates_commitment_not_creation() {
    new_test_ext().execute_with(|| {
        configure();
        assert_ok!(
            <Creatures as EntityManager<u64, u64>>::ensure_conversion_profile_active(
                42,
                1,
                CardRarity::Epic,
            )
        );
        assert_noop!(
            <Creatures as EntityManager<u64, u64>>::ensure_conversion_profile_active(
                42,
                2,
                CardRarity::Epic,
            ),
            Error::<Test>::ProfileInvalid
        );

        let entity_id = <Creatures as EntityManager<u64, u64>>::reserve_entity_id().unwrap();
        assert_ok!(Creatures::set_profile_activation(
            RuntimeOrigin::root(),
            7,
            false,
        ));
        assert_noop!(
            <Creatures as EntityManager<u64, u64>>::ensure_conversion_profile_active(
                42,
                1,
                CardRarity::Epic,
            ),
            Error::<Test>::ProfileInactive
        );

        assert_ok!(
            <Creatures as EntityManager<u64, u64>>::create_from_conversion(ConversionEntityInput {
                entity_id,
                owner: 1,
                economic_realm: EconomicRealm::Production,
                source_card_id: 100,
                source_rarity: CardRarity::Epic,
                subject_id: 42,
                subject_version: 1,
                genome_seed: [17; 32],
                stasis_genome: false,
            },)
        );
        assert!(Entities::<Test>::contains_key(entity_id));
    });
}

#[test]
fn stasis_timeout_uses_prereserved_id_and_fixed_genes() {
    new_test_ext().execute_with(|| {
        configure();
        let id = <Creatures as EntityManager<u64, u64>>::reserve_entity_id().unwrap();
        assert_ok!(
            <Creatures as EntityManager<u64, u64>>::create_from_conversion(ConversionEntityInput {
                entity_id: id,
                owner: 1,
                economic_realm: EconomicRealm::Training,
                source_card_id: 5,
                source_rarity: CardRarity::Epic,
                subject_id: 42,
                subject_version: 1,
                genome_seed: [0; 32],
                stasis_genome: true,
            })
        );
        let entity = Entities::<Test>::get(id).unwrap();
        assert!(entity.stasis_genome);
        assert_eq!(entity.genes.vitality, 15);
        assert_eq!(entity.genes.resistance, 15);
    });
}

#[test]
fn move_learning_and_league_budget_are_runtime_enforced() {
    new_test_ext().execute_with(|| {
        configure();
        let id = <Creatures as EntityManager<u64, u64>>::reserve_entity_id().unwrap();
        assert_ok!(
            <Creatures as EntityManager<u64, u64>>::create_from_conversion(ConversionEntityInput {
                entity_id: id,
                owner: 1,
                economic_realm: EconomicRealm::Production,
                source_card_id: 5,
                source_rarity: CardRarity::Epic,
                subject_id: 42,
                subject_version: 1,
                genome_seed: [10; 32],
                stasis_genome: false,
            })
        );
        seed_essence(1, EconomicRealm::Production, Element::Fire as u8, 10);
        assert_noop!(
            Creatures::learn_move(RuntimeOrigin::signed(1), id, 3),
            Error::<Test>::MoveLevelLocked
        );
        assert_ok!(<Creatures as EntityManager<u64, u64>>::grant_experience(
            &1, id, 1_000, [4; 32]
        ));
        assert_ok!(Creatures::learn_move(RuntimeOrigin::signed(1), id, 3));
        assert_ok!(Creatures::equip_moves(
            RuntimeOrigin::signed(1),
            id,
            vec![1, 2, 3]
        ));
        let entity = Entities::<Test>::get(id).unwrap();
        assert_ok!(Creatures::publish_league_format(
            RuntimeOrigin::root(),
            EntityLeagueFormat {
                format_id: 1,
                version: 1,
                min_max_cp: 0,
                max_max_cp: entity.max_cp,
                current_cp_cap: entity.max_cp,
                max_move_load: 6,
                maximum_ultimate_tier: 1,
                normalized: false,
                rules_hash: [7; 32],
            }
        ));
        assert_noop!(
            Creatures::validate_for_league(&1, id, 1, 1),
            Error::<Test>::LeagueMovePolicyMissing
        );
        assert_ok!(Creatures::publish_league_move_policy(
            RuntimeOrigin::root(),
            1,
            1,
            vec![1, 2, 3],
            [4; 32],
            [7; 32],
        ));
        assert_ok!(Creatures::validate_for_league(&1, id, 1, 1));

        for (format_id, rules_hash) in [(2, [8; 32]), (3, [9; 32])] {
            assert_ok!(Creatures::publish_league_format(
                RuntimeOrigin::root(),
                EntityLeagueFormat {
                    format_id,
                    version: 1,
                    min_max_cp: 0,
                    max_max_cp: entity.max_cp,
                    current_cp_cap: entity.max_cp,
                    max_move_load: 6,
                    maximum_ultimate_tier: 1,
                    normalized: false,
                    rules_hash,
                },
            ));
        }
        let mut tag_limits = [4; 32];
        tag_limits[0] = 2;
        assert_ok!(Creatures::publish_league_move_policy(
            RuntimeOrigin::root(),
            2,
            1,
            vec![1, 2, 3],
            tag_limits,
            [8; 32],
        ));
        assert_noop!(
            Creatures::validate_for_league(&1, id, 2, 1),
            Error::<Test>::LeagueMoveLoadViolation
        );
        assert_ok!(Creatures::publish_league_move_policy(
            RuntimeOrigin::root(),
            3,
            1,
            vec![1, 2],
            [4; 32],
            [9; 32],
        ));
        assert_noop!(
            Creatures::validate_for_league(&1, id, 3, 1),
            Error::<Test>::LeagueMoveLoadViolation
        );
    });
}

#[test]
fn alpha_access_gates_player_move_learning_and_equipping() {
    new_test_ext().execute_with(|| {
        configure();
        let id = <Creatures as EntityManager<u64, u64>>::reserve_entity_id().unwrap();
        assert_ok!(
            <Creatures as EntityManager<u64, u64>>::create_from_conversion(ConversionEntityInput {
                entity_id: id,
                owner: 1,
                economic_realm: EconomicRealm::Production,
                source_card_id: 5,
                source_rarity: CardRarity::Epic,
                subject_id: 42,
                subject_version: 1,
                genome_seed: [10; 32],
                stasis_genome: false,
            })
        );
        assert_ok!(<Creatures as EntityManager<u64, u64>>::grant_experience(
            &1, id, 1_000, [4; 32]
        ));
        seed_essence(1, EconomicRealm::Production, Element::Fire as u8, 10);
        set_access_allowed(false);
        assert_noop!(
            Creatures::learn_move(RuntimeOrigin::signed(1), id, 3),
            sp_runtime::DispatchError::Other("not whitelisted")
        );
        assert_noop!(
            Creatures::equip_moves(RuntimeOrigin::signed(1), id, vec![1, 2]),
            sp_runtime::DispatchError::Other("not whitelisted")
        );
        let entity = Entities::<Test>::get(id).expect("entity remains");
        assert!(!entity.learned_moves.contains(&3));
        assert_eq!(entity.equipped_moves.as_slice(), &[1, 2]);
    });
}

#[test]
fn future_reforge_can_only_be_reserved_disabled() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Creatures::reserve_future_reforge_schema(
                RuntimeOrigin::root(),
                eterra_nexus_primitives::EntityReforgeConfig {
                    version: 1,
                    pool_id: 1,
                    pool_version: 1,
                    pool_hash: [1; 32],
                    enabled: true,
                }
            ),
            Error::<Test>::ReservedReforgeMustRemainDisabled
        );
    });
}
