#![cfg(test)]

use crate::mock::MaxWeightEntries;
use crate::mock::RuntimeEvent;
use crate::mock::*;
use crate::ReelWeights;
use crate::RollsThisBlock;
use crate::RollsThisWindow;
use crate::{
    Config, Error, Event, LastDrawingTime, LastRollTime, Pallet, RollHistory, TicketsPerUser,
    TotalTickets,
};
use frame_support::traits::Hooks;
use frame_support::BoundedVec;
use frame_support::{assert_noop, assert_ok};
use frame_system::pallet_prelude::BlockNumberFor;
use frame_system::RawOrigin;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::collections::HashMap; // Optional: use fixed seed for deterministic tests

// ─── Helpers ────────────────────────────────────────────────────────────────

fn set_mock_time_to_sunday_6pm() {
    MockTimeState::set_now(324_000);
}

fn roll_n_times<T: crate::pallet::Config>(who: &T::AccountId, n: u32) {
    for _ in 0..n {
        assert_ok!(crate::Pallet::<T>::roll(
            frame_system::RawOrigin::Signed(who.clone()).into()
        ));
    }
}

// 6h window at 6s/block → 3_600 blocks
const BLOCKS_PER_WINDOW: u64 = 3_600;

fn advance_blocks(n: u64) {
    let b: u64 = frame_system::Pallet::<TestRuntime>::block_number();
    frame_system::Pallet::<TestRuntime>::set_block_number(b + n);
}

// ─── Basic Slot Roll Tests ─────────────────────────────────────────────────

#[test]
fn test_roll_succeeds_with_valid_config() {
    new_test_ext().execute_with(|| {
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
        // our MockTime starts at 90_000
        assert_eq!(LastRollTime::<TestRuntime>::get(1), 90_000);
    });
}

/// You can roll up to 3× per ~6h window, so the second and third rolls still succeed.
#[test]
fn test_second_and_third_roll_succeed() {
    new_test_ext().execute_with(|| {
        // first roll
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
        // second roll must also succeed
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
        // third roll still under the 3-roll limit
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
    });
}

#[test]
fn test_exceed_rolls_per_window() {
    new_test_ext().execute_with(|| {
        // allow three rolls within the same ~6h window
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
        // fourth roll in the same window must now fail
        let fourth = Pallet::<TestRuntime>::roll(frame_system::RawOrigin::Signed(1).into());
        assert_noop!(fourth, Error::<TestRuntime>::ExceedRollsPerRound);
    });
}

#[test]
fn test_roll_succeeds_after_new_window() {
    new_test_ext().execute_with(|| {
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
        // Advance by ~6h worth of blocks (6s block time ⇒ 3600 blocks)
        let current = frame_system::Pallet::<TestRuntime>::block_number();
        frame_system::Pallet::<TestRuntime>::set_block_number(current + 3_600u64);
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
    });
}

// ─── Edge Case: independent accounts ─────────────────────────────────────────

#[test]
fn test_different_accounts_can_roll_independently() {
    new_test_ext().execute_with(|| {
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(2).into()
        ));
    });
}

// ─── Slot Event Tests ──────────────────────────────────────────────────────

#[test]
fn test_slot_rolled_event_emitted() {
    new_test_ext().execute_with(|| {
        frame_system::Pallet::<TestRuntime>::reset_events();
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));

        let evts = frame_system::Pallet::<TestRuntime>::events();
        assert_eq!(evts.len(), 1);
        match &evts[0].event {
            RuntimeEvent::EterraDailySlots(Event::SlotRolled { player, result }) => {
                assert_eq!(*player, 1);
                assert_eq!(result.len(), 3);
            }
            _ => panic!("unexpected event"),
        }
    });
}

#[test]
fn test_slot_rolled_event_emitted_correctly() {
    new_test_ext().execute_with(|| {
        LastRollTime::<TestRuntime>::insert(1, 0);
        frame_system::Pallet::<TestRuntime>::reset_events();
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));

        let found = frame_system::Pallet::<TestRuntime>::events()
            .iter()
            .any(|r| {
                matches!(
                    r.event,
                    RuntimeEvent::EterraDailySlots(Event::SlotRolled { .. })
                )
            });
        assert!(found, "SlotRolled should have been emitted");
    });
}

// ─── Ticket Awarding ────────────────────────────────────────────────────────

#[test]
fn test_ticket_awarded_on_special_symbol() {
    new_test_ext().execute_with(|| {
        LastRollTime::<TestRuntime>::insert(1, 0);
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));
        assert_eq!(TicketsPerUser::<TestRuntime>::get(1), 0);
    });
}

// ─── Weekly Drawing Tests ──────────────────────────────────────────────────

#[test]
fn test_no_weekly_drawing_if_not_sunday_6pm() {
    new_test_ext().execute_with(|| {
        TicketsPerUser::<TestRuntime>::insert(1, 5);
        TotalTickets::<TestRuntime>::put(5);
        LastDrawingTime::<TestRuntime>::put(89_500);

        Pallet::<TestRuntime>::on_initialize(1);

        assert_eq!(TotalTickets::<TestRuntime>::get(), 5);
        let fired = frame_system::Pallet::<TestRuntime>::events()
            .iter()
            .any(|r| {
                matches!(
                    r.event,
                    RuntimeEvent::EterraDailySlots(Event::WeeklyWinner { .. })
                )
            });
        assert!(!fired);
    });
}

#[test]
fn test_no_weekly_drawing_with_no_tickets() {
    new_test_ext().execute_with(|| {
        set_mock_time_to_sunday_6pm();
        TotalTickets::<TestRuntime>::put(0);
        LastDrawingTime::<TestRuntime>::put(0);
        frame_system::Pallet::<TestRuntime>::set_block_number(1001);
        frame_system::Pallet::<TestRuntime>::reset_events();

        Pallet::<TestRuntime>::on_initialize(1001);

        assert_eq!(TotalTickets::<TestRuntime>::get(), 0);
        let fired = frame_system::Pallet::<TestRuntime>::events()
            .iter()
            .any(|r| {
                matches!(
                    r.event,
                    RuntimeEvent::EterraDailySlots(Event::WeeklyWinner { .. })
                )
            });
        assert!(!fired);
    });
}

#[test]
fn test_weekly_drawing_selects_winner() {
    new_test_ext().execute_with(|| {
        set_mock_time_to_sunday_6pm();
        TicketsPerUser::<TestRuntime>::insert(1, 5);
        TotalTickets::<TestRuntime>::put(5);
        LastDrawingTime::<TestRuntime>::put(0);
        frame_system::Pallet::<TestRuntime>::set_block_number(1001);
        frame_system::Pallet::<TestRuntime>::reset_events();

        Pallet::<TestRuntime>::on_initialize(1001);

        assert_eq!(TotalTickets::<TestRuntime>::get(), 0);
        let fired = frame_system::Pallet::<TestRuntime>::events()
            .iter()
            .any(|r| {
                matches!(
                    r.event,
                    RuntimeEvent::EterraDailySlots(Event::WeeklyWinner { .. })
                )
            });
        assert!(fired);
    });
}

#[test]
fn test_weekly_drawing_only_once_per_week() {
    new_test_ext().execute_with(|| {
        set_mock_time_to_sunday_6pm();
        TicketsPerUser::<TestRuntime>::insert(1, 5);
        TotalTickets::<TestRuntime>::put(5);
        LastDrawingTime::<TestRuntime>::put(0);
        frame_system::Pallet::<TestRuntime>::set_block_number(1001);
        frame_system::Pallet::<TestRuntime>::reset_events();

        Pallet::<TestRuntime>::on_initialize(1001);
        Pallet::<TestRuntime>::on_initialize(1002);

        let count = frame_system::Pallet::<TestRuntime>::events()
            .iter()
            .filter(|r| {
                matches!(
                    r.event,
                    RuntimeEvent::EterraDailySlots(Event::WeeklyWinner { .. })
                )
            })
            .count();
        assert_eq!(count, 1);
    });
}

#[test]
fn test_weekly_winner_event_emitted_correctly() {
    new_test_ext().execute_with(|| {
        set_mock_time_to_sunday_6pm();
        TicketsPerUser::<TestRuntime>::insert(1, 5);
        TotalTickets::<TestRuntime>::put(5);
        LastDrawingTime::<TestRuntime>::put(0);
        frame_system::Pallet::<TestRuntime>::set_block_number(1001);
        frame_system::Pallet::<TestRuntime>::reset_events();

        Pallet::<TestRuntime>::on_initialize(1001);

        let found = frame_system::Pallet::<TestRuntime>::events()
            .iter()
            .any(|r| {
                matches!(
                    r.event,
                    RuntimeEvent::EterraDailySlots(Event::WeeklyWinner { .. })
                )
            });
        assert!(found);
    });
}

#[test]
fn roll_creates_history_entry() {
    new_test_ext().execute_with(|| {
        let user = 1u64; // Assume u64 AccountId in mock.rs
        assert_eq!(RollHistory::<Test>::get(user).len(), 0);

        assert_ok!(Pallet::<Test>::roll(RawOrigin::Signed(user).into()));
        let history = RollHistory::<Test>::get(user);
        assert_eq!(history.len(), 1);
        let entry = &history[0];

        assert!(entry.timestamp > 0);
        assert!(!entry.result.is_empty());
    });
}

#[test]
fn roll_history_respects_max_length() {
    new_test_ext().execute_with(|| {
        let user = 1u64;

        let max_len = <Test as Config>::MaxRollHistoryLength::get();
        let roll_limit = <Test as Config>::MaxRollsPerRound::get();

        // Roll up to allowed limit
        roll_n_times::<Test>(&user, roll_limit);

        let history = RollHistory::<Test>::get(user);
        assert!(history.len() as u32 <= roll_limit);
        assert!(history.len() as u32 <= max_len);
    });
}

#[test]
fn test_set_reel_weights_and_roll_with_weights() {
    new_test_ext().execute_with(|| {
        let weights = vec![(1, 10), (2, 0), (3, 0)];
        assert_ok!(Pallet::<Test>::set_reel_weights(
            RawOrigin::Root.into(),
            0,
            weights.clone()
        ));
        let expected: BoundedVec<_, MaxWeightEntries> = weights.try_into().unwrap();
        assert_eq!(crate::ReelWeights::<Test>::get(0).unwrap(), expected);
        // Perform a roll and ensure the result includes the weighted symbol
        assert_ok!(Pallet::<Test>::roll(RawOrigin::Signed(1).into()));
        let history = RollHistory::<Test>::get(1);
        assert!(!history.is_empty());
    });
}

#[test]
fn test_outcomes_follow_weights_distribution() {
    new_test_ext().execute_with(|| {
        let total_rolls = 1000;
        let rolls_per_user = <Test as Config>::MaxRollsPerRound::get();
        let num_users = (total_rolls + rolls_per_user - 1) / rolls_per_user;

        // Set known weights for each reel
        for reel in 0..<Test as Config>::MaxSlotLength::get() {
            let weights = vec![(0, 5), (1, 3), (2, 2)];
            let bounded: BoundedVec<_, MaxWeightEntries> = weights.try_into().unwrap();
            ReelWeights::<Test>::insert(reel, bounded);
        }

        // Outcome counter
        let mut counter: HashMap<u32, u32> = HashMap::new();
        let mut total_performed = 0;

        for user_id in 1..=num_users {
            let user = user_id as u64;

            for _ in 0..rolls_per_user {
                if total_performed >= total_rolls {
                    break;
                }

                // Clear per-block dedupe, per-window counter, and reset timestamp/history for this synthetic roll
                for slot in 0..<Test as Config>::MaxSlotLength::get() {
                    RollsThisBlock::<Test>::remove(user, slot as u64);
                }
                RollsThisWindow::<Test>::remove(user);
                LastRollTime::<Test>::insert(user, 0);
                RollHistory::<Test>::remove(user);

                // advance block to simulate time passing between rolls
                let b = frame_system::Pallet::<Test>::block_number();
                frame_system::Pallet::<Test>::set_block_number(b + 1);
                assert_ok!(Pallet::<Test>::roll(RawOrigin::Signed(user).into()));

                let result = RollHistory::<Test>::get(user)
                    .last()
                    .unwrap()
                    .result
                    .clone();

                for symbol in result {
                    *counter.entry(symbol).or_insert(0) += 1;
                }

                total_performed += 1;
            }
        }

        // Print symbol counts (optional for debugging)
        for (symbol, count) in &counter {
            println!("Symbol {}: {}", symbol, count);
        }

        // Validate the observed distribution against expected weights
        let total_symbols = (total_rolls * <Test as Config>::MaxSlotLength::get()) as u32;
        let tolerance = 0.10;

        let expected_weights = vec![(0, 5), (1, 3), (2, 2)];
        let total_weight: u32 = expected_weights.iter().map(|(_, w)| w).sum();

        for (symbol, weight) in expected_weights {
            let expected = (weight * total_symbols) / total_weight;
            let actual = counter.get(&symbol).cloned().unwrap_or(0);

            let lower = (expected as f32 * (1.0 - tolerance)) as u32;
            let upper = (expected as f32 * (1.0 + tolerance)) as u32;

            assert!(
                (lower..=upper).contains(&actual),
                "Symbol {} out of expected range: {} not in {}..={}",
                symbol,
                actual,
                lower,
                upper
            );
        }
    });
}

#[test]
fn test_roll_fails_without_weights_set() {
    new_test_ext().execute_with(|| {
        // Remove weights for reel 0 to simulate missing config
        crate::ReelWeights::<Test>::remove(0);
        let result = Pallet::<Test>::roll(RawOrigin::Signed(1).into());
        assert_noop!(result, Error::<Test>::InvalidConfiguration);
    });
}

#[test]
fn test_set_all_reel_weights_successfully_sets_all() {
    new_test_ext().execute_with(|| {
        let weights = vec![(7, 10)];
        let all: Vec<_> = (0..<Test as Config>::MaxSlotLength::get())
            .map(|i| (i, weights.clone()))
            .collect();

        assert_ok!(Pallet::<Test>::set_all_reel_weights(
            RawOrigin::Root.into(),
            all.clone()
        ));

        for (reel, w) in all {
            let stored = ReelWeights::<Test>::get(reel).unwrap();
            let expected: BoundedVec<_, MaxWeightEntries> = w.try_into().unwrap();
            assert_eq!(stored, expected);
        }
    });
}

#[test]
fn test_set_empty_reel_weights_fails() {
    new_test_ext().execute_with(|| {
        let result = Pallet::<Test>::set_reel_weights(RawOrigin::Root.into(), 0, vec![]);
        assert_noop!(result, Error::<Test>::InvalidConfiguration);
    });
}

#[test]
fn test_set_reel_weights_exceeds_max_entries() {
    new_test_ext().execute_with(|| {
        let too_many_weights: Vec<(u32, u32)> = (0..(<Test as Config>::MaxWeightEntries::get()
            + 1))
            .map(|i| (i, 1))
            .collect();

        let result = Pallet::<Test>::set_reel_weights(RawOrigin::Root.into(), 0, too_many_weights);
        assert_noop!(result, Error::<Test>::InvalidConfiguration);
    });
}

#[test]
fn test_ticket_counter_does_not_overflow() {
    new_test_ext().execute_with(|| {
        TicketsPerUser::<Test>::insert(1, u32::MAX - 1);
        TotalTickets::<Test>::put(u32::MAX - 1);

        for reel in 0..<Test as Config>::MaxSlotLength::get() {
            let weights = vec![(7, 10)];
            let bounded: BoundedVec<_, MaxWeightEntries> = weights.try_into().unwrap();
            ReelWeights::<Test>::insert(reel, bounded);
        }

        assert_ok!(Pallet::<Test>::roll(RawOrigin::Signed(1).into()));
        assert_eq!(TicketsPerUser::<Test>::get(1), u32::MAX);
        assert_eq!(TotalTickets::<Test>::get(), u32::MAX);
    });
}

#[test]
fn test_fourth_roll_fails_in_same_window() {
    new_test_ext().execute_with(|| {
        // Start at a known block so window math is deterministic
        frame_system::Pallet::<TestRuntime>::set_block_number(1);

        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into()));
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into()));
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into()));

        // Still the same window → fourth must fail
        let fourth = Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into());
        assert_noop!(fourth, Error::<TestRuntime>::ExceedRollsPerRound);
    });
}

#[test]
fn test_roll_succeeds_exactly_at_window_boundary() {
    new_test_ext().execute_with(|| {
        frame_system::Pallet::<TestRuntime>::set_block_number(1);

        // Exhaust 3 rolls in current window
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into()));
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into()));
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into()));

        // Advance to the *last block* of the *current* window (still should fail)
        let b = frame_system::Pallet::<TestRuntime>::block_number();
        let window_start = b - (b % BLOCKS_PER_WINDOW);
        let last_in_window = window_start + (BLOCKS_PER_WINDOW - 1); // inclusive last block of this window
        frame_system::Pallet::<TestRuntime>::set_block_number(last_in_window);

        let should_fail = Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into());
        assert_noop!(should_fail, Error::<TestRuntime>::ExceedRollsPerRound);

        // Cross the boundary by 1 block → new window → should succeed
        frame_system::Pallet::<TestRuntime>::set_block_number(last_in_window + 1);
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into()));
    });
}

#[test]
fn test_rolls_reset_after_multiple_windows() {
    new_test_ext().execute_with(|| {
        frame_system::Pallet::<TestRuntime>::set_block_number(10);

        // Use up the window allowance
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into()));
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into()));
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into()));
        let fourth = Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into());
        assert_noop!(fourth, Error::<TestRuntime>::ExceedRollsPerRound);

        // Jump two full windows ahead
        let b = frame_system::Pallet::<TestRuntime>::block_number();
        frame_system::Pallet::<TestRuntime>::set_block_number(b + 2 * BLOCKS_PER_WINDOW);

        // Allowance should be fresh again
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into()));
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into()));
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into()));
    });
}

#[test]
fn test_window_isolated_per_account() {
    new_test_ext().execute_with(|| {
        frame_system::Pallet::<TestRuntime>::set_block_number(5);

        // Account 1 uses all rolls
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into()));
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into()));
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into()));
        let fail1 = Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into());
        assert_noop!(fail1, Error::<TestRuntime>::ExceedRollsPerRound);

        // Account 2 is unaffected in same window
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(2).into()));
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(2).into()));
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(2).into()));
        let fail2 = Pallet::<TestRuntime>::roll(RawOrigin::Signed(2).into());
        assert_noop!(fail2, Error::<TestRuntime>::ExceedRollsPerRound);
    });
}

#[test]
fn test_advancing_less_than_window_does_not_reset() {
    new_test_ext().execute_with(|| {
        frame_system::Pallet::<TestRuntime>::set_block_number(1);

        // Hit the limit
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into()));
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into()));
        assert_ok!(Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into()));

        // Advance to the *last block* of the *current* window (still same window)
        let b = frame_system::Pallet::<TestRuntime>::block_number();
        let window_start = b - (b % BLOCKS_PER_WINDOW);
        let last_in_window = window_start + (BLOCKS_PER_WINDOW - 1);
        frame_system::Pallet::<TestRuntime>::set_block_number(last_in_window);

        let fourth = Pallet::<TestRuntime>::roll(RawOrigin::Signed(1).into());
        assert_noop!(fourth, Error::<TestRuntime>::ExceedRollsPerRound);
    });
}
