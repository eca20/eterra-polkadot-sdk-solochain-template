use crate as pallet_eterra_flow;

use frame_support::{
    construct_runtime, parameter_types,
    traits::{ConstU32, ConstU64, Everything},
};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage,
};

pub type AccountId = u64;
type Block = system::mocking::MockBlock<Test>;

construct_runtime!(
    pub enum Test {
        System: system,
        EterraFlow: pallet_eterra_flow,
        EterraAuthority: pallet_eterra_authority,
        EterraEconomy: pallet_eterra_economy,
        EterraProfile: pallet_eterra_profile,
    }
);

parameter_types! {
    pub const BlockHashCount: u64 = 250;
}

pub struct MockTicketAssets;

impl pallet_eterra_economy::TicketAssetProvider<AccountId> for MockTicketAssets {
    fn asset_exists(_asset_id: u32) -> bool {
        true
    }

    fn decimals(_asset_id: u32) -> u8 {
        0
    }

    fn balance(_asset_id: u32, _account: &AccountId) -> u128 {
        0
    }

    fn mint(
        _asset_id: u32,
        _account: &AccountId,
        _amount: u128,
    ) -> frame_support::dispatch::DispatchResult {
        Ok(())
    }

    fn burn(
        _asset_id: u32,
        _account: &AccountId,
        _amount: u128,
    ) -> frame_support::dispatch::DispatchResult {
        Ok(())
    }

    fn transfer(
        _asset_id: u32,
        _from: &AccountId,
        _to: &AccountId,
        _amount: u128,
    ) -> frame_support::dispatch::DispatchResult {
        Ok(())
    }
}

pub struct MockNativePayments;

impl pallet_eterra_economy::NativePaymentProvider<AccountId> for MockNativePayments {
    fn pay_treasury(
        _account: &AccountId,
        _amount: u128,
    ) -> frame_support::dispatch::DispatchResult {
        Ok(())
    }
}

pub struct MockRandomness;

impl pallet_eterra_economy::ArcadeRandomnessProvider for MockRandomness {
    fn random(_domain: &[u8], _payload: &[u8]) -> [u8; 32] {
        [0; 32]
    }
}

impl system::Config for Test {
    type BaseCallFilter = Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type Block = Block;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
    type BlockHashCount = BlockHashCount;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = ();
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    type MaxConsumers = ConstU32<16>;
    type Nonce = u64;
    type SingleBlockMigrations = ();
    type MultiBlockMigrator = ();
    type PreInherents = ();
    type PostInherents = ();
    type PostTransactions = ();
    type RuntimeTask = ();
}

pub struct MockAuthorityProvider;

impl pallet_eterra_flow::AuthorityProvider<AccountId> for MockAuthorityProvider {
    fn resolve_authority(
        account: &AccountId,
        game_id: pallet_eterra_flow::GameId,
        version_id: Option<pallet_eterra_flow::VersionId>,
        event_type: pallet_eterra_flow::EventTypeId,
    ) -> Option<pallet_eterra_flow::AuthorityId> {
        pallet_eterra_authority::Pallet::<Test>::resolve_authority(
            account, game_id, version_id, event_type,
        )
    }
}

#[cfg(feature = "runtime-benchmarks")]
pub struct MockBenchmarkAuthorityProvider;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_eterra_flow::BenchmarkAuthorityProvider<AccountId> for MockBenchmarkAuthorityProvider {
    fn authorize(
        account: &AccountId,
        game_id: pallet_eterra_flow::GameId,
        version_id: pallet_eterra_flow::VersionId,
        event_type: pallet_eterra_flow::EventTypeId,
    ) -> frame_support::dispatch::DispatchResult {
        let allowed_events = frame_support::BoundedVec::try_from(sp_std::vec![event_type])
            .expect("benchmark event list fits");
        pallet_eterra_authority::Pallet::<Test>::authorize_authority(
            RuntimeOrigin::root(),
            game_id,
            1,
            *account,
            pallet_eterra_authority::AuthorityKind::GameServer,
            Some(version_id),
            allowed_events,
            None,
            H256::repeat_byte(7),
        )
    }
}

pub struct MockEconomyProvider;

impl pallet_eterra_flow::EconomyProvider<AccountId> for MockEconomyProvider {
    fn has_entitlement(
        account: &AccountId,
        game_id: pallet_eterra_flow::GameId,
        entitlement_id: pallet_eterra_flow::EntitlementId,
    ) -> bool {
        pallet_eterra_economy::Pallet::<Test>::has_entitlement(account, game_id, entitlement_id)
    }

    fn credit_balance(
        account: &AccountId,
        game_id: pallet_eterra_flow::GameId,
        credit_type: pallet_eterra_flow::CreditTypeId,
    ) -> u64 {
        pallet_eterra_economy::Pallet::<Test>::credit_balance(account, game_id, credit_type)
    }

    fn consume_credit(
        account: &AccountId,
        game_id: pallet_eterra_flow::GameId,
        credit_type: pallet_eterra_flow::CreditTypeId,
        amount: u64,
    ) -> frame_support::dispatch::DispatchResult {
        pallet_eterra_economy::Pallet::<Test>::try_consume_credit(
            account,
            game_id,
            credit_type,
            amount,
        )
    }

    fn grant_credit(
        account: &AccountId,
        game_id: pallet_eterra_flow::GameId,
        credit_type: pallet_eterra_flow::CreditTypeId,
        amount: u64,
    ) -> frame_support::dispatch::DispatchResult {
        pallet_eterra_economy::Pallet::<Test>::try_grant_credit(
            account,
            game_id,
            credit_type,
            amount,
        )
    }

    fn grant_entitlement(
        account: &AccountId,
        game_id: pallet_eterra_flow::GameId,
        entitlement_id: pallet_eterra_flow::EntitlementId,
    ) -> frame_support::dispatch::DispatchResult {
        pallet_eterra_economy::Pallet::<Test>::try_grant_entitlement(
            account,
            game_id,
            entitlement_id,
        )
    }

    fn revoke_entitlement(
        account: &AccountId,
        game_id: pallet_eterra_flow::GameId,
        entitlement_id: pallet_eterra_flow::EntitlementId,
    ) -> frame_support::dispatch::DispatchResult {
        pallet_eterra_economy::Pallet::<Test>::try_revoke_entitlement(
            account,
            game_id,
            entitlement_id,
        )
    }

    fn spend_sponsor_funds(
        game_id: pallet_eterra_flow::GameId,
        amount: u128,
    ) -> frame_support::dispatch::DispatchResult {
        pallet_eterra_economy::Pallet::<Test>::try_spend_sponsor_funds(game_id, amount)
    }
}

pub struct MockProfileProvider;

impl pallet_eterra_flow::ProfileProvider<AccountId> for MockProfileProvider {
    fn update_passport_counter(
        account: &AccountId,
        field_id: pallet_eterra_flow::PassportFieldId,
        amount: u64,
    ) -> frame_support::dispatch::DispatchResult {
        pallet_eterra_profile::Pallet::<Test>::try_increment_counter(account, field_id, amount)
    }

    fn grant_passport_badge(
        account: &AccountId,
        badge_id: pallet_eterra_flow::PassportBadgeId,
    ) -> frame_support::dispatch::DispatchResult {
        pallet_eterra_profile::Pallet::<Test>::try_grant_badge(account, badge_id)
    }

    fn revoke_passport_badge(
        account: &AccountId,
        badge_id: pallet_eterra_flow::PassportBadgeId,
    ) -> frame_support::dispatch::DispatchResult {
        pallet_eterra_profile::Pallet::<Test>::try_revoke_badge(account, badge_id)
    }
}

impl pallet_eterra_authority::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
    type AdminOrigin = frame_system::EnsureRoot<AccountId>;
    type MaxAllowedEventsPerAuthority = ConstU32<8>;
}

impl pallet_eterra_economy::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
    type AdminOrigin = frame_system::EnsureRoot<AccountId>;
    type TicketAssets = MockTicketAssets;
    type NativePayments = MockNativePayments;
    type PrizeFulfillment = ();
    type PackCreditIssuer = ();
    type AccountEligibility = ();
    type RandomnessProvider = MockRandomness;
    type ArcadeCreditFaucetGameId = ConstU64<1000>;
    type ArcadeCreditFaucetType = ConstU32<1>;
    type ArcadeCreditFaucetAmount = ConstU64<1000>;
    type MaxScoreTiers = ConstU32<8>;
    type MaxEligibleRewardModes = ConstU32<8>;
    type MaxEligibleEndedReasons = ConstU32<8>;
    type MaxFeaturedPoolSubjects = ConstU32<8>;
    type MaxFeaturedSlots = ConstU32<8>;
    type FeaturedSlotCount = ConstU32<4>;
    type MaxPrizeCards = ConstU32<8>;
}

impl pallet_eterra_profile::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
    type AdminOrigin = frame_system::EnsureRoot<AccountId>;
}

impl pallet_eterra_flow::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
    type AuthorityProvider = MockAuthorityProvider;
    type EconomyProvider = MockEconomyProvider;
    type ProfileProvider = MockProfileProvider;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkAuthorityProvider = MockBenchmarkAuthorityProvider;
    type MaxUriBytes = ConstU32<64>;
    type MaxManifestChunkBytes = ConstU32<4096>;
    type MaxManifestChunks = ConstU32<4>;
    type MaxManifestBytes = ConstU32<16_384>;
    type MaxActionPayloadBytes = ConstU32<128>;
    type MaxAttestedPayloadBytes = ConstU32<256>;
    type MaxMachinesPerManifest = ConstU32<4>;
    type MaxStatesPerMachine = ConstU32<8>;
    type MaxVariablesPerManifest = ConstU32<8>;
    type MaxActionsPerManifest = ConstU32<8>;
    type MaxTransitionsPerManifest = ConstU32<2>;
    type MaxConditionsPerTransition = ConstU32<4>;
    type MaxConditionClauses = ConstU32<4>;
    type MaxEconomyGateClauses = ConstU32<4>;
    type MaxEffectsPerTransition = ConstU32<4>;
    type MaxEventsPerManifest = ConstU32<4>;
    type MaxAttestedEffectsPerEvent = ConstU32<4>;
    type MaxEventEffectPolicies = ConstU32<4>;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let storage = system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("frame-system storage build should not fail");
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| System::set_block_number(1));
    ext
}
