//! Aegis Run arcade game pallet.
//!
//! Purpose: game-specific validation and entrypoints for the Aegis Run cabinet.
#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
pub mod weights;
pub use weights::WeightInfo;

pub const AEGIS_RUN_GAME_ID: pallet_eterra_arcade_core::GameId = 1002;
pub const AEGIS_RUN_SLUG: &[u8] = b"aegis_run";

#[frame_support::pallet]
pub mod pallet {
    use super::{weights::WeightInfo, AEGIS_RUN_GAME_ID};
    use frame_support::{dispatch::DispatchResult, ensure, pallet_prelude::*};
    use frame_system::pallet_prelude::*;
    use pallet_eterra_arcade_core::{self as arcade_core, ClientRunIdOf, RunId, RunResultInput};

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct AegisRunResult<T: arcade_core::Config> {
        pub common: RunResultInput<T>,
        pub stage_index: u32,
        pub stages_cleared: u32,
        pub checkpoints_reached: u32,
        pub weakness_hits: u32,
        pub elemental_kills: u32,
        pub boss_defeated: bool,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config + arcade_core::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type WeightInfo: WeightInfo;

        #[pallet::constant]
        type MaxAegisStagesPerRun: Get<u32>;
        #[pallet::constant]
        type MaxAegisCheckpointsPerRun: Get<u32>;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        AegisRunStarted {
            run_id: RunId,
            player: T::AccountId,
        },
        AegisResultSubmitted {
            run_id: RunId,
            score: u64,
            ranked: bool,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        WrongGameId,
        StageOutOfBounds,
        CheckpointsOutOfBounds,
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
                AEGIS_RUN_GAME_ID,
                ruleset_version,
                client_run_id,
                seed_commitment,
            )?;
            Self::deposit_event(Event::AegisRunStarted { run_id, player });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::submit_result())]
        pub fn submit_result(origin: OriginFor<T>, summary: AegisRunResult<T>) -> DispatchResult {
            let authority = ensure_signed(origin)?;
            Self::validate_summary(&summary)?;
            let run_id = summary.common.run_id;
            let score = summary.common.score;
            let ranked = summary.common.ranked;
            arcade_core::Pallet::<T>::submit_result_for_authority(&authority, summary.common)?;
            Self::deposit_event(Event::AegisResultSubmitted {
                run_id,
                score,
                ranked,
            });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn validate_summary(summary: &AegisRunResult<T>) -> DispatchResult {
            ensure!(
                summary.common.game_id == AEGIS_RUN_GAME_ID,
                Error::<T>::WrongGameId
            );
            ensure!(
                summary.stage_index < T::MaxAegisStagesPerRun::get()
                    && summary.stages_cleared <= T::MaxAegisStagesPerRun::get(),
                Error::<T>::StageOutOfBounds
            );
            ensure!(
                summary.checkpoints_reached <= T::MaxAegisCheckpointsPerRun::get(),
                Error::<T>::CheckpointsOutOfBounds
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
            ArcadeAegisRun: crate,
        }
    );

    parameter_types! {
        pub const BlockHashCount: u64 = 250;
        pub const MaxSlugLen: u32 = 32;
        pub const MaxClientRunIdLen: u32 = 64;
        pub const MaxResultIdLen: u32 = 64;
        pub const MaxProgressLabelLen: u32 = 64;
        pub const MaxLeaderboardEntries: u32 = 4;
        pub const MaxAegisStagesPerRun: u32 = 3;
        pub const MaxAegisCheckpointsPerRun: u32 = 12;
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

        fn grant_gameplay_tickets(
            _account: &AccountId,
            _game_id: GameId,
            _ruleset_version: RulesetVersion,
            _result_id: &[u8],
            _score: u64,
            _ranked: bool,
            _ended_reason: u8,
        ) -> DispatchResult {
            Ok(())
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
        type MaxAegisStagesPerRun = MaxAegisStagesPerRun;
        type MaxAegisCheckpointsPerRun = MaxAegisCheckpointsPerRun;
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

    fn authorize(account: AccountId) {
        AUTHORITIES.with(|authorities| {
            authorities.borrow_mut().insert((
                account,
                AEGIS_RUN_GAME_ID,
                1,
                EVENT_ARCADE_SUBMIT_RUN_RESULT,
            ));
        });
    }

    fn configure_aegis() {
        assert_ok!(ArcadeCore::configure_game(
            RuntimeOrigin::root(),
            AEGIS_RUN_GAME_ID,
            slug("aegis_run"),
            true,
            1,
            ARCADE_CORE_GAME_ID,
            ARCADE_PLAY_CREDIT_TYPE,
            1,
            10,
            100_000,
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
            progress_label: progress("Stage 3 Clear"),
            progress_hash: H256::repeat_byte(1),
            metrics_hash: Some(H256::repeat_byte(2)),
        }
    }

    fn aegis_summary(
        run_id: u64,
        game_id: GameId,
        stage_index: u32,
        checkpoints: u32,
    ) -> AegisRunResult<Test> {
        AegisRunResult::<Test> {
            common: common_result(run_id, game_id, 1_200),
            stage_index,
            stages_cleared: stage_index,
            checkpoints_reached: checkpoints,
            weakness_hits: 5,
            elemental_kills: 3,
            boss_defeated: true,
        }
    }

    #[test]
    fn wrapper_starts_aegis_game_only() {
        new_test_ext().execute_with(|| {
            configure_aegis();
            grant_credit(42, 1);
            assert_ok!(ArcadeAegisRun::start_run(
                RuntimeOrigin::signed(42),
                1,
                client_run_id("client-1"),
                H256::repeat_byte(1),
            ));
            assert!(ActiveRunByPlayerGame::<Test>::get(AEGIS_RUN_GAME_ID, 42).is_some());
        });
    }

    #[test]
    fn wrapper_rejects_wrong_game_id_and_out_of_bounds_summary() {
        new_test_ext().execute_with(|| {
            configure_aegis();
            grant_credit(42, 1);
            let run_id = ArcadeCore::start_run_for_game(
                &42,
                AEGIS_RUN_GAME_ID,
                1,
                client_run_id("client-1"),
                H256::repeat_byte(1),
            )
            .expect("run starts");

            assert_noop!(
                ArcadeAegisRun::submit_result(
                    RuntimeOrigin::signed(9),
                    aegis_summary(run_id, 1001, 1, 3)
                ),
                Error::<Test>::WrongGameId
            );
            assert_noop!(
                ArcadeAegisRun::submit_result(
                    RuntimeOrigin::signed(9),
                    aegis_summary(run_id, AEGIS_RUN_GAME_ID, 3, 3)
                ),
                Error::<Test>::StageOutOfBounds
            );
            assert_noop!(
                ArcadeAegisRun::submit_result(
                    RuntimeOrigin::signed(9),
                    aegis_summary(run_id, AEGIS_RUN_GAME_ID, 1, 13)
                ),
                Error::<Test>::CheckpointsOutOfBounds
            );
        });
    }

    #[test]
    fn wrapper_delegates_valid_result_to_core_leaderboard() {
        new_test_ext().execute_with(|| {
            configure_aegis();
            authorize(9);
            grant_credit(42, 1);
            let run_id = ArcadeCore::start_run_for_game(
                &42,
                AEGIS_RUN_GAME_ID,
                1,
                client_run_id("client-1"),
                H256::repeat_byte(1),
            )
            .expect("run starts");

            assert_ok!(ArcadeAegisRun::submit_result(
                RuntimeOrigin::signed(9),
                aegis_summary(run_id, AEGIS_RUN_GAME_ID, 2, 8)
            ));
            assert_eq!(
                PlayerBest::<Test>::get((AEGIS_RUN_GAME_ID, 1, 0, 42))
                    .expect("best")
                    .score,
                1_200
            );
            assert_eq!(
                Leaderboards::<Test>::get((AEGIS_RUN_GAME_ID, 1, 0)).len(),
                1
            );
        });
    }
}
