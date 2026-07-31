use clap::Parser;
use frame_remote_externalities::{Builder, Mode, OfflineConfig, SnapshotConfig};
use frame_support::{
    dispatch::{DispatchResult, RawOrigin},
    storage::{with_transaction, TransactionOutcome},
    traits::{Hooks, StorageVersion},
    weights::Weight,
};
use pallet_eterra_tcg::{
    CardInfo, LegacyCardClassification, LegacyCustodyKind, MigrationPhaseV16, NexusStorageLocation,
    V2Feature,
};
use parity_scale_codec::Encode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use solochain_eterra_runtime::{
    configs::RuntimeBlockWeights, AccountId, AlphaAccess, Block, BlockNumber, Eterra,
    EterraCardEscrow, EterraTCG, Nfts, Runtime, RuntimeOrigin, System,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};

type Hash32 = [u8; 32];

const ATTESTATION_DOMAIN: &[u8] = b"ETERRA_TCG_V16_COPIED_STATE_ATTESTATION_V3";
const DOMAIN_DIGEST_PREFIX: &[u8] = b"ETERRA_TCG_V16_CANONICAL_DOMAIN_SHA256_V3";

#[derive(Debug, Parser)]
#[command(
    name = "nexus-v2-migration-verifier",
    about = "Verify the bounded Nexus V14/V15 to V16 migration against an offline snapshot"
)]
struct Args {
    #[arg(long)]
    manifest: PathBuf,
    #[arg(long)]
    snapshot: PathBuf,
    #[arg(long)]
    runtime_wasm: PathBuf,
    #[arg(long)]
    try_runtime_log: PathBuf,
    #[arg(long)]
    result: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupManifest {
    release_id: String,
    source_commit: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MigrationChecks {
    interrupted_resume_safe: bool,
    no_card_lost: bool,
    no_card_duplicated: bool,
    no_silent_reclassification: bool,
    ownership_indexes_match: bool,
    subject_indexes_match: bool,
    custody_domains_match: bool,
    lifecycle_quiescent: bool,
    retired_economies_quiescent: bool,
    v2_sidecar_prefixes_absent: bool,
    anomalies_accounted: bool,
    next_card_id_monotonic: bool,
    safe_legacy_exits_preserved: bool,
    v2_writes_remain_paused: bool,
    bounded_batch_weight_respected: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MigrationCounts {
    legacy_cards_before: u64,
    legacy_cards_after: u64,
    cards_seen: u32,
    ordinary: u32,
    nft_wrapped: u32,
    known_escrow: u32,
    anomalies: u32,
    next_card_id: u32,
    max_card_id_seen: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct V2Features {
    #[serde(rename = "Packs")]
    packs: bool,
    #[serde(rename = "Conversion")]
    conversion: bool,
    #[serde(rename = "Ranked")]
    ranked: bool,
    #[serde(rename = "MythicalAscension")]
    mythical_ascension: bool,
}

impl V2Features {
    fn all_disabled(&self) -> bool {
        !self.packs && !self.conversion && !self.ranked && !self.mythical_ascension
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum SafeExitStatus {
    Passed,
    NotPresent,
    Blocked,
    Failed,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SafeExitEvidence {
    path: &'static str,
    status: SafeExitStatus,
    candidate_card_id: Option<u32>,
    detail: String,
}

#[derive(Clone, Copy, Debug, Encode, PartialEq, Eq)]
struct DomainEvidence {
    count: u64,
    sha256: Hash32,
}

#[derive(Clone, Debug, Encode, PartialEq, Eq)]
struct AttestationEvidence {
    schema_version: u32,
    from_storage_version: u16,
    to_storage_version: u16,
    upper_bound: u32,
    next_card_id: u32,
    cards_seen: u32,
    ordinary: u32,
    nft_wrapped: u32,
    known_escrow: u32,
    anomalies: u32,
    max_card_id_seen: Option<u32>,
    next_vault_variant_id: u32,
    cards: DomainEvidence,
    legacy_owner_indexes: DomainEvidence,
    nexus_cards: DomainEvidence,
    vault_variants: DomainEvidence,
    nexus_subject_indexes: DomainEvidence,
    overflow_owner_indexes: DomainEvidence,
    overflow_subject_indexes: DomainEvidence,
    nft_wrapped_cards: DomainEvidence,
    nft_collection: DomainEvidence,
    nft_collection_config: DomainEvidence,
    nft_items: DomainEvidence,
    nft_item_configs: DomainEvidence,
    nft_account_index: DomainEvidence,
    external_escrow_entries: DomainEvidence,
    external_escrow_owner_indexes: DomainEvidence,
    external_escrow_available_count: DomainEvidence,
    external_escrow_available_by_index: DomainEvidence,
    external_escrow_index_by_card: DomainEvidence,
    external_escrow_game_assignments: DomainEvidence,
    game_authority_games: DomainEvidence,
    game_authority_active_by_player: DomainEvidence,
    game_authority_expirations: DomainEvidence,
    cryptostrike_pending_claims: DomainEvidence,
    cryptostrike_servers: DomainEvidence,
    cryptostrike_pending_unstakes: DomainEvidence,
    cryptostrike_allowances: DomainEvidence,
    cryptostrike_session_rosters: DomainEvidence,
    cryptostrike_active_players: DomainEvidence,
    preexisting_v2_sidecar_records: DomainEvidence,
    classifications: DomainEvidence,
    anomaly_records: DomainEvidence,
    repaired_owner_indexes: DomainEvidence,
    repaired_subject_indexes: DomainEvidence,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DomainEvidenceReport {
    count: u64,
    sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AttestationReport {
    algorithm: &'static str,
    evidence_encoding: &'static str,
    verification_hash: String,
    upper_bound: u32,
    next_card_id: u32,
    cards_seen: u32,
    next_vault_variant_id: u32,
    domains: BTreeMap<&'static str, DomainEvidenceReport>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LegacyStateEvidence {
    full_card_map_within_upper_bound: bool,
    cards_preserved: bool,
    legacy_owner_indexes_preserved: bool,
    nexus_records_preserved: bool,
    vault_records_preserved: bool,
    nexus_subject_indexes_preserved: bool,
    overflow_owner_indexes_preserved: bool,
    overflow_subject_indexes_preserved: bool,
    nft_wrapping_records_preserved: bool,
    nft_auxiliary_records_preserved: bool,
    external_escrow_records_preserved: bool,
    external_escrow_indexes_preserved: bool,
    external_escrow_assignments_preserved: bool,
    game_authority_records_preserved: bool,
    retired_economy_records_preserved: bool,
    lifecycle_quiescent: bool,
    retired_economies_quiescent: bool,
    v2_sidecar_prefixes_absent: bool,
    legacy_owner_indexes_consistent: bool,
    nft_wrapping_domain_consistent: bool,
    external_escrow_domain_consistent: bool,
    nexus_subject_indexes_consistent: bool,
    overflow_owner_indexes_consistent: bool,
    overflow_subject_indexes_consistent: bool,
    vault_links_consistent: bool,
    next_vault_variant_id_monotonic: bool,
}

impl LegacyStateEvidence {
    fn all_passed(&self) -> bool {
        self.full_card_map_within_upper_bound
            && self.cards_preserved
            && self.legacy_owner_indexes_preserved
            && self.nexus_records_preserved
            && self.vault_records_preserved
            && self.nexus_subject_indexes_preserved
            && self.overflow_owner_indexes_preserved
            && self.overflow_subject_indexes_preserved
            && self.nft_wrapping_records_preserved
            && self.nft_auxiliary_records_preserved
            && self.external_escrow_records_preserved
            && self.external_escrow_indexes_preserved
            && self.external_escrow_assignments_preserved
            && self.game_authority_records_preserved
            && self.retired_economy_records_preserved
            && self.lifecycle_quiescent
            && self.retired_economies_quiescent
            && self.v2_sidecar_prefixes_absent
            && self.legacy_owner_indexes_consistent
            && self.nft_wrapping_domain_consistent
            && self.external_escrow_domain_consistent
            && self.nexus_subject_indexes_consistent
            && self.overflow_owner_indexes_consistent
            && self.overflow_subject_indexes_consistent
            && self.vault_links_consistent
            && self.next_vault_variant_id_monotonic
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MigrationResult {
    schema_version: u32,
    kind: &'static str,
    release_id: String,
    source_commit: String,
    snapshot_sha256: String,
    runtime_wasm_sha256: String,
    try_runtime_log_sha256: String,
    from_storage_version: u16,
    to_storage_version: u16,
    migration_phase: &'static str,
    legacy_creation_sealed: bool,
    legacy_writes_paused: bool,
    v2_features: V2Features,
    attestation: AttestationReport,
    legacy_state_evidence: LegacyStateEvidence,
    safe_exit_evidence: Vec<SafeExitEvidence>,
    checks: MigrationChecks,
    counts: MigrationCounts,
}

#[derive(Clone)]
struct LegacySnapshot {
    cards: BTreeMap<u32, Vec<u8>>,
    owner_indexes: BTreeMap<AccountId, Vec<u32>>,
    nexus_cards: BTreeMap<u32, Vec<u8>>,
    vault_variants: BTreeMap<u32, Vec<u8>>,
    nexus_subject_indexes: BTreeMap<(AccountId, u32), u32>,
    overflow_owner_indexes: BTreeMap<AccountId, Vec<u32>>,
    overflow_subject_indexes: BTreeMap<(AccountId, u32), u32>,
    converted: BTreeSet<u32>,
    nft_collection_id: Option<u32>,
    nft_collection: Option<Vec<u8>>,
    nft_collection_config: Option<Vec<u8>>,
    nft_items: BTreeMap<u32, Vec<u8>>,
    nft_item_configs: BTreeMap<u32, Vec<u8>>,
    nft_account_index: BTreeSet<(AccountId, u32)>,
    escrow_entries: BTreeMap<u32, Vec<u8>>,
    escrow_owner_indexes: BTreeMap<AccountId, Vec<u32>>,
    escrow_available_count: u32,
    escrow_available_by_index: BTreeMap<u32, u32>,
    escrow_index_by_card: BTreeMap<u32, u32>,
    escrow_game_assignments: BTreeMap<u64, Vec<u8>>,
    game_authority_games: Vec<Vec<u8>>,
    game_authority_active_by_player: Vec<Vec<u8>>,
    game_authority_expirations: Vec<Vec<u8>>,
    cryptostrike_pending_claims: Vec<Vec<u8>>,
    cryptostrike_servers: Vec<Vec<u8>>,
    cryptostrike_pending_unstakes: Vec<Vec<u8>>,
    cryptostrike_allowances: Vec<Vec<u8>>,
    cryptostrike_session_rosters: Vec<Vec<u8>>,
    cryptostrike_active_players: Vec<Vec<u8>>,
    preexisting_v2_sidecar_records: Vec<Vec<u8>>,
    lifecycle_quiescent: bool,
    retired_economies_quiescent: bool,
    next_card_id: u32,
    next_vault_variant_id: u32,
    storage_version: StorageVersion,
}

#[derive(Clone, Copy)]
struct NexusIndexChecks {
    subject_indexes_consistent: bool,
    overflow_owner_indexes_consistent: bool,
    overflow_subject_indexes_consistent: bool,
    vault_links_consistent: bool,
    next_vault_variant_id_monotonic: bool,
}

#[derive(Clone, Copy)]
struct CustodyDomainChecks {
    legacy_owner_indexes_consistent: bool,
    nft_wrapping_domain_consistent: bool,
    external_escrow_domain_consistent: bool,
}

fn sha256(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn hex_hash(value: &Hash32) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn canonical_domain_evidence(
    domain: &[u8],
    records: impl IntoIterator<Item = Vec<u8>>,
) -> DomainEvidence {
    let mut records: Vec<Vec<u8>> = records.into_iter().collect();
    records.sort();
    let mut hasher = Sha256::new();
    hasher.update(DOMAIN_DIGEST_PREFIX);
    hasher.update((domain.len() as u64).to_le_bytes());
    hasher.update(domain);
    hasher.update((records.len() as u64).to_le_bytes());
    for record in &records {
        hasher.update((record.len() as u64).to_le_bytes());
        hasher.update(record);
    }
    DomainEvidence {
        count: records.len() as u64,
        sha256: hasher.finalize().into(),
    }
}

fn pallet_prefix_records(pallet_name: &[u8]) -> Vec<Vec<u8>> {
    let prefix = sp_io::hashing::twox_128(pallet_name);
    let mut cursor = prefix.to_vec();
    let mut records = Vec::new();
    while let Some(key) = sp_io::storage::next_key(&cursor) {
        if !key.starts_with(&prefix) {
            break;
        }
        let value = sp_io::storage::get(&key).unwrap_or_default();
        records.push((pallet_name, key.as_slice(), value.as_ref()).encode());
        cursor = key;
    }
    records
}

fn capture_legacy() -> LegacySnapshot {
    let cards: BTreeMap<_, _> = pallet_eterra_tcg::Cards::<Runtime>::iter()
        .map(|(card_id, card)| (card_id, card.encode()))
        .collect();
    let nft_collection_id = pallet_eterra_tcg::CardNftCollectionId::<Runtime>::get();
    let nft_collection = nft_collection_id
        .and_then(pallet_nfts::Collection::<Runtime>::get)
        .map(|details| details.encode());
    let nft_collection_config = nft_collection_id
        .and_then(pallet_nfts::CollectionConfigOf::<Runtime>::get)
        .map(|config| config.encode());
    let nft_items = nft_collection_id
        .map(|collection_id| {
            pallet_nfts::Item::<Runtime>::iter_prefix(collection_id)
                .map(|(item_id, details)| (item_id, details.encode()))
                .collect()
        })
        .unwrap_or_default();
    let nft_item_configs = nft_collection_id
        .map(|collection_id| {
            pallet_nfts::ItemConfigOf::<Runtime>::iter_prefix(collection_id)
                .map(|(item_id, config)| (item_id, config.encode()))
                .collect()
        })
        .unwrap_or_default();
    let nft_account_index = nft_collection_id
        .map(|configured_collection| {
            pallet_nfts::Account::<Runtime>::iter()
                .filter_map(|((owner, collection_id, item_id), ())| {
                    (collection_id == configured_collection).then_some((owner, item_id))
                })
                .collect()
        })
        .unwrap_or_default();

    let lifecycle_quiescent = !pallet_eterra_game_authority::Games::<Runtime>::iter()
        .any(|(_, game)| game.started && !game.ended)
        && pallet_eterra_game_authority::ActiveGameByPlayer::<Runtime>::iter()
            .next()
            .is_none()
        && pallet_eterra_game_authority::Expirations::<Runtime>::iter()
            .next()
            .is_none()
        && pallet_eterra_card_escrow::GameEnemyAssignments::<Runtime>::iter()
            .next()
            .is_none()
        && !pallet_eterra_card_escrow::EscrowEntries::<Runtime>::iter()
            .any(|(_, entry)| entry.reserved_by.is_some());
    let retired_economies_quiescent = pallet_cryptostrike::PendingGuapClaims::<Runtime>::iter()
        .next()
        .is_none()
        && pallet_cryptostrike::Servers::<Runtime>::iter()
            .next()
            .is_none()
        && pallet_cryptostrike::PendingUnstakes::<Runtime>::iter()
            .next()
            .is_none()
        && pallet_cryptostrike::ServerAllowances::<Runtime>::iter()
            .next()
            .is_none()
        && pallet_cryptostrike::ActiveSessionRoster::<Runtime>::iter()
            .next()
            .is_none()
        && pallet_cryptostrike::ActivePlayer::<Runtime>::iter()
            .next()
            .is_none();
    let preexisting_v2_sidecar_records = [
        b"EterraRandomness".as_slice(),
        b"EterraCreatures".as_slice(),
        b"EterraMagic".as_slice(),
        b"EterraGameResults".as_slice(),
    ]
    .into_iter()
    .flat_map(pallet_prefix_records)
    .collect();
    LegacySnapshot {
        cards,
        owner_indexes: pallet_eterra_tcg::CardsByOwner::<Runtime>::iter()
            .map(|(owner, card_ids)| (owner, card_ids.into_iter().collect()))
            .collect(),
        nexus_cards: pallet_eterra_tcg::NexusCollectionCards::<Runtime>::iter()
            .map(|(card_id, card)| (card_id, card.encode()))
            .collect(),
        vault_variants: pallet_eterra_tcg::VaultVariants::<Runtime>::iter()
            .map(|(variant_id, variant)| (variant_id, variant.encode()))
            .collect(),
        nexus_subject_indexes: pallet_eterra_tcg::NexusSubjectCopyCounts::<Runtime>::iter()
            .map(|(owner, subject_id, count)| ((owner, subject_id), count))
            .collect(),
        overflow_owner_indexes: pallet_eterra_tcg::NexusOverflowCards::<Runtime>::iter()
            .map(|(owner, card_ids)| (owner, card_ids.into_inner()))
            .collect(),
        overflow_subject_indexes: pallet_eterra_tcg::NexusOverflowSubjectCounts::<Runtime>::iter()
            .map(|(owner, subject_id, count)| ((owner, subject_id), count))
            .collect(),
        converted: pallet_eterra_tcg::Converted::<Runtime>::iter_keys().collect(),
        nft_collection_id,
        nft_collection,
        nft_collection_config,
        nft_items,
        nft_item_configs,
        nft_account_index,
        escrow_entries: pallet_eterra_card_escrow::EscrowEntries::<Runtime>::iter()
            .map(|(card_id, entry)| (card_id, entry.encode()))
            .collect(),
        escrow_owner_indexes: pallet_eterra_card_escrow::EscrowedByOwner::<Runtime>::iter()
            .map(|(owner, card_ids)| (owner, card_ids.into_iter().collect()))
            .collect(),
        escrow_available_count: pallet_eterra_card_escrow::AvailableEscrowCount::<Runtime>::get(),
        escrow_available_by_index:
            pallet_eterra_card_escrow::AvailableCardByIndex::<Runtime>::iter().collect(),
        escrow_index_by_card: pallet_eterra_card_escrow::AvailableIndexByCard::<Runtime>::iter()
            .collect(),
        escrow_game_assignments: pallet_eterra_card_escrow::GameEnemyAssignments::<Runtime>::iter()
            .map(|(game_id, assignments)| (game_id, assignments.encode()))
            .collect(),
        game_authority_games: pallet_eterra_game_authority::Games::<Runtime>::iter()
            .map(|(game_id, game)| (game_id, game).encode())
            .collect(),
        game_authority_active_by_player:
            pallet_eterra_game_authority::ActiveGameByPlayer::<Runtime>::iter()
                .map(|(player, game_id)| (player, game_id).encode())
                .collect(),
        game_authority_expirations: pallet_eterra_game_authority::Expirations::<Runtime>::iter()
            .map(|(block, game_ids)| (block, game_ids).encode())
            .collect(),
        cryptostrike_pending_claims: pallet_cryptostrike::PendingGuapClaims::<Runtime>::iter()
            .map(|(steam_hash, amount)| (steam_hash, amount).encode())
            .collect(),
        cryptostrike_servers: pallet_cryptostrike::Servers::<Runtime>::iter()
            .map(|(server_id, server)| (server_id, server).encode())
            .collect(),
        cryptostrike_pending_unstakes: pallet_cryptostrike::PendingUnstakes::<Runtime>::iter()
            .map(|(server_id, unstake)| (server_id, unstake).encode())
            .collect(),
        cryptostrike_allowances: pallet_cryptostrike::ServerAllowances::<Runtime>::iter()
            .map(|(account, server_id, allowance)| (account, server_id, allowance).encode())
            .collect(),
        cryptostrike_session_rosters: pallet_cryptostrike::ActiveSessionRoster::<Runtime>::iter()
            .map(|(server_id, session_id, roster)| (server_id, session_id, roster).encode())
            .collect(),
        cryptostrike_active_players: pallet_cryptostrike::ActivePlayer::<Runtime>::iter()
            .map(|(key, player)| (key, player).encode())
            .collect(),
        preexisting_v2_sidecar_records,
        lifecycle_quiescent,
        retired_economies_quiescent,
        next_card_id: pallet_eterra_tcg::NextCardId::<Runtime>::get(),
        next_vault_variant_id: pallet_eterra_tcg::NextVaultVariantId::<Runtime>::get(),
        storage_version: StorageVersion::get::<EterraTCG>(),
    }
}

fn extended_domains_preserved(before: &LegacySnapshot, after: &LegacySnapshot) -> bool {
    after.nft_collection_config == before.nft_collection_config
        && after.nft_item_configs == before.nft_item_configs
        && after.nft_account_index == before.nft_account_index
        && after.game_authority_games == before.game_authority_games
        && after.game_authority_active_by_player == before.game_authority_active_by_player
        && after.game_authority_expirations == before.game_authority_expirations
        && after.cryptostrike_pending_claims == before.cryptostrike_pending_claims
        && after.cryptostrike_servers == before.cryptostrike_servers
        && after.cryptostrike_pending_unstakes == before.cryptostrike_pending_unstakes
        && after.cryptostrike_allowances == before.cryptostrike_allowances
        && after.cryptostrike_session_rosters == before.cryptostrike_session_rosters
        && after.cryptostrike_active_players == before.cryptostrike_active_players
        && after.preexisting_v2_sidecar_records == before.preexisting_v2_sidecar_records
}

fn full_card_coverage(
    cards: &BTreeMap<u32, Vec<u8>>,
    upper_bound: u32,
    cards_seen: u32,
) -> Result<Option<u32>, String> {
    let card_count: u32 = cards
        .len()
        .try_into()
        .map_err(|_| "full Cards map count exceeds u32".to_string())?;
    if card_count != cards_seen {
        return Err(format!(
            "bounded migration saw {cards_seen} cards but the full Cards map contains {card_count}"
        ));
    }
    if let Some(card_id) = cards.keys().find(|card_id| **card_id >= upper_bound) {
        return Err(format!(
            "full Cards map contains card {card_id} outside migration upper bound {upper_bound}"
        ));
    }
    let max_seen = cards.keys().next_back().copied();
    if max_seen
        .map(|card_id| upper_bound <= card_id)
        .unwrap_or(false)
    {
        return Err("NextCardId is not greater than the maximum legacy card ID".to_string());
    }
    Ok(max_seen)
}

fn validate_classification(
    card_id: u32,
    card: &CardInfo<AccountId>,
    classification: &LegacyCardClassification<AccountId>,
) -> bool {
    let converted = pallet_eterra_tcg::Converted::<Runtime>::contains_key(card_id);
    let escrow_owner = EterraCardEscrow::escrow_entry(card_id).map(|entry| entry.owner);
    let external_escrow_custodian = EterraCardEscrow::account_id();
    let nft_owner = pallet_eterra_tcg::CardNftCollectionId::<Runtime>::get()
        .and_then(|collection_id| Nfts::owner(collection_id, card_id));
    let nexus_owner =
        pallet_eterra_tcg::NexusCollectionCards::<Runtime>::get(card_id).map(|record| record.owner);

    let custody_valid = match classification.custody {
        LegacyCustodyKind::Ordinary => {
            !classification.frozen
                && !converted
                && escrow_owner.is_none()
                && card.get_owner() != &external_escrow_custodian
                && classification.beneficial_owner.as_ref() == Some(card.get_owner())
        }
        LegacyCustodyKind::NftWrapped => {
            !classification.frozen
                && converted
                && escrow_owner.is_none()
                && nft_owner.is_some()
                && classification.beneficial_owner == nft_owner
        }
        LegacyCustodyKind::KnownEscrow => {
            !classification.frozen
                && !converted
                && escrow_owner.is_some()
                && card.get_owner() == &external_escrow_custodian
                && classification.beneficial_owner == escrow_owner
        }
        LegacyCustodyKind::UnknownFrozen => {
            classification.frozen && classification.beneficial_owner.is_none()
        }
    };
    custody_valid
        && (classification.frozen
            || nexus_owner.is_none()
            || nexus_owner == classification.beneficial_owner)
}

fn all_v2_features() -> V2Features {
    V2Features {
        packs: pallet_eterra_tcg::V2FeatureEnabled::<Runtime>::get(V2Feature::Packs),
        conversion: pallet_eterra_tcg::V2FeatureEnabled::<Runtime>::get(V2Feature::Conversion),
        ranked: pallet_eterra_tcg::V2FeatureEnabled::<Runtime>::get(V2Feature::Ranked),
        mythical_ascension: pallet_eterra_tcg::V2FeatureEnabled::<Runtime>::get(
            V2Feature::MythicalAscension,
        ),
    }
}

fn verify_nexus_indexes() -> NexusIndexChecks {
    let nexus_cards: BTreeMap<_, _> =
        pallet_eterra_tcg::NexusCollectionCards::<Runtime>::iter().collect();
    let vault_variants: BTreeMap<_, _> =
        pallet_eterra_tcg::VaultVariants::<Runtime>::iter().collect();

    let mut expected_subject_indexes = BTreeMap::<(AccountId, u32), u32>::new();
    let mut expected_overflow_owner_indexes = BTreeMap::<AccountId, BTreeSet<u32>>::new();
    let mut expected_overflow_subject_indexes = BTreeMap::<(AccountId, u32), u32>::new();
    for (card_id, record) in &nexus_cards {
        match record.location {
            NexusStorageLocation::Collection | NexusStorageLocation::Vault => {
                *expected_subject_indexes
                    .entry((record.owner.clone(), record.subject_id))
                    .or_default() += 1;
            }
            NexusStorageLocation::Overflow => {
                expected_overflow_owner_indexes
                    .entry(record.owner.clone())
                    .or_default()
                    .insert(*card_id);
                *expected_overflow_subject_indexes
                    .entry((record.owner.clone(), record.subject_id))
                    .or_default() += 1;
            }
        }
    }

    let actual_subject_indexes: BTreeMap<_, _> =
        pallet_eterra_tcg::NexusSubjectCopyCounts::<Runtime>::iter()
            .map(|(owner, subject, count)| ((owner, subject), count))
            .collect();
    let mut overflow_owner_indexes_have_no_duplicates = true;
    let actual_overflow_owner_indexes: BTreeMap<AccountId, BTreeSet<u32>> =
        pallet_eterra_tcg::NexusOverflowCards::<Runtime>::iter()
            .map(|(owner, cards)| {
                let as_set: BTreeSet<_> = cards.iter().copied().collect();
                overflow_owner_indexes_have_no_duplicates &= as_set.len() == cards.len();
                (owner, as_set)
            })
            .collect();
    let actual_overflow_subject_indexes: BTreeMap<_, _> =
        pallet_eterra_tcg::NexusOverflowSubjectCounts::<Runtime>::iter()
            .map(|(owner, subject, count)| ((owner, subject), count))
            .collect();

    let mut seen_vault_cards = BTreeSet::new();
    let variants_point_to_vault_cards = vault_variants.values().all(|variant| {
        seen_vault_cards.insert(variant.card_record_id)
            && nexus_cards
                .get(&variant.card_record_id)
                .map(|record| {
                    record.location == NexusStorageLocation::Vault
                        && record.subject_id == variant.subject_id
                })
                .unwrap_or(false)
    });
    let every_vault_card_has_variant = nexus_cards.iter().all(|(card_id, record)| {
        record.location != NexusStorageLocation::Vault || seen_vault_cards.contains(card_id)
    });
    let max_variant = vault_variants.keys().next_back().copied();
    let next_vault_variant_id = pallet_eterra_tcg::NextVaultVariantId::<Runtime>::get();

    NexusIndexChecks {
        subject_indexes_consistent: expected_subject_indexes == actual_subject_indexes,
        overflow_owner_indexes_consistent: overflow_owner_indexes_have_no_duplicates
            && expected_overflow_owner_indexes == actual_overflow_owner_indexes,
        overflow_subject_indexes_consistent: expected_overflow_subject_indexes
            == actual_overflow_subject_indexes,
        vault_links_consistent: variants_point_to_vault_cards && every_vault_card_has_variant,
        next_vault_variant_id_monotonic: max_variant
            .map(|variant_id| next_vault_variant_id > variant_id)
            .unwrap_or(true),
    }
}

fn verify_custody_domains() -> CustodyDomainChecks {
    let cards: BTreeMap<_, _> = pallet_eterra_tcg::Cards::<Runtime>::iter().collect();

    // `CardsByOwner` remains the physical-custody index used by preserved
    // transfer/wrap/unwrap paths. Do not attest migration completion if stale
    // or missing entries could strand a safe exit or bypass capacity checks.
    let mut expected_physical_owner_indexes = BTreeMap::<AccountId, BTreeSet<u32>>::new();
    for (card_id, card) in &cards {
        expected_physical_owner_indexes
            .entry(card.get_owner().clone())
            .or_default()
            .insert(*card_id);
    }
    let mut physical_owner_indexes_have_no_encoded_duplicates = true;
    let actual_physical_owner_indexes: BTreeMap<AccountId, BTreeSet<u32>> =
        pallet_eterra_tcg::CardsByOwner::<Runtime>::iter()
            .filter_map(|(owner, card_ids)| {
                let card_ids: BTreeSet<_> = card_ids.into_iter().collect();
                physical_owner_indexes_have_no_encoded_duplicates &=
                    pallet_eterra_tcg::CardsByOwner::<Runtime>::decode_non_dedup_len(&owner)
                        == Some(card_ids.len());
                (!card_ids.is_empty()).then_some((owner, card_ids))
            })
            .collect();
    let legacy_owner_indexes_consistent = physical_owner_indexes_have_no_encoded_duplicates
        && expected_physical_owner_indexes == actual_physical_owner_indexes;

    // The configured TCG NFT collection is dedicated to wrapped cards. Its
    // item keys, the Converted set, the Cards map and internal escrow custody
    // therefore form a strict bijection.
    let converted: BTreeSet<_> = pallet_eterra_tcg::Converted::<Runtime>::iter_keys().collect();
    let internal_escrow = {
        use sp_runtime::traits::AccountIdConversion;
        frame_support::PalletId(*b"et/tcgsc").into_account_truncating()
    };
    let nft_wrapping_domain_consistent =
        match pallet_eterra_tcg::CardNftCollectionId::<Runtime>::get() {
            Some(collection_id) => {
                let collection_exists =
                    pallet_nfts::Collection::<Runtime>::contains_key(collection_id);
                let collection_config_exists =
                    pallet_nfts::CollectionConfigOf::<Runtime>::contains_key(collection_id);
                let item_ids: BTreeSet<_> =
                    pallet_nfts::Item::<Runtime>::iter_prefix(collection_id)
                        .map(|(item_id, _)| item_id)
                        .collect();
                let item_config_ids: BTreeSet<_> =
                    pallet_nfts::ItemConfigOf::<Runtime>::iter_prefix(collection_id)
                        .map(|(item_id, _)| item_id)
                        .collect();
                let expected_account_index: BTreeSet<_> = item_ids
                    .iter()
                    .filter_map(|item_id| {
                        Nfts::owner(collection_id, *item_id).map(|owner| (owner, *item_id))
                    })
                    .collect();
                let actual_account_index: BTreeSet<_> = pallet_nfts::Account::<Runtime>::iter()
                    .filter_map(|((owner, stored_collection, item_id), ())| {
                        (stored_collection == collection_id).then_some((owner, item_id))
                    })
                    .collect();
                collection_exists
                    && collection_config_exists
                    && item_ids == converted
                    && item_config_ids == converted
                    && expected_account_index.len() == item_ids.len()
                    && actual_account_index == expected_account_index
                    && converted.iter().all(|card_id| {
                        cards
                            .get(card_id)
                            .map(|card| card.get_owner() == &internal_escrow)
                            .unwrap_or(false)
                            && Nfts::owner(collection_id, *card_id).is_some()
                            && !pallet_eterra_card_escrow::EscrowEntries::<Runtime>::contains_key(
                                card_id,
                            )
                    })
            }
            None => converted.is_empty(),
        };

    let external_escrow = EterraCardEscrow::account_id();
    let escrow_entries: BTreeMap<_, _> =
        pallet_eterra_card_escrow::EscrowEntries::<Runtime>::iter().collect();
    let mut expected_escrow_owner_indexes = BTreeMap::<AccountId, BTreeSet<u32>>::new();
    let mut expected_available_cards = BTreeSet::<u32>::new();
    let mut escrow_entries_valid = true;
    for (card_id, entry) in &escrow_entries {
        expected_escrow_owner_indexes
            .entry(entry.owner.clone())
            .or_default()
            .insert(*card_id);
        escrow_entries_valid &= cards
            .get(card_id)
            .map(|card| card.get_owner() == &external_escrow)
            .unwrap_or(false);
        escrow_entries_valid &= !converted.contains(card_id);
        if entry.reserved_by.is_none() {
            escrow_entries_valid &= !entry.withdraw_requested;
            expected_available_cards.insert(*card_id);
        }
    }
    let mut escrow_owner_indexes_have_no_encoded_duplicates = true;
    let actual_escrow_owner_indexes: BTreeMap<AccountId, BTreeSet<u32>> =
        pallet_eterra_card_escrow::EscrowedByOwner::<Runtime>::iter()
            .filter_map(|(owner, card_ids)| {
                let card_ids: BTreeSet<_> = card_ids.into_iter().collect();
                escrow_owner_indexes_have_no_encoded_duplicates &=
                    pallet_eterra_card_escrow::EscrowedByOwner::<Runtime>::decode_non_dedup_len(
                        &owner,
                    ) == Some(card_ids.len());
                (!card_ids.is_empty()).then_some((owner, card_ids))
            })
            .collect();

    let available_count = pallet_eterra_card_escrow::AvailableEscrowCount::<Runtime>::get();
    let available_by_index: BTreeMap<_, _> =
        pallet_eterra_card_escrow::AvailableCardByIndex::<Runtime>::iter().collect();
    let index_by_card: BTreeMap<_, _> =
        pallet_eterra_card_escrow::AvailableIndexByCard::<Runtime>::iter().collect();
    let expected_indices: BTreeSet<_> = (0..available_count).collect();
    let actual_indices: BTreeSet<_> = available_by_index.keys().copied().collect();
    let actual_available_cards: BTreeSet<_> = available_by_index.values().copied().collect();
    let available_indexes_consistent = expected_indices == actual_indices
        && available_by_index.len() == available_count as usize
        && index_by_card.len() == available_count as usize
        && actual_available_cards.len() == available_count as usize
        && actual_available_cards == expected_available_cards
        && available_by_index
            .iter()
            .all(|(index, card_id)| index_by_card.get(card_id) == Some(index))
        && index_by_card
            .iter()
            .all(|(card_id, index)| available_by_index.get(index) == Some(card_id));

    let mut assigned_cards = BTreeSet::<u32>::new();
    let mut assignments_consistent = true;
    for (game_id, assignments) in pallet_eterra_card_escrow::GameEnemyAssignments::<Runtime>::iter()
    {
        for assignment in assignments {
            assignments_consistent &= assigned_cards.insert(assignment.card_id);
            assignments_consistent &= escrow_entries
                .get(&assignment.card_id)
                .map(|entry| {
                    entry.reserved_by == Some(game_id)
                        && entry.owner == assignment.owner
                        && entry.genome == assignment.genome
                })
                .unwrap_or(false);
        }
    }
    let expected_assigned_cards: BTreeSet<_> = escrow_entries
        .iter()
        .filter_map(|(card_id, entry)| entry.reserved_by.map(|_| *card_id))
        .collect();
    assignments_consistent &= assigned_cards == expected_assigned_cards;

    CustodyDomainChecks {
        legacy_owner_indexes_consistent,
        nft_wrapping_domain_consistent,
        external_escrow_domain_consistent: escrow_entries_valid
            && escrow_owner_indexes_have_no_encoded_duplicates
            && expected_escrow_owner_indexes == actual_escrow_owner_indexes
            && available_indexes_consistent
            && assignments_consistent,
    }
}

fn build_attestation(
    snapshot: &LegacySnapshot,
    state: &pallet_eterra_tcg::TcgMigrationStateV16,
) -> (AttestationEvidence, Hash32) {
    let classifications: Vec<_> = pallet_eterra_tcg::LegacyCardClassifications::<Runtime>::iter()
        .map(|(card_id, classification)| (card_id, classification).encode())
        .collect();
    let anomaly_records: Vec<_> = pallet_eterra_tcg::TcgMigrationAnomaliesV16::<Runtime>::iter()
        .map(|(card_id, anomaly)| (card_id, anomaly).encode())
        .collect();
    let repaired_owner_indexes: Vec<_> =
        pallet_eterra_tcg::RepairedLegacyCardsByOwnerV16::<Runtime>::iter()
            .filter(|(_, _, present)| *present)
            .map(|(owner, card_id, present)| (owner, card_id, present).encode())
            .collect();
    let repaired_subject_indexes: Vec<_> =
        pallet_eterra_tcg::RepairedLegacySubjectCountsV16::<Runtime>::iter()
            .map(|(owner, subject_id, count)| (owner, subject_id, count).encode())
            .collect();

    let evidence = AttestationEvidence {
        schema_version: 3,
        from_storage_version: state.from_storage_version,
        to_storage_version: 16,
        upper_bound: state.upper_bound,
        next_card_id: snapshot.next_card_id,
        cards_seen: state.cards_seen,
        ordinary: state.ordinary,
        nft_wrapped: state.nft_wrapped,
        known_escrow: state.known_escrow,
        anomalies: state.anomalies,
        max_card_id_seen: state.max_card_id_seen,
        next_vault_variant_id: snapshot.next_vault_variant_id,
        cards: canonical_domain_evidence(
            b"Cards",
            snapshot
                .cards
                .iter()
                .map(|(card_id, card)| (card_id, card).encode()),
        ),
        legacy_owner_indexes: canonical_domain_evidence(
            b"CardsByOwner",
            snapshot
                .owner_indexes
                .iter()
                .map(|(owner, cards)| (owner, cards).encode()),
        ),
        nexus_cards: canonical_domain_evidence(
            b"NexusCollectionCards",
            snapshot
                .nexus_cards
                .iter()
                .map(|(card_id, card)| (card_id, card).encode()),
        ),
        vault_variants: canonical_domain_evidence(
            b"VaultVariants",
            snapshot
                .vault_variants
                .iter()
                .map(|(variant_id, variant)| (variant_id, variant).encode()),
        ),
        nexus_subject_indexes: canonical_domain_evidence(
            b"NexusSubjectCopyCounts",
            snapshot
                .nexus_subject_indexes
                .iter()
                .map(|(key, count)| (key, count).encode()),
        ),
        overflow_owner_indexes: canonical_domain_evidence(
            b"NexusOverflowCards",
            snapshot
                .overflow_owner_indexes
                .iter()
                .map(|(owner, cards)| (owner, cards).encode()),
        ),
        overflow_subject_indexes: canonical_domain_evidence(
            b"NexusOverflowSubjectCounts",
            snapshot
                .overflow_subject_indexes
                .iter()
                .map(|(key, count)| (key, count).encode()),
        ),
        nft_wrapped_cards: canonical_domain_evidence(
            b"Converted",
            snapshot.converted.iter().map(Encode::encode),
        ),
        nft_collection: canonical_domain_evidence(
            b"CardNftCollection",
            snapshot
                .nft_collection_id
                .iter()
                .map(|collection_id| (collection_id, &snapshot.nft_collection).encode()),
        ),
        nft_collection_config: canonical_domain_evidence(
            b"CardNftCollectionConfig",
            snapshot
                .nft_collection_id
                .iter()
                .map(|collection_id| (collection_id, &snapshot.nft_collection_config).encode()),
        ),
        nft_items: canonical_domain_evidence(
            b"CardNftItems",
            snapshot
                .nft_items
                .iter()
                .map(|(item_id, item)| (item_id, item).encode()),
        ),
        nft_item_configs: canonical_domain_evidence(
            b"CardNftItemConfigs",
            snapshot
                .nft_item_configs
                .iter()
                .map(|(item_id, config)| (item_id, config).encode()),
        ),
        nft_account_index: canonical_domain_evidence(
            b"CardNftAccountIndex",
            snapshot.nft_account_index.iter().map(Encode::encode),
        ),
        external_escrow_entries: canonical_domain_evidence(
            b"CardEscrowEntries",
            snapshot
                .escrow_entries
                .iter()
                .map(|(card_id, entry)| (card_id, entry).encode()),
        ),
        external_escrow_owner_indexes: canonical_domain_evidence(
            b"CardEscrowedByOwner",
            snapshot
                .escrow_owner_indexes
                .iter()
                .map(|(owner, card_ids)| (owner, card_ids).encode()),
        ),
        external_escrow_available_count: canonical_domain_evidence(
            b"CardEscrowAvailableCount",
            [snapshot.escrow_available_count.encode()],
        ),
        external_escrow_available_by_index: canonical_domain_evidence(
            b"CardEscrowAvailableCardByIndex",
            snapshot
                .escrow_available_by_index
                .iter()
                .map(|(index, card_id)| (index, card_id).encode()),
        ),
        external_escrow_index_by_card: canonical_domain_evidence(
            b"CardEscrowAvailableIndexByCard",
            snapshot
                .escrow_index_by_card
                .iter()
                .map(|(card_id, index)| (card_id, index).encode()),
        ),
        external_escrow_game_assignments: canonical_domain_evidence(
            b"CardEscrowGameEnemyAssignments",
            snapshot
                .escrow_game_assignments
                .iter()
                .map(|(game_id, assignments)| (game_id, assignments).encode()),
        ),
        game_authority_games: canonical_domain_evidence(
            b"GameAuthorityGames",
            snapshot.game_authority_games.iter().cloned(),
        ),
        game_authority_active_by_player: canonical_domain_evidence(
            b"GameAuthorityActiveGameByPlayer",
            snapshot.game_authority_active_by_player.iter().cloned(),
        ),
        game_authority_expirations: canonical_domain_evidence(
            b"GameAuthorityExpirations",
            snapshot.game_authority_expirations.iter().cloned(),
        ),
        cryptostrike_pending_claims: canonical_domain_evidence(
            b"CryptoStrikePendingGuapClaims",
            snapshot.cryptostrike_pending_claims.iter().cloned(),
        ),
        cryptostrike_servers: canonical_domain_evidence(
            b"CryptoStrikeServers",
            snapshot.cryptostrike_servers.iter().cloned(),
        ),
        cryptostrike_pending_unstakes: canonical_domain_evidence(
            b"CryptoStrikePendingUnstakes",
            snapshot.cryptostrike_pending_unstakes.iter().cloned(),
        ),
        cryptostrike_allowances: canonical_domain_evidence(
            b"CryptoStrikeServerAllowances",
            snapshot.cryptostrike_allowances.iter().cloned(),
        ),
        cryptostrike_session_rosters: canonical_domain_evidence(
            b"CryptoStrikeActiveSessionRoster",
            snapshot.cryptostrike_session_rosters.iter().cloned(),
        ),
        cryptostrike_active_players: canonical_domain_evidence(
            b"CryptoStrikeActivePlayer",
            snapshot.cryptostrike_active_players.iter().cloned(),
        ),
        preexisting_v2_sidecar_records: canonical_domain_evidence(
            b"PreexistingV2SidecarRecords",
            snapshot.preexisting_v2_sidecar_records.iter().cloned(),
        ),
        classifications: canonical_domain_evidence(b"LegacyCardClassifications", classifications),
        anomaly_records: canonical_domain_evidence(b"TcgMigrationAnomaliesV16", anomaly_records),
        repaired_owner_indexes: canonical_domain_evidence(
            b"RepairedLegacyCardsByOwnerV16",
            repaired_owner_indexes,
        ),
        repaired_subject_indexes: canonical_domain_evidence(
            b"RepairedLegacySubjectCountsV16",
            repaired_subject_indexes,
        ),
    };
    let encoded = evidence.encode();
    let verification_hash = canonical_domain_evidence(ATTESTATION_DOMAIN, [encoded]).sha256;
    (evidence, verification_hash)
}

fn domain_report(value: DomainEvidence) -> DomainEvidenceReport {
    DomainEvidenceReport {
        count: value.count,
        sha256: hex_hash(&value.sha256),
    }
}

fn attestation_report(
    evidence: &AttestationEvidence,
    verification_hash: Hash32,
) -> AttestationReport {
    let mut domains = BTreeMap::new();
    for (name, value) in [
        ("cards", evidence.cards),
        ("legacyOwnerIndexes", evidence.legacy_owner_indexes),
        ("nexusCards", evidence.nexus_cards),
        ("vaultVariants", evidence.vault_variants),
        ("nexusSubjectIndexes", evidence.nexus_subject_indexes),
        ("overflowOwnerIndexes", evidence.overflow_owner_indexes),
        ("overflowSubjectIndexes", evidence.overflow_subject_indexes),
        ("nftWrappedCards", evidence.nft_wrapped_cards),
        ("nftCollection", evidence.nft_collection),
        ("nftCollectionConfig", evidence.nft_collection_config),
        ("nftItems", evidence.nft_items),
        ("nftItemConfigs", evidence.nft_item_configs),
        ("nftAccountIndex", evidence.nft_account_index),
        ("externalEscrowEntries", evidence.external_escrow_entries),
        (
            "externalEscrowOwnerIndexes",
            evidence.external_escrow_owner_indexes,
        ),
        (
            "externalEscrowAvailableCount",
            evidence.external_escrow_available_count,
        ),
        (
            "externalEscrowAvailableByIndex",
            evidence.external_escrow_available_by_index,
        ),
        (
            "externalEscrowIndexByCard",
            evidence.external_escrow_index_by_card,
        ),
        (
            "externalEscrowGameAssignments",
            evidence.external_escrow_game_assignments,
        ),
        ("gameAuthorityGames", evidence.game_authority_games),
        (
            "gameAuthorityActiveByPlayer",
            evidence.game_authority_active_by_player,
        ),
        (
            "gameAuthorityExpirations",
            evidence.game_authority_expirations,
        ),
        (
            "cryptostrikePendingClaims",
            evidence.cryptostrike_pending_claims,
        ),
        ("cryptostrikeServers", evidence.cryptostrike_servers),
        (
            "cryptostrikePendingUnstakes",
            evidence.cryptostrike_pending_unstakes,
        ),
        ("cryptostrikeAllowances", evidence.cryptostrike_allowances),
        (
            "cryptostrikeSessionRosters",
            evidence.cryptostrike_session_rosters,
        ),
        (
            "cryptostrikeActivePlayers",
            evidence.cryptostrike_active_players,
        ),
        (
            "preexistingV2SidecarRecords",
            evidence.preexisting_v2_sidecar_records,
        ),
        ("classifications", evidence.classifications),
        ("anomalyRecords", evidence.anomaly_records),
        ("repairedOwnerIndexes", evidence.repaired_owner_indexes),
        ("repairedSubjectIndexes", evidence.repaired_subject_indexes),
    ] {
        domains.insert(name, domain_report(value));
    }
    AttestationReport {
        algorithm: "sha256",
        evidence_encoding: "domain-separated-canonical-records-plus-scale-summary-v3",
        verification_hash: hex_hash(&verification_hash),
        upper_bound: evidence.upper_bound,
        next_card_id: evidence.next_card_id,
        cards_seen: evidence.cards_seen,
        next_vault_variant_id: evidence.next_vault_variant_id,
        domains,
    }
}

fn rollback_dispatch(dispatch: impl FnOnce() -> DispatchResult) -> Result<(), String> {
    let result: DispatchResult = with_transaction(|| TransactionOutcome::Rollback(dispatch()));
    result.map_err(|error| format!("{error:?}"))
}

fn signed_origin(owner: AccountId) -> RuntimeOrigin {
    RawOrigin::Signed(owner).into()
}

fn deterministic_recipient(owner: &AccountId) -> AccountId {
    for marker in [0xd1, 0xd2, 0xd3, 0xd4] {
        let candidate = AccountId::from([marker; 32]);
        if &candidate != owner
            && pallet_eterra_tcg::CardsByOwner::<Runtime>::get(&candidate).is_empty()
        {
            return candidate;
        }
    }
    AccountId::from([0xd5; 32])
}

fn owner_can_dispatch(owner: &AccountId) -> bool {
    AlphaAccess::ensure_whitelisted(owner).is_ok()
}

fn card_is_transferable(card_id: u32, card: &CardInfo<AccountId>) -> bool {
    card.is_finalized()
        && pallet_eterra_tcg::NexusCollectionCards::<Runtime>::get(card_id)
            .map(|record| !record.account_bound)
            .unwrap_or(true)
        && Eterra::current_hand_of(card.get_owner())
            .map(|hand| !hand.contains(&card_id))
            .unwrap_or(true)
        && owner_can_dispatch(card.get_owner())
}

fn dispatch_probe(
    path: &'static str,
    card_id: u32,
    dispatch: impl FnOnce() -> DispatchResult,
) -> SafeExitEvidence {
    match rollback_dispatch(dispatch) {
        Ok(()) => SafeExitEvidence {
            path,
            status: SafeExitStatus::Passed,
            candidate_card_id: Some(card_id),
            detail: "representative dispatch succeeded and all writes were rolled back".into(),
        },
        Err(error) => SafeExitEvidence {
            path,
            status: SafeExitStatus::Failed,
            candidate_card_id: Some(card_id),
            detail: format!("representative dispatch failed: {error}"),
        },
    }
}

fn missing_probe(path: &'static str, detail: &str) -> SafeExitEvidence {
    SafeExitEvidence {
        path,
        status: SafeExitStatus::NotPresent,
        candidate_card_id: None,
        detail: detail.into(),
    }
}

fn probe_transfer_for_location(
    path: &'static str,
    expected_location: Option<NexusStorageLocation>,
) -> SafeExitEvidence {
    let cards: BTreeMap<_, _> = pallet_eterra_tcg::Cards::<Runtime>::iter().collect();
    for (card_id, card) in cards {
        let Some(classification) =
            pallet_eterra_tcg::LegacyCardClassifications::<Runtime>::get(card_id)
        else {
            continue;
        };
        if classification.frozen || classification.custody != LegacyCustodyKind::Ordinary {
            continue;
        }
        let location = pallet_eterra_tcg::NexusCollectionCards::<Runtime>::get(card_id)
            .map(|record| record.location);
        let location_matches = match expected_location {
            Some(expected) => location == Some(expected),
            None => location.is_none() || location == Some(NexusStorageLocation::Collection),
        };
        if !location_matches || !card_is_transferable(card_id, &card) {
            continue;
        }
        let owner = card.get_owner().clone();
        let recipient = deterministic_recipient(&owner);
        return dispatch_probe(path, card_id, || {
            EterraTCG::transfer_card(signed_origin(owner), card_id, recipient)
        });
    }
    missing_probe(
        path,
        "copied state contains no eligible non-frozen representative card for this location",
    )
}

fn probe_vault_transfer() -> SafeExitEvidence {
    let cards: BTreeMap<_, _> = pallet_eterra_tcg::Cards::<Runtime>::iter().collect();
    for (card_id, card) in cards {
        let is_vault = pallet_eterra_tcg::NexusCollectionCards::<Runtime>::get(card_id)
            .map(|record| record.location == NexusStorageLocation::Vault)
            .unwrap_or(false);
        if !is_vault {
            continue;
        }
        let Some(classification) =
            pallet_eterra_tcg::LegacyCardClassifications::<Runtime>::get(card_id)
        else {
            return SafeExitEvidence {
                path: "vaultTransfer",
                status: SafeExitStatus::Blocked,
                candidate_card_id: Some(card_id),
                detail: "Vault card has no V16 classification".into(),
            };
        };
        if classification.frozen || classification.custody == LegacyCustodyKind::UnknownFrozen {
            return SafeExitEvidence {
                path: "vaultTransfer",
                status: SafeExitStatus::Blocked,
                candidate_card_id: Some(card_id),
                detail: "Vault card is quarantined by its V16 classification".into(),
            };
        }
        if !card_is_transferable(card_id, &card) {
            continue;
        }
        let owner = classification
            .beneficial_owner
            .clone()
            .unwrap_or_else(|| card.get_owner().clone());
        let recipient = deterministic_recipient(&owner);
        return dispatch_probe("vaultTransfer", card_id, || {
            EterraTCG::transfer_card(signed_origin(owner), card_id, recipient)
        });
    }
    missing_probe(
        "vaultTransfer",
        "copied state contains no transferable Vault card",
    )
}

fn probe_market_unlist() -> SafeExitEvidence {
    let prices: BTreeMap<_, _> = pallet_eterra_tcg::CardPrices::<Runtime>::iter().collect();
    for (card_id, _) in prices {
        let Some(card) = pallet_eterra_tcg::Cards::<Runtime>::get(card_id) else {
            continue;
        };
        let Some(classification) =
            pallet_eterra_tcg::LegacyCardClassifications::<Runtime>::get(card_id)
        else {
            continue;
        };
        if classification.frozen || !owner_can_dispatch(card.get_owner()) {
            continue;
        }
        let owner = card.get_owner().clone();
        return dispatch_probe("marketUnlist", card_id, || {
            EterraTCG::remove_price(signed_origin(owner), card_id)
        });
    }
    missing_probe(
        "marketUnlist",
        "copied state contains no listed non-frozen card with an eligible signer",
    )
}

fn probe_nft_unwrap() -> SafeExitEvidence {
    let classifications: BTreeMap<_, _> =
        pallet_eterra_tcg::LegacyCardClassifications::<Runtime>::iter().collect();
    for (card_id, classification) in classifications {
        if classification.frozen || classification.custody != LegacyCustodyKind::NftWrapped {
            continue;
        }
        let Some(owner) = classification.beneficial_owner else {
            continue;
        };
        if !owner_can_dispatch(&owner) {
            continue;
        }
        return dispatch_probe("nftUnwrap", card_id, || {
            EterraTCG::unwrap_from_nft(signed_origin(owner), card_id)
        });
    }
    missing_probe(
        "nftUnwrap",
        "copied state contains no non-frozen NFT-wrapped card with an eligible signer",
    )
}

fn probe_external_escrow_withdrawal() -> SafeExitEvidence {
    let classifications: BTreeMap<_, _> =
        pallet_eterra_tcg::LegacyCardClassifications::<Runtime>::iter().collect();
    for (card_id, classification) in classifications {
        if classification.frozen || classification.custody != LegacyCustodyKind::KnownEscrow {
            continue;
        }
        let Some(entry) = EterraCardEscrow::escrow_entry(card_id) else {
            continue;
        };
        if classification.beneficial_owner.as_ref() != Some(&entry.owner)
            || !owner_can_dispatch(&entry.owner)
        {
            continue;
        }
        let owner = entry.owner;
        return dispatch_probe("externalEscrowWithdrawal", card_id, || {
            EterraCardEscrow::withdraw_cards(
                signed_origin(owner),
                vec![card_id]
                    .try_into()
                    .expect("one card always fits the runtime escrow bound"),
            )
        });
    }
    missing_probe(
        "externalEscrowWithdrawal",
        "copied state contains no non-frozen external-escrow card with an eligible signer",
    )
}

fn run_safe_exit_probes() -> Vec<SafeExitEvidence> {
    vec![
        probe_transfer_for_location("ordinaryTransfer", None),
        probe_transfer_for_location("overflowTransfer", Some(NexusStorageLocation::Overflow)),
        probe_vault_transfer(),
        probe_market_unlist(),
        probe_nft_unwrap(),
        probe_external_escrow_withdrawal(),
    ]
}

fn safe_exit_probes_acceptable(probes: &[SafeExitEvidence]) -> bool {
    probes.iter().all(|probe| {
        matches!(
            probe.status,
            SafeExitStatus::Passed | SafeExitStatus::NotPresent
        )
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.result.exists() {
        return Err(format!("refusing to overwrite result: {}", args.result.display()).into());
    }
    for path in [
        &args.manifest,
        &args.snapshot,
        &args.runtime_wasm,
        &args.try_runtime_log,
    ] {
        if !path.is_file() {
            return Err(format!("required input is not a regular file: {}", path.display()).into());
        }
    }

    let manifest: BackupManifest = serde_json::from_slice(&fs::read(&args.manifest)?)?;
    if manifest.source_commit.len() != 40
        || !manifest
            .source_commit
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("manifest sourceCommit is not a lowercase 40-hex commit".into());
    }
    let try_log = fs::read_to_string(&args.try_runtime_log)?;
    if !try_log.contains("ETERRA_V16_MIGRATION_AWAITING_VERIFICATION") {
        return Err("try-runtime log does not contain the V16 awaiting-verification marker".into());
    }

    let mut ext = Builder::<Block>::new()
        .mode(Mode::Offline(OfflineConfig {
            state_snapshot: SnapshotConfig::new(args.snapshot.clone()),
        }))
        .build()
        .await?;

    let before = ext.execute_with(capture_legacy);
    let from_storage_version = if before.storage_version == StorageVersion::new(14) {
        14
    } else if before.storage_version == StorageVersion::new(15) {
        15
    } else {
        return Err("unsupported copied TCG storage version; expected V14 or V15".into());
    };
    if from_storage_version != 14 {
        return Err("current private-alpha evidence must start from observed V14".into());
    }
    let before_count: u32 = before
        .cards
        .len()
        .try_into()
        .map_err(|_| "pre-upgrade full Cards map count exceeds u32")?;
    full_card_coverage(&before.cards, before.next_card_id, before_count)?;
    if !before.lifecycle_quiescent {
        return Err(
            "copied state is not lifecycle-quiescent: active/scheduled GameAuthority or reserved CardEscrow state remains"
                .into(),
        );
    }
    if !before.retired_economies_quiescent {
        return Err(
            "copied state contains live CryptoStrike claims, servers, stakes, allowances, rosters, or players"
                .into(),
        );
    }
    if !before.preexisting_v2_sidecar_records.is_empty() {
        return Err(
            "copied state already contains EterraRandomness/Creatures/Magic/GameResults sidecar keys"
                .into(),
        );
    }

    let upgrade_weight = ext.execute_with(<EterraTCG as Hooks<BlockNumber>>::on_runtime_upgrade);
    let after_upgrade = ext.execute_with(|| {
        (
            StorageVersion::get::<EterraTCG>(),
            pallet_eterra_tcg::TcgMigrationStateStorageV16::<Runtime>::get(),
            pallet_eterra_tcg::LegacyWritesPausedV16::<Runtime>::get(),
            capture_legacy(),
            all_v2_features(),
        )
    });
    if after_upgrade.0 != StorageVersion::new(16)
        || after_upgrade
            .1
            .as_ref()
            .map(|state| {
                state.phase != MigrationPhaseV16::Running
                    || state.from_storage_version != from_storage_version
            })
            .unwrap_or(true)
        || !after_upgrade.2
        || after_upgrade.3.cards != before.cards
        || after_upgrade.3.owner_indexes != before.owner_indexes
        || after_upgrade.3.nexus_cards != before.nexus_cards
        || after_upgrade.3.vault_variants != before.vault_variants
        || after_upgrade.3.nexus_subject_indexes != before.nexus_subject_indexes
        || after_upgrade.3.overflow_owner_indexes != before.overflow_owner_indexes
        || after_upgrade.3.overflow_subject_indexes != before.overflow_subject_indexes
        || after_upgrade.3.converted != before.converted
        || after_upgrade.3.nft_collection_id != before.nft_collection_id
        || after_upgrade.3.nft_collection != before.nft_collection
        || after_upgrade.3.nft_items != before.nft_items
        || !extended_domains_preserved(&before, &after_upgrade.3)
        || after_upgrade.3.escrow_entries != before.escrow_entries
        || after_upgrade.3.escrow_owner_indexes != before.escrow_owner_indexes
        || after_upgrade.3.escrow_available_count != before.escrow_available_count
        || after_upgrade.3.escrow_available_by_index != before.escrow_available_by_index
        || after_upgrade.3.escrow_index_by_card != before.escrow_index_by_card
        || after_upgrade.3.escrow_game_assignments != before.escrow_game_assignments
        || after_upgrade.3.next_card_id != before.next_card_id
        || after_upgrade.3.next_vault_variant_id != before.next_vault_variant_id
    {
        return Err(
            "V16 on-runtime-upgrade did not start fail-closed without rewriting legacy state"
                .into(),
        );
    }
    if !after_upgrade.4.all_disabled() {
        return Err("a V2 feature became enabled during migration start".into());
    }

    // Deliberately end one externalities execution after a bounded partial step.
    // A zero-card Alpha still crosses the execution boundary before completion.
    let partial_limit = if before.cards.len() > 1 { 1 } else { 0 };
    let partial_processed =
        ext.execute_with(|| pallet_eterra_tcg::Pallet::<Runtime>::migrate_v16_batch(partial_limit));
    if partial_processed > partial_limit {
        return Err("partial migration exceeded its explicit item bound".into());
    }
    let partial_state =
        ext.execute_with(pallet_eterra_tcg::TcgMigrationStateStorageV16::<Runtime>::get);
    if partial_state.is_none() {
        return Err("migration state disappeared across the interruption boundary".into());
    }

    let remaining_weight = Weight::from_parts(u64::MAX / 4, u64::MAX / 4);
    let upper_bound = u64::from(before.next_card_id);
    let max_rounds = upper_bound.saturating_add(4);
    let mut bounded_weight = upgrade_weight.all_lte(RuntimeBlockWeights::get().max_block);
    let mut rounds = 0u64;
    loop {
        let state = ext
            .execute_with(pallet_eterra_tcg::TcgMigrationStateStorageV16::<Runtime>::get)
            .ok_or("migration state missing during resume")?;
        match state.phase {
            MigrationPhaseV16::AwaitingVerification => break,
            MigrationPhaseV16::Running => {}
            MigrationPhaseV16::Completed => {
                return Err("bounded migration completed without copied-state attestation".into())
            }
            MigrationPhaseV16::UnsupportedSource => {
                return Err("bounded migration entered UnsupportedSource".into())
            }
        }
        if rounds >= max_rounds {
            return Err(
                "bounded migration failed to reach AwaitingVerification within the sparse-ID upper bound"
                    .into(),
            );
        }
        let now = ext.execute_with(System::block_number);
        let used =
            ext.execute_with(|| <EterraTCG as Hooks<BlockNumber>>::on_idle(now, remaining_weight));
        bounded_weight &= used.all_lte(remaining_weight);
        rounds = rounds.saturating_add(1);
    }

    let verification = ext.execute_with(|| {
        let after = capture_legacy();
        let state = pallet_eterra_tcg::TcgMigrationStateStorageV16::<Runtime>::get()
            .expect("awaiting verification keeps its audit state");
        let classifications: BTreeMap<_, _> =
            pallet_eterra_tcg::LegacyCardClassifications::<Runtime>::iter().collect();
        let anomalies: BTreeSet<_> =
            pallet_eterra_tcg::TcgMigrationAnomaliesV16::<Runtime>::iter_keys().collect();

        let mut expected_owner_cards: BTreeMap<AccountId, BTreeSet<u32>> = BTreeMap::new();
        let mut expected_subject_counts: BTreeMap<(AccountId, u32), u32> = BTreeMap::new();
        let mut no_silent_reclassification = true;
        for (card_id, encoded) in &before.cards {
            let card = pallet_eterra_tcg::Cards::<Runtime>::get(card_id)
                .expect("a before card must remain decodable");
            no_silent_reclassification &= card.encode() == *encoded;
            let Some(classification) = classifications.get(card_id) else {
                no_silent_reclassification = false;
                continue;
            };
            no_silent_reclassification &= validate_classification(*card_id, &card, classification);
            if classification.custody == LegacyCustodyKind::UnknownFrozen {
                no_silent_reclassification &= anomalies.contains(card_id);
            }
            if let Some(owner) = classification.beneficial_owner.clone() {
                expected_owner_cards
                    .entry(owner.clone())
                    .or_default()
                    .insert(*card_id);
                if let Some(nexus) =
                    pallet_eterra_tcg::NexusCollectionCards::<Runtime>::get(card_id)
                {
                    *expected_subject_counts
                        .entry((owner, nexus.subject_id))
                        .or_default() += 1;
                }
            }
        }

        let actual_owner_cards: BTreeMap<AccountId, BTreeSet<u32>> =
            pallet_eterra_tcg::RepairedLegacyCardsByOwnerV16::<Runtime>::iter()
                .filter(|(_, _, present)| *present)
                .fold(BTreeMap::new(), |mut owners, (owner, card_id, _)| {
                    owners.entry(owner).or_default().insert(card_id);
                    owners
                });
        let actual_subject_counts: BTreeMap<(AccountId, u32), u32> =
            pallet_eterra_tcg::RepairedLegacySubjectCountsV16::<Runtime>::iter()
                .map(|(owner, subject, count)| ((owner, subject), count))
                .collect();
        let index_checks = verify_nexus_indexes();
        let custody_domain_checks = verify_custody_domains();
        let (attestation, verification_hash) = build_attestation(&after, &state);
        (
            after,
            state,
            classifications,
            anomalies,
            no_silent_reclassification,
            expected_owner_cards == actual_owner_cards,
            expected_subject_counts == actual_subject_counts,
            index_checks,
            custody_domain_checks,
            attestation,
            verification_hash,
        )
    });

    let (
        after,
        awaiting_state,
        classifications,
        anomalies,
        no_silent_reclassification,
        ownership_indexes_match,
        repaired_subject_indexes_match,
        nexus_index_checks,
        custody_domain_checks,
        attestation_evidence,
        verification_hash,
    ) = verification;
    let max_seen = full_card_coverage(
        &after.cards,
        awaiting_state.upper_bound,
        awaiting_state.cards_seen,
    )?;
    if awaiting_state.phase != MigrationPhaseV16::AwaitingVerification
        || awaiting_state.upper_bound != before.next_card_id
        || after.next_card_id != awaiting_state.upper_bound
    {
        return Err("migration did not stop at the expected attestation boundary".into());
    }

    let legacy_state_evidence = LegacyStateEvidence {
        full_card_map_within_upper_bound: true,
        cards_preserved: after.cards == before.cards,
        legacy_owner_indexes_preserved: after.owner_indexes == before.owner_indexes,
        nexus_records_preserved: after.nexus_cards == before.nexus_cards,
        vault_records_preserved: after.vault_variants == before.vault_variants,
        nexus_subject_indexes_preserved: after.nexus_subject_indexes
            == before.nexus_subject_indexes,
        overflow_owner_indexes_preserved: after.overflow_owner_indexes
            == before.overflow_owner_indexes,
        overflow_subject_indexes_preserved: after.overflow_subject_indexes
            == before.overflow_subject_indexes,
        nft_wrapping_records_preserved: after.converted == before.converted,
        nft_auxiliary_records_preserved: after.nft_collection_config
            == before.nft_collection_config
            && after.nft_item_configs == before.nft_item_configs
            && after.nft_account_index == before.nft_account_index,
        external_escrow_records_preserved: after.escrow_entries == before.escrow_entries,
        external_escrow_indexes_preserved: after.escrow_owner_indexes
            == before.escrow_owner_indexes
            && after.escrow_available_count == before.escrow_available_count
            && after.escrow_available_by_index == before.escrow_available_by_index
            && after.escrow_index_by_card == before.escrow_index_by_card,
        external_escrow_assignments_preserved: after.escrow_game_assignments
            == before.escrow_game_assignments,
        game_authority_records_preserved: after.game_authority_games == before.game_authority_games
            && after.game_authority_active_by_player == before.game_authority_active_by_player
            && after.game_authority_expirations == before.game_authority_expirations,
        retired_economy_records_preserved: after.cryptostrike_pending_claims
            == before.cryptostrike_pending_claims
            && after.cryptostrike_servers == before.cryptostrike_servers
            && after.cryptostrike_pending_unstakes == before.cryptostrike_pending_unstakes
            && after.cryptostrike_allowances == before.cryptostrike_allowances
            && after.cryptostrike_session_rosters == before.cryptostrike_session_rosters
            && after.cryptostrike_active_players == before.cryptostrike_active_players,
        lifecycle_quiescent: before.lifecycle_quiescent && after.lifecycle_quiescent,
        retired_economies_quiescent: before.retired_economies_quiescent
            && after.retired_economies_quiescent,
        v2_sidecar_prefixes_absent: before.preexisting_v2_sidecar_records.is_empty()
            && after.preexisting_v2_sidecar_records.is_empty(),
        legacy_owner_indexes_consistent: custody_domain_checks.legacy_owner_indexes_consistent,
        nft_wrapping_domain_consistent: custody_domain_checks.nft_wrapping_domain_consistent
            && after.nft_collection_id == before.nft_collection_id
            && after.nft_collection == before.nft_collection
            && after.nft_items == before.nft_items
            && after.nft_collection_config == before.nft_collection_config
            && after.nft_item_configs == before.nft_item_configs
            && after.nft_account_index == before.nft_account_index,
        external_escrow_domain_consistent: custody_domain_checks.external_escrow_domain_consistent,
        nexus_subject_indexes_consistent: nexus_index_checks.subject_indexes_consistent,
        overflow_owner_indexes_consistent: nexus_index_checks.overflow_owner_indexes_consistent,
        overflow_subject_indexes_consistent: nexus_index_checks.overflow_subject_indexes_consistent,
        vault_links_consistent: nexus_index_checks.vault_links_consistent,
        next_vault_variant_id_monotonic: nexus_index_checks.next_vault_variant_id_monotonic,
    };
    if !legacy_state_evidence.all_passed() {
        return Err("Vault/Overflow or other critical legacy state evidence failed".into());
    }

    let classified_total = u64::from(awaiting_state.ordinary)
        + u64::from(awaiting_state.nft_wrapped)
        + u64::from(awaiting_state.known_escrow)
        + u64::from(awaiting_state.anomalies);
    let no_card_duplicated = classifications.len() == before.cards.len()
        && classified_total == before.cards.len() as u64;
    let anomalies_accounted = anomalies.len() == awaiting_state.anomalies as usize
        && classifications
            .values()
            .filter(|item| item.custody == LegacyCustodyKind::UnknownFrozen)
            .count()
            == anomalies.len();
    if !no_silent_reclassification
        || !ownership_indexes_match
        || !repaired_subject_indexes_match
        || !no_card_duplicated
        || !custody_domain_checks.legacy_owner_indexes_consistent
        || !custody_domain_checks.nft_wrapping_domain_consistent
        || !custody_domain_checks.external_escrow_domain_consistent
        || !anomalies_accounted
    {
        return Err("classification or repaired-index attestation failed".into());
    }

    ext.execute_with(|| {
        EterraTCG::complete_legacy_migration_v16(
            RawOrigin::Root.into(),
            awaiting_state.cards_seen,
            awaiting_state.anomalies,
            verification_hash,
        )
    })
    .map_err(|error| format!("attested call 58 failed: {error:?}"))?;

    let completion = ext.execute_with(|| {
        (
            pallet_eterra_tcg::TcgMigrationStateStorageV16::<Runtime>::get(),
            pallet_eterra_tcg::V16MigrationVerificationHash::<Runtime>::get(),
            pallet_eterra_tcg::LegacyCreationSealedV16::<Runtime>::get(),
            pallet_eterra_tcg::LegacyWritesPausedV16::<Runtime>::get(),
            capture_legacy(),
            all_v2_features(),
        )
    });
    let completed_state = completion
        .0
        .ok_or("migration state missing after attested completion")?;
    if completed_state.phase != MigrationPhaseV16::Completed
        || completion.1 != Some(verification_hash)
        || !completion.2
        || completion.3
        || completion.4.cards != after.cards
        || completion.4.owner_indexes != after.owner_indexes
        || completion.4.nexus_cards != after.nexus_cards
        || completion.4.vault_variants != after.vault_variants
        || completion.4.nexus_subject_indexes != after.nexus_subject_indexes
        || completion.4.overflow_owner_indexes != after.overflow_owner_indexes
        || completion.4.overflow_subject_indexes != after.overflow_subject_indexes
        || completion.4.converted != after.converted
        || completion.4.nft_collection_id != after.nft_collection_id
        || completion.4.nft_collection != after.nft_collection
        || completion.4.nft_items != after.nft_items
        || !extended_domains_preserved(&after, &completion.4)
        || completion.4.escrow_entries != after.escrow_entries
        || completion.4.escrow_owner_indexes != after.escrow_owner_indexes
        || completion.4.escrow_available_count != after.escrow_available_count
        || completion.4.escrow_available_by_index != after.escrow_available_by_index
        || completion.4.escrow_index_by_card != after.escrow_index_by_card
        || completion.4.escrow_game_assignments != after.escrow_game_assignments
        || completion.4.next_card_id != after.next_card_id
        || completion.4.next_vault_variant_id != after.next_vault_variant_id
        || !completion.5.all_disabled()
    {
        return Err(
            "attested call 58 did not complete exactly once with its hash and legacy state intact"
                .into(),
        );
    }

    let safe_exit_evidence = ext.execute_with(run_safe_exit_probes);
    let safe_legacy_exits_preserved = safe_exit_probes_acceptable(&safe_exit_evidence);
    let interrupted_resume_safe = partial_state.is_some()
        && completed_state.from_storage_version == from_storage_version
        && completion.4.cards == before.cards;
    let next_card_id_monotonic = max_seen
        .map(|card_id| completion.4.next_card_id > card_id)
        .unwrap_or(true);

    let result = MigrationResult {
        schema_version: 1,
        kind: "nexus-v2-v14-v16-migration-result",
        release_id: manifest.release_id,
        source_commit: manifest.source_commit,
        snapshot_sha256: sha256(&args.snapshot)?,
        runtime_wasm_sha256: sha256(&args.runtime_wasm)?,
        try_runtime_log_sha256: sha256(&args.try_runtime_log)?,
        from_storage_version,
        to_storage_version: 16,
        migration_phase: "Completed",
        legacy_creation_sealed: completion.2,
        legacy_writes_paused: completion.3,
        v2_features: completion.5.clone(),
        attestation: attestation_report(&attestation_evidence, verification_hash),
        legacy_state_evidence,
        safe_exit_evidence,
        checks: MigrationChecks {
            interrupted_resume_safe,
            no_card_lost: completion.4.cards == before.cards
                && completion.4.nexus_cards == before.nexus_cards
                && completion.4.vault_variants == before.vault_variants,
            no_card_duplicated,
            no_silent_reclassification,
            ownership_indexes_match,
            subject_indexes_match: repaired_subject_indexes_match
                && nexus_index_checks.subject_indexes_consistent
                && nexus_index_checks.overflow_owner_indexes_consistent
                && nexus_index_checks.overflow_subject_indexes_consistent
                && nexus_index_checks.vault_links_consistent,
            custody_domains_match: custody_domain_checks.legacy_owner_indexes_consistent
                && custody_domain_checks.nft_wrapping_domain_consistent
                && custody_domain_checks.external_escrow_domain_consistent,
            lifecycle_quiescent: completion.4.lifecycle_quiescent,
            retired_economies_quiescent: completion.4.retired_economies_quiescent,
            v2_sidecar_prefixes_absent: completion.4.preexisting_v2_sidecar_records.is_empty(),
            anomalies_accounted,
            next_card_id_monotonic,
            safe_legacy_exits_preserved,
            v2_writes_remain_paused: completion.5.all_disabled(),
            bounded_batch_weight_respected: bounded_weight,
        },
        counts: MigrationCounts {
            legacy_cards_before: before.cards.len() as u64,
            legacy_cards_after: completion.4.cards.len() as u64,
            cards_seen: completed_state.cards_seen,
            ordinary: completed_state.ordinary,
            nft_wrapped: completed_state.nft_wrapped,
            known_escrow: completed_state.known_escrow,
            anomalies: completed_state.anomalies,
            next_card_id: completion.4.next_card_id,
            max_card_id_seen: max_seen,
        },
    };

    if !result.legacy_creation_sealed
        || result.legacy_writes_paused
        || result.counts.cards_seen as u64 != result.counts.legacy_cards_before
        || !result.checks.interrupted_resume_safe
        || !result.checks.no_card_lost
        || !result.checks.no_card_duplicated
        || !result.checks.no_silent_reclassification
        || !result.checks.ownership_indexes_match
        || !result.checks.subject_indexes_match
        || !result.checks.custody_domains_match
        || !result.checks.lifecycle_quiescent
        || !result.checks.retired_economies_quiescent
        || !result.checks.v2_sidecar_prefixes_absent
        || !result.checks.anomalies_accounted
        || !result.checks.next_card_id_monotonic
        || !result.checks.safe_legacy_exits_preserved
        || !result.checks.v2_writes_remain_paused
        || !result.checks.bounded_batch_weight_respected
    {
        return Err("one or more migration invariants failed".into());
    }

    fs::write(&args.result, serde_json::to_vec_pretty(&result)?)?;
    println!(
        "verified attested V14→V16 copied-state migration: cards={} anomalies={} rounds={} attestation={}",
        result.counts.cards_seen,
        result.counts.anomalies,
        rounds,
        result.attestation.verification_hash,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_domain_hash_is_order_independent_and_domain_separated() {
        let left =
            canonical_domain_evidence(b"cards", [b"record-b".to_vec(), b"record-a".to_vec()]);
        let right =
            canonical_domain_evidence(b"cards", [b"record-a".to_vec(), b"record-b".to_vec()]);
        let other_domain =
            canonical_domain_evidence(b"vault", [b"record-a".to_vec(), b"record-b".to_vec()]);
        assert_eq!(left, right);
        assert_ne!(left.sha256, other_domain.sha256);
    }

    #[test]
    fn full_card_scan_rejects_out_of_bound_keys_and_count_mismatch() {
        let cards = BTreeMap::from([(0, vec![1]), (2, vec![2])]);
        assert_eq!(full_card_coverage(&cards, 3, 2), Ok(Some(2)));
        assert!(full_card_coverage(&cards, 2, 2)
            .unwrap_err()
            .contains("outside migration upper bound"));
        assert!(full_card_coverage(&cards, 3, 1)
            .unwrap_err()
            .contains("full Cards map contains 2"));
    }

    #[test]
    fn raw_sidecar_prefix_scan_detects_only_the_exact_pallet_prefix() {
        sp_io::TestExternalities::default().execute_with(|| {
            let mut target_key = sp_io::hashing::twox_128(b"EterraGameResults").to_vec();
            target_key.extend(sp_io::hashing::twox_128(b"Sessions"));
            sp_io::storage::set(&target_key, b"present");

            let mut neighbor_key = sp_io::hashing::twox_128(b"EterraGameResultz").to_vec();
            neighbor_key.extend(sp_io::hashing::twox_128(b"Sessions"));
            sp_io::storage::set(&neighbor_key, b"neighbor");

            assert_eq!(pallet_prefix_records(b"EterraGameResults").len(), 1);
            assert!(pallet_prefix_records(b"EterraRandomness").is_empty());
        });
    }

    #[test]
    fn unavailable_safe_exit_is_explicit_but_failure_is_not_accepted() {
        let unavailable = SafeExitEvidence {
            path: "nftUnwrap",
            status: SafeExitStatus::NotPresent,
            candidate_card_id: None,
            detail: "no candidate".into(),
        };
        assert!(safe_exit_probes_acceptable(&[unavailable]));

        let failed = SafeExitEvidence {
            path: "ordinaryTransfer",
            status: SafeExitStatus::Failed,
            candidate_card_id: Some(1),
            detail: "dispatch failed".into(),
        };
        assert!(!safe_exit_probes_acceptable(&[failed]));

        let blocked = SafeExitEvidence {
            path: "vaultTransfer",
            status: SafeExitStatus::Blocked,
            candidate_card_id: Some(2),
            detail: "quarantined".into(),
        };
        assert!(!safe_exit_probes_acceptable(&[blocked]));
    }
}
