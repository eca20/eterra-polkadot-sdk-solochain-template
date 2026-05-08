#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

pub mod weights;
pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub type GameId = u64;

pub trait GameLifecycleHooks<AccountId> {
    fn on_game_created(
        _game_id: GameId,
        _server: &AccountId,
        _players: &[AccountId],
    ) -> frame_support::dispatch::DispatchResult {
        Ok(())
    }

    fn on_game_ended(_game_id: GameId, _server: &AccountId, _players: &[AccountId]) {}
}

impl<AccountId> GameLifecycleHooks<AccountId> for () {}

#[frame_support::pallet]
pub mod pallet {
    use crate::weights::WeightInfo;
    use crate::{GameId, GameLifecycleHooks};
    use frame_support::pallet_prelude::*;
    use frame_support::sp_runtime::traits::Saturating;
    use frame_support::traits::{BuildGenesisConfig, StorageVersion};
    use frame_support::transactional;
    use frame_support::{BoundedBTreeSet, BoundedVec};
    use frame_system::pallet_prelude::BlockNumberFor;
    use frame_system::pallet_prelude::*;
    use sp_std::marker::PhantomData;
    use sp_std::vec::Vec;

    #[derive(Clone, Encode, Decode, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    #[scale_info(skip_type_params(MaxPlayers))]
    pub struct GameInfo<AccountId, MaxPlayers: Get<u32>> {
        pub server: AccountId,
        pub players: BoundedBTreeSet<AccountId, MaxPlayers>,
        pub started: bool,
        pub ended: bool,
    }

    #[derive(Clone, Encode, Decode, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    #[scale_info(skip_type_params(MaxOutcomeLen))]
    pub struct ProcessedEndCommand<BlockNumber, MaxOutcomeLen: Get<u32>> {
        pub game_id: GameId,
        pub outcome: BoundedVec<u8, MaxOutcomeLen>,
        pub block_number: BlockNumber,
    }

    #[derive(Clone, Encode, Decode, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub struct ProcessedEliminationEvent<AccountId, BlockNumber> {
        pub game_id: GameId,
        pub player: AccountId,
        pub delta: u32,
        pub block_number: BlockNumber,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type AccessControl: pallet_alpha_access::AccessControl<Self::AccountId>;

        #[pallet::constant]
        type MaxPlayersPerGame: Get<u32>;

        #[pallet::constant]
        type MaxBatchAdd: Get<u32>;

        #[pallet::constant]
        type MaxRequestIdLen: Get<u32>;

        #[pallet::constant]
        type MaxOutcomeLen: Get<u32>;

        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        #[pallet::constant]
        type MaxExpirationsPerBlock: Get<u32>;

        type MaxRoundBlocks: Get<BlockNumberFor<Self>>;

        type GameLifecycleHooks: crate::GameLifecycleHooks<Self::AccountId>;

        type WeightInfo: WeightInfo;
    }

    pub type RequestIdOf<T> = BoundedVec<u8, <T as Config>::MaxRequestIdLen>;
    pub type OutcomeOf<T> = BoundedVec<u8, <T as Config>::MaxOutcomeLen>;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn next_game_id)]
    pub type NextGameId<T: Config> = StorageValue<_, GameId, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn games)]
    pub type Games<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        GameId,
        GameInfo<T::AccountId, T::MaxPlayersPerGame>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn eliminations)]
    pub type Eliminations<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        GameId,
        Blake2_128Concat,
        T::AccountId,
        u32,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn is_server_whitelisted)]
    pub type WhitelistedServers<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, (), OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn active_game_by_player)]
    pub type ActiveGameByPlayer<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, GameId, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn game_id_by_round_id)]
    pub type GameIdByRoundId<T: Config> =
        StorageMap<_, Blake2_128Concat, RequestIdOf<T>, GameId, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn processed_end_commands)]
    pub type ProcessedEndCommands<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        RequestIdOf<T>,
        ProcessedEndCommand<BlockNumberFor<T>, T::MaxOutcomeLen>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn processed_elimination_events)]
    pub type ProcessedEliminationEvents<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        RequestIdOf<T>,
        ProcessedEliminationEvent<T::AccountId, BlockNumberFor<T>>,
        OptionQuery,
    >;

    /// BlockNumber => list of game IDs scheduled to auto-end at that block.
    #[pallet::storage]
    #[pallet::getter(fn expirations)]
    pub type Expirations<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        BlockNumberFor<T>,
        BoundedVec<GameId, T::MaxExpirationsPerBlock>,
        ValueQuery,
    >;

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_runtime_upgrade() -> Weight {
            let mut weight = T::DbWeight::get().reads(1);
            if StorageVersion::get::<Pallet<T>>() < STORAGE_VERSION {
                STORAGE_VERSION.put::<Pallet<T>>();
                weight = weight.saturating_add(T::DbWeight::get().writes(1));
            }
            weight
        }

        fn on_initialize(n: BlockNumberFor<T>) -> Weight {
            let games: BoundedVec<GameId, T::MaxExpirationsPerBlock> = Expirations::<T>::take(n);
            let games_len = games.len() as u32;
            let mut removed_players: u32 = 0;

            for game_id in games.into_inner().into_iter() {
                if let Some(mut game) = Games::<T>::get(game_id) {
                    if !game.ended {
                        removed_players = removed_players.saturating_add(game.players.len() as u32);
                        let players: Vec<T::AccountId> = game.players.iter().cloned().collect();
                        let server = game.server.clone();
                        game.ended = true;
                        for player in &players {
                            ActiveGameByPlayer::<T>::remove(&player);
                        }
                        Games::<T>::insert(game_id, game);
                        T::GameLifecycleHooks::on_game_ended(game_id, &server, &players);
                        Self::deposit_event(Event::GameEnded(game_id));
                    }
                }
            }

            T::WeightInfo::on_initialize(games_len, removed_players)
        }
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        ServerWhitelisted(T::AccountId),
        ServerRemoved(T::AccountId),
        GameCreated(GameId, T::AccountId),
        PlayerAdded(GameId, T::AccountId),
        EliminationsRecorded(GameId, T::AccountId, u32, u32),
        GameEnded(GameId),
    }

    #[pallet::error]
    pub enum Error<T> {
        GameNotFound,
        GameAlreadyEnded,
        GameNotStarted,
        GameFull,
        PlayerAlreadyInGame,
        PlayerNotInGame,
        PlayerInAnotherActiveGame,
        NotWhitelistedServer,
        AlreadyWhitelisted,
        NotWhitelisted,
        NotGameOwnerServer,
        TooManyExpirations,
    }

    impl<T: Config> Pallet<T> {
        fn ensure_whitelisted(who: &T::AccountId) -> Result<(), Error<T>> {
            WhitelistedServers::<T>::contains_key(who)
                .then_some(())
                .ok_or(Error::<T>::NotWhitelistedServer)
        }

        fn ensure_game_owned_by_internal(
            game_id: GameId,
            caller: &T::AccountId,
        ) -> Result<GameInfo<T::AccountId, T::MaxPlayersPerGame>, Error<T>> {
            let game = Games::<T>::get(game_id).ok_or(Error::<T>::GameNotFound)?;
            ensure!(caller == &game.server, Error::<T>::NotGameOwnerServer);
            Ok(game)
        }

        fn schedule_expiration(game_id: GameId) -> Result<(), Error<T>> {
            let now = <frame_system::Pallet<T>>::block_number();
            let expire_at = now.saturating_add(T::MaxRoundBlocks::get());
            Expirations::<T>::try_mutate(expire_at, |list| -> Result<(), Error<T>> {
                list.try_push(game_id)
                    .map_err(|_| Error::<T>::TooManyExpirations)?;
                Ok(())
            })
        }

        fn create_game_internal(
            server: &T::AccountId,
            players: BoundedVec<T::AccountId, T::MaxBatchAdd>,
        ) -> Result<GameId, DispatchError> {
            let game_id = NextGameId::<T>::get();
            let mut info = GameInfo::<T::AccountId, T::MaxPlayersPerGame> {
                server: server.clone(),
                players: BoundedBTreeSet::new(),
                started: true,
                ended: false,
            };

            Self::schedule_expiration(game_id)?;

            let mut added_players: Vec<T::AccountId> = Vec::new();
            for player in players.into_iter() {
                if ActiveGameByPlayer::<T>::get(&player).is_some() {
                    continue;
                }
                if info.players.contains(&player) {
                    continue;
                }
                if info.players.try_insert(player.clone()).is_ok() {
                    added_players.push(player);
                } else {
                    break;
                }
            }

            Games::<T>::insert(game_id, info);
            NextGameId::<T>::put(game_id.saturating_add(1));

            Self::deposit_event(Event::GameCreated(game_id, server.clone()));
            for player in &added_players {
                ActiveGameByPlayer::<T>::insert(&player, game_id);
                Self::deposit_event(Event::PlayerAdded(game_id, player.clone()));
            }

            T::GameLifecycleHooks::on_game_created(game_id, server, &added_players)?;
            Ok(game_id)
        }

        fn end_game_internal(game_id: GameId) -> Result<bool, Error<T>> {
            Games::<T>::try_mutate(game_id, |maybe_game| -> Result<bool, Error<T>> {
                let game = maybe_game.as_mut().ok_or(Error::<T>::GameNotFound)?;
                if game.ended {
                    return Ok(false);
                }

                game.ended = true;
                let players: Vec<T::AccountId> = game.players.iter().cloned().collect();
                for player in players {
                    ActiveGameByPlayer::<T>::remove(player);
                }

                Ok(true)
            })
        }

        pub fn ensure_game_owned_by(game_id: GameId, caller: &T::AccountId) -> DispatchResult {
            Self::ensure_game_owned_by_internal(game_id, caller)
                .map(|_| ())
                .map_err(Into::into)
        }

        pub fn ensure_active_game_owned_by(
            game_id: GameId,
            caller: &T::AccountId,
        ) -> DispatchResult {
            let game = Self::ensure_game_owned_by_internal(game_id, caller)?;
            ensure!(game.started, Error::<T>::GameNotStarted);
            ensure!(!game.ended, Error::<T>::GameAlreadyEnded);
            Ok(())
        }

        pub fn ensure_player_in_game(game_id: GameId, player: &T::AccountId) -> DispatchResult {
            let game = Games::<T>::get(game_id).ok_or(Error::<T>::GameNotFound)?;
            ensure!(game.players.contains(player), Error::<T>::PlayerNotInGame);
            Ok(())
        }

        pub fn game_is_active(game_id: GameId) -> bool {
            Games::<T>::get(game_id)
                .map(|game| game.started && !game.ended)
                .unwrap_or(false)
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::add_server())]
        pub fn add_server(origin: T::RuntimeOrigin, server: T::AccountId) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                !WhitelistedServers::<T>::contains_key(&server),
                Error::<T>::AlreadyWhitelisted
            );
            WhitelistedServers::<T>::insert(&server, ());
            Self::deposit_event(Event::ServerWhitelisted(server));
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::remove_server())]
        pub fn remove_server(origin: T::RuntimeOrigin, server: T::AccountId) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                WhitelistedServers::<T>::contains_key(&server),
                Error::<T>::NotWhitelisted
            );
            WhitelistedServers::<T>::remove(&server);
            Self::deposit_event(Event::ServerRemoved(server));
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::create_game_with_round_id(players.len() as u32))]
        #[transactional]
        pub fn create_game_with_round_id(
            origin: T::RuntimeOrigin,
            round_id: RequestIdOf<T>,
            players: BoundedVec<T::AccountId, T::MaxBatchAdd>,
        ) -> DispatchResult {
            let server = ensure_signed(origin)?;
            Self::ensure_whitelisted(&server)?;

            if let Some(existing_game_id) = GameIdByRoundId::<T>::get(&round_id) {
                let existing_game =
                    Games::<T>::get(existing_game_id).ok_or(Error::<T>::GameNotFound)?;
                ensure!(
                    server == existing_game.server,
                    Error::<T>::NotGameOwnerServer
                );
                return Ok(());
            }

            let game_id = Self::create_game_internal(&server, players)?;
            GameIdByRoundId::<T>::insert(round_id, game_id);
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::end_game_with_command_id())]
        pub fn end_game_with_command_id(
            origin: T::RuntimeOrigin,
            game_id: GameId,
            command_id: RequestIdOf<T>,
            outcome: OutcomeOf<T>,
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;
            Self::ensure_whitelisted(&caller)?;

            if let Some(processed) = ProcessedEndCommands::<T>::get(&command_id) {
                Self::ensure_game_owned_by(processed.game_id, &caller)?;
                return Ok(());
            }

            let game = Self::ensure_game_owned_by_internal(game_id, &caller)?;
            let transitioned = Self::end_game_internal(game_id)?;
            let block_number = <frame_system::Pallet<T>>::block_number();

            ProcessedEndCommands::<T>::insert(
                command_id,
                ProcessedEndCommand::<BlockNumberFor<T>, T::MaxOutcomeLen> {
                    game_id,
                    outcome,
                    block_number,
                },
            );

            if transitioned {
                let players: Vec<T::AccountId> = game.players.iter().cloned().collect();
                T::GameLifecycleHooks::on_game_ended(game_id, &game.server, &players);
                Self::deposit_event(Event::GameEnded(game_id));
            }

            Ok(())
        }

        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::record_eliminations_with_event_id())]
        pub fn record_eliminations_with_event_id(
            origin: T::RuntimeOrigin,
            game_id: GameId,
            event_id: RequestIdOf<T>,
            player: T::AccountId,
            count: u32,
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;
            Self::ensure_whitelisted(&caller)?;

            if let Some(processed) = ProcessedEliminationEvents::<T>::get(&event_id) {
                Self::ensure_game_owned_by(processed.game_id, &caller)?;
                return Ok(());
            }

            let new_total =
                Games::<T>::try_mutate_exists(game_id, |maybe_game| -> Result<u32, Error<T>> {
                    let game = maybe_game.as_mut().ok_or(Error::<T>::GameNotFound)?;
                    ensure!(caller == game.server, Error::<T>::NotGameOwnerServer);
                    ensure!(game.started, Error::<T>::GameNotStarted);
                    ensure!(!game.ended, Error::<T>::GameAlreadyEnded);
                    ensure!(game.players.contains(&player), Error::<T>::PlayerNotInGame);

                    let total = Eliminations::<T>::mutate(game_id, &player, |elims| {
                        *elims = elims.saturating_add(count);
                        *elims
                    });

                    Ok(total)
                })?;

            let block_number = <frame_system::Pallet<T>>::block_number();
            ProcessedEliminationEvents::<T>::insert(
                event_id,
                ProcessedEliminationEvent::<T::AccountId, BlockNumberFor<T>> {
                    game_id,
                    player: player.clone(),
                    delta: count,
                    block_number,
                },
            );

            Self::deposit_event(Event::EliminationsRecorded(
                game_id, player, count, new_total,
            ));
            Ok(())
        }
    }

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub initial_servers: Vec<T::AccountId>,
        pub _phantom: PhantomData<T>,
    }

    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self {
                initial_servers: Vec::new(),
                _phantom: Default::default(),
            }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            for server in &self.initial_servers {
                WhitelistedServers::<T>::insert(server, ());
            }
        }
    }
}

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
