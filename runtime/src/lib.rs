#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "512"]

#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

pub mod apis;
#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;
pub mod configs;
pub mod signed_extensions;

extern crate alloc;
use alloc::vec::Vec;
use sp_runtime::{
    create_runtime_str, generic, impl_opaque_keys,
    traits::{BlakeTwo256, IdentifyAccount, Verify},
    MultiAddress, MultiSignature,
};
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

pub use frame_system::Call as SystemCall;
pub use pallet_balances::Call as BalancesCall;

pub use pallet_alpha_access;
pub use pallet_cryptostrike;
pub use pallet_eterra;
pub use pallet_eterra_arcade_aegis_run;
pub use pallet_eterra_arcade_core;
pub use pallet_eterra_arcade_nova_rail;
pub use pallet_eterra_arcade_ouro;
pub use pallet_eterra_authority;
pub use pallet_eterra_card_escrow;
pub use pallet_eterra_creatures;
pub use pallet_eterra_daily_slots;
pub use pallet_eterra_economy;
pub use pallet_eterra_faucet;
pub use pallet_eterra_flow;
pub use pallet_eterra_game_authority;
pub use pallet_eterra_game_results;
pub use pallet_eterra_gamer;
pub use pallet_eterra_magic;
pub use pallet_eterra_media;
pub use pallet_eterra_profile;
pub use pallet_eterra_randomness;
pub use pallet_eterra_seasons;
pub use pallet_eterra_simple_matchmaker;
pub use pallet_eterra_tcg;
pub use pallet_nfts;

pub struct HandProviderAdapter;

pub use pallet_timestamp::Call as TimestampCall;

pub use signed_extensions::CheckNonceWithFaucet;

#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;

/// Opaque types. These are used by the CLI to instantiate machinery that don't need to know
/// the specifics of the runtime. They can then be made to be agnostic over specific formats
/// of data like extrinsics, allowing for them to continue syncing the network through upgrades
/// to even the core data structures.
pub mod opaque {
    use super::*;
    use sp_runtime::{
        generic,
        traits::{BlakeTwo256, Hash as HashT},
    };

    pub use sp_runtime::OpaqueExtrinsic as UncheckedExtrinsic;

    /// Opaque block header type.
    pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
    /// Opaque block type.
    pub type Block = generic::Block<Header, UncheckedExtrinsic>;
    /// Opaque block identifier type.
    pub type BlockId = generic::BlockId<Block>;
    /// Opaque block hash type.
    pub type Hash = <BlakeTwo256 as HashT>::Output;
}

impl_opaque_keys! {
    pub struct SessionKeys {
        pub aura: Aura,
        pub grandpa: Grandpa,
    }
}

// To learn more about runtime versioning, see:
// https://docs.substrate.io/main-docs/build/upgrade#runtime-versioning
#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
    spec_name: create_runtime_str!("solochain-eterra-runtime"),
    impl_name: create_runtime_str!("solochain-eterra-runtime"),
    authoring_version: 1,
    // The version of the runtime specification. A full node will not attempt to use its native
    //   runtime in substitute for the on-chain Wasm runtime unless all of `spec_name`,
    //   `spec_version`, and `authoring_version` are the same between Wasm and native.
    // This value is set to 100 to notify Polkadot-JS App (https://polkadot.js.org/apps) to use
    //   the compatible custom types.
    spec_version: 107,
    impl_version: 1,
    apis: apis::RUNTIME_API_VERSIONS,
    transaction_version: 1,
    state_version: 1,
};

mod block_times {
    /// This determines the average expected block time that we are targeting. Blocks will be
    /// produced at a minimum duration defined by `SLOT_DURATION`. `SLOT_DURATION` is picked up by
    /// `pallet_timestamp` which is in turn picked up by `pallet_aura` to implement `fn
    /// slot_duration()`.
    ///
    /// Change this to adjust the block time.
    pub const MILLI_SECS_PER_BLOCK: u64 = 6000;

    // NOTE: Currently it is not possible to change the slot duration after the chain has started.
    // Attempting to do so will brick block production.
    pub const SLOT_DURATION: u64 = MILLI_SECS_PER_BLOCK;
}
pub use block_times::*;

// Time is measured by number of blocks.
pub const MINUTES: BlockNumber = 60_000 / (MILLI_SECS_PER_BLOCK as BlockNumber);
pub const HOURS: BlockNumber = MINUTES * 60;
pub const DAYS: BlockNumber = HOURS * 24;

pub const BLOCK_HASH_COUNT: BlockNumber = 2400;

// Unit = the base number of indivisible units for balances
pub const UNIT: Balance = 1_000_000_000_000;
pub const MILLI_UNIT: Balance = 1_000_000_000;
pub const MICRO_UNIT: Balance = 1_000_000;

/// Existential deposit.
pub const EXISTENTIAL_DEPOSIT: Balance = MILLI_UNIT;

/// The version information used to identify this runtime when compiled natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
    NativeVersion {
        runtime_version: VERSION,
        can_author_with: Default::default(),
    }
}

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// Balance of an account.
pub type Balance = u128;

/// Index of a transaction in the chain.
pub type Nonce = u32;

/// A hash of some data used by the chain.
pub type Hash = sp_core::H256;

/// An index to a block.
pub type BlockNumber = u32;

/// The address format for describing accounts.
pub type Address = MultiAddress<AccountId, ()>;

/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;

/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;

/// A Block signed with a Justification
pub type SignedBlock = generic::SignedBlock<Block>;

/// BlockId type as expected by this runtime.
pub type BlockId = generic::BlockId<Block>;

/// The SignedExtension to the basic transaction logic.
pub type SignedExtra = (
    frame_system::CheckNonZeroSender<Runtime>,
    frame_system::CheckSpecVersion<Runtime>,
    frame_system::CheckTxVersion<Runtime>,
    frame_system::CheckGenesis<Runtime>,
    frame_system::CheckEra<Runtime>,
    CheckNonceWithFaucet,
    frame_system::CheckWeight<Runtime>,
    pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
    frame_metadata_hash_extension::CheckMetadataHash<Runtime>,
);

/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic =
    generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;

/// The payload being signed in transactions.
pub type SignedPayload = generic::SignedPayload<RuntimeCall, SignedExtra>;

/// All migrations of the runtime, aside from the ones declared in the pallets.
///
/// This can be a tuple of types, each implementing `OnRuntimeUpgrade`.
#[allow(unused_parens)]
type Migrations = ();

/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
    Runtime,
    Block,
    frame_system::ChainContext<Runtime>,
    Runtime,
    AllPalletsWithSystem,
    Migrations,
>;

// Create the runtime by composing the FRAME pallets that were previously configured.
#[frame_support::runtime]
mod runtime {
    #[runtime::runtime]
    #[runtime::derive(
        RuntimeCall,
        RuntimeEvent,
        RuntimeError,
        RuntimeOrigin,
        RuntimeFreezeReason,
        RuntimeHoldReason,
        RuntimeSlashReason,
        RuntimeLockId,
        RuntimeTask
    )]
    pub struct Runtime;

    #[runtime::pallet_index(0)]
    pub type System = frame_system;

    #[runtime::pallet_index(1)]
    pub type Timestamp = pallet_timestamp;

    #[runtime::pallet_index(2)]
    pub type Aura = pallet_aura;

    #[runtime::pallet_index(3)]
    pub type Grandpa = pallet_grandpa;

    #[runtime::pallet_index(4)]
    pub type Balances = pallet_balances;

    #[runtime::pallet_index(5)]
    pub type TransactionPayment = pallet_transaction_payment;

    #[runtime::pallet_index(6)]
    pub type Sudo = pallet_sudo;

    #[runtime::pallet_index(7)]
    pub type Council = pallet_collective<Instance1>;

    #[runtime::pallet_index(8)]
    pub type Eterra = pallet_eterra;

    #[runtime::pallet_index(9)]
    pub type EterraTCG = pallet_eterra_tcg;

    #[runtime::pallet_index(10)]
    pub type EterraDailySlots = pallet_eterra_daily_slots;

    #[runtime::pallet_index(12)]
    pub type EterraFaucet = pallet_eterra_faucet;

    #[runtime::pallet_index(13)]
    pub type EterraMonteCarloAi = pallet_eterra_monte_carlo_ai;

    #[runtime::pallet_index(14)]
    pub type EterraSimpleMatchMaker = pallet_eterra_simple_matchmaker;

    #[runtime::pallet_index(15)]
    pub type EterraGamer = pallet_eterra_gamer;

    #[runtime::pallet_index(16)]
    pub type NodeAuthorization = pallet_node_authorization;

    #[runtime::pallet_index(17)]
    pub type EterraGameAuthority = pallet_eterra_game_authority;

    #[runtime::pallet_index(18)]
    pub type EterraMedia = pallet_eterra_media;

    #[runtime::pallet_index(19)]
    pub type CouncilMembership = pallet_membership<Instance1>;

    #[runtime::pallet_index(20)]
    pub type Treasury = pallet_treasury;

    #[runtime::pallet_index(21)]
    pub type Assets = pallet_assets;

    #[runtime::pallet_index(22)]
    pub type EterraSeasons = pallet_eterra_seasons;

    #[runtime::pallet_index(23)]
    pub type Nfts = pallet_nfts;

    #[runtime::pallet_index(24)]
    pub type EterraCardEscrow = pallet_eterra_card_escrow;

    #[runtime::pallet_index(25)]
    pub type AlphaAccess = pallet_alpha_access;

    #[runtime::pallet_index(26)]
    pub type EterraAuthority = pallet_eterra_authority;

    #[runtime::pallet_index(27)]
    pub type EterraEconomy = pallet_eterra_economy;

    #[runtime::pallet_index(28)]
    pub type EterraProfile = pallet_eterra_profile;

    #[runtime::pallet_index(29)]
    pub type EterraFlow = pallet_eterra_flow;

    #[runtime::pallet_index(30)]
    pub type EterraArcadeCore = pallet_eterra_arcade_core;

    #[runtime::pallet_index(31)]
    pub type EterraArcadeOuro = pallet_eterra_arcade_ouro;

    #[runtime::pallet_index(32)]
    pub type EterraArcadeAegisRun = pallet_eterra_arcade_aegis_run;

    #[runtime::pallet_index(33)]
    pub type EterraArcadeNovaRail = pallet_eterra_arcade_nova_rail;

    #[runtime::pallet_index(34)]
    pub type CryptoStrike = pallet_cryptostrike;

    #[runtime::pallet_index(35)]
    pub type EterraRandomness = pallet_eterra_randomness;

    #[runtime::pallet_index(36)]
    pub type EterraCreatures = pallet_eterra_creatures;

    #[runtime::pallet_index(37)]
    pub type EterraMagic = pallet_eterra_magic;

    #[runtime::pallet_index(38)]
    pub type EterraGameResults = pallet_eterra_game_results;

    /// Atomic, bounded administration batches used by the deterministic
    /// private-alpha catalog seeder. Added at a new index; no legacy encoding
    /// moves.
    #[runtime::pallet_index(39)]
    pub type Utility = pallet_utility;
}

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "try-runtime"))]
mod try_runtime_tests {
    use super::*;
    use frame_support::traits::UpgradeCheckSelect;
    use sp_io::TestExternalities;
    use sp_runtime::BuildStorage;

    #[test]
    fn try_runtime_upgrade_executes_on_genesis_state() {
        let storage = RuntimeGenesisConfig::default()
            .build_storage()
            .expect("runtime genesis storage should build");
        let mut ext = TestExternalities::new(storage);

        ext.execute_with(|| {
            System::set_block_number(1);
            Executive::try_runtime_upgrade(UpgradeCheckSelect::PreAndPost)
                .expect("try-runtime on_runtime_upgrade should succeed on genesis state");
        });
    }
}
