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
use frame_support::PalletId;
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
use scale_info::TypeInfo;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_runtime::{
    traits::{AccountIdConversion, Morph, One},
    Perbill, Permill,
};
use sp_version::RuntimeVersion;

// Bring in UNIT and HandProviderAdapter from the parent module (lib.rs)
use super::{HandProviderAdapter, UNIT};

// Bring in the pallets re-exported in lib.rs
use super::{
    pallet_eterra, pallet_eterra_daily_slots, pallet_eterra_faucet, pallet_eterra_game_authority,
    pallet_eterra_gamer, pallet_eterra_media, pallet_eterra_seasons, pallet_eterra_simple_matchmaker,
    pallet_eterra_tcg, pallet_nfts,
};
// Monte Carlo AI pallet lives at the crate root; bring it in explicitly.

// Local module imports
use super::{
    AccountId, Assets, Aura, Balance, Balances, Block, BlockNumber, Council, EterraGamer, Hash,
    Nonce, PalletInfo, Runtime, RuntimeCall, RuntimeEvent, RuntimeFreezeReason, RuntimeHoldReason,
    RuntimeOrigin, RuntimeTask, Signature, System, DAYS, EXISTENTIAL_DEPOSIT, HOURS, SLOT_DURATION,
    VERSION,
};

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

// Runtime privileged-origin policy:
// centralized owner-control in both default and production modes.
// This alias can be switched to governance origins when governance is introduced.
type PrivilegedControlOrigin = frame_system::EnsureRoot<AccountId>;

pub struct TcgHandChecker;

impl pallet_eterra_tcg::HandChecker<AccountId> for TcgHandChecker {
    fn is_card_in_current_hand(owner: &AccountId, card_id: u32) -> bool {
        pallet_eterra::CurrentHandOf::<Runtime>::get(owner)
            .map(|hand| hand.iter().any(|&id| id == card_id))
            .unwrap_or(false)
    }
}

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

// --- Assets (multi-currency fungibles) ---
parameter_types! {
    // Keep these low while we are iterating; increase for production if needed.
    pub const AssetDeposit: Balance = 0;
    pub const AssetAccountDeposit: Balance = EXISTENTIAL_DEPOSIT;
    pub const MetadataDepositBase: Balance = 0;
    pub const MetadataDepositPerByte: Balance = 0;
    pub const ApprovalDeposit: Balance = 0;
    pub const AssetsStringLimit: u32 = 64;
}

pub struct RootToAssetOwner;
impl Morph<()> for RootToAssetOwner {
    type Outcome = AccountId;
    fn morph(_: ()) -> AccountId {
        // Root has no inherent account id. We return a fixed owner for the `create` origin
        // and rely on `force_create`/`set_team` for explicit ownership assignment.
        TreasuryAccount::get()
    }
}

type RootAsAssetOwner =
    frame_support::traits::MapSuccess<PrivilegedControlOrigin, RootToAssetOwner>;
type AssetsCreateOrigin = frame_support::traits::AsEnsureOriginWithArg<RootAsAssetOwner>;

impl pallet_assets::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Balance = Balance;
    type AssetId = u32;
    type AssetIdParameter = u32;
    type Currency = Balances;
    type CreateOrigin = AssetsCreateOrigin;
    type ForceOrigin = PrivilegedControlOrigin;
    type AssetDeposit = AssetDeposit;
    type AssetAccountDeposit = AssetAccountDeposit;
    type MetadataDepositBase = MetadataDepositBase;
    type MetadataDepositPerByte = MetadataDepositPerByte;
    type ApprovalDeposit = ApprovalDeposit;
    type StringLimit = AssetsStringLimit;
    type Freezer = ();
    type Extra = ();
    type CallbackHandle = ();
    type WeightInfo = pallet_assets::weights::SubstrateWeight<Runtime>;
    type RemoveItemsLimit = ConstU32<1_000>;

    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = ();
}

parameter_types! {
    pub FeeMultiplier: Multiplier = Multiplier::one();
}

// --- Make faucet.claim feeless by customizing OnChargeTransaction ---
use pallet_transaction_payment::OnChargeTransaction;
#[cfg(feature = "runtime-production")]
use sp_runtime::traits::Zero;
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
        fee: Self::Balance,
        tip: Self::Balance,
    ) -> Result<Self::LiquidityInfo, TransactionValidityError> {
        #[cfg(not(feature = "runtime-production"))]
        {
            // Dev/test mode: sponsor all self-claims for fast onboarding/testing.
            if let RuntimeCall::EterraFaucet(pallet_eterra_faucet::Call::claim { dest }) = call {
                if dest == who {
                    System::deposit_event(RuntimeEvent::EterraFaucet(
                        pallet_eterra_faucet::Event::<Runtime>::FeeSponsorshipApplied {
                            who: who.clone(),
                        },
                    ));
                    return Ok(Default::default());
                }
            }
        }

        #[cfg(feature = "runtime-production")]
        {
            // Production mode: sponsor only capped self-claims where signer cannot
            // afford normal fee withdrawal, and only with zero tip.
            if let RuntimeCall::EterraFaucet(pallet_eterra_faucet::Call::claim { dest }) = call {
                let now = System::block_number();
                let sponsored_ok = pallet_eterra_faucet::Pallet::<Runtime>::can_receive_sponsored_claim_pre_dispatch(who, now, fee);
                if dest == who && sponsored_ok && tip.is_zero() {
                    System::deposit_event(RuntimeEvent::EterraFaucet(
                        pallet_eterra_faucet::Event::<Runtime>::FeeSponsorshipApplied {
                            who: who.clone(),
                        },
                    ));
                    return Ok(Default::default());
                }
            }
        }

        // Otherwise delegate to the default adapter.
        <pallet_transaction_payment::FungibleAdapter<Balances, ()> as OnChargeTransaction<
            Runtime,
        >>::withdraw_fee(who, call, info, fee, tip)
    }

    fn correct_and_deposit_fee(
        who: &AccountId,
        info: &DispatchInfoOf<RuntimeCall>,
        post_info: &PostDispatchInfoOf<RuntimeCall>,
        corrected_fee: Self::Balance,
        tip: Self::Balance,
        paid: Self::LiquidityInfo,
    ) -> Result<(), TransactionValidityError> {
        // If we skipped withdrawing (paid is default/None), do nothing on deposit.
        if paid == Default::default() {
            return Ok(());
        }
        // Otherwise delegate to the default adapter.
        <pallet_transaction_payment::FungibleAdapter<Balances, ()> as OnChargeTransaction<
            Runtime,
        >>::correct_and_deposit_fee(who, info, post_info, corrected_fee, tip, paid)
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

// --- Governance (Council) ---
parameter_types! {
    // ~24h at 6s blocks.
    pub const CouncilMotionDuration: BlockNumber = 14_400;
    pub const CouncilMaxProposals: u32 = 100;
    pub const CouncilMaxMembers: u32 = 7;

    // Allow dispatching up to 1s of weight via council motions.
    pub const CouncilMaxProposalWeight: Weight =
        Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND, u64::MAX);
}

type CouncilCollective = pallet_collective::Instance1;

impl pallet_collective::Config<CouncilCollective> for Runtime {
    type RuntimeOrigin = RuntimeOrigin;
    type Proposal = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type MotionDuration = CouncilMotionDuration;
    type MaxProposals = CouncilMaxProposals;
    type MaxMembers = CouncilMaxMembers;
    type DefaultVote = pallet_collective::PrimeDefaultVote;
    type WeightInfo = pallet_collective::weights::SubstrateWeight<Runtime>;
    type SetMembersOrigin = PrivilegedControlOrigin;
    type MaxProposalWeight = CouncilMaxProposalWeight;
}

type CouncilMembershipInstance = pallet_membership::Instance1;

impl pallet_membership::Config<CouncilMembershipInstance> for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type AddOrigin = PrivilegedControlOrigin;
    type RemoveOrigin = PrivilegedControlOrigin;
    type SwapOrigin = PrivilegedControlOrigin;
    type ResetOrigin = PrivilegedControlOrigin;
    type PrimeOrigin = PrivilegedControlOrigin;
    type MembershipInitialized = Council;
    type MembershipChanged = Council;
    type MaxMembers = CouncilMaxMembers;
    type WeightInfo = pallet_membership::weights::SubstrateWeight<Runtime>;
}

// --- Treasury ---
parameter_types! {
    // ~24h at 6s blocks.
    pub const TreasurySpendPeriod: BlockNumber = 14_400;
    // How long an approved spend can be claimed for.
    pub const TreasuryPayoutPeriod: BlockNumber = 14_400;
    pub const TreasuryBurn: Permill = Permill::from_percent(0);
    pub const TreasuryMaxApprovals: u32 = 100;
}

pub struct MaxTreasurySpend;
impl Morph<()> for MaxTreasurySpend {
    type Outcome = Balance;
    fn morph(_: ()) -> Balance {
        Balance::MAX
    }
}

type CouncilMajorityOrigin =
    pallet_collective::EnsureProportionAtLeast<AccountId, CouncilCollective, 1, 2>;

type TreasuryApproveRejectOrigin =
    frame_support::traits::EitherOfDiverse<PrivilegedControlOrigin, CouncilMajorityOrigin>;

type RootAsMaxSpend = frame_support::traits::MapSuccess<PrivilegedControlOrigin, MaxTreasurySpend>;
type CouncilAsMaxSpend = frame_support::traits::MapSuccess<CouncilMajorityOrigin, MaxTreasurySpend>;
type TreasurySpendOrigin = frame_support::traits::EitherOf<RootAsMaxSpend, CouncilAsMaxSpend>;

#[cfg(feature = "runtime-benchmarks")]
pub struct TreasuryBenchHelper;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_treasury::ArgumentsFactory<(), AccountId> for TreasuryBenchHelper {
    fn create_asset_kind(_: u32) -> () {
        ()
    }

    fn create_beneficiary(seed: [u8; 32]) -> AccountId {
        AccountId::from(seed)
    }
}

impl pallet_treasury::Config for Runtime {
    type Currency = Balances;
    type RejectOrigin = TreasuryApproveRejectOrigin;
    type RuntimeEvent = RuntimeEvent;
    type SpendPeriod = TreasurySpendPeriod;
    type Burn = TreasuryBurn;
    type PalletId = TreasuryPalletId;
    type BurnDestination = ();
    type WeightInfo = pallet_treasury::weights::SubstrateWeight<Runtime>;
    type SpendFunds = ();
    type MaxApprovals = TreasuryMaxApprovals;
    type SpendOrigin = TreasurySpendOrigin;

    type AssetKind = ();
    type Beneficiary = AccountId;
    type BeneficiaryLookup = <Runtime as frame_system::Config>::Lookup;
    type Paymaster = frame_support::traits::tokens::PayFromAccount<Balances, TreasuryAccount>;
    type BalanceConverter = frame_support::traits::tokens::UnityAssetBalanceConversion;
    type PayoutPeriod = TreasuryPayoutPeriod;

    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = TreasuryBenchHelper;
}

impl pallet_node_authorization::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type MaxWellKnownNodes = MaxWellKnownNodes;
    type MaxPeerIdLength = MaxPeerIdLength;

    type AddOrigin = PrivilegedControlOrigin;
    type RemoveOrigin = PrivilegedControlOrigin;
    type SwapOrigin = PrivilegedControlOrigin;
    type ResetOrigin = PrivilegedControlOrigin;

    type WeightInfo = ();
}

#[derive(Clone, TypeInfo)]
pub struct EterraNumPlayers;
impl frame_support::traits::Get<u32> for EterraNumPlayers {
    fn get() -> u32 {
        2
    }
}

parameter_types! {
    pub const EterraMaxRounds: u8 = 5;
    // The limit in blocks each player has until their turn is force finished.
    pub const EterraBlocksToPlayLimit: u8 = 6;
    // AI controller window lengths (blocks).
    pub const EterraBlocksPerHour: BlockNumber = HOURS;
    pub const EterraBlocksPerDay: BlockNumber = DAYS;
    pub const EterraBlocksPerWeek: BlockNumber = DAYS.saturating_mul(7);
    pub const EterraBlocksPerMonth: BlockNumber = DAYS.saturating_mul(30);
    // Gridlock: lock 1..=5 random cells at game start.
    pub const EterraGridlockMinLocks: u8 = 1;
    pub const EterraGridlockMaxLocks: u8 = 5;
    pub const MaxSlotLength: u32 = 3;
    pub const MaxOptionsPerSlot: u32 = 10;
    pub const MaxRollsPerRound: u32 = 3;
    pub const MaxRollHistoryLength: u32 = 100;
    pub const MaxWeightEntries: u32 = 100;
    pub const MaxDrawingEntries: u32 = 1_000;

    // 6 seconds per block → ~30 blocks for ~3 minutes
    pub const MaxExpirationsPerBlock: u32 = 256; // tune as needed
    pub const MaxPlayersPerGameConst: u32 = 128; // tune as needed
    pub const MaxWellKnownNodes: u32 = 128;   // adjust as you like
    pub const MaxPeerIdLength: u32 = 128;     // libp2p PeerId length upper bound
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
    pub RewardPerWinAmount: Balance = 100 * UNIT;

    // `pallet-assets` ids for additional fungible currencies.
    pub const DevCoinAssetId: u32 = 1;
    pub const BetaCoinAssetId: u32 = 2;

    // Per-win rewards for the Eterra game.
    pub EterraWinRewardCoin: Balance = 10 * UNIT;
    pub EterraWinRewardDevCoin: Balance = 100 * UNIT;
    pub EterraWinRewardBetaCoin: Balance = 100 * UNIT;
    pub const EterraWinRewardExperience: u128 = 100;
}

#[cfg(not(feature = "runtime-production"))]
parameter_types! {
    // Dev/test defaults: no cooldown and generous sponsorship for rapid iteration.
    pub const FaucetClaimCooldownBlocks: BlockNumber = 0;
    pub const FaucetSponsoredClaimMaxCount: u32 = 10_000;
    pub const FaucetSponsoredClaimWindowBlocks: BlockNumber = 432_000; // ~30 days
}

#[cfg(feature = "runtime-production")]
parameter_types! {
    // Production defaults: conservative anti-abuse limits.
    pub const FaucetClaimCooldownBlocks: BlockNumber = 14_400; // ~24h at 6s block time
    pub const FaucetSponsoredClaimMaxCount: u32 = 3;
    pub const FaucetSponsoredClaimWindowBlocks: BlockNumber = 432_000; // ~30 days
}

impl pallet_eterra_faucet::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type ClaimCooldownBlocks = FaucetClaimCooldownBlocks;
    type SponsoredClaimMaxCount = FaucetSponsoredClaimMaxCount;
    type SponsoredClaimWindowBlocks = FaucetSponsoredClaimWindowBlocks;
    type WeightInfo = pallet_eterra_faucet::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    pub const GamerTagMaxLen: u32 = 32;
    pub const AvatarCidMaxLen: u32 = 96; // or 128
    pub const GamerChangeFee: Balance = 100 * UNIT;
}
impl pallet_eterra_gamer::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type ExpIssuerOrigin = PrivilegedControlOrigin;
    type FaucetAccount = TreasuryAccount;
    type ChangeFee = GamerChangeFee;
    type MaxTagLen = GamerTagMaxLen;
    type MaxAvatarCidLen = AvatarCidMaxLen;
    type WeightInfo = pallet_eterra_gamer::weights::SubstrateWeight<Runtime>;
}

#[cfg(feature = "runtime-benchmarks")]
pub struct MonteCarloBenchHelper;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_eterra_monte_carlo_ai::BenchmarkHelper<eterra_card_ai_adapter::eterra_adapter::Adapter>
    for MonteCarloBenchHelper
{
    fn bench_state() -> eterra_card_ai_adapter::eterra_adapter::State {
        let mut s = eterra_card_ai_adapter::eterra_adapter::State::default();
        s.max_rounds = 1;
        s.round = 0;
        s.player_turn = 0;
        s
    }
}

impl pallet_eterra_monte_carlo_ai::pallet::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Adapter = eterra_card_ai_adapter::eterra_adapter::Adapter;
    // Limits & tuning params for Monte Carlo search
    type MaxActions = ConstU32<64>; // max legal moves enumerated
    type BaseIterations = ConstU32<200>; // baseline simulations per suggest() call
    type MaxPlayoutDepth = ConstU16<16>; // cut off long playouts
    type WeightInfo = pallet_eterra_monte_carlo_ai::weights::SubstrateWeight<Runtime>;

    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = MonteCarloBenchHelper;
}

impl pallet_eterra_game_authority::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type MaxPlayersPerGame = MaxPlayersPerGameConst;
    type AdminOrigin = PrivilegedControlOrigin;
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

impl pallet_eterra_simple_matchmaker::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type PlayersPerMatch = PlayersPerMatchConst;
    type QueueCapacity = QueueCapacityConst;
    type HandProvider = MatchmakerHandProvider;
    type GameCreator = pallet_eterra::Pallet<Runtime>;
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

    type PaymentCurrency = Balances;
    type HandChecker = TcgHandChecker;
    type PackPrice = ConstU128<{ 500 * UNIT }>;
    type PackPriceReceiver = TreasuryAccount;
    type ProPrice = ConstU128<{ 200 * UNIT }>;
    type ProPriceReceiver = TreasuryAccount;
    type MintCardPrice = ConstU128<{ 100 * UNIT }>;
    type MintCardPriceReceiver = TreasuryAccount;
    type MaxProSpins = ConstU8<5>;
    type MaxAttempts = ConstU8<3>; // Set maximum attempts per card to 3
    type CardsPerPack = ConstU8<6>; // Set number of cards per pack to 6
    type MaxOwnedCards = ConstU32<5000>;
    type BaseCardCapacity = ConstU32<500>;
    type CardCapacityUpgradeAmount = ConstU32<100>;
    type CardCapacityUpgradePrice = ConstU128<{ 100 * UNIT }>;
    type CardCapacityUpgradePriceReceiver = TreasuryAccount;
    type MaxBorders = ConstU32<32>;
    type MaxBackgrounds = ConstU32<32>;
    type MaxSubjects = ConstU32<128>;
    type WeightInfo = pallet_eterra_tcg::weights::SubstrateWeight<Runtime>;
}

impl pallet_eterra::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type NumPlayers = EterraNumPlayers;
    type MaxRounds = EterraMaxRounds;
    type BlocksToPlayLimit = EterraBlocksToPlayLimit;
    type HandSize = ConstU32<5>; // <<—— added
    type AiAccount = AiBotAccountParam;
    // Start low by default; the on-chain controller can raise/lower this over time.
    type AiDifficulty = ConstU8<20>;
    type AdminOrigin = PrivilegedControlOrigin;
    type BlocksPerHour = EterraBlocksPerHour;
    type BlocksPerDay = EterraBlocksPerDay;
    type BlocksPerWeek = EterraBlocksPerWeek;
    type BlocksPerMonth = EterraBlocksPerMonth;
    type GridlockMinLocks = EterraGridlockMinLocks;
    type GridlockMaxLocks = EterraGridlockMaxLocks;
    type Assets = Assets;
    type ExperienceManager = EterraGamer;
    type DevCoinAssetId = DevCoinAssetId;
    type BetaCoinAssetId = BetaCoinAssetId;
    type WinRewardCoin = EterraWinRewardCoin;
    type WinRewardDevCoin = EterraWinRewardDevCoin;
    type WinRewardBetaCoin = EterraWinRewardBetaCoin;
    type WinRewardExperience = EterraWinRewardExperience;
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
    type WeightInfo = pallet_eterra_media::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    pub const MaxSeasonNameLen: u32 = 64;
    pub const MaxSeasonDescLen: u32 = 256;
}

impl pallet_eterra_seasons::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type AdminOrigin = PrivilegedControlOrigin;
    type MaxSeasonNameLen = MaxSeasonNameLen;
    type MaxSeasonDescLen = MaxSeasonDescLen;
    type WeightInfo = pallet_eterra_seasons::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    pub NftsFeatures: pallet_nfts::PalletFeatures = pallet_nfts::PalletFeatures::all_enabled();
}

impl pallet_nfts::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;

    type CollectionId = u32;
    type ItemId = u32;

    type Currency = Balances;
    type ForceOrigin = PrivilegedControlOrigin;
    type CreateOrigin = frame_support::traits::AsEnsureOriginWithArg<frame_system::EnsureSigned<AccountId>>;
    type Locker = ();

    type CollectionDeposit = ConstU128<0>;
    type ItemDeposit = ConstU128<0>;
    type MetadataDepositBase = ConstU128<0>;
    type AttributeDepositBase = ConstU128<0>;
    type DepositPerByte = ConstU128<0>;

    type StringLimit = ConstU32<256>;
    type KeyLimit = ConstU32<64>;
    type ValueLimit = ConstU32<256>;

    type ApprovalsLimit = ConstU32<20>;
    type ItemAttributesApprovalsLimit = ConstU32<20>;
    type MaxTips = ConstU32<10>;
    type MaxDeadlineDuration = ConstU32<100_000>;
    type MaxAttributesPerCall = ConstU32<10>;

    type Features = NftsFeatures;

    type OffchainSignature = Signature;
    type OffchainPublic = <Signature as sp_runtime::traits::Verify>::Signer;

    #[cfg(feature = "runtime-benchmarks")]
    type Helper = ();

    type WeightInfo = pallet_nfts::weights::SubstrateWeight<Runtime>;
}
