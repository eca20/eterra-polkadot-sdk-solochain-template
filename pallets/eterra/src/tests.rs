use crate::types::game::GameProperties; // Import the GameProperties trait
use crate::GameStorage;
use crate::Color;
use crate::Move;
use crate::{mock::*, Card};
use frame_support::{assert_noop, assert_ok};
use log::{Level, Metadata, Record};
use sp_core::H256; // Fix: Import H256
use sp_runtime::traits::{BlakeTwo256, Hash};
use std::sync::Once;

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

#[test]
fn create_game_with_same_players_fails() {
    init_logger();
    new_test_ext().execute_with(|| {
        let player = 1; // Define `player` explicitly
        let result = Eterra::create_game(
            frame_system::RawOrigin::Signed(player).into(),
            vec![player, player], // Pass the same player twice
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

        let card = Card::new(5, 3, 2, 4).with_color(Color::Blue);

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

        let opponent_card = Card::new(2, 4, 5, 3).with_color(Color::Red);

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

        let creator_card = Card::new(3, 5, 2, 1).with_color(Color::Blue);

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

        let opponent_card = Card::new(2, 4, 5, 3).with_color(Color::Red);

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

        let capturing_card = Card::new(6, 6, 3, 3).with_color(Color::Blue);

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
            game.board[0][1].as_ref().unwrap().get_color(),
            Some(&Color::Blue)
        );
        assert_eq!(
            game.board[0][2].as_ref().unwrap().get_color(),
            Some(&Color::Blue)
        ); // Captured card
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
                place_card: Card::new(2, 4, 5, 3).with_color(Color::Red),
            }, // Top
            Move {
                place_index_x: 1,
                place_index_y: 0,
                place_card: Card::new(3, 5, 2, 4).with_color(Color::Red),
            }, // Left
            Move {
                place_index_x: 1,
                place_index_y: 2,
                place_card: Card::new(3, 5, 2, 4).with_color(Color::Red),
            }, // Right
            Move {
                place_index_x: 2,
                place_index_y: 1,
                place_card: Card::new(2, 4, 5, 3).with_color(Color::Red),
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
        let capturing_card = Card::new(6, 6, 6, 6).with_color(Color::Blue);
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
            updated_game.board[1][1].as_ref().unwrap().get_color(),
            Some(&Color::Blue)
        );
        assert_eq!(
            updated_game.board[0][1].as_ref().unwrap().get_color(),
            Some(&Color::Blue)
        );
        assert_eq!(
            updated_game.board[1][0].as_ref().unwrap().get_color(),
            Some(&Color::Blue)
        );
        assert_eq!(
            updated_game.board[1][2].as_ref().unwrap().get_color(),
            Some(&Color::Blue)
        );
        assert_eq!(
            updated_game.board[2][1].as_ref().unwrap().get_color(),
            Some(&Color::Blue)
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
            Eterra::create_game(frame_system::RawOrigin::Signed(creator).into(), vec![]);
        assert_noop!(
            result_zero_players,
            crate::Error::<Test>::InvalidNumberOfPlayers
        );

        // Test with one player
        let result_one_player = Eterra::create_game(
            frame_system::RawOrigin::Signed(creator).into(),
            vec![creator],
        );
        assert_noop!(
            result_one_player,
            crate::Error::<Test>::InvalidNumberOfPlayers
        );

        // Test with three players
        let result_three_players = Eterra::create_game(
            frame_system::RawOrigin::Signed(creator).into(),
            vec![creator, opponent, third_player],
        );
        assert_noop!(
            result_three_players,
            crate::Error::<Test>::InvalidNumberOfPlayers
        );

        // Valid two-player game
        let result_two_players = Eterra::create_game(
            frame_system::RawOrigin::Signed(creator).into(),
            vec![creator, opponent],
        );
        assert_ok!(result_two_players);
    });
}
