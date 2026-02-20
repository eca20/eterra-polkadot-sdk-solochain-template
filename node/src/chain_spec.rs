use frame_support::PalletId;
use sc_service::{ChainType, Properties};
use solochain_eterra_runtime::{AccountId, Signature, WASM_BINARY};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::{sr25519, Pair, Public};
use sp_runtime::traits::AccountIdConversion;
use sp_runtime::traits::{IdentifyAccount, Verify};

// The URL for the telemetry server.
// const STAGING_TELEMETRY_URL: &str = "wss://telemetry.polkadot.io/submit/";

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec;

/// Generate a crypto pair from seed.
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
    TPublic::Pair::from_string(&format!("//{}", seed), None)
        .expect("static values are valid; qed")
        .public()
}

type AccountPublic = <Signature as Verify>::Signer;

/// Generate an account ID from seed.
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
    AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
    AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

/// Generate an Aura authority key.
pub fn authority_keys_from_seed(s: &str) -> (AuraId, GrandpaId) {
    (get_from_seed::<AuraId>(s), get_from_seed::<GrandpaId>(s))
}

// Treasury derived from the same PalletId as in the runtime.
const TREASURY_PALLET_ID: PalletId = PalletId(*b"py/trsry");

fn treasury_account() -> AccountId {
    TREASURY_PALLET_ID.into_account_truncating()
}

fn chain_properties() -> Properties {
    let mut props = Properties::new();
    props.insert("tokenSymbol".into(), "COIN".into());
    props.insert("tokenDecimals".into(), 12.into());
    props
}

pub fn development_config() -> Result<ChainSpec, String> {
    let treasury = treasury_account();

    Ok(ChainSpec::builder(
        WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
        None,
    )
    .with_name("Development")
    .with_id("dev")
    .with_chain_type(ChainType::Development)
    .with_properties(chain_properties())
    .with_genesis_config_patch(testnet_genesis(
        // Initial PoA authorities
        vec![authority_keys_from_seed("Alice")],
        // Sudo account
        Some(get_account_id_from_seed::<sr25519::Public>("Alice")),
        // Pre-funded accounts
        vec![
            get_account_id_from_seed::<sr25519::Public>("Alice"),
            get_account_id_from_seed::<sr25519::Public>("Bob"),
            get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
            get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
            treasury.clone(),
        ],
        true,
        treasury,
        1_000_000_000_000_000u128,
        vec![get_account_id_from_seed::<sr25519::Public>("Alice")],
    ))
    .build())
}

// NOTE:
// We intentionally use `ChainType::Live` and a non-template chain ID ("eterra_testnet").
// Substrate injects a default 127.0.0.1 bootnode when the ID is "local_testnet" or the
// chain type is Local during build-spec -> RAW conversion (unless --disable-default-bootnode).
// Using a unique ID and Live avoids hidden bootnode injection. All bootnodes must now be
// explicitly provided in the human spec (or via CLI), which is what we want for Eterra.
pub fn local_testnet_config() -> Result<ChainSpec, String> {
    let treasury = treasury_account();

    Ok(ChainSpec::builder(
        WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
        None,
    )
    .with_name("Eterra Testnet")
    .with_id("eterra_testnet")
    .with_chain_type(ChainType::Live)
    .with_properties(chain_properties())
    .with_genesis_config_patch(testnet_genesis(
        // Initial PoA authorities
        vec![
            authority_keys_from_seed("Alice"),
            authority_keys_from_seed("Bob"),
        ],
        // Sudo account
        Some(get_account_id_from_seed::<sr25519::Public>("Alice")),
        // Pre-funded accounts
        vec![
            get_account_id_from_seed::<sr25519::Public>("Alice"),
            get_account_id_from_seed::<sr25519::Public>("Bob"),
            get_account_id_from_seed::<sr25519::Public>("Charlie"),
            get_account_id_from_seed::<sr25519::Public>("Dave"),
            get_account_id_from_seed::<sr25519::Public>("Eve"),
            get_account_id_from_seed::<sr25519::Public>("Ferdie"),
            get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
            get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
            get_account_id_from_seed::<sr25519::Public>("Charlie//stash"),
            get_account_id_from_seed::<sr25519::Public>("Dave//stash"),
            get_account_id_from_seed::<sr25519::Public>("Eve//stash"),
            get_account_id_from_seed::<sr25519::Public>("Ferdie//stash"),
            treasury.clone(),
        ],
        true,
        treasury,
        1_000_000_000_000_000u128,
        vec![
            get_account_id_from_seed::<sr25519::Public>("Alice"),
            get_account_id_from_seed::<sr25519::Public>("Bob"),
        ],
    ))
    .build())
}

pub fn production_config() -> Result<ChainSpec, String> {
    let treasury = treasury_account();

    Ok(ChainSpec::builder(
        WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
        None,
    )
    .with_name("Eterra Production")
    .with_id("eterra_production")
    .with_chain_type(ChainType::Live)
    .with_properties(chain_properties())
    .with_genesis_config_patch(testnet_genesis(
        // Replace these with real validator keys in production chainspec JSON.
        vec![
            authority_keys_from_seed("Alice"),
            authority_keys_from_seed("Bob"),
        ],
        // Owner-controlled production baseline. Replace in finalized spec with your cold owner key.
        Some(get_account_id_from_seed::<sr25519::Public>("Alice")),
        vec![
            get_account_id_from_seed::<sr25519::Public>("Alice"),
            get_account_id_from_seed::<sr25519::Public>("Bob"),
            treasury.clone(),
        ],
        true,
        treasury,
        1_000_000_000_000_000u128,
        vec![],
    ))
    .build())
}

/// Configure initial storage state for FRAME modules.
fn testnet_genesis(
    initial_authorities: Vec<(AuraId, GrandpaId)>,
    sudo_key: Option<AccountId>,
    endowed_accounts: Vec<AccountId>,
    _enable_println: bool,
    faucet_account: AccountId,
    payout_amount: u128,
    initial_servers: Vec<AccountId>,
) -> serde_json::Value {
    serde_json::json!({
        "balances": {
            // Configure endowed accounts with initial balance of 1 << 60.
            "balances": endowed_accounts.iter().cloned().map(|k| (k, (1u128 << 60))).collect::<Vec<_>>(),
        },
        "aura": {
            "authorities": initial_authorities.iter().map(|x| (x.0.clone())).collect::<Vec<_>>(),
        },
        "grandpa": {
            "authorities": initial_authorities.iter().map(|x| (x.1.clone(), 1)).collect::<Vec<_>>(),
        },
        "sudo": {
            // Assign network admin rights.
            "key": sudo_key,
        },
        "eterraFaucet": {
            "faucetAccount": faucet_account,
            "payoutAmount": payout_amount,
        },
        "eterraGameAuthority": {
            "initialServers": initial_servers
        }
    })
}

pub fn testnet_config() -> Result<ChainSpec, String> {
    // For compatibility with `--chain testnet`, reuse the eterra_testnet config.
    local_testnet_config()
}

/// Load a chain spec by identifier or from a JSON file path.
pub fn load_spec(id: &str) -> Result<ChainSpec, String> {
    match id {
        // Built-in configs
        "dev" | "development" => development_config(),
        "local" | "local_testnet" => local_testnet_config(),
        "testnet" => testnet_config(),
        "production" | "mainnet" | "eterra_production" => production_config(),
        "eterra_testnet" => local_testnet_config(),
        // Fallback: treat the argument as a path to a JSON chainspec file
        path => ChainSpec::from_json_file(std::path::PathBuf::from(path)),
    }
}
