// pallets/eterra-game-authority/src/mock.rs

#![cfg(test)]

use crate as pallet_eterra_game_authority;
use frame_support::{
    construct_runtime, parameter_types,
    traits::Everything,
    sp_runtime::BuildStorage,
    sp_runtime::traits::{BlakeTwo256, IdentityLookup, Hash as HashT},
};
use sp_io::TestExternalities;
use frame_system::EnsureRoot;

pub type AccountId = u64;

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const MaxPlayersPerGame: u32 = 128;
}

impl frame_system::Config for Test {
    type BaseCallFilter = Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type Nonce = u64;
    type Block = frame_system::mocking::MockBlock<Test>;
    type RuntimeTask = ();
    type SingleBlockMigrations = ();
    type MultiBlockMigrator = ();
    type PreInherents = ();
    type PostInherents = ();
    type PostTransactions = ();
    type Hash = <BlakeTwo256 as HashT>::Output;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<AccountId>;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = ();
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type DbWeight = ();
    type BlockHashCount = BlockHashCount;
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl pallet_eterra_game_authority::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxPlayersPerGame = MaxPlayersPerGame;
    // Governance-only management of server whitelist for tests:
    type AdminOrigin = EnsureRoot<AccountId>;
    type MaxExpirationsPerBlock = frame_support::traits::ConstU32<256>;
    type MaxRoundBlocks = frame_support::traits::ConstU64<30>;
    type MaxBatchAdd = frame_support::traits::ConstU32<32>;
}

// Build a mock runtime for tests.
construct_runtime!(
    pub enum Test
    {
        System: frame_system,
        GameAuthority: pallet_eterra_game_authority,
    }
);

/// Simple accounts for tests
pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;
pub const CHARLIE: AccountId = 3;

/// Builder to initialize genesis state, including pallet whitelist.
pub struct ExtBuilder {
    pub initial_servers: Vec<AccountId>,
}

impl Default for ExtBuilder {
    fn default() -> Self {
        Self { initial_servers: vec![] }
    }
}

impl ExtBuilder {
    pub fn with_servers(mut self, servers: Vec<AccountId>) -> Self {
        self.initial_servers = servers;
        self
    }

    pub fn build(self) -> TestExternalities {
        let mut t = frame_system::GenesisConfig::<Test>::default()
            .build_storage()
            .expect("system storage builds");

        // Seed the pallet with whitelisted servers at genesis.
        pallet_eterra_game_authority::GenesisConfig::<Test> {
            initial_servers: self.initial_servers,
            _phantom: Default::default(),
        }
        .assimilate_storage(&mut t)
        .expect("pallet storage assimilates");

        let mut ext = TestExternalities::new(t);
        ext.execute_with(|| System::set_block_number(1));
        ext
    }
}