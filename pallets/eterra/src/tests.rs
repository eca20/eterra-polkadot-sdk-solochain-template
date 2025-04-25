use crate::mock::RuntimeEvent;
use crate::types::game::GameProperties; // Import the GameProperties trait
use crate::Color;
use crate::GameId;
use crate::GameStorage;
use crate::Move;
use crate::{mock::*, types::card::Card};
use frame_support::traits::Get;
use frame_support::traits::Hooks;
use frame_support::{assert_noop, assert_ok};
use frame_system::pallet_prelude::BlockNumberFor;
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

/// Utility function that advances the current block number by `n` blocks.
fn run_to_block(n: u64) {
    while System::block_number() < n {
        System::set_block_number(System::block_number() + 1);
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
            crate::Error::<Test>::CreatorMustBeInGame
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
        let creator_card = Card::new(5, 3, 2, 4).with_color(Color::Blue);
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
        let opponent_card = Card::new(2, 4, 5, 3).with_color(Color::Red);
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
fn test_force_idle_turns() {
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();
        let blocks_to_wait = <Test as crate::Config>::BlocksToPlayLimit::get() as u64 * 2;

        // Ensure the game exists in storage
        assert!(GameStorage::<Test>::contains_key(&game_id));

        let game = GameStorage::<Test>::get(&game_id).unwrap();
        let initial_player = game.players[game.player_turn as usize];

        // Advance the chain by blocks less than the threshold and check no force finish
        System::set_block_number(System::block_number() + blocks_to_wait - 1);
        Eterra::on_finalize(System::block_number());

        let game = GameStorage::<Test>::get(&game_id).unwrap();
        let expected_turn = game.players[game.player_turn as usize];
        assert_eq!(
            expected_turn, initial_player,
            "Turn should not be forced yet"
        );

        // Advance the chain past the threshold
        System::set_block_number(System::block_number() + 2);
        Eterra::on_finalize(System::block_number());

        // Fetch the updated game state
        let updated_game = GameStorage::<Test>::get(&game_id).unwrap();

        // Check if the turn was forced
        assert_ne!(
            updated_game.players[updated_game.player_turn as usize], initial_player,
            "Turn should have been forced"
        );

        // Check emitted events
        let events = System::events();
        assert!(events.iter().any(|r| matches!(
            r.event,
            RuntimeEvent::Eterra(crate::Event::TurnForceFinished {
                game_id: _,
                player: _
            })
        )));
        assert!(events.iter().any(|r| matches!(
            r.event,
            RuntimeEvent::Eterra(crate::Event::NewTurn {
                game_id: _,
                next_player: _
            })
        )));
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
