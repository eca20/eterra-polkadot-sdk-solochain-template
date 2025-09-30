use super::*;
use crate::pallet::Pallet as EterraAi;
use frame_support::{assert_ok, traits::OnInitialize};
use crate::pallet::Config;

struct AdapterShim;

impl crate::GameAdapter for AdapterShim {
    type State = eterra_card_ai_adapter::eterra_adapter::State;
    type Action = eterra_card_ai_adapter::eterra_adapter::Action;
    type Player = u8;

    fn list_actions<const MAX: usize>(
        s: &Self::State,
        out: &mut [Option<Self::Action>; MAX],
    ) -> usize {
        eterra_card_ai_adapter::eterra_adapter::Adapter::list_actions_pure::<MAX>(s, out)
    }

    fn apply(s: &Self::State, a: &Self::Action) -> Self::State {
        eterra_card_ai_adapter::eterra_adapter::Adapter::apply_pure(s, a)
    }

    fn is_terminal(s: &Self::State) -> bool {
        s.round >= s.max_rounds
    }

    fn current_player(s: &Self::State) -> Self::Player {
        s.player_turn
    }

    fn score(s: &Self::State, for_player: Self::Player) -> i32 {
        let (a,b) = s.scores; if for_player == 0 { (a as i32) - (b as i32) } else { (b as i32) - (a as i32) }
    }

    fn random_action(s: &Self::State, seed: u64) -> Option<Self::Action> {
        const MAX: usize = 128;
        let mut buf: [Option<Self::Action>; MAX] = core::array::from_fn(|_| None);
        let n = Self::list_actions::<MAX>(s, &mut buf);
        if n == 0 { return None; }
        let idx = (seed as usize) % n; buf[idx].clone()
    }
}

#[test]
fn nim_ai_picks_optimal_at_high_difficulty() {
    let mut ext = crate::mock::new_test_ext();
    ext.execute_with(|| {
        use crate::mock::{NimState, NimAction, Test};

        let s = NimState { pile: 3, to_move: 0 }; // optimal is Take1 (random reply; Take2 gives opponent forced win)
        let a = EterraAi::<Test>::suggest::<crate::mock::NimAdapter>(&s, 95).expect("action");
        assert_eq!(a, NimAction::Take1);

        // Lower difficulty may still pick optimal, but let's ensure it returns a legal action.
        let a2 = EterraAi::<Test>::suggest::<crate::mock::NimAdapter>(&s, 10).expect("action");
        assert!(a2 == NimAction::Take1 || a2 == NimAction::Take2);
    });
}

#[test]
fn extrinsic_emits_suggested_event() {
    let mut ext = crate::mock::new_test_ext();
    ext.execute_with(|| {
        use crate::mock::{NimState, NimAction, Test};
        let who: u64 = 1;
        let state = NimState { pile: 4, to_move: 0 };
        assert_ok!(
            crate::pallet::Pallet::<Test>::suggest_move(
                frame_system::RawOrigin::Signed(who).into(),
                state.clone(),
                80
            )
        );

        // Check that an event was emitted
        let events = frame_system::Pallet::<Test>::events();
        assert!(events.iter().any(|ev| {
            matches!(
                ev.event,
                crate::mock::RuntimeEvent::EterraAi(
                    crate::pallet::Event::Suggested { .. }
                )
            )
        }));
    });
}

#[test]
fn nim_terminal_has_no_suggestion() {
    let mut ext = crate::mock::new_test_ext();
    ext.execute_with(|| {
        use crate::mock::{NimState, Test};
        let terminal = NimState { pile: 0, to_move: 0 };
        let a = EterraAi::<Test>::suggest::<crate::mock::NimAdapter>(&terminal, 50);
        assert!(a.is_none());
    });
}

#[test]
fn eterra_adapter_ai_returns_legal_move_and_applies() {
    use eterra_card_ai_adapter::eterra_adapter::{Adapter, Hand, HandEntry, State, Board};

    let mut ext = crate::mock::new_test_ext();
    ext.execute_with(|| {
        // Start from an empty 4x4 board and equal scores 5–5, like your game does.
        let board: Board = Default::default();

        // Helper to build a hand entry
        let mk = |n, e, s, w| HandEntry {
            north: n,
            east: e,
            south: s,
            west: w,
            used: false,
        };

        // Build simple hands (5 entries each). Tweak values as needed.
        let hand0 = Hand {
            entries: [mk(5, 4, 5, 4), mk(4, 6, 4, 6), mk(7, 3, 7, 3), mk(4, 4, 4, 4), mk(6, 2, 6, 2)],
        };
        let hand1 = Hand {
            entries: [mk(4, 5, 4, 5), mk(6, 4, 6, 4), mk(3, 7, 3, 7), mk(4, 4, 4, 4), mk(2, 6, 2, 6)],
        };

        let s0 = State {
            board,
            scores: (5, 5),
            player_turn: 0,
            round: 0,
            max_rounds: 10,
            hands: [hand0, hand1],
        };

        // Ask AI for a suggestion at moderate difficulty
        let a = crate::pallet::Pallet::<crate::mock::Test>::suggest::<AdapterShim>(&s0, 60)
            .expect("AI should suggest a legal move");

        // Check basic legality
        assert!(a.x < 4 && a.y < 4, "coords in range");
        assert!(a.hand_index < 5, "hand index in range");

        // Apply one step and validate state consistency
        let s1 = eterra_card_ai_adapter::eterra_adapter::Adapter::apply_pure(&s0, &a);

        // The chosen hand entry should be marked used
        assert!(s1.hands[0].entries[a.hand_index as usize].used);

        // Board cell must be occupied now
        assert!(s1.board[a.x as usize][a.y as usize].is_some());

        // Player turn should advance (0 -> 1), and round should only increment on wrap.
        assert_eq!(s1.player_turn, 1);
        assert_eq!(s1.round, 0);
    });
}

#[test]
fn adapter_list_actions_respects_max_bound() {
    use eterra_card_ai_adapter::eterra_adapter::{Hand, HandEntry, State, Board};

    let mut ext = crate::mock::new_test_ext();
    ext.execute_with(|| {
        let board: Board = Default::default();
        let mk = |n, e, s, w| HandEntry { north: n, east: e, south: s, west: w, used: false };
        let base = mk(1,1,1,1);
        let hand0 = Hand { entries: core::array::from_fn(|_| base.clone()) };
        let hand1 = Hand { entries: core::array::from_fn(|_| base.clone()) };
        let s = State { board, scores: (5,5), player_turn: 0, round: 0, max_rounds: 10, hands: [hand0, hand1] };

        // With an empty 4x4, maximum distinct actions is 16 cells * 5 unused cards = 80.
        // We intentionally set MAX lower to ensure the adapter doesn't write past bounds.
        const MAX: usize = 7; // small cap
        let mut buf: [Option<eterra_card_ai_adapter::eterra_adapter::Action>; MAX] = core::array::from_fn(|_| None);
        let n = AdapterShim::list_actions::<MAX>(&s, &mut buf);
        assert!(n <= MAX, "listed {} actions, exceeds MAX {}", n, MAX);
        // All populated entries should be Some(...)
        for i in 0..n { assert!(buf[i].is_some(), "index {} should be populated", i); }
        // And any remaining slots should be None
        for i in n..MAX { assert!(buf[i].is_none(), "index {} should remain None", i); }
    });
}

#[test]
fn ai_prefers_capture_when_available_high_difficulty() {
    use eterra_card_ai_adapter::eterra_adapter::{Adapter, Hand, HandEntry, State, Board, Card as ACard, Possession as Possession};

    let mut ext = crate::mock::new_test_ext();
    ext.execute_with(|| {
        let mut board: Board = Default::default();
        // Place an opponent card at (1,1) with weak side facing left (so we can capture from (0,1))
        let opp_card = ACard { top: 3, right: 3, bottom: 3, left: 2, possession: Some(eterra_card_ai_adapter::eterra_adapter::Possession::PlayerTwo) };
        board[1][1] = Some(opp_card);

        // Our hand has one strong-right card: right=5 beats opp.left(2) when we place at (0,1)
        let strong_left = HandEntry { north: 1, east: 5, south: 1, west: 1, used: false };
        // Fill remaining entries with dummies
        let dummy = HandEntry { north: 1, east: 1, south: 1, west: 1, used: false };
        let hand0 = Hand { entries: [
            strong_left,
            dummy.clone(),
            dummy.clone(),
            dummy.clone(),
            dummy.clone(),
        ] };
        let hand1 = Hand { entries: core::array::from_fn(|_| dummy.clone()) };

        let s0 = State { board, scores: (5,5), player_turn: 0, round: 0, max_rounds: 10, hands: [hand0, hand1] };

        // Suggest at high difficulty – should favor the capturing move at x=0,y=1 using hand_index=0
        let a = crate::pallet::Pallet::<crate::mock::Test>::suggest::<AdapterShim>(&s0, 95)
            .expect("AI should suggest a move");

        // Apply the suggestion using pure helper so we avoid trait mismatch issues
        let s1 = Adapter::apply_pure(&s0, &a);

        // After a correct capture from (0,1), our score should increase (6,4) and enemy card at (1,1) flips to Blue
        assert!(s1.scores.0 >= s0.scores.0, "our score should not decrease after capturing opportunity");
        if let Some(c) = s1.board[1][1].clone() { assert_eq!(c.possession, Some(eterra_card_ai_adapter::eterra_adapter::Possession::PlayerOne)); }
    });
}