use crate::*;
use crate::pallet::{MediaClass, Delivery, CollectionRole, Error};

use frame_support::{assert_ok, assert_noop};
use crate::mock::{new_test_ext, Test, EterraMedia};
use crate::mock::RuntimeOrigin;

#[test]
fn create_collection_and_register_media_works() {
    new_test_ext().execute_with(|| {
        // Account 1 is our DefaultCollectionOwnerForMock; but we can create a
        // new collection explicitly to test the extrinsic.
        assert_ok!(EterraMedia::create_collection(
            RuntimeOrigin::signed(1),
            b"My Collection".to_vec(),
            b"My collection description".to_vec(),
        ));

        // This should be collection ID 0 or 1 depending on how many we create.
        // In this simple test, NextCollectionId starts at 0 and increments,
        // so the first create_collection gives ID 0.
        assert_ok!(EterraMedia::register_media(
            RuntimeOrigin::signed(1),
            Some(0), // explicit collection
            b"ipfs://example_cid".to_vec(),
            b"image/png".to_vec(),
            MediaClass::CoreAsset,
            Delivery::RemoteIpfs,
            Some(123),
        ));

        // Optional: verify some storage.
        let meta = pallet::Media::<Test>::get(0).expect("media 0 should exist");
        assert_eq!(meta.collection_id, 0);
        assert_eq!(meta.size_bytes, Some(123));
        assert!(!meta.is_deprecated);
    });
}

#[test]
fn non_admin_cannot_set_collection_role() {
    new_test_ext().execute_with(|| {
        // Create collection owned by account 1.
        assert_ok!(EterraMedia::create_collection(
            RuntimeOrigin::signed(1),
            b"Coll".to_vec(),
            b"Desc".to_vec(),
        ));

        // Account 2 is not an admin, so this should fail.
        assert_noop!(
            EterraMedia::set_collection_role(
                RuntimeOrigin::signed(2),
                0,
                3,
                CollectionRole::Uploader,
                true,
            ),
            Error::<Test>::NoPermission
        );
    });
}

#[test]
fn uploader_can_register_media_but_unauthorized_cannot() {
    new_test_ext().execute_with(|| {
        // Create collection owned by account 1 (id 0).
        assert_ok!(EterraMedia::create_collection(
            RuntimeOrigin::signed(1),
            b"Coll".to_vec(),
            b"Desc".to_vec(),
        ));

        // Grant Uploader role to account 2.
        assert_ok!(EterraMedia::set_collection_role(
            RuntimeOrigin::signed(1),
            0,
            2,
            CollectionRole::Uploader,
            true,
        ));

        // Account 2 can now upload.
        assert_ok!(EterraMedia::register_media(
            RuntimeOrigin::signed(2),
            Some(0),
            b"ipfs://cid_uploader".to_vec(),
            b"image/png".to_vec(),
            MediaClass::Cosmetic,
            Delivery::RemoteIpfs,
            None,
        ));

        // Account 3 is neither owner nor uploader, so this should fail.
        assert_noop!(
            EterraMedia::register_media(
                RuntimeOrigin::signed(3),
                Some(0),
                b"ipfs://cid_unauth".to_vec(),
                b"image/png".to_vec(),
                MediaClass::Cosmetic,
                Delivery::RemoteIpfs,
                None,
            ),
            Error::<Test>::NoPermission
        );
    });
}

#[test]
fn register_media_uses_default_collection_when_none() {
    new_test_ext().execute_with(|| {
        // Create collection 0.
        assert_ok!(EterraMedia::create_collection(
            RuntimeOrigin::signed(1),
            b"DefaultColl".to_vec(),
            b"Default desc".to_vec(),
        ));

        // Register media without specifying collection -> should use DefaultCollectionId (0).
        assert_ok!(EterraMedia::register_media(
            RuntimeOrigin::signed(1),
            None,
            b"ipfs://cid_default".to_vec(),
            b"image/png".to_vec(),
            MediaClass::CoreAsset,
            Delivery::RemoteIpfs,
            None,
        ));

        let meta = pallet::Media::<Test>::get(0).expect("media 0 should exist");
        assert_eq!(meta.collection_id, 0);
    });
}

#[test]
fn deprecate_media_permissions_and_double_deprecate() {
    new_test_ext().execute_with(|| {
        // Create collection and media owned by account 1.
        assert_ok!(EterraMedia::create_collection(
            RuntimeOrigin::signed(1),
            b"Coll".to_vec(),
            b"Desc".to_vec(),
        ));
        assert_ok!(EterraMedia::register_media(
            RuntimeOrigin::signed(1),
            Some(0),
            b"ipfs://cid".to_vec(),
            b"image/png".to_vec(),
            MediaClass::CoreAsset,
            Delivery::RemoteIpfs,
            None,
        ));

        // Account 2 is not owner or admin; cannot deprecate.
        assert_noop!(
            EterraMedia::deprecate_media(RuntimeOrigin::signed(2), 0),
            Error::<Test>::NoPermission
        );

        // Owner can deprecate.
        assert_ok!(EterraMedia::deprecate_media(RuntimeOrigin::signed(1), 0));

        // Media should be marked deprecated.
        let meta = pallet::Media::<Test>::get(0).expect("media 0 should exist");
        assert!(meta.is_deprecated);

        // Double deprecate should fail with AlreadyDeprecated.
        assert_noop!(
            EterraMedia::deprecate_media(RuntimeOrigin::signed(1), 0),
            Error::<Test>::AlreadyDeprecated
        );
    });
}

#[test]
fn create_collection_and_register_media_respect_length_limits() {
    new_test_ext().execute_with(|| {
        // Name too long (MaxNameLen = 64 in mock).
        let long_name = vec![b'a'; 65];
        assert_noop!(
            EterraMedia::create_collection(
                RuntimeOrigin::signed(1),
                long_name,
                b"Valid description".to_vec(),
            ),
            Error::<Test>::NameTooLong
        );

        // Create a valid collection to test URI / content-type limits.
        assert_ok!(EterraMedia::create_collection(
            RuntimeOrigin::signed(1),
            b"Coll".to_vec(),
            b"Desc".to_vec(),
        ));

        // URI too long (MaxUriLen = 256 in mock).
        let long_uri = vec![b'u'; 257];
        assert_noop!(
            EterraMedia::register_media(
                RuntimeOrigin::signed(1),
                Some(0),
                long_uri,
                b"image/png".to_vec(),
                MediaClass::CoreAsset,
                Delivery::RemoteIpfs,
                None,
            ),
            Error::<Test>::UriTooLong
        );

        // Content-type too long (MaxContentTypeLen = 64 in mock).
        let long_ct = vec![b't'; 65];
        assert_noop!(
            EterraMedia::register_media(
                RuntimeOrigin::signed(1),
                Some(0),
                b"ipfs://cid".to_vec(),
                long_ct,
                MediaClass::CoreAsset,
                Delivery::RemoteIpfs,
                None,
            ),
            Error::<Test>::ContentTypeTooLong
        );
    });
}