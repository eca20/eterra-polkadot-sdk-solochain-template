// This is free and unencumbered software released into the public domain.
//
// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.
//
// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.
//
// For more information, please refer to <http://unlicense.org>

// Substrate and Polkadot dependencies
use frame_support::{
    derive_impl, parameter_types,
    traits::{ConstBool, ConstU128, ConstU16, ConstU32, ConstU64, ConstU8, VariantCountOf},
    weights::{
        constants::{RocksDbWeight, WEIGHT_REF_TIME_PER_SECOND},
        IdentityFee, Weight,
    },
};
use frame_system::limits::{BlockLength, BlockWeights};
use pallet_transaction_payment::{ConstFeeMultiplier, Multiplier};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_runtime::{
    traits::{One, AccountIdConversion},
    Perbill,
};
use sp_version::RuntimeVersion;
use scale_info::TypeInfo;
use codec::{Encode, Decode};
use frame_support::traits::Get;
use frame_support::PalletId;

// Bring in UNIT and HandProviderAdapter from the parent module (lib.rs)
use super::{UNIT, HandProviderAdapter};

// Bring in the pallets re-exported in lib.rs
use super::{
    pallet_eterra,
    pallet_eterra_daily_slots,
    pallet_eterra_faucet,
    pallet_eterra_simple_tcg,
    pallet_eterra_tcg,
    pallet_eterra_simple_matchmaker,
    pallet_eterra_gamer,
    pallet_eterra_game_authority,
    pallet_eterra_media,
};
// Monte Carlo AI pallet lives at the crate root; bring it in explicitly.

// Local module imports
use super::{
    AccountId, Aura, Balance, Balances, Block, BlockNumber, Hash, Nonce, PalletInfo, Runtime,
    RuntimeCall, RuntimeEvent, RuntimeFreezeReason, RuntimeHoldReason, RuntimeOrigin, RuntimeTask,
    System, EXISTENTIAL_DEPOSIT, SLOT_DURATION, VERSION,
};

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

parameter_types! {
    pub const BlockHashCount: BlockNumber = 2400;
    pub const Version: RuntimeVersion = VERSION;

    /// We allow for 2 seconds of compute with a 6 second average block time.
    pub RuntimeBlockWeights: BlockWeights = BlockWeights::with_sensible_defaults(
        Weight::from_parts(2u64 * WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
        NORMAL_DISPATCH_RATIO,
    );
    pub RuntimeBlockLength: BlockLength = BlockLength::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
    pub const SS58Prefix: u8 = 42;
}

/// The default types are being injected by [`derive_impl`](`frame_support::derive_impl`) from
/// [`SoloChainDefaultConfig`](`struct@frame_system::config_preludes::SolochainDefaultConfig`),
/// but overridden as needed.
#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig)]
impl frame_system::Config for Runtime {
    /// The block type for the runtime.
    type Block = Block;
    /// Block & extrinsics weights: base values and limits.
    type BlockWeights = RuntimeBlockWeights;
    /// The maximum length of a block (in bytes).
    type BlockLength = RuntimeBlockLength;
    /// The identifier used to distinguish between accounts.
    type AccountId = AccountId;
    /// The type for storing how many extrinsics an account has signed.
    type Nonce = Nonce;
    /// The type for hashing blocks and tries.
    type Hash = Hash;
    /// Maximum number of block number to block hash mappings to keep (oldest pruned first).
    type BlockHashCount = BlockHashCount;
    /// The weight of database operations that the runtime can invoke.
    type DbWeight = RocksDbWeight;
    /// Version of the runtime.
    type Version = Version;
    /// The data to be stored in an account.
    type AccountData = pallet_balances::AccountData<Balance>;
    /// This is used as an identifier of the chain. 42 is the generic substrate prefix.
    type SS58Prefix = SS58Prefix;
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl pallet_aura::Config for Runtime {
    type AuthorityId = AuraId;
    type DisabledValidators = ();
    type MaxAuthorities = ConstU32<32>;
    type AllowMultipleBlocksPerSlot = ConstBool<false>;
    type SlotDuration = pallet_aura::MinimumPeriodTimesTwo<Runtime>;
}

impl pallet_grandpa::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;

    type WeightInfo = ();
    type MaxAuthorities = ConstU32<32>;
    type MaxNominators = ConstU32<0>;
    type MaxSetIdSessionEntries = ConstU64<0>;

    type KeyOwnerProof = sp_core::Void;
    type EquivocationReportSystem = ();
}

impl pallet_timestamp::Config for Runtime {
    /// A timestamp: milliseconds since the unix epoch.
    type Moment = u64;
    type OnTimestampSet = Aura;
    type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
    type WeightInfo = ();
}

impl pallet_balances::Config for Runtime {
    type MaxLocks = ConstU32<50>;
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    /// The type for recording an account's balance.
    type Balance = Balance;
    /// The ubiquitous event type.
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = ();
    type ExistentialDeposit = ConstU128<EXISTENTIAL_DEPOSIT>;
    type AccountStore = System;
    type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
    type FreezeIdentifier = RuntimeFreezeReason;
    type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeHoldReason;
}

parameter_types! {
    pub FeeMultiplier: Multiplier = Multiplier::one();
}

// --- Make faucet.claim feeless by customizing OnChargeTransaction ---
use pallet_transaction_payment::OnChargeTransaction;
use sp_runtime::traits::{DispatchInfoOf, PostDispatchInfoOf};
use sp_runtime::transaction_validity::TransactionValidityError;

pub struct FreeFaucetOrCurrencyAdapter;

impl OnChargeTransaction<Runtime> for FreeFaucetOrCurrencyAdapter {
    type Balance = Balance;
    type LiquidityInfo =
        <pallet_transaction_payment::FungibleAdapter<Balances, ()> as OnChargeTransaction<
            Runtime,
        >>::LiquidityInfo;

    fn withdraw_fee(
        who: &AccountId,
        call: &RuntimeCall,
        info: &DispatchInfoOf<RuntimeCall>,
        tip: Self::Balance,
        fee: Self::Balance,
    ) -> Result<Self::LiquidityInfo, TransactionValidityError> {
        // If the call is the faucet claim, skip withdrawing any fee (including tip).
        if matches!(
            call,
            RuntimeCall::EterraFaucet(pallet_eterra_faucet::Call::claim { .. })
        ) {
            return Ok(Default::default());
        }
        // Otherwise delegate to the default adapter.
        <pallet_transaction_payment::FungibleAdapter<Balances, ()> as OnChargeTransaction<
            Runtime,
        >>::withdraw_fee(who, call, info, tip, fee)
    }

    fn correct_and_deposit_fee(
        who: &AccountId,
        info: &DispatchInfoOf<RuntimeCall>,
        post_info: &PostDispatchInfoOf<RuntimeCall>,
        tip: Self::Balance,
        fee: Self::Balance,
        paid: Self::LiquidityInfo,
    ) -> Result<(), TransactionValidityError> {
        // If we skipped withdrawing (paid is default/None), do nothing on deposit.
        if paid == Default::default() {
            return Ok(());
        }
        // Otherwise delegate to the default adapter.
        <pallet_transaction_payment::FungibleAdapter<Balances, ()> as OnChargeTransaction<
            Runtime,
        >>::correct_and_deposit_fee(who, info, post_info, tip, fee, paid)
    }
}

impl pallet_transaction_payment::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnChargeTransaction = FreeFaucetOrCurrencyAdapter;
    type OperationalFeeMultiplier = ConstU8<5>;
    type WeightToFee = IdentityFee<Balance>;
    type LengthToFee = IdentityFee<Balance>;
    type FeeMultiplierUpdate = ConstFeeMultiplier<FeeMultiplier>;
}

impl pallet_sudo::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type WeightInfo = pallet_sudo::weights::SubstrateWeight<Runtime>;
}

impl pallet_node_authorization::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type MaxWellKnownNodes = MaxWellKnownNodes;
    type MaxPeerIdLength = MaxPeerIdLength;

    // While bootstrapping, keep it simple: Root controls the allowlist.
    type AddOrigin   = frame_system::EnsureRoot<AccountId>;
    type RemoveOrigin= frame_system::EnsureRoot<AccountId>;
    type SwapOrigin  = frame_system::EnsureRoot<AccountId>;
    type ResetOrigin = frame_system::EnsureRoot<AccountId>;

    type WeightInfo = ();
}


#[derive(Encode, Decode, TypeInfo, Clone, Copy, PartialEq, Eq, Debug)]
pub struct EterraNumPlayers;
impl Get<u32> for EterraNumPlayers {
    fn get() -> u32 {
        2 // The number of players in the game
    }
}

pub struct EterraMaxRounds;
impl Get<u8> for EterraMaxRounds {
    fn get() -> u8 {
        5 // The number of players in the game
    }
}

pub struct MaxRollHistoryLength;
impl Get<u32> for MaxRollHistoryLength {
    fn get() -> u32 {
        100 // The number of players in the game
    }
}

pub struct EterraBlocksToPlayLimit;
impl Get<u8> for EterraBlocksToPlayLimit {
    fn get() -> u8 {
        6 // The limit in blocks each player has until their turn is force finished
          // Eventually, the force finish may allow the opponent to click to force finish
          // but forcing the node to finish turns prevents stale games from laying around
          // while risking bots accruing rewards.
    }
}

pub struct MaxSlotLength;
impl Get<u32> for MaxSlotLength {
    fn get() -> u32 {
        3 // The number of slots per slot roll
    }
}

pub struct MaxOptionsPerSlot;
impl Get<u32> for MaxOptionsPerSlot {
    fn get() -> u32 {
        10 // The number of players in the game
    }
}

pub struct MaxRollsPerRound;
impl Get<u32> for MaxRollsPerRound {
    fn get() -> u32 {
        3 // The number of players in the game
    }
}

pub struct MaxWeightEntries;
impl Get<u32> for MaxWeightEntries {
    fn get() -> u32 {
        100 // max symbol entries per reel (example)
    }
}

pub struct MaxDrawingEntries;
impl Get<u32> for MaxDrawingEntries {
    fn get() -> u32 {
        1_000 // max ticket holders processed per weekly draw
    }
}

parameter_types! {
    // 6 seconds per block → ~30 blocks for ~3 minutes
    pub const MaxExpirationsPerBlock: u32 = 256; // tune as needed
}

parameter_types! {
    pub const MaxPlayersPerGameConst: u32 = 128; // tune as needed
}

parameter_types! {
    pub const MaxWellKnownNodes: u32 = 128;   // adjust as you like
    pub const MaxPeerIdLength: u32 = 128;     // libp2p PeerId length upper bound
}

// === Faucet configuration parameters ===

parameter_types! {
    // Treasury account derived from a fixed PalletId; do not change after genesis.
    pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
    pub TreasuryAccount: AccountId = TreasuryPalletId::get().into_account_truncating();

    // AI bot can also use a PalletId-based account to avoid dev keys.
    pub const AiBotPalletId: PalletId = PalletId(*b"ai/bot__");
    pub AiBotAccountParam: AccountId = AiBotPalletId::get().into_account_truncating();

    pub const PlayersPerMatchConst: u8 = 2;
    pub const QueueCapacityConst: u32 = 1024;

    // Payout is 1000 whole tokens (adjust UNIT to your decimals)
    pub FaucetPayoutAmount: Balance = 1_000 * UNIT;
}

impl pallet_eterra_faucet::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type WeightInfo = pallet_eterra_faucet::weights::SubstrateWeight<Runtime>;
}



parameter_types! {
    pub const GamerTagMaxLen: u32 = 32;
    pub const AvatarCidMaxLen: u32 = 96; // or 128
    pub const GamerChangeFee: Balance = 100u128;
}
impl pallet_eterra_gamer::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type ExpIssuerOrigin = frame_system::EnsureRoot<AccountId>;
    type FaucetAccount = TreasuryAccount;
    type ChangeFee = GamerChangeFee;
    type MaxTagLen = GamerTagMaxLen;
    type MaxAvatarCidLen = AvatarCidMaxLen;
}

impl pallet_eterra_monte_carlo_ai::pallet::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Adapter = eterra_card_ai_adapter::eterra_adapter::Adapter;
    // Limits & tuning params for Monte Carlo search
    type MaxActions = ConstU32<64>;        // max legal moves enumerated
    type BaseIterations = ConstU32<200>;   // baseline simulations per suggest() call
    type MaxPlayoutDepth = ConstU16<16>;   // cut off long playouts
    type RandomnessSeed = ConstU64<12345>; // deterministic-ish seed for hashing/entropy
    type WeightInfo = ();
}



pub struct RewardPerWinAmount;
impl frame_support::traits::Get<Balance> for RewardPerWinAmount {
    fn get() -> Balance {
        100 * UNIT
    }
}

impl pallet_eterra_game_authority::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type MaxPlayersPerGame = MaxPlayersPerGameConst;
    type AdminOrigin = frame_system::EnsureRoot<AccountId>;
    type MaxExpirationsPerBlock = MaxExpirationsPerBlock;
    // If your BlockNumber is u32/u64, set 30 blocks:
    type MaxRoundBlocks = frame_support::traits::ConstU32<30>;
    // or, if BlockNumber is u64:
    // type MaxRoundBlocks = frame_support::traits::ConstU64<30>;

    // Max players that can be added in a single batch to a game
    type MaxBatchAdd = frame_support::traits::ConstU32<32>;
    type WeightInfo = pallet_eterra_game_authority::weights::SubstrateWeight<Runtime>;
}

impl pallet_eterra_daily_slots::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type TimeProvider = pallet_timestamp::Pallet<Runtime>;
    type MaxSlotLength = MaxSlotLength;
    type MaxOptionsPerSlot = MaxOptionsPerSlot;
    type MaxRollsPerRound = MaxRollsPerRound;
    type MaxRollHistoryLength = MaxRollHistoryLength;
    type MaxWeightEntries = MaxWeightEntries;
    type MaxDrawingEntries = MaxDrawingEntries;
    type Currency = Balances;
    type RewardPerWin = RewardPerWinAmount; // defined below
    type WeightInfo = pallet_eterra_daily_slots::weights::SubstrateWeight<Runtime>;
}

impl pallet_eterra_simple_tcg::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;

    // You already had this:
    type RandomnessSeed = ConstU64<12345>;

    // NEW: hook up balances as the currency
    type Currency = Balances;

    // NEW: fixed mint fee of 100 whole tokens (uses your UNIT = base units)
    type MintFee = ConstU128<{ 100 * UNIT }>;

    // NEW: the faucet account that should receive the fee (Treasury via PalletId!)
    type FaucetAccount = TreasuryAccount;

    type WeightInfo = pallet_eterra_simple_tcg::weights::SubstrateWeight<Runtime>;

}

impl pallet_eterra_simple_matchmaker::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type PlayersPerMatch = PlayersPerMatchConst;
    type QueueCapacity = QueueCapacityConst;
    type HandProvider = MatchmakerHandProvider;
    type GameCreator  = pallet_eterra::Pallet<Runtime>;
    type WeightInfo = pallet_eterra_simple_matchmaker::weights::SubstrateWeight<Runtime>;
}

impl pallet_eterra_simple_matchmaker::CurrentHandProvider<AccountId> for HandProviderAdapter {
    fn has_current_hand(who: &AccountId) -> bool {
        // Delegate to your game/cards pallet storage:
        // Adjust the path to your pallet module and types.
        pallet_eterra::CurrentHandOf::<Runtime>::contains_key(who)
    }
}

#[cfg(feature = "runtime-benchmarks")]
pub struct BenchmarkHandProvider;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_eterra_simple_matchmaker::CurrentHandProvider<AccountId> for BenchmarkHandProvider {
    fn has_current_hand(_who: &AccountId) -> bool {
        true
    }
}

#[cfg(feature = "runtime-benchmarks")]
type MatchmakerHandProvider = BenchmarkHandProvider;

#[cfg(not(feature = "runtime-benchmarks"))]
type MatchmakerHandProvider = HandProviderAdapter;


impl pallet_eterra_tcg::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RandomnessSeed = ConstU64<42>;

    type MaxAttempts = ConstU8<3>; // Set maximum attempts per card to 3
    type CardsPerPack = ConstU8<5>; // Set number of cards per pack to 5
    type MaxPacks = ConstU32<10>; // Set maximum packs a player can have to 10
    type WeightInfo = pallet_eterra_tcg::weights::SubstrateWeight<Runtime>;
}

pub struct AiBotDifficulty;
impl Get<u8> for AiBotDifficulty {
    fn get() -> u8 { 60 }
}

impl pallet_eterra::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type NumPlayers = EterraNumPlayers;
    type MaxRounds = EterraMaxRounds;
    type BlocksToPlayLimit = EterraBlocksToPlayLimit;
    type HandSize = ConstU32<5>; // <<—— added
    type AiAccount = AiBotAccountParam;
    type AiDifficulty = ConstU8<60>;
    type WeightInfo = pallet_eterra::weights::SubstrateWeight<Runtime>;
}

// FILE: runtime/src/configs/mod.rs
parameter_types! {
    // Maximum length (in bytes) of the on-chain URI (e.g. "ipfs://...").
    pub const MaxMediaUriLen: u32 = 256;

    // Maximum length (in bytes) of the on-chain content type string
    // (e.g. "image/png", "image/jpeg").
    pub const MaxMediaContentTypeLen: u32 = 64;

    // Upper bound on the number of distinct collections.
    pub const MaxMediaCollections: u32 = 1024;
}

parameter_types! {
    // Maximum length (in bytes) of a collection or media name.
    pub const MaxMediaNameLen: u32 = 64;

    // Maximum length (in bytes) of a collection or media description.
    pub const MaxMediaDescriptionLen: u32 = 256;

    // Maximum number of roles an account can have across collections.
    pub const MaxMediaRolesPerAccount: u32 = 8;

    // Default collection id used when none is specified.
    pub const DefaultMediaCollectionId: u32 = 0;
}

// FILE: runtime/src/configs/mod.rs
impl pallet_eterra_media::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;

    // Bounded sizes for URI and content-type.
    type MaxUriLen = MaxMediaUriLen;
    type MaxContentTypeLen = MaxMediaContentTypeLen;

    // New: bounded sizes for names and descriptions.
    type MaxNameLen = MaxMediaNameLen;
    type MaxDescriptionLen = MaxMediaDescriptionLen;

    // New: maximum roles per account.
    type MaxRolesPerAccount = MaxMediaRolesPerAccount;

    // New: default collection id and owner.
    type DefaultCollectionId = DefaultMediaCollectionId;
    type DefaultCollectionOwner = TreasuryAccount;

}
