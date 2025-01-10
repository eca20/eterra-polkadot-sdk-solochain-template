use crate::{mock::*, Card};
use frame_support::{assert_ok, assert_noop};
use sp_runtime::traits::{BlakeTwo256, Hash};

#[test]
fn create_game_works() {
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
    init_simple_logger().expect("Failed to initialize logger");

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
    new_test_ext().execute_with(|| {
        let creator = 1;
        let opponent = 2;

        let game_id = BlakeTwo256::hash_of(&(creator, opponent));
        assert_ok!(Eterra::create_game(
            frame_system::RawOrigin::Signed(creator).into(),
            opponent
        ));

        // Play 10 moves alternately
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