use crate::pallet::{CollectionRole, Delivery, Error, MediaClass};
use crate::*;

use crate::mock::RuntimeOrigin;
use crate::mock::{new_test_ext, new_test_ext_with_default_collection, EterraMedia, Test};
use frame_support::BoundedVec;
use frame_support::{assert_noop, assert_ok};

fn bounded_name(bytes: &[u8]) -> BoundedVec<u8, <Test as crate::pallet::Config>::MaxNameLen> {
    BoundedVec::try_from(bytes.to_vec()).expect("within name limit")
}

fn bounded_desc(
    bytes: &[u8],
) -> BoundedVec<u8, <Test as crate::pallet::Config>::MaxDescriptionLen> {
    BoundedVec::try_from(bytes.to_vec()).expect("within description limit")
}

fn bounded_uri(bytes: &[u8]) -> BoundedVec<u8, <Test as crate::pallet::Config>::MaxUriLen> {
    BoundedVec::try_from(bytes.to_vec()).expect("within uri limit")
}

fn bounded_ct(bytes: &[u8]) -> BoundedVec<u8, <Test as crate::pallet::Config>::MaxContentTypeLen> {
    BoundedVec::try_from(bytes.to_vec()).expect("within content-type limit")
}

#[test]
fn create_collection_and_register_media_works() {
    new_test_ext().execute_with(|| {
        // Account 1 is our DefaultCollectionOwnerForMock; but we can create a
        // new collection explicitly to test the extrinsic.
        assert_ok!(EterraMedia::create_collection(
            RuntimeOrigin::signed(1),
            bounded_name(b"My Collection"),
            bounded_desc(b"My collection description"),
        ));

        // This should be collection ID 0 or 1 depending on how many we create.
        // In this simple test, NextCollectionId starts at 0 and increments,
        // so the first create_collection gives ID 0.
        assert_ok!(EterraMedia::register_media(
            RuntimeOrigin::signed(1),
            Some(0), // explicit collection
            bounded_uri(b"ipfs://example_cid"),
            bounded_ct(b"image/png"),
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
            bounded_name(b"Coll"),
            bounded_desc(b"Desc"),
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
            bounded_name(b"Coll"),
            bounded_desc(b"Desc"),
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
            bounded_uri(b"ipfs://cid_uploader"),
            bounded_ct(b"image/png"),
            MediaClass::Cosmetic,
            Delivery::RemoteIpfs,
            None,
        ));

        // Account 3 is neither owner nor uploader, so this should fail.
        assert_noop!(
            EterraMedia::register_media(
                RuntimeOrigin::signed(3),
                Some(0),
                bounded_uri(b"ipfs://cid_unauth"),
                bounded_ct(b"image/png"),
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
            bounded_name(b"DefaultColl"),
            bounded_desc(b"Default desc"),
        ));

        // Register media without specifying collection -> should use DefaultCollectionId (0).
        assert_ok!(EterraMedia::register_media(
            RuntimeOrigin::signed(1),
            None,
            bounded_uri(b"ipfs://cid_default"),
            bounded_ct(b"image/png"),
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
            bounded_name(b"Coll"),
            bounded_desc(b"Desc"),
        ));
        assert_ok!(EterraMedia::register_media(
            RuntimeOrigin::signed(1),
            Some(0),
            bounded_uri(b"ipfs://cid"),
            bounded_ct(b"image/png"),
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
        assert!(
            BoundedVec::<u8, <Test as crate::pallet::Config>::MaxNameLen>::try_from(long_name)
                .is_err(),
            "name input should be bounded before dispatch"
        );

        // Create a valid collection to test URI / content-type limits.
        assert_ok!(EterraMedia::create_collection(
            RuntimeOrigin::signed(1),
            bounded_name(b"Coll"),
            bounded_desc(b"Desc"),
        ));

        // URI too long (MaxUriLen = 256 in mock).
        let long_uri = vec![b'u'; 257];
        assert!(
            BoundedVec::<u8, <Test as crate::pallet::Config>::MaxUriLen>::try_from(long_uri)
                .is_err(),
            "uri input should be bounded before dispatch"
        );

        // Content-type too long (MaxContentTypeLen = 64 in mock).
        let long_ct = vec![b't'; 65];
        assert!(
            BoundedVec::<u8, <Test as crate::pallet::Config>::MaxContentTypeLen>::try_from(long_ct)
                .is_err(),
            "content-type input should be bounded before dispatch"
        );
    });
}

#[test]
fn register_media_fails_for_unknown_collection() {
    new_test_ext().execute_with(|| {
        // No collections exist; using an arbitrary non-existent id should fail.
        assert_noop!(
            EterraMedia::register_media(
                RuntimeOrigin::signed(1),
                Some(42),
                bounded_uri(b"ipfs://cid_unknown"),
                bounded_ct(b"image/png"),
                MediaClass::CoreAsset,
                Delivery::RemoteIpfs,
                None,
            ),
            Error::<Test>::UnknownCollection
        );
    });
}

#[test]
fn freeze_collection_prevents_new_uploads() {
    new_test_ext().execute_with(|| {
        // Create collection 0 owned by account 1.
        assert_ok!(EterraMedia::create_collection(
            RuntimeOrigin::signed(1),
            bounded_name(b"Coll"),
            bounded_desc(b"Desc"),
        ));

        // First upload succeeds.
        assert_ok!(EterraMedia::register_media(
            RuntimeOrigin::signed(1),
            Some(0),
            bounded_uri(b"ipfs://cid_before_freeze"),
            bounded_ct(b"image/png"),
            MediaClass::CoreAsset,
            Delivery::RemoteIpfs,
            None,
        ));

        // Freeze the collection.
        assert_ok!(EterraMedia::freeze_collection(RuntimeOrigin::signed(1), 0));

        // New uploads should now fail with CollectionFrozen.
        assert_noop!(
            EterraMedia::register_media(
                RuntimeOrigin::signed(1),
                Some(0),
                bounded_uri(b"ipfs://cid_after_freeze"),
                bounded_ct(b"image/png"),
                MediaClass::CoreAsset,
                Delivery::RemoteIpfs,
                None,
            ),
            Error::<Test>::CollectionFrozen
        );
    });
}

#[test]
fn create_collection_fails_on_collection_id_overflow() {
    new_test_ext().execute_with(|| {
        NextCollectionId::<Test>::put(u32::MAX);

        assert_noop!(
            EterraMedia::create_collection(
                RuntimeOrigin::signed(1),
                bounded_name(b"Overflow"),
                bounded_desc(b"Overflow"),
            ),
            Error::<Test>::CollectionIdOverflow
        );
    });
}

#[test]
fn register_media_fails_on_media_id_overflow() {
    new_test_ext().execute_with(|| {
        assert_ok!(EterraMedia::create_collection(
            RuntimeOrigin::signed(1),
            bounded_name(b"Coll"),
            bounded_desc(b"Desc"),
        ));

        NextMediaId::<Test>::put(u64::MAX);

        assert_noop!(
            EterraMedia::register_media(
                RuntimeOrigin::signed(1),
                Some(0),
                bounded_uri(b"ipfs://overflow"),
                bounded_ct(b"image/png"),
                MediaClass::CoreAsset,
                Delivery::RemoteIpfs,
                None,
            ),
            Error::<Test>::MediaIdOverflow
        );
    });
}

#[test]
fn genesis_creates_default_collection_and_roles() {
    new_test_ext_with_default_collection().execute_with(|| {
        // In the mock, DefaultCollectionId = 0 and DefaultCollectionOwnerForMock::get() = 1.
        let info = pallet::Collections::<Test>::get(0)
            .expect("default collection should exist at genesis");

        assert_eq!(info.owner, 1);
        assert!(!info.frozen);

        let roles = pallet::CollectionRoles::<Test>::get(0, 1);
        assert!(roles.contains(&CollectionRole::Admin));
        assert!(roles.contains(&CollectionRole::Uploader));

        // Next collection id should not overwrite the default collection.
        assert_eq!(pallet::NextCollectionId::<Test>::get(), 1);
    });
}
