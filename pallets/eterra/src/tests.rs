use crate::{mock::*, Card};
use frame_support::{assert_ok, assert_noop};
use sp_runtime::traits::{BlakeTwo256, Hash};
use std::sync::Once;
use log::{Record, Level, Metadata};

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


#[test]
fn create_game_works() {
      init_logger(); // Initialize custom logger
    new_test_ext().execute_with(|| {
        let creator = 1;
        let opponent = 2;

        let game_id = <BlakeTwo256 as Hash>::hash_of(&(creator, opponent));
        assert_ok!(Eterra::create_game(
            frame_system::RawOrigin::Signed(creator).into(),
            opponent
        ));

        let (board, game_creator, game_opponent) = Eterra::game_board(game_id).unwrap();
        let current_turn = Eterra::current_turn(game_id).unwrap();

        assert_eq!(game_creator, creator);
        assert_eq!(game_opponent, opponent);
        assert!(current_turn == creator || current_turn == opponent);
    });
}

#[test]
fn play_turn_works() {
    init_logger(); // Initialize custom logger

    new_test_ext().execute_with(|| {
        let creator = 1;
        let opponent = 2;

        let game_id = BlakeTwo256::hash_of(&(creator, opponent));
        assert_ok!(Eterra::create_game(
            frame_system::RawOrigin::Signed(creator).into(),
            opponent
        ));

        log::debug!(
            "Game created. Current turn: {:?}",
            Eterra::current_turn(game_id).unwrap()
        );

        let card = Card::new(5, 3, 2, 4);
        assert_ok!(Eterra::play_turn(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            1,
            1,
            card.clone()
        ));

        log::debug!(
            "After creator's turn, current turn: {:?}",
            Eterra::current_turn(game_id).unwrap()
        );

        let (board, _, _) = Eterra::game_board(game_id).unwrap();
        log::debug!("Board state after creator's turn: {:?}", board);
        assert_eq!(board[1][1], Some(card.clone()));

        let opponent_card = Card::new(2, 4, 5, 3);
        assert_ok!(Eterra::play_turn(
            frame_system::RawOrigin::Signed(opponent).into(),
            game_id,
            1,
            2,
            opponent_card.clone()
        ));

        log::debug!(
            "After opponent's turn, current turn: {:?}",
            Eterra::current_turn(game_id).unwrap()
        );

        let (updated_board, _, _) = Eterra::game_board(game_id).unwrap();
        assert_eq!(updated_board[1][2], Some(opponent_card));
    });
}

#[test]
fn create_game_with_same_players_fails() {
      init_logger(); // Initialize custom logger

    new_test_ext().execute_with(|| {
        let player = 1;

        let result = Eterra::create_game(
            frame_system::RawOrigin::Signed(player).into(),
            player,
        );

        assert_noop!(result, crate::Error::<Test>::InvalidMove);
    });
}

#[test]
fn play_turn_out_of_bounds_fails() {
      init_logger(); // Initialize custom logger

    new_test_ext().execute_with(|| {
        let creator = 1;
        let opponent = 2;

        let game_id = BlakeTwo256::hash_of(&(creator, opponent));
        assert_ok!(Eterra::create_game(
            frame_system::RawOrigin::Signed(creator).into(),
            opponent
        ));

        let card = Card::new(5, 3, 2, 4);

        // Attempt to play a turn outside the 4x4 board
        let result = Eterra::play_turn(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            5, // Invalid x coordinate
            1,
            card,
        );

        assert_noop!(result, crate::Error::<Test>::InvalidMove);
    });
}

#[test]
fn full_game_simulation() {
      init_logger(); // Initialize custom logger

    new_test_ext().execute_with(|| {
        let creator = 1;
        let opponent = 2;

        let game_id = BlakeTwo256::hash_of(&(creator, opponent));
        assert_ok!(Eterra::create_game(
            frame_system::RawOrigin::Signed(creator).into(),
            opponent
        ));

        // Play exactly 10 moves (5 for each player)
        let moves = vec![
            (creator, 0, 0, Card::new(5, 3, 2, 4)),
            (opponent, 0, 1, Card::new(2, 4, 5, 3)),
            (creator, 0, 2, Card::new(4, 3, 1, 2)),
            (opponent, 0, 3, Card::new(3, 5, 2, 4)),
            (creator, 1, 0, Card::new(5, 3, 2, 1)),
            (opponent, 1, 1, Card::new(2, 4, 5, 3)),
            (creator, 1, 2, Card::new(4, 3, 1, 2)),
            (opponent, 1, 3, Card::new(3, 5, 2, 4)),
            (creator, 2, 0, Card::new(5, 3, 2, 4)),
            (opponent, 2, 1, Card::new(2, 4, 5, 3)),
        ];

        for (player, x, y, card) in moves {
            assert_ok!(Eterra::play_turn(
                frame_system::RawOrigin::Signed(player).into(),
                game_id,
                x,
                y,
                card
            ));
        }

        // Ensure the game has ended
        assert!(Eterra::game_board(game_id).is_none());
        assert!(Eterra::moves_played(game_id).is_none());
        log::debug!("Game simulation completed successfully.");
    });
}

#[test]
fn invalid_move_on_occupied_cell() {
    init_logger(); // Initialize custom logger

    new_test_ext().execute_with(|| {
        let creator = 1;
        let opponent = 2;

        let game_id = BlakeTwo256::hash_of(&(creator, opponent));
        assert_ok!(Eterra::create_game(
            frame_system::RawOrigin::Signed(creator).into(),
            opponent
        ));

        let card = Card::new(5, 3, 2, 4);
        assert_ok!(Eterra::play_turn(
            frame_system::RawOrigin::Signed(creator).into(),
            game_id,
            1,
            1,
            card.clone() // Clone the card for reuse
        ));

        // Attempt to play on the same cell
        let result = Eterra::play_turn(
            frame_system::RawOrigin::Signed(opponent).into(),
            game_id,
            1,
            1,
            card, // Use the original card here
        );

        assert_noop!(result, crate::Error::<Test>::CellOccupied);
    });
}