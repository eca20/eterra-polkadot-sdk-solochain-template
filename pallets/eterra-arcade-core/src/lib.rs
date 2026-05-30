//! Eterra Arcade core pallet.
//!
//! Purpose: shared paid-run tickets, authority-attested result receipts, and
//! global scoreboards for Eterra arcade cabinets.
#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::too_many_arguments)]

pub use pallet::*;
pub mod weights;
pub use weights::WeightInfo;

pub type GameId = u64;
pub type RunId = u64;
pub type RulesetVersion = u32;
pub type CreditTypeId = u32;
pub type AuthorityEventTypeId = u32;

pub const ARCADE_CORE_GAME_ID: GameId = 1000;
pub const ARCADE_PLAY_CREDIT_TYPE: CreditTypeId = 1;
pub const EVENT_ARCADE_SUBMIT_RUN_RESULT: AuthorityEventTypeId = 100;

pub trait EconomyProvider<AccountId> {
    fn consume_credit(
        account: &AccountId,
        game_id: GameId,
        credit_type: CreditTypeId,
        amount: u64,
    ) -> frame_support::dispatch::DispatchResult;

    fn credit_balance(account: &AccountId, game_id: GameId, credit_type: CreditTypeId) -> u64;
}

impl<AccountId> EconomyProvider<AccountId> for () {
    fn consume_credit(
        _account: &AccountId,
        _game_id: GameId,
        _credit_type: CreditTypeId,
        _amount: u64,
    ) -> frame_support::dispatch::DispatchResult {
        Ok(())
    }

    fn credit_balance(_account: &AccountId, _game_id: GameId, _credit_type: CreditTypeId) -> u64 {
        u64::MAX
    }
}

pub trait AuthorityProvider<AccountId> {
    fn can_submit(
        account: &AccountId,
        game_id: GameId,
        ruleset_version: RulesetVersion,
        event_type: AuthorityEventTypeId,
    ) -> bool;
}

impl<AccountId> AuthorityProvider<AccountId> for () {
    fn can_submit(
        _account: &AccountId,
        _game_id: GameId,
        _ruleset_version: RulesetVersion,
        _event_type: AuthorityEventTypeId,
    ) -> bool {
        true
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::{
        weights::WeightInfo, AuthorityProvider, CreditTypeId, EconomyProvider, GameId,
        RulesetVersion, RunId, ARCADE_CORE_GAME_ID, ARCADE_PLAY_CREDIT_TYPE,
        EVENT_ARCADE_SUBMIT_RUN_RESULT,
    };
    use frame_support::{
        dispatch::DispatchResult, ensure, pallet_prelude::*, sp_runtime::traits::Saturating,
        traits::StorageVersion, transactional,
    };
    use frame_system::pallet_prelude::*;
    use sp_runtime::DispatchError;
    use sp_std::vec::Vec;

    pub type SlugOf<T> = BoundedVec<u8, <T as Config>::MaxSlugLen>;
    pub type ClientRunIdOf<T> = BoundedVec<u8, <T as Config>::MaxClientRunIdLen>;
    pub type ResultIdOf<T> = BoundedVec<u8, <T as Config>::MaxResultIdLen>;
    pub type ProgressLabelOf<T> = BoundedVec<u8, <T as Config>::MaxProgressLabelLen>;

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
    pub enum RunStatus {
        Active,
        Abandoned,
        Expired,
        Completed,
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
    pub enum EndedReason {
        Completed,
        BossDefeated,
        TimerExpired,
        HullDepleted,
        Abandoned,
        Restarted,
        ReturnedToArcade,
        Expired,
        PracticeContinue,
        Other(u8),
    }

    impl EndedReason {
        pub fn allows_ranked(&self) -> bool {
            matches!(
                self,
                EndedReason::Completed
                    | EndedReason::BossDefeated
                    | EndedReason::TimerExpired
                    | EndedReason::HullDepleted
            )
        }
    }

    #[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
    pub enum UnrankedReason {
        None,
        ContinueUsed,
        Abandoned,
        Restarted,
        ReturnedToArcade,
        Expired,
        Practice,
        Other(u8),
    }

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct GameConfig<T: Config> {
        pub slug: SlugOf<T>,
        pub enabled: bool,
        pub ruleset_version: RulesetVersion,
        pub credit_game_id: GameId,
        pub credit_type: CreditTypeId,
        pub credit_cost: u64,
        pub max_run_blocks: BlockNumberFor<T>,
        pub max_score: u64,
        pub leaderboard_size: u32,
    }

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct RunRecord<T: Config> {
        pub game_id: GameId,
        pub ruleset_version: RulesetVersion,
        pub player: T::AccountId,
        pub client_run_id: ClientRunIdOf<T>,
        pub seed_commitment: T::Hash,
        pub started_at: BlockNumberFor<T>,
        pub expires_at: BlockNumberFor<T>,
        pub status: RunStatus,
    }

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct RunResultInput<T: Config> {
        pub run_id: RunId,
        pub result_id: ResultIdOf<T>,
        pub game_id: GameId,
        pub ruleset_version: RulesetVersion,
        pub score: u64,
        pub ranked: bool,
        pub unranked_reason: UnrankedReason,
        pub ended_reason: EndedReason,
        pub continues_used: u32,
        pub progress_label: ProgressLabelOf<T>,
        pub progress_hash: T::Hash,
        pub metrics_hash: Option<T::Hash>,
    }

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct RunResultRecord<T: Config> {
        pub run_id: RunId,
        pub result_id: ResultIdOf<T>,
        pub game_id: GameId,
        pub ruleset_version: RulesetVersion,
        pub player: T::AccountId,
        pub authority: T::AccountId,
        pub score: u64,
        pub ranked: bool,
        pub unranked_reason: UnrankedReason,
        pub ended_reason: EndedReason,
        pub continues_used: u32,
        pub progress_label: ProgressLabelOf<T>,
        pub progress_hash: T::Hash,
        pub metrics_hash: Option<T::Hash>,
        pub submitted_at: BlockNumberFor<T>,
    }

    #[derive(
        Encode, Decode, MaxEncodedLen, TypeInfo, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct LeaderboardEntry<T: Config> {
        pub player: T::AccountId,
        pub run_id: RunId,
        pub score: u64,
        pub submitted_at: BlockNumberFor<T>,
        pub progress_label: ProgressLabelOf<T>,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type WeightInfo: WeightInfo;
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;
        type EconomyProvider: EconomyProvider<Self::AccountId>;
        type AuthorityProvider: AuthorityProvider<Self::AccountId>;

        #[pallet::constant]
        type MaxSlugLen: Get<u32>;
        #[pallet::constant]
        type MaxClientRunIdLen: Get<u32>;
        #[pallet::constant]
        type MaxResultIdLen: Get<u32>;
        #[pallet::constant]
        type MaxProgressLabelLen: Get<u32>;
        #[pallet::constant]
        type MaxLeaderboardEntries: Get<u32>;
    }

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub type GameConfigs<T: Config> =
        StorageMap<_, Blake2_128Concat, GameId, GameConfig<T>, OptionQuery>;

    #[pallet::storage]
    pub type NextRunId<T: Config> = StorageValue<_, RunId, ValueQuery>;

    #[pallet::storage]
    pub type Runs<T: Config> = StorageMap<_, Blake2_128Concat, RunId, RunRecord<T>, OptionQuery>;

    #[pallet::storage]
    pub type ActiveRunByPlayerGame<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        GameId,
        Blake2_128Concat,
        T::AccountId,
        RunId,
        OptionQuery,
    >;

    #[pallet::storage]
    pub type RunIdByClientRunId<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        GameId,
        Blake2_128Concat,
        ClientRunIdOf<T>,
        RunId,
        OptionQuery,
    >;

    #[pallet::storage]
    pub type ProcessedResultIds<T: Config> =
        StorageMap<_, Blake2_128Concat, ResultIdOf<T>, RunId, OptionQuery>;

    #[pallet::storage]
    pub type RunResultsByRun<T: Config> =
        StorageMap<_, Blake2_128Concat, RunId, RunResultRecord<T>, OptionQuery>;

    #[pallet::storage]
    pub type PlayerBest<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, GameId>,
            NMapKey<Blake2_128Concat, RulesetVersion>,
            NMapKey<Blake2_128Concat, T::AccountId>,
        ),
        LeaderboardEntry<T>,
        OptionQuery,
    >;

    #[pallet::storage]
    pub type Leaderboards<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, GameId>,
            NMapKey<Blake2_128Concat, RulesetVersion>,
        ),
        BoundedVec<LeaderboardEntry<T>, T::MaxLeaderboardEntries>,
        ValueQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        GameConfigured {
            game_id: GameId,
            ruleset_version: RulesetVersion,
            enabled: bool,
        },
        RunStarted {
            run_id: RunId,
            game_id: GameId,
            player: T::AccountId,
            expires_at: BlockNumberFor<T>,
        },
        RunAbandoned {
            run_id: RunId,
            game_id: GameId,
            player: T::AccountId,
        },
        RunExpired {
            run_id: RunId,
            game_id: GameId,
            player: T::AccountId,
        },
        RunResultAccepted {
            run_id: RunId,
            game_id: GameId,
            player: T::AccountId,
            score: u64,
            ranked: bool,
        },
        PlayerBestUpdated {
            game_id: GameId,
            ruleset_version: RulesetVersion,
            player: T::AccountId,
            score: u64,
        },
        LeaderboardUpdated {
            game_id: GameId,
            ruleset_version: RulesetVersion,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        GameNotConfigured,
        GameDisabled,
        RulesetVersionMismatch,
        LeaderboardSizeTooLarge,
        EmptyClientRunId,
        EmptyResultId,
        EmptyProgressLabel,
        ClientRunIdAlreadyUsed,
        PlayerAlreadyHasActiveRun,
        RunNotFound,
        NotRunOwner,
        RunNotActive,
        RunNotExpired,
        RunExpired,
        ResultAlreadyProcessed,
        UnauthorizedAuthority,
        ScoreTooHigh,
        RankedRunUsedContinue,
        RankedRunHasUnrankedReason,
        RankedRunEndedWithUnrankedReason,
        UnrankedRunMissingReason,
        ArithmeticOverflow,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::configure_game())]
        pub fn configure_game(
            origin: OriginFor<T>,
            game_id: GameId,
            slug: SlugOf<T>,
            enabled: bool,
            ruleset_version: RulesetVersion,
            credit_game_id: GameId,
            credit_type: CreditTypeId,
            credit_cost: u64,
            max_run_blocks: BlockNumberFor<T>,
            max_score: u64,
            leaderboard_size: u32,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                leaderboard_size > 0 && leaderboard_size <= T::MaxLeaderboardEntries::get(),
                Error::<T>::LeaderboardSizeTooLarge
            );
            GameConfigs::<T>::insert(
                game_id,
                GameConfig::<T> {
                    slug,
                    enabled,
                    ruleset_version,
                    credit_game_id,
                    credit_type,
                    credit_cost,
                    max_run_blocks,
                    max_score,
                    leaderboard_size,
                },
            );
            Self::deposit_event(Event::GameConfigured {
                game_id,
                ruleset_version,
                enabled,
            });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::start_run())]
        pub fn start_run(
            origin: OriginFor<T>,
            game_id: GameId,
            ruleset_version: RulesetVersion,
            client_run_id: ClientRunIdOf<T>,
            seed_commitment: T::Hash,
        ) -> DispatchResult {
            let player = ensure_signed(origin)?;
            Self::start_run_for_game(
                &player,
                game_id,
                ruleset_version,
                client_run_id,
                seed_commitment,
            )
            .map(|_| ())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::abandon_run())]
        pub fn abandon_run(origin: OriginFor<T>, run_id: RunId) -> DispatchResult {
            let player = ensure_signed(origin)?;
            let mut run = Runs::<T>::get(run_id).ok_or(Error::<T>::RunNotFound)?;
            ensure!(run.player == player, Error::<T>::NotRunOwner);
            ensure!(run.status == RunStatus::Active, Error::<T>::RunNotActive);
            run.status = RunStatus::Abandoned;
            Runs::<T>::insert(run_id, &run);
            ActiveRunByPlayerGame::<T>::remove(run.game_id, &run.player);
            Self::deposit_event(Event::RunAbandoned {
                run_id,
                game_id: run.game_id,
                player: run.player,
            });
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::expire_run())]
        pub fn expire_run(origin: OriginFor<T>, run_id: RunId) -> DispatchResult {
            let _ = ensure_signed(origin)?;
            let run = Runs::<T>::get(run_id).ok_or(Error::<T>::RunNotFound)?;
            ensure!(run.status == RunStatus::Active, Error::<T>::RunNotActive);
            ensure!(
                frame_system::Pallet::<T>::block_number() >= run.expires_at,
                Error::<T>::RunNotExpired
            );
            Self::expire_run_internal(run_id, run)
        }
    }

    impl<T: Config> Pallet<T> {
        #[transactional]
        pub fn start_run_for_game(
            player: &T::AccountId,
            game_id: GameId,
            ruleset_version: RulesetVersion,
            client_run_id: ClientRunIdOf<T>,
            seed_commitment: T::Hash,
        ) -> Result<RunId, DispatchError> {
            ensure!(!client_run_id.is_empty(), Error::<T>::EmptyClientRunId);
            let config = GameConfigs::<T>::get(game_id).ok_or(Error::<T>::GameNotConfigured)?;
            ensure!(config.enabled, Error::<T>::GameDisabled);
            ensure!(
                config.ruleset_version == ruleset_version,
                Error::<T>::RulesetVersionMismatch
            );
            ensure!(
                !RunIdByClientRunId::<T>::contains_key(game_id, &client_run_id),
                Error::<T>::ClientRunIdAlreadyUsed
            );

            let now = frame_system::Pallet::<T>::block_number();
            if let Some(active_run_id) = ActiveRunByPlayerGame::<T>::get(game_id, player) {
                if let Some(active_run) = Runs::<T>::get(active_run_id) {
                    if active_run.status == RunStatus::Active && now >= active_run.expires_at {
                        Self::expire_run_internal(active_run_id, active_run)?;
                    } else if active_run.status == RunStatus::Active {
                        return Err(Error::<T>::PlayerAlreadyHasActiveRun.into());
                    } else {
                        ActiveRunByPlayerGame::<T>::remove(game_id, player);
                    }
                } else {
                    ActiveRunByPlayerGame::<T>::remove(game_id, player);
                }
            }

            T::EconomyProvider::consume_credit(
                player,
                config.credit_game_id,
                config.credit_type,
                config.credit_cost,
            )?;

            let run_id = NextRunId::<T>::get();
            let next = run_id
                .checked_add(1)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            NextRunId::<T>::put(next);

            let expires_at = now.saturating_add(config.max_run_blocks);
            Runs::<T>::insert(
                run_id,
                RunRecord::<T> {
                    game_id,
                    ruleset_version,
                    player: player.clone(),
                    client_run_id: client_run_id.clone(),
                    seed_commitment,
                    started_at: now,
                    expires_at,
                    status: RunStatus::Active,
                },
            );
            ActiveRunByPlayerGame::<T>::insert(game_id, player.clone(), run_id);
            RunIdByClientRunId::<T>::insert(game_id, client_run_id, run_id);
            Self::deposit_event(Event::RunStarted {
                run_id,
                game_id,
                player: player.clone(),
                expires_at,
            });
            Ok(run_id)
        }

        #[transactional]
        pub fn submit_result_for_authority(
            authority: &T::AccountId,
            result: RunResultInput<T>,
        ) -> DispatchResult {
            ensure!(!result.result_id.is_empty(), Error::<T>::EmptyResultId);
            ensure!(
                !result.progress_label.is_empty(),
                Error::<T>::EmptyProgressLabel
            );
            ensure!(
                !ProcessedResultIds::<T>::contains_key(&result.result_id),
                Error::<T>::ResultAlreadyProcessed
            );

            let config =
                GameConfigs::<T>::get(result.game_id).ok_or(Error::<T>::GameNotConfigured)?;
            ensure!(
                config.ruleset_version == result.ruleset_version,
                Error::<T>::RulesetVersionMismatch
            );
            ensure!(result.score <= config.max_score, Error::<T>::ScoreTooHigh);
            Self::ensure_ranked_consistency(&result)?;
            ensure!(
                T::AuthorityProvider::can_submit(
                    authority,
                    result.game_id,
                    result.ruleset_version,
                    EVENT_ARCADE_SUBMIT_RUN_RESULT,
                ),
                Error::<T>::UnauthorizedAuthority
            );

            let mut run = Runs::<T>::get(result.run_id).ok_or(Error::<T>::RunNotFound)?;
            ensure!(run.status == RunStatus::Active, Error::<T>::RunNotActive);
            ensure!(run.game_id == result.game_id, Error::<T>::GameNotConfigured);
            ensure!(
                run.ruleset_version == result.ruleset_version,
                Error::<T>::RulesetVersionMismatch
            );

            let now = frame_system::Pallet::<T>::block_number();
            if now >= run.expires_at {
                Self::expire_run_internal(result.run_id, run)?;
                return Err(Error::<T>::RunExpired.into());
            }

            run.status = RunStatus::Completed;
            Runs::<T>::insert(result.run_id, &run);
            ActiveRunByPlayerGame::<T>::remove(result.game_id, &run.player);

            let record = RunResultRecord::<T> {
                run_id: result.run_id,
                result_id: result.result_id.clone(),
                game_id: result.game_id,
                ruleset_version: result.ruleset_version,
                player: run.player.clone(),
                authority: authority.clone(),
                score: result.score,
                ranked: result.ranked,
                unranked_reason: result.unranked_reason.clone(),
                ended_reason: result.ended_reason.clone(),
                continues_used: result.continues_used,
                progress_label: result.progress_label.clone(),
                progress_hash: result.progress_hash,
                metrics_hash: result.metrics_hash,
                submitted_at: now,
            };
            ProcessedResultIds::<T>::insert(result.result_id, result.run_id);
            RunResultsByRun::<T>::insert(result.run_id, &record);

            if record.ranked {
                Self::maybe_update_player_best_and_leaderboard(&config, &record)?;
            }

            Self::deposit_event(Event::RunResultAccepted {
                run_id: record.run_id,
                game_id: record.game_id,
                player: record.player,
                score: record.score,
                ranked: record.ranked,
            });
            Ok(())
        }

        pub fn default_arcade_credit_config() -> (GameId, CreditTypeId, u64) {
            (ARCADE_CORE_GAME_ID, ARCADE_PLAY_CREDIT_TYPE, 1)
        }

        fn ensure_ranked_consistency(result: &RunResultInput<T>) -> DispatchResult {
            if result.ranked {
                ensure!(
                    result.continues_used == 0,
                    Error::<T>::RankedRunUsedContinue
                );
                ensure!(
                    result.unranked_reason == UnrankedReason::None,
                    Error::<T>::RankedRunHasUnrankedReason
                );
                ensure!(
                    result.ended_reason.allows_ranked(),
                    Error::<T>::RankedRunEndedWithUnrankedReason
                );
            } else {
                ensure!(
                    result.unranked_reason != UnrankedReason::None,
                    Error::<T>::UnrankedRunMissingReason
                );
            }
            Ok(())
        }

        fn expire_run_internal(run_id: RunId, mut run: RunRecord<T>) -> DispatchResult {
            run.status = RunStatus::Expired;
            Runs::<T>::insert(run_id, &run);
            ActiveRunByPlayerGame::<T>::remove(run.game_id, &run.player);
            Self::deposit_event(Event::RunExpired {
                run_id,
                game_id: run.game_id,
                player: run.player,
            });
            Ok(())
        }

        fn maybe_update_player_best_and_leaderboard(
            config: &GameConfig<T>,
            record: &RunResultRecord<T>,
        ) -> DispatchResult {
            let current_best =
                PlayerBest::<T>::get((record.game_id, record.ruleset_version, &record.player));
            let should_update = current_best
                .as_ref()
                .map(|best| {
                    record.score > best.score
                        || (record.score == best.score && record.run_id < best.run_id)
                })
                .unwrap_or(true);
            if !should_update {
                return Ok(());
            }

            let entry = LeaderboardEntry::<T> {
                player: record.player.clone(),
                run_id: record.run_id,
                score: record.score,
                submitted_at: record.submitted_at,
                progress_label: record.progress_label.clone(),
            };
            PlayerBest::<T>::insert(
                (
                    record.game_id,
                    record.ruleset_version,
                    record.player.clone(),
                ),
                &entry,
            );
            Self::deposit_event(Event::PlayerBestUpdated {
                game_id: record.game_id,
                ruleset_version: record.ruleset_version,
                player: record.player.clone(),
                score: record.score,
            });

            Leaderboards::<T>::try_mutate(
                (record.game_id, record.ruleset_version),
                |entries| -> DispatchResult {
                    let mut raw: Vec<LeaderboardEntry<T>> = entries.clone().into_inner();
                    raw.retain(|existing| existing.player != record.player);
                    raw.push(entry.clone());
                    raw.sort_by(|a, b| b.score.cmp(&a.score).then(a.run_id.cmp(&b.run_id)));
                    raw.truncate(config.leaderboard_size as usize);
                    *entries = BoundedVec::try_from(raw)
                        .map_err(|_| Error::<T>::LeaderboardSizeTooLarge)?;
                    Ok(())
                },
            )?;
            Self::deposit_event(Event::LeaderboardUpdated {
                game_id: record.game_id,
                ruleset_version: record.ruleset_version,
            });
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
            ArcadeCore: crate,
        }
    );

    parameter_types! {
        pub const BlockHashCount: u64 = 250;
        pub const MaxSlugLen: u32 = 32;
        pub const MaxClientRunIdLen: u32 = 64;
        pub const MaxResultIdLen: u32 = 64;
        pub const MaxProgressLabelLen: u32 = 64;
        pub const MaxLeaderboardEntries: u32 = 4;
    }

    thread_local! {
        static CREDITS: RefCell<BTreeMap<(AccountId, GameId, CreditTypeId), u64>> = RefCell::new(BTreeMap::new());
        static AUTHORITIES: RefCell<BTreeSet<(AccountId, GameId, RulesetVersion, AuthorityEventTypeId)>> = RefCell::new(BTreeSet::new());
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

    impl Config for Test {
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

    fn slug(value: &str) -> SlugOf<Test> {
        value.as_bytes().to_vec().try_into().expect("bounded slug")
    }

    fn client_run_id(value: &str) -> ClientRunIdOf<Test> {
        value
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("bounded client run id")
    }

    fn result_id(value: &str) -> ResultIdOf<Test> {
        value
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("bounded result id")
    }

    fn progress(value: &str) -> ProgressLabelOf<Test> {
        value
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("bounded progress label")
    }

    fn grant_credit(account: AccountId, amount: u64) {
        CREDITS.with(|credits| {
            credits.borrow_mut().insert(
                (account, ARCADE_CORE_GAME_ID, ARCADE_PLAY_CREDIT_TYPE),
                amount,
            );
        });
    }

    fn authorize(account: AccountId, game_id: GameId, ruleset_version: RulesetVersion) {
        AUTHORITIES.with(|authorities| {
            authorities.borrow_mut().insert((
                account,
                game_id,
                ruleset_version,
                EVENT_ARCADE_SUBMIT_RUN_RESULT,
            ));
        });
    }

    fn configure_game(game_id: GameId) {
        assert_ok!(ArcadeCore::configure_game(
            RuntimeOrigin::root(),
            game_id,
            slug("cabinet"),
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

    fn ranked_result(
        run_id: RunId,
        game_id: GameId,
        result: &str,
        score: u64,
    ) -> RunResultInput<Test> {
        RunResultInput::<Test> {
            run_id,
            result_id: result_id(result),
            game_id,
            ruleset_version: 1,
            score,
            ranked: true,
            unranked_reason: UnrankedReason::None,
            ended_reason: EndedReason::BossDefeated,
            continues_used: 0,
            progress_label: progress("Boss Clear"),
            progress_hash: H256::repeat_byte(7),
            metrics_hash: Some(H256::repeat_byte(8)),
        }
    }

    #[test]
    fn start_run_consumes_credit_and_blocks_duplicate_client_id() {
        new_test_ext().execute_with(|| {
            configure_game(1001);
            grant_credit(42, 2);

            assert_ok!(ArcadeCore::start_run(
                RuntimeOrigin::signed(42),
                1001,
                1,
                client_run_id("client-1"),
                H256::repeat_byte(1),
            ));
            assert_eq!(
                TestEconomyProvider::credit_balance(
                    &42,
                    ARCADE_CORE_GAME_ID,
                    ARCADE_PLAY_CREDIT_TYPE
                ),
                1
            );

            assert_noop!(
                ArcadeCore::start_run(
                    RuntimeOrigin::signed(42),
                    1001,
                    1,
                    client_run_id("client-1"),
                    H256::repeat_byte(1),
                ),
                Error::<Test>::ClientRunIdAlreadyUsed
            );
            assert_eq!(
                TestEconomyProvider::credit_balance(
                    &42,
                    ARCADE_CORE_GAME_ID,
                    ARCADE_PLAY_CREDIT_TYPE
                ),
                1
            );
        });
    }

    #[test]
    fn insufficient_credit_fails_without_creating_run() {
        new_test_ext().execute_with(|| {
            configure_game(1001);
            assert_noop!(
                ArcadeCore::start_run(
                    RuntimeOrigin::signed(42),
                    1001,
                    1,
                    client_run_id("client-1"),
                    H256::repeat_byte(1),
                ),
                DispatchError::Other("insufficient_credit")
            );
            assert_eq!(NextRunId::<Test>::get(), 0);
        });
    }

    #[test]
    fn only_authorized_authority_can_submit_ranked_results() {
        new_test_ext().execute_with(|| {
            configure_game(1001);
            grant_credit(42, 1);
            let run_id = ArcadeCore::start_run_for_game(
                &42,
                1001,
                1,
                client_run_id("client-1"),
                H256::repeat_byte(1),
            )
            .expect("run starts");

            assert_noop!(
                ArcadeCore::submit_result_for_authority(
                    &9,
                    ranked_result(run_id, 1001, "result-1", 500)
                ),
                Error::<Test>::UnauthorizedAuthority
            );
            authorize(9, 1001, 1);
            assert_ok!(ArcadeCore::submit_result_for_authority(
                &9,
                ranked_result(run_id, 1001, "result-1", 500)
            ));
        });
    }

    #[test]
    fn duplicate_result_id_cannot_update_leaderboard_twice() {
        new_test_ext().execute_with(|| {
            configure_game(1001);
            authorize(9, 1001, 1);
            grant_credit(42, 2);
            let run_id = ArcadeCore::start_run_for_game(
                &42,
                1001,
                1,
                client_run_id("client-1"),
                H256::repeat_byte(1),
            )
            .expect("run starts");
            assert_ok!(ArcadeCore::submit_result_for_authority(
                &9,
                ranked_result(run_id, 1001, "result-1", 500)
            ));

            let second_run = ArcadeCore::start_run_for_game(
                &42,
                1001,
                1,
                client_run_id("client-2"),
                H256::repeat_byte(2),
            )
            .expect("second run starts");
            assert_noop!(
                ArcadeCore::submit_result_for_authority(
                    &9,
                    ranked_result(second_run, 1001, "result-1", 800)
                ),
                Error::<Test>::ResultAlreadyProcessed
            );
            assert_eq!(
                PlayerBest::<Test>::get((1001, 1, 42)).expect("best").score,
                500
            );
        });
    }

    #[test]
    fn unranked_or_continue_used_results_do_not_update_leaderboard() {
        new_test_ext().execute_with(|| {
            configure_game(1001);
            authorize(9, 1001, 1);
            grant_credit(42, 1);
            let run_id = ArcadeCore::start_run_for_game(
                &42,
                1001,
                1,
                client_run_id("client-1"),
                H256::repeat_byte(1),
            )
            .expect("run starts");
            let mut result = ranked_result(run_id, 1001, "result-1", 900);
            result.ranked = false;
            result.unranked_reason = UnrankedReason::ContinueUsed;
            result.continues_used = 1;
            result.ended_reason = EndedReason::PracticeContinue;

            assert_ok!(ArcadeCore::submit_result_for_authority(&9, result));
            assert!(PlayerBest::<Test>::get((1001, 1, 42)).is_none());
            assert!(Leaderboards::<Test>::get((1001, 1)).is_empty());
        });
    }

    #[test]
    fn ranked_results_update_and_sort_global_leaderboard() {
        new_test_ext().execute_with(|| {
            configure_game(1001);
            authorize(9, 1001, 1);
            for player in [10, 11, 12, 13] {
                grant_credit(player, 1);
            }
            for (player, client_id, result, score) in [
                (10, "client-10", "result-10", 400),
                (11, "client-11", "result-11", 800),
                (12, "client-12", "result-12", 600),
                (13, "client-13", "result-13", 700),
            ] {
                let run_id = ArcadeCore::start_run_for_game(
                    &player,
                    1001,
                    1,
                    client_run_id(client_id),
                    H256::repeat_byte(player as u8),
                )
                .expect("run starts");
                assert_ok!(ArcadeCore::submit_result_for_authority(
                    &9,
                    ranked_result(run_id, 1001, result, score)
                ));
            }

            let board = Leaderboards::<Test>::get((1001, 1));
            assert_eq!(board.len(), 3);
            assert_eq!(board[0].player, 11);
            assert_eq!(board[1].player, 13);
            assert_eq!(board[2].player, 12);
        });
    }

    #[test]
    fn abandon_and_expire_do_not_rank_runs() {
        new_test_ext().execute_with(|| {
            configure_game(1001);
            grant_credit(42, 2);
            let run_id = ArcadeCore::start_run_for_game(
                &42,
                1001,
                1,
                client_run_id("client-1"),
                H256::repeat_byte(1),
            )
            .expect("run starts");
            assert_ok!(ArcadeCore::abandon_run(RuntimeOrigin::signed(42), run_id));
            assert!(PlayerBest::<Test>::get((1001, 1, 42)).is_none());

            let expiring_run = ArcadeCore::start_run_for_game(
                &42,
                1001,
                1,
                client_run_id("client-2"),
                H256::repeat_byte(2),
            )
            .expect("run starts");
            System::set_block_number(20);
            assert_ok!(ArcadeCore::expire_run(
                RuntimeOrigin::signed(77),
                expiring_run
            ));
            assert_eq!(
                Runs::<Test>::get(expiring_run).expect("run").status,
                RunStatus::Expired
            );
        });
    }

    #[test]
    fn score_above_configured_max_is_rejected() {
        new_test_ext().execute_with(|| {
            configure_game(1001);
            authorize(9, 1001, 1);
            grant_credit(42, 1);
            let run_id = ArcadeCore::start_run_for_game(
                &42,
                1001,
                1,
                client_run_id("client-1"),
                H256::repeat_byte(1),
            )
            .expect("run starts");
            assert_noop!(
                ArcadeCore::submit_result_for_authority(
                    &9,
                    ranked_result(run_id, 1001, "result-1", 100_001)
                ),
                Error::<Test>::ScoreTooHigh
            );
        });
    }
}
