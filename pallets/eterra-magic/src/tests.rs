use crate::{
    mock::*, ChargeAmount, ChargeCraftReceipt, ChargeCraftingRecipe, EssenceBalances,
    MagicLoadoutLimits, MagicManager, PrismSpellDefinition, PrismSpells, ProcessedChargeCrafts,
    ReservedSpellChargeBalances, SpellChargeBalances, SpellChargeDefinition,
};
use codec::Encode;
use eterra_nexus_primitives::{AssetLock, AssetRole, EconomicRealm, Element};
use frame_support::{assert_noop, assert_ok};

fn configure() {
    assert_ok!(Magic::publish_spell_charge_definition(
        RuntimeOrigin::root(),
        SpellChargeDefinition {
            definition_id: 1,
            element: Element::Fire,
            competitive_load: 2,
            max_per_session: 3,
            effect_hash: [1; 32],
            transferable: false,
        }
    ));
    assert_ok!(Magic::publish_prism_definition(
        RuntimeOrigin::root(),
        PrismSpellDefinition {
            definition_id: 10,
            element: Element::Water,
            competitive_load: 4,
            max_level: 5,
            deterministic_quest_available: true,
            effect_hash: [2; 32],
            transferable: false,
        }
    ));
}

fn configure_recipe() {
    assert_ok!(Magic::publish_charge_crafting_recipe(
        RuntimeOrigin::root(),
        ChargeCraftingRecipe {
            definition_id: 1,
            formula_hash: [11; 32],
            recipe_hash: [12; 32],
            essence_per_charge: 30,
            eon_coin_fee_per_charge: 75,
            max_batch: 3,
        }
    ));
}

#[test]
fn reward_policy_definition_validation_is_immutable_and_fail_closed() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            <Magic as MagicManager<u64, u64>>::validate_reward_definitions(Some(1), Some(10)),
            crate::Error::<Test>::DefinitionMissing
        );
        configure();
        assert_ok!(
            <Magic as MagicManager<u64, u64>>::validate_reward_definitions(Some(1), Some(10))
        );
        assert_ok!(<Magic as MagicManager<u64, u64>>::validate_reward_definitions(None, None));
    });
}

#[test]
fn charge_reservation_burns_used_and_releases_unused() {
    new_test_ext().execute_with(|| {
        configure();
        assert_ok!(Magic::grant_training_spell_charges(
            RuntimeOrigin::root(),
            1,
            1,
            3,
            [3; 32]
        ));
        assert_ok!(<Magic as MagicManager<u64, u64>>::reserve_charges(
            7,
            &1,
            EconomicRealm::Training,
            &[(1, 3)]
        ));
        assert_eq!(
            ReservedSpellChargeBalances::<Test>::get(1, (EconomicRealm::Training, 1)),
            3
        );
        assert_ok!(<Magic as MagicManager<u64, u64>>::settle_charges(
            7,
            &[(1, 1)]
        ));
        assert_eq!(
            SpellChargeBalances::<Test>::get(1, (EconomicRealm::Training, 1)),
            2
        );
        assert_eq!(
            ReservedSpellChargeBalances::<Test>::get(1, (EconomicRealm::Training, 1)),
            0
        );
    });
}

#[test]
fn prism_xp_requires_verified_session_lock_and_is_idempotent() {
    new_test_ext().execute_with(|| {
        configure();
        assert_ok!(Magic::create_training_prism(
            RuntimeOrigin::root(),
            1,
            10,
            [4; 32]
        ));
        let spell_id = 1;
        assert_noop!(
            <Magic as MagicManager<u64, u64>>::grant_prism_xp(&1, spell_id, 2_000, [5; 32]),
            crate::Error::<Test>::PrismNotLocked
        );
        let revision = PrismSpells::<Test>::get(spell_id).unwrap().revision;
        assert_ok!(<Magic as MagicManager<u64, u64>>::lock_prism(
            &1,
            spell_id,
            AssetLock {
                session_id: 9,
                role: AssetRole::PrismSpell,
                revision_at_lock: revision,
                expires_at: 100,
            }
        ));
        assert_ok!(<Magic as MagicManager<u64, u64>>::grant_prism_xp(
            &1, spell_id, 2_000, [5; 32]
        ));
        assert_noop!(
            <Magic as MagicManager<u64, u64>>::grant_prism_xp(&1, spell_id, 2_000, [5; 32]),
            crate::Error::<Test>::ResultAlreadyProcessed
        );
    });
}

#[test]
fn training_and_production_balances_never_cross_reserve() {
    new_test_ext().execute_with(|| {
        configure();
        assert_ok!(Magic::grant_training_spell_charges(
            RuntimeOrigin::root(),
            1,
            1,
            2,
            [8; 32]
        ));
        assert_noop!(
            <Magic as MagicManager<u64, u64>>::reserve_charges(
                1,
                &1,
                EconomicRealm::Production,
                &[(1, 1)]
            ),
            crate::Error::<Test>::InsufficientSpellCharges
        );
    });
}

#[test]
fn session_loadout_validation_binds_realm_revision_ownership_and_magic_load() {
    new_test_ext().execute_with(|| {
        configure();
        assert_ok!(Magic::grant_training_spell_charges(
            RuntimeOrigin::root(),
            1,
            1,
            2,
            [9; 32],
        ));
        assert_ok!(Magic::create_training_prism(
            RuntimeOrigin::root(),
            1,
            10,
            [10; 32],
        ));
        let revision = PrismSpells::<Test>::get(1).unwrap().revision;
        assert_ok!(<Magic as MagicManager<u64, u64>>::validate_session_loadout(
            &1,
            EconomicRealm::Training,
            MagicLoadoutLimits {
                max_magic_load: 8,
                max_prisms: 1,
                max_charge_definitions: 1,
                max_total_charges: 2,
            },
            &[(1, revision)],
            &[(1, 2)],
        ));
        assert_noop!(
            <Magic as MagicManager<u64, u64>>::validate_session_loadout(
                &1,
                EconomicRealm::Training,
                MagicLoadoutLimits {
                    max_magic_load: 7,
                    max_prisms: 1,
                    max_charge_definitions: 1,
                    max_total_charges: 2,
                },
                &[(1, revision)],
                &[(1, 2)],
            ),
            crate::Error::<Test>::MagicLoadoutViolation
        );
        assert_noop!(
            <Magic as MagicManager<u64, u64>>::validate_session_loadout(
                &1,
                EconomicRealm::Training,
                MagicLoadoutLimits {
                    max_magic_load: 8,
                    max_prisms: 1,
                    max_charge_definitions: 1,
                    max_total_charges: 2,
                },
                &[(1, revision + 1)],
                &[(1, 2)],
            ),
            crate::Error::<Test>::MagicLoadoutViolation
        );
        assert_noop!(
            <Magic as MagicManager<u64, u64>>::validate_session_loadout(
                &1,
                EconomicRealm::Production,
                MagicLoadoutLimits {
                    max_magic_load: 8,
                    max_prisms: 1,
                    max_charge_definitions: 1,
                    max_total_charges: 2,
                },
                &[(1, revision)],
                &[(1, 2)],
            ),
            crate::Error::<Test>::InsufficientSpellCharges
        );
    });
}

#[test]
fn deterministic_charge_crafting_is_conservative_and_idempotent() {
    new_test_ext().execute_with(|| {
        configure();
        configure_recipe();
        assert_ok!(Magic::grant_training_essence(
            RuntimeOrigin::root(),
            1,
            Element::Fire,
            100,
            [13; 32],
        ));
        let owner_before = Balances::free_balance(1);
        let sink_before = Balances::free_balance(99);
        let issuance_before = Balances::total_issuance();
        assert_ok!(Magic::craft_spell_charges(
            RuntimeOrigin::signed(1),
            EconomicRealm::Training,
            1,
            2,
            [11; 32],
            [12; 32],
            [14; 32],
        ));
        assert_eq!(
            EssenceBalances::<Test>::get(1, (EconomicRealm::Training, Element::Fire)),
            40
        );
        assert_eq!(
            SpellChargeBalances::<Test>::get(1, (EconomicRealm::Training, 1)),
            2
        );
        assert_eq!(Balances::free_balance(1), owner_before - 150);
        assert_eq!(Balances::free_balance(99), sink_before + 150);
        assert_eq!(Balances::total_issuance(), issuance_before);
        System::assert_has_event(RuntimeEvent::Magic(crate::Event::EssenceConsumed {
            owner: 1,
            economic_realm: EconomicRealm::Training,
            element: Element::Fire,
            amount: 60,
        }));
        System::assert_has_event(RuntimeEvent::Magic(crate::Event::SpellChargesCrafted {
            owner: 1,
            economic_realm: EconomicRealm::Training,
            definition_id: 1,
            amount: 2,
            request_id: [14; 32],
            formula_hash: [11; 32],
            recipe_hash: [12; 32],
            essence_consumed: 60,
            eon_coin_fee: 150,
        }));
        let expected_commitment = sp_io::hashing::blake2_256(
            &(
                b"ETERRA_MAGIC_CHARGE_CRAFT_V1".as_slice(),
                &1u64,
                EconomicRealm::Training,
                1u32,
                2u32,
                [11u8; 32],
                [12u8; 32],
                [14u8; 32],
            )
                .encode(),
        );
        assert_eq!(
            ProcessedChargeCrafts::<Test>::get(1, [14; 32]),
            Some(ChargeCraftReceipt {
                request_id: [14; 32],
                commitment: expected_commitment,
                economic_realm: EconomicRealm::Training,
                definition_id: 1,
                amount: 2,
                formula_hash: [11; 32],
                recipe_hash: [12; 32],
                essence_consumed: 60,
                eon_coin_fee: 150,
            })
        );

        let owner_after = Balances::free_balance(1);
        let sink_after = Balances::free_balance(99);
        let events_after = System::events().len();
        assert_ok!(Magic::craft_spell_charges(
            RuntimeOrigin::signed(1),
            EconomicRealm::Training,
            1,
            2,
            [11; 32],
            [12; 32],
            [14; 32],
        ));
        assert_eq!(Balances::free_balance(1), owner_after);
        assert_eq!(Balances::free_balance(99), sink_after);
        assert_eq!(Balances::total_issuance(), issuance_before);
        assert_eq!(System::events().len(), events_after);
        assert_eq!(
            SpellChargeBalances::<Test>::get(1, (EconomicRealm::Training, 1)),
            2
        );
        assert_noop!(
            Magic::craft_spell_charges(
                RuntimeOrigin::signed(1),
                EconomicRealm::Training,
                1,
                1,
                [11; 32],
                [12; 32],
                [14; 32],
            ),
            crate::Error::<Test>::CraftRequestConflict
        );
    });
}

#[test]
fn alpha_access_gates_player_charge_crafting_before_any_spend() {
    new_test_ext().execute_with(|| {
        configure();
        configure_recipe();
        assert_ok!(Magic::grant_training_essence(
            RuntimeOrigin::root(),
            1,
            Element::Fire,
            100,
            [13; 32],
        ));
        let owner_before = Balances::free_balance(1);
        let sink_before = Balances::free_balance(99);
        set_access_allowed(false);
        assert_noop!(
            Magic::craft_spell_charges(
                RuntimeOrigin::signed(1),
                EconomicRealm::Training,
                1,
                2,
                [11; 32],
                [12; 32],
                [14; 32],
            ),
            sp_runtime::DispatchError::Other("not whitelisted")
        );
        assert_eq!(
            EssenceBalances::<Test>::get(1, (EconomicRealm::Training, Element::Fire)),
            100
        );
        assert_eq!(
            SpellChargeBalances::<Test>::get(1, (EconomicRealm::Training, 1)),
            0
        );
        assert_eq!(Balances::free_balance(1), owner_before);
        assert_eq!(Balances::free_balance(99), sink_before);
        assert!(!ProcessedChargeCrafts::<Test>::contains_key(1, [14; 32]));
    });
}

#[test]
fn production_crafting_is_compiled_off_until_explicitly_enabled() {
    new_test_ext().execute_with(|| {
        configure();
        configure_recipe();
        assert_ok!(<Magic as MagicManager<u64, u64>>::grant_essence(
            &1,
            EconomicRealm::Production,
            Element::Fire,
            60,
            [15; 32],
        ));
        assert_noop!(
            Magic::craft_spell_charges(
                RuntimeOrigin::signed(1),
                EconomicRealm::Production,
                1,
                1,
                [11; 32],
                [12; 32],
                [16; 32],
            ),
            crate::Error::<Test>::ProductionCraftingDisabled
        );
        assert_eq!(
            EssenceBalances::<Test>::get(1, (EconomicRealm::Production, Element::Fire)),
            60
        );
        assert_eq!(
            SpellChargeBalances::<Test>::get(1, (EconomicRealm::Production, 1)),
            0
        );

        set_production_crafting_enabled(true);
        assert_ok!(Magic::craft_spell_charges(
            RuntimeOrigin::signed(1),
            EconomicRealm::Production,
            1,
            1,
            [11; 32],
            [12; 32],
            [16; 32],
        ));
        assert_eq!(
            SpellChargeBalances::<Test>::get(1, (EconomicRealm::Production, 1)),
            1
        );
    });
}

#[test]
fn crafting_rejects_stale_formulas_limits_and_insufficient_inputs_atomically() {
    new_test_ext().execute_with(|| {
        configure();
        configure_recipe();
        assert_noop!(
            Magic::publish_charge_crafting_recipe(
                RuntimeOrigin::root(),
                ChargeCraftingRecipe {
                    definition_id: 1,
                    formula_hash: [21; 32],
                    recipe_hash: [22; 32],
                    essence_per_charge: 1,
                    eon_coin_fee_per_charge: 1,
                    max_batch: 1,
                }
            ),
            crate::Error::<Test>::CraftingRecipeAlreadyPublished
        );
        assert_eq!(
            crate::ChargeCraftingRecipes::<Test>::get(1),
            Some(ChargeCraftingRecipe {
                definition_id: 1,
                formula_hash: [11; 32],
                recipe_hash: [12; 32],
                essence_per_charge: 30,
                eon_coin_fee_per_charge: 75,
                max_batch: 3,
            })
        );
        let owner_before = Balances::free_balance(2);
        assert_noop!(
            Magic::craft_spell_charges(
                RuntimeOrigin::signed(2),
                EconomicRealm::Training,
                1,
                1,
                [99; 32],
                [12; 32],
                [17; 32],
            ),
            crate::Error::<Test>::CraftFormulaMismatch
        );
        assert_noop!(
            Magic::craft_spell_charges(
                RuntimeOrigin::signed(2),
                EconomicRealm::Training,
                1,
                1,
                [11; 32],
                [98; 32],
                [20; 32],
            ),
            crate::Error::<Test>::CraftRecipeMismatch
        );
        assert_noop!(
            Magic::craft_spell_charges(
                RuntimeOrigin::signed(2),
                EconomicRealm::Training,
                1,
                4,
                [11; 32],
                [12; 32],
                [18; 32],
            ),
            crate::Error::<Test>::CraftAmountInvalid
        );
        assert_noop!(
            Magic::craft_spell_charges(
                RuntimeOrigin::signed(2),
                EconomicRealm::Training,
                1,
                1,
                [11; 32],
                [12; 32],
                [19; 32],
            ),
            crate::Error::<Test>::InsufficientEssence
        );
        assert_eq!(Balances::free_balance(2), owner_before);
        assert_eq!(
            SpellChargeBalances::<Test>::get(2, (EconomicRealm::Training, 1)),
            0
        );
        assert!(!ProcessedChargeCrafts::<Test>::contains_key(2, [19; 32]));
    });
}

#[test]
fn crafting_is_transactional_across_keep_alive_fee_and_charge_mint_failures() {
    new_test_ext().execute_with(|| {
        configure();
        configure_recipe();

        assert_ok!(Magic::grant_training_essence(
            RuntimeOrigin::root(),
            2,
            Element::Fire,
            30,
            [21; 32],
        ));
        assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 2, 75));
        let sink_before = Balances::free_balance(99);
        let issuance_before = Balances::total_issuance();
        let events_before = System::events().len();
        assert_noop!(
            Magic::craft_spell_charges(
                RuntimeOrigin::signed(2),
                EconomicRealm::Training,
                1,
                1,
                [11; 32],
                [12; 32],
                [22; 32],
            ),
            crate::Error::<Test>::CraftFeePaymentFailed
        );
        assert_eq!(
            EssenceBalances::<Test>::get(2, (EconomicRealm::Training, Element::Fire)),
            30
        );
        assert_eq!(Balances::free_balance(2), 75);
        assert_eq!(Balances::free_balance(99), sink_before);
        assert_eq!(Balances::total_issuance(), issuance_before);
        assert_eq!(System::events().len(), events_before);
        assert_eq!(
            SpellChargeBalances::<Test>::get(2, (EconomicRealm::Training, 1)),
            0
        );
        assert!(!ProcessedChargeCrafts::<Test>::contains_key(2, [22; 32]));

        assert_ok!(Magic::grant_training_essence(
            RuntimeOrigin::root(),
            1,
            Element::Fire,
            30,
            [23; 32],
        ));
        SpellChargeBalances::<Test>::insert(1, (EconomicRealm::Training, 1), u32::MAX);
        let owner_before = Balances::free_balance(1);
        let sink_before = Balances::free_balance(99);
        let issuance_before = Balances::total_issuance();
        let events_before = System::events().len();
        assert_noop!(
            Magic::craft_spell_charges(
                RuntimeOrigin::signed(1),
                EconomicRealm::Training,
                1,
                1,
                [11; 32],
                [12; 32],
                [24; 32],
            ),
            crate::Error::<Test>::ArithmeticOverflow
        );
        assert_eq!(
            EssenceBalances::<Test>::get(1, (EconomicRealm::Training, Element::Fire)),
            30
        );
        assert_eq!(Balances::free_balance(1), owner_before);
        assert_eq!(Balances::free_balance(99), sink_before);
        assert_eq!(Balances::total_issuance(), issuance_before);
        assert_eq!(System::events().len(), events_before);
        assert_eq!(
            SpellChargeBalances::<Test>::get(1, (EconomicRealm::Training, 1)),
            u32::MAX
        );
        assert!(!ProcessedChargeCrafts::<Test>::contains_key(1, [24; 32]));
    });
}

#[test]
fn crafting_bounds_and_both_cost_multiplications_fail_before_state_changes() {
    new_test_ext().execute_with(|| {
        assert_ok!(Magic::publish_spell_charge_definition(
            RuntimeOrigin::root(),
            SpellChargeDefinition {
                definition_id: 2,
                element: Element::Earth,
                competitive_load: 2,
                max_per_session: 3,
                effect_hash: [31; 32],
                transferable: false,
            }
        ));
        assert_noop!(
            Magic::publish_charge_crafting_recipe(
                RuntimeOrigin::root(),
                ChargeCraftingRecipe {
                    definition_id: 2,
                    formula_hash: [32; 32],
                    recipe_hash: [33; 32],
                    essence_per_charge: 1,
                    eon_coin_fee_per_charge: 1,
                    max_batch: 11,
                }
            ),
            crate::Error::<Test>::InvalidDefinition
        );
        assert_ok!(Magic::publish_charge_crafting_recipe(
            RuntimeOrigin::root(),
            ChargeCraftingRecipe {
                definition_id: 2,
                formula_hash: [32; 32],
                recipe_hash: [33; 32],
                essence_per_charge: 1,
                eon_coin_fee_per_charge: u128::MAX,
                max_batch: 2,
            }
        ));
        assert_ok!(Magic::grant_training_essence(
            RuntimeOrigin::root(),
            1,
            Element::Earth,
            2,
            [34; 32],
        ));
        let owner_before = Balances::free_balance(1);
        let sink_before = Balances::free_balance(99);
        assert_noop!(
            Magic::craft_spell_charges(
                RuntimeOrigin::signed(1),
                EconomicRealm::Training,
                2,
                2,
                [32; 32],
                [33; 32],
                [35; 32],
            ),
            crate::Error::<Test>::ArithmeticOverflow
        );
        assert_eq!(
            EssenceBalances::<Test>::get(1, (EconomicRealm::Training, Element::Earth)),
            2
        );
        assert_eq!(Balances::free_balance(1), owner_before);
        assert_eq!(Balances::free_balance(99), sink_before);
        assert_eq!(
            SpellChargeBalances::<Test>::get(1, (EconomicRealm::Training, 2)),
            0
        );
        assert!(!ProcessedChargeCrafts::<Test>::contains_key(1, [35; 32]));

        assert_ok!(Magic::publish_spell_charge_definition(
            RuntimeOrigin::root(),
            SpellChargeDefinition {
                definition_id: 3,
                element: Element::Water,
                competitive_load: 2,
                max_per_session: 3,
                effect_hash: [36; 32],
                transferable: false,
            }
        ));
        assert_ok!(Magic::publish_charge_crafting_recipe(
            RuntimeOrigin::root(),
            ChargeCraftingRecipe {
                definition_id: 3,
                formula_hash: [37; 32],
                recipe_hash: [38; 32],
                essence_per_charge: u32::MAX,
                eon_coin_fee_per_charge: 1,
                max_batch: 2,
            }
        ));
        assert_ok!(Magic::grant_training_essence(
            RuntimeOrigin::root(),
            1,
            Element::Water,
            u32::MAX,
            [39; 32],
        ));
        assert_noop!(
            Magic::craft_spell_charges(
                RuntimeOrigin::signed(1),
                EconomicRealm::Training,
                3,
                2,
                [37; 32],
                [38; 32],
                [40; 32],
            ),
            crate::Error::<Test>::ArithmeticOverflow
        );
        assert_eq!(
            EssenceBalances::<Test>::get(1, (EconomicRealm::Training, Element::Water)),
            u128::from(u32::MAX)
        );
        assert_eq!(
            SpellChargeBalances::<Test>::get(1, (EconomicRealm::Training, 3)),
            0
        );
        assert!(!ProcessedChargeCrafts::<Test>::contains_key(1, [40; 32]));
    });
}

#[test]
fn crafting_additions_preserve_scale_discriminants() {
    let definition = SpellChargeDefinition {
        definition_id: 1,
        element: Element::Fire,
        competitive_load: 2,
        max_per_session: 3,
        effect_hash: [1; 32],
        transferable: false,
    };
    let recipe = ChargeCraftingRecipe {
        definition_id: 1,
        formula_hash: [11; 32],
        recipe_hash: [12; 32],
        essence_per_charge: 30,
        eon_coin_fee_per_charge: 75,
        max_batch: 3,
    };
    let calls = [
        crate::Call::<Test>::publish_spell_charge_definition { definition },
        crate::Call::<Test>::publish_prism_definition {
            definition: PrismSpellDefinition {
                definition_id: 10,
                element: Element::Water,
                competitive_load: 4,
                max_level: 5,
                deterministic_quest_available: true,
                effect_hash: [2; 32],
                transferable: false,
            },
        },
        crate::Call::<Test>::grant_training_essence {
            owner: 1,
            element: Element::Fire,
            amount: 1,
            result_id: [1; 32],
        },
        crate::Call::<Test>::grant_training_spell_charges {
            owner: 1,
            definition_id: 1,
            amount: 1,
            result_id: [2; 32],
        },
        crate::Call::<Test>::create_training_prism {
            owner: 1,
            definition_id: 10,
            traits_seed: [3; 32],
        },
        crate::Call::<Test>::emergency_unlock_prism { spell_id: 1 },
        crate::Call::<Test>::publish_charge_crafting_recipe { recipe },
        crate::Call::<Test>::craft_spell_charges {
            economic_realm: EconomicRealm::Training,
            definition_id: 1,
            amount: 1,
            formula_hash: [11; 32],
            recipe_hash: [12; 32],
            request_id: [4; 32],
        },
    ];
    for (expected, call) in calls.into_iter().enumerate() {
        assert_eq!(call.encode()[0], expected as u8);
    }

    let events: [crate::Event<Test>; 15] = [
        crate::Event::SpellChargeDefinitionPublished {
            definition_id: 1,
            effect_hash: [1; 32],
        },
        crate::Event::PrismSpellDefinitionPublished {
            definition_id: 1,
            effect_hash: [2; 32],
        },
        crate::Event::EssenceGranted {
            owner: 1,
            economic_realm: EconomicRealm::Training,
            element: Element::Fire,
            amount: 1,
            result_id: [3; 32],
        },
        crate::Event::EssenceConsumed {
            owner: 1,
            economic_realm: EconomicRealm::Training,
            element: Element::Fire,
            amount: 1,
        },
        crate::Event::SpellChargesGranted {
            owner: 1,
            economic_realm: EconomicRealm::Training,
            definition_id: 1,
            amount: 1,
            result_id: [4; 32],
        },
        crate::Event::SpellChargesReserved {
            owner: 1,
            session_id: 1,
            economic_realm: EconomicRealm::Training,
            charges: vec![ChargeAmount {
                definition_id: 1,
                amount: 1,
            }],
        },
        crate::Event::SpellChargesConsumed {
            session_id: 1,
            used: vec![ChargeAmount {
                definition_id: 1,
                amount: 1,
            }],
        },
        crate::Event::SpellChargesReleased { session_id: 1 },
        crate::Event::PrismSpellCreated {
            owner: 1,
            spell_id: 1,
            definition_id: 1,
            economic_realm: EconomicRealm::Training,
        },
        crate::Event::PrismSpellExperienceGranted {
            owner: 1,
            spell_id: 1,
            amount: 1,
            result_id: [5; 32],
        },
        crate::Event::PrismSpellLeveled {
            spell_id: 1,
            old_level: 1,
            new_level: 2,
        },
        crate::Event::PrismSpellLocked {
            spell_id: 1,
            session_id: 1,
        },
        crate::Event::PrismSpellUnlocked {
            spell_id: 1,
            session_id: 1,
            emergency: false,
        },
        crate::Event::ChargeCraftingRecipePublished {
            definition_id: 1,
            formula_hash: [11; 32],
            recipe_hash: [12; 32],
        },
        crate::Event::SpellChargesCrafted {
            owner: 1,
            economic_realm: EconomicRealm::Training,
            definition_id: 1,
            amount: 1,
            request_id: [6; 32],
            formula_hash: [11; 32],
            recipe_hash: [12; 32],
            essence_consumed: 30,
            eon_coin_fee: 75,
        },
    ];
    for (expected, event) in events.into_iter().enumerate() {
        assert_eq!(event.encode()[0], expected as u8);
    }

    let errors = [
        crate::Error::<Test>::DefinitionAlreadyPublished,
        crate::Error::<Test>::DefinitionMissing,
        crate::Error::<Test>::InvalidDefinition,
        crate::Error::<Test>::InsufficientEssence,
        crate::Error::<Test>::InsufficientSpellCharges,
        crate::Error::<Test>::ReservationAlreadyExists,
        crate::Error::<Test>::ReservationMissing,
        crate::Error::<Test>::ReservationOwnerMismatch,
        crate::Error::<Test>::ReservationRealmMismatch,
        crate::Error::<Test>::TooManyChargeDefinitions,
        crate::Error::<Test>::DuplicateChargeDefinition,
        crate::Error::<Test>::ChargeLimitExceeded,
        crate::Error::<Test>::MagicLoadoutViolation,
        crate::Error::<Test>::UsedChargeExceedsReservation,
        crate::Error::<Test>::PrismSpellIdExhausted,
        crate::Error::<Test>::PrismSpellMissing,
        crate::Error::<Test>::NotPrismOwner,
        crate::Error::<Test>::PrismLocked,
        crate::Error::<Test>::PrismNotLocked,
        crate::Error::<Test>::WrongSessionLock,
        crate::Error::<Test>::PrismXpGrantTooLarge,
        crate::Error::<Test>::ResultAlreadyProcessed,
        crate::Error::<Test>::ArithmeticOverflow,
        crate::Error::<Test>::TransferDisabled,
        crate::Error::<Test>::TrainingOnlyHelper,
        crate::Error::<Test>::CraftingRecipeAlreadyPublished,
        crate::Error::<Test>::CraftingRecipeMissing,
        crate::Error::<Test>::ProductionCraftingDisabled,
        crate::Error::<Test>::CraftRequestConflict,
        crate::Error::<Test>::CraftAmountInvalid,
        crate::Error::<Test>::CraftFormulaMismatch,
        crate::Error::<Test>::CraftRecipeMismatch,
        crate::Error::<Test>::CraftFeePaymentFailed,
    ];
    for (expected, error) in errors.into_iter().enumerate() {
        assert_eq!(error.encode(), vec![expected as u8]);
    }
}
