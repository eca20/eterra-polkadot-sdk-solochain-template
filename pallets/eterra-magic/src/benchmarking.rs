use super::*;
use frame_benchmarking::{account, benchmarks};
use frame_support::traits::Currency;
use frame_system::RawOrigin;

fn charge_definition() -> SpellChargeDefinition {
    SpellChargeDefinition {
        definition_id: 1,
        element: Element::Fire,
        competitive_load: 1,
        max_per_session: 4,
        effect_hash: [1; 32],
        transferable: false,
    }
}

fn prism_definition() -> PrismSpellDefinition {
    PrismSpellDefinition {
        definition_id: 1,
        element: Element::Fire,
        competitive_load: 2,
        max_level: 10,
        deterministic_quest_available: true,
        effect_hash: [2; 32],
        transferable: false,
    }
}

fn crafting_recipe<T: Config>() -> ChargeCraftingRecipe<BalanceOf<T>> {
    ChargeCraftingRecipe {
        definition_id: 1,
        formula_hash: [7; 32],
        recipe_hash: [8; 32],
        essence_per_charge: 30,
        eon_coin_fee_per_charge: T::Currency::minimum_balance(),
        max_batch: 4,
    }
}

benchmarks! {
    publish_spell_charge_definition {
        let definition = charge_definition();
    }: _(RawOrigin::Root, definition)
    verify {
        assert!(SpellChargeDefinitions::<T>::contains_key(1));
    }

    publish_prism_definition {
        let definition = prism_definition();
    }: _(RawOrigin::Root, definition)
    verify {
        assert!(PrismSpellDefinitions::<T>::contains_key(1));
    }

    grant_training_essence {
        let owner: T::AccountId = account("owner", 0, 0);
    }: _(RawOrigin::Root, owner.clone(), Element::Fire, 100, [3; 32])
    verify {
        assert_eq!(EssenceBalances::<T>::get(&owner, (EconomicRealm::Training, Element::Fire)), 100);
    }

    grant_training_spell_charges {
        let owner: T::AccountId = account("owner", 0, 0);
        SpellChargeDefinitions::<T>::insert(1, charge_definition());
    }: _(RawOrigin::Root, owner.clone(), 1, 4, [4; 32])
    verify {
        assert_eq!(SpellChargeBalances::<T>::get(&owner, (EconomicRealm::Training, 1)), 4);
    }

    create_training_prism {
        let owner: T::AccountId = account("owner", 0, 0);
        PrismSpellDefinitions::<T>::insert(1, prism_definition());
    }: _(RawOrigin::Root, owner.clone(), 1, [5; 32])
    verify {
        assert_eq!(PrismSpells::<T>::get(1).expect("prism exists").owner, owner);
    }

    emergency_unlock_prism {
        let owner: T::AccountId = account("owner", 0, 0);
        PrismSpellDefinitions::<T>::insert(1, prism_definition());
        Pallet::<T>::create_training_prism(
            RawOrigin::Root.into(),
            owner,
            1,
            [6; 32],
        )
        .expect("prism created");
        PrismSpells::<T>::mutate(1, |maybe| {
            maybe.as_mut().expect("prism exists").lock = Some(AssetLock {
                session_id: 7,
                role: eterra_nexus_primitives::AssetRole::PrismSpell,
                revision_at_lock: 1,
                expires_at: frame_system::Pallet::<T>::block_number(),
            });
        });
    }: _(RawOrigin::Root, 1)
    verify {
        assert!(PrismSpells::<T>::get(1).expect("prism exists").lock.is_none());
    }

    publish_charge_crafting_recipe {
        SpellChargeDefinitions::<T>::insert(1, charge_definition());
        let recipe = crafting_recipe::<T>();
    }: _(RawOrigin::Root, recipe)
    verify {
        assert!(ChargeCraftingRecipes::<T>::contains_key(1));
    }

    craft_spell_charges {
        let owner: T::AccountId = account("owner", 0, 0);
        SpellChargeDefinitions::<T>::insert(1, charge_definition());
        let recipe = crafting_recipe::<T>();
        ChargeCraftingRecipes::<T>::insert(1, recipe);
        EssenceBalances::<T>::insert(
            &owner,
            (EconomicRealm::Training, Element::Fire),
            100u128,
        );
        let _ = T::Currency::deposit_creating(&owner, T::Currency::minimum_balance());
        let _ = T::Currency::deposit_creating(&owner, T::Currency::minimum_balance());
    }: _(
        RawOrigin::Signed(owner.clone()),
        EconomicRealm::Training,
        1,
        1,
        recipe.formula_hash,
        recipe.recipe_hash,
        [9; 32]
    )
    verify {
        assert_eq!(
            EssenceBalances::<T>::get(
                &owner,
                (EconomicRealm::Training, Element::Fire),
            ),
            70
        );
        assert_eq!(
            SpellChargeBalances::<T>::get(&owner, (EconomicRealm::Training, 1)),
            1
        );
        assert!(ProcessedChargeCrafts::<T>::contains_key(&owner, [9; 32]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_benchmarking::impl_benchmark_test_suite;

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
