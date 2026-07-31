use crate as pallet_eterra_randomness;
use crate::{DrandProofVerifier, RandomnessChainContextProvider};
#[cfg(feature = "runtime-benchmarks")]
use codec::Decode;
use eterra_nexus_primitives::Hash32;
use frame_support::{
    construct_runtime, parameter_types,
    traits::{ConstU32, Everything, UnixTime},
};
use sp_core::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage,
};
use std::{cell::Cell, time::Duration};

type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
    pub struct Test {
        System: frame_system,
        Randomness: pallet_eterra_randomness,
    }
);

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const MinFutureEpochs: u64 = 4;
    pub const MinAlphaDelayBlocks: u64 = 2;
    pub const RequestTimeoutBlocks: u64 = 20;
    pub const BeaconStaleAfterBlocks: u64 = 5;
    pub const MaxCheckpointLagRounds: u64 = 10;
    pub const MaxSignatureBytes: u32 = 96;
}

thread_local! {
    pub static MOCK_GENESIS_HASH: Cell<Hash32> = const { Cell::new([9; 32]) };
    pub static MOCK_PALLET_INSTANCE_ID: Cell<u8> = const { Cell::new(35) };
}

pub struct MockChainContext;
impl RandomnessChainContextProvider for MockChainContext {
    fn genesis_hash() -> Hash32 {
        MOCK_GENESIS_HASH.with(Cell::get)
    }

    fn pallet_instance_id() -> u8 {
        MOCK_PALLET_INSTANCE_ID.with(Cell::get)
    }
}

pub struct MockUnixTime;
impl UnixTime for MockUnixTime {
    fn now() -> Duration {
        #[cfg(feature = "runtime-benchmarks")]
        {
            let key = frame_support::storage::storage_prefix(b"Timestamp", b"Now");
            if let Some(encoded) = sp_io::storage::get(&key) {
                let millis = u64::decode(&mut &encoded[..]).expect("benchmark timestamp is u64");
                return Duration::from_millis(millis);
            }
        }
        let elapsed = 300u64.saturating_add(System::block_number().saturating_mul(6));
        Duration::from_secs(
            eterra_drand_quicknet::QUICKNET_GENESIS_UNIX_SECONDS.saturating_add(elapsed),
        )
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

pub struct MockDrandVerifier;
impl DrandProofVerifier for MockDrandVerifier {
    fn verify_quicknet(_chain_hash: &Hash32, round: u64, raw_signature: &[u8]) -> Option<Hash32> {
        if raw_signature.len() == 32 {
            let mut output = [0u8; 32];
            output.copy_from_slice(raw_signature);
            return Some(output);
        }
        eterra_drand_quicknet::verify_and_derive(round, raw_signature)
    }
}

impl crate::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type AdminOrigin = frame_system::EnsureRoot<u64>;
    type DrandVerifier = MockDrandVerifier;
    type ChainContext = MockChainContext;
    type MinFutureEpochs = MinFutureEpochs;
    type MinAlphaDelayBlocks = MinAlphaDelayBlocks;
    type RequestTimeoutBlocks = RequestTimeoutBlocks;
    type BeaconStaleAfterBlocks = BeaconStaleAfterBlocks;
    type MaxCheckpointLagRounds = MaxCheckpointLagRounds;
    type UnixTime = MockUnixTime;
    type MaxSignatureBytes = MaxSignatureBytes;
    type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1);
        MOCK_GENESIS_HASH.with(|value| value.set([9; 32]));
        MOCK_PALLET_INSTANCE_ID.with(|value| value.set(35));
    });
    ext
}
