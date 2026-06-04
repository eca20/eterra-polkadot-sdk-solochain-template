use crate::{
    mock::*, ActivePlayer, ActivePlayerInfo, ActiveSessionRoster, CurrentSeason, Error, Event,
    NextServerId, PendingGuapClaims, PendingUnstakes, PlayerRole, RewardEntry, RoundSettlement,
    SeasonInfo, SeasonStats, Seasons, ServerAllowance, ServerAllowances, ServerIdByPubkey,
    ServerStatus, Servers, SettledRounds, SettlementParticipant, TransferEntry, UnstakeInfo,
    UsedTransferNonces, WeaponSpendEntry,
};
use frame_support::{assert_noop, assert_ok, BoundedVec};

#[test]
fn register_server_stores_pending_server_and_index() {
    new_test_ext().execute_with(|| {
        let pubkey = server_pubkey(3);
        set_guap_balance(10, 500);

        assert_ok!(CryptoStrike::register_server(
            RuntimeOrigin::signed(10),
            pubkey,
            metadata_hash(4),
            100
        ));

        let server = Servers::<Test>::get(1).expect("server exists");
        assert_eq!(server.owner, 10);
        assert_eq!(server.server_pubkey, pubkey);
        assert_eq!(server.metadata_hash, metadata_hash(4));
        assert_eq!(server.stake, 100);
        assert_eq!(server.status, ServerStatus::Pending);
        assert_eq!(server.reputation, 0);
        assert_eq!(server.registered_at, 1);
        assert_eq!(server.last_heartbeat, 1);
        assert_eq!(ServerIdByPubkey::<Test>::get(pubkey), Some(1));
        assert_eq!(NextServerId::<Test>::get(), 2);
        assert_eq!(guap_balance(10), 400);
        assert_eq!(reserved_stake(10), 100);
        System::assert_last_event(RuntimeEvent::CryptoStrike(Event::ServerRegistered {
            server_id: 1,
            owner: 10,
        }));
    });
}

#[test]
fn register_server_rejects_invalid_stake_and_duplicate_pubkey() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CryptoStrike::register_server(
                RuntimeOrigin::signed(10),
                [0; 32],
                metadata_hash(4),
                100
            ),
            Error::<Test>::InvalidServerPubkey
        );

        assert_noop!(
            CryptoStrike::register_server(
                RuntimeOrigin::signed(10),
                server_pubkey(3),
                metadata_hash(4),
                99
            ),
            Error::<Test>::StakeBelowMinimum
        );

        assert_noop!(
            CryptoStrike::register_server(
                RuntimeOrigin::signed(10),
                server_pubkey(3),
                metadata_hash(4),
                100
            ),
            Error::<Test>::InsufficientBalance
        );

        set_guap_balance(10, 100);
        assert_ok!(CryptoStrike::register_server(
            RuntimeOrigin::signed(10),
            server_pubkey(3),
            metadata_hash(4),
            100
        ));

        assert_noop!(
            CryptoStrike::register_server(
                RuntimeOrigin::signed(11),
                server_pubkey(3),
                metadata_hash(5),
                100
            ),
            Error::<Test>::ServerPubkeyAlreadyRegistered
        );
    });
}

#[test]
fn heartbeat_is_owner_only_and_updates_block() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CryptoStrike::heartbeat(RuntimeOrigin::signed(10), 99, roster_root(1)),
            Error::<Test>::ServerNotFound
        );

        set_guap_balance(10, 100);
        assert_ok!(CryptoStrike::register_server(
            RuntimeOrigin::signed(10),
            server_pubkey(3),
            metadata_hash(4),
            100
        ));

        assert_noop!(
            CryptoStrike::heartbeat(RuntimeOrigin::signed(11), 1, roster_root(1)),
            Error::<Test>::NotServerOwner
        );

        System::set_block_number(8);
        assert_ok!(CryptoStrike::heartbeat(
            RuntimeOrigin::signed(10),
            1,
            roster_root(1)
        ));

        let server = Servers::<Test>::get(1).expect("server exists");
        assert_eq!(server.last_heartbeat, 8);
        System::assert_last_event(RuntimeEvent::CryptoStrike(Event::ServerHeartbeat {
            server_id: 1,
        }));
    });
}

#[test]
fn set_server_status_is_admin_only() {
    new_test_ext().execute_with(|| {
        set_guap_balance(10, 100);
        assert_ok!(CryptoStrike::register_server(
            RuntimeOrigin::signed(10),
            server_pubkey(3),
            metadata_hash(4),
            100
        ));

        assert_noop!(
            CryptoStrike::set_server_status(RuntimeOrigin::signed(10), 1, ServerStatus::Active),
            sp_runtime::DispatchError::BadOrigin
        );
        assert_noop!(
            CryptoStrike::set_server_status(RuntimeOrigin::root(), 99, ServerStatus::Active),
            Error::<Test>::ServerNotFound
        );

        assert_ok!(CryptoStrike::set_server_status(
            RuntimeOrigin::root(),
            1,
            ServerStatus::Active
        ));

        let server = Servers::<Test>::get(1).expect("server exists");
        assert_eq!(server.status, ServerStatus::Active);
        System::assert_last_event(RuntimeEvent::CryptoStrike(Event::ServerStatusChanged {
            server_id: 1,
            status: ServerStatus::Active,
        }));
    });
}

#[test]
fn increase_server_stake_reserves_more_and_clears_pending_unstake() {
    new_test_ext().execute_with(|| {
        register_active_server(10);
        assert_ok!(CryptoStrike::request_unstake(RuntimeOrigin::signed(10), 1));

        assert_noop!(
            CryptoStrike::increase_server_stake(RuntimeOrigin::signed(10), 1, 0),
            Error::<Test>::InvalidStakeAmount
        );
        assert_noop!(
            CryptoStrike::increase_server_stake(RuntimeOrigin::signed(11), 1, 50),
            Error::<Test>::NotServerOwner
        );

        assert_ok!(CryptoStrike::increase_server_stake(
            RuntimeOrigin::signed(10),
            1,
            50
        ));

        let server = Servers::<Test>::get(1).expect("server exists");
        assert_eq!(server.stake, 150);
        assert_eq!(reserved_stake(10), 150);
        assert_eq!(PendingUnstakes::<Test>::get(1), None);
        System::assert_last_event(RuntimeEvent::CryptoStrike(Event::ServerStaked {
            server_id: 1,
            amount: 50,
        }));
    });
}

#[test]
fn request_unstake_records_delay_and_retires_active_server() {
    new_test_ext().execute_with(|| {
        register_active_server(10);

        assert_noop!(
            CryptoStrike::request_unstake(RuntimeOrigin::signed(11), 1),
            Error::<Test>::NotServerOwner
        );
        assert_ok!(CryptoStrike::request_unstake(RuntimeOrigin::signed(10), 1));

        assert_eq!(
            PendingUnstakes::<Test>::get(1),
            Some(UnstakeInfo {
                amount: 100,
                eligible_at: 6,
            })
        );
        assert_eq!(
            Servers::<Test>::get(1).expect("server exists").status,
            ServerStatus::Retired
        );
        System::assert_last_event(RuntimeEvent::CryptoStrike(Event::ServerUnstakeRequested {
            server_id: 1,
            amount: 100,
            eligible_at: 6,
        }));

        assert_noop!(
            CryptoStrike::request_unstake(RuntimeOrigin::signed(10), 1),
            Error::<Test>::UnstakeAlreadyRequested
        );

        assert_ok!(CryptoStrike::set_server_status(
            RuntimeOrigin::root(),
            1,
            ServerStatus::Active
        ));
        assert_noop!(
            CryptoStrike::set_session_roster_root(
                RuntimeOrigin::signed(10),
                1,
                session_id(1),
                roster_root(7)
            ),
            Error::<Test>::ServerNotActive
        );
    });
}

#[test]
fn finalize_unstake_releases_stake_and_removes_server() {
    new_test_ext().execute_with(|| {
        register_active_server(10);
        assert_eq!(guap_balance(10), 999_900);
        assert_eq!(reserved_stake(10), 100);

        assert_ok!(CryptoStrike::request_unstake(RuntimeOrigin::signed(10), 1));
        assert_noop!(
            CryptoStrike::finalize_unstake(RuntimeOrigin::signed(10), 1),
            Error::<Test>::UnstakeNotReady
        );

        System::set_block_number(6);
        assert_ok!(CryptoStrike::finalize_unstake(RuntimeOrigin::signed(10), 1));

        assert_eq!(guap_balance(10), 1_000_000);
        assert_eq!(reserved_stake(10), 0);
        assert_eq!(Servers::<Test>::get(1), None);
        assert_eq!(ServerIdByPubkey::<Test>::get(server_pubkey(10)), None);
        assert_eq!(PendingUnstakes::<Test>::get(1), None);
        System::assert_last_event(RuntimeEvent::CryptoStrike(Event::ServerUnstaked {
            server_id: 1,
            amount: 100,
        }));
    });
}

#[test]
fn slash_server_updates_stake_status_and_clears_pending_unstake() {
    new_test_ext().execute_with(|| {
        register_active_server(10);
        assert_ok!(CryptoStrike::request_unstake(RuntimeOrigin::signed(10), 1));

        assert_noop!(
            CryptoStrike::slash_server(RuntimeOrigin::signed(11), 1, 40, metadata_hash(9)),
            sp_runtime::DispatchError::BadOrigin
        );
        assert_noop!(
            CryptoStrike::slash_server(RuntimeOrigin::root(), 1, 0, metadata_hash(9)),
            Error::<Test>::InvalidStakeAmount
        );
        assert_noop!(
            CryptoStrike::slash_server(RuntimeOrigin::root(), 1, 101, metadata_hash(9)),
            Error::<Test>::InvalidStakeAmount
        );

        assert_ok!(CryptoStrike::slash_server(
            RuntimeOrigin::root(),
            1,
            40,
            metadata_hash(9)
        ));

        let server = Servers::<Test>::get(1).expect("server exists");
        assert_eq!(server.status, ServerStatus::Slashed);
        assert_eq!(server.stake, 60);
        assert_eq!(reserved_stake(10), 60);
        assert_eq!(slashed_stake(10), 40);
        assert_eq!(PendingUnstakes::<Test>::get(1), None);
        System::assert_last_event(RuntimeEvent::CryptoStrike(Event::ServerSlashed {
            server_id: 1,
            amount: 40,
        }));
    });
}

fn register_active_server(owner: u64) {
    set_guap_balance(owner, 1_000_000);
    assert_ok!(CryptoStrike::register_server(
        RuntimeOrigin::signed(owner),
        server_pubkey(owner as u8),
        metadata_hash(owner as u8),
        100
    ));
    assert_ok!(CryptoStrike::set_server_status(
        RuntimeOrigin::root(),
        1,
        ServerStatus::Active
    ));
}

fn register_active_server_with_roster(owner: u64) {
    register_active_server(owner);
    assert_ok!(CryptoStrike::set_session_roster_root(
        RuntimeOrigin::signed(owner),
        1,
        session_id(1),
        roster_root(7)
    ));
}

fn sign_settlement(mut settlement: Box<RoundSettlement<Test>>) -> Box<RoundSettlement<Test>> {
    settlement.server_signature = server_signature_for(&settlement);
    settlement
}

fn server_signature_for(
    settlement: &RoundSettlement<Test>,
) -> BoundedVec<u8, MaxServerSignatureLen> {
    let payload_hash = CryptoStrike::settlement_payload_hash(settlement);
    let mut signature = b"server-signature".to_vec();
    signature.extend_from_slice(payload_hash.as_ref());
    signature.try_into().unwrap()
}

fn empty_settlement(round_number: u32, root: crate::RosterRoot) -> Box<RoundSettlement<Test>> {
    sign_settlement(Box::new(RoundSettlement {
        server_id: 1,
        session_id: session_id(1),
        map_name_hash: map_name_hash(1),
        round_number,
        previous_round_hash: previous_round_hash(1),
        roster_root: root,
        reward_entries: Vec::new().try_into().unwrap(),
        weapon_spend_entries: Vec::new().try_into().unwrap(),
        guap_transfer_entries: Vec::new().try_into().unwrap(),
        config_hash: config_hash(1),
        server_signature: Vec::new().try_into().unwrap(),
    }))
}

fn settlement_with_transfer(
    round_number: u32,
    nonce: crate::MenuNonce,
) -> Box<RoundSettlement<Test>> {
    let unlinked_hash = steam_hash(21);

    sign_settlement(Box::new(RoundSettlement {
        server_id: 1,
        session_id: session_id(1),
        map_name_hash: map_name_hash(1),
        round_number,
        previous_round_hash: previous_round_hash(round_number as u8),
        roster_root: roster_root(7),
        reward_entries: vec![
            RewardEntry {
                participant: SettlementParticipant::Account(1),
                kills: 1,
                valid_damage: 50,
                reward_guap: 150,
            },
            RewardEntry {
                participant: SettlementParticipant::SteamHash(unlinked_hash),
                kills: 0,
                valid_damage: 25,
                reward_guap: 25,
            },
        ]
        .try_into()
        .unwrap(),
        weapon_spend_entries: vec![WeaponSpendEntry {
            account: 1,
            weapon_id: 7,
            guap_cost: 300,
            round_number,
        }]
        .try_into()
        .unwrap(),
        guap_transfer_entries: vec![TransferEntry {
            from_account: 1,
            to: SettlementParticipant::SteamHash(unlinked_hash),
            amount: 25,
            from_userid: 10,
            to_userid: 11,
            target_role: PlayerRole::Spectator,
            menu_nonce: nonce,
        }]
        .try_into()
        .unwrap(),
        config_hash: config_hash(1),
        server_signature: Vec::new().try_into().unwrap(),
    }))
}

fn settlement_with_linked_recipient(
    round_number: u32,
    nonce: crate::MenuNonce,
) -> Box<RoundSettlement<Test>> {
    sign_settlement(Box::new(RoundSettlement {
        server_id: 1,
        session_id: session_id(1),
        map_name_hash: map_name_hash(1),
        round_number,
        previous_round_hash: previous_round_hash(round_number as u8),
        roster_root: roster_root(7),
        reward_entries: vec![RewardEntry {
            participant: SettlementParticipant::Account(1),
            kills: 2,
            valid_damage: 75,
            reward_guap: 275,
        }]
        .try_into()
        .unwrap(),
        weapon_spend_entries: vec![WeaponSpendEntry {
            account: 1,
            weapon_id: 7,
            guap_cost: 300,
            round_number,
        }]
        .try_into()
        .unwrap(),
        guap_transfer_entries: vec![TransferEntry {
            from_account: 1,
            to: SettlementParticipant::Account(2),
            amount: 40,
            from_userid: 10,
            to_userid: 11,
            target_role: PlayerRole::Terrorist,
            menu_nonce: nonce,
        }]
        .try_into()
        .unwrap(),
        config_hash: config_hash(1),
        server_signature: Vec::new().try_into().unwrap(),
    }))
}

#[test]
fn authorize_server_allowance_stores_and_replaces_allowance() {
    new_test_ext().execute_with(|| {
        register_active_server(10);
        link_steam(1, steam_hash(40));
        System::set_block_number(5);

        assert_ok!(CryptoStrike::authorize_server_allowance(
            RuntimeOrigin::signed(1),
            1,
            500,
            20
        ));

        assert_eq!(
            ServerAllowances::<Test>::get(1, 1),
            Some(ServerAllowance {
                max_guap: 500,
                spent_guap: 0,
                expires_at: 20,
            })
        );
        System::assert_last_event(RuntimeEvent::CryptoStrike(
            Event::ServerAllowanceAuthorized {
                account: 1,
                server_id: 1,
                max_guap: 500,
                expires_at: 20,
            },
        ));

        assert_ok!(CryptoStrike::authorize_server_allowance(
            RuntimeOrigin::signed(1),
            1,
            750,
            30
        ));

        assert_eq!(
            ServerAllowances::<Test>::get(1, 1),
            Some(ServerAllowance {
                max_guap: 750,
                spent_guap: 0,
                expires_at: 30,
            })
        );
    });
}

#[test]
fn authorize_server_allowance_rejects_invalid_inputs() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CryptoStrike::authorize_server_allowance(RuntimeOrigin::signed(1), 1, 0, 20),
            Error::<Test>::InvalidAllowanceAmount
        );

        assert_noop!(
            CryptoStrike::authorize_server_allowance(RuntimeOrigin::signed(1), 1, 500, 1),
            Error::<Test>::AllowanceExpiresInPast
        );

        assert_noop!(
            CryptoStrike::authorize_server_allowance(RuntimeOrigin::signed(1), 99, 500, 20),
            Error::<Test>::ServerNotFound
        );

        set_guap_balance(10, 100);
        assert_ok!(CryptoStrike::register_server(
            RuntimeOrigin::signed(10),
            server_pubkey(10),
            metadata_hash(10),
            100
        ));

        assert_noop!(
            CryptoStrike::authorize_server_allowance(RuntimeOrigin::signed(1), 1, 500, 20),
            Error::<Test>::ServerNotActive
        );
    });
}

#[test]
fn revoke_server_allowance_removes_only_callers_allowance() {
    new_test_ext().execute_with(|| {
        register_active_server(10);
        link_steam(1, steam_hash(40));
        link_steam(2, steam_hash(41));

        assert_noop!(
            CryptoStrike::revoke_server_allowance(RuntimeOrigin::signed(1), 1),
            Error::<Test>::ServerAllowanceNotFound
        );

        assert_ok!(CryptoStrike::authorize_server_allowance(
            RuntimeOrigin::signed(1),
            1,
            500,
            20
        ));
        assert_ok!(CryptoStrike::authorize_server_allowance(
            RuntimeOrigin::signed(2),
            1,
            250,
            20
        ));

        assert_ok!(CryptoStrike::revoke_server_allowance(
            RuntimeOrigin::signed(1),
            1
        ));

        assert_eq!(ServerAllowances::<Test>::get(1, 1), None);
        assert_eq!(
            ServerAllowances::<Test>::get(2, 1),
            Some(ServerAllowance {
                max_guap: 250,
                spent_guap: 0,
                expires_at: 20,
            })
        );
        System::assert_last_event(RuntimeEvent::CryptoStrike(Event::ServerAllowanceRevoked {
            account: 1,
            server_id: 1,
        }));
    });
}

#[test]
fn set_session_roster_root_requires_active_server_owner() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CryptoStrike::set_session_roster_root(
                RuntimeOrigin::signed(10),
                99,
                session_id(1),
                roster_root(1)
            ),
            Error::<Test>::ServerNotFound
        );

        set_guap_balance(10, 100);
        assert_ok!(CryptoStrike::register_server(
            RuntimeOrigin::signed(10),
            server_pubkey(10),
            metadata_hash(10),
            100
        ));

        assert_noop!(
            CryptoStrike::set_session_roster_root(
                RuntimeOrigin::signed(11),
                1,
                session_id(1),
                roster_root(1)
            ),
            Error::<Test>::NotServerOwner
        );
        assert_noop!(
            CryptoStrike::set_session_roster_root(
                RuntimeOrigin::signed(10),
                1,
                session_id(1),
                roster_root(1)
            ),
            Error::<Test>::ServerNotActive
        );

        assert_ok!(CryptoStrike::set_server_status(
            RuntimeOrigin::root(),
            1,
            ServerStatus::Active
        ));
        assert_ok!(CryptoStrike::set_session_roster_root(
            RuntimeOrigin::signed(10),
            1,
            session_id(1),
            roster_root(7)
        ));

        assert_eq!(
            ActiveSessionRoster::<Test>::get(1, session_id(1)),
            Some(roster_root(7))
        );
        System::assert_last_event(RuntimeEvent::CryptoStrike(
            Event::ActiveSessionRosterUpdated {
                server_id: 1,
                session_id: session_id(1),
                roster_root: roster_root(7),
            },
        ));
    });
}

#[test]
fn upsert_active_player_supports_linked_unlinked_and_canonical_account() {
    new_test_ext().execute_with(|| {
        register_active_server(10);
        let linked_hash = steam_hash(4);
        let unlinked_hash = steam_hash(5);

        link_steam(1, linked_hash);

        assert_ok!(CryptoStrike::upsert_active_player(
            RuntimeOrigin::signed(10),
            1,
            session_id(1),
            linked_hash,
            None,
            PlayerRole::CounterTerrorist,
            3,
            20
        ));
        assert_eq!(
            ActivePlayer::<Test>::get((1, session_id(1), linked_hash)),
            Some(ActivePlayerInfo {
                account: Some(1),
                role: PlayerRole::CounterTerrorist,
                joined_at_block: 1,
                last_seen_round: 3,
                expires_at_block: 20,
            })
        );

        assert_ok!(CryptoStrike::upsert_active_player(
            RuntimeOrigin::signed(10),
            1,
            session_id(1),
            unlinked_hash,
            None,
            PlayerRole::Spectator,
            3,
            20
        ));
        assert_eq!(
            ActivePlayer::<Test>::get((1, session_id(1), unlinked_hash)),
            Some(ActivePlayerInfo {
                account: None,
                role: PlayerRole::Spectator,
                joined_at_block: 1,
                last_seen_round: 3,
                expires_at_block: 20,
            })
        );
    });
}

#[test]
fn upsert_active_player_rejects_bad_account_expiry_and_inactive_server() {
    new_test_ext().execute_with(|| {
        set_guap_balance(10, 100);
        assert_ok!(CryptoStrike::register_server(
            RuntimeOrigin::signed(10),
            server_pubkey(10),
            metadata_hash(10),
            100
        ));

        assert_noop!(
            CryptoStrike::upsert_active_player(
                RuntimeOrigin::signed(10),
                1,
                session_id(1),
                steam_hash(4),
                None,
                PlayerRole::Terrorist,
                1,
                20
            ),
            Error::<Test>::ServerNotActive
        );

        assert_ok!(CryptoStrike::set_server_status(
            RuntimeOrigin::root(),
            1,
            ServerStatus::Active
        ));
        assert_noop!(
            CryptoStrike::upsert_active_player(
                RuntimeOrigin::signed(10),
                1,
                session_id(1),
                steam_hash(4),
                Some(1),
                PlayerRole::Terrorist,
                1,
                20
            ),
            Error::<Test>::LinkedAccountMismatch
        );
        assert_noop!(
            CryptoStrike::upsert_active_player(
                RuntimeOrigin::signed(10),
                1,
                session_id(1),
                steam_hash(4),
                None,
                PlayerRole::Terrorist,
                1,
                1
            ),
            Error::<Test>::ActivePlayerExpiresInPast
        );
    });
}

#[test]
fn upsert_active_player_preserves_joined_block_and_remove_targets_one_player() {
    new_test_ext().execute_with(|| {
        register_active_server(10);
        let hash_a = steam_hash(8);
        let hash_b = steam_hash(9);

        System::set_block_number(4);
        assert_ok!(CryptoStrike::upsert_active_player(
            RuntimeOrigin::signed(10),
            1,
            session_id(1),
            hash_a,
            None,
            PlayerRole::Terrorist,
            1,
            20
        ));
        assert_ok!(CryptoStrike::upsert_active_player(
            RuntimeOrigin::signed(10),
            1,
            session_id(1),
            hash_b,
            None,
            PlayerRole::Spectator,
            1,
            20
        ));

        System::set_block_number(9);
        assert_ok!(CryptoStrike::upsert_active_player(
            RuntimeOrigin::signed(10),
            1,
            session_id(1),
            hash_a,
            None,
            PlayerRole::CounterTerrorist,
            2,
            30
        ));
        assert_eq!(
            ActivePlayer::<Test>::get((1, session_id(1), hash_a)),
            Some(ActivePlayerInfo {
                account: None,
                role: PlayerRole::CounterTerrorist,
                joined_at_block: 4,
                last_seen_round: 2,
                expires_at_block: 30,
            })
        );

        assert_noop!(
            CryptoStrike::remove_active_player(
                RuntimeOrigin::signed(10),
                1,
                session_id(1),
                steam_hash(10)
            ),
            Error::<Test>::ActivePlayerNotFound
        );
        assert_ok!(CryptoStrike::remove_active_player(
            RuntimeOrigin::signed(10),
            1,
            session_id(1),
            hash_a
        ));

        assert_eq!(ActivePlayer::<Test>::get((1, session_id(1), hash_a)), None);
        assert!(ActivePlayer::<Test>::get((1, session_id(1), hash_b)).is_some());
        System::assert_last_event(RuntimeEvent::CryptoStrike(Event::ActivePlayerRemoved {
            server_id: 1,
            session_id: session_id(1),
            steam_hash: hash_a,
        }));
    });
}

fn prepare_settlement_participants() {
    let linked_hash = steam_hash(20);
    let unlinked_hash = steam_hash(21);

    link_steam(1, linked_hash);
    assert_ok!(CryptoStrike::upsert_active_player(
        RuntimeOrigin::signed(10),
        1,
        session_id(1),
        linked_hash,
        None,
        PlayerRole::CounterTerrorist,
        1,
        20
    ));
    assert_ok!(CryptoStrike::upsert_active_player(
        RuntimeOrigin::signed(10),
        1,
        session_id(1),
        unlinked_hash,
        None,
        PlayerRole::Spectator,
        1,
        20
    ));
    set_guap_balance(1, 1_000);
    assert_ok!(CryptoStrike::authorize_server_allowance(
        RuntimeOrigin::signed(1),
        1,
        1_000,
        20
    ));
}

fn prepare_two_linked_settlement_participants() {
    let sender_hash = steam_hash(30);
    let recipient_hash = steam_hash(31);

    link_steam(1, sender_hash);
    link_steam(2, recipient_hash);
    assert_ok!(CryptoStrike::upsert_active_player(
        RuntimeOrigin::signed(10),
        1,
        session_id(1),
        sender_hash,
        None,
        PlayerRole::CounterTerrorist,
        1,
        20
    ));
    assert_ok!(CryptoStrike::upsert_active_player(
        RuntimeOrigin::signed(10),
        1,
        session_id(1),
        recipient_hash,
        None,
        PlayerRole::Terrorist,
        1,
        20
    ));
    set_guap_balance(1, 1_000);
    assert_ok!(CryptoStrike::authorize_server_allowance(
        RuntimeOrigin::signed(1),
        1,
        1_000,
        20
    ));
}

fn prepare_two_linked_settlement_participants_without_allowance() {
    let sender_hash = steam_hash(30);
    let recipient_hash = steam_hash(31);

    link_steam(1, sender_hash);
    link_steam(2, recipient_hash);
    assert_ok!(CryptoStrike::upsert_active_player(
        RuntimeOrigin::signed(10),
        1,
        session_id(1),
        sender_hash,
        None,
        PlayerRole::CounterTerrorist,
        1,
        20
    ));
    assert_ok!(CryptoStrike::upsert_active_player(
        RuntimeOrigin::signed(10),
        1,
        session_id(1),
        recipient_hash,
        None,
        PlayerRole::Terrorist,
        1,
        20
    ));
}

#[test]
fn submit_round_settlement_marks_round_and_transfer_nonce() {
    new_test_ext().execute_with(|| {
        register_active_server_with_roster(10);
        prepare_settlement_participants();

        assert_ok!(CryptoStrike::submit_round_settlement(
            RuntimeOrigin::signed(10),
            settlement_with_transfer(1, menu_nonce(1))
        ));

        assert!(SettledRounds::<Test>::contains_key((1, session_id(1), 1)));
        assert!(UsedTransferNonces::<Test>::contains_key((
            1,
            session_id(1),
            menu_nonce(1)
        )));
        System::assert_last_event(RuntimeEvent::CryptoStrike(Event::RoundSettled {
            server_id: 1,
            session_id: session_id(1),
            round_number: 1,
        }));
    });
}

#[test]
fn submit_round_settlement_applies_guap_economy_for_linked_participants() {
    new_test_ext().execute_with(|| {
        register_active_server_with_roster(10);
        prepare_two_linked_settlement_participants();

        assert_ok!(CryptoStrike::submit_round_settlement(
            RuntimeOrigin::signed(10),
            settlement_with_linked_recipient(1, menu_nonce(1))
        ));

        assert_eq!(guap_balance(1), 935);
        assert_eq!(guap_balance(2), 40);
        assert_eq!(
            ServerAllowances::<Test>::get(1, 1),
            Some(ServerAllowance {
                max_guap: 1_000,
                spent_guap: 340,
                expires_at: 20,
            })
        );
    });
}

#[test]
fn submit_round_settlement_creates_pending_claims_for_unlinked_participants() {
    new_test_ext().execute_with(|| {
        register_active_server_with_roster(10);
        prepare_settlement_participants();

        assert_ok!(CryptoStrike::submit_round_settlement(
            RuntimeOrigin::signed(10),
            settlement_with_transfer(1, menu_nonce(1))
        ));

        assert_eq!(guap_balance(1), 825);
        assert_eq!(PendingGuapClaims::<Test>::get(steam_hash(21)), 50);
        assert_eq!(
            ServerAllowances::<Test>::get(1, 1),
            Some(ServerAllowance {
                max_guap: 1_000,
                spent_guap: 325,
                expires_at: 20,
            })
        );
    });
}

#[test]
fn claim_pending_guap_mints_to_newly_linked_account() {
    new_test_ext().execute_with(|| {
        register_active_server_with_roster(10);
        prepare_settlement_participants();

        assert_ok!(CryptoStrike::submit_round_settlement(
            RuntimeOrigin::signed(10),
            settlement_with_transfer(1, menu_nonce(1))
        ));
        assert_eq!(PendingGuapClaims::<Test>::get(steam_hash(21)), 50);

        link_steam(2, steam_hash(21));
        assert_ok!(CryptoStrike::claim_pending_guap(RuntimeOrigin::signed(2)));

        assert_eq!(PendingGuapClaims::<Test>::get(steam_hash(21)), 0);
        assert_eq!(guap_balance(2), 50);
        System::assert_last_event(RuntimeEvent::CryptoStrike(Event::GuapClaimed {
            steam_hash: steam_hash(21),
            account: 2,
            amount: 50,
        }));
    });
}

#[test]
fn claim_pending_guap_requires_linked_account_with_claim() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CryptoStrike::claim_pending_guap(RuntimeOrigin::signed(1)),
            Error::<Test>::SteamHashNotLinked
        );

        link_steam(1, steam_hash(22));
        assert_noop!(
            CryptoStrike::claim_pending_guap(RuntimeOrigin::signed(1)),
            Error::<Test>::NoPendingGuapClaim
        );
    });
}

#[test]
fn submit_round_settlement_rejects_missing_or_insufficient_allowance() {
    new_test_ext().execute_with(|| {
        register_active_server_with_roster(10);
        prepare_two_linked_settlement_participants_without_allowance();
        set_guap_balance(1, 1_000);

        assert_noop!(
            CryptoStrike::submit_round_settlement(
                RuntimeOrigin::signed(10),
                settlement_with_linked_recipient(1, menu_nonce(1))
            ),
            Error::<Test>::InsufficientAllowance
        );

        assert_ok!(CryptoStrike::authorize_server_allowance(
            RuntimeOrigin::signed(1),
            1,
            100,
            20
        ));
        assert_noop!(
            CryptoStrike::submit_round_settlement(
                RuntimeOrigin::signed(10),
                settlement_with_linked_recipient(1, menu_nonce(1))
            ),
            Error::<Test>::InsufficientAllowance
        );
        assert!(!SettledRounds::<Test>::contains_key((1, session_id(1), 1)));
    });
}

#[test]
fn submit_round_settlement_rejects_insufficient_guap_balance() {
    new_test_ext().execute_with(|| {
        register_active_server_with_roster(10);
        prepare_two_linked_settlement_participants();
        set_guap_balance(1, 100);

        assert_noop!(
            CryptoStrike::submit_round_settlement(
                RuntimeOrigin::signed(10),
                settlement_with_linked_recipient(1, menu_nonce(1))
            ),
            Error::<Test>::InsufficientBalance
        );
        assert!(!SettledRounds::<Test>::contains_key((1, session_id(1), 1)));
    });
}

#[test]
fn submit_round_settlement_rejects_duplicate_round() {
    new_test_ext().execute_with(|| {
        register_active_server_with_roster(10);
        prepare_settlement_participants();

        assert_ok!(CryptoStrike::submit_round_settlement(
            RuntimeOrigin::signed(10),
            settlement_with_transfer(1, menu_nonce(1))
        ));
        assert_noop!(
            CryptoStrike::submit_round_settlement(
                RuntimeOrigin::signed(10),
                settlement_with_transfer(1, menu_nonce(2))
            ),
            Error::<Test>::DuplicateRound
        );
    });
}

#[test]
fn submit_round_settlement_rejects_missing_or_mismatched_roster_and_empty_signature() {
    new_test_ext().execute_with(|| {
        register_active_server(10);

        assert_noop!(
            CryptoStrike::submit_round_settlement(
                RuntimeOrigin::signed(10),
                empty_settlement(1, roster_root(7))
            ),
            Error::<Test>::MissingSessionRoster
        );

        assert_ok!(CryptoStrike::set_session_roster_root(
            RuntimeOrigin::signed(10),
            1,
            session_id(1),
            roster_root(7)
        ));

        assert_noop!(
            CryptoStrike::submit_round_settlement(
                RuntimeOrigin::signed(10),
                empty_settlement(1, roster_root(8))
            ),
            Error::<Test>::RosterRootMismatch
        );

        let mut settlement = empty_settlement(1, roster_root(7));
        settlement.server_signature = Vec::new().try_into().unwrap();
        assert_noop!(
            CryptoStrike::submit_round_settlement(RuntimeOrigin::signed(10), settlement),
            Error::<Test>::InvalidServerSignature
        );

        let mut tampered_settlement = empty_settlement(2, roster_root(7));
        tampered_settlement.config_hash = config_hash(99);
        assert_noop!(
            CryptoStrike::submit_round_settlement(RuntimeOrigin::signed(10), tampered_settlement),
            Error::<Test>::InvalidServerSignature
        );
    });
}

#[test]
fn submit_round_settlement_rejects_inactive_or_expired_participants() {
    new_test_ext().execute_with(|| {
        register_active_server_with_roster(10);

        let missing_participant_settlement = sign_settlement(Box::new(RoundSettlement {
            reward_entries: vec![RewardEntry {
                participant: SettlementParticipant::SteamHash(steam_hash(88)),
                kills: 1,
                valid_damage: 0,
                reward_guap: 100,
            }]
            .try_into()
            .unwrap(),
            ..*empty_settlement(1, roster_root(7))
        }));

        assert_noop!(
            CryptoStrike::submit_round_settlement(
                RuntimeOrigin::signed(10),
                missing_participant_settlement
            ),
            Error::<Test>::SettlementParticipantNotActive
        );

        assert_ok!(CryptoStrike::upsert_active_player(
            RuntimeOrigin::signed(10),
            1,
            session_id(1),
            steam_hash(88),
            None,
            PlayerRole::Terrorist,
            1,
            5
        ));
        System::set_block_number(5);

        let expired_participant_settlement = sign_settlement(Box::new(RoundSettlement {
            reward_entries: vec![RewardEntry {
                participant: SettlementParticipant::SteamHash(steam_hash(88)),
                kills: 1,
                valid_damage: 0,
                reward_guap: 100,
            }]
            .try_into()
            .unwrap(),
            round_number: 2,
            ..*empty_settlement(2, roster_root(7))
        }));

        assert_noop!(
            CryptoStrike::submit_round_settlement(
                RuntimeOrigin::signed(10),
                expired_participant_settlement
            ),
            Error::<Test>::ActivePlayerExpired
        );
    });
}

#[test]
fn submit_round_settlement_rejects_duplicate_transfer_nonce_across_rounds() {
    new_test_ext().execute_with(|| {
        register_active_server_with_roster(10);
        prepare_settlement_participants();

        assert_ok!(CryptoStrike::submit_round_settlement(
            RuntimeOrigin::signed(10),
            settlement_with_transfer(1, menu_nonce(1))
        ));
        assert_noop!(
            CryptoStrike::submit_round_settlement(
                RuntimeOrigin::signed(10),
                settlement_with_transfer(2, menu_nonce(1))
            ),
            Error::<Test>::DuplicateTransferNonce
        );
    });
}

#[test]
fn start_and_end_season_are_admin_controlled() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CryptoStrike::start_season(RuntimeOrigin::signed(1), 1, metadata_hash(90)),
            sp_runtime::DispatchError::BadOrigin
        );

        assert_ok!(CryptoStrike::start_season(
            RuntimeOrigin::root(),
            1,
            metadata_hash(90)
        ));
        assert_eq!(CurrentSeason::<Test>::get(), Some(1));
        assert_eq!(
            Seasons::<Test>::get(1),
            Some(SeasonInfo {
                metadata_hash: metadata_hash(90),
                started_at: 1,
                ended_at: None,
            })
        );
        System::assert_last_event(RuntimeEvent::CryptoStrike(Event::SeasonStarted {
            season_id: 1,
        }));

        assert_noop!(
            CryptoStrike::start_season(RuntimeOrigin::root(), 2, metadata_hash(91)),
            Error::<Test>::SeasonAlreadyActive
        );
        assert_noop!(
            CryptoStrike::end_season(RuntimeOrigin::signed(1), 1),
            sp_runtime::DispatchError::BadOrigin
        );
        assert_noop!(
            CryptoStrike::end_season(RuntimeOrigin::root(), 2),
            Error::<Test>::SeasonNotActive
        );

        System::set_block_number(8);
        assert_ok!(CryptoStrike::end_season(RuntimeOrigin::root(), 1));
        assert_eq!(CurrentSeason::<Test>::get(), None);
        assert_eq!(
            Seasons::<Test>::get(1),
            Some(SeasonInfo {
                metadata_hash: metadata_hash(90),
                started_at: 1,
                ended_at: Some(8),
            })
        );
        System::assert_last_event(RuntimeEvent::CryptoStrike(Event::SeasonEnded {
            season_id: 1,
        }));

        assert_noop!(
            CryptoStrike::start_season(RuntimeOrigin::root(), 1, metadata_hash(92)),
            Error::<Test>::SeasonAlreadyExists
        );
    });
}

#[test]
fn settlement_without_active_season_writes_no_stats() {
    new_test_ext().execute_with(|| {
        register_active_server_with_roster(10);
        prepare_settlement_participants();

        assert_ok!(CryptoStrike::submit_round_settlement(
            RuntimeOrigin::signed(10),
            settlement_with_transfer(1, menu_nonce(1))
        ));

        assert_eq!(SeasonStats::<Test>::get(1, 1), None);
    });
}

#[test]
fn settlement_updates_active_season_stats_for_linked_accounts() {
    new_test_ext().execute_with(|| {
        register_active_server_with_roster(10);
        prepare_two_linked_settlement_participants();
        assert_ok!(CryptoStrike::start_season(
            RuntimeOrigin::root(),
            1,
            metadata_hash(90)
        ));

        assert_ok!(CryptoStrike::submit_round_settlement(
            RuntimeOrigin::signed(10),
            settlement_with_linked_recipient(1, menu_nonce(1))
        ));

        let sender_stats = SeasonStats::<Test>::get(1, 1).expect("sender stats");
        assert_eq!(sender_stats.kills, 2);
        assert_eq!(sender_stats.valid_damage, 75);
        assert_eq!(sender_stats.rounds_played, 1);
        assert_eq!(sender_stats.guap_earned, 275);
        assert_eq!(sender_stats.guap_spent, 300);
        assert_eq!(sender_stats.guap_transferred_out, 40);
        assert_eq!(sender_stats.guap_transferred_in, 0);
        assert_eq!(sender_stats.season_points, 275);

        let recipient_stats = SeasonStats::<Test>::get(1, 2).expect("recipient stats");
        assert_eq!(recipient_stats.kills, 0);
        assert_eq!(recipient_stats.valid_damage, 0);
        assert_eq!(recipient_stats.rounds_played, 0);
        assert_eq!(recipient_stats.guap_earned, 0);
        assert_eq!(recipient_stats.guap_spent, 0);
        assert_eq!(recipient_stats.guap_transferred_out, 0);
        assert_eq!(recipient_stats.guap_transferred_in, 40);
        assert_eq!(recipient_stats.season_points, 0);
    });
}

#[test]
fn frozen_player_cannot_authorize_allowance() {
    new_test_ext().execute_with(|| {
        register_active_server(10);
        link_steam(1, steam_hash(60));
        freeze_account(1);

        assert_noop!(
            CryptoStrike::authorize_server_allowance(RuntimeOrigin::signed(1), 1, 500, 20),
            Error::<Test>::PlayerFrozen
        );
    });
}

#[test]
fn settlement_rejects_frozen_linked_account() {
    new_test_ext().execute_with(|| {
        register_active_server_with_roster(10);
        prepare_two_linked_settlement_participants();
        freeze_account(1);

        assert_noop!(
            CryptoStrike::submit_round_settlement(
                RuntimeOrigin::signed(10),
                settlement_with_linked_recipient(1, menu_nonce(1))
            ),
            Error::<Test>::PlayerFrozen
        );
    });
}

#[test]
fn settlement_rejects_frozen_account_referenced_by_steam_hash() {
    new_test_ext().execute_with(|| {
        register_active_server_with_roster(10);
        let hash = steam_hash(70);

        link_steam(1, hash);
        assert_ok!(CryptoStrike::upsert_active_player(
            RuntimeOrigin::signed(10),
            1,
            session_id(1),
            hash,
            None,
            PlayerRole::CounterTerrorist,
            1,
            20
        ));
        freeze_account(1);

        let settlement = sign_settlement(Box::new(RoundSettlement {
            reward_entries: vec![RewardEntry {
                participant: SettlementParticipant::SteamHash(hash),
                kills: 1,
                valid_damage: 0,
                reward_guap: 100,
            }]
            .try_into()
            .unwrap(),
            ..*empty_settlement(1, roster_root(7))
        }));

        assert_noop!(
            CryptoStrike::submit_round_settlement(RuntimeOrigin::signed(10), settlement),
            Error::<Test>::PlayerFrozen
        );
    });
}
