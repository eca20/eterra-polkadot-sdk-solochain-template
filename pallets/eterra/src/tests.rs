use crate::{mock::*, Card};
use frame_support::assert_ok;
use sp_runtime::traits::BlakeTwo256;
use sp_runtime::traits::Hash;

#[test]
fn create_game_works() {
    new_test_ext().execute_with(|| {
        let creator = 1;
        let opponent = 2;

        let game_id = <BlakeTwo256 as Hash>::hash_of(&(creator, opponent));        assert_ok!(Eterra::create_game(
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