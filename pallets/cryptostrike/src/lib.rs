//! Crypto-Strike runtime contract scaffold.
//!
//! Early PIs define and implement the runtime boundary one validated behavior
//! slice at a time.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(dead_code)]

pub use pallet::*;

pub mod weights;
pub use weights::WeightInfo;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::dispatch::DispatchResult;
use frame_support::pallet_prelude::*;
use scale_info::TypeInfo;
use sp_runtime::DispatchError;

pub type SteamHash = [u8; 32];
pub type SessionId = [u8; 32];
pub type ConfigHash = [u8; 32];
pub type MapNameHash = [u8; 32];
pub type RoundHash = [u8; 32];
pub type RosterRoot = [u8; 32];
pub type MenuNonce = [u8; 32];
pub type MetadataHash = [u8; 32];
pub type ReasonHash = [u8; 32];
pub type ServerId = u64;
pub type SeasonId = u32;
pub type RoundNumber = u32;
pub type WeaponId = u32;

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum PlayerRole {
    Terrorist,
    CounterTerrorist,
    Spectator,
    Unassigned,
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum ServerStatus {
    Pending,
    Active,
    Suspended,
    Slashed,
    Retired,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum SettlementParticipant<AccountId> {
    Account(AccountId),
    SteamHash(SteamHash),
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ServerInfo<AccountId, Balance, BlockNumber> {
    pub owner: AccountId,
    pub server_pubkey: [u8; 32],
    pub metadata_hash: MetadataHash,
    pub stake: Balance,
    pub status: ServerStatus,
    pub reputation: i32,
    pub registered_at: BlockNumber,
    pub last_heartbeat: BlockNumber,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ServerAllowance<Balance, BlockNumber> {
    pub max_guap: Balance,
    pub spent_guap: Balance,
    pub expires_at: BlockNumber,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ActivePlayerInfo<AccountId, BlockNumber> {
    pub account: Option<AccountId>,
    pub role: PlayerRole,
    pub joined_at_block: BlockNumber,
    pub last_seen_round: RoundNumber,
    pub expires_at_block: BlockNumber,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct RewardEntry<AccountId, Balance> {
    pub participant: SettlementParticipant<AccountId>,
    pub kills: u32,
    pub valid_damage: u64,
    pub reward_guap: Balance,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct TransferEntry<AccountId, Balance> {
    pub from_account: AccountId,
    pub to: SettlementParticipant<AccountId>,
    pub amount: Balance,
    pub from_userid: u32,
    pub to_userid: u32,
    pub target_role: PlayerRole,
    pub menu_nonce: MenuNonce,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct WeaponSpendEntry<AccountId, Balance> {
    pub account: AccountId,
    pub weapon_id: WeaponId,
    pub guap_cost: Balance,
    pub round_number: RoundNumber,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct PlayerSeasonStats<Balance> {
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub valid_damage: u64,
    pub headshots: u32,
    pub rounds_played: u32,
    pub rounds_won: u32,
    pub guap_earned: Balance,
    pub guap_spent: Balance,
    pub guap_transferred_in: Balance,
    pub guap_transferred_out: Balance,
    pub season_points: u64,
}

impl<Balance: Default> Default for PlayerSeasonStats<Balance> {
    fn default() -> Self {
        Self {
            kills: 0,
            deaths: 0,
            assists: 0,
            valid_damage: 0,
            headshots: 0,
            rounds_played: 0,
            rounds_won: 0,
            guap_earned: Balance::default(),
            guap_spent: Balance::default(),
            guap_transferred_in: Balance::default(),
            guap_transferred_out: Balance::default(),
            season_points: 0,
        }
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct SeasonInfo<BlockNumber> {
    pub metadata_hash: MetadataHash,
    pub started_at: BlockNumber,
    pub ended_at: Option<BlockNumber>,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct UnstakeInfo<Balance, BlockNumber> {
    pub amount: Balance,
    pub eligible_at: BlockNumber,
}

pub trait GuapLedger<AccountId, Balance> {
    fn mint(account: &AccountId, amount: Balance) -> DispatchResult;
    fn burn(account: &AccountId, amount: Balance) -> DispatchResult;
    fn transfer(from: &AccountId, to: &AccountId, amount: Balance) -> DispatchResult;
}

pub trait StakeLedger<AccountId, Balance> {
    fn reserve(account: &AccountId, amount: Balance) -> DispatchResult;
    fn release(account: &AccountId, amount: Balance) -> DispatchResult;
    fn slash_reserved(account: &AccountId, amount: Balance) -> DispatchResult;
}

pub trait ServerSignatureVerifier<Hash, Signature> {
    fn verify(server_pubkey: &[u8; 32], payload_hash: &Hash, signature: &Signature) -> bool;
}

pub trait SteamIdentityProvider<AccountId> {
    fn account_for_steam_hash(steam_hash: SteamHash) -> Option<AccountId>;
    fn steam_hash_for_account(account: &AccountId) -> Option<SteamHash>;
    fn is_frozen(account: &AccountId) -> bool;
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{traits::StorageVersion, transactional};
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::{AtLeast32BitUnsigned, Hash, Saturating};
    use sp_std::boxed::Box;

    type BalanceOf<T> = <T as Config>::Balance;
    type BlockNumberOf<T> = BlockNumberFor<T>;
    type ServerInfoOf<T> =
        ServerInfo<<T as frame_system::Config>::AccountId, BalanceOf<T>, BlockNumberOf<T>>;
    type ServerAllowanceOf<T> = ServerAllowance<BalanceOf<T>, BlockNumberOf<T>>;
    type ActivePlayerInfoOf<T> =
        ActivePlayerInfo<<T as frame_system::Config>::AccountId, BlockNumberOf<T>>;
    type PlayerSeasonStatsOf<T> = PlayerSeasonStats<BalanceOf<T>>;
    type SeasonInfoOf<T> = SeasonInfo<BlockNumberOf<T>>;
    type UnstakeInfoOf<T> = UnstakeInfo<BalanceOf<T>, BlockNumberOf<T>>;

    #[derive(Encode, Decode, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound, TypeInfo)]
    #[scale_info(skip_type_params(T))]
    pub struct RoundSettlement<T: Config> {
        pub server_id: ServerId,
        pub session_id: SessionId,
        pub map_name_hash: MapNameHash,
        pub round_number: RoundNumber,
        pub previous_round_hash: RoundHash,
        pub roster_root: RosterRoot,
        pub reward_entries:
            BoundedVec<RewardEntry<T::AccountId, BalanceOf<T>>, T::MaxSettlementEntries>,
        pub weapon_spend_entries:
            BoundedVec<WeaponSpendEntry<T::AccountId, BalanceOf<T>>, T::MaxSettlementEntries>,
        pub guap_transfer_entries:
            BoundedVec<TransferEntry<T::AccountId, BalanceOf<T>>, T::MaxSettlementEntries>,
        pub config_hash: ConfigHash,
        pub server_signature: BoundedVec<u8, T::MaxServerSignatureLen>,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        type Balance: Parameter
            + Member
            + AtLeast32BitUnsigned
            + Default
            + Copy
            + MaxEncodedLen
            + Saturating
            + TypeInfo;

        #[pallet::constant]
        type MaxSettlementEntries: Get<u32>;

        #[pallet::constant]
        type MaxServerSignatureLen: Get<u32>;

        #[pallet::constant]
        type MinServerStake: Get<Self::Balance>;

        #[pallet::constant]
        type UnstakeDelay: Get<BlockNumberFor<Self>>;

        type GuapLedger: crate::GuapLedger<Self::AccountId, Self::Balance>;

        type StakeLedger: crate::StakeLedger<Self::AccountId, Self::Balance>;

        type ServerSignatureVerifier: crate::ServerSignatureVerifier<
            Self::Hash,
            BoundedVec<u8, Self::MaxServerSignatureLen>,
        >;

        type IdentityProvider: crate::SteamIdentityProvider<Self::AccountId>;

        type WeightInfo: WeightInfo;
    }

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub type PendingGuapClaims<T: Config> =
        StorageMap<_, Blake2_128Concat, SteamHash, BalanceOf<T>, ValueQuery>;

    #[pallet::type_value]
    pub fn DefaultNextServerId() -> ServerId {
        1
    }

    #[pallet::storage]
    pub type NextServerId<T: Config> = StorageValue<_, ServerId, ValueQuery, DefaultNextServerId>;

    #[pallet::storage]
    pub type Servers<T: Config> =
        StorageMap<_, Blake2_128Concat, ServerId, ServerInfoOf<T>, OptionQuery>;

    #[pallet::storage]
    pub type ServerIdByPubkey<T: Config> =
        StorageMap<_, Blake2_128Concat, [u8; 32], ServerId, OptionQuery>;

    #[pallet::storage]
    pub type PendingUnstakes<T: Config> =
        StorageMap<_, Blake2_128Concat, ServerId, UnstakeInfoOf<T>, OptionQuery>;

    #[pallet::storage]
    pub type ServerAllowances<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        ServerId,
        ServerAllowanceOf<T>,
        OptionQuery,
    >;

    #[pallet::storage]
    pub type ActiveSessionRoster<T: Config> =
        StorageDoubleMap<_, Blake2_128Concat, ServerId, Blake2_128Concat, SessionId, RosterRoot>;

    #[pallet::storage]
    pub type ActivePlayer<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, ServerId>,
            NMapKey<Blake2_128Concat, SessionId>,
            NMapKey<Blake2_128Concat, SteamHash>,
        ),
        ActivePlayerInfoOf<T>,
        OptionQuery,
    >;

    #[pallet::storage]
    pub type SettledRounds<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, ServerId>,
            NMapKey<Blake2_128Concat, SessionId>,
            NMapKey<Blake2_128Concat, RoundNumber>,
        ),
        (),
        OptionQuery,
    >;

    #[pallet::storage]
    pub type UsedTransferNonces<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, ServerId>,
            NMapKey<Blake2_128Concat, SessionId>,
            NMapKey<Blake2_128Concat, MenuNonce>,
        ),
        (),
        OptionQuery,
    >;

    #[pallet::storage]
    pub type CurrentSeason<T: Config> = StorageValue<_, Option<SeasonId>, ValueQuery>;

    #[pallet::storage]
    pub type Seasons<T: Config> =
        StorageMap<_, Blake2_128Concat, SeasonId, SeasonInfoOf<T>, OptionQuery>;

    #[pallet::storage]
    pub type SeasonStats<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        SeasonId,
        Blake2_128Concat,
        T::AccountId,
        PlayerSeasonStatsOf<T>,
        OptionQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        ServerRegistered {
            server_id: ServerId,
            owner: T::AccountId,
        },
        ServerStaked {
            server_id: ServerId,
            amount: BalanceOf<T>,
        },
        ServerSlashed {
            server_id: ServerId,
            amount: BalanceOf<T>,
        },
        ServerUnstakeRequested {
            server_id: ServerId,
            amount: BalanceOf<T>,
            eligible_at: BlockNumberOf<T>,
        },
        ServerUnstaked {
            server_id: ServerId,
            amount: BalanceOf<T>,
        },
        ServerHeartbeat {
            server_id: ServerId,
        },
        ActiveSessionRosterUpdated {
            server_id: ServerId,
            session_id: SessionId,
            roster_root: RosterRoot,
        },
        ActivePlayerUpserted {
            server_id: ServerId,
            session_id: SessionId,
            steam_hash: SteamHash,
            account: Option<T::AccountId>,
            role: PlayerRole,
        },
        ActivePlayerRemoved {
            server_id: ServerId,
            session_id: SessionId,
            steam_hash: SteamHash,
        },
        ServerStatusChanged {
            server_id: ServerId,
            status: ServerStatus,
        },
        ServerAllowanceAuthorized {
            account: T::AccountId,
            server_id: ServerId,
            max_guap: BalanceOf<T>,
            expires_at: BlockNumberOf<T>,
        },
        ServerAllowanceRevoked {
            account: T::AccountId,
            server_id: ServerId,
        },
        RoundSettled {
            server_id: ServerId,
            session_id: SessionId,
            round_number: RoundNumber,
        },
        GuapMinted {
            account: T::AccountId,
            amount: BalanceOf<T>,
        },
        GuapTransferredInServer {
            from: T::AccountId,
            to: SettlementParticipant<T::AccountId>,
            amount: BalanceOf<T>,
        },
        GuapClaimCreated {
            steam_hash: SteamHash,
            amount: BalanceOf<T>,
        },
        GuapClaimed {
            steam_hash: SteamHash,
            account: T::AccountId,
            amount: BalanceOf<T>,
        },
        WeaponSpendSettled {
            account: T::AccountId,
            weapon_id: WeaponId,
            amount: BalanceOf<T>,
        },
        SeasonStarted {
            season_id: SeasonId,
        },
        SeasonEnded {
            season_id: SeasonId,
        },
        SeasonStatsUpdated {
            season_id: SeasonId,
            account: T::AccountId,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        NotImplemented,
        SteamHashNotLinked,
        NoPendingGuapClaim,
        InvalidServerPubkey,
        ServerPubkeyAlreadyRegistered,
        ServerIdOverflow,
        ServerNotFound,
        ServerNotActive,
        ServerCannotUnstake,
        NotServerOwner,
        StakeBelowMinimum,
        InvalidStakeAmount,
        UnstakeAlreadyRequested,
        UnstakeNotRequested,
        UnstakeNotReady,
        LinkedAccountMismatch,
        ActivePlayerExpiresInPast,
        ActivePlayerNotFound,
        MissingSessionRoster,
        RosterRootMismatch,
        InvalidServerSignature,
        SettlementParticipantNotActive,
        ActivePlayerExpired,
        DuplicateRound,
        DuplicateTransferNonce,
        SeasonAlreadyActive,
        SeasonAlreadyExists,
        SeasonNotActive,
        NotSameSession,
        InvalidAllowanceAmount,
        AllowanceExpiresInPast,
        ServerAllowanceNotFound,
        InsufficientAllowance,
        InsufficientBalance,
        PlayerFrozen,
        SettlementTooLarge,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::claim_pending_guap())]
        #[transactional]
        pub fn claim_pending_guap(origin: OriginFor<T>) -> DispatchResult {
            let account = ensure_signed(origin)?;
            let steam_hash = T::IdentityProvider::steam_hash_for_account(&account)
                .ok_or(Error::<T>::SteamHashNotLinked)?;
            Self::ensure_account_not_frozen(&account)?;
            let amount = PendingGuapClaims::<T>::get(steam_hash);
            ensure!(
                amount > BalanceOf::<T>::default(),
                Error::<T>::NoPendingGuapClaim
            );

            T::GuapLedger::mint(&account, amount)?;
            PendingGuapClaims::<T>::remove(steam_hash);
            Self::deposit_event(Event::GuapClaimed {
                steam_hash,
                account,
                amount,
            });
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::register_server())]
        #[transactional]
        pub fn register_server(
            origin: OriginFor<T>,
            server_pubkey: [u8; 32],
            metadata_hash: MetadataHash,
            stake: BalanceOf<T>,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            ensure!(
                server_pubkey.iter().any(|byte| *byte != 0),
                Error::<T>::InvalidServerPubkey
            );
            ensure!(
                stake >= T::MinServerStake::get(),
                Error::<T>::StakeBelowMinimum
            );
            ensure!(
                !ServerIdByPubkey::<T>::contains_key(server_pubkey),
                Error::<T>::ServerPubkeyAlreadyRegistered
            );

            let server_id = NextServerId::<T>::get();
            let next_server_id = server_id
                .checked_add(1)
                .ok_or(Error::<T>::ServerIdOverflow)?;
            let now = frame_system::Pallet::<T>::block_number();

            T::StakeLedger::reserve(&owner, stake).map_err(|_| Error::<T>::InsufficientBalance)?;
            Servers::<T>::insert(
                server_id,
                ServerInfo {
                    owner: owner.clone(),
                    server_pubkey,
                    metadata_hash,
                    stake,
                    status: ServerStatus::Pending,
                    reputation: 0,
                    registered_at: now,
                    last_heartbeat: now,
                },
            );
            ServerIdByPubkey::<T>::insert(server_pubkey, server_id);
            NextServerId::<T>::put(next_server_id);

            Self::deposit_event(Event::ServerRegistered { server_id, owner });
            Ok(())
        }

        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::increase_server_stake())]
        pub fn increase_server_stake(
            origin: OriginFor<T>,
            server_id: ServerId,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            ensure!(
                amount > BalanceOf::<T>::default(),
                Error::<T>::InvalidStakeAmount
            );

            let server = Servers::<T>::get(server_id).ok_or(Error::<T>::ServerNotFound)?;
            ensure!(server.owner == owner, Error::<T>::NotServerOwner);

            T::StakeLedger::reserve(&owner, amount).map_err(|_| Error::<T>::InsufficientBalance)?;
            Servers::<T>::mutate(server_id, |maybe_server| {
                if let Some(server) = maybe_server {
                    server.stake = server.stake.saturating_add(amount);
                }
            });
            PendingUnstakes::<T>::remove(server_id);

            Self::deposit_event(Event::ServerStaked { server_id, amount });
            Ok(())
        }

        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::request_unstake())]
        pub fn request_unstake(origin: OriginFor<T>, server_id: ServerId) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            ensure!(
                !PendingUnstakes::<T>::contains_key(server_id),
                Error::<T>::UnstakeAlreadyRequested
            );

            let now = frame_system::Pallet::<T>::block_number();
            let eligible_at = now.saturating_add(T::UnstakeDelay::get());
            let mut unstake_amount = BalanceOf::<T>::default();

            Servers::<T>::try_mutate(server_id, |maybe_server| -> DispatchResult {
                let server = maybe_server.as_mut().ok_or(Error::<T>::ServerNotFound)?;
                ensure!(server.owner == owner, Error::<T>::NotServerOwner);
                ensure!(
                    server.status != ServerStatus::Slashed,
                    Error::<T>::ServerCannotUnstake
                );
                ensure!(
                    server.stake > BalanceOf::<T>::default(),
                    Error::<T>::InvalidStakeAmount
                );

                if matches!(server.status, ServerStatus::Active | ServerStatus::Retired) {
                    server.status = ServerStatus::Retired;
                }
                unstake_amount = server.stake;
                Ok(())
            })?;

            PendingUnstakes::<T>::insert(
                server_id,
                UnstakeInfo {
                    amount: unstake_amount,
                    eligible_at,
                },
            );
            Self::deposit_event(Event::ServerUnstakeRequested {
                server_id,
                amount: unstake_amount,
                eligible_at,
            });
            Ok(())
        }

        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::finalize_unstake())]
        #[transactional]
        pub fn finalize_unstake(origin: OriginFor<T>, server_id: ServerId) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            let server = Servers::<T>::get(server_id).ok_or(Error::<T>::ServerNotFound)?;
            ensure!(server.owner == owner, Error::<T>::NotServerOwner);
            let unstake =
                PendingUnstakes::<T>::get(server_id).ok_or(Error::<T>::UnstakeNotRequested)?;
            ensure!(
                frame_system::Pallet::<T>::block_number() >= unstake.eligible_at,
                Error::<T>::UnstakeNotReady
            );

            T::StakeLedger::release(&owner, unstake.amount)
                .map_err(|_| Error::<T>::InsufficientBalance)?;
            PendingUnstakes::<T>::remove(server_id);
            ServerIdByPubkey::<T>::remove(server.server_pubkey);
            Servers::<T>::remove(server_id);

            Self::deposit_event(Event::ServerUnstaked {
                server_id,
                amount: unstake.amount,
            });
            Ok(())
        }

        #[pallet::call_index(7)]
        #[pallet::weight(T::WeightInfo::heartbeat())]
        pub fn heartbeat(
            origin: OriginFor<T>,
            server_id: ServerId,
            _roster_root: RosterRoot,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            Servers::<T>::try_mutate(server_id, |maybe_server| -> DispatchResult {
                let server = maybe_server.as_mut().ok_or(Error::<T>::ServerNotFound)?;
                ensure!(server.owner == owner, Error::<T>::NotServerOwner);
                server.last_heartbeat = frame_system::Pallet::<T>::block_number();
                Ok(())
            })?;

            Self::deposit_event(Event::ServerHeartbeat { server_id });
            Ok(())
        }

        #[pallet::call_index(8)]
        #[pallet::weight(T::WeightInfo::authorize_server_allowance())]
        pub fn authorize_server_allowance(
            origin: OriginFor<T>,
            server_id: ServerId,
            max_guap: BalanceOf<T>,
            expires_at: BlockNumberOf<T>,
        ) -> DispatchResult {
            let account = ensure_signed(origin)?;
            ensure!(
                max_guap > BalanceOf::<T>::default(),
                Error::<T>::InvalidAllowanceAmount
            );
            ensure!(
                expires_at > frame_system::Pallet::<T>::block_number(),
                Error::<T>::AllowanceExpiresInPast
            );
            let server = Servers::<T>::get(server_id).ok_or(Error::<T>::ServerNotFound)?;
            ensure!(
                server.status == ServerStatus::Active,
                Error::<T>::ServerNotActive
            );
            Self::ensure_account_not_frozen(&account)?;

            ServerAllowances::<T>::insert(
                &account,
                server_id,
                ServerAllowance {
                    max_guap,
                    spent_guap: BalanceOf::<T>::default(),
                    expires_at,
                },
            );

            Self::deposit_event(Event::ServerAllowanceAuthorized {
                account,
                server_id,
                max_guap,
                expires_at,
            });
            Ok(())
        }

        #[pallet::call_index(9)]
        #[pallet::weight(T::WeightInfo::revoke_server_allowance())]
        pub fn revoke_server_allowance(
            origin: OriginFor<T>,
            server_id: ServerId,
        ) -> DispatchResult {
            let account = ensure_signed(origin)?;
            ensure!(
                ServerAllowances::<T>::contains_key(&account, server_id),
                Error::<T>::ServerAllowanceNotFound
            );
            ServerAllowances::<T>::remove(&account, server_id);

            Self::deposit_event(Event::ServerAllowanceRevoked { account, server_id });
            Ok(())
        }

        #[pallet::call_index(10)]
        #[pallet::weight(T::WeightInfo::submit_round_settlement())]
        #[transactional]
        pub fn submit_round_settlement(
            origin: OriginFor<T>,
            settlement: Box<RoundSettlement<T>>,
        ) -> DispatchResult {
            let server_owner = ensure_signed(origin)?;
            let settlement = *settlement;
            let server = Self::ensure_active_server_owner(&server_owner, settlement.server_id)?;
            let payload_hash = Self::settlement_payload_hash(&settlement);
            ensure!(
                T::ServerSignatureVerifier::verify(
                    &server.server_pubkey,
                    &payload_hash,
                    &settlement.server_signature
                ),
                Error::<T>::InvalidServerSignature
            );
            ensure!(
                !SettledRounds::<T>::contains_key((
                    settlement.server_id,
                    settlement.session_id,
                    settlement.round_number,
                )),
                Error::<T>::DuplicateRound
            );

            let stored_roster_root =
                ActiveSessionRoster::<T>::get(settlement.server_id, settlement.session_id)
                    .ok_or(Error::<T>::MissingSessionRoster)?;
            ensure!(
                stored_roster_root == settlement.roster_root,
                Error::<T>::RosterRootMismatch
            );

            let now = frame_system::Pallet::<T>::block_number();

            for reward in settlement.reward_entries.iter() {
                Self::ensure_participant_active(
                    settlement.server_id,
                    settlement.session_id,
                    &reward.participant,
                    now,
                )?;
            }

            for spend in settlement.weapon_spend_entries.iter() {
                Self::ensure_account_active(
                    settlement.server_id,
                    settlement.session_id,
                    &spend.account,
                    now,
                )?;
            }

            for transfer in settlement.guap_transfer_entries.iter() {
                Self::ensure_account_active(
                    settlement.server_id,
                    settlement.session_id,
                    &transfer.from_account,
                    now,
                )?;
                Self::ensure_participant_active(
                    settlement.server_id,
                    settlement.session_id,
                    &transfer.to,
                    now,
                )?;
                ensure!(
                    !UsedTransferNonces::<T>::contains_key((
                        settlement.server_id,
                        settlement.session_id,
                        transfer.menu_nonce,
                    )),
                    Error::<T>::DuplicateTransferNonce
                );
            }

            for spend in settlement.weapon_spend_entries.iter() {
                Self::ensure_allowance_available(
                    &spend.account,
                    settlement.server_id,
                    spend.guap_cost,
                    now,
                )?;
            }

            for transfer in settlement.guap_transfer_entries.iter() {
                Self::ensure_allowance_available(
                    &transfer.from_account,
                    settlement.server_id,
                    transfer.amount,
                    now,
                )?;
            }

            Self::apply_guap_economy(&settlement, now)?;

            for transfer in settlement.guap_transfer_entries.iter() {
                UsedTransferNonces::<T>::insert(
                    (
                        settlement.server_id,
                        settlement.session_id,
                        transfer.menu_nonce,
                    ),
                    (),
                );
            }

            Self::apply_season_stats(&settlement);

            SettledRounds::<T>::insert(
                (
                    settlement.server_id,
                    settlement.session_id,
                    settlement.round_number,
                ),
                (),
            );
            Self::deposit_event(Event::RoundSettled {
                server_id: settlement.server_id,
                session_id: settlement.session_id,
                round_number: settlement.round_number,
            });
            Ok(())
        }

        #[pallet::call_index(11)]
        #[pallet::weight(T::WeightInfo::start_season())]
        pub fn start_season(
            origin: OriginFor<T>,
            season_id: SeasonId,
            metadata_hash: MetadataHash,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                CurrentSeason::<T>::get().is_none(),
                Error::<T>::SeasonAlreadyActive
            );
            ensure!(
                !Seasons::<T>::contains_key(season_id),
                Error::<T>::SeasonAlreadyExists
            );

            let now = frame_system::Pallet::<T>::block_number();
            Seasons::<T>::insert(
                season_id,
                SeasonInfo {
                    metadata_hash,
                    started_at: now,
                    ended_at: None,
                },
            );
            CurrentSeason::<T>::put(Some(season_id));
            Self::deposit_event(Event::SeasonStarted { season_id });
            Ok(())
        }

        #[pallet::call_index(12)]
        #[pallet::weight(T::WeightInfo::end_season())]
        pub fn end_season(origin: OriginFor<T>, season_id: SeasonId) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                CurrentSeason::<T>::get() == Some(season_id),
                Error::<T>::SeasonNotActive
            );

            Seasons::<T>::try_mutate(season_id, |maybe_season| -> DispatchResult {
                let season = maybe_season.as_mut().ok_or(Error::<T>::SeasonNotActive)?;
                season.ended_at = Some(frame_system::Pallet::<T>::block_number());
                Ok(())
            })?;
            CurrentSeason::<T>::put(None::<SeasonId>);
            Self::deposit_event(Event::SeasonEnded { season_id });
            Ok(())
        }

        #[pallet::call_index(13)]
        #[pallet::weight(T::WeightInfo::set_server_status())]
        pub fn set_server_status(
            origin: OriginFor<T>,
            server_id: ServerId,
            status: ServerStatus,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Servers::<T>::try_mutate(server_id, |maybe_server| -> DispatchResult {
                let server = maybe_server.as_mut().ok_or(Error::<T>::ServerNotFound)?;
                server.status = status;
                Ok(())
            })?;

            Self::deposit_event(Event::ServerStatusChanged { server_id, status });
            Ok(())
        }

        #[pallet::call_index(14)]
        #[pallet::weight(T::WeightInfo::slash_server())]
        pub fn slash_server(
            origin: OriginFor<T>,
            server_id: ServerId,
            amount: BalanceOf<T>,
            _reason_hash: ReasonHash,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                amount > BalanceOf::<T>::default(),
                Error::<T>::InvalidStakeAmount
            );

            let mut server = Servers::<T>::get(server_id).ok_or(Error::<T>::ServerNotFound)?;
            ensure!(amount <= server.stake, Error::<T>::InvalidStakeAmount);

            T::StakeLedger::slash_reserved(&server.owner, amount)
                .map_err(|_| Error::<T>::InsufficientBalance)?;
            server.stake = server.stake.saturating_sub(amount);
            server.status = ServerStatus::Slashed;
            Servers::<T>::insert(server_id, server);
            PendingUnstakes::<T>::remove(server_id);

            Self::deposit_event(Event::ServerSlashed { server_id, amount });
            Ok(())
        }

        #[pallet::call_index(17)]
        #[pallet::weight(T::WeightInfo::set_session_roster_root())]
        pub fn set_session_roster_root(
            origin: OriginFor<T>,
            server_id: ServerId,
            session_id: SessionId,
            roster_root: RosterRoot,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            Self::ensure_active_server_owner(&owner, server_id)?;

            ActiveSessionRoster::<T>::insert(server_id, session_id, roster_root);
            Self::deposit_event(Event::ActiveSessionRosterUpdated {
                server_id,
                session_id,
                roster_root,
            });
            Ok(())
        }

        #[pallet::call_index(18)]
        #[pallet::weight(T::WeightInfo::upsert_active_player())]
        pub fn upsert_active_player(
            origin: OriginFor<T>,
            server_id: ServerId,
            session_id: SessionId,
            steam_hash: SteamHash,
            account: Option<T::AccountId>,
            role: PlayerRole,
            last_seen_round: RoundNumber,
            expires_at_block: BlockNumberOf<T>,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            Self::ensure_active_server_owner(&owner, server_id)?;
            let now = frame_system::Pallet::<T>::block_number();
            ensure!(
                expires_at_block > now,
                Error::<T>::ActivePlayerExpiresInPast
            );

            let canonical_account = match account {
                Some(provided_account) => {
                    let linked_account = T::IdentityProvider::account_for_steam_hash(steam_hash)
                        .ok_or(Error::<T>::LinkedAccountMismatch)?;
                    ensure!(
                        linked_account == provided_account,
                        Error::<T>::LinkedAccountMismatch
                    );
                    Some(provided_account)
                }
                None => T::IdentityProvider::account_for_steam_hash(steam_hash),
            };

            let joined_at_block = ActivePlayer::<T>::get((server_id, session_id, steam_hash))
                .map(|info| info.joined_at_block)
                .unwrap_or(now);

            ActivePlayer::<T>::insert(
                (server_id, session_id, steam_hash),
                ActivePlayerInfo {
                    account: canonical_account.clone(),
                    role,
                    joined_at_block,
                    last_seen_round,
                    expires_at_block,
                },
            );
            Self::deposit_event(Event::ActivePlayerUpserted {
                server_id,
                session_id,
                steam_hash,
                account: canonical_account,
                role,
            });
            Ok(())
        }

        #[pallet::call_index(19)]
        #[pallet::weight(T::WeightInfo::remove_active_player())]
        pub fn remove_active_player(
            origin: OriginFor<T>,
            server_id: ServerId,
            session_id: SessionId,
            steam_hash: SteamHash,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            Self::ensure_active_server_owner(&owner, server_id)?;
            ensure!(
                ActivePlayer::<T>::contains_key((server_id, session_id, steam_hash)),
                Error::<T>::ActivePlayerNotFound
            );
            ActivePlayer::<T>::remove((server_id, session_id, steam_hash));
            Self::deposit_event(Event::ActivePlayerRemoved {
                server_id,
                session_id,
                steam_hash,
            });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn ensure_active_server_owner(
            owner: &T::AccountId,
            server_id: ServerId,
        ) -> Result<ServerInfoOf<T>, DispatchError> {
            let server = Servers::<T>::get(server_id).ok_or(Error::<T>::ServerNotFound)?;
            ensure!(&server.owner == owner, Error::<T>::NotServerOwner);
            ensure!(
                server.status == ServerStatus::Active,
                Error::<T>::ServerNotActive
            );
            ensure!(
                server.stake >= T::MinServerStake::get(),
                Error::<T>::ServerNotActive
            );
            ensure!(
                !PendingUnstakes::<T>::contains_key(server_id),
                Error::<T>::ServerNotActive
            );
            Ok(server)
        }

        pub(crate) fn settlement_payload_hash(settlement: &RoundSettlement<T>) -> T::Hash {
            T::Hashing::hash_of(&(
                settlement.server_id,
                settlement.session_id,
                settlement.map_name_hash,
                settlement.round_number,
                settlement.previous_round_hash,
                settlement.roster_root,
                &settlement.reward_entries,
                &settlement.weapon_spend_entries,
                &settlement.guap_transfer_entries,
                settlement.config_hash,
            ))
        }

        fn ensure_participant_active(
            server_id: ServerId,
            session_id: SessionId,
            participant: &SettlementParticipant<T::AccountId>,
            now: BlockNumberOf<T>,
        ) -> DispatchResult {
            match participant {
                SettlementParticipant::Account(account) => {
                    Self::ensure_account_active(server_id, session_id, account, now)
                }
                SettlementParticipant::SteamHash(steam_hash) => {
                    Self::ensure_steam_hash_active(server_id, session_id, *steam_hash, None, now)
                }
            }
        }

        fn ensure_account_active(
            server_id: ServerId,
            session_id: SessionId,
            account: &T::AccountId,
            now: BlockNumberOf<T>,
        ) -> DispatchResult {
            Self::ensure_account_not_frozen(account)?;
            let steam_hash = T::IdentityProvider::steam_hash_for_account(account)
                .ok_or(Error::<T>::SettlementParticipantNotActive)?;
            Self::ensure_steam_hash_active(server_id, session_id, steam_hash, Some(account), now)
        }

        fn ensure_account_not_frozen(account: &T::AccountId) -> DispatchResult {
            ensure!(
                !T::IdentityProvider::is_frozen(account),
                Error::<T>::PlayerFrozen
            );
            Ok(())
        }

        fn ensure_steam_hash_active(
            server_id: ServerId,
            session_id: SessionId,
            steam_hash: SteamHash,
            expected_account: Option<&T::AccountId>,
            now: BlockNumberOf<T>,
        ) -> DispatchResult {
            let active_player = ActivePlayer::<T>::get((server_id, session_id, steam_hash))
                .ok_or(Error::<T>::SettlementParticipantNotActive)?;
            ensure!(
                active_player.expires_at_block > now,
                Error::<T>::ActivePlayerExpired
            );

            if let Some(account) = expected_account {
                ensure!(
                    active_player.account.as_ref() == Some(account),
                    Error::<T>::SettlementParticipantNotActive
                );
            }

            let linked_account = expected_account
                .cloned()
                .or(active_player.account)
                .or_else(|| T::IdentityProvider::account_for_steam_hash(steam_hash));
            if let Some(account) = linked_account {
                Self::ensure_account_not_frozen(&account)?;
            }

            Ok(())
        }

        fn ensure_allowance_available(
            account: &T::AccountId,
            server_id: ServerId,
            amount: BalanceOf<T>,
            now: BlockNumberOf<T>,
        ) -> DispatchResult {
            if amount == BalanceOf::<T>::default() {
                return Ok(());
            }

            let allowance = ServerAllowances::<T>::get(account, server_id)
                .ok_or(Error::<T>::InsufficientAllowance)?;
            ensure!(
                allowance.expires_at > now,
                Error::<T>::InsufficientAllowance
            );
            let remaining = allowance.max_guap.saturating_sub(allowance.spent_guap);
            ensure!(remaining >= amount, Error::<T>::InsufficientAllowance);
            Ok(())
        }

        fn consume_allowance(
            account: &T::AccountId,
            server_id: ServerId,
            amount: BalanceOf<T>,
            now: BlockNumberOf<T>,
        ) -> DispatchResult {
            if amount == BalanceOf::<T>::default() {
                return Ok(());
            }

            ServerAllowances::<T>::try_mutate(
                account,
                server_id,
                |maybe_allowance| -> DispatchResult {
                    let allowance = maybe_allowance
                        .as_mut()
                        .ok_or(Error::<T>::InsufficientAllowance)?;
                    ensure!(
                        allowance.expires_at > now,
                        Error::<T>::InsufficientAllowance
                    );
                    let remaining = allowance.max_guap.saturating_sub(allowance.spent_guap);
                    ensure!(remaining >= amount, Error::<T>::InsufficientAllowance);
                    allowance.spent_guap = allowance.spent_guap.saturating_add(amount);
                    Ok(())
                },
            )
        }

        fn apply_guap_economy(
            settlement: &RoundSettlement<T>,
            now: BlockNumberOf<T>,
        ) -> DispatchResult {
            for spend in settlement.weapon_spend_entries.iter() {
                if spend.guap_cost > BalanceOf::<T>::default() {
                    T::GuapLedger::burn(&spend.account, spend.guap_cost)
                        .map_err(|_| Error::<T>::InsufficientBalance)?;
                    Self::consume_allowance(
                        &spend.account,
                        settlement.server_id,
                        spend.guap_cost,
                        now,
                    )?;
                    Self::deposit_event(Event::WeaponSpendSettled {
                        account: spend.account.clone(),
                        weapon_id: spend.weapon_id,
                        amount: spend.guap_cost,
                    });
                }
            }

            for transfer in settlement.guap_transfer_entries.iter() {
                if transfer.amount > BalanceOf::<T>::default() {
                    Self::settle_transfer(transfer, settlement.server_id, now)?;
                }
            }

            for reward in settlement.reward_entries.iter() {
                Self::settle_reward(&reward.participant, reward.reward_guap)?;
            }

            Ok(())
        }

        fn settle_reward(
            participant: &SettlementParticipant<T::AccountId>,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            if amount == BalanceOf::<T>::default() {
                return Ok(());
            }

            if let Some(account) = Self::account_for_participant(participant) {
                T::GuapLedger::mint(&account, amount)?;
                Self::deposit_event(Event::GuapMinted { account, amount });
                return Ok(());
            }

            if let SettlementParticipant::SteamHash(steam_hash) = participant {
                Self::create_pending_claim(*steam_hash, amount);
            }

            Ok(())
        }

        fn settle_transfer(
            transfer: &TransferEntry<T::AccountId, BalanceOf<T>>,
            server_id: ServerId,
            now: BlockNumberOf<T>,
        ) -> DispatchResult {
            if let Some(to_account) = Self::account_for_participant(&transfer.to) {
                T::GuapLedger::transfer(&transfer.from_account, &to_account, transfer.amount)
                    .map_err(|_| Error::<T>::InsufficientBalance)?;
            } else if let SettlementParticipant::SteamHash(steam_hash) = &transfer.to {
                T::GuapLedger::burn(&transfer.from_account, transfer.amount)
                    .map_err(|_| Error::<T>::InsufficientBalance)?;
                Self::create_pending_claim(*steam_hash, transfer.amount);
            }

            Self::consume_allowance(&transfer.from_account, server_id, transfer.amount, now)?;
            Self::deposit_event(Event::GuapTransferredInServer {
                from: transfer.from_account.clone(),
                to: transfer.to.clone(),
                amount: transfer.amount,
            });
            Ok(())
        }

        fn create_pending_claim(steam_hash: SteamHash, amount: BalanceOf<T>) {
            PendingGuapClaims::<T>::mutate(steam_hash, |pending| {
                *pending = pending.saturating_add(amount);
            });
            Self::deposit_event(Event::GuapClaimCreated { steam_hash, amount });
        }

        fn apply_season_stats(settlement: &RoundSettlement<T>) {
            let Some(season_id) = CurrentSeason::<T>::get() else {
                return;
            };

            for reward in settlement.reward_entries.iter() {
                if let Some(account) = Self::account_for_participant(&reward.participant) {
                    let earned = reward.reward_guap;
                    let season_points = (reward.kills as u64)
                        .saturating_mul(100)
                        .saturating_add(reward.valid_damage);
                    Self::mutate_season_stats(season_id, account, |stats| {
                        stats.kills = stats.kills.saturating_add(reward.kills);
                        stats.valid_damage = stats.valid_damage.saturating_add(reward.valid_damage);
                        stats.rounds_played = stats.rounds_played.saturating_add(1);
                        stats.guap_earned = stats.guap_earned.saturating_add(earned);
                        stats.season_points = stats.season_points.saturating_add(season_points);
                    });
                }
            }

            for spend in settlement.weapon_spend_entries.iter() {
                Self::mutate_season_stats(season_id, spend.account.clone(), |stats| {
                    stats.guap_spent = stats.guap_spent.saturating_add(spend.guap_cost);
                });
            }

            for transfer in settlement.guap_transfer_entries.iter() {
                Self::mutate_season_stats(season_id, transfer.from_account.clone(), |stats| {
                    stats.guap_transferred_out =
                        stats.guap_transferred_out.saturating_add(transfer.amount);
                });

                if let Some(to_account) = Self::account_for_participant(&transfer.to) {
                    Self::mutate_season_stats(season_id, to_account, |stats| {
                        stats.guap_transferred_in =
                            stats.guap_transferred_in.saturating_add(transfer.amount);
                    });
                }
            }
        }

        fn mutate_season_stats<F>(season_id: SeasonId, account: T::AccountId, mutate: F)
        where
            F: FnOnce(&mut PlayerSeasonStatsOf<T>),
        {
            SeasonStats::<T>::mutate(season_id, &account, |maybe_stats| {
                let stats = maybe_stats.get_or_insert_with(PlayerSeasonStats::default);
                mutate(stats);
            });
            Self::deposit_event(Event::SeasonStatsUpdated { season_id, account });
        }

        fn account_for_participant(
            participant: &SettlementParticipant<T::AccountId>,
        ) -> Option<T::AccountId> {
            match participant {
                SettlementParticipant::Account(account) => Some(account.clone()),
                SettlementParticipant::SteamHash(steam_hash) => {
                    T::IdentityProvider::account_for_steam_hash(*steam_hash)
                }
            }
        }
    }
}
