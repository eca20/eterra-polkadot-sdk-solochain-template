use crate as pallet_cryptostrike;
use frame_support::derive_impl;
use frame_support::parameter_types;
use sp_core::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage, DispatchError,
};
use std::{cell::RefCell, collections::BTreeMap};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
    pub enum Test
    {
        System: frame_system,
        CryptoStrike: pallet_cryptostrike,
    }
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
}

parameter_types! {
    pub const MaxSettlementEntries: u32 = 16;
    pub const MaxCombatStatEntries: u32 = 16;
    pub const MaxRivalStatEntries: u32 = 16;
    pub const MaxServerSignatureLen: u32 = 96;
    pub const MinServerStake: u128 = 100;
    pub const UnstakeDelay: u64 = 5;
}

thread_local! {
    static GUAP_BALANCES: RefCell<BTreeMap<u64, u128>> = const { RefCell::new(BTreeMap::new()) };
    static RESERVED_STAKES: RefCell<BTreeMap<u64, u128>> = const { RefCell::new(BTreeMap::new()) };
    static SLASHED_STAKES: RefCell<BTreeMap<u64, u128>> = const { RefCell::new(BTreeMap::new()) };
    static STEAM_TO_ACCOUNT: RefCell<BTreeMap<pallet_cryptostrike::SteamHash, u64>> = const { RefCell::new(BTreeMap::new()) };
    static ACCOUNT_TO_STEAM: RefCell<BTreeMap<u64, pallet_cryptostrike::SteamHash>> = const { RefCell::new(BTreeMap::new()) };
    static FROZEN_ACCOUNTS: RefCell<BTreeMap<u64, bool>> = const { RefCell::new(BTreeMap::new()) };
}

pub struct MockGuapLedger;

impl pallet_cryptostrike::GuapLedger<u64, u128> for MockGuapLedger {
    fn mint(account: &u64, amount: u128) -> frame_support::dispatch::DispatchResult {
        GUAP_BALANCES.with(|balances| {
            let mut balances = balances.borrow_mut();
            let current = balances.get(account).copied().unwrap_or_default();
            balances.insert(*account, current.saturating_add(amount));
        });
        Ok(())
    }

    fn burn(account: &u64, amount: u128) -> frame_support::dispatch::DispatchResult {
        GUAP_BALANCES.with(|balances| {
            let mut balances = balances.borrow_mut();
            let current = balances.get(account).copied().unwrap_or_default();
            if current < amount {
                return Err(DispatchError::Other("insufficient guap"));
            }
            balances.insert(*account, current - amount);
            Ok(())
        })
    }

    fn transfer(from: &u64, to: &u64, amount: u128) -> frame_support::dispatch::DispatchResult {
        GUAP_BALANCES.with(|balances| {
            let mut balances = balances.borrow_mut();
            let from_balance = balances.get(from).copied().unwrap_or_default();
            if from_balance < amount {
                return Err(DispatchError::Other("insufficient guap"));
            }
            if from == to {
                return Ok(());
            }
            let to_balance = balances.get(to).copied().unwrap_or_default();
            balances.insert(*from, from_balance - amount);
            balances.insert(*to, to_balance.saturating_add(amount));
            Ok(())
        })
    }
}

pub struct MockStakeLedger;

impl pallet_cryptostrike::StakeLedger<u64, u128> for MockStakeLedger {
    fn reserve(account: &u64, amount: u128) -> frame_support::dispatch::DispatchResult {
        GUAP_BALANCES.with(|balances| {
            RESERVED_STAKES.with(|reserved| {
                let mut balances = balances.borrow_mut();
                let mut reserved = reserved.borrow_mut();
                let free = balances.get(account).copied().unwrap_or_default();
                if free < amount {
                    return Err(DispatchError::Other("insufficient stake balance"));
                }
                let reserved_balance = reserved.get(account).copied().unwrap_or_default();
                balances.insert(*account, free - amount);
                reserved.insert(*account, reserved_balance.saturating_add(amount));
                Ok(())
            })
        })
    }

    fn release(account: &u64, amount: u128) -> frame_support::dispatch::DispatchResult {
        RESERVED_STAKES.with(|reserved| {
            GUAP_BALANCES.with(|balances| {
                let mut reserved = reserved.borrow_mut();
                let mut balances = balances.borrow_mut();
                let reserved_balance = reserved.get(account).copied().unwrap_or_default();
                if reserved_balance < amount {
                    return Err(DispatchError::Other("insufficient reserved stake"));
                }
                let free = balances.get(account).copied().unwrap_or_default();
                reserved.insert(*account, reserved_balance - amount);
                balances.insert(*account, free.saturating_add(amount));
                Ok(())
            })
        })
    }

    fn slash_reserved(account: &u64, amount: u128) -> frame_support::dispatch::DispatchResult {
        RESERVED_STAKES.with(|reserved| {
            SLASHED_STAKES.with(|slashed| {
                let mut reserved = reserved.borrow_mut();
                let mut slashed = slashed.borrow_mut();
                let reserved_balance = reserved.get(account).copied().unwrap_or_default();
                if reserved_balance < amount {
                    return Err(DispatchError::Other("insufficient reserved stake"));
                }
                let slashed_balance = slashed.get(account).copied().unwrap_or_default();
                reserved.insert(*account, reserved_balance - amount);
                slashed.insert(*account, slashed_balance.saturating_add(amount));
                Ok(())
            })
        })
    }
}

pub struct MockServerSignatureVerifier;

impl
    pallet_cryptostrike::ServerSignatureVerifier<
        H256,
        frame_support::BoundedVec<u8, MaxServerSignatureLen>,
    > for MockServerSignatureVerifier
{
    fn verify(
        server_pubkey: &[u8; 32],
        payload_hash: &H256,
        signature: &frame_support::BoundedVec<u8, MaxServerSignatureLen>,
    ) -> bool {
        if server_pubkey.iter().all(|byte| *byte == 0) {
            return false;
        }

        let mut expected = b"server-signature".to_vec();
        expected.extend_from_slice(payload_hash.as_ref());
        signature.as_slice() == expected.as_slice()
    }
}

pub struct MockIdentityProvider;

impl pallet_cryptostrike::SteamIdentityProvider<u64> for MockIdentityProvider {
    fn account_for_steam_hash(steam_hash: pallet_cryptostrike::SteamHash) -> Option<u64> {
        STEAM_TO_ACCOUNT.with(|links| links.borrow().get(&steam_hash).copied())
    }

    fn steam_hash_for_account(account: &u64) -> Option<pallet_cryptostrike::SteamHash> {
        ACCOUNT_TO_STEAM.with(|links| links.borrow().get(account).copied())
    }

    fn is_frozen(account: &u64) -> bool {
        FROZEN_ACCOUNTS.with(|frozen| frozen.borrow().get(account).copied().unwrap_or(false))
    }
}

impl pallet_cryptostrike::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type AdminOrigin = frame_system::EnsureRoot<u64>;
    type Balance = u128;
    type MaxSettlementEntries = MaxSettlementEntries;
    type MaxCombatStatEntries = MaxCombatStatEntries;
    type MaxRivalStatEntries = MaxRivalStatEntries;
    type MaxServerSignatureLen = MaxServerSignatureLen;
    type MinServerStake = MinServerStake;
    type UnstakeDelay = UnstakeDelay;
    type GuapLedger = MockGuapLedger;
    type StakeLedger = MockStakeLedger;
    type ServerSignatureVerifier = MockServerSignatureVerifier;
    type IdentityProvider = MockIdentityProvider;
    type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    reset_guap_balances();
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    let mut ext: sp_io::TestExternalities = storage.into();
    ext.execute_with(|| System::set_block_number(1));
    ext
}

pub fn steam_hash(seed: u8) -> pallet_cryptostrike::SteamHash {
    [seed; 32]
}

pub fn metadata_hash(seed: u8) -> pallet_cryptostrike::MetadataHash {
    [seed; 32]
}

pub fn map_name_hash(seed: u8) -> pallet_cryptostrike::MapNameHash {
    [seed; 32]
}

pub fn previous_round_hash(seed: u8) -> pallet_cryptostrike::RoundHash {
    [seed; 32]
}

pub fn config_hash(seed: u8) -> pallet_cryptostrike::ConfigHash {
    [seed; 32]
}

pub fn session_id(seed: u8) -> pallet_cryptostrike::SessionId {
    [seed; 32]
}

pub fn roster_root(seed: u8) -> pallet_cryptostrike::RosterRoot {
    [seed; 32]
}

pub fn menu_nonce(seed: u8) -> pallet_cryptostrike::MenuNonce {
    [seed; 32]
}

pub fn server_pubkey(seed: u8) -> [u8; 32] {
    [seed; 32]
}

pub fn link_steam(account: u64, hash: pallet_cryptostrike::SteamHash) {
    STEAM_TO_ACCOUNT.with(|links| {
        links.borrow_mut().insert(hash, account);
    });
    ACCOUNT_TO_STEAM.with(|links| {
        links.borrow_mut().insert(account, hash);
    });
}

pub fn unlink_steam(account: u64) {
    let maybe_hash = ACCOUNT_TO_STEAM.with(|links| links.borrow_mut().remove(&account));
    if let Some(hash) = maybe_hash {
        STEAM_TO_ACCOUNT.with(|links| {
            links.borrow_mut().remove(&hash);
        });
    }
}

pub fn freeze_account(account: u64) {
    FROZEN_ACCOUNTS.with(|frozen| {
        frozen.borrow_mut().insert(account, true);
    });
}

pub fn unfreeze_account(account: u64) {
    FROZEN_ACCOUNTS.with(|frozen| {
        frozen.borrow_mut().remove(&account);
    });
}

pub fn set_guap_balance(account: u64, amount: u128) {
    GUAP_BALANCES.with(|balances| {
        balances.borrow_mut().insert(account, amount);
    });
}

pub fn guap_balance(account: u64) -> u128 {
    GUAP_BALANCES.with(|balances| balances.borrow().get(&account).copied().unwrap_or_default())
}

fn reset_guap_balances() {
    GUAP_BALANCES.with(|balances| balances.borrow_mut().clear());
    RESERVED_STAKES.with(|reserved| reserved.borrow_mut().clear());
    SLASHED_STAKES.with(|slashed| slashed.borrow_mut().clear());
    STEAM_TO_ACCOUNT.with(|links| links.borrow_mut().clear());
    ACCOUNT_TO_STEAM.with(|links| links.borrow_mut().clear());
    FROZEN_ACCOUNTS.with(|frozen| frozen.borrow_mut().clear());
}

pub fn reserved_stake(account: u64) -> u128 {
    RESERVED_STAKES.with(|reserved| reserved.borrow().get(&account).copied().unwrap_or_default())
}

pub fn slashed_stake(account: u64) -> u128 {
    SLASHED_STAKES.with(|slashed| slashed.borrow().get(&account).copied().unwrap_or_default())
}
