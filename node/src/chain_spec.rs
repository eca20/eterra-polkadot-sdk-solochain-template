use frame_support::PalletId;
use sc_service::{ChainType, Properties};
use solochain_eterra_runtime::{AccountId, Signature, UNIT, WASM_BINARY};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::crypto::Ss58Codec;
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
    let extra_season_admin =
        AccountId::from_ss58check("5CS7vvxam6GJrEWtQsYenccZVz7BDX2hqTcgJQEcBrDcF4hV")
            .expect("hard-coded ss58 address is valid");
    let council_members = vec![get_account_id_from_seed::<sr25519::Public>("Alice")];
    let mut season_admins = council_members.clone();
    season_admins.push(extra_season_admin.clone());
    season_admins.sort();
    season_admins.dedup();

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
            extra_season_admin,
        ],
        true,
        treasury,
        1_000_000_000_000_000u128,
        vec![get_account_id_from_seed::<sr25519::Public>("Alice")],
        council_members,
        season_admins,
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
    let extra_season_admin =
        AccountId::from_ss58check("5CS7vvxam6GJrEWtQsYenccZVz7BDX2hqTcgJQEcBrDcF4hV")
            .expect("hard-coded ss58 address is valid");
    let council_members = vec![
        get_account_id_from_seed::<sr25519::Public>("Alice"),
        get_account_id_from_seed::<sr25519::Public>("Bob"),
    ];
    let mut season_admins = council_members.clone();
    season_admins.push(extra_season_admin.clone());
    season_admins.sort();
    season_admins.dedup();

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
            extra_season_admin,
        ],
        true,
        treasury,
        1_000_000_000_000_000u128,
        vec![
            get_account_id_from_seed::<sr25519::Public>("Alice"),
            get_account_id_from_seed::<sr25519::Public>("Bob"),
        ],
        council_members,
        season_admins,
    ))
    .build())
}

pub fn production_config() -> Result<ChainSpec, String> {
    let treasury = treasury_account();
    let council_members = vec![
        get_account_id_from_seed::<sr25519::Public>("Alice"),
        get_account_id_from_seed::<sr25519::Public>("Bob"),
    ];
    let season_admins = council_members.clone();

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
        council_members,
        season_admins,
    ))
    .build())
}

pub fn alpha_config() -> Result<ChainSpec, String> {
    let treasury = treasury_account();
    let owner = get_account_id_from_seed::<sr25519::Public>("AlphaOwner");
    let validator = get_account_id_from_seed::<sr25519::Public>("AlphaValidator");
    let media_signer = get_account_id_from_seed::<sr25519::Public>("AlphaMediaSigner");
    let hot_admin = AccountId::from_ss58check("5Dq5eLhbKhUpzcuwbsFYisiWnRQkXonTt2RTmvaiVsFwkUsY")
        .expect("hard-coded ss58 address is valid");
    let council_members = vec![owner.clone()];
    let mut season_admins = vec![hot_admin.clone(), media_signer.clone()];
    season_admins.sort();
    season_admins.dedup();

    Ok(ChainSpec::builder(
        WASM_BINARY.ok_or_else(|| "Alpha wasm not available".to_string())?,
        None,
    )
    .with_name("Eterra Alpha")
    .with_id("eterra_alpha")
    .with_chain_type(ChainType::Live)
    .with_properties(chain_properties())
    .with_genesis_config_patch(testnet_genesis(
        vec![authority_keys_from_seed("AlphaValidator")],
        Some(owner.clone()),
        vec![
            owner.clone(),
            validator,
            media_signer,
            treasury.clone(),
            hot_admin,
        ],
        true,
        owner,
        1_000_000_000_000_000u128,
        vec![],
        council_members,
        season_admins,
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
    council_members: Vec<AccountId>,
    season_admins: Vec<AccountId>,
) -> serde_json::Value {
    // Initialize the Treasury account with 200 million COIN (12 decimals via UNIT = 1e12).
    let treasury_endowment: u128 = 200_000_000u128.saturating_mul(UNIT);
    let treasury_account = treasury_account();

    // Multi-currency fungible assets via `pallet-assets`.
    // Use the sudo key as the initial owner so it can mint/burn/transfer without Root calls.
    let asset_owner: AccountId = sudo_key.clone().unwrap_or_else(|| treasury_account.clone());
    let dev_coin_id: u32 = 1;
    let beta_coin_id: u32 = 2;
    // Give the initial owner a large supply of each asset for testing/distribution.
    let initial_asset_supply: u128 = 1_000_000_000u128.saturating_mul(UNIT);
    // `pallet-assets` requires a non-zero minimum balance.
    let asset_min_balance: u128 = 1;

    // Default owner of the genesis media collection. Prefer sudo if present (so the key exists),
    // otherwise fall back to the first council member.
    let default_media_owner: AccountId = sudo_key
        .clone()
        .or_else(|| council_members.first().cloned())
        .unwrap_or_else(|| treasury_account.clone());

    // Season-admin allowlist: include the media service signer (and any ops keys).
    // In production, set this to your actual service key(s) in the finalized chain spec JSON.

    serde_json::json!({
        "balances": {
            // Configure endowed accounts with initial balance of 1 << 60, except the Treasury.
            "balances": endowed_accounts.iter().cloned().map(|k| {
                let amount = if k == treasury_account { treasury_endowment } else { 1u128 << 60 };
                (k, amount)
            }).collect::<Vec<_>>(),
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
        "councilMembership": {
            // `pallet-membership` drives council membership and initializes the collective.
            "members": council_members,
        },
        "assets": {
            // Genesis assets: (id, owner, is_sufficient, min_balance)
            "assets": [
                [dev_coin_id, asset_owner, false, asset_min_balance],
                [beta_coin_id, asset_owner, false, asset_min_balance],
            ],
            // Genesis metadata: (id, name, symbol, decimals)
            "metadata": [
                [dev_coin_id, b"devCOIN".to_vec(), b"devCOIN".to_vec(), 12],
                [beta_coin_id, b"betaCOIN".to_vec(), b"betaCOIN".to_vec(), 12],
            ],
            // Genesis balances: (id, account, amount)
            "accounts": [
                [dev_coin_id, asset_owner, initial_asset_supply],
                [beta_coin_id, asset_owner, initial_asset_supply],
            ],
            // Reserve ids 1 and 2; future assets should start at 3.
            "nextAssetId": 3,
        },
        "eterraFaucet": {
            "faucetAccount": faucet_account,
            "payoutAmount": payout_amount,
        },
        "eterraGameAuthority": {
            "initialServers": initial_servers
        },
        "eterraArcadeCore": {
            "initialGameConfigs": [
                [
                    1003,
                    b"nova_rail".to_vec(),
                    true,
                    1,
                    1000,
                    1,
                    25,
                    100,
                    1_000_000,
                    3
                ]
            ]
        },
        "eterraMedia": {
            // Ensure the runtime default collection exists so callers can omit a collection id
            // when registering media (the pallet falls back to `DefaultCollectionId`).
            "createDefaultCollection": true,
            "defaultCollectionName": b"TCG Art".to_vec(),
            "defaultCollectionDescription": b"Default media collection for seasonal card layers".to_vec(),
            "defaultCollectionOwner": default_media_owner
        },
        "eterraSeasons": {
            "admins": season_admins,
            // Seed a draft season at genesis so admins can upload assets before activation.
            "initialDraftSeason": [b"Season 1".to_vec(), b"Genesis Season 1".to_vec()]
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
        // Some CLI paths pass an empty chain identifier when `--chain` is omitted.
        // Treat it as `dev` instead of trying to open an empty filename.
        "" => development_config(),
        "dev" | "development" => development_config(),
        "alpha" | "eterra_alpha" => alpha_config(),
        "local" | "local_testnet" => local_testnet_config(),
        "testnet" => testnet_config(),
        "production" | "mainnet" | "eterra_production" => production_config(),
        "eterra_testnet" => local_testnet_config(),
        // Fallback: treat the argument as a path to a JSON chainspec file
        path => ChainSpec::from_json_file(std::path::PathBuf::from(path)),
    }
}
