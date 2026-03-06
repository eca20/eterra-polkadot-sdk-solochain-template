use crate::{mock::*, Error, SeasonStatus};
use frame_support::{assert_noop, assert_ok};
use frame_support::BoundedVec;

#[test]
fn only_admin_can_create_and_activate() {
    new_test_ext().execute_with(|| {
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S1".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D".to_vec().try_into().unwrap();

        assert_noop!(
            Seasons::create_season(RuntimeOrigin::signed(1), name.clone(), desc.clone()),
            Error::<Test>::NotAdmin
        );

        assert_ok!(Seasons::add_admin(RuntimeOrigin::root(), 1));
        assert_ok!(Seasons::create_season(
            RuntimeOrigin::signed(1),
            name,
            desc
        ));
        assert_ok!(Seasons::activate_season(RuntimeOrigin::signed(1), 1));
        assert_eq!(Seasons::active_season_id(), Some(1));
        let info = Seasons::seasons(1).expect("season exists");
        assert_eq!(info.status, SeasonStatus::Active);
    });
}

#[test]
fn activate_closes_previous_active() {
    new_test_ext().execute_with(|| {
        assert_ok!(Seasons::add_admin(RuntimeOrigin::root(), 1));
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D".to_vec().try_into().unwrap();

        assert_ok!(Seasons::create_season(
            RuntimeOrigin::signed(1),
            name.clone(),
            desc.clone()
        ));
        assert_ok!(Seasons::create_season(RuntimeOrigin::signed(1), name, desc));

        assert_ok!(Seasons::activate_season(RuntimeOrigin::signed(1), 1));
        assert_eq!(Seasons::active_season_id(), Some(1));

        assert_ok!(Seasons::activate_season(RuntimeOrigin::signed(1), 2));
        assert_eq!(Seasons::active_season_id(), Some(2));

        let s1 = Seasons::seasons(1).expect("season 1 exists");
        let s2 = Seasons::seasons(2).expect("season 2 exists");
        assert_eq!(s1.status, SeasonStatus::Closed);
        assert_eq!(s2.status, SeasonStatus::Active);
    });
}

#[test]
fn close_clears_active() {
    new_test_ext().execute_with(|| {
        assert_ok!(Seasons::add_admin(RuntimeOrigin::root(), 1));
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D".to_vec().try_into().unwrap();
        assert_ok!(Seasons::create_season(RuntimeOrigin::signed(1), name, desc));
        assert_ok!(Seasons::activate_season(RuntimeOrigin::signed(1), 1));
        assert_ok!(Seasons::close_season(RuntimeOrigin::signed(1), 1));
        assert_eq!(Seasons::active_season_id(), None);
        let s1 = Seasons::seasons(1).expect("season exists");
        assert_eq!(s1.status, SeasonStatus::Closed);
    });
}
