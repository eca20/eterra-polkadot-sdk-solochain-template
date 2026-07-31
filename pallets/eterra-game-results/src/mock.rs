use crate as pallet_eterra_game_results;
use codec::Encode;
use eterra_nexus_primitives::{
    AssetLock, EconomicRealm, Hash32, PrismSpellId, DRAND_QUICKNET_CHAIN_HASH,
};
use frame_support::{
    construct_runtime, parameter_types,
    traits::{ConstU32, Everything},
};
use sp_core::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage, DispatchError,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};

type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
    pub struct Test {
        System: frame_system,
        GameResults: pallet_eterra_game_results,
    }
);

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const GenesisHash: Hash32 = [9; 32];
    pub const PalletInstanceId: u8 = 38;
    pub const MaxSessionEntities: u32 = 3;
    pub const MaxSessionPrisms: u32 = 4;
    pub const MaxChargeDefinitions: u32 = 8;
    pub const MaxSignatureBytes: u32 = 64;
    pub const MaxActiveSessionsPerAccount: u32 = 4;
    pub const MaxActiveSessionsPerAuthority: u32 = 4;
    pub const MaxSessionAuthorizationReceiptsPerEpoch: u32 = 4;
    pub const MaxPendingDropsPerAccount: u32 = 2;
    pub const MaxSessionLifetime: u64 = 100;
    pub const ExpiryGrace: u64 = 2;
    pub const ResultEpochSize: u64 = 4;
    pub const MaxResultsPerEpoch: u32 = 4;
    pub const ResultDisputeWindow: u64 = 5;
    pub const RewardDayBlocks: u64 = 100;
}

pub struct MockGenesisHash;

impl crate::GenesisHashProvider for MockGenesisHash {
    fn genesis_hash() -> Hash32 {
        GenesisHash::get()
    }
}

#[cfg(feature = "runtime-benchmarks")]
pub struct MockBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl crate::BenchmarkHelper for MockBenchmarkHelper {
    fn authority_public_key() -> [u8; 32] {
        [1; 32]
    }

    fn sign_result(payload_hash: &Hash32) -> Vec<u8> {
        payload_hash.to_vec()
    }

    fn seed_finalized_randomness(request_id: Hash32, output: Hash32) {
        RANDOM_OUTPUTS.with(|outputs| {
            outputs.borrow_mut().insert(request_id, output);
        });
    }

    fn seed_timed_out_randomness(request_id: Hash32) {
        RANDOM_TIMEOUTS.with(|timeouts| {
            timeouts.borrow_mut().insert(request_id, true);
        });
    }
}

impl frame_system::Config for Test {
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
    type AccountData = ();
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
    type BlockHashCount = BlockHashCount;
}

pub struct MockSignatureVerifier;
impl crate::ServerSignatureVerifier for MockSignatureVerifier {
    fn verify(_public_key: &[u8; 32], payload_hash: &Hash32, signature: &[u8]) -> bool {
        signature == payload_hash
    }
}

pub struct MockAccessControl;

impl pallet_alpha_access::AccessControl<u64> for MockAccessControl {
    fn ensure_whitelisted(_: &u64) -> frame_support::dispatch::DispatchResult {
        if ACCESS_ALLOWED.with(Cell::get) {
            Ok(())
        } else {
            Err(DispatchError::Other("not whitelisted"))
        }
    }
}

thread_local! {
    pub static ACCESS_ALLOWED: Cell<bool> = const { Cell::new(true) };
    pub static ENTITY_LOCKS: RefCell<BTreeMap<u64, (u64, AssetLock<u64>)>> =
        const { RefCell::new(BTreeMap::new()) };
    pub static ENTITY_XP: RefCell<Vec<(u64, u64, u64, Hash32)>> =
        const { RefCell::new(Vec::new()) };
    pub static RESERVED_CHARGES: RefCell<BTreeMap<u64, Vec<(u32, u32)>>> =
        const { RefCell::new(BTreeMap::new()) };
    pub static PRISM_LOCKS: RefCell<BTreeMap<PrismSpellId, (u64, AssetLock<u64>)>> =
        const { RefCell::new(BTreeMap::new()) };
    pub static PRISM_REWARDS: RefCell<Vec<(u64, EconomicRealm, u32, Hash32, Hash32)>> =
        const { RefCell::new(Vec::new()) };
    pub static PLAYER_XP: RefCell<Vec<(u64, EconomicRealm, u128, Hash32)>> =
        const { RefCell::new(Vec::new()) };
    pub static RANDOM_OUTPUTS: RefCell<BTreeMap<Hash32, Hash32>> =
        const { RefCell::new(BTreeMap::new()) };
    pub static RANDOM_TIMEOUTS: RefCell<BTreeMap<Hash32, bool>> =
        const { RefCell::new(BTreeMap::new()) };
    pub static RANDOM_REQUEST_CONTEXTS: RefCell<BTreeMap<Hash32, (EconomicRealm, pallet_eterra_randomness::RandomnessMode)>> =
        const { RefCell::new(BTreeMap::new()) };
    pub static RANDOM_OUTPUT_PROVENANCE: RefCell<BTreeMap<Hash32, pallet_eterra_randomness::RandomnessMode>> =
        const { RefCell::new(BTreeMap::new()) };
    pub static RANDOM_REQUEST_AVAILABLE: Cell<bool> = const { Cell::new(true) };
    pub static PRODUCTION_RANDOMNESS_READY: Cell<bool> = const { Cell::new(true) };
    pub static RANDOMNESS_MODE: Cell<pallet_eterra_randomness::RandomnessMode> =
        const { Cell::new(pallet_eterra_randomness::RandomnessMode::DrandQuicknet) };
}

pub struct MockEntities;
impl pallet_eterra_creatures::EntityManager<u64, u64> for MockEntities {
    fn reserve_entity_id() -> Result<u64, DispatchError> {
        Ok(1)
    }

    fn ensure_conversion_profile_active(
        _subject_id: u32,
        _subject_version: u32,
        _rarity: eterra_nexus_primitives::CardRarity,
    ) -> frame_support::dispatch::DispatchResult {
        Ok(())
    }

    fn create_from_conversion(
        _input: pallet_eterra_creatures::ConversionEntityInput<u64>,
    ) -> frame_support::dispatch::DispatchResult {
        Ok(())
    }

    fn validate_session_entity(
        _owner: &u64,
        _economic_realm: EconomicRealm,
        _entity_id: u64,
        _revision: u32,
        format_id: u32,
        format_version: u32,
        allowed_roles_mask: u8,
    ) -> frame_support::dispatch::DispatchResult {
        if format_id == 0 || format_version == 0 || allowed_roles_mask == 0 {
            return Err(DispatchError::Other("invalid entity loadout policy"));
        }
        Ok(())
    }

    fn lock_entity(
        owner: &u64,
        entity_id: u64,
        lock: AssetLock<u64>,
    ) -> frame_support::dispatch::DispatchResult {
        ENTITY_LOCKS.with(|locks| {
            if locks.borrow().contains_key(&entity_id) {
                return Err(DispatchError::Other("locked"));
            }
            locks.borrow_mut().insert(entity_id, (*owner, lock));
            Ok(())
        })
    }

    fn unlock_entity(session_id: u64, entity_id: u64) -> frame_support::dispatch::DispatchResult {
        ENTITY_LOCKS.with(|locks| {
            let lock = locks
                .borrow_mut()
                .remove(&entity_id)
                .ok_or(DispatchError::Other("not locked"))?;
            if lock.1.session_id != session_id {
                return Err(DispatchError::Other("wrong session"));
            }
            Ok(())
        })
    }

    fn force_unlock_entity(
        session_id: u64,
        entity_id: u64,
    ) -> frame_support::dispatch::DispatchResult {
        ENTITY_LOCKS.with(|locks| {
            let mut locks = locks.borrow_mut();
            if let Some((_, lock)) = locks.get(&entity_id) {
                if lock.session_id != session_id {
                    return Err(DispatchError::Other("wrong session"));
                }
            }
            locks.remove(&entity_id);
            Ok(())
        })
    }

    fn grant_experience(
        owner: &u64,
        entity_id: u64,
        amount: u64,
        result_id: Hash32,
    ) -> frame_support::dispatch::DispatchResult {
        ENTITY_XP.with(|xp| {
            xp.borrow_mut().push((*owner, entity_id, amount, result_id));
        });
        Ok(())
    }
}

pub struct MockMagic;
impl pallet_eterra_magic::MagicManager<u64, u64> for MockMagic {
    fn validate_reward_definitions(
        charge_definition_id: Option<u32>,
        prism_definition_id: Option<u32>,
    ) -> frame_support::dispatch::DispatchResult {
        if charge_definition_id.is_some_and(|definition_id| definition_id != 10)
            || prism_definition_id.is_some_and(|definition_id| definition_id != 20)
        {
            return Err(DispatchError::Other("reward definition missing"));
        }
        Ok(())
    }

    fn validate_session_loadout(
        _owner: &u64,
        _realm: EconomicRealm,
        limits: pallet_eterra_magic::MagicLoadoutLimits,
        prisms: &[(PrismSpellId, u32)],
        charges: &[(u32, u32)],
    ) -> frame_support::dispatch::DispatchResult {
        if prisms.len() > usize::from(limits.max_prisms)
            || charges.len() > usize::from(limits.max_charge_definitions)
            || charges.iter().map(|(_, amount)| *amount).sum::<u32>()
                > u32::from(limits.max_total_charges)
            || (!prisms.is_empty() || !charges.is_empty()) && limits.max_magic_load == 0
        {
            return Err(DispatchError::Other("invalid magic loadout"));
        }
        Ok(())
    }

    fn reserve_charges(
        session_id: u64,
        _owner: &u64,
        _realm: EconomicRealm,
        charges: &[(u32, u32)],
    ) -> frame_support::dispatch::DispatchResult {
        RESERVED_CHARGES.with(|reserved| {
            reserved.borrow_mut().insert(session_id, charges.to_vec());
        });
        Ok(())
    }

    fn settle_charges(
        session_id: u64,
        _used: &[(u32, u32)],
    ) -> frame_support::dispatch::DispatchResult {
        RESERVED_CHARGES.with(|reserved| {
            reserved
                .borrow_mut()
                .remove(&session_id)
                .ok_or(DispatchError::Other("not reserved"))?;
            Ok(())
        })
    }

    fn release_charges(session_id: u64) -> frame_support::dispatch::DispatchResult {
        RESERVED_CHARGES.with(|reserved| {
            reserved
                .borrow_mut()
                .remove(&session_id)
                .ok_or(DispatchError::Other("not reserved"))?;
            Ok(())
        })
    }

    fn grant_essence(
        _owner: &u64,
        _realm: EconomicRealm,
        _element: eterra_nexus_primitives::Element,
        _amount: u32,
        _result_id: Hash32,
    ) -> frame_support::dispatch::DispatchResult {
        Ok(())
    }

    fn grant_spell_charges(
        _owner: &u64,
        _realm: EconomicRealm,
        _definition_id: u32,
        _amount: u32,
        _result_id: Hash32,
    ) -> frame_support::dispatch::DispatchResult {
        Ok(())
    }

    fn grant_prism_xp(
        _owner: &u64,
        _spell_id: PrismSpellId,
        _amount: u64,
        _result_id: Hash32,
    ) -> frame_support::dispatch::DispatchResult {
        Ok(())
    }

    fn create_prism_reward(
        owner: &u64,
        realm: EconomicRealm,
        definition_id: u32,
        traits_seed: Hash32,
        result_id: Hash32,
    ) -> frame_support::dispatch::DispatchResult {
        PRISM_REWARDS.with(|rewards| {
            rewards
                .borrow_mut()
                .push((*owner, realm, definition_id, traits_seed, result_id));
        });
        Ok(())
    }

    fn lock_prism(
        owner: &u64,
        spell_id: PrismSpellId,
        lock: AssetLock<u64>,
    ) -> frame_support::dispatch::DispatchResult {
        PRISM_LOCKS.with(|locks| {
            locks.borrow_mut().insert(spell_id, (*owner, lock));
        });
        Ok(())
    }

    fn unlock_prism(
        session_id: u64,
        spell_id: PrismSpellId,
    ) -> frame_support::dispatch::DispatchResult {
        PRISM_LOCKS.with(|locks| {
            let lock = locks
                .borrow_mut()
                .remove(&spell_id)
                .ok_or(DispatchError::Other("not locked"))?;
            if lock.1.session_id != session_id {
                return Err(DispatchError::Other("wrong session"));
            }
            Ok(())
        })
    }

    fn force_unlock_prism(
        session_id: u64,
        spell_id: PrismSpellId,
    ) -> frame_support::dispatch::DispatchResult {
        PRISM_LOCKS.with(|locks| {
            let mut locks = locks.borrow_mut();
            if let Some((_, lock)) = locks.get(&spell_id) {
                if lock.session_id != session_id {
                    return Err(DispatchError::Other("wrong session"));
                }
            }
            locks.remove(&spell_id);
            Ok(())
        })
    }
}

pub struct MockProgression;
impl pallet_eterra_gamer::V2PlayerProgressionManager<u64> for MockProgression {
    fn grant_settled_fps_xp(
        owner: &u64,
        realm: EconomicRealm,
        amount: u128,
        result_id: Hash32,
    ) -> frame_support::dispatch::DispatchResult {
        PLAYER_XP.with(|xp| xp.borrow_mut().push((*owner, realm, amount, result_id)));
        Ok(())
    }
}

pub struct MockRandomness;
impl pallet_eterra_randomness::VerifiableRandomness for MockRandomness {
    fn production_ready() -> bool {
        PRODUCTION_RANDOMNESS_READY.with(Cell::get)
            && RANDOMNESS_MODE.with(Cell::get)
                == pallet_eterra_randomness::RandomnessMode::DrandQuicknet
    }

    fn current_mode() -> pallet_eterra_randomness::RandomnessMode {
        RANDOMNESS_MODE.with(Cell::get)
    }

    fn request(
        domain: Hash32,
        commitment: Hash32,
        config: Hash32,
        min_epoch: u64,
    ) -> Result<Hash32, DispatchError> {
        if !RANDOM_REQUEST_AVAILABLE.with(Cell::get) {
            return Err(DispatchError::Other("randomness unavailable"));
        }
        Ok(sp_io::hashing::blake2_256(
            &(domain, commitment, config, min_epoch).encode(),
        ))
    }

    fn output(request_id: Hash32) -> Option<(u64, Hash32, Hash32)> {
        RANDOM_OUTPUTS.with(|outputs| {
            outputs
                .borrow()
                .get(&request_id)
                .copied()
                .map(|output| (1, output, [7; 32]))
        })
    }

    fn timed_out(request_id: Hash32) -> bool {
        RANDOM_TIMEOUTS
            .with(|timeouts| timeouts.borrow().get(&request_id).copied().unwrap_or(false))
    }

    fn request_for(
        economic_realm: EconomicRealm,
        expected_provenance: pallet_eterra_randomness::RandomnessMode,
        domain: Hash32,
        commitment: Hash32,
        config: Hash32,
        min_epoch: u64,
    ) -> Result<Hash32, DispatchError> {
        if !RANDOM_REQUEST_AVAILABLE.with(Cell::get) {
            return Err(DispatchError::Other("randomness unavailable"));
        }
        if RANDOMNESS_MODE.with(Cell::get) != expected_provenance {
            return Err(DispatchError::Other("randomness provenance changed"));
        }
        if economic_realm == EconomicRealm::Production
            && (expected_provenance != pallet_eterra_randomness::RandomnessMode::DrandQuicknet
                || !Self::production_ready())
        {
            return Err(DispatchError::Other(
                "production requires reviewed DrandQuicknet",
            ));
        }
        let request_id = sp_io::hashing::blake2_256(
            &(
                economic_realm,
                expected_provenance,
                domain,
                commitment,
                config,
                min_epoch,
            )
                .encode(),
        );
        RANDOM_REQUEST_CONTEXTS.with(|contexts| {
            contexts
                .borrow_mut()
                .insert(request_id, (economic_realm, expected_provenance));
        });
        Ok(request_id)
    }

    fn output_for(
        request_id: Hash32,
        expected_realm: EconomicRealm,
        expected_provenance: pallet_eterra_randomness::RandomnessMode,
    ) -> Option<pallet_eterra_randomness::RealmBoundRandomnessOutput> {
        if expected_realm == EconomicRealm::Production
            && expected_provenance != pallet_eterra_randomness::RandomnessMode::DrandQuicknet
        {
            return None;
        }
        let context =
            RANDOM_REQUEST_CONTEXTS.with(|contexts| contexts.borrow().get(&request_id).copied());
        if context.is_some_and(|bound| bound != (expected_realm, expected_provenance))
            || (expected_realm == EconomicRealm::Production && context.is_none())
        {
            return None;
        }
        let output = RANDOM_OUTPUTS.with(|outputs| outputs.borrow().get(&request_id).copied())?;
        let actual_provenance = RANDOM_OUTPUT_PROVENANCE
            .with(|modes| modes.borrow().get(&request_id).copied())
            .or_else(|| context.map(|(_, provenance)| provenance))
            .unwrap_or(expected_provenance);
        if actual_provenance != expected_provenance {
            return None;
        }
        Some(pallet_eterra_randomness::RealmBoundRandomnessOutput {
            epoch: 1,
            output,
            proof_hash: [7; 32],
            economic_realm: expected_realm,
            provenance: actual_provenance,
            provider_chain_hash: if actual_provenance
                == pallet_eterra_randomness::RandomnessMode::DrandQuicknet
            {
                DRAND_QUICKNET_CHAIN_HASH
            } else {
                [0; 32]
            },
        })
    }
}

impl crate::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type AdminOrigin = frame_system::EnsureRoot<u64>;
    type AccessControl = MockAccessControl;
    type SignatureVerifier = MockSignatureVerifier;
    type Entities = MockEntities;
    type Magic = MockMagic;
    type PlayerProgression = MockProgression;
    type Randomness = MockRandomness;
    type GenesisHashProvider = MockGenesisHash;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = MockBenchmarkHelper;
    type PalletInstanceId = PalletInstanceId;
    type MaxSessionEntities = MaxSessionEntities;
    type MaxSessionPrisms = MaxSessionPrisms;
    type MaxChargeDefinitions = MaxChargeDefinitions;
    type MaxSignatureBytes = MaxSignatureBytes;
    type MaxActiveSessionsPerAccount = MaxActiveSessionsPerAccount;
    type MaxActiveSessionsPerAuthority = MaxActiveSessionsPerAuthority;
    type MaxSessionAuthorizationReceiptsPerEpoch = MaxSessionAuthorizationReceiptsPerEpoch;
    type MaxPendingDropsPerAccount = MaxPendingDropsPerAccount;
    type MaxSessionLifetime = MaxSessionLifetime;
    type ExpiryGrace = ExpiryGrace;
    type ResultEpochSize = ResultEpochSize;
    type MaxResultsPerEpoch = MaxResultsPerEpoch;
    type ResultDisputeWindow = ResultDisputeWindow;
    type RewardDayBlocks = RewardDayBlocks;
    type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1);
        ENTITY_LOCKS.with(|value| value.borrow_mut().clear());
        ENTITY_XP.with(|value| value.borrow_mut().clear());
        RESERVED_CHARGES.with(|value| value.borrow_mut().clear());
        PRISM_LOCKS.with(|value| value.borrow_mut().clear());
        PRISM_REWARDS.with(|value| value.borrow_mut().clear());
        PLAYER_XP.with(|value| value.borrow_mut().clear());
        RANDOM_OUTPUTS.with(|value| value.borrow_mut().clear());
        RANDOM_TIMEOUTS.with(|value| value.borrow_mut().clear());
        RANDOM_REQUEST_CONTEXTS.with(|value| value.borrow_mut().clear());
        RANDOM_OUTPUT_PROVENANCE.with(|value| value.borrow_mut().clear());
        ACCESS_ALLOWED.with(|value| value.set(true));
        RANDOM_REQUEST_AVAILABLE.with(|value| value.set(true));
        PRODUCTION_RANDOMNESS_READY.with(|value| value.set(true));
        RANDOMNESS_MODE
            .with(|value| value.set(pallet_eterra_randomness::RandomnessMode::DrandQuicknet));
    });
    ext
}
