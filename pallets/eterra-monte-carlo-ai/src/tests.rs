use super::*;
use crate::pallet::Pallet as EterraAi;
use frame_support::{assert_ok, traits::OnInitialize};
use crate::pallet::Config;

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
