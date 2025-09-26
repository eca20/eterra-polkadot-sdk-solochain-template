use crate::pallet;
use crate::mock::RuntimeEvent;
use crate::types::game::GameProperties; // Import the GameProperties trait
use crate::types::card::Possession as Player;
use crate::GameId;
use crate::GameStorage;
use crate::Move;
use crate::{mock::*, types::card::Card};
use frame_support::traits::Get;
use frame_support::traits::Hooks;
use frame_support::{assert_noop, assert_ok};
use frame_support::BoundedVec;
use frame_system::pallet_prelude::BlockNumberFor;
use log::{Level, Metadata, Record};
use sp_core::H256; // Fix: Import H256
use sp_runtime::traits::{BlakeTwo256, Hash};
use std::sync::Once;
use frame_system::RawOrigin;
use crate::types::card::Possession;

use pallet_eterra_simple_tcg as cards;
use cards::pallet as card_pallet;

use eterra_card_ai_adapter::eterra_adapter as ai;
use pallet_eterra_monte_carlo_ai as mc_ai;
use crate::HandsOfGame;

static INIT: Once = Once::new();

pub struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            println!(
                "[{}] {}: {}",
                record.level(),
                record.target(),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

static LOGGER: SimpleLogger = SimpleLogger;

pub fn init_logger() {
    INIT.call_once(|| {
        log::set_logger(&LOGGER).unwrap();
        log::set_max_level(log::LevelFilter::Debug);
    });
}

fn setup_new_game() -> (H256, u64, u64) {
    let creator = 1;
    let opponent = 2;
    // Get the current block number
    let current_block_number = <frame_system::Pallet<Test>>::block_number();
    // Calculate game_id using the hashing function
    let game_id = BlakeTwo256::hash_of(&(creator, opponent, current_block_number));
    // Create the game with two players
    assert_ok!(Eterra::create_game(
        frame_system::RawOrigin::Signed(creator).into(),
        vec![creator, opponent],
        pallet::GameMode::PvP,
    ));
    log::debug!(
        "Game created with ID: {:?}, Creator: {}, Opponent: {}, Block: {}",
        game_id,
        creator,
        opponent,
        current_block_number,
    );
    (game_id, creator, opponent)
}

/// Helper to setup a new game with the given creator and opponent.
fn setup_new_game_with(creator: u64, opponent: u64) -> (H256, u64, u64) {
    // Get the current block number
    let current_block_number = <frame_system::Pallet<Test>>::block_number();
    // Calculate game_id using the hashing function
    let game_id = BlakeTwo256::hash_of(&(creator, opponent, current_block_number));
    // Create the game with two players
    assert_ok!(Eterra::create_game(
        frame_system::RawOrigin::Signed(creator).into(),
        vec![creator, opponent],
        pallet::GameMode::PvP,
    ));
    log::debug!(
        "Game created with ID: {:?}, Creator: {}, Opponent: {}, Block: {}",
        game_id,
        creator,
        opponent,
        current_block_number,
    );
    (game_id, creator, opponent)
}

/// Utility function that advances the current block number by `n` blocks.
fn run_to_block(n: u64) {
    while System::block_number() < n {
        System::set_block_number(System::block_number() + 1);
    }
}

/// Mint `n` cards for `owner` in the simple TCG pallet and return their IDs.
fn mint_cards_for(owner: u64, n: usize) -> Vec<u32> {
    for _ in 0..n {
        assert_ok!(cards::Pallet::<Test>::mint_card(frame_system::RawOrigin::Signed(owner).into()));
    }
    // Read from OwnedCards index (bounded vec) and collect the most recent `n` ids
    let owned = card_pallet::OwnedCards::<Test>::get(owner);
    owned.into_iter().rev().take(n).rev().collect()
}

/// Ensure it's `me`'s turn; if not, make a simple legal move for `other` to advance.
fn ensure_my_turn(game_id: H256, me: u64, other: u64) {
    loop {
        let game = Eterra::game_board(game_id).expect("game must exist");
        let current = game.players[game.player_turn as usize];
        if current == me { break; }
        // Make `other` play a trivial move using the original `play` (not from hand)
        // Find the first empty slot
        'outer: for x in 0..4u8 {
            for y in 0..4u8 {
                if game.board[x as usize][y as usize].is_none() {
                    let m = Move { place_index_x: x, place_index_y: y, place_card: Card::new(1,1,1,1) };
                    assert_ok!(Eterra::play(frame_system::RawOrigin::Signed(other).into(), game_id, m));
                    break 'outer;
                }
            }
        }
    }
}

#[test]
fn create_game_with_same_players_fails() {
    init_logger();
    new_test_ext().execute_with(|| {
        let player = 1; // Define `player` explicitly
        let result = Eterra::create_game(
            frame_system::RawOrigin::Signed(player).into(),
            vec![player, player], // Pass the same player twice
            pallet::GameMode::PvP,
        );
        assert_noop!(result, crate::Error::<Test>::InvalidMove);
    });
}

#[test]
fn invalid_move_on_occupied_cell() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();
        let card = Card::new(5, 3, 2, 4);

        let player_move = Move {
            place_index_x: 1,
            place_index_y: 1,
            place_card: card,
        };

        assert_ok!(Eterra::play(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            player_move.clone(),
        ));

        // Attempt to play on the same cell
        let result = Eterra::play(
            frame_system::RawOrigin::Signed(opponent).into(),
            game_id,
            player_move,
        );

        assert_noop!(result, crate::Error::<Test>::CellOccupied);
    });
}

#[test]
fn create_game_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();

        let game = Eterra::game_board(game_id).unwrap();
        let current_turn = game.players[game.player_turn as usize].clone();

        assert_eq!(game.players[0], creator);
        assert_eq!(game.players[1], opponent);
        assert!(current_turn == creator || current_turn == opponent);
        assert!(game.board.iter().flatten().all(|cell| cell.is_none())); // Verify empty board
    });
}
#[test]
fn play_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();

        let card = Card::new(5, 3, 2, 4).with_possession(Player::PlayerOne);

        // Creator's move
        let creator_move = Move {
            place_index_x: 1,
            place_index_y: 1,
            place_card: card.clone(),
        };
        assert_ok!(Eterra::play(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            creator_move.clone(),
        ));

        let game = Eterra::game_board(game_id).unwrap(); // Fetch the updated game
        assert_eq!(
            game.board[creator_move.place_index_x as usize][creator_move.place_index_y as usize],
            Some(card.clone())
        );

        let opponent_card = Card::new(2, 4, 5, 3).with_possession(Player::PlayerTwo);

        // Opponent's move
        let opponent_move = Move {
            place_index_x: 1,
            place_index_y: 2,
            place_card: opponent_card.clone(),
        };
        assert_ok!(Eterra::play(
            frame_system::RawOrigin::Signed(opponent).into(),
            game_id,
            opponent_move.clone(),
        ));

        let updated_game = Eterra::game_board(game_id).unwrap(); // Fetch the game after the opponent's move
        assert_eq!(
            updated_game.board[opponent_move.place_index_x as usize]
                [opponent_move.place_index_y as usize],
            Some(opponent_card.clone())
        );
    });
}


#[test]
fn card_capture_multiple_directions() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();

        let creator_card = Card::new(3, 5, 2, 1).with_possession(Player::PlayerOne);

        // Creator's first move
        let creator_move = Move {
            place_index_x: 0,
            place_index_y: 0,
            place_card: creator_card,
        };
        assert_ok!(Eterra::play(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            creator_move.clone(),
        ));

        let opponent_card = Card::new(2, 4, 5, 3).with_possession(Player::PlayerTwo);

        // Opponent's move
        let opponent_move = Move {
            place_index_x: 0,
            place_index_y: 1,
            place_card: opponent_card,
        };
        assert_ok!(Eterra::play(
            frame_system::RawOrigin::Signed(opponent).into(),
            game_id,
            opponent_move.clone(),
        ));

        let capturing_card = Card::new(6, 6, 3, 3).with_possession(Player::PlayerOne);

        // Creator's capturing move
        let capturing_move = Move {
            place_index_x: 0,
            place_index_y: 2,
            place_card: capturing_card,
        };
        assert_ok!(Eterra::play(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            capturing_move.clone(),
        ));

        // Validate board state
        let game = Eterra::game_board(game_id).unwrap(); // Fetch the updated game
        assert_eq!(
            game.board[0][1].as_ref().unwrap().get_possession(),
            Some(&Player::PlayerOne)
        );
        assert_eq!(
            game.board[0][2].as_ref().unwrap().get_possession(),
            Some(&Player::PlayerOne)
        ); // Captured card
    });
}

#[test]
fn capture_updates_possession_on_valid_adjacent_flips() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Helper that sets up a fresh game and ensures it's opponent's turn,
        // then places opponent card at (ox,oy) and creator (PlayerOne) at (cx,cy),
        // asserting that the opponent card gets flipped to PlayerOne.
        let run_case = |creator: u64, opponent: u64,
                        ox: u8, oy: u8, cx: u8, cy: u8, opp_bottom: u8, opp_left: u8, opp_top: u8, opp_right: u8,
                        my_top: u8, my_right: u8, my_bottom: u8, my_left: u8| {
            let (game_id, creator, opponent) = setup_new_game_with(creator, opponent);

            // Ensure opponent plays first so their card's possession = PlayerTwo on-chain.
            ensure_my_turn(game_id, opponent, creator);

            // Opponent places a card next to the target cell with specified ranks.
            let opp_card = Card::new(opp_top, opp_right, opp_bottom, opp_left)
                .with_possession(Player::PlayerTwo);
            let opp_mv = Move { place_index_x: ox, place_index_y: oy, place_card: opp_card };
            assert_ok!(Eterra::play(frame_system::RawOrigin::Signed(opponent).into(), game_id, opp_mv));

            // Now it's creator's turn; place the capturing card at (cx,cy).
            let my_card = Card::new(my_top, my_right, my_bottom, my_left)
                .with_possession(Player::PlayerOne);
            let my_mv = Move { place_index_x: cx, place_index_y: cy, place_card: my_card };
            assert_ok!(Eterra::play(frame_system::RawOrigin::Signed(creator).into(), game_id, my_mv));

            // Verify the neighbor at (ox,oy) was flipped to PlayerOne (creator).
            let g = Eterra::game_board(game_id).unwrap();
            let flipped = g.board[ox as usize][oy as usize].as_ref().unwrap().get_possession();
            assert_eq!(flipped, Some(&Player::PlayerOne), "Expected capture and flip at ({},{})", ox, oy);

            // And the placed card cell should also belong to PlayerOne.
            let mine = g.board[cx as usize][cy as usize].as_ref().unwrap().get_possession();
            assert_eq!(mine, Some(&Player::PlayerOne), "Placed card at ({},{}) should belong to PlayerOne", cx, cy);
        };

        // Four directional cases, each with unique player ids:
        // 1) Capture "north" neighbor: my TOP > their BOTTOM
        //    Opponent at (1,0), creator plays at (1,1)
        run_case(
            11, 12,  // creator, opponent
            1, 0,  // opp x,y
            1, 1,  // my  x,y
            /* opp bottom */ 3, /* opp left */ 1, /* opp top */ 1, /* opp right */ 1,
            /* my top   */ 5, /* my right */ 1, /* my bottom */ 1, /* my left  */ 1
        );

        // 2) Capture "east" neighbor: my RIGHT > their LEFT
        //    Opponent at (2,1), creator plays at (1,1)
        run_case(
            21, 22,
            2, 1,
            1, 1,
            /* opp bottom */ 1, /* opp left */ 3, /* opp top */ 1, /* opp right */ 1,
            /* my top   */ 1, /* my right */ 5, /* my bottom */ 1, /* my left  */ 1
        );

        // 3) Capture "south" neighbor: my BOTTOM > their TOP
        //    Opponent at (1,2), creator plays at (1,1)
        run_case(
            31, 32,
            1, 2,
            1, 1,
            /* opp bottom */ 1, /* opp left */ 1, /* opp top */ 3, /* opp right */ 1,
            /* my top   */ 1, /* my right */ 1, /* my bottom */ 5, /* my left  */ 1
        );

        // 4) Capture "west" neighbor: my LEFT > their RIGHT
        //    Opponent at (0,1), creator plays at (1,1)
        run_case(
            41, 42,
            0, 1,
            1, 1,
            /* opp bottom */ 1, /* opp left */ 1, /* opp top */ 1, /* opp right */ 3,
            /* my top   */ 1, /* my right */ 1, /* my bottom */ 1, /* my left  */ 5
        );
    });
}

#[test]
fn play_out_of_bounds_fails() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, _) = setup_new_game();
        let card = Card::new(5, 3, 2, 4);

        let out_of_bounds_move = Move {
            place_index_x: 5, // Invalid X coordinate
            place_index_y: 1, // Valid Y coordinate
            place_card: card,
        };
        let result = Eterra::play(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            out_of_bounds_move,
        );

        assert_noop!(result, crate::Error::<Test>::InvalidMove);
    });
}

#[test]
fn full_game_simulation() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();

        let moves = vec![
            (
                creator,
                Move {
                    place_index_x: 0,
                    place_index_y: 0,
                    place_card: Card::new(5, 3, 2, 4),
                },
            ),
            (
                opponent,
                Move {
                    place_index_x: 0,
                    place_index_y: 1,
                    place_card: Card::new(2, 4, 5, 3),
                },
            ),
            (
                creator,
                Move {
                    place_index_x: 1,
                    place_index_y: 0,
                    place_card: Card::new(4, 3, 1, 6),
                },
            ),
            (
                opponent,
                Move {
                    place_index_x: 1,
                    place_index_y: 1,
                    place_card: Card::new(3, 5, 2, 4),
                },
            ),
            (
                creator,
                Move {
                    place_index_x: 2,
                    place_index_y: 0,
                    place_card: Card::new(5, 3, 2, 4),
                },
            ),
            (
                opponent,
                Move {
                    place_index_x: 2,
                    place_index_y: 1,
                    place_card: Card::new(2, 4, 5, 3),
                },
            ),
            (
                creator,
                Move {
                    place_index_x: 3,
                    place_index_y: 0,
                    place_card: Card::new(4, 3, 1, 6),
                },
            ),
            (
                opponent,
                Move {
                    place_index_x: 3,
                    place_index_y: 1,
                    place_card: Card::new(3, 5, 2, 4),
                },
            ),
            (
                creator,
                Move {
                    place_index_x: 0,
                    place_index_y: 2,
                    place_card: Card::new(5, 3, 2, 4),
                },
            ),
            (
                opponent,
                Move {
                    place_index_x: 1,
                    place_index_y: 2,
                    place_card: Card::new(2, 4, 5, 3),
                },
            ),
        ];

        let mut expected_round = 0;

        for (i, (player, player_move)) in moves.iter().enumerate() {
            log::debug!(
                "Player {} is attempting to play at ({}, {}) with card: {:?}",
                player,
                player_move.place_index_x,
                player_move.place_index_y,
                player_move.place_card
            );

            // Play the move
            assert_ok!(Eterra::play(
                frame_system::RawOrigin::Signed(*player).into(),
                game_id,
                player_move.clone(),
            ));

            // Check round progression before the game is removed
            if i % 2 == 1 {
                expected_round += 1;

                // Fetch the game before it might be removed
                if let Some(game) = GameStorage::<Test>::get(&game_id) {
                    let max_rounds = game.max_rounds;
                    assert_eq!(game.round, expected_round);

                    // Log the current round "of" max_rounds
                    log::info!(
                        "Current round: {} of max rounds: {}",
                        game.round,
                        max_rounds
                    );
                } else {
                    log::warn!("Game has already been removed from storage.");
                    break;
                }
            }
        }

        // Ensure GameFinished event is emitted without relying on GameStorage
        let events = frame_system::Pallet::<Test>::events();
        let game_finished_event_found = events.iter().any(|record| match record.event {
            RuntimeEvent::Eterra(crate::Event::GameFinished {
                game_id: event_game_id,
                winner: event_winner,
            }) => {
                log::debug!(
                    "GameFinished event detected: {:?}, Winner: {:?}",
                    event_game_id,
                    event_winner
                );
                event_game_id == game_id
            }
            _ => false,
        });

        assert!(
            game_finished_event_found,
            "Expected GameFinished event was not found"
        );

        log::debug!("Full game simulation completed and GameFinished event detected.");
    });
}

#[test]
fn play_out_of_turn_fails() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Set up a new game
        let (game_id, creator, opponent) = setup_new_game();
        let card = Card::new(5, 3, 2, 4);

        // First turn: the creator plays a card
        let player_move = Move {
            place_index_x: 1,
            place_index_y: 1,
            place_card: card,
        };
        assert_ok!(Eterra::play(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            player_move.clone(),
        ));
        let another_card = Card::new(3, 4, 1, 2);

        // Second turn: the creator attempts to play again (out of turn)
        let another_move = Move {
            place_index_x: 1,
            place_index_y: 2,
            place_card: another_card,
        };
        let result = Eterra::play(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            another_move.clone(),
        );

        // Assert that the play fails with `NotYourTurn`
        assert_noop!(result, crate::Error::<Test>::NotYourTurn);

        // Confirm the opponent can play their turn
        let opponent_card = Card::new(2, 4, 5, 3);
        assert_ok!(Eterra::play(
            frame_system::RawOrigin::Signed(opponent).into(),
            game_id,
            another_move,
        ));

        log::debug!("Test completed: A player cannot play out of turn.");
    });
}

#[test]
fn capture_cards_in_all_directions() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();

        // Determine the first player based on the current turn from the pallet
        let mut current_player = Eterra::game_board(game_id).unwrap().get_player_turn();
        let mut other_player = if current_player == 0 { 1 } else { 0 };

        // Place opponent cards in cardinal directions
        let opponent_cards = vec![
            Move {
                place_index_x: 0,
                place_index_y: 1,
                place_card: Card::new(2, 4, 5, 3).with_possession(Player::PlayerTwo),
            }, // Top
            Move {
                place_index_x: 1,
                place_index_y: 0,
                place_card: Card::new(3, 5, 2, 4).with_possession(Player::PlayerTwo),
            }, // Left
            Move {
                place_index_x: 1,
                place_index_y: 2,
                place_card: Card::new(3, 5, 2, 4).with_possession(Player::PlayerTwo),
            }, // Right
            Move {
                place_index_x: 2,
                place_index_y: 1,
                place_card: Card::new(2, 4, 5, 3).with_possession(Player::PlayerTwo),
            }, // Bottom
        ];

        for player_move in opponent_cards {
            // Ensure the current player matches the expected player
            let current_game = Eterra::game_board(game_id).unwrap();
            assert_eq!(current_game.get_player_turn(), current_player);

            // Play the turn
            let player_id = if current_player == 0 {
                creator
            } else {
                opponent
            };
            assert_ok!(Eterra::play(
                frame_system::RawOrigin::Signed(player_id).into(),
                game_id,
                player_move.clone(),
            ));

            // Alternate the players for the next move
            std::mem::swap(&mut current_player, &mut other_player);
        }

        // Place capturing card in the center
        let capturing_card = Card::new(6, 6, 6, 6).with_possession(Player::PlayerOne);
        let capturing_move = Move {
            place_index_x: 1,
            place_index_y: 1,
            place_card: capturing_card,
        };

        // Ensure it's the creator's turnclear

        let game = Eterra::game_board(game_id).unwrap();
        assert_eq!(game.get_player_turn(), 0); // Creator's turn is 0
        assert_ok!(Eterra::play(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            capturing_move.clone(),
        ));

        // Validate board state
        let updated_game = Eterra::game_board(game_id).unwrap();
        assert_eq!(
            updated_game.board[1][1].as_ref().unwrap().get_possession(),
            Some(&Player::PlayerOne)
        );
        assert_eq!(
            updated_game.board[0][1].as_ref().unwrap().get_possession(),
            Some(&Player::PlayerOne)
        );
        assert_eq!(
            updated_game.board[1][0].as_ref().unwrap().get_possession(),
            Some(&Player::PlayerOne)
        );
        assert_eq!(
            updated_game.board[1][2].as_ref().unwrap().get_possession(),
            Some(&Player::PlayerOne)
        );
        assert_eq!(
            updated_game.board[2][1].as_ref().unwrap().get_possession(),
            Some(&Player::PlayerOne)
        );
    });
}
#[test]
fn invalid_game_id_fails() {
    new_test_ext().execute_with(|| {
        // Generate a random game ID that does not exist
        let invalid_game_id = H256::random();
        let card = Card::new(5, 3, 2, 4);
        let invalid_move = Move {
            place_index_x: 0,
            place_index_y: 0,
            place_card: card,
        };
        // Attempt to play a turn with the invalid game ID
        let result = Eterra::play(
            frame_system::RawOrigin::Signed(1).into(),
            invalid_game_id,
            invalid_move,
        );

        // Assert that the call fails with the `GameNotFound` error
        assert_noop!(result, crate::Error::<Test>::GameNotFound);

        // Attempt to retrieve the game board for the invalid game ID
        let board_result = Eterra::game_board(invalid_game_id);
        assert!(
            board_result.is_none(),
            "Expected None for non-existent game board"
        );

        // Log the test case for debugging purposes
        log::debug!(
            "Invalid Game ID test completed. Game ID: {:?}, Result: {:?}",
            invalid_game_id,
            result
        );
    });
}

#[test]
fn create_game_invalid_number_of_players() {
    init_logger();
    new_test_ext().execute_with(|| {
        let creator = 1;
        let opponent = 2;
        let third_player = 3;

        // Test with zero players
        let result_zero_players =
            Eterra::create_game(frame_system::RawOrigin::Signed(creator).into(), vec![], pallet::GameMode::PvP);
        assert_noop!(
            result_zero_players,
            crate::Error::<Test>::CreatorMustBeInGame
        );

        // Test with one player
        let result_one_player = Eterra::create_game(
            frame_system::RawOrigin::Signed(creator).into(),
            vec![creator],
            pallet::GameMode::PvP,
        );
        assert_noop!(
            result_one_player,
            crate::Error::<Test>::InvalidNumberOfPlayers
        );

        // Test with three players
        let result_three_players = Eterra::create_game(
            frame_system::RawOrigin::Signed(creator).into(),
            vec![creator, opponent, third_player],
            pallet::GameMode::PvP,
        );
        assert_noop!(
            result_three_players,
            crate::Error::<Test>::InvalidNumberOfPlayers
        );

        // Valid two-player game
        let result_two_players = Eterra::create_game(
            frame_system::RawOrigin::Signed(creator).into(),
            vec![creator, opponent],
            pallet::GameMode::PvP,
        );
        assert_ok!(result_two_players);
    });
}

#[test]
fn game_winner_is_correctly_emitted() {
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();

        // Strategic moves: Player 0 (creator) will win
        let moves = vec![
            (
                creator,
                Move {
                    place_index_x: 0,
                    place_index_y: 0,
                    place_card: Card::new(5, 3, 2, 4), // Strong card for Player 0
                },
            ),
            (
                opponent,
                Move {
                    place_index_x: 0,
                    place_index_y: 1,
                    place_card: Card::new(2, 2, 2, 2), // Weak card for Player 1
                },
            ),
            (
                creator,
                Move {
                    place_index_x: 1,
                    place_index_y: 0,
                    place_card: Card::new(6, 6, 6, 6), // Strong card for Player 0
                },
            ),
            (
                opponent,
                Move {
                    place_index_x: 1,
                    place_index_y: 1,
                    place_card: Card::new(3, 3, 3, 3), // Moderate card for Player 1
                },
            ),
            (
                creator,
                Move {
                    place_index_x: 2,
                    place_index_y: 0,
                    place_card: Card::new(4, 3, 4, 3), // Strong card for Player 0
                },
            ),
            (
                opponent,
                Move {
                    place_index_x: 2,
                    place_index_y: 1,
                    place_card: Card::new(2, 4, 2, 4), // Weak card for Player 1
                },
            ),
            (
                creator,
                Move {
                    place_index_x: 3,
                    place_index_y: 0,
                    place_card: Card::new(5, 5, 5, 5), // Strong card for Player 0
                },
            ),
            (
                opponent,
                Move {
                    place_index_x: 3,
                    place_index_y: 1,
                    place_card: Card::new(1, 1, 1, 1), // Weak card for Player 1
                },
            ),
            (
                creator,
                Move {
                    place_index_x: 0,
                    place_index_y: 2,
                    place_card: Card::new(6, 4, 6, 4), // Strong card for Player 0
                },
            ),
            (
                opponent,
                Move {
                    place_index_x: 1,
                    place_index_y: 2,
                    place_card: Card::new(2, 2, 2, 2), // Weak card for Player 1
                },
            ),
        ];

        // Play the game
        for (player, player_move) in moves.iter() {
            assert_ok!(Eterra::play(
                frame_system::RawOrigin::Signed(*player).into(),
                game_id,
                player_move.clone(),
            ));
        }

        // Check if the GameFinished event is emitted
        let events = frame_system::Pallet::<Test>::events();
        let game_finished_event_found = events.iter().any(|record| match &record.event {
            RuntimeEvent::Eterra(crate::Event::GameFinished {
                game_id: event_game_id,
                winner: event_winner,
            }) => {
                assert_eq!(*event_game_id, game_id);
                assert_eq!(*event_winner, Some(creator)); // Player 0 should win
                true
            }
            _ => false,
        });

        assert!(
            game_finished_event_found,
            "Expected GameFinished event was not found"
        );

        log::info!("Game winner is correctly emitted as Player 0.");
    });
}

#[test]
fn exceeding_max_moves_emits_error() {
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();

        // 10 valid moves to complete the game
        let moves = vec![
            (
                creator,
                Move {
                    place_index_x: 0,
                    place_index_y: 0,
                    place_card: Card::new(5, 3, 2, 4),
                },
            ),
            (
                opponent,
                Move {
                    place_index_x: 0,
                    place_index_y: 1,
                    place_card: Card::new(2, 2, 2, 2),
                },
            ),
            (
                creator,
                Move {
                    place_index_x: 1,
                    place_index_y: 0,
                    place_card: Card::new(6, 6, 6, 6),
                },
            ),
            (
                opponent,
                Move {
                    place_index_x: 1,
                    place_index_y: 1,
                    place_card: Card::new(3, 3, 3, 3),
                },
            ),
            (
                creator,
                Move {
                    place_index_x: 2,
                    place_index_y: 0,
                    place_card: Card::new(4, 3, 4, 3),
                },
            ),
            (
                opponent,
                Move {
                    place_index_x: 2,
                    place_index_y: 1,
                    place_card: Card::new(2, 4, 2, 4),
                },
            ),
            (
                creator,
                Move {
                    place_index_x: 3,
                    place_index_y: 0,
                    place_card: Card::new(5, 5, 5, 5),
                },
            ),
            (
                opponent,
                Move {
                    place_index_x: 3,
                    place_index_y: 1,
                    place_card: Card::new(1, 1, 1, 1),
                },
            ),
            (
                creator,
                Move {
                    place_index_x: 0,
                    place_index_y: 2,
                    place_card: Card::new(6, 4, 6, 4),
                },
            ),
            (
                opponent,
                Move {
                    place_index_x: 1,
                    place_index_y: 2,
                    place_card: Card::new(2, 2, 2, 2),
                },
            ),
        ];

        // Play all 10 moves
        for (player, player_move) in moves.iter() {
            assert_ok!(Eterra::play(
                frame_system::RawOrigin::Signed(*player).into(),
                game_id,
                player_move.clone(),
            ));
        }

        // Attempt an 11th move
        let extra_move = Move {
            place_index_x: 2,
            place_index_y: 2,
            place_card: Card::new(5, 5, 5, 5),
        };
        let result = Eterra::play(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            extra_move,
        );

        // Assert that the play fails with an error indicating the game has ended
        assert_noop!(result, crate::Error::<Test>::GameNotFound);

        // Verify that the game has already finished and removed from storage
        let game = GameStorage::<Test>::get(&game_id);
        assert!(
            game.is_none(),
            "Game should have been removed after completion."
        );

        log::info!("Exceeding max moves correctly emits an error and prevents further plays.");
    });
}

/// Test: force_finish_turn fails if the caller is not in the game.
#[test]
fn force_finish_turn_fails_if_caller_not_in_game() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();
        let non_player = 99; // someone who is not a game participant

        // Attempt to force finish with a non-player
        let result =
            Eterra::force_finish_turn(frame_system::RawOrigin::Signed(non_player).into(), game_id);

        assert_noop!(result, crate::Error::<Test>::PlayerNotInGame);
    });
}

/// Test: force_finish_turn fails if the caller is the current player.
#[test]
fn force_finish_turn_fails_if_current_player() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();

        // By default, one of these (creator or opponent) will be randomly chosen as current player.
        let game = Eterra::game_board(game_id).unwrap();
        let current_player = game.players[game.player_turn as usize];

        // Attempt to force finish turn from the current player
        let result = Eterra::force_finish_turn(
            frame_system::RawOrigin::Signed(current_player).into(),
            game_id,
        );

        assert_noop!(
            result,
            crate::Error::<Test>::CurrentPlayerCannotForceFinishTurn
        );
    });
}

/// Test: force_finish_turn fails if the BlocksToPlayLimit has not yet passed.
#[test]
fn force_finish_turn_fails_if_blocks_not_passed() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();

        // Let's assume the initial turn is creator. Then the opponent tries to force finish.
        // We intentionally do NOT move the block number forward, so BlocksToPlayLimit is not passed.

        // Attempt to force finish from the opponent
        let result =
            Eterra::force_finish_turn(frame_system::RawOrigin::Signed(opponent).into(), game_id);
        assert_noop!(result, crate::Error::<Test>::BlocksToPlayLimitNotPassed);
    });
}

/// Test: force_finish_turn succeeds under correct conditions.
/// * The caller is a game participant, but not the current player.
/// * The BlocksToPlayLimit has passed.
///
/// We will:
/// 1. Create a new game
/// 2. Identify the current player
/// 3. Advance the block number by (BlocksToPlayLimit + 1) to exceed the limit
/// 4. Call force_finish_turn from the other player
/// 5. Check that the turn is forced, a TurnForceFinished and NewTurn event are emitted,
///    and the last_played_block is updated.
#[test]
fn force_finish_turn_works_when_limit_passed() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Setup
        let (game_id, creator, opponent) = setup_new_game();
        let blocks_limit = <Test as crate::Config>::BlocksToPlayLimit::get();

        // Identify the current player
        let game_before = Eterra::game_board(game_id).unwrap();
        let current_player = game_before.players[game_before.player_turn as usize];

        // We'll assume that if the current_player == creator, then the caller to force finish
        // will be the opponent, and vice versa.
        let caller = if current_player == creator {
            opponent
        } else {
            creator
        };

        // Advance the block number to exceed the limit
        let current_block = <frame_system::Pallet<Test>>::block_number();
        let target_block = current_block + (blocks_limit as u64) + 1;
        run_to_block(target_block);

        // Attempt to force finish turn from the other player
        assert_ok!(Eterra::force_finish_turn(
            frame_system::RawOrigin::Signed(caller).into(),
            game_id,
        ));

        // Now check that the game updated the turn and block number
        let game_after = Eterra::game_board(game_id).unwrap();
        let new_current_player = game_after.players[game_after.player_turn as usize];

        // The current player should be different after forcing the turn
        assert_ne!(current_player, new_current_player);

        // The last_played_block should be set to whatever block number we advanced to
        assert_eq!(
            game_after.last_played_block,
            <u64 as Into<BlockNumberFor<Test>>>::into(target_block)
        );

        // Check that events were emitted
        let events = frame_system::Pallet::<Test>::events();
        let mut force_finished_found = false;
        let mut new_turn_found = false;

        for record in events {
            if let RuntimeEvent::Eterra(crate::Event::TurnForceFinished {
                game_id: event_game_id,
                player: event_player,
            }) = record.event
            {
                if event_game_id == game_id && event_player == current_player {
                    force_finished_found = true;
                }
            } else if let RuntimeEvent::Eterra(crate::Event::NewTurn {
                game_id: event_game_id,
                next_player: event_next_player,
            }) = record.event
            {
                if event_game_id == game_id && event_next_player == new_current_player {
                    new_turn_found = true;
                }
            }
        }

        assert!(
            force_finished_found,
            "Expected TurnForceFinished event not found"
        );
        assert!(new_turn_found, "Expected NewTurn event not found");
    });
}

#[test]
fn play_emits_new_turn_event() {
    init_logger();
    new_test_ext().execute_with(|| {
        // 1. Setup game
        let (game_id, creator, opponent) = setup_new_game();

        // 2. Creator plays first
        let creator_card = Card::new(5, 3, 2, 4).with_possession(Player::PlayerOne);
        let creator_move = Move {
            place_index_x: 1,
            place_index_y: 1,
            place_card: creator_card.clone(),
        };

        assert_ok!(Eterra::play(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            creator_move.clone(),
        ));

        // 3. Check for NewTurn event
        let events = frame_system::Pallet::<Test>::events();
        let mut new_turn_found = false;

        // We expect the new turn to belong to the other player (i.e., opponent)
        let expected_next_player = opponent;

        for record in &events {
            if let RuntimeEvent::Eterra(crate::Event::NewTurn {
                game_id: event_game_id,
                next_player,
            }) = &record.event
            {
                if *event_game_id == game_id && *next_player == expected_next_player {
                    new_turn_found = true;
                    break;
                }
            }
        }

        assert!(
            new_turn_found,
            "Expected NewTurn event was not found after creator's move!"
        );

        // 4. Clear the events so we can test the opponent's move cleanly
        System::reset_events();

        // 5. Opponent plays next
        let opponent_card = Card::new(2, 4, 5, 3).with_possession(Player::PlayerTwo);
        let opponent_move = Move {
            place_index_x: 1,
            place_index_y: 2,
            place_card: opponent_card.clone(),
        };

        assert_ok!(Eterra::play(
            frame_system::RawOrigin::Signed(opponent).into(),
            game_id,
            opponent_move.clone(),
        ));

        // 6. Check for NewTurn event again (now the next turn should belong to the creator)
        let events = frame_system::Pallet::<Test>::events();
        let mut new_turn_found = false;

        let expected_next_player = creator; // After opponent, it goes back to creator

        for record in &events {
            if let RuntimeEvent::Eterra(crate::Event::NewTurn {
                game_id: event_game_id,
                next_player,
            }) = &record.event
            {
                if *event_game_id == game_id && *next_player == expected_next_player {
                    new_turn_found = true;
                    break;
                }
            }
        }

        assert!(
            new_turn_found,
            "Expected NewTurn event was not found after opponent's move!"
        );
    });
}

#[test]
fn debug_game_rounds_and_termination() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();
        let max_rounds = <Test as crate::Config>::MaxRounds::get();
        let mut expected_round = 0;

        log::info!("Starting game with max rounds: {}", max_rounds);

        // Play a full game up to max_rounds
        for i in 0..(max_rounds * 2) {
            let current_player = if i % 2 == 0 { creator } else { opponent };
            let card = Card::new(5, 3, 2, 4);
            let player_move = Move {
                place_index_x: (i % 4) as u8,
                place_index_y: ((i / 4) % 4) as u8,
                place_card: card,
            };

            log::debug!(
                "Turn {}: Player {} placing at ({}, {})",
                i + 1,
                current_player,
                player_move.place_index_x,
                player_move.place_index_y
            );

            assert_ok!(Eterra::play(
                frame_system::RawOrigin::Signed(current_player).into(),
                game_id,
                player_move.clone(),
            ));

            // Check round progression before game removal
            if i % 2 == 1 {
                expected_round += 1;

                if let Some(game) = GameStorage::<Test>::get(&game_id) {
                    assert_eq!(game.round, expected_round);
                    log::info!("✅ Current round: {} / {}", game.round, max_rounds);
                } else {
                    log::warn!("⚠️ Game removed from storage, likely finished.");
                    break;
                }
            }
        }

        // Ensure `GameFinished` event was emitted
        let events = frame_system::Pallet::<Test>::events();
        let game_finished_event_found = events.iter().any(|record| match record.event {
            RuntimeEvent::Eterra(crate::Event::GameFinished {
                game_id: event_game_id,
                winner: event_winner,
            }) => {
                log::info!(
                    "🎉 GameFinished Event Found: Game ID {:?}, Winner: {:?}",
                    event_game_id,
                    event_winner
                );
                event_game_id == game_id
            }
            _ => false,
        });

        assert!(
            game_finished_event_found,
            "❌ Expected GameFinished event was NOT found!"
        );

        // Ensure the game is removed from storage
        let game = GameStorage::<Test>::get(&game_id);
        assert!(
            game.is_none(),
            "❌ Game should have been removed after completion."
        );

        log::info!("✅ Game successfully completed and removed from storage.");
    });
}


#[test]
fn submit_hand_rejects_unowned_card() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();
        // creator owns 5 cards, opponent owns 1 card
        let mut creator_cards = mint_cards_for(creator, 5);
        let opp_cards = mint_cards_for(opponent, 1);
        // Replace one creator card with opponent's card id
        creator_cards[0] = opp_cards[0];

        let res = Eterra::submit_hand(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            creator_cards,
        );
        assert_noop!(res, crate::Error::<Test>::CardNotOwned);
    });
}

#[test]
fn submit_hand_accepts_owned_cards_and_prevents_resubmit() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, _opponent) = setup_new_game();
        let creator_cards = mint_cards_for(creator, 5);

        assert_ok!(Eterra::submit_hand(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            creator_cards.clone(),
        ));

        // Submitting again should fail
        let res = Eterra::submit_hand(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            creator_cards,
        );
        assert_noop!(res, crate::Error::<Test>::HandAlreadySubmitted);
    });
}

#[test]
fn play_from_hand_requires_hand_submission() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();
        // Ensure creator has the turn; advance with simple plays if needed
        ensure_my_turn(game_id, creator, opponent);
        let res = Eterra::play_from_hand(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            0, // index
            0, // x
            0, // y
        );
        assert_noop!(res, crate::Error::<Test>::HandNotSubmitted);
    });
}

#[test]
fn play_from_hand_marks_used_and_prevents_reuse() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();
        let creator_cards = mint_cards_for(creator, 5);
        assert_ok!(Eterra::submit_hand(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            creator_cards,
        ));

        // Make sure it's creator's turn
        ensure_my_turn(game_id, creator, opponent);

        // First play from hand index 0 is OK
        assert_ok!(Eterra::play_from_hand(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            0,
            0,
            0,
        ));

        // Advance back to creator's turn
        ensure_my_turn(game_id, creator, opponent);

        // Attempt to reuse index 0 should fail
        let res = Eterra::play_from_hand(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            0,
            1,
            0,
        );
        assert_noop!(res, crate::Error::<Test>::CardAlreadyUsed);
    });
}

#[test]
fn play_from_hand_index_out_of_range_fails() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();
        let creator_cards = mint_cards_for(creator, 5);
        assert_ok!(Eterra::submit_hand(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            creator_cards,
        ));

        ensure_my_turn(game_id, creator, opponent);
        // Hand size is 5 (indices 0..=4). Use 5 to cause out-of-range.
        let res = Eterra::play_from_hand(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            5,
            0,
            1,
        );
        assert_noop!(res, crate::Error::<Test>::HandIndexOutOfRange);
    });
}

#[test]
fn submit_hand_fails_for_unknown_game() {
    init_logger();
    new_test_ext().execute_with(|| {
        let creator = 1u64;
        // Mint exactly 5 cards so the hand size is valid
        let card_ids = mint_cards_for(creator, 5);

        // Use a random game id that does not exist in storage
        let fake_game_id = H256::random();

        // Attempt to submit a hand for a non-existent game should fail fast
        let res = Eterra::submit_hand(
            frame_system::RawOrigin::Signed(creator).into(),
            fake_game_id,
            card_ids,
        );
        assert_noop!(res, crate::Error::<Test>::GameNotFound);
    });
}

#[test]
fn submit_hand_wrong_size_rejected() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, _opponent) = setup_new_game();
        // Mint 5 cards but submit only 4
        let cards = mint_cards_for(creator, 5);
        let mut too_small = cards.clone();
        too_small.pop();
        let res = Eterra::submit_hand(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            too_small,
        );
        assert_noop!(res, crate::Error::<Test>::HandSizeInvalid);

        // Submit 6 (too many)
        // Mint one more card
        let extra = mint_cards_for(creator, 1);
        let mut too_big = cards.clone();
        too_big.extend_from_slice(&extra);
        let res = Eterra::submit_hand(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            too_big,
        );
        assert_noop!(res, crate::Error::<Test>::HandSizeInvalid);
    });
}

#[test]
fn submit_hand_rejects_duplicates() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, _opponent) = setup_new_game();
        let mut cards = mint_cards_for(creator, 5);
        // Duplicate first id into second slot
        if cards.len() >= 2 { cards[1] = cards[0]; }
        let res = Eterra::submit_hand(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            cards,
        );
        assert_noop!(res, crate::Error::<Test>::DuplicateCardInHand);
    });
}

#[test]
fn submit_hand_by_non_player_fails() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, _creator, _opponent) = setup_new_game();
        let rando = 77u64;
        let ids = mint_cards_for(rando, 5);
        let res = Eterra::submit_hand(
            frame_system::RawOrigin::Signed(rando).into(),
            game_id,
            ids,
        );
        assert_noop!(res, crate::Error::<Test>::PlayerNotInGame);
    });
}

#[test]
fn play_from_hand_not_your_turn_fails() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();
        // Submit valid hand for creator
        let ids = mint_cards_for(creator, 5);
        assert_ok!(Eterra::submit_hand(frame_system::RawOrigin::Signed(creator).into(), game_id, ids));

        // Ensure it's NOT the creator's turn: if it is, have creator make a normal play to pass turn
        {
            let game = Eterra::game_board(game_id).unwrap();
            let current = game.players[game.player_turn as usize];
            if current == creator {
                let m = Move { place_index_x: 0, place_index_y: 0, place_card: Card::new(1,1,1,1) };
                assert_ok!(Eterra::play(frame_system::RawOrigin::Signed(creator).into(), game_id, m));
            }
        }

        // Now it should be opponent's turn; creator attempts to play_from_hand
        let res = Eterra::play_from_hand(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            0,
            1,
            1,
        );
        assert_noop!(res, crate::Error::<Test>::NotYourTurn);
    });
}

#[test]
fn play_from_hand_cell_occupied_and_bounds_checked() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();
        let ids = mint_cards_for(creator, 5);
        assert_ok!(Eterra::submit_hand(frame_system::RawOrigin::Signed(creator).into(), game_id, ids));

        // Ensure creator's turn
        ensure_my_turn(game_id, creator, opponent);

        // Opponent occupies (0,0) first to trigger CellOccupied later
        {
            let m = Move { place_index_x: 0, place_index_y: 0, place_card: Card::new(1,1,1,1) };
            // if it's not opponent's turn yet, make creator play one trivial move to pass
            let game = Eterra::game_board(game_id).unwrap();
            if game.players[game.player_turn as usize] != opponent {
                let m2 = Move { place_index_x: 1, place_index_y: 0, place_card: Card::new(1,1,1,1) };
                assert_ok!(Eterra::play(frame_system::RawOrigin::Signed(creator).into(), game_id, m2));
            }
            assert_ok!(Eterra::play(frame_system::RawOrigin::Signed(opponent).into(), game_id, m));
        }

        // Bring turn back to creator
        ensure_my_turn(game_id, creator, opponent);

        // Out of bounds from hand
        let res = Eterra::play_from_hand(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            0,
            4, // x out of bounds
            0,
        );
        assert_noop!(res, crate::Error::<Test>::InvalidMove);

        // Cell occupied
        let res = Eterra::play_from_hand(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            0,
            0,
            0,
        );
        assert_noop!(res, crate::Error::<Test>::CellOccupied);
    });
}

#[test]
fn transfer_after_submit_does_not_block_play() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();
        let ids = mint_cards_for(creator, 5);
        assert_ok!(Eterra::submit_hand(frame_system::RawOrigin::Signed(creator).into(), game_id, ids.clone()));

        // Transfer the first card to opponent AFTER submission
        let first = ids[0];
        assert_ok!(cards::Pallet::<Test>::transfer_card(
            frame_system::RawOrigin::Signed(creator).into(),
            first,
            opponent,
        ));

        // Ensure it's creator's turn and play from the hand index 0; should still work
        ensure_my_turn(game_id, creator, opponent);
        assert_ok!(Eterra::play_from_hand(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            0,
            0,
            0,
        ));
    });
}
#[cfg(test)]
mod ai_integration_tests {
    use super::*;
    use crate::mock::*;
    use crate::{Move, GameStorage};
    use frame_support::{assert_ok, assert_noop};
    use sp_core::H256;
    use eterra_card_ai_adapter::eterra_adapter as ai;
    use pallet_eterra_monte_carlo_ai as mc_ai;
    use crate::types::card::Card;
    use crate::types::game::GameProperties;
    use frame_system::RawOrigin;
    use crate::HandsOfGame;
    use crate::types::card::Possession as Player;

    // Bring in the mint_cards_for helper
    use super::mint_cards_for;

    /// Helper to create a new PvE game (human vs AI).
    fn setup_pve_game() -> (H256, u64, <Test as frame_system::Config>::AccountId) {
        let human: u64 = 1;
        let ai_account: <Test as frame_system::Config>::AccountId = <Test as crate::Config>::AiAccount::get();
        let current_block_number = <frame_system::Pallet<Test>>::block_number();
        let game_id = sp_runtime::traits::BlakeTwo256::hash_of(&(human, ai_account, current_block_number));
        assert_ok!(Eterra::create_game(
            RawOrigin::Signed(human).into(),
            vec![human],
            pallet::GameMode::PvE,
        ));
        (game_id, human, ai_account)
    }

    #[test]
    fn pve_game_creation_generates_ai_hand() {
        new_test_ext().execute_with(|| {
            let (game_id, human, ai_account) = setup_pve_game();
            // AI hand is generated and stored in HandsOfGame
            let ai_hand = HandsOfGame::<Test>::get(&game_id, &ai_account).expect("AI hand exists");
            assert_eq!(ai_hand.len() as u32, <Test as crate::Config>::HandSize::get());
        });
    }

    #[test]
    fn human_submits_hand_and_plays_one_move_pve() {
        new_test_ext().execute_with(|| {
            let (game_id, human, ai_account) = setup_pve_game();
            // Mint cards and submit hand for human
            let ids = mint_cards_for(human, 5);
            assert_ok!(Eterra::submit_hand(
                RawOrigin::Signed(human).into(),
                game_id,
                ids.clone()
            ));
            // Ensure it's the human's turn
            let game = GameStorage::<Test>::get(&game_id).unwrap();
            let human_idx = if game.players[0] == human { 0 } else { 1 };
            let ai_idx = 1 - human_idx;
            assert_eq!(game.players[game.player_turn as usize], human);
            // Play from hand index 0 at (0,0)
            assert_ok!(Eterra::play_from_hand(
                RawOrigin::Signed(human).into(),
                game_id,
                0,
                0,
                0
            ));
            // After the move: after AI auto-move, turn returns to human and board updated
            let updated = GameStorage::<Test>::get(&game_id).unwrap();
            assert_eq!(updated.players[updated.player_turn as usize], human, "After AI auto-move, turn returns to human");
            assert_eq!(updated.round, 1, "Round should advance after human+AI moves");
            // Board at (0,0) should be Some
            assert!(updated.board[0][0].is_some());
        });
    }

    #[test]
    fn ai_can_produce_suggestion_from_current_state() {
        new_test_ext().execute_with(|| {
            let (game_id, human, ai_account) = setup_pve_game();
            // Mint cards and submit hand for human
            let ids = mint_cards_for(human, 5);
            assert_ok!(Eterra::submit_hand(
                RawOrigin::Signed(human).into(),
                game_id,
                ids.clone()
            ));
            // Ensure both hands exist
            let game = GameStorage::<Test>::get(&game_id).unwrap();
            // Use the AI adapter to map state explicitly (avoid relying on non-existent EterraState::from_game)
            // Map board: crate Card -> adapter Card
            let mut board: [[Option<ai::Card>; 4]; 4] = core::array::from_fn(|_| core::array::from_fn(|_| None));
            for x in 0..4usize {
                for y in 0..4usize {
                    if let Some(c) = &game.board[x][y] {
                        let possession = c.get_possession().cloned().map(|p| match p {
                            Player::PlayerOne => ai::Possession::PlayerOne,
                            Player::PlayerTwo => ai::Possession::PlayerTwo,
                        });
                        board[x][y] = Some(ai::Card {
                            top: c.top,
                            right: c.right,
                            bottom: c.bottom,
                            left: c.left,
                            possession,
                        });
                    }
                }
            }

            // Map hands from on-chain storage into adapter hands
            let human_hand_bv = HandsOfGame::<Test>::get(&game_id, &human).expect("human hand (or submit earlier if needed)");
            let ai_hand_bv = HandsOfGame::<Test>::get(&game_id, &ai_account).expect("AI hand exists");

            assert_eq!(human_hand_bv.len() as u32, <Test as crate::Config>::HandSize::get(), "hand size must equal HandSize");
            assert_eq!(ai_hand_bv.len() as u32, <Test as crate::Config>::HandSize::get(), "ai hand size must equal HandSize");

            let to_adapter_hand = |bv: &BoundedVec<crate::pallet::HandEntry, crate::pallet::HandLimit>| -> ai::Hand {
                let entries: [ai::HandEntry; 5] = core::array::from_fn(|i| {
                    let he = &bv[i];
                    ai::HandEntry {
                        north: he.north,
                        east: he.east,
                        south: he.south,
                        west: he.west,
                        used: he.used,
                    }
                });
                ai::Hand { entries }
            };

            let hands = [
                if game.players[0] == human { to_adapter_hand(&human_hand_bv) } else { to_adapter_hand(&ai_hand_bv) },
                if game.players[1] == ai_account { to_adapter_hand(&ai_hand_bv) } else { to_adapter_hand(&human_hand_bv) },
            ];

            let state = ai::State {
                board,
                scores: game.scores,
                player_turn: game.player_turn,
                round: game.round,
                max_rounds: game.max_rounds,
                hands,
            };

            let diff = <Test as crate::Config>::AiDifficulty::get();
            let suggestion = mc_ai::Pallet::<Test>::suggest::<ai::Adapter>(&state, diff);
            assert!(suggestion.is_some(), "Monte Carlo AI should produce a suggestion");
            // The suggestion should be a valid move index (hand_idx, x, y)
            let a = suggestion.unwrap();
            let hand_idx = a.hand_index;
            let (x, y) = (a.x, a.y);
            assert!(usize::from(hand_idx) < <Test as crate::Config>::HandSize::get() as usize);
            assert!(x < 4 && y < 4, "Board is 4x4");
        });
    }
}

#[test]
fn multiple_pve_games_have_independent_ai_state() {
    new_test_ext().execute_with(|| {
        // --- Create two separate PvE games (different humans, same AI account) ---
        let human1: u64 = 1;
        let human2: u64 = 3;
        let ai_account: <Test as frame_system::Config>::AccountId =
            <Test as crate::Config>::AiAccount::get();

        // Game A
        let current_block_a = <frame_system::Pallet<Test>>::block_number();
        let game_id_a =
            sp_runtime::traits::BlakeTwo256::hash_of(&(human1, ai_account, current_block_a));
        assert_ok!(Eterra::create_game(
            RawOrigin::Signed(human1).into(),
            vec![human1],
            pallet::GameMode::PvE,
        ));

        // Game B
        let current_block_b = <frame_system::Pallet<Test>>::block_number();
        let game_id_b =
            sp_runtime::traits::BlakeTwo256::hash_of(&(human2, ai_account, current_block_b));
        assert_ok!(Eterra::create_game(
            RawOrigin::Signed(human2).into(),
            vec![human2],
            pallet::GameMode::PvE,
        ));

        // AI hands should start with all entries unused
        let ai_hand_a_initial = HandsOfGame::<Test>::get(&game_id_a, &ai_account).expect("AI hand A exists");
        let ai_hand_b_initial = HandsOfGame::<Test>::get(&game_id_b, &ai_account).expect("AI hand B exists");
        assert_eq!(ai_hand_a_initial.iter().filter(|e| e.used).count(), 0, "AI hand A should start unused");
        assert_eq!(ai_hand_b_initial.iter().filter(|e| e.used).count(), 0, "AI hand B should start unused");

        // --- Submit human hands (AI hand was auto-generated at game creation) ---
        let ids1 = mint_cards_for(human1, 5);
        let ids2 = mint_cards_for(human2, 5);
        assert_ok!(Eterra::submit_hand(RawOrigin::Signed(human1).into(), game_id_a, ids1));
        assert_ok!(Eterra::submit_hand(RawOrigin::Signed(human2).into(), game_id_b, ids2));

        // --- Human1 plays one move in Game A ---
        assert_ok!(Eterra::play_from_hand(
            RawOrigin::Signed(human1).into(),
            game_id_a, 0, 0, 0,
        ));
        // After human1 plays, AI in Game A should auto-move once.
        let g_a_after_h1 = GameStorage::<Test>::get(&game_id_a).unwrap();
        assert_eq!(g_a_after_h1.players[g_a_after_h1.player_turn as usize], human1, "Turn should be back to human1 after AI auto-move in Game A");
        let ai_hand_a_after_h1 = HandsOfGame::<Test>::get(&game_id_a, &ai_account).unwrap();
        let used_a_after_h1 = ai_hand_a_after_h1.iter().filter(|e| e.used).count();
        assert_eq!(used_a_after_h1, 1, "Game A AI should have used exactly one card after human1's move");

        // Game B AI should still have used none at this point
        let ai_hand_b_still = HandsOfGame::<Test>::get(&game_id_b, &ai_account).unwrap();
        let used_b_still = ai_hand_b_still.iter().filter(|e| e.used).count();
        assert_eq!(used_b_still, 0, "Game B AI should not have moved yet");

        // --- Human2 plays one move in Game B ---
        assert_ok!(Eterra::play_from_hand(
            RawOrigin::Signed(human2).into(),
            game_id_b, 0, 0, 0,
        ));
        // After human2 plays, AI in Game B should auto-move once.
        let g_b_after_h2 = GameStorage::<Test>::get(&game_id_b).unwrap();
        assert_eq!(g_b_after_h2.players[g_b_after_h2.player_turn as usize], human2, "Turn should be back to human2 after AI auto-move in Game B");
        let ai_hand_b_after_h2 = HandsOfGame::<Test>::get(&game_id_b, &ai_account).unwrap();
        let used_b_after_h2 = ai_hand_b_after_h2.iter().filter(|e| e.used).count();
        assert_eq!(used_b_after_h2, 1, "Game B AI should have used exactly one card after human2's move");

        // Game A should still show exactly one AI-used card (independent state)
        let ai_hand_a_final = HandsOfGame::<Test>::get(&game_id_a, &ai_account).unwrap();
        let used_a_final = ai_hand_a_final.iter().filter(|e| e.used).count();
        assert_eq!(used_a_final, 1, "Game A AI used count should remain 1 and be independent of Game B");
    });
}

#[test]
fn creator_cannot_start_second_pvp_game_while_active() {
    new_test_ext().execute_with(|| {
        let creator: u64 = 1;
        let opponent_a: u64 = 2;
        let opponent_b: u64 = 3;

        // First PvP game should succeed.
        assert_ok!(Eterra::create_game(
            RawOrigin::Signed(creator).into(),
            vec![creator, opponent_a],
            pallet::GameMode::PvP,
        ));

        // Attempt to start a second PvP game while the first is still active must fail.
        let res = Eterra::create_game(
            RawOrigin::Signed(creator).into(),
            vec![creator, opponent_b],
            pallet::GameMode::PvP,
        );
        assert_noop!(res, crate::Error::<Test>::PlayerAlreadyInGame);

        // Sanity: the opponent who isn't in any game can still start a game with someone else.
        assert_ok!(Eterra::create_game(
            RawOrigin::Signed(opponent_b).into(),
            vec![opponent_b, 4u64],
            pallet::GameMode::PvP,
        ));
    });
}

#[test]
fn creator_cannot_start_second_pve_game_while_active() {
    new_test_ext().execute_with(|| {
        let human: u64 = 10;
        let ai_acc: <Test as frame_system::Config>::AccountId = <Test as crate::Config>::AiAccount::get();

        // First PvE game should succeed. (Players vec must include the human/creator.)
        assert_ok!(Eterra::create_game(
            RawOrigin::Signed(human).into(),
            vec![human],
            pallet::GameMode::PvE,
        ));

        // Attempt to start a second PvE game for the same human while the first is active must fail.
        let res = Eterra::create_game(
            RawOrigin::Signed(human).into(),
            vec![human],
            pallet::GameMode::PvE,
        );
        assert_noop!(res, crate::Error::<Test>::PlayerAlreadyInGame);

        // Another human should still be able to start their own PvE game concurrently.
        let other_human: u64 = 11;
        assert_ok!(Eterra::create_game(
            RawOrigin::Signed(other_human).into(),
            vec![other_human],
            pallet::GameMode::PvE,
        ));
    });
}