use crate as pallet_eterra_monte_carlo_ai;
use frame_support::{parameter_types, traits::Everything};
use frame_system as system;
use sp_runtime::BuildStorage;

use sp_core::H256;
use sp_io::TestExternalities;
use sp_runtime::traits::{BlakeTwo256, IdentityLookup};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
    pub enum Test where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: frame_system,
        EterraAi: pallet_eterra_monte_carlo_ai,
    }
);

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const MaxActionsConst: u32 = 8;
    pub const BaseIterationsConst: u32 = 200; // base rollouts
    pub const MaxPlayoutDepthConst: u16 = 32;
    pub const RandomnessSeedConst: u64 = 0xDEAD_BEEF_CAFE_BABE;
}

impl system::Config for Test {
    type BaseCallFilter = Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type Block = Block;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = u64;
    type Lookup = IdentityLookup<u64>;
    type BlockHashCount = BlockHashCount;
    type DbWeight = ();
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = ();
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
    type RuntimeTask = ();
    type Nonce = u32;
    type SingleBlockMigrations = ();
    type MultiBlockMigrator = ();
    type PreInherents = ();
    type PostInherents = ();
    type PostTransactions = ();
}

/// --- Toy Nim adapter for tests ---
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

#[derive(Clone, Encode, Decode, TypeInfo, MaxEncodedLen, Debug, PartialEq, Eq)]
pub enum NimAction {
    Take1,
    Take2,
}

#[derive(Clone, Encode, Decode, TypeInfo, MaxEncodedLen, Debug, PartialEq, Eq)]
pub struct NimState {
    pub pile: u8,
    pub to_move: u8,
} // player {0,1}

pub struct NimAdapter;
impl pallet_eterra_monte_carlo_ai::GameAdapter for NimAdapter {
    type State = NimState;
    type Action = NimAction;
    type Player = u8;

    fn list_actions<const MAX: usize>(
        s: &Self::State,
        out: &mut [Option<Self::Action>; MAX],
    ) -> usize {
        if s.pile == 0 {
            return 0;
        }
        let mut k = 0;
        out[k] = Some(NimAction::Take1);
        k += 1;
        if s.pile >= 2 {
            out[k] = Some(NimAction::Take2);
            k += 1;
        }
        k
    }
    fn apply(s: &Self::State, a: &Self::Action) -> Self::State {
        let take = match a {
            NimAction::Take1 => 1,
            NimAction::Take2 => 2,
        };
        NimState {
            pile: s.pile.saturating_sub(take),
            to_move: 1 - s.to_move,
        }
    }
    fn is_terminal(s: &Self::State) -> bool {
        s.pile == 0
    }
    fn current_player(s: &Self::State) -> Self::Player {
        s.to_move
    }
    fn score(s: &Self::State, for_p: Self::Player) -> i32 {
        if !Self::is_terminal(s) {
            0
        } else {
            // If terminal, the player who JUST moved wins
            let winner = 1 - s.to_move;
            if winner == for_p {
                1
            } else {
                -1
            }
        }
    }
    fn random_action(s: &Self::State, seed: u64) -> Option<Self::Action> {
        if s.pile == 0 {
            return None;
        }
        if s.pile == 1 {
            return Some(NimAction::Take1);
        }
        if (seed & 1) == 0 {
            Some(NimAction::Take1)
        } else {
            Some(NimAction::Take2)
        }
    }
}

parameter_types! {
    pub const SS: u8 = 0;
}

impl pallet_eterra_monte_carlo_ai::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Adapter = NimAdapter;
    type MaxActions = MaxActionsConst;
    type BaseIterations = BaseIterationsConst;
    type MaxPlayoutDepth = MaxPlayoutDepthConst;
    type RandomnessSeed = RandomnessSeedConst;
}

pub fn new_test_ext() -> TestExternalities {
    let t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    let mut ext = TestExternalities::from(t);
    ext.execute_with(|| {
        System::set_block_number(1);
    });
    ext
}
