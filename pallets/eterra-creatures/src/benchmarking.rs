use super::*;
use eterra_nexus_primitives::{AssetLock, AssetRole};
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::{traits::Get, BoundedVec};
use frame_system::pallet_prelude::BlockNumberFor;
use frame_system::RawOrigin;
use sp_std::vec;
use sp_std::vec::Vec;

fn curve() -> CpLevelCurve {
    let mut ratios = [200u16; 50];
    for (index, ratio) in ratios.iter_mut().enumerate() {
        *ratio = 200u16.saturating_mul((index as u16).saturating_add(1));
    }
    CpLevelCurve {
        version: 1,
        ratios_bps: ratios,
        curve_hash: [1; 32],
    }
}

fn move_definition(move_id: MoveId) -> MoveDefinition {
    MoveDefinition {
        move_id,
        element: eterra_nexus_primitives::Element::Neutral,
        unlock_level: 1,
        essence_cost: 0,
        competitive_load: 1,
        tier: 1,
        tags: 0,
        cooldown_turns: 0,
        resource_cost: 1,
        rules_hash: [move_id as u8; 32],
    }
}

fn profile() -> EntityProfile {
    EntityProfile {
        profile_id: 1,
        subject_id: 1,
        subject_version: 1,
        rarity: CardRarity::Common,
        role: eterra_nexus_primitives::EntityRole::Hero,
        base_combat_stats: [10; 6],
        base_max_cp: 1_000,
        genetic_cp_span: 600,
        starter_moves: [1, 2],
        formula_version: 1,
        definition_hash: [4; 32],
    }
}

fn setup_entity<T: Config>(
    owner: T::AccountId,
    economic_realm: EconomicRealm,
    move_count: u32,
) -> EntityId {
    assert!(move_count >= 2);
    assert!(move_count <= T::MaxProfileMoves::get());

    CpLevelCurves::<T>::insert(1, curve());
    let mut learnset = Vec::with_capacity(move_count as usize);
    for move_id in 1..=move_count {
        MoveDefinitions::<T>::insert(move_id, move_definition(move_id));
        learnset.push(move_id);
    }
    let definition = profile();
    EntityProfiles::<T>::insert(definition.subject_id, definition.rarity, definition);
    EntityProfileKeys::<T>::insert(
        definition.profile_id,
        (definition.subject_id, definition.rarity),
    );
    let bounded_learnset: BoundedVec<MoveId, T::MaxProfileMoves> =
        learnset.try_into().expect("benchmark learnset fits");
    ProfileLearnsets::<T>::insert(definition.profile_id, bounded_learnset);

    let entity_id =
        <Pallet<T> as EntityManager<T::AccountId, BlockNumberFor<T>>>::reserve_entity_id()
            .expect("benchmark entity id is available");
    <Pallet<T> as EntityManager<T::AccountId, BlockNumberFor<T>>>::create_from_conversion(
        ConversionEntityInput {
            entity_id,
            owner,
            economic_realm,
            source_card_id: 1,
            source_rarity: CardRarity::Common,
            subject_id: definition.subject_id,
            subject_version: definition.subject_version,
            genome_seed: [7; 32],
            stasis_genome: false,
        },
    )
    .expect("benchmark entity is valid");
    entity_id
}

benchmarks! {
    publish_cp_level_curve {
        let definition = curve();
    }: _(RawOrigin::Root, definition)
    verify {
        assert!(CpLevelCurves::<T>::contains_key(1));
    }

    publish_move {
        let definition = move_definition(1);
    }: _(RawOrigin::Root, definition)
    verify {
        assert!(MoveDefinitions::<T>::contains_key(1));
    }

    publish_entity_profile {
        CpLevelCurves::<T>::insert(1, curve());
        MoveDefinitions::<T>::insert(1, move_definition(1));
        MoveDefinitions::<T>::insert(2, move_definition(2));
        let definition = profile();
        let learnset = vec![1, 2];
    }: _(RawOrigin::Root, definition, learnset)
    verify {
        assert!(EntityProfiles::<T>::contains_key(1, CardRarity::Common));
    }

    set_profile_activation {
        let definition = profile();
        EntityProfileKeys::<T>::insert(1, (definition.subject_id, definition.rarity));
    }: _(RawOrigin::Root, 1, true)
    verify {
        assert!(EntityProfileActivation::<T>::get(1));
    }

    publish_league_format {
        let format = EntityLeagueFormat {
            format_id: 1,
            version: 1,
            min_max_cp: 0,
            max_max_cp: 2_000,
            current_cp_cap: 1_500,
            max_move_load: 20,
            maximum_ultimate_tier: 1,
            normalized: false,
            rules_hash: [5; 32],
        };
    }: _(RawOrigin::Root, format)
    verify {
        assert!(LeagueFormats::<T>::contains_key(1, 1));
    }

    publish_league_move_policy {
        let move_count = T::MaxLeagueAllowedMoves::get();
        assert!(move_count > 0);
        let rules_hash = [5; 32];
        LeagueFormats::<T>::insert(
            1,
            1,
            EntityLeagueFormat {
                format_id: 1,
                version: 1,
                min_max_cp: 0,
                max_max_cp: 2_000,
                current_cp_cap: 1_500,
                max_move_load: 20,
                maximum_ultimate_tier: 4,
                normalized: false,
                rules_hash,
            },
        );
        let mut allowed_moves = Vec::with_capacity(move_count as usize);
        for move_id in 1..=move_count {
            MoveDefinitions::<T>::insert(move_id, move_definition(move_id));
            allowed_moves.push(move_id);
        }
        let per_tag_limits = [4; 32];
    }: _(
        RawOrigin::Root,
        1,
        1,
        allowed_moves,
        per_tag_limits,
        rules_hash
    )
    verify {
        assert_eq!(
            LeagueMovePolicies::<T>::get((1, 1))
                .expect("move policy exists")
                .allowed_moves
                .len() as u32,
            move_count
        );
    }

    learn_move {
        let owner: T::AccountId = whitelisted_caller();
        let target_move = core::cmp::min(
            T::MaxLearnedMoves::get(),
            T::MaxProfileMoves::get(),
        );
        assert!(target_move >= 3);
        let entity_id =
            setup_entity::<T>(owner.clone(), EconomicRealm::Production, target_move);
        Entities::<T>::mutate(entity_id, |maybe| {
            let entity = maybe.as_mut().expect("benchmark entity exists");
            for move_id in 3..target_move {
                entity
                    .learned_moves
                    .try_push(move_id)
                    .expect("one learned-move slot remains");
            }
        });
    }: _(RawOrigin::Signed(owner.clone()), entity_id, target_move)
    verify {
        let entity = Entities::<T>::get(entity_id).expect("benchmark entity exists");
        assert!(entity.learned_moves.contains(&target_move));
        assert_eq!(entity.learned_moves.len() as u32, target_move);
        assert_eq!(entity.revision, 2);
    }

    equip_moves {
        let owner: T::AccountId = whitelisted_caller();
        let move_count = core::cmp::min(
            T::MaxEquippedMoves::get(),
            core::cmp::min(T::MaxLearnedMoves::get(), T::MaxProfileMoves::get()),
        );
        assert!(move_count >= 2);
        let entity_id =
            setup_entity::<T>(owner.clone(), EconomicRealm::Production, move_count);
        Entities::<T>::mutate(entity_id, |maybe| {
            let entity = maybe.as_mut().expect("benchmark entity exists");
            for move_id in 3..=move_count {
                entity
                    .learned_moves
                    .try_push(move_id)
                    .expect("benchmark learned move fits");
            }
        });
        let moves: Vec<MoveId> = (1..=move_count).collect();
    }: _(RawOrigin::Signed(owner.clone()), entity_id, moves.clone())
    verify {
        let entity = Entities::<T>::get(entity_id).expect("benchmark entity exists");
        assert_eq!(entity.equipped_moves.as_slice(), moves.as_slice());
        assert_eq!(entity.revision, 2);
    }

    grant_training_experience {
        let owner: T::AccountId = whitelisted_caller();
        let entity_id =
            setup_entity::<T>(owner.clone(), EconomicRealm::Training, 2);
        let amount = T::MaxExperienceGrant::get();
        let result_id = [8; 32];
    }: _(RawOrigin::Root, owner.clone(), entity_id, amount, result_id)
    verify {
        let entity = Entities::<T>::get(entity_id).expect("benchmark entity exists");
        assert_eq!(entity.level_xp, amount);
        assert_eq!(entity.revision, 2);
        assert!(ProcessedEntityResults::<T>::contains_key(result_id));
    }

    emergency_unlock {
        let owner: T::AccountId = whitelisted_caller();
        let entity_id =
            setup_entity::<T>(owner, EconomicRealm::Production, 2);
        let session_id = 17;
        Entities::<T>::mutate(entity_id, |maybe| {
            let entity = maybe.as_mut().expect("benchmark entity exists");
            entity.lock = Some(AssetLock {
                session_id,
                role: AssetRole::Entity,
                revision_at_lock: entity.revision,
                expires_at: frame_system::Pallet::<T>::block_number(),
            });
        });
    }: _(RawOrigin::Root, entity_id)
    verify {
        let entity = Entities::<T>::get(entity_id).expect("benchmark entity exists");
        assert!(entity.lock.is_none());
        assert_eq!(entity.revision, 2);
    }

    reserve_future_reforge_schema {
        let config = eterra_nexus_primitives::EntityReforgeConfig {
            version: 1,
            pool_id: 1,
            pool_version: 1,
            pool_hash: [6; 32],
            enabled: false,
        };
    }: _(RawOrigin::Root, config)
    verify {
        assert!(ReservedReforgeConfigs::<T>::contains_key(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_benchmarking::impl_benchmark_test_suite;

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
