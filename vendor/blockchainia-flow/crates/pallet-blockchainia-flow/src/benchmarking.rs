#![cfg(feature = "runtime-benchmarks")]

use super::*;
use codec::Encode;
use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::{traits::Get, BoundedVec};
use frame_system::RawOrigin;
use sp_runtime::traits::Hash as HashT;
use sp_std::vec;
use sp_std::vec::Vec;

const BENCH_GAME: GameId = 1;
const BENCH_VERSION: VersionId = 1;
const BENCH_INSTANCE: InstanceId = 1;
const BENCH_ACTOR: ActorId = 1;
const BENCH_MACHINE: MachineId = 1;
const BENCH_ACTION: ActionId = 1;
const BENCH_TRANSITION: TransitionId = 1;
const BENCH_AUTHORITY: AuthorityId = 1;
const BENCH_STATE_START: StateId = 1;
const BENCH_STATE_DONE: StateId = 2;
const BENCH_EVENT: EventTypeId = 1;

fn bvec<Value, Limit: Get<u32>>(values: Vec<Value>) -> BoundedVec<Value, Limit> {
    BoundedVec::try_from(values)
        .unwrap_or_else(|_| panic!("benchmark vector must fit configured bound"))
}

fn metadata_hash<T: Config>() -> T::Hash {
    T::Hashing::hash(b"eterra-flow-benchmark")
}

fn metadata_uri<T: Config>() -> BoundedVec<u8, T::MaxUriBytes> {
    bvec(b"ipfs://eterra-flow-benchmark".to_vec())
}

fn payload<T: Config>() -> BoundedVec<u8, T::MaxActionPayloadBytes> {
    BoundedVec::default()
}

fn attested_payload<T: Config>() -> BoundedVec<u8, T::MaxAttestedPayloadBytes> {
    BoundedVec::default()
}

fn attested_effects<T: Config>() -> BoundedVec<AttestedEffect<T>, T::MaxAttestedEffectsPerEvent> {
    BoundedVec::default()
}

fn machine<T: Config>() -> MachineDefinition<T> {
    MachineDefinition::<T> {
        machine_id: BENCH_MACHINE,
        initial_state: BENCH_STATE_START,
        states: bvec(vec![BENCH_STATE_START, BENCH_STATE_DONE]),
    }
}

fn transition<T: Config>() -> Transition<T> {
    Transition::<T> {
        transition_id: BENCH_TRANSITION,
        machine_id: BENCH_MACHINE,
        action_id: BENCH_ACTION,
        from_state: Some(BENCH_STATE_START),
        to_state: Some(BENCH_STATE_DONE),
        priority: 0,
        conditions: BoundedVec::default(),
        economy_gate: EconomyGate::Free,
        effects: BoundedVec::default(),
    }
}

fn manifest<T: Config>() -> Manifest<T> {
    Manifest::<T> {
        manifest_version: 0,
        game_id: BENCH_GAME,
        version_id: BENCH_VERSION,
        machines: bvec(vec![machine::<T>()]),
        variables: BoundedVec::default(),
        actions: bvec(vec![BENCH_ACTION]),
        transitions: bvec(vec![transition::<T>()]),
        event_definitions: BoundedVec::default(),
    }
}

fn attested_manifest<T: Config>() -> Manifest<T> {
    let event = EventDefinition::<T> {
        event_type: BENCH_EVENT,
        policies: BoundedVec::default(),
    };
    Manifest::<T> {
        event_definitions: bvec(vec![event]),
        ..manifest::<T>()
    }
}

fn seed_game<T: Config>(owner: T::AccountId) {
    Games::<T>::insert(
        BENCH_GAME,
        GameRecord::<T> {
            owner,
            status: GameStatus::Active,
            active_version: None,
            metadata_hash: metadata_hash::<T>(),
            metadata_uri: metadata_uri::<T>(),
        },
    );
}

fn seed_finalized_version<T: Config>(owner: T::AccountId, manifest: Manifest<T>) {
    seed_game::<T>(owner);
    let manifest_hash = Pallet::<T>::canonical_manifest_hash(&manifest);
    Manifests::<T>::insert(BENCH_GAME, BENCH_VERSION, manifest);
    Versions::<T>::insert(
        BENCH_GAME,
        BENCH_VERSION,
        VersionRecord {
            status: VersionStatus::Finalized,
            manifest_hash: Some(manifest_hash),
            chunk_count: 1,
        },
    );
}

fn seed_active_version<T: Config>(owner: T::AccountId, manifest: Manifest<T>) {
    seed_finalized_version::<T>(owner, manifest);
    Versions::<T>::mutate(BENCH_GAME, BENCH_VERSION, |maybe_version| {
        if let Some(version) = maybe_version {
            version.status = VersionStatus::Active;
        }
    });
    Games::<T>::mutate(BENCH_GAME, |maybe_game| {
        if let Some(game) = maybe_game {
            game.active_version = Some(BENCH_VERSION);
        }
    });
}

fn seed_instance<T: Config>(owner: T::AccountId, manifest: Manifest<T>) {
    seed_active_version::<T>(owner.clone(), manifest);
    Instances::<T>::insert(
        BENCH_INSTANCE,
        InstanceRecord::<T> {
            game_id: BENCH_GAME,
            version_id: BENCH_VERSION,
            creator: owner,
            status: InstanceStatus::Active,
            config_hash: metadata_hash::<T>(),
        },
    );
}

fn upload_manifest_chunk<T: Config>(manifest: Manifest<T>) -> T::Hash {
    let encoded = manifest.encode();
    let chunk: BoundedVec<u8, T::MaxManifestChunkBytes> =
        BoundedVec::try_from(encoded.clone()).expect("benchmark manifest must fit in one chunk");
    VersionChunks::<T>::insert((BENCH_GAME, BENCH_VERSION, 0), chunk);
    Versions::<T>::insert(
        BENCH_GAME,
        BENCH_VERSION,
        VersionRecord {
            status: VersionStatus::Draft,
            manifest_hash: None,
            chunk_count: 1,
        },
    );
    T::Hashing::hash(&encoded)
}

benchmarks! {
    create_game {
        let owner: T::AccountId = whitelisted_caller();
    }: _(RawOrigin::Signed(owner.clone()), BENCH_GAME, metadata_hash::<T>(), metadata_uri::<T>())
    verify {
        assert!(Games::<T>::contains_key(BENCH_GAME));
    }

    upload_version_chunk {
        let owner: T::AccountId = whitelisted_caller();
        seed_game::<T>(owner.clone());
        let chunk: BoundedVec<u8, T::MaxManifestChunkBytes> =
            BoundedVec::try_from(manifest::<T>().encode()).expect("benchmark manifest chunk fits");
    }: _(RawOrigin::Signed(owner.clone()), BENCH_GAME, BENCH_VERSION, 0, chunk)
    verify {
        assert!(VersionChunks::<T>::contains_key((BENCH_GAME, BENCH_VERSION, 0)));
        let version = Versions::<T>::get(BENCH_GAME, BENCH_VERSION).expect("version exists");
        assert_eq!(version.chunk_count, 1);
    }

    finalize_version {
        let owner: T::AccountId = whitelisted_caller();
        seed_game::<T>(owner.clone());
        let manifest = manifest::<T>();
        let manifest_hash = upload_manifest_chunk::<T>(manifest);
    }: _(RawOrigin::Signed(owner.clone()), BENCH_GAME, BENCH_VERSION, manifest_hash)
    verify {
        let version = Versions::<T>::get(BENCH_GAME, BENCH_VERSION).expect("version exists");
        assert_eq!(version.status, VersionStatus::Finalized);
        assert!(Manifests::<T>::contains_key(BENCH_GAME, BENCH_VERSION));
    }

    activate_version {
        let owner: T::AccountId = whitelisted_caller();
        seed_finalized_version::<T>(owner.clone(), manifest::<T>());
    }: _(RawOrigin::Signed(owner.clone()), BENCH_GAME, BENCH_VERSION)
    verify {
        let game = Games::<T>::get(BENCH_GAME).expect("game exists");
        assert_eq!(game.active_version, Some(BENCH_VERSION));
    }

    create_instance {
        let owner: T::AccountId = whitelisted_caller();
        seed_active_version::<T>(owner.clone(), manifest::<T>());
    }: _(RawOrigin::Signed(owner.clone()), BENCH_GAME, BENCH_INSTANCE, Some(BENCH_VERSION), metadata_hash::<T>())
    verify {
        assert!(Instances::<T>::contains_key(BENCH_INSTANCE));
    }

    submit_action {
        let owner: T::AccountId = account("owner", 0, 0);
        let player: T::AccountId = whitelisted_caller();
        seed_instance::<T>(owner, manifest::<T>());
    }: _(RawOrigin::Signed(player.clone()), BENCH_GAME, BENCH_INSTANCE, BENCH_ACTOR, BENCH_MACHINE, BENCH_ACTION, 0, payload::<T>())
    verify {
        assert_eq!(ActorNonces::<T>::get((BENCH_GAME, BENCH_INSTANCE, BENCH_ACTOR)), 1);
        assert_eq!(
            MachineStates::<T>::get((BENCH_GAME, BENCH_INSTANCE, Scope::Actor(BENCH_ACTOR), BENCH_MACHINE)),
            Some(BENCH_STATE_DONE)
        );
    }

    submit_attested_event {
        let owner: T::AccountId = account("owner", 0, 0);
        let server: T::AccountId = whitelisted_caller();
        seed_instance::<T>(owner, attested_manifest::<T>());
        T::BenchmarkAuthorityProvider::authorize(&server, BENCH_GAME, BENCH_VERSION, BENCH_EVENT)
            .expect("benchmark authority seeding succeeds");
        let replay_hash = Some(metadata_hash::<T>());
    }: _(RawOrigin::Signed(server.clone()), BENCH_GAME, BENCH_INSTANCE, BENCH_EVENT, 0, attested_payload::<T>(), replay_hash, attested_effects::<T>())
    verify {
        assert_eq!(
            AttestedSequences::<T>::get((BENCH_GAME, BENCH_INSTANCE, BENCH_AUTHORITY, BENCH_EVENT)),
            1
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_benchmarking::impl_benchmark_test_suite;

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
