#![cfg(test)]

use crate as pallet_eterra_card_escrow;
use crate::{CardCustodian, CardGenomeHash, GameAuthority, GameId};
use frame_support::{
    construct_runtime, parameter_types,
    sp_runtime::{
        traits::{BlakeTwo256, IdentityLookup},
        BuildStorage, DispatchError,
    },
    traits::Everything,
};
use sp_io::TestExternalities;
use std::cell::RefCell;
use std::collections::BTreeMap;

pub type AccountId = u64;
pub type Balance = u128;
pub type BlockNumber = u64;

#[derive(Clone)]
pub struct CardRecord {
    pub owner: AccountId,
    pub genome: CardGenomeHash,
}

#[derive(Clone)]
pub struct GameRecord {
    pub server: AccountId,
    pub players: Vec<AccountId>,
    pub active: bool,
}

thread_local! {
    static CARDS: RefCell<BTreeMap<u32, CardRecord>> = const { RefCell::new(BTreeMap::new()) };
    static GAMES: RefCell<BTreeMap<GameId, GameRecord>> = const { RefCell::new(BTreeMap::new()) };
    static FAIL_WITHDRAW: RefCell<bool> = const { RefCell::new(false) };
}

pub fn reset_fixtures() {
    CARDS.with(|cards| cards.borrow_mut().clear());
    GAMES.with(|games| games.borrow_mut().clear());
    FAIL_WITHDRAW.with(|fail| *fail.borrow_mut() = false);
}

pub fn seed_card(card_id: u32, owner: AccountId, genome: CardGenomeHash) {
    CARDS.with(|cards| {
        cards
            .borrow_mut()
            .insert(card_id, CardRecord { owner, genome });
    });
}

pub fn seed_game(game_id: GameId, server: AccountId, players: Vec<AccountId>, active: bool) {
    GAMES.with(|games| {
        games.borrow_mut().insert(
            game_id,
            GameRecord {
                server,
                players,
                active,
            },
        );
    });
}

pub fn card_owner(card_id: u32) -> Option<AccountId> {
    CARDS.with(|cards| cards.borrow().get(&card_id).map(|record| record.owner))
}

pub fn set_withdraw_failure(fail: bool) {
    FAIL_WITHDRAW.with(|configured| *configured.borrow_mut() = fail);
}

parameter_types! {
    pub const BlockHashCount: BlockNumber = 250;
    pub const ExistentialDeposit: Balance = 1;
    pub const RewardAmount: Balance = 100;
    pub const MaxEscrowedPerOwner: u32 = 5;
    pub const MaxReservedPerGame: u32 = 5;
    pub const MaxEventIdLen: u32 = 128;
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
    type Hash = sp_core::H256;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<AccountId>;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<Balance>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type DbWeight = ();
    type BlockHashCount = BlockHashCount;
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl pallet_balances::Config for Test {
    type Balance = Balance;
    type DustRemoval = ();
    type RuntimeEvent = RuntimeEvent;
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = ();
    type MaxLocks = frame_support::traits::ConstU32<0>;
    type MaxReserves = frame_support::traits::ConstU32<0>;
    type ReserveIdentifier = [u8; 8];
    type FreezeIdentifier = ();
    type MaxFreezes = frame_support::traits::ConstU32<0>;
    type RuntimeHoldReason = ();
    type RuntimeFreezeReason = ();
}

pub struct TestCardCustodian;

impl CardCustodian<AccountId> for TestCardCustodian {
    fn move_card_to_escrow(
        owner: &AccountId,
        escrow_account: &AccountId,
        card_id: u32,
    ) -> Result<CardGenomeHash, DispatchError> {
        CARDS.with(|cards| {
            let mut cards = cards.borrow_mut();
            let record = cards
                .get_mut(&card_id)
                .ok_or(DispatchError::Other("missing_card"))?;
            if record.owner != *owner {
                return Err(DispatchError::Other("not_owner"));
            }
            record.owner = *escrow_account;
            Ok(record.genome)
        })
    }

    fn move_card_from_escrow(
        escrow_account: &AccountId,
        owner: &AccountId,
        card_id: u32,
    ) -> frame_support::dispatch::DispatchResult {
        if FAIL_WITHDRAW.with(|fail| *fail.borrow()) {
            return Err(DispatchError::Other("forced_withdraw_failure"));
        }
        CARDS.with(|cards| {
            let mut cards = cards.borrow_mut();
            let record = cards
                .get_mut(&card_id)
                .ok_or(DispatchError::Other("missing_card"))?;
            if record.owner != *escrow_account {
                return Err(DispatchError::Other("not_escrowed"));
            }
            record.owner = *owner;
            Ok(())
        })
    }
}

pub struct TestGameAuthority;

impl GameAuthority<AccountId> for TestGameAuthority {
    fn ensure_game_owned_by(
        game_id: GameId,
        caller: &AccountId,
    ) -> frame_support::dispatch::DispatchResult {
        GAMES.with(|games| {
            let games = games.borrow();
            let game = games
                .get(&game_id)
                .ok_or(DispatchError::Other("missing_game"))?;
            if game.server != *caller {
                return Err(DispatchError::Other("not_server"));
            }
            Ok(())
        })
    }

    fn ensure_active_game_owned_by(
        game_id: GameId,
        caller: &AccountId,
    ) -> frame_support::dispatch::DispatchResult {
        GAMES.with(|games| {
            let games = games.borrow();
            let game = games
                .get(&game_id)
                .ok_or(DispatchError::Other("missing_game"))?;
            if game.server != *caller {
                return Err(DispatchError::Other("not_server"));
            }
            if !game.active {
                return Err(DispatchError::Other("inactive_game"));
            }
            Ok(())
        })
    }

    fn ensure_player_in_game(
        game_id: GameId,
        player: &AccountId,
    ) -> frame_support::dispatch::DispatchResult {
        GAMES.with(|games| {
            let games = games.borrow();
            let game = games
                .get(&game_id)
                .ok_or(DispatchError::Other("missing_game"))?;
            if !game.players.contains(player) {
                return Err(DispatchError::Other("not_player"));
            }
            Ok(())
        })
    }
}

impl pallet_eterra_card_escrow::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type AccessControl = ();
    type Currency = Balances;
    type RewardAmount = RewardAmount;
    type MaxEscrowedPerOwner = MaxEscrowedPerOwner;
    type MaxReservedPerGame = MaxReservedPerGame;
    type MaxEventIdLen = MaxEventIdLen;
    type CardCustodian = TestCardCustodian;
    type GameAuthority = TestGameAuthority;
    type WeightInfo = ();
}

construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        CardEscrow: pallet_eterra_card_escrow,
    }
);

pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;
pub const CHARLIE: AccountId = 3;
pub const SERVER: AccountId = 99;

pub struct ExtBuilder;

impl ExtBuilder {
    pub fn build() -> TestExternalities {
        reset_fixtures();
        let mut storage = frame_system::GenesisConfig::<Test>::default()
            .build_storage()
            .expect("system storage");
        pallet_balances::GenesisConfig::<Test> {
            balances: vec![
                (ALICE, 1_000),
                (BOB, 1_000),
                (CHARLIE, 1_000),
                (SERVER, 1_000),
            ],
        }
        .assimilate_storage(&mut storage)
        .expect("balances storage");
        let mut ext = TestExternalities::new(storage);
        ext.execute_with(|| System::set_block_number(1));
        ext
    }
}
