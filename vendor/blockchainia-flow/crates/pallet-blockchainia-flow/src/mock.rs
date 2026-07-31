use crate as pallet_blockchainia_flow;

use frame_support::{
    construct_runtime,
    traits::{ConstU32, ConstU64, Everything, Hooks},
};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage, StateVersion,
};

pub type AccountId = u64;
type Block = system::mocking::MockBlock<Test>;

pub struct MockAuthorityProvider;

impl pallet_blockchainia_flow::AuthorityProvider<AccountId> for MockAuthorityProvider {
    fn resolve_authority(
        _account: &AccountId,
        _game_id: pallet_blockchainia_flow::GameId,
        _version_id: Option<pallet_blockchainia_flow::VersionId>,
        _event_type: pallet_blockchainia_flow::EventTypeId,
    ) -> Option<pallet_blockchainia_flow::AuthorityId> {
        Some(1)
    }
}

pub struct MockBenchmarkAuthorityProvider;

impl pallet_blockchainia_flow::BenchmarkAuthorityProvider<AccountId>
    for MockBenchmarkAuthorityProvider
{
    fn authorize(
        _account: &AccountId,
        _game_id: pallet_blockchainia_flow::GameId,
        _version_id: pallet_blockchainia_flow::VersionId,
        _event_type: pallet_blockchainia_flow::EventTypeId,
    ) -> frame_support::dispatch::DispatchResult {
        Ok(())
    }
}

construct_runtime!(
    pub enum Test {
        System: system,
        BlockchainiaFlow: pallet_blockchainia_flow,
    }
);

impl system::Config for Test {
    type BaseCallFilter = Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type Block = Block;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
    type BlockHashCount = ConstU64<250>;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = ();
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    type MaxConsumers = ConstU32<16>;
    type Nonce = u64;
    type SingleBlockMigrations = ();
    type MultiBlockMigrator = ();
    type PreInherents = ();
    type PostInherents = ();
    type PostTransactions = ();
    type RuntimeTask = ();
}

impl pallet_blockchainia_flow::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
    type AuthorityProvider = MockAuthorityProvider;
    type EconomyProvider = ();
    type ProfileProvider = ();
    type BenchmarkAuthorityProvider = MockBenchmarkAuthorityProvider;
    type MaxUriBytes = ConstU32<64>;
    type MaxManifestChunkBytes = ConstU32<4096>;
    type MaxManifestChunks = ConstU32<4>;
    type MaxManifestBytes = ConstU32<16_384>;
    type MaxActionPayloadBytes = ConstU32<128>;
    type MaxAttestedPayloadBytes = ConstU32<256>;
    type MaxMachinesPerManifest = ConstU32<4>;
    type MaxStatesPerMachine = ConstU32<8>;
    type MaxVariablesPerManifest = ConstU32<8>;
    type MaxActionsPerManifest = ConstU32<8>;
    type MaxTransitionsPerManifest = ConstU32<2>;
    type MaxConditionsPerTransition = ConstU32<4>;
    type MaxConditionClauses = ConstU32<4>;
    type MaxEconomyGateClauses = ConstU32<4>;
    type MaxEffectsPerTransition = ConstU32<4>;
    type MaxEventsPerManifest = ConstU32<4>;
    type MaxAttestedEffectsPerEvent = ConstU32<4>;
    type MaxEventEffectPolicies = ConstU32<4>;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let storage = system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("frame-system storage build should not fail");
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

#[test]
fn runtime_upgrade_hook_is_zero_write() {
    new_test_ext().execute_with(|| {
        let before = sp_io::storage::root(StateVersion::V1);
        let weight = <pallet_blockchainia_flow::Pallet<Test> as Hooks<u64>>::on_runtime_upgrade();
        let after = sp_io::storage::root(StateVersion::V1);
        assert_eq!(before, after, "Flow extraction must not migrate state");
        assert_eq!(weight, frame_support::weights::Weight::zero());
    });
}
