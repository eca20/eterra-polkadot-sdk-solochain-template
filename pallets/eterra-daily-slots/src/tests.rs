#![cfg(test)]

use crate::mock::*;
use crate::{Error, LastRollTime, SlotMachineConfig, Pallet};
use frame_support::{assert_noop, assert_ok};

#[test]
fn test_roll_succeeds_with_valid_config() {
    new_test_ext().execute_with(|| {
        // 1. Set a valid config
        SlotMachineConfig::<TestRuntime>::put((3, 5, 2));

        // 2. Perform a roll
        let result = Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        );
        assert_ok!(result);

        // 3. Verify last roll time is now 90_000
        let stored_time = LastRollTime::<TestRuntime>::get(1);
        assert_eq!(stored_time, 90_000);
    });
}

#[test]
fn test_roll_fails_if_not_enough_time_has_passed() {
    new_test_ext().execute_with(|| {
        // Valid config
        SlotMachineConfig::<TestRuntime>::put((3, 5, 2));

        // First roll OK
        assert_ok!(Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        ));

        // Second roll immediately -> fails
        let second_roll = Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        );
        assert_noop!(second_roll, Error::<TestRuntime>::RollNotAvailableYet);
    });
}

#[test]
fn test_roll_fails_on_invalid_configuration() {
    new_test_ext().execute_with(|| {
        // Invalid config
        SlotMachineConfig::<TestRuntime>::put((0, 5, 2));

        // Rolling now should fail
        let result = Pallet::<TestRuntime>::roll(
            frame_system::RawOrigin::Signed(1).into()
        );
        assert_noop!(result, Error::<TestRuntime>::InvalidConfiguration);
    });
}