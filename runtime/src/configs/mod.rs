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
use alloc::vec::Vec;
use frame_support::PalletId;
use frame_support::{
    derive_impl,
    dispatch::DispatchResult,
    parameter_types,
    traits::{
        fungibles::{
            metadata::Inspect as FungiblesMetadataInspect, Inspect as FungiblesInspect,
            Mutate as FungiblesMutate,
        },
        tokens::{Fortitude, Precision, Preservation},
        ConstBool, ConstU128, ConstU16, ConstU32, ConstU64, ConstU8, Contains, Currency,
        ExistenceRequirement, ReservableCurrency, VariantCountOf, WithdrawReasons,
    },
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
    traits::{AccountIdConversion, Hash as HashT, Morph, One},
    DispatchError, Perbill, Permill,
};
use sp_version::RuntimeVersion;

// Bring in UNIT and HandProviderAdapter from the parent module (lib.rs)
use super::{HandProviderAdapter, UNIT};

// Bring in the pallets re-exported in lib.rs
use super::{
    pallet_alpha_access, pallet_cryptostrike, pallet_eterra, pallet_eterra_arcade_aegis_run,
    pallet_eterra_arcade_core, pallet_eterra_arcade_nova_rail, pallet_eterra_arcade_ouro,
    pallet_eterra_authority, pallet_eterra_card_escrow, pallet_eterra_creatures,
    pallet_eterra_daily_slots, pallet_eterra_economy, pallet_eterra_faucet, pallet_eterra_flow,
    pallet_eterra_game_authority, pallet_eterra_game_results, pallet_eterra_gamer,
    pallet_eterra_magic, pallet_eterra_media, pallet_eterra_profile, pallet_eterra_randomness,
    pallet_eterra_seasons, pallet_eterra_simple_matchmaker, pallet_eterra_tcg, pallet_nfts,
};
// Monte Carlo AI pallet lives at the crate root; bring it in explicitly.

// Local module imports
use super::{
    AccountId, Assets, Aura, Balance, Balances, Block, BlockNumber, Council, EterraGamer, Hash,
    Nonce, PalletInfo, Runtime, RuntimeCall, RuntimeEvent, RuntimeFreezeReason, RuntimeHoldReason,
    RuntimeOrigin, RuntimeTask, Signature, System, Timestamp, DAYS, EXISTENTIAL_DEPOSIT, HOURS,
    SLOT_DURATION, VERSION,
};

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

// Runtime privileged-origin policy:
// centralized owner-control in both default and production modes.
// This alias can be switched to governance origins when governance is introduced.
type PrivilegedControlOrigin = frame_system::EnsureRoot<AccountId>;

pub struct CryptoStrikeNativeGuapLedger;

impl pallet_cryptostrike::GuapLedger<AccountId, Balance> for CryptoStrikeNativeGuapLedger {
    fn mint(account: &AccountId, amount: Balance) -> DispatchResult {
        let _imbalance = <Balances as Currency<AccountId>>::deposit_creating(account, amount);
        Ok(())
    }

    fn burn(account: &AccountId, amount: Balance) -> DispatchResult {
        <Balances as Currency<AccountId>>::withdraw(
            account,
            amount,
            WithdrawReasons::TRANSFER,
            ExistenceRequirement::AllowDeath,
        )
        .map(|_imbalance| ())
    }

    fn transfer(from: &AccountId, to: &AccountId, amount: Balance) -> DispatchResult {
        <Balances as Currency<AccountId>>::transfer(
            from,
            to,
            amount,
            ExistenceRequirement::AllowDeath,
        )
    }
}

pub struct CryptoStrikeNativeStakeLedger;

impl pallet_cryptostrike::StakeLedger<AccountId, Balance> for CryptoStrikeNativeStakeLedger {
    fn reserve(account: &AccountId, amount: Balance) -> DispatchResult {
        <Balances as ReservableCurrency<AccountId>>::reserve(account, amount)
    }

    fn release(account: &AccountId, amount: Balance) -> DispatchResult {
        let remaining = <Balances as ReservableCurrency<AccountId>>::unreserve(account, amount);
        if remaining == 0 {
            Ok(())
        } else {
            Err(DispatchError::Other("cryptostrike stake release underflow"))
        }
    }

    fn slash_reserved(account: &AccountId, amount: Balance) -> DispatchResult {
        let (_imbalance, remaining) =
            <Balances as ReservableCurrency<AccountId>>::slash_reserved(account, amount);
        if remaining == 0 {
            Ok(())
        } else {
            Err(DispatchError::Other("cryptostrike stake slash underflow"))
        }
    }
}

pub struct CryptoStrikeGamerIdentityProvider;

impl pallet_cryptostrike::SteamIdentityProvider<AccountId> for CryptoStrikeGamerIdentityProvider {
    fn account_for_steam_hash(steam_hash: pallet_cryptostrike::SteamHash) -> Option<AccountId> {
        pallet_eterra_gamer::SteamToAccount::<Runtime>::get(steam_hash)
    }

    fn steam_hash_for_account(account: &AccountId) -> Option<pallet_cryptostrike::SteamHash> {
        pallet_eterra_gamer::AccountToSteam::<Runtime>::get(account)
    }

    fn is_frozen(account: &AccountId) -> bool {
        pallet_eterra_gamer::GamerProfiles::<Runtime>::get(account)
            .map(|profile| profile.frozen)
            .unwrap_or(false)
    }
}

pub struct CryptoStrikeAlphaSignatureVerifier;

impl<Signature> pallet_cryptostrike::ServerSignatureVerifier<Hash, Signature>
    for CryptoStrikeAlphaSignatureVerifier
where
    Signature: AsRef<[u8]>,
{
    fn verify(server_pubkey: &[u8; 32], _payload_hash: &Hash, signature: &Signature) -> bool {
        if server_pubkey.iter().all(|byte| *byte == 0) {
            return false;
        }

        #[cfg(feature = "runtime-production")]
        {
            let _ = signature;
            false
        }

        #[cfg(not(feature = "runtime-production"))]
        {
            signature.as_ref().starts_with(b"dev-v1:")
        }
    }
}

pub struct EterraFlowAuthorityProvider;

impl pallet_eterra_flow::AuthorityProvider<AccountId> for EterraFlowAuthorityProvider {
    fn resolve_authority(
        account: &AccountId,
        game_id: pallet_eterra_flow::GameId,
        version_id: Option<pallet_eterra_flow::VersionId>,
        event_type: pallet_eterra_flow::EventTypeId,
    ) -> Option<pallet_eterra_flow::AuthorityId> {
        pallet_eterra_authority::Pallet::<Runtime>::resolve_authority(
            account, game_id, version_id, event_type,
        )
    }
}

#[cfg(feature = "runtime-benchmarks")]
pub struct EterraFlowBenchmarkAuthorityProvider;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_eterra_flow::BenchmarkAuthorityProvider<AccountId>
    for EterraFlowBenchmarkAuthorityProvider
{
    fn authorize(
        account: &AccountId,
        game_id: pallet_eterra_flow::GameId,
        version_id: pallet_eterra_flow::VersionId,
        event_type: pallet_eterra_flow::EventTypeId,
    ) -> DispatchResult {
        let mut allowed_events: frame_support::BoundedVec<
            _,
            EterraAuthorityMaxAllowedEventsPerAuthority,
        > = frame_support::BoundedVec::default();
        allowed_events
            .try_push(event_type)
            .expect("benchmark event list fits");
        pallet_eterra_authority::Pallet::<Runtime>::authorize_authority(
            RuntimeOrigin::root(),
            game_id,
            1,
            account.clone(),
            pallet_eterra_authority::AuthorityKind::GameServer,
            Some(version_id),
            allowed_events,
            None,
            <<Runtime as frame_system::Config>::Hashing as sp_runtime::traits::Hash>::hash(
                b"eterra-flow-benchmark-authority",
            ),
        )
    }
}

pub struct EterraFlowEconomyProvider;

impl pallet_eterra_flow::EconomyProvider<AccountId> for EterraFlowEconomyProvider {
    fn has_entitlement(
        account: &AccountId,
        game_id: pallet_eterra_flow::GameId,
        entitlement_id: pallet_eterra_flow::EntitlementId,
    ) -> bool {
        pallet_eterra_economy::Pallet::<Runtime>::has_entitlement(account, game_id, entitlement_id)
    }

    fn credit_balance(
        account: &AccountId,
        game_id: pallet_eterra_flow::GameId,
        credit_type: pallet_eterra_flow::CreditTypeId,
    ) -> u64 {
        pallet_eterra_economy::Pallet::<Runtime>::credit_balance(account, game_id, credit_type)
    }

    fn consume_credit(
        _account: &AccountId,
        _game_id: pallet_eterra_flow::GameId,
        _credit_type: pallet_eterra_flow::CreditTypeId,
        _amount: u64,
    ) -> DispatchResult {
        Err(DispatchError::Other(
            "Flow economic effects are disabled in Nexus V2 private alpha",
        ))
    }

    fn grant_credit(
        _account: &AccountId,
        _game_id: pallet_eterra_flow::GameId,
        _credit_type: pallet_eterra_flow::CreditTypeId,
        _amount: u64,
    ) -> DispatchResult {
        Err(DispatchError::Other(
            "Flow economic effects are disabled in Nexus V2 private alpha",
        ))
    }

    fn grant_entitlement(
        _account: &AccountId,
        _game_id: pallet_eterra_flow::GameId,
        _entitlement_id: pallet_eterra_flow::EntitlementId,
    ) -> DispatchResult {
        Err(DispatchError::Other(
            "Flow economic effects are disabled in Nexus V2 private alpha",
        ))
    }

    fn revoke_entitlement(
        _account: &AccountId,
        _game_id: pallet_eterra_flow::GameId,
        _entitlement_id: pallet_eterra_flow::EntitlementId,
    ) -> DispatchResult {
        Err(DispatchError::Other(
            "Flow economic effects are disabled in Nexus V2 private alpha",
        ))
    }

    fn spend_sponsor_funds(_game_id: pallet_eterra_flow::GameId, _amount: u128) -> DispatchResult {
        Err(DispatchError::Other(
            "Flow economic effects are disabled in Nexus V2 private alpha",
        ))
    }
}

pub struct EterraArcadeEconomyProvider;

impl pallet_eterra_arcade_core::EconomyProvider<AccountId> for EterraArcadeEconomyProvider {
    fn consume_credit(
        account: &AccountId,
        game_id: pallet_eterra_arcade_core::GameId,
        credit_type: pallet_eterra_arcade_core::CreditTypeId,
        amount: u64,
    ) -> DispatchResult {
        if amount == 0 {
            return Ok(());
        }
        pallet_eterra_economy::Pallet::<Runtime>::try_consume_credit(
            account,
            game_id,
            credit_type,
            amount,
        )
    }

    fn credit_balance(
        account: &AccountId,
        game_id: pallet_eterra_arcade_core::GameId,
        credit_type: pallet_eterra_arcade_core::CreditTypeId,
    ) -> u64 {
        pallet_eterra_economy::Pallet::<Runtime>::credit_balance(account, game_id, credit_type)
    }

    fn grant_gameplay_tickets(
        account: &AccountId,
        game_id: pallet_eterra_arcade_core::GameId,
        ruleset_version: pallet_eterra_arcade_core::RulesetVersion,
        result_id: &[u8],
        score: u64,
        ranked: bool,
        ended_reason: u8,
    ) -> DispatchResult {
        let result_id_hash = <Runtime as frame_system::Config>::Hashing::hash(result_id);
        pallet_eterra_economy::Pallet::<Runtime>::try_grant_gameplay_tickets(
            account,
            game_id,
            ruleset_version,
            result_id_hash,
            score,
            ranked,
            ended_reason,
        )
        .map(|_| ())
    }
}

pub struct EterraTicketAssetProvider;

impl pallet_eterra_economy::TicketAssetProvider<AccountId> for EterraTicketAssetProvider {
    fn asset_exists(asset_id: u32) -> bool {
        <Assets as FungiblesInspect<AccountId>>::asset_exists(asset_id)
    }

    fn decimals(asset_id: u32) -> u8 {
        <Assets as FungiblesMetadataInspect<AccountId>>::decimals(asset_id)
    }

    fn balance(asset_id: u32, account: &AccountId) -> u128 {
        <Assets as FungiblesInspect<AccountId>>::balance(asset_id, account)
    }

    fn mint(asset_id: u32, account: &AccountId, amount: u128) -> DispatchResult {
        <Assets as FungiblesMutate<AccountId>>::mint_into(asset_id, account, amount).map(|_| ())
    }

    fn burn(asset_id: u32, account: &AccountId, amount: u128) -> DispatchResult {
        <Assets as FungiblesMutate<AccountId>>::burn_from(
            asset_id,
            account,
            amount,
            Preservation::Expendable,
            Precision::Exact,
            Fortitude::Polite,
        )
        .map(|_| ())
    }

    fn transfer(asset_id: u32, from: &AccountId, to: &AccountId, amount: u128) -> DispatchResult {
        <Assets as FungiblesMutate<AccountId>>::transfer(
            asset_id,
            from,
            to,
            amount,
            Preservation::Expendable,
        )
        .map(|_| ())
    }
}

pub struct EterraNativePaymentProvider;

impl pallet_eterra_economy::NativePaymentProvider<AccountId> for EterraNativePaymentProvider {
    fn pay_treasury(account: &AccountId, amount: u128) -> DispatchResult {
        <Balances as Currency<AccountId>>::transfer(
            account,
            &TreasuryAccount::get(),
            amount,
            ExistenceRequirement::KeepAlive,
        )
    }
}

pub struct EterraArcadeAccountEligibility;

impl pallet_eterra_economy::AccountEligibilityProvider<AccountId>
    for EterraArcadeAccountEligibility
{
    fn eligible(account: &AccountId) -> bool {
        <super::AlphaAccess as pallet_alpha_access::AccessControl<AccountId>>::ensure_whitelisted(
            account,
        )
        .is_ok()
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn prepare_benchmark_account(_account: &AccountId) {
        pallet_alpha_access::AccessMode::<Runtime>::put(pallet_alpha_access::GateMode::Open);
    }
}

pub struct EterraArcadeRandomness;

impl pallet_eterra_economy::ArcadeRandomnessProvider for EterraArcadeRandomness {
    fn random(domain: &[u8], payload: &[u8]) -> [u8; 32] {
        let mut input = Vec::with_capacity(domain.len() + payload.len() + 32);
        input.extend_from_slice(domain);
        input.extend_from_slice(payload);
        input.extend_from_slice(System::parent_hash().as_ref());
        sp_io::hashing::blake2_256(&input)
    }
}

pub struct EterraPrizeFulfillmentProvider;

impl pallet_eterra_economy::PrizeFulfillmentProvider<AccountId> for EterraPrizeFulfillmentProvider {
    fn validate_pool(pool_id: u32, featured_subjects: &[u32]) -> DispatchResult {
        let pool = pallet_eterra_tcg::NexusPrizePools::<Runtime>::get(pool_id)
            .ok_or(pallet_eterra_tcg::Error::<Runtime>::NexusPrizePoolMissing)?;
        for subject_id in featured_subjects {
            if !pool
                .templates
                .iter()
                .any(|template| template.card.subject_id == *subject_id)
            {
                return Err(
                    pallet_eterra_tcg::Error::<Runtime>::NexusPrizeSubjectUnavailable.into(),
                );
            }
        }
        Ok(())
    }

    fn fulfill(
        account: &AccountId,
        kind: pallet_eterra_economy::PrizeFulfillmentKind,
        pool_id: u32,
        subject_id: Option<u32>,
        entropy: [u8; 32],
        source: pallet_eterra_economy::PrizeAcquisitionSource,
    ) -> Result<Vec<u32>, DispatchError> {
        let kind = match kind {
            pallet_eterra_economy::PrizeFulfillmentKind::RandomSingle => {
                pallet_eterra_tcg::NexusPrizeKind::RandomSingle
            }
            pallet_eterra_economy::PrizeFulfillmentKind::RandomPack => {
                pallet_eterra_tcg::NexusPrizeKind::RandomPack
            }
            pallet_eterra_economy::PrizeFulfillmentKind::FeaturedSubject => {
                pallet_eterra_tcg::NexusPrizeKind::FeaturedSubject
            }
        };
        let origin = match source {
            pallet_eterra_economy::PrizeAcquisitionSource::TicketClaim => {
                pallet_eterra_tcg::NexusCardOrigin::Claim
            }
            pallet_eterra_economy::PrizeAcquisitionSource::NativePull => {
                pallet_eterra_tcg::NexusCardOrigin::Pull
            }
        };
        pallet_eterra_tcg::Pallet::<Runtime>::try_fulfill_nexus_prize(
            account, kind, pool_id, subject_id, entropy, origin,
        )
    }
}

pub struct EterraArcadePackCreditIssuer;

impl pallet_eterra_economy::V2PackCreditIssuer<AccountId> for EterraArcadePackCreditIssuer {
    fn validate_target(
        pack_sku: u32,
        sku_version: u32,
        realm: eterra_nexus_primitives::EconomicRealm,
    ) -> DispatchResult {
        if realm != eterra_nexus_primitives::EconomicRealm::Training {
            return Err(
                pallet_eterra_tcg::Error::<Runtime>::V2ProductionAlphaIssuanceDisabled.into(),
            );
        }
        if !pallet_eterra_tcg::PackSkuVersionsV2::<Runtime>::contains_key((pack_sku, sku_version)) {
            return Err(pallet_eterra_tcg::Error::<Runtime>::V2PackSkuMissing.into());
        }
        Ok(())
    }

    fn issue_pack_credit(
        owner: &AccountId,
        pack_sku: u32,
        sku_version: u32,
        realm: eterra_nexus_primitives::EconomicRealm,
        source: eterra_nexus_primitives::PackCreditSource,
    ) -> DispatchResult {
        <super::EterraTCG as pallet_eterra_tcg::V2PackCreditManager<AccountId>>::issue_credit(
            owner,
            pack_sku,
            sku_version,
            realm,
            source,
        )
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn prepare_benchmark_target(
        pack_sku: u32,
        sku_version: u32,
        _realm: eterra_nexus_primitives::EconomicRealm,
    ) {
        pallet_eterra_tcg::PackSkuVersionsV2::<Runtime>::insert(
            (pack_sku, sku_version),
            eterra_nexus_primitives::PackSkuVersion {
                pack_sku,
                version: sku_version,
                card_count: eterra_nexus_primitives::PACK_CARD_COUNT,
                set_id: 1,
                pool_id: 1,
                pool_version: 1,
                rarity_weights: [6_800, 2_200, 750, 200, 50],
                discovery_policy: eterra_nexus_primitives::DiscoveryPolicy::Standard,
                odds_metadata_hash: [0u8; 32],
                immutable_config_hash: [0u8; 32],
                active_from: 0,
                active_until: None,
            },
        );
    }
}

pub struct EterraArcadeAuthorityProvider;

impl pallet_eterra_arcade_core::AuthorityProvider<AccountId> for EterraArcadeAuthorityProvider {
    fn can_submit(
        account: &AccountId,
        game_id: pallet_eterra_arcade_core::GameId,
        ruleset_version: pallet_eterra_arcade_core::RulesetVersion,
        event_type: pallet_eterra_arcade_core::AuthorityEventTypeId,
    ) -> bool {
        pallet_eterra_authority::Pallet::<Runtime>::resolve_authority(
            account,
            game_id,
            Some(ruleset_version),
            event_type,
        )
        .is_some()
    }
}

pub struct EterraFlowProfileProvider;

impl pallet_eterra_flow::ProfileProvider<AccountId> for EterraFlowProfileProvider {
    fn update_passport_counter(
        _account: &AccountId,
        _field_id: pallet_eterra_flow::PassportFieldId,
        _amount: u64,
    ) -> DispatchResult {
        Err(DispatchError::Other(
            "Flow global profile effects are disabled in Nexus V2 private alpha",
        ))
    }

    fn grant_passport_badge(
        _account: &AccountId,
        _badge_id: pallet_eterra_flow::PassportBadgeId,
    ) -> DispatchResult {
        Err(DispatchError::Other(
            "Flow global profile effects are disabled in Nexus V2 private alpha",
        ))
    }

    fn revoke_passport_badge(
        _account: &AccountId,
        _badge_id: pallet_eterra_flow::PassportBadgeId,
    ) -> DispatchResult {
        Err(DispatchError::Other(
            "Flow global profile effects are disabled in Nexus V2 private alpha",
        ))
    }
}

pub struct TcgHandChecker;

impl pallet_eterra_tcg::HandChecker<AccountId> for TcgHandChecker {
    fn is_card_in_current_hand(owner: &AccountId, card_id: u32) -> bool {
        pallet_eterra::CurrentHandOf::<Runtime>::get(owner)
            .map(|hand| hand.contains(&card_id))
            .unwrap_or(false)
    }
}

pub struct TcgProgressionAuthorityProvider;

impl pallet_eterra_tcg::ProgressionAuthorityProvider<AccountId>
    for TcgProgressionAuthorityProvider
{
    fn resolve_authority(
        account: &AccountId,
        game_id: pallet_eterra_tcg::ProgressionGameId,
        version_id: Option<pallet_eterra_tcg::ProgressionVersionId>,
        event_type: pallet_eterra_tcg::ProgressionEventTypeId,
    ) -> Option<pallet_eterra_tcg::ProgressionAuthorityId> {
        pallet_eterra_authority::Pallet::<Runtime>::resolve_authority(
            account, game_id, version_id, event_type,
        )
    }
}

pub struct TcgLegacyEscrowOwnerProvider;

impl pallet_eterra_tcg::LegacyEscrowOwnerProvider<AccountId> for TcgLegacyEscrowOwnerProvider {
    fn beneficial_owner(card_id: u32) -> Option<AccountId> {
        pallet_eterra_card_escrow::EscrowEntries::<Runtime>::get(card_id).map(|entry| entry.owner)
    }

    fn custodian_account() -> Option<AccountId> {
        Some(pallet_eterra_card_escrow::Pallet::<Runtime>::account_id())
    }
}

pub struct TcgSeasonActivationValidator;

impl pallet_eterra_seasons::SeasonActivationValidator<u32> for TcgSeasonActivationValidator {
    fn ensure_can_activate(season_id: u32) -> frame_support::dispatch::DispatchResult {
        pallet_eterra_tcg::Pallet::<Runtime>::ensure_season_ready_for_activation(season_id)
    }
}

pub struct EscrowCardCustodian;

impl pallet_eterra_card_escrow::CardCustodian<AccountId> for EscrowCardCustodian {
    fn move_card_to_escrow(
        owner: &AccountId,
        escrow_account: &AccountId,
        card_id: u32,
    ) -> Result<pallet_eterra_card_escrow::CardGenomeHash, sp_runtime::DispatchError> {
        pallet_eterra_tcg::Pallet::<Runtime>::move_card_to_external_escrow(
            owner,
            escrow_account,
            card_id,
        )
    }

    fn move_card_from_escrow(
        escrow_account: &AccountId,
        owner: &AccountId,
        card_id: u32,
    ) -> DispatchResult {
        pallet_eterra_tcg::Pallet::<Runtime>::move_card_from_external_escrow(
            escrow_account,
            owner,
            card_id,
        )
    }
}

pub struct EscrowGameAuthorityAdapter;

impl pallet_eterra_card_escrow::GameAuthority<AccountId> for EscrowGameAuthorityAdapter {
    fn ensure_game_owned_by(game_id: u64, caller: &AccountId) -> DispatchResult {
        pallet_eterra_game_authority::Pallet::<Runtime>::ensure_game_owned_by(game_id, caller)
    }

    fn ensure_active_game_owned_by(game_id: u64, caller: &AccountId) -> DispatchResult {
        pallet_eterra_game_authority::Pallet::<Runtime>::ensure_active_game_owned_by(
            game_id, caller,
        )
    }

    fn ensure_player_in_game(game_id: u64, player: &AccountId) -> DispatchResult {
        pallet_eterra_game_authority::Pallet::<Runtime>::ensure_player_in_game(game_id, player)
    }
}

pub struct EscrowGameLifecycleHooks;

impl pallet_eterra_game_authority::GameLifecycleHooks<AccountId> for EscrowGameLifecycleHooks {
    fn on_game_created(
        game_id: pallet_eterra_game_authority::GameId,
        _server: &AccountId,
        _players: &[AccountId],
    ) -> DispatchResult {
        if pallet_eterra_tcg::LegacyWritesPausedV16::<Runtime>::get() {
            return Err(DispatchError::Other(
                "legacy game creation is paused during TCG V16 migration",
            ));
        }
        pallet_eterra_card_escrow::Pallet::<Runtime>::handle_game_created(game_id)
    }

    fn on_game_ended(
        game_id: pallet_eterra_game_authority::GameId,
        _server: &AccountId,
        _players: &[AccountId],
    ) {
        pallet_eterra_card_escrow::Pallet::<Runtime>::handle_game_ended(game_id);
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
    type BaseCallFilter = EterraRuntimeCallFilter;
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

pub struct EterraRuntimeCallFilter;

impl Contains<RuntimeCall> for EterraRuntimeCallFilter {
    fn contains(call: &RuntimeCall) -> bool {
        // Flow V0 remains byte/storage compatible at pallet 29, but its
        // current public authoring surface has zero-deposit state growth and
        // legacy Economy/Profile effects. The extracted Blockchainia product
        // and builder remain testable; Eterra private alpha keeps the adapter
        // read-only until bounded admission and benchmarked weights ship.
        if matches!(call, RuntimeCall::EterraFlow(_)) {
            return false;
        }

        // Legacy Media lets any signer create zero-deposit collections and
        // append unbounded records. Nexus V2 uses immutable, hash-pinned media
        // manifests and TCG catalog definitions instead.
        if matches!(call, RuntimeCall::EterraMedia(_)) {
            return false;
        }

        // The legacy ring-buffer matchmaker can accumulate tombstones and
        // execute a 1,024-slot scan behind constant weights. Unity V2 uses the
        // authority/session service, so retire this public pallet surface.
        if matches!(call, RuntimeCall::EterraSimpleMatchMaker(_)) {
            return false;
        }

        // The legacy CryptoStrike economy can mint native balances from
        // server-authored settlements without the bounded V2 reward budget.
        // Keep only owner-controlled recovery exits for preexisting stake and
        // allowance state; all issuance, registration and gameplay writes are
        // fail-closed.
        if let RuntimeCall::CryptoStrike(inner) = call {
            return matches!(
                inner,
                pallet_cryptostrike::Call::request_unstake { .. }
                    | pallet_cryptostrike::Call::finalize_unstake { .. }
                    | pallet_cryptostrike::Call::revoke_server_allowance { .. }
            );
        }

        // The iPhone vertical slice admits only bounded, economically
        // valueless training calls. A production runtime continues to reject
        // them at the outermost dispatch boundary even if a caller or remote
        // flag attempts to enable them.
        #[cfg(feature = "runtime-production")]
        if matches!(
            call,
            RuntimeCall::EterraFaucet(pallet_eterra_faucet::Call::claim { .. })
                | RuntimeCall::EterraEconomy(
                    pallet_eterra_economy::Call::claim_arcade_credit { .. }
                )
                | RuntimeCall::EterraArcadeCore(
                    pallet_eterra_arcade_core::Call::start_run { .. }
                        | pallet_eterra_arcade_core::Call::pay_continue { .. }
                )
                | RuntimeCall::EterraArcadeNovaRail(
                    pallet_eterra_arcade_nova_rail::Call::pay_continue { .. }
                )
                | RuntimeCall::EterraDailySlots(pallet_eterra_daily_slots::Call::roll { .. })
        ) {
            return false;
        }

        // Paid, transferable, prize-redemption, and marketplace surfaces stay
        // disabled in both training and production runtimes.
        if matches!(
            call,
            RuntimeCall::EterraTCG(
                pallet_eterra_tcg::Call::set_price { .. }
                    | pallet_eterra_tcg::Call::buy_card { .. }
                    | pallet_eterra_tcg::Call::buy_card_capacity { .. }
            ) | RuntimeCall::EterraEconomy(
                pallet_eterra_economy::Call::consume_credit { .. }
                    | pallet_eterra_economy::Call::fulfill_product { .. }
                    | pallet_eterra_economy::Call::transfer_tickets { .. }
                    | pallet_eterra_economy::Call::redeem_prize_with_tickets { .. }
                    | pallet_eterra_economy::Call::purchase_prize_with_native { .. }
            )
        ) {
            return false;
        }

        // Legacy extraction rewards are economically valueless in private
        // alpha. Keep custody deposits/withdrawals available, but reject both
        // native-mint reward extrinsics.
        if matches!(
            call,
            RuntimeCall::EterraCardEscrow(
                pallet_eterra_card_escrow::Call::record_enemy_defeat_with_event_id { .. }
                    | pallet_eterra_card_escrow::Call::record_enemy_elimination_with_event_id { .. }
            )
        ) {
            return false;
        }

        // The legacy GameAuthority/CardEscrow FPS lane is retired. Preserve
        // end-game recovery, deposits and withdrawals, but do not admit new
        // games or elimination writes after the V2 cutover.
        if matches!(
            call,
            RuntimeCall::EterraGameAuthority(
                pallet_eterra_game_authority::Call::create_game_with_round_id { .. }
                    | pallet_eterra_game_authority::Call::record_eliminations_with_event_id { .. }
            )
        ) {
            return false;
        }

        // Upstream pallet-node-authorization 38 exposes unbounded public
        // connection vectors behind flat weights. Eterra only needs the four
        // governance-managed well-known-node calls, so claims and connection
        // mutation remain fail-closed at the runtime boundary.
        if matches!(
            call,
            RuntimeCall::NodeAuthorization(
                pallet_node_authorization::Call::claim_node { .. }
                    | pallet_node_authorization::Call::remove_claim { .. }
                    | pallet_node_authorization::Call::transfer_node { .. }
                    | pallet_node_authorization::Call::add_connections { .. }
                    | pallet_node_authorization::Call::remove_connections { .. }
            )
        ) {
            return false;
        }

        // pallet-nfts is configured with zero storage deposits. Its complete
        // public call surface is disabled to prevent free state growth,
        // transfers, markets, or custody-index bypass. TCG uses custody-aware
        // internal `do_create`/`do_mint` and call 59 for wrapped transfers.
        // Governance retains explicit `dispatch_bypass_filter` recovery.
        if matches!(call, RuntimeCall::Nfts(_)) {
            return false;
        }

        match call {
            RuntimeCall::Assets(pallet_assets::Call::transfer { id, .. })
            | RuntimeCall::Assets(pallet_assets::Call::transfer_keep_alive { id, .. })
            | RuntimeCall::Assets(pallet_assets::Call::force_transfer { id, .. })
            | RuntimeCall::Assets(pallet_assets::Call::approve_transfer { id, .. })
            | RuntimeCall::Assets(pallet_assets::Call::cancel_approval { id, .. })
            | RuntimeCall::Assets(pallet_assets::Call::transfer_approved { id, .. })
            | RuntimeCall::Assets(pallet_assets::Call::transfer_all { id, .. }) => {
                // Raw fungible transfers are disabled for private alpha. The
                // Economy pallet may still mint/burn valueless Tickets through
                // its internal typed adapter. Avoid a dynamic TicketAsset
                // storage read in BaseCallFilter, which would otherwise be
                // uncharged on every dispatch.
                let _ = id;
                false
            }
            _ => true,
        }
    }
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

impl pallet_transaction_payment::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnChargeTransaction = pallet_transaction_payment::FungibleAdapter<Balances, ()>;
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
    fn create_asset_kind(_: u32) {}

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

    type WeightInfo = EterraNodeAuthorizationWeights;
}

pub struct EterraNodeAuthorizationWeights;

impl pallet_node_authorization::WeightInfo for EterraNodeAuthorizationWeights {
    fn add_well_known_node() -> Weight {
        Weight::from_parts(100_000_000_000, 65_536)
            .saturating_add(RocksDbWeight::get().reads(1))
            .saturating_add(RocksDbWeight::get().writes(2))
    }

    fn remove_well_known_node() -> Weight {
        Weight::from_parts(100_000_000_000, 65_536)
            .saturating_add(RocksDbWeight::get().reads(1))
            .saturating_add(RocksDbWeight::get().writes(3))
    }

    fn swap_well_known_node() -> Weight {
        Weight::from_parts(150_000_000_000, 131_072)
            .saturating_add(RocksDbWeight::get().reads(3))
            .saturating_add(RocksDbWeight::get().writes(5))
    }

    fn reset_well_known_nodes() -> Weight {
        // Fixed worst-case bound for fewer than MaxWellKnownNodes (128).
        Weight::from_parts(1_000_000_000_000, 4_000_000)
            .saturating_add(RocksDbWeight::get().reads(1))
            .saturating_add(RocksDbWeight::get().writes(129))
    }

    fn claim_node() -> Weight {
        Weight::MAX
    }

    fn remove_claim() -> Weight {
        Weight::MAX
    }

    fn transfer_node() -> Weight {
        Weight::MAX
    }

    fn add_connections() -> Weight {
        Weight::MAX
    }

    fn remove_connections() -> Weight {
        Weight::MAX
    }
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
    pub RewardPerWinAmount: Balance = 0;

    // `pallet-assets` ids for additional fungible currencies.
    pub const DevCoinAssetId: u32 = 1;
    pub const BetaCoinAssetId: u32 = 2;

    // Per-win rewards for the Eterra game.
    pub EterraWinRewardCoin: Balance = 0;
    pub EterraWinRewardDevCoin: Balance = 0;
    pub EterraWinRewardBetaCoin: Balance = 0;
    pub const EterraWinRewardExperience: u128 = 0;
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
    pub const GamerInitialsMaxLen: u32 = 4;
    pub const AvatarCidMaxLen: u32 = 96; // or 128
    pub const GamerRegionCodeMaxLen: u32 = 2;
    pub const SteamLinkSignatureMaxLen: u32 = 64;
    pub const GamerChangeFee: Balance = 0;
    pub const MaxV2XpGrant: u128 = 1_000_000;
    pub const MaxPackCreditsPerAllocation: u32 = 16;
}

#[cfg(feature = "runtime-benchmarks")]
type V2OwnerAccessControl = ();

#[cfg(not(feature = "runtime-benchmarks"))]
type V2OwnerAccessControl = super::AlphaAccess;

pub struct TcgPackCreditIssuer;

impl pallet_eterra_gamer::PackCreditIssuer<AccountId> for TcgPackCreditIssuer {
    fn issue_pack_credit(
        owner: &AccountId,
        pack_sku: u32,
        sku_version: u32,
        realm: eterra_nexus_primitives::EconomicRealm,
        source: eterra_nexus_primitives::PackCreditSource,
    ) -> DispatchResult {
        <super::EterraTCG as pallet_eterra_tcg::V2PackCreditManager<AccountId>>::issue_credit(
            owner,
            pack_sku,
            sku_version,
            realm,
            source,
        )
    }
}

pub struct TcgPackTrackCatalogPolicy;

impl pallet_eterra_gamer::PackTrackCatalogPolicy for TcgPackTrackCatalogPolicy {
    fn ensure_earned_pack_sku(pack_sku: u32, sku_version: u32) -> DispatchResult {
        let sku = pallet_eterra_tcg::PackSkuVersionsV2::<Runtime>::get((pack_sku, sku_version))
            .ok_or(DispatchError::Other("pack SKU missing"))?;
        if sku.discovery_policy != eterra_nexus_primitives::DiscoveryPolicy::Earned {
            return Err(DispatchError::Other("pack SKU is not Earned"));
        }
        Ok(())
    }
}

#[cfg(feature = "runtime-benchmarks")]
pub struct BenchmarkPackTrackCatalogPolicy;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_eterra_gamer::PackTrackCatalogPolicy for BenchmarkPackTrackCatalogPolicy {
    fn ensure_earned_pack_sku(_pack_sku: u32, _sku_version: u32) -> DispatchResult {
        Ok(())
    }
}

#[cfg(feature = "runtime-benchmarks")]
type RuntimePackTrackCatalogPolicy = BenchmarkPackTrackCatalogPolicy;

#[cfg(not(feature = "runtime-benchmarks"))]
type RuntimePackTrackCatalogPolicy = TcgPackTrackCatalogPolicy;

impl pallet_eterra_gamer::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type AccessControl = V2OwnerAccessControl;
    type ExpIssuerOrigin = PrivilegedControlOrigin;
    type AdminOrigin = PrivilegedControlOrigin;
    type FaucetAccount = TreasuryAccount;
    type ChangeFee = GamerChangeFee;
    type MaxTagLen = GamerTagMaxLen;
    type MaxInitialsLen = GamerInitialsMaxLen;
    type MaxAvatarCidLen = AvatarCidMaxLen;
    type MaxRegionCodeLen = GamerRegionCodeMaxLen;
    type MaxSteamLinkSignatureLen = SteamLinkSignatureMaxLen;
    type PackCreditIssuer = TcgPackCreditIssuer;
    type PackTrackCatalogPolicy = RuntimePackTrackCatalogPolicy;
    type MaxV2XpGrant = MaxV2XpGrant;
    type MaxPackCreditsPerAllocation = MaxPackCreditsPerAllocation;
    type WeightInfo = pallet_eterra_gamer::weights::SubstrateWeight<Runtime>;
}

/// The verifier pins Quicknet's chain hash and public key in no-std code.
/// Activation remains fail-closed behind `CryptographyReviewApproved`; passing
/// interoperability vectors is not a substitute for external review.
pub struct PinnedDrandQuicknetVerifier;

impl pallet_eterra_randomness::DrandProofVerifier for PinnedDrandQuicknetVerifier {
    fn verify_quicknet(
        chain_hash: &eterra_nexus_primitives::Hash32,
        round: u64,
        raw_signature: &[u8],
    ) -> Option<eterra_nexus_primitives::Hash32> {
        if chain_hash != &eterra_drand_quicknet::QUICKNET_CHAIN_HASH {
            return None;
        }
        eterra_drand_quicknet::verify_and_derive(round, raw_signature)
    }
}

parameter_types! {
    pub const RandomnessMinFutureEpochs: u64 = 4;
    pub const RandomnessMinAlphaDelayBlocks: BlockNumber = 2;
    pub const RandomnessRequestTimeoutBlocks: BlockNumber = 7 * DAYS;
    pub const RandomnessBeaconStaleAfterBlocks: BlockNumber = 5;
    pub const RandomnessMaxCheckpointLagRounds: u64 = 10;
    pub const RandomnessMaxSignatureBytes: u32 = 48;
}

pub struct RuntimeRandomnessChainContext;

impl pallet_eterra_randomness::RandomnessChainContextProvider for RuntimeRandomnessChainContext {
    fn genesis_hash() -> eterra_nexus_primitives::Hash32 {
        *System::block_hash(0).as_fixed_bytes()
    }

    fn pallet_instance_id() -> u8 {
        35
    }
}

impl pallet_eterra_randomness::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type AdminOrigin = PrivilegedControlOrigin;
    type ChainContext = RuntimeRandomnessChainContext;
    type DrandVerifier = PinnedDrandQuicknetVerifier;
    type MinFutureEpochs = RandomnessMinFutureEpochs;
    type MinAlphaDelayBlocks = RandomnessMinAlphaDelayBlocks;
    type RequestTimeoutBlocks = RandomnessRequestTimeoutBlocks;
    type BeaconStaleAfterBlocks = RandomnessBeaconStaleAfterBlocks;
    type MaxCheckpointLagRounds = RandomnessMaxCheckpointLagRounds;
    type UnixTime = Timestamp;
    type MaxSignatureBytes = RandomnessMaxSignatureBytes;
    type WeightInfo = pallet_eterra_randomness::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    pub const MaxEntityLearnedMoves: u32 = 12;
    pub const MaxEntityEquippedMoves: u32 = 4;
    pub const MaxEntityProfileMoves: u32 = 48;
    pub const MaxEntityLeagueAllowedMoves: u32 = 48;
    pub const MaxEntityExperienceGrant: u64 = 1_000_000;
}

impl pallet_eterra_creatures::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type AdminOrigin = PrivilegedControlOrigin;
    type ResultOrigin = PrivilegedControlOrigin;
    type AccessControl = V2OwnerAccessControl;
    type Essence = super::EterraMagic;
    type MaxLearnedMoves = MaxEntityLearnedMoves;
    type MaxEquippedMoves = MaxEntityEquippedMoves;
    type MaxProfileMoves = MaxEntityProfileMoves;
    type MaxLeagueAllowedMoves = MaxEntityLeagueAllowedMoves;
    type MaxExperienceGrant = MaxEntityExperienceGrant;
    type WeightInfo = pallet_eterra_creatures::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    pub const MaxChargeDefinitionsPerSession: u32 = 12;
    pub const MaxMagicCraftBatch: u32 = 10;
    pub const MaxPrismXpGrant: u64 = 1_000_000;
}

impl pallet_eterra_magic::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type AdminOrigin = PrivilegedControlOrigin;
    type AccessControl = V2OwnerAccessControl;
    type Currency = Balances;
    type CraftingFeeDestination = TreasuryAccount;
    type ProductionCraftingEnabled = ConstBool<false>;
    type MaxChargeDefinitionsPerSession = MaxChargeDefinitionsPerSession;
    type MaxCraftBatch = MaxMagicCraftBatch;
    type MaxPrismXpGrant = MaxPrismXpGrant;
    type WeightInfo = pallet_eterra_magic::weights::SubstrateWeight<Runtime>;
}

pub struct RuntimeGenesisHashProvider;

impl pallet_eterra_game_results::GenesisHashProvider for RuntimeGenesisHashProvider {
    fn genesis_hash() -> eterra_nexus_primitives::Hash32 {
        *System::block_hash(0).as_fixed_bytes()
    }
}

impl pallet_eterra_tcg::V2ChainDomainProvider for RuntimeGenesisHashProvider {
    fn genesis_hash() -> eterra_nexus_primitives::Hash32 {
        *System::block_hash(0).as_fixed_bytes()
    }
}

pub struct RuntimeResultSignatureVerifier;

impl pallet_eterra_game_results::ServerSignatureVerifier for RuntimeResultSignatureVerifier {
    fn verify(
        public_key: &[u8; 32],
        payload_hash: &eterra_nexus_primitives::Hash32,
        signature: &[u8],
    ) -> bool {
        #[cfg(feature = "runtime-benchmarks")]
        if *public_key == [38; 32]
            && signature.len() == 64
            && signature[..32] == payload_hash[..]
            && signature[32..] == [38; 32]
        {
            // A deterministic benchmark-only witness avoids relying on native
            // keystore/signing APIs in the Wasm benchmark runtime. Production
            // builds do not compile this branch.
            return true;
        }
        let Ok(raw_signature) = <[u8; 64]>::try_from(signature) else {
            return false;
        };
        let signature = sp_core::sr25519::Signature::from_raw(raw_signature);
        let public_key = sp_core::sr25519::Public::from_raw(*public_key);
        sp_io::crypto::sr25519_verify(&signature, payload_hash, &public_key)
    }
}

#[cfg(feature = "runtime-benchmarks")]
pub struct GameResultsBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_eterra_game_results::BenchmarkHelper for GameResultsBenchmarkHelper {
    fn authority_public_key() -> [u8; 32] {
        [38; 32]
    }

    fn sign_result(payload_hash: &eterra_nexus_primitives::Hash32) -> Vec<u8> {
        let mut witness = Vec::with_capacity(64);
        witness.extend_from_slice(payload_hash);
        witness.extend_from_slice(&[38; 32]);
        witness
    }

    fn seed_finalized_randomness(
        request_id: eterra_nexus_primitives::Hash32,
        output: eterra_nexus_primitives::Hash32,
    ) {
        pallet_eterra_randomness::Outputs::<Runtime>::insert(
            request_id,
            pallet_eterra_randomness::VerifiedRandomnessOutput {
                epoch: 1,
                output,
                proof_hash: [39; 32],
                finalized_at: System::block_number(),
                deterministic_alpha: true,
            },
        );
    }

    fn seed_timed_out_randomness(request_id: eterra_nexus_primitives::Hash32) {
        let now = System::block_number();
        pallet_eterra_randomness::Requests::<Runtime>::insert(
            request_id,
            pallet_eterra_randomness::RandomnessRequest {
                request_id,
                domain: [40; 32],
                commitment: [41; 32],
                immutable_config_hash: [42; 32],
                exact_epoch: 1,
                requested_at: now,
                not_before: now,
                timeout_at: now,
                mode: pallet_eterra_randomness::RandomnessMode::DeterministicPrivateAlpha,
                status: pallet_eterra_randomness::RequestStatus::TimedOut,
            },
        );
    }
}

parameter_types! {
    pub const GameResultsPalletInstanceId: u8 = 38;
    pub const MaxSessionEntities: u32 = 3;
    pub const MaxSessionPrisms: u32 = 4;
    pub const MaxSessionChargeDefinitions: u32 = 12;
    pub const MaxResultSignatureBytes: u32 = 64;
    pub const MaxActiveSessionsPerAccount: u32 = 4;
    pub const MaxActiveSessionsPerAuthority: u32 = 64;
    pub const MaxSessionAuthorizationReceiptsPerEpoch: u32 = 256;
    pub const MaxPendingDropsPerAccount: u32 = 4;
    pub const MaxGameSessionLifetime: BlockNumber = 2 * HOURS;
    pub const GameSessionExpiryGrace: BlockNumber = 10;
    pub const ResultEpochSize: u64 = 256;
    pub const MaxResultsPerEpoch: u32 = 1_024;
    pub const ResultDisputeWindow: BlockNumber = 7 * DAYS;
    pub const RewardDayBlocks: u64 = DAYS as u64;
}

impl pallet_eterra_game_results::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type AdminOrigin = PrivilegedControlOrigin;
    type AccessControl = V2OwnerAccessControl;
    type SignatureVerifier = RuntimeResultSignatureVerifier;
    type Entities = super::EterraCreatures;
    type Magic = super::EterraMagic;
    type PlayerProgression = super::EterraGamer;
    type Randomness = super::EterraRandomness;
    type GenesisHashProvider = RuntimeGenesisHashProvider;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = GameResultsBenchmarkHelper;
    type PalletInstanceId = GameResultsPalletInstanceId;
    type MaxSessionEntities = MaxSessionEntities;
    type MaxSessionPrisms = MaxSessionPrisms;
    type MaxChargeDefinitions = MaxSessionChargeDefinitions;
    type MaxSignatureBytes = MaxResultSignatureBytes;
    type MaxActiveSessionsPerAccount = MaxActiveSessionsPerAccount;
    type MaxActiveSessionsPerAuthority = MaxActiveSessionsPerAuthority;
    type MaxSessionAuthorizationReceiptsPerEpoch = MaxSessionAuthorizationReceiptsPerEpoch;
    type MaxPendingDropsPerAccount = MaxPendingDropsPerAccount;
    type MaxSessionLifetime = MaxGameSessionLifetime;
    type ExpiryGrace = GameSessionExpiryGrace;
    type ResultEpochSize = ResultEpochSize;
    type MaxResultsPerEpoch = MaxResultsPerEpoch;
    type ResultDisputeWindow = ResultDisputeWindow;
    type RewardDayBlocks = RewardDayBlocks;
    type WeightInfo = pallet_eterra_game_results::weights::SubstrateWeight<Runtime>;
}

#[cfg(feature = "runtime-benchmarks")]
pub struct MonteCarloBenchHelper;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_eterra_monte_carlo_ai::BenchmarkHelper<eterra_card_ai_adapter::eterra_adapter::Adapter>
    for MonteCarloBenchHelper
{
    fn bench_state() -> eterra_card_ai_adapter::eterra_adapter::State {
        eterra_card_ai_adapter::eterra_adapter::State {
            max_rounds: 1,
            round: 0,
            player_turn: 0,
            ..Default::default()
        }
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
    type AccessControl = super::AlphaAccess;
    type MaxPlayersPerGame = MaxPlayersPerGameConst;
    type MaxRequestIdLen = frame_support::traits::ConstU32<128>;
    type MaxOutcomeLen = frame_support::traits::ConstU32<128>;
    type AdminOrigin = PrivilegedControlOrigin;
    type MaxExpirationsPerBlock = MaxExpirationsPerBlock;
    type MaxScheduledExpirationsPerBlock = ConstU32<8>;
    // If your BlockNumber is u32/u64, set 30 blocks:
    type MaxRoundBlocks = frame_support::traits::ConstU32<30>;
    // or, if BlockNumber is u64:
    // type MaxRoundBlocks = frame_support::traits::ConstU64<30>;

    // Max players that can be added in a single batch to a game
    type MaxBatchAdd = frame_support::traits::ConstU32<32>;
    type GameLifecycleHooks = EscrowGameLifecycleHooks;
    type WeightInfo = pallet_eterra_game_authority::weights::SubstrateWeight<Runtime>;
}

impl pallet_eterra_card_escrow::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type AccessControl = super::AlphaAccess;
    type Currency = Balances;
    type RewardAmount = ConstU128<0>;
    type MaxEscrowedPerOwner = ConstU32<5>;
    type MaxReservedPerGame = ConstU32<5>;
    type MaxEventIdLen = ConstU32<128>;
    type CardCustodian = EscrowCardCustodian;
    type GameAuthority = EscrowGameAuthorityAdapter;
    type WeightInfo = pallet_eterra_card_escrow::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    pub const AlphaAccessMaxRevokeReasonLen: u32 = 128;
}

impl pallet_alpha_access::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type AdminOrigin = PrivilegedControlOrigin;
    type TimeProvider = Timestamp;
    type MaxRevokeReasonLen = AlphaAccessMaxRevokeReasonLen;
    type WeightInfo = pallet_alpha_access::weights::SubstrateWeight<Runtime>;
}

impl pallet_cryptostrike::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type AdminOrigin = PrivilegedControlOrigin;
    type Balance = Balance;
    type MaxSettlementEntries = ConstU32<64>;
    type MaxCombatStatEntries = ConstU32<64>;
    type MaxRivalStatEntries = ConstU32<64>;
    type MaxServerSignatureLen = ConstU32<128>;
    type MinServerStake = ConstU128<{ 100_000 * UNIT }>;
    type UnstakeDelay = ConstU32<DAYS>;
    type GuapLedger = CryptoStrikeNativeGuapLedger;
    type StakeLedger = CryptoStrikeNativeStakeLedger;
    type ServerSignatureVerifier = CryptoStrikeAlphaSignatureVerifier;
    type IdentityProvider = CryptoStrikeGamerIdentityProvider;
    type WeightInfo = pallet_cryptostrike::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    pub const EterraAuthorityMaxAllowedEventsPerAuthority: u32 = 32;
}

impl pallet_eterra_authority::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = pallet_eterra_authority::weights::SubstrateWeight<Runtime>;
    type AdminOrigin = PrivilegedControlOrigin;
    type MaxAllowedEventsPerAuthority = EterraAuthorityMaxAllowedEventsPerAuthority;
}

parameter_types! {
    pub const EterraEconomyArcadeCreditFaucetGameId: pallet_eterra_economy::GameId =
        pallet_eterra_arcade_core::ARCADE_CORE_GAME_ID;
    pub const EterraEconomyArcadeCreditFaucetType: pallet_eterra_economy::CreditTypeId =
        pallet_eterra_arcade_core::ARCADE_PLAY_CREDIT_TYPE;
    pub const EterraEconomyArcadeCreditFaucetAmount: u64 = 1000;
    pub const EterraEconomyMaxScoreTiers: u32 = 16;
    pub const EterraEconomyMaxEligibleRewardModes: u32 = 2;
    pub const EterraEconomyMaxEligibleEndedReasons: u32 = 16;
    pub const EterraEconomyMaxFeaturedPoolSubjects: u32 = 128;
    pub const EterraEconomyMaxFeaturedSlots: u32 = 12;
    pub const EterraEconomyFeaturedSlotCount: u32 = 12;
    pub const EterraEconomyMaxPrizeCards: u32 = 6;
}

impl pallet_eterra_economy::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = pallet_eterra_economy::weights::SubstrateWeight<Runtime>;
    type AdminOrigin = PrivilegedControlOrigin;
    type TicketAssets = EterraTicketAssetProvider;
    type NativePayments = EterraNativePaymentProvider;
    type PrizeFulfillment = EterraPrizeFulfillmentProvider;
    type PackCreditIssuer = EterraArcadePackCreditIssuer;
    type AccountEligibility = EterraArcadeAccountEligibility;
    type RandomnessProvider = EterraArcadeRandomness;
    type ArcadeCreditFaucetGameId = EterraEconomyArcadeCreditFaucetGameId;
    type ArcadeCreditFaucetType = EterraEconomyArcadeCreditFaucetType;
    type ArcadeCreditFaucetAmount = EterraEconomyArcadeCreditFaucetAmount;
    type MaxScoreTiers = EterraEconomyMaxScoreTiers;
    type MaxEligibleRewardModes = EterraEconomyMaxEligibleRewardModes;
    type MaxEligibleEndedReasons = EterraEconomyMaxEligibleEndedReasons;
    type MaxFeaturedPoolSubjects = EterraEconomyMaxFeaturedPoolSubjects;
    type MaxFeaturedSlots = EterraEconomyMaxFeaturedSlots;
    type FeaturedSlotCount = EterraEconomyFeaturedSlotCount;
    type MaxPrizeCards = EterraEconomyMaxPrizeCards;
}

impl pallet_eterra_profile::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = pallet_eterra_profile::weights::SubstrateWeight<Runtime>;
    type AdminOrigin = PrivilegedControlOrigin;
}

parameter_types! {
    pub const EterraArcadeMaxSlugLen: u32 = 32;
    pub const EterraArcadeMaxClientRunIdLen: u32 = 128;
    pub const EterraArcadeMaxResultIdLen: u32 = 128;
    pub const EterraArcadeMaxProgressLabelLen: u32 = 128;
    pub const EterraArcadeMaxLeaderboardEntries: u32 = 32;
    pub const EterraArcadeMaxOuroRoomsPerRun: u32 = 256;
    pub const EterraArcadeMaxOuroBossesPerRun: u32 = 64;
    pub const EterraArcadeMaxAegisStagesPerRun: u32 = 32;
    pub const EterraArcadeMaxAegisCheckpointsPerRun: u32 = 256;
    pub const EterraArcadeMaxNovaRailStage: u32 = 64;
    pub const EterraArcadeMaxNovaRailEnemiesDefeated: u32 = 100_000;
    pub const EterraArcadeMaxNovaRailTerrainHits: u32 = 10_000;
}

impl pallet_eterra_arcade_core::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = pallet_eterra_arcade_core::weights::SubstrateWeight<Runtime>;
    type AdminOrigin = PrivilegedControlOrigin;
    type EconomyProvider = EterraArcadeEconomyProvider;
    type AuthorityProvider = EterraArcadeAuthorityProvider;
    type MaxSlugLen = EterraArcadeMaxSlugLen;
    type MaxClientRunIdLen = EterraArcadeMaxClientRunIdLen;
    type MaxResultIdLen = EterraArcadeMaxResultIdLen;
    type MaxProgressLabelLen = EterraArcadeMaxProgressLabelLen;
    type MaxLeaderboardEntries = EterraArcadeMaxLeaderboardEntries;
}

impl pallet_eterra_arcade_ouro::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = pallet_eterra_arcade_ouro::weights::SubstrateWeight<Runtime>;
    type MaxOuroRoomsPerRun = EterraArcadeMaxOuroRoomsPerRun;
    type MaxOuroBossesPerRun = EterraArcadeMaxOuroBossesPerRun;
}

impl pallet_eterra_arcade_aegis_run::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = pallet_eterra_arcade_aegis_run::weights::SubstrateWeight<Runtime>;
    type MaxAegisStagesPerRun = EterraArcadeMaxAegisStagesPerRun;
    type MaxAegisCheckpointsPerRun = EterraArcadeMaxAegisCheckpointsPerRun;
}

impl pallet_eterra_arcade_nova_rail::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = pallet_eterra_arcade_nova_rail::weights::SubstrateWeight<Runtime>;
    type MaxNovaRailStage = EterraArcadeMaxNovaRailStage;
    type MaxNovaRailEnemiesDefeated = EterraArcadeMaxNovaRailEnemiesDefeated;
    type MaxNovaRailTerrainHits = EterraArcadeMaxNovaRailTerrainHits;
}

parameter_types! {
    pub const EterraFlowMaxUriBytes: u32 = 256;
    pub const EterraFlowMaxManifestChunkBytes: u32 = 64 * 1024;
    pub const EterraFlowMaxManifestChunks: u32 = 64;
    pub const EterraFlowMaxManifestBytes: u32 = 4 * 1024 * 1024;
    pub const EterraFlowMaxActionPayloadBytes: u32 = 1024;
    pub const EterraFlowMaxAttestedPayloadBytes: u32 = 4096;
    pub const EterraFlowMaxMachinesPerManifest: u32 = 256;
    pub const EterraFlowMaxStatesPerMachine: u32 = 1024;
    pub const EterraFlowMaxVariablesPerManifest: u32 = 4096;
    pub const EterraFlowMaxActionsPerManifest: u32 = 4096;
    pub const EterraFlowMaxTransitionsPerManifest: u32 = 20_000;
    pub const EterraFlowMaxConditionsPerTransition: u32 = 64;
    pub const EterraFlowMaxConditionClauses: u32 = 64;
    pub const EterraFlowMaxEconomyGateClauses: u32 = 16;
    pub const EterraFlowMaxEffectsPerTransition: u32 = 64;
    pub const EterraFlowMaxEventsPerManifest: u32 = 256;
    pub const EterraFlowMaxAttestedEffectsPerEvent: u32 = 32;
    pub const EterraFlowMaxEventEffectPolicies: u32 = 64;
}

impl pallet_eterra_flow::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = pallet_eterra_flow::weights::SubstrateWeight<Runtime>;
    type AuthorityProvider = EterraFlowAuthorityProvider;
    type EconomyProvider = EterraFlowEconomyProvider;
    type ProfileProvider = EterraFlowProfileProvider;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkAuthorityProvider = EterraFlowBenchmarkAuthorityProvider;
    type MaxUriBytes = EterraFlowMaxUriBytes;
    type MaxManifestChunkBytes = EterraFlowMaxManifestChunkBytes;
    type MaxManifestChunks = EterraFlowMaxManifestChunks;
    type MaxManifestBytes = EterraFlowMaxManifestBytes;
    type MaxActionPayloadBytes = EterraFlowMaxActionPayloadBytes;
    type MaxAttestedPayloadBytes = EterraFlowMaxAttestedPayloadBytes;
    type MaxMachinesPerManifest = EterraFlowMaxMachinesPerManifest;
    type MaxStatesPerMachine = EterraFlowMaxStatesPerMachine;
    type MaxVariablesPerManifest = EterraFlowMaxVariablesPerManifest;
    type MaxActionsPerManifest = EterraFlowMaxActionsPerManifest;
    type MaxTransitionsPerManifest = EterraFlowMaxTransitionsPerManifest;
    type MaxConditionsPerTransition = EterraFlowMaxConditionsPerTransition;
    type MaxConditionClauses = EterraFlowMaxConditionClauses;
    type MaxEconomyGateClauses = EterraFlowMaxEconomyGateClauses;
    type MaxEffectsPerTransition = EterraFlowMaxEffectsPerTransition;
    type MaxEventsPerManifest = EterraFlowMaxEventsPerManifest;
    type MaxAttestedEffectsPerEvent = EterraFlowMaxAttestedEffectsPerEvent;
    type MaxEventEffectPolicies = EterraFlowMaxEventEffectPolicies;
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
    type DrawingsEnabled = ConstBool<false>;
    type WeightInfo = pallet_eterra_daily_slots::weights::SubstrateWeight<Runtime>;
}

impl pallet_eterra_simple_matchmaker::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type AccessControl = super::AlphaAccess;
    type PlayersPerMatch = PlayersPerMatchConst;
    type QueueCapacity = QueueCapacityConst;
    type HandProvider = MatchmakerHandProvider;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHandSeeder = ();
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

#[cfg(feature = "runtime-benchmarks")]
type TcgAccessControl = ();

#[cfg(not(feature = "runtime-benchmarks"))]
type TcgAccessControl = super::AlphaAccess;

#[cfg(feature = "runtime-benchmarks")]
pub struct TcgV2BenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_eterra_tcg::V2BenchmarkHelper for TcgV2BenchmarkHelper {
    fn prepare_randomness() {
        pallet_eterra_randomness::CurrentMode::<Runtime>::put(
            pallet_eterra_randomness::RandomnessMode::DeterministicPrivateAlpha,
        );
    }

    fn seed_finalized_randomness(
        request_id: eterra_nexus_primitives::Hash32,
        output: eterra_nexus_primitives::Hash32,
    ) {
        pallet_eterra_randomness::Outputs::<Runtime>::insert(
            request_id,
            pallet_eterra_randomness::VerifiedRandomnessOutput {
                epoch: 1,
                output,
                proof_hash: [71; 32],
                finalized_at: System::block_number(),
                deterministic_alpha: true,
            },
        );
    }

    fn seed_timed_out_randomness(request_id: eterra_nexus_primitives::Hash32) {
        let now = System::block_number();
        pallet_eterra_randomness::Requests::<Runtime>::insert(
            request_id,
            pallet_eterra_randomness::RandomnessRequest {
                request_id,
                domain: [72; 32],
                commitment: [73; 32],
                immutable_config_hash: [74; 32],
                exact_epoch: 1,
                requested_at: now,
                not_before: now,
                timeout_at: now,
                mode: pallet_eterra_randomness::RandomnessMode::DeterministicPrivateAlpha,
                status: pallet_eterra_randomness::RequestStatus::TimedOut,
            },
        );
    }

    fn prepare_conversion_entity_profile(
        subject_id: u32,
        subject_version: u32,
        rarity: eterra_nexus_primitives::CardRarity,
    ) {
        let profile_id = 50_000u32
            .saturating_add(subject_id.saturating_mul(5))
            .saturating_add(rarity.index() as u32);
        pallet_eterra_creatures::CpLevelCurves::<Runtime>::insert(
            1,
            pallet_eterra_creatures::CpLevelCurve {
                version: 1,
                ratios_bps: [10_000; 50],
                curve_hash: [75; 32],
            },
        );
        pallet_eterra_creatures::EntityProfiles::<Runtime>::insert(
            subject_id,
            rarity,
            eterra_nexus_primitives::EntityProfile {
                profile_id,
                subject_id,
                subject_version,
                rarity,
                role: eterra_nexus_primitives::EntityRole::Hero,
                base_combat_stats: [10; 6],
                base_max_cp: 1_000,
                genetic_cp_span: 600,
                starter_moves: [1, 2],
                formula_version: 1,
                definition_hash: [76; 32],
            },
        );
        pallet_eterra_creatures::EntityProfileActivation::<Runtime>::insert(profile_id, true);
    }
}

impl pallet_eterra_tcg::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type AccessControl = TcgAccessControl;

    type PaymentCurrency = Balances;
    type HandChecker = TcgHandChecker;
    type ProgressionAuthorityProvider = TcgProgressionAuthorityProvider;
    type V2Randomness = super::EterraRandomness;
    type V2ChainDomain = RuntimeGenesisHashProvider;
    #[cfg(feature = "runtime-benchmarks")]
    type V2BenchmarkHelper = TcgV2BenchmarkHelper;
    type V2Entities = super::EterraCreatures;
    type LegacyEscrowOwnerProvider = TcgLegacyEscrowOwnerProvider;
    type PackPrice = ConstU128<{ 500 * UNIT }>;
    type PackPriceReceiver = TreasuryAccount;
    type ProPrice = ConstU128<{ 200 * UNIT }>;
    type ProPriceReceiver = TreasuryAccount;
    type MintCardPrice = ConstU128<{ 100 * UNIT }>;
    type MintCardPriceReceiver = TreasuryAccount;
    type MaxProSpins = ConstU8<5>;
    type MaxAttempts = ConstU8<3>; // Set maximum attempts per card to 3
    type CardsPerPack = ConstU8<6>; // Set number of cards per pack to 6
    type MaxOwnedCards = ConstU32<100000>;
    type BaseCardCapacity = ConstU32<500>;
    type CardCapacityUpgradeAmount = ConstU32<100>;
    type CardCapacityUpgradePrice = ConstU128<{ 100 * UNIT }>;
    type CardCapacityUpgradePriceReceiver = TreasuryAccount;
    type MaxBorders = ConstU32<32>;
    type MaxBackgrounds = ConstU32<32>;
    type MaxSubjects = ConstU32<128>;
    type MaxBacks = ConstU32<32>;
    type MaxPackagingFronts = ConstU32<16>;
    type MaxPackagingBacks = ConstU32<16>;
    type MaxSeasonCollections = ConstU32<32>;
    type MaxSeasonCollectionNameLen = ConstU32<64>;
    type NexusTeamSize = ConstU32<5>;
    type NexusSubjectCopyCap = ConstU32<5>;
    type NexusOverflowTotalCapacity = ConstU32<30>;
    type NexusOverflowPerSubjectCapacity = ConstU32<2>;
    type NexusBaseVaultCapacity = ConstU32<20>;
    type MaxNexusMetadataUriLen = ConstU32<256>;
    type MaxNexusReasonLen = ConstU32<128>;
    type MaxNexusSpellSlotsPerCard = ConstU32<3>;
    type MaxProgressionNodesPerTree = ConstU32<16>;
    type MaxProgressionNodesPerCard = ConstU32<16>;
    type MaxMagicSlotsPerCard = ConstU32<3>;
    type MaxProgressionTrees = ConstU32<64>;
    type CardXpPerLevel = ConstU32<100>;
    type MaxCardXpGrantAmount = ConstU32<500>;
    type MaxNexusMatchPlayers = ConstU32<2>;
    type MaxV2PoolProfiles = ConstU32<400>;
    type MaxV2PoolPoses = ConstU32<256>;
    type MaxV2PoolBackgrounds = ConstU32<32>;
    type MaxV2CreditsPerAccountSku = ConstU32<64>;
    type MaxV2ProtectionBytes = ConstU32<600>;
    type MaxV2TeamSize = ConstU32<6>;
    type V2OperationalCardWarningThreshold = ConstU64<9_000>;
    type V2OperationalCardLimit = ConstU64<10_000>;
    type V16MigrationBatchSize = ConstU32<100>;
    type MinimumActiveCardsAfterConversion = ConstU32<5>;
    type MaxPendingConversionsPerAccount = ConstU32<2>;
    type MythicalAscensionSeasonDurationBlocks = ConstU32<{ 90u32.saturating_mul(DAYS) }>;
    type MythicalAscensionWeekDurationBlocks = ConstU32<{ 7u32.saturating_mul(DAYS) }>;
    type WeightInfo = pallet_eterra_tcg::weights::SubstrateWeight<Runtime>;
}

impl pallet_eterra::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type AccessControl = super::AlphaAccess;
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
    type SeasonActivationValidator = TcgSeasonActivationValidator;
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
    type CreateOrigin =
        frame_support::traits::AsEnsureOriginWithArg<frame_system::EnsureSigned<AccountId>>;
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

impl pallet_utility::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type PalletsOrigin = super::OriginCaller;
    type WeightInfo = pallet_utility::weights::SubstrateWeight<Runtime>;
}
