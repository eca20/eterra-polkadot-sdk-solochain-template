//! Nova Rail arcade game pallet.
//!
//! Purpose: game-specific validation and entrypoints for the Nova Rail cabinet.
#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
pub mod weights;
pub use weights::WeightInfo;

pub const NOVA_RAIL_GAME_ID: pallet_eterra_arcade_core::GameId = 1003;
pub const NOVA_RAIL_SLUG: &[u8] = b"nova_rail";

#[frame_support::pallet]
pub mod pallet {
    use super::{weights::WeightInfo, NOVA_RAIL_GAME_ID};
    use frame_support::{dispatch::DispatchResult, ensure, pallet_prelude::*};
    use frame_system::pallet_prelude::*;
    use pallet_eterra_arcade_core::{self as arcade_core, ClientRunIdOf, RunId, RunResultInput};

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct NovaRailRunResult<T: arcade_core::Config> {
        pub common: RunResultInput<T>,
        pub stage_reached: u32,
        pub enemies_defeated: u32,
        pub boss_spawned: bool,
        pub boss_defeated: bool,
        pub terrain_hits: u32,
        pub pickups_collected: u32,
        pub deflections: u32,
        pub nova_bombs_used: u32,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config + arcade_core::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type WeightInfo: WeightInfo;

        #[pallet::constant]
        type MaxNovaRailStage: Get<u32>;
        #[pallet::constant]
        type MaxNovaRailEnemiesDefeated: Get<u32>;
        #[pallet::constant]
        type MaxNovaRailTerrainHits: Get<u32>;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        NovaRailRunStarted {
            run_id: RunId,
            player: T::AccountId,
        },
        NovaRailContinuePaid {
            run_id: RunId,
            player: T::AccountId,
            paid_continues_used: u32,
        },
        NovaRailResultSubmitted {
            run_id: RunId,
            score: u64,
            ranked: bool,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        WrongGameId,
        StageOutOfBounds,
        EnemiesOutOfBounds,
        TerrainHitsOutOfBounds,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::start_run())]
        pub fn start_run(
            origin: OriginFor<T>,
            ruleset_version: arcade_core::RulesetVersion,
            client_run_id: ClientRunIdOf<T>,
            seed_commitment: T::Hash,
        ) -> DispatchResult {
            let player = ensure_signed(origin)?;
            let run_id = arcade_core::Pallet::<T>::start_run_for_game(
                &player,
                NOVA_RAIL_GAME_ID,
                ruleset_version,
                client_run_id,
                seed_commitment,
            )?;
            Self::deposit_event(Event::NovaRailRunStarted { run_id, player });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::pay_continue())]
        pub fn pay_continue(origin: OriginFor<T>, run_id: RunId) -> DispatchResult {
            let player = ensure_signed(origin)?;
            let paid_continues_used =
                arcade_core::Pallet::<T>::pay_continue_for_run(&player, run_id)?;
            Self::deposit_event(Event::NovaRailContinuePaid {
                run_id,
                player,
                paid_continues_used,
            });
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(<T as Config>::WeightInfo::submit_result())]
        pub fn submit_result(
            origin: OriginFor<T>,
            summary: NovaRailRunResult<T>,
        ) -> DispatchResult {
            let authority = ensure_signed(origin)?;
            Self::validate_summary(&summary)?;
            let run_id = summary.common.run_id;
            let score = summary.common.score;
            let ranked = summary.common.ranked;
            arcade_core::Pallet::<T>::submit_result_for_authority(&authority, summary.common)?;
            Self::deposit_event(Event::NovaRailResultSubmitted {
                run_id,
                score,
                ranked,
            });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn validate_summary(summary: &NovaRailRunResult<T>) -> DispatchResult {
            ensure!(
                summary.common.game_id == NOVA_RAIL_GAME_ID,
                Error::<T>::WrongGameId
            );
            ensure!(
                summary.stage_reached <= T::MaxNovaRailStage::get(),
                Error::<T>::StageOutOfBounds
            );
            ensure!(
                summary.enemies_defeated <= T::MaxNovaRailEnemiesDefeated::get(),
                Error::<T>::EnemiesOutOfBounds
            );
            ensure!(
                summary.terrain_hits <= T::MaxNovaRailTerrainHits::get(),
                Error::<T>::TerrainHitsOutOfBounds
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_support::{
        assert_noop, assert_ok, construct_runtime,
        dispatch::DispatchResult,
        ensure, parameter_types,
        traits::{ConstU32, Everything},
    };
    use frame_system as system;
    use pallet_eterra_arcade_core::{
        ActiveRunByPlayerGame, AuthorityEventTypeId, AuthorityProvider, CreditTypeId,
        EconomyProvider, EndedReason, GameId, Leaderboards, PlayerBest, ProgressLabelOf,
        ResultIdOf, RulesetVersion, RunResultInput, SlugOf, UnrankedReason, ARCADE_CORE_GAME_ID,
        ARCADE_PLAY_CREDIT_TYPE, EVENT_ARCADE_SUBMIT_RUN_RESULT,
    };
    use sp_core::H256;
    use sp_runtime::{
        traits::{BlakeTwo256, IdentityLookup},
        BuildStorage, DispatchError,
    };
    use std::{cell::RefCell, collections::BTreeMap, collections::BTreeSet};

    type AccountId = u64;
    type Block = system::mocking::MockBlock<Test>;

    construct_runtime!(
        pub enum Test {
            System: system,
            ArcadeCore: pallet_eterra_arcade_core,
            ArcadeNovaRail: crate,
        }
    );

    parameter_types! {
        pub const BlockHashCount: u64 = 250;
        pub const MaxSlugLen: u32 = 32;
        pub const MaxClientRunIdLen: u32 = 64;
        pub const MaxResultIdLen: u32 = 64;
        pub const MaxProgressLabelLen: u32 = 64;
        pub const MaxLeaderboardEntries: u32 = 4;
        pub const MaxNovaRailStage: u32 = 32;
        pub const MaxNovaRailEnemiesDefeated: u32 = 10_000;
        pub const MaxNovaRailTerrainHits: u32 = 1_000;
    }

    thread_local! {
        static CREDITS: RefCell<BTreeMap<(AccountId, GameId, CreditTypeId), u64>> = const { RefCell::new(BTreeMap::new()) };
        static AUTHORITIES: RefCell<BTreeSet<(AccountId, GameId, RulesetVersion, AuthorityEventTypeId)>> = const { RefCell::new(BTreeSet::new()) };
    }

    pub struct TestEconomyProvider;
    impl EconomyProvider<AccountId> for TestEconomyProvider {
        fn consume_credit(
            account: &AccountId,
            game_id: GameId,
            credit_type: CreditTypeId,
            amount: u64,
        ) -> DispatchResult {
            CREDITS.with(|credits| {
                let mut credits = credits.borrow_mut();
                let balance = credits.entry((*account, game_id, credit_type)).or_default();
                ensure!(
                    *balance >= amount,
                    DispatchError::Other("insufficient_credit")
                );
                *balance -= amount;
                Ok(())
            })
        }

        fn credit_balance(account: &AccountId, game_id: GameId, credit_type: CreditTypeId) -> u64 {
            CREDITS.with(|credits| {
                credits
                    .borrow()
                    .get(&(*account, game_id, credit_type))
                    .copied()
                    .unwrap_or_default()
            })
        }
    }

    pub struct TestAuthorityProvider;
    impl AuthorityProvider<AccountId> for TestAuthorityProvider {
        fn can_submit(
            account: &AccountId,
            game_id: GameId,
            ruleset_version: RulesetVersion,
            event_type: AuthorityEventTypeId,
        ) -> bool {
            AUTHORITIES.with(|authorities| {
                authorities
                    .borrow()
                    .contains(&(*account, game_id, ruleset_version, event_type))
            })
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

    impl pallet_eterra_arcade_core::Config for Test {
        type RuntimeEvent = RuntimeEvent;
        type WeightInfo = ();
        type AdminOrigin = frame_system::EnsureRoot<AccountId>;
        type EconomyProvider = TestEconomyProvider;
        type AuthorityProvider = TestAuthorityProvider;
        type MaxSlugLen = MaxSlugLen;
        type MaxClientRunIdLen = MaxClientRunIdLen;
        type MaxResultIdLen = MaxResultIdLen;
        type MaxProgressLabelLen = MaxProgressLabelLen;
        type MaxLeaderboardEntries = MaxLeaderboardEntries;
    }

    impl Config for Test {
        type RuntimeEvent = RuntimeEvent;
        type WeightInfo = ();
        type MaxNovaRailStage = MaxNovaRailStage;
        type MaxNovaRailEnemiesDefeated = MaxNovaRailEnemiesDefeated;
        type MaxNovaRailTerrainHits = MaxNovaRailTerrainHits;
    }

    fn new_test_ext() -> sp_io::TestExternalities {
        CREDITS.with(|credits| credits.borrow_mut().clear());
        AUTHORITIES.with(|authorities| authorities.borrow_mut().clear());
        let storage = system::GenesisConfig::<Test>::default()
            .build_storage()
            .expect("frame-system storage build should not fail");
        let mut ext = sp_io::TestExternalities::new(storage);
        ext.execute_with(|| System::set_block_number(1));
        ext
    }

    fn bounded<TBound: frame_support::traits::Get<u32>>(
        value: &str,
    ) -> frame_support::BoundedVec<u8, TBound> {
        value.as_bytes().to_vec().try_into().expect("bounded value")
    }

    fn slug(value: &str) -> SlugOf<Test> {
        bounded(value)
    }

    fn client_run_id(value: &str) -> pallet_eterra_arcade_core::ClientRunIdOf<Test> {
        bounded(value)
    }

    fn result_id(value: &str) -> ResultIdOf<Test> {
        bounded(value)
    }

    fn progress(value: &str) -> ProgressLabelOf<Test> {
        bounded(value)
    }

    fn grant_credit(account: AccountId, amount: u64) {
        CREDITS.with(|credits| {
            credits.borrow_mut().insert(
                (account, ARCADE_CORE_GAME_ID, ARCADE_PLAY_CREDIT_TYPE),
                amount,
            );
        });
    }

    fn authorize(account: AccountId, game_id: GameId) {
        AUTHORITIES.with(|authorities| {
            authorities
                .borrow_mut()
                .insert((account, game_id, 1, EVENT_ARCADE_SUBMIT_RUN_RESULT));
        });
    }

    fn configure_game(game_id: GameId) {
        assert_ok!(ArcadeCore::configure_game(
            RuntimeOrigin::root(),
            game_id,
            slug("nova_rail"),
            true,
            1,
            ARCADE_CORE_GAME_ID,
            ARCADE_PLAY_CREDIT_TYPE,
            25,
            100,
            1_000_000,
            3,
        ));
    }

    fn common_result(run_id: u64, game_id: GameId, score: u64) -> RunResultInput<Test> {
        RunResultInput::<Test> {
            run_id,
            result_id: result_id("result-1"),
            game_id,
            ruleset_version: 1,
            score,
            ranked: true,
            unranked_reason: UnrankedReason::None,
            ended_reason: EndedReason::BossDefeated,
            continues_used: 0,
            progress_label: progress("Stage 1"),
            progress_hash: H256::repeat_byte(1),
            metrics_hash: Some(H256::repeat_byte(2)),
        }
    }

    fn nova_summary(run_id: u64, game_id: GameId, score: u64) -> NovaRailRunResult<Test> {
        NovaRailRunResult::<Test> {
            common: common_result(run_id, game_id, score),
            stage_reached: 1,
            enemies_defeated: 32,
            boss_spawned: true,
            boss_defeated: true,
            terrain_hits: 2,
            pickups_collected: 4,
            deflections: 3,
            nova_bombs_used: 1,
        }
    }

    #[test]
    fn wrapper_starts_nova_rail_and_consumes_twenty_five_credits() {
        new_test_ext().execute_with(|| {
            configure_game(NOVA_RAIL_GAME_ID);
            grant_credit(42, 50);
            assert_ok!(ArcadeNovaRail::start_run(
                RuntimeOrigin::signed(42),
                1,
                client_run_id("client-1"),
                H256::repeat_byte(1),
            ));
            assert!(ActiveRunByPlayerGame::<Test>::get(NOVA_RAIL_GAME_ID, 42).is_some());
            assert_eq!(
                TestEconomyProvider::credit_balance(
                    &42,
                    ARCADE_CORE_GAME_ID,
                    ARCADE_PLAY_CREDIT_TYPE
                ),
                25
            );
        });
    }

    #[test]
    fn paid_continue_consumes_twenty_five_and_preserves_ranked_result() {
        new_test_ext().execute_with(|| {
            configure_game(NOVA_RAIL_GAME_ID);
            authorize(9, NOVA_RAIL_GAME_ID);
            grant_credit(42, 50);
            let run_id = ArcadeCore::start_run_for_game(
                &42,
                NOVA_RAIL_GAME_ID,
                1,
                client_run_id("client-1"),
                H256::repeat_byte(1),
            )
            .expect("run starts");

            assert_ok!(ArcadeNovaRail::pay_continue(
                RuntimeOrigin::signed(42),
                run_id
            ));
            let mut summary = nova_summary(run_id, NOVA_RAIL_GAME_ID, 9_000);
            summary.common.continues_used = 1;
            assert_ok!(ArcadeNovaRail::submit_result(
                RuntimeOrigin::signed(9),
                summary
            ));
            assert_eq!(
                PlayerBest::<Test>::get((NOVA_RAIL_GAME_ID, 1, 1, 42))
                    .expect("best")
                    .score,
                9_000
            );
            assert_eq!(
                Leaderboards::<Test>::get((NOVA_RAIL_GAME_ID, 1, 1))[0].player,
                42
            );
            assert!(Leaderboards::<Test>::get((NOVA_RAIL_GAME_ID, 1, 0)).is_empty());
        });
    }

    #[test]
    fn unpaid_continue_cannot_enter_ranked_leaderboard() {
        new_test_ext().execute_with(|| {
            configure_game(NOVA_RAIL_GAME_ID);
            authorize(9, NOVA_RAIL_GAME_ID);
            grant_credit(42, 25);
            let run_id = ArcadeCore::start_run_for_game(
                &42,
                NOVA_RAIL_GAME_ID,
                1,
                client_run_id("client-1"),
                H256::repeat_byte(1),
            )
            .expect("run starts");

            let mut summary = nova_summary(run_id, NOVA_RAIL_GAME_ID, 9_000);
            summary.common.continues_used = 1;
            assert_noop!(
                ArcadeNovaRail::submit_result(RuntimeOrigin::signed(9), summary),
                pallet_eterra_arcade_core::Error::<Test>::RankedRunUsedUnpaidContinue
            );
            assert!(Leaderboards::<Test>::get((NOVA_RAIL_GAME_ID, 1, 1)).is_empty());
        });
    }

    #[test]
    fn nova_rail_leaderboard_does_not_collide_with_other_game_ids() {
        new_test_ext().execute_with(|| {
            configure_game(NOVA_RAIL_GAME_ID);
            configure_game(1001);
            authorize(9, NOVA_RAIL_GAME_ID);
            authorize(9, 1001);
            grant_credit(42, 50);
            grant_credit(77, 50);

            let nova_run = ArcadeCore::start_run_for_game(
                &42,
                NOVA_RAIL_GAME_ID,
                1,
                client_run_id("nova-client"),
                H256::repeat_byte(1),
            )
            .expect("nova run starts");
            let ouro_run = ArcadeCore::start_run_for_game(
                &77,
                1001,
                1,
                client_run_id("ouro-client"),
                H256::repeat_byte(2),
            )
            .expect("ouro run starts");

            assert_ok!(ArcadeNovaRail::submit_result(
                RuntimeOrigin::signed(9),
                nova_summary(nova_run, NOVA_RAIL_GAME_ID, 9_000)
            ));
            let mut ouro_result = common_result(ouro_run, 1001, 1_000);
            ouro_result.result_id = result_id("ouro-result-1");
            assert_ok!(ArcadeCore::submit_result_for_authority(&9, ouro_result));

            assert_eq!(
                Leaderboards::<Test>::get((NOVA_RAIL_GAME_ID, 1, 0)).len(),
                1
            );
            assert_eq!(Leaderboards::<Test>::get((1001, 1, 0)).len(), 1);
            assert_eq!(
                Leaderboards::<Test>::get((NOVA_RAIL_GAME_ID, 1, 0))[0].player,
                42
            );
            assert_eq!(Leaderboards::<Test>::get((1001, 1, 0))[0].player, 77);
        });
    }
}
