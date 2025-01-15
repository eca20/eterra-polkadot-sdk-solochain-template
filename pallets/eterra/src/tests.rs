use crate::Color;
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

    let game_id = BlakeTwo256::hash_of(&(creator, opponent));
    assert_ok!(Eterra::create_game(
        frame_system::RawOrigin::Signed(creator).into(),
        vec![creator, opponent], // Pass creator and opponent as a Vec
    ));

    log::debug!(
        "Game created with ID: {:?}, Creator: {}, Opponent: {}",
        game_id,
        creator,
        opponent
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
        assert_ok!(Eterra::play_turn(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            1,
            1,
            card.clone()
        ));

        // Attempt to play on the same cell
        let result = Eterra::play_turn(
            frame_system::RawOrigin::Signed(opponent).into(),
            game_id,
            1,
            1,
            card,
        );

        assert_noop!(result, crate::Error::<Test>::CellOccupied);
    });
}

#[test]
fn create_game_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();

        let (board, game_creator, game_opponent) = Eterra::game_board(game_id).unwrap();
        let current_turn = Eterra::current_turn(game_id).unwrap();

        assert_eq!(game_creator, creator);
        assert_eq!(game_opponent, opponent);
        assert!(current_turn == creator || current_turn == opponent);
        assert!(board.iter().flatten().all(|cell| cell.is_none())); // Verify empty board
    });
}

#[test]
fn play_turn_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();

        let card = Card::new(5, 3, 2, 4).with_color(Color::Blue); // Direct `Color`
        assert_ok!(Eterra::play_turn(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            1,
            1,
            card.clone()
        ));

        let (board, _, _) = Eterra::game_board(game_id).unwrap();
        assert_eq!(board[1][1], Some(card));

        let opponent_card = Card::new(2, 4, 5, 3).with_color(Color::Red); // Direct `Color`
        assert_ok!(Eterra::play_turn(
            frame_system::RawOrigin::Signed(opponent).into(),
            game_id,
            1,
            2,
            opponent_card.clone()
        ));

        let (updated_board, _, _) = Eterra::game_board(game_id).unwrap();
        assert_eq!(updated_board[1][2], Some(opponent_card));
    });
}

#[test]
fn card_capture_multiple_directions() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();

        // Creator's first move
        let creator_card = Card::new(3, 5, 2, 1).with_color(Color::Blue); // Direct `Color`
        assert_ok!(Eterra::play_turn(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            0,
            0,
            creator_card.clone()
        ));

        let opponent_card = Card::new(2, 4, 5, 3).with_color(Color::Red); // Direct `Color`
        assert_ok!(Eterra::play_turn(
            frame_system::RawOrigin::Signed(opponent).into(),
            game_id,
            0,
            1,
            opponent_card.clone()
        ));

        let capturing_card = Card::new(6, 6, 3, 3).with_color(Color::Blue); // Direct `Color`
        assert_ok!(Eterra::play_turn(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            0,
            2,
            capturing_card.clone()
        ));

        let (board, _, _) = Eterra::game_board(game_id).unwrap();
        assert_eq!(
            board[0][1].as_ref().unwrap().get_color(),
            Some(&Color::Blue)
        );
        assert_eq!(
            board[0][2].as_ref().unwrap().get_color(),
            Some(&Color::Blue)
        ); // Captured card
    });
}

#[test]
fn play_turn_out_of_bounds_fails() {
    init_logger();
    new_test_ext().execute_with(|| {
        let (game_id, creator, _) = setup_new_game();

        let card = Card::new(5, 3, 2, 4);
        let result = Eterra::play_turn(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            5, // Invalid X coordinate
            1, // Valid Y coordinate
            card,
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
            (creator, 0, 0, Card::new(5, 3, 2, 4)),
            (opponent, 0, 1, Card::new(2, 4, 5, 3)),
            (creator, 1, 0, Card::new(4, 3, 1, 6)),
            (opponent, 1, 1, Card::new(3, 5, 2, 4)),
            (creator, 2, 0, Card::new(5, 3, 2, 4)),
            (opponent, 2, 1, Card::new(2, 4, 5, 3)),
            (creator, 3, 0, Card::new(4, 3, 1, 6)),
            (opponent, 3, 1, Card::new(3, 5, 2, 4)),
            (creator, 0, 2, Card::new(5, 3, 2, 4)),
            (opponent, 1, 2, Card::new(2, 4, 5, 3)),
        ];

        for (player, x, y, card) in &moves {
            log::debug!(
                "Player {} is attempting to play at ({}, {}) with card: {:?}",
                player,
                x,
                y,
                card
            );
            assert_ok!(Eterra::play_turn(
                frame_system::RawOrigin::Signed(*player).into(),
                game_id,
                *x,
                *y,
                card.clone()
            ));
        }

        // Assert that GameFinished event is emitted
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

        // First turn: the creator plays a card
        let card = Card::new(5, 3, 2, 4);
        assert_ok!(Eterra::play_turn(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            1,
            1,
            card.clone()
        ));

        // Second turn: the creator attempts to play again (out of turn)
        let another_card = Card::new(3, 4, 1, 2);
        let result = Eterra::play_turn(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            1,
            2,
            another_card,
        );

        // Assert that the play fails with `NotYourTurn`
        assert_noop!(result, crate::Error::<Test>::NotYourTurn);

        // Confirm the opponent can play their turn
        let opponent_card = Card::new(2, 4, 5, 3);
        assert_ok!(Eterra::play_turn(
            frame_system::RawOrigin::Signed(opponent).into(),
            game_id,
            1,
            2,
            opponent_card
        ));

        log::debug!("Test completed: A player cannot play out of turn.");
    });
}

#[test]
fn capture_cards_in_all_directions() {
    new_test_ext().execute_with(|| {
        let (game_id, creator, opponent) = setup_new_game();

        // Determine the first player based on the current turn from the pallet
        let mut current_player = Eterra::current_turn(game_id).unwrap();
        let mut other_player = if current_player == creator {
            opponent
        } else {
            creator
        };

        // Place opponent cards in cardinal directions
        let opponent_cards = vec![
            (0, 1, Card::new(2, 4, 5, 3)), // Top
            (1, 0, Card::new(3, 5, 2, 4)), // Left
            (1, 2, Card::new(3, 5, 2, 4)), // Right
            (2, 1, Card::new(2, 4, 5, 3)), // Bottom
        ];

        for (x, y, card) in opponent_cards {
            // Ensure the current player matches the expected player
            assert_eq!(Eterra::current_turn(game_id).unwrap(), current_player);

            // Play the turn
            assert_ok!(Eterra::play_turn(
                frame_system::RawOrigin::Signed(current_player).into(),
                game_id,
                x,
                y,
                card.with_color(Color::Red)
            ));

            // Alternate the players for the next move
            std::mem::swap(&mut current_player, &mut other_player);
        }

        // Place capturing card in the center
        let capturing_card = Card::new(6, 6, 6, 6).with_color(Color::Blue);

        // Ensure it's the creator's turn (or the current player determined by the pallet)
        assert_eq!(Eterra::current_turn(game_id).unwrap(), creator);
        assert_ok!(Eterra::play_turn(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            1,
            1,
            capturing_card
        ));

        // Validate board state
        let (board, _, _) = Eterra::game_board(game_id).unwrap();
        assert_eq!(
            board[1][1].as_ref().unwrap().get_color(),
            Some(&Color::Blue)
        );
        assert_eq!(
            board[0][1].as_ref().unwrap().get_color(),
            Some(&Color::Blue)
        );
        assert_eq!(
            board[1][0].as_ref().unwrap().get_color(),
            Some(&Color::Blue)
        );
        assert_eq!(
            board[1][2].as_ref().unwrap().get_color(),
            Some(&Color::Blue)
        );
        assert_eq!(
            board[2][1].as_ref().unwrap().get_color(),
            Some(&Color::Blue)
        );
    });
}


#[test]
fn invalid_game_id_fails() {
    new_test_ext().execute_with(|| {
        // Generate a random game ID that does not exist
        let invalid_game_id = H256::random();

        // Attempt to play a turn with the invalid game ID
        let card = Card::new(5, 3, 2, 4);
        let result = Eterra::play_turn(
            frame_system::RawOrigin::Signed(1).into(),
            invalid_game_id,
            0,
            0,
            card.clone(),
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
