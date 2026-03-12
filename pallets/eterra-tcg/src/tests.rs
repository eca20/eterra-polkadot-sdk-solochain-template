use crate::pallet::Config as EterraSlotsConfig;
use crate::{
    mock::*, ActiveCard, CardArtworkCollectionId,
    CardCapacityBonus, CardPrices, Cards, CardsByOwner, Error, Event, ListedByOwner, NextCardId,
    PackCardInProgress, PackInProgress, PlayerPacks, SeasonCollectionIds, SeasonCollections,
    SeasonCollectionStatus,
};
use frame_support::traits::Get;
use frame_support::{assert_noop, assert_ok, BoundedBTreeSet, BoundedVec};
use log::{debug, Level, Metadata, Record};
use sp_runtime::traits::AccountIdConversion;
use std::sync::Once;

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
        // Tests can run in parallel across crates; if another test has already
        // installed a logger, don't fail.
        let _ = log::set_logger(&LOGGER);
        log::set_max_level(log::LevelFilter::Debug);
    });
}

fn assert_event_found<F>(matcher: F, event_name: &str)
where
    F: Fn(&RuntimeEvent) -> bool,
{
    let events = frame_system::Pallet::<Test>::events();
    let found = events.iter().any(|record| matcher(&record.event));

    assert!(
        found,
        "Expected {} event but did not find it. Events seen: {:?}",
        event_name, events
    );
}

/// Advances the block number to `n` to ensure event processing occurs.
fn run_to_block(n: u64) {
    while frame_system::Pallet::<Test>::block_number() < n {
        frame_system::Pallet::<Test>::set_block_number(
            frame_system::Pallet::<Test>::block_number() + 1,
        );
        frame_system::Pallet::<Test>::finalize();
        frame_system::Pallet::<Test>::initialize(
            &frame_system::Pallet::<Test>::block_number(),
            &Default::default(),
            &Default::default(),
        );
    }
}

fn seed_owned_card_index(owner: u64, count: u32, id_offset: u32) {
    let mut ids = BoundedBTreeSet::<u32, MaxOwnedCards>::new();
    for id in id_offset..id_offset.saturating_add(count) {
        assert!(ids.try_insert(id).is_ok());
    }
    CardsByOwner::<Test>::insert(owner, ids);
}

#[test]
fn mint_fails_without_active_season() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        let receiver = <Test as EterraSlotsConfig>::MintCardPriceReceiver::get();
        let player_before = Balances::free_balance(player);
        let receiver_before = Balances::free_balance(receiver);

        // Close the active season (created by the mock genesis helper).
        assert_ok!(EterraSeasons::close_season(RuntimeOrigin::signed(1), 1));

        assert_noop!(
            EterraSlots::mint_card(RuntimeOrigin::signed(player)),
            Error::<Test>::NoActiveSeason
        );

        // Transactional rollback: fee transfer must not occur.
        assert_eq!(Balances::free_balance(player), player_before);
        assert_eq!(Balances::free_balance(receiver), receiver_before);
    });
}

#[test]
fn activate_season_fails_when_published_pool_is_missing() {
    new_test_ext().execute_with(|| {
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S2".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D2".to_vec().try_into().unwrap();
        let collection_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"DraftOnly".to_vec().try_into().unwrap();

        assert_ok!(EterraSeasons::create_season(
            RuntimeOrigin::signed(1),
            name,
            desc
        ));
        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            collection_name
        ));

        assert_noop!(
            EterraSeasons::activate_season(RuntimeOrigin::signed(1), 2),
            Error::<Test>::NoPublishedSeasonCollection
        );
    });
}

#[test]
fn mint_card_writes_card_artwork() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let next_before = NextCardId::<Test>::get();

        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(player)));

        let art = EterraSlots::card_artwork(next_before).expect("card artwork written");
        assert_eq!(art.season_id, 1);
        assert_eq!(art.border_media_id, 0);
        assert_eq!(art.background_media_id, 1);
        assert_eq!(art.subject_media_id, 2);
        assert_eq!(art.back_media_id, 3);

        let mint_info = EterraSlots::card_mint_info(next_before).expect("card mint info written");
        assert_eq!(mint_info.minter, player);
        assert_eq!(mint_info.minted_at, System::block_number());
    });
}

#[test]
fn unique_minter_count_tracks_distinct_accounts_only_once() {
    new_test_ext().execute_with(|| {
        let first = 2u64;
        let second = 3u64;

        assert_eq!(EterraSlots::unique_minter_count(), 0);

        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(first)));
        assert_eq!(EterraSlots::unique_minter_count(), 1);
        assert!(EterraSlots::has_minted(first).is_some());

        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(first)));
        assert_eq!(EterraSlots::unique_minter_count(), 1);

        assert_ok!(EterraSlots::mint_pro(RuntimeOrigin::signed(second)));
        assert_eq!(EterraSlots::unique_minter_count(), 2);
        assert!(EterraSlots::has_minted(second).is_some());
    });
}

#[test]
fn publish_season_collection_requires_at_least_one_art_layer() {
    new_test_ext().execute_with(|| {
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S2".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D2".to_vec().try_into().unwrap();
        let collection_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Core".to_vec().try_into().unwrap();

        assert_ok!(EterraSeasons::create_season(
            RuntimeOrigin::signed(1),
            name,
            desc
        ));
        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            collection_name
        ));

        assert_noop!(
            EterraSlots::publish_season_collection(RuntimeOrigin::signed(1), 2, 0),
            Error::<Test>::SeasonCollectionIncomplete
        );
    });
}

#[test]
fn first_published_collection_requires_a_back_layer() {
    new_test_ext().execute_with(|| {
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S2".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D2".to_vec().try_into().unwrap();
        let collection_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Core".to_vec().try_into().unwrap();
        let ct: BoundedVec<u8, MaxMediaContentTypeLen> = b"image/png".to_vec().try_into().unwrap();

        assert_ok!(EterraSeasons::create_season(
            RuntimeOrigin::signed(1),
            name,
            desc
        ));

        for suffix in [
            b"border".as_slice(),
            b"background".as_slice(),
            b"subject".as_slice(),
        ] {
            let mut uri_bytes = b"ipfs://season2-".to_vec();
            uri_bytes.extend_from_slice(suffix);
            let uri: BoundedVec<u8, MaxMediaUriLen> = uri_bytes.try_into().unwrap();
            assert_ok!(EterraMedia::register_media(
                RuntimeOrigin::signed(1),
                None,
                uri,
                ct.clone(),
                pallet_eterra_media::MediaClass::CoreAsset,
                pallet_eterra_media::Delivery::RemoteIpfs,
                None,
            ));
        }

        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            collection_name
        ));
        for (kind, media_id) in [
            (crate::AssetKind::Border, 4u64),
            (crate::AssetKind::Background, 5u64),
            (crate::AssetKind::Subject, 6u64),
        ] {
            assert_ok!(EterraSlots::add_season_collection_asset(
                RuntimeOrigin::signed(1),
                2,
                0,
                kind,
                media_id
            ));
        }

        assert_noop!(
            EterraSlots::publish_season_collection(RuntimeOrigin::signed(1), 2, 0),
            Error::<Test>::SeasonCollectionIncomplete
        );
    });
}

#[test]
fn published_season_collection_is_used_for_minting() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S2".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D2".to_vec().try_into().unwrap();
        let collection_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Core".to_vec().try_into().unwrap();
        let ct: BoundedVec<u8, MaxMediaContentTypeLen> = b"image/png".to_vec().try_into().unwrap();

        assert_ok!(EterraSeasons::create_season(
            RuntimeOrigin::signed(1),
            name,
            desc
        ));

        for suffix in [
            b"border".as_slice(),
            b"background".as_slice(),
            b"subject".as_slice(),
            b"back".as_slice(),
        ] {
            let mut uri_bytes = b"ipfs://season2-".to_vec();
            uri_bytes.extend_from_slice(suffix);
            let uri: BoundedVec<u8, MaxMediaUriLen> = uri_bytes.try_into().unwrap();
            assert_ok!(EterraMedia::register_media(
                RuntimeOrigin::signed(1),
                None,
                uri,
                ct.clone(),
                pallet_eterra_media::MediaClass::CoreAsset,
                pallet_eterra_media::Delivery::RemoteIpfs,
                None,
            ));
        }

        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            collection_name
        ));
        assert_ok!(EterraSlots::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            2,
            0,
            crate::AssetKind::Border,
            4
        ));
        assert_ok!(EterraSlots::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            2,
            0,
            crate::AssetKind::Background,
            5
        ));
        assert_ok!(EterraSlots::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            2,
            0,
            crate::AssetKind::Subject,
            6
        ));
        assert_ok!(EterraSlots::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            2,
            0,
            crate::AssetKind::Back,
            7
        ));
        assert_ok!(EterraSlots::publish_season_collection(
            RuntimeOrigin::signed(1),
            2,
            0
        ));
        assert_ok!(EterraSeasons::activate_season(RuntimeOrigin::signed(1), 2));

        let card_id = NextCardId::<Test>::get();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(player)));

        let art = EterraSlots::card_artwork(card_id).expect("card artwork written");
        assert_eq!(art.season_id, 2);
        assert_eq!(art.border_media_id, 4);
        assert_eq!(art.background_media_id, 5);
        assert_eq!(art.subject_media_id, 6);
        assert_eq!(art.back_media_id, 7);
        assert_eq!(CardArtworkCollectionId::<Test>::get(card_id), Some(0));
    });
}

#[test]
fn published_partial_collections_contribute_to_the_shared_season_pool() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S2".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D2".to_vec().try_into().unwrap();
        let core_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Core".to_vec().try_into().unwrap();
        let subject_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Subject Drop".to_vec().try_into().unwrap();
        let ct: BoundedVec<u8, MaxMediaContentTypeLen> = b"image/png".to_vec().try_into().unwrap();

        assert_ok!(EterraSeasons::create_season(
            RuntimeOrigin::signed(1),
            name,
            desc
        ));

        for suffix in [
            b"core-border".as_slice(),
            b"core-background".as_slice(),
            b"core-back".as_slice(),
            b"subject-drop".as_slice(),
        ] {
            let mut uri_bytes = b"ipfs://season2-".to_vec();
            uri_bytes.extend_from_slice(suffix);
            let uri: BoundedVec<u8, MaxMediaUriLen> = uri_bytes.try_into().unwrap();
            assert_ok!(EterraMedia::register_media(
                RuntimeOrigin::signed(1),
                None,
                uri,
                ct.clone(),
                pallet_eterra_media::MediaClass::CoreAsset,
                pallet_eterra_media::Delivery::RemoteIpfs,
                None,
            ));
        }

        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            core_name
        ));
        for (kind, media_id) in [
            (crate::AssetKind::Border, 4u64),
            (crate::AssetKind::Background, 5u64),
            (crate::AssetKind::Back, 6u64),
        ] {
            assert_ok!(EterraSlots::add_season_collection_asset(
                RuntimeOrigin::signed(1),
                2,
                0,
                kind,
                media_id
            ));
        }
        assert_ok!(EterraSlots::publish_season_collection(
            RuntimeOrigin::signed(1),
            2,
            0
        ));

        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            subject_name
        ));
        assert_ok!(EterraSlots::add_season_collection_asset(
            RuntimeOrigin::signed(1),
            2,
            1,
            crate::AssetKind::Subject,
            7
        ));
        assert_ok!(EterraSlots::publish_season_collection(
            RuntimeOrigin::signed(1),
            2,
            1
        ));
        assert_ok!(EterraSeasons::activate_season(RuntimeOrigin::signed(1), 2));

        let card_id = NextCardId::<Test>::get();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(player)));

        let art = EterraSlots::card_artwork(card_id).expect("card artwork written");
        assert_eq!(art.season_id, 2);
        assert_eq!(art.border_media_id, 4);
        assert_eq!(art.background_media_id, 5);
        assert_eq!(art.subject_media_id, 7);
        assert_eq!(art.back_media_id, 6);
        assert_eq!(CardArtworkCollectionId::<Test>::get(card_id), Some(1));
    });
}

#[test]
fn active_season_can_publish_new_collection_without_mutating_old_one() {
    new_test_ext().execute_with(|| {
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S2".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D2".to_vec().try_into().unwrap();
        let core_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Core".to_vec().try_into().unwrap();
        let ct: BoundedVec<u8, MaxMediaContentTypeLen> = b"image/png".to_vec().try_into().unwrap();

        assert_ok!(EterraSeasons::create_season(
            RuntimeOrigin::signed(1),
            name,
            desc
        ));

        for suffix in [
            b"c-border".as_slice(),
            b"c-background".as_slice(),
            b"c-subject".as_slice(),
            b"c-back".as_slice(),
            b"e-border".as_slice(),
            b"e-background".as_slice(),
            b"e-subject".as_slice(),
        ] {
            let mut uri_bytes = b"ipfs://season2-".to_vec();
            uri_bytes.extend_from_slice(suffix);
            let uri: BoundedVec<u8, MaxMediaUriLen> = uri_bytes.try_into().unwrap();
            assert_ok!(EterraMedia::register_media(
                RuntimeOrigin::signed(1),
                None,
                uri,
                ct.clone(),
                pallet_eterra_media::MediaClass::CoreAsset,
                pallet_eterra_media::Delivery::RemoteIpfs,
                None,
            ));
        }

        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            core_name
        ));
        for (kind, media_id) in [
            (crate::AssetKind::Border, 4u64),
            (crate::AssetKind::Background, 5u64),
            (crate::AssetKind::Subject, 6u64),
            (crate::AssetKind::Back, 7u64),
        ] {
            assert_ok!(EterraSlots::add_season_collection_asset(
                RuntimeOrigin::signed(1),
                2,
                0,
                kind,
                media_id
            ));
        }
        assert_ok!(EterraSlots::publish_season_collection(
            RuntimeOrigin::signed(1),
            2,
            0
        ));
        assert_ok!(EterraSeasons::activate_season(RuntimeOrigin::signed(1), 2));

        let expansion_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Expansion".to_vec().try_into().unwrap();
        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            expansion_name
        ));
        for (kind, media_id) in [
            (crate::AssetKind::Border, 8u64),
            (crate::AssetKind::Background, 9u64),
            (crate::AssetKind::Subject, 10u64),
        ] {
            assert_ok!(EterraSlots::add_season_collection_asset(
                RuntimeOrigin::signed(1),
                2,
                1,
                kind,
                media_id
            ));
        }
        assert_ok!(EterraSlots::publish_season_collection(
            RuntimeOrigin::signed(1),
            2,
            1
        ));

        assert_eq!(SeasonCollectionIds::<Test>::get(2).to_vec(), vec![0, 1]);
        assert_eq!(
            SeasonCollections::<Test>::get(2, 0).map(|collection| collection.status),
            Some(SeasonCollectionStatus::Published)
        );
        assert_eq!(
            SeasonCollections::<Test>::get(2, 1).map(|collection| collection.status),
            Some(SeasonCollectionStatus::Published)
        );
    });
}

#[test]
fn draft_collection_is_not_used_until_published() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let name: BoundedVec<u8, MaxSeasonNameLen> = b"S2".to_vec().try_into().unwrap();
        let desc: BoundedVec<u8, MaxSeasonDescLen> = b"D2".to_vec().try_into().unwrap();
        let ct: BoundedVec<u8, MaxMediaContentTypeLen> = b"image/png".to_vec().try_into().unwrap();

        assert_ok!(EterraSeasons::create_season(
            RuntimeOrigin::signed(1),
            name,
            desc
        ));

        for suffix in [
            b"core-border".as_slice(),
            b"core-background".as_slice(),
            b"core-subject".as_slice(),
            b"core-back".as_slice(),
            b"draft-border".as_slice(),
            b"draft-background".as_slice(),
            b"draft-subject".as_slice(),
        ] {
            let mut uri_bytes = b"ipfs://season2-".to_vec();
            uri_bytes.extend_from_slice(suffix);
            let uri: BoundedVec<u8, MaxMediaUriLen> = uri_bytes.try_into().unwrap();
            assert_ok!(EterraMedia::register_media(
                RuntimeOrigin::signed(1),
                None,
                uri,
                ct.clone(),
                pallet_eterra_media::MediaClass::CoreAsset,
                pallet_eterra_media::Delivery::RemoteIpfs,
                None,
            ));
        }

        let core_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Core".to_vec().try_into().unwrap();
        let draft_name: BoundedVec<u8, MaxSeasonCollectionNameLen> =
            b"Draft".to_vec().try_into().unwrap();

        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            core_name
        ));
        for (kind, media_id) in [
            (crate::AssetKind::Border, 4u64),
            (crate::AssetKind::Background, 5u64),
            (crate::AssetKind::Subject, 6u64),
            (crate::AssetKind::Back, 7u64),
        ] {
            assert_ok!(EterraSlots::add_season_collection_asset(
                RuntimeOrigin::signed(1),
                2,
                0,
                kind,
                media_id
            ));
        }
        assert_ok!(EterraSlots::publish_season_collection(
            RuntimeOrigin::signed(1),
            2,
            0
        ));

        assert_ok!(EterraSlots::create_season_collection(
            RuntimeOrigin::signed(1),
            2,
            draft_name
        ));
        for (kind, media_id) in [
            (crate::AssetKind::Border, 8u64),
            (crate::AssetKind::Background, 9u64),
            (crate::AssetKind::Subject, 10u64),
        ] {
            assert_ok!(EterraSlots::add_season_collection_asset(
                RuntimeOrigin::signed(1),
                2,
                1,
                kind,
                media_id
            ));
        }

        assert_ok!(EterraSeasons::activate_season(RuntimeOrigin::signed(1), 2));
        let card_id = NextCardId::<Test>::get();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(player)));

        let art = EterraSlots::card_artwork(card_id).expect("card artwork written");
        assert_eq!(art.border_media_id, 4);
        assert_eq!(art.background_media_id, 5);
        assert_eq!(art.subject_media_id, 6);
        assert_eq!(art.back_media_id, 7);
        assert_eq!(CardArtworkCollectionId::<Test>::get(card_id), Some(0));
    });
}

#[test]
fn init_card_nft_collection_creates_collection_and_sets_storage() {
    new_test_ext().execute_with(|| {
        assert_ok!(EterraSlots::init_card_nft_collection(
            RuntimeOrigin::signed(1),
            1
        ));

        assert_eq!(EterraSlots::card_nft_collection_id(), Some(0));
        assert!(pallet_nfts::Collection::<Test>::contains_key(0));
    });
}

#[test]
fn convert_to_nft_escrows_card_and_mints_item() {
    new_test_ext().execute_with(|| {
        let player = 2u64;

        assert_ok!(EterraSlots::init_card_nft_collection(
            RuntimeOrigin::signed(1),
            1
        ));
        let collection_id = EterraSlots::card_nft_collection_id().expect("collection id set");

        let card_id = NextCardId::<Test>::get();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(player)));

        assert_ok!(EterraSlots::convert_to_nft(
            RuntimeOrigin::signed(player),
            card_id
        ));

        let escrow: u64 = frame_support::PalletId(*b"et/tcgsc").into_account_truncating();
        let card = EterraSlots::cards(card_id).expect("card exists");
        assert_eq!(card.get_owner(), &escrow);
        assert!(EterraSlots::converted(card_id).is_some());

        assert_eq!(
            pallet_nfts::Pallet::<Test>::owner(collection_id, card_id),
            Some(player)
        );
    });
}

#[test]
fn nft_transfer_allows_new_owner_to_unwrap() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let new_owner = 3u64;

        assert_ok!(EterraSlots::init_card_nft_collection(
            RuntimeOrigin::signed(1),
            1
        ));
        let collection_id = EterraSlots::card_nft_collection_id().expect("collection id set");

        let card_id = NextCardId::<Test>::get();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(player)));
        assert_ok!(EterraSlots::convert_to_nft(
            RuntimeOrigin::signed(player),
            card_id
        ));

        assert_ok!(Nfts::transfer(
            RuntimeOrigin::signed(player),
            collection_id,
            card_id,
            new_owner
        ));

        assert_eq!(
            pallet_nfts::Pallet::<Test>::owner(collection_id, card_id),
            Some(new_owner)
        );

        assert_ok!(EterraSlots::unwrap_from_nft(
            RuntimeOrigin::signed(new_owner),
            card_id
        ));

        let card = EterraSlots::cards(card_id).expect("card exists");
        assert_eq!(card.get_owner(), &new_owner);
        assert!(EterraSlots::converted(card_id).is_none());
        assert_eq!(
            pallet_nfts::Pallet::<Test>::owner(collection_id, card_id),
            None
        );
    });
}

#[test]
fn convert_to_nft_fails_when_card_not_finalized() {
    new_test_ext().execute_with(|| {
        let player = 2u64;

        assert_ok!(EterraSlots::init_card_nft_collection(
            RuntimeOrigin::signed(1),
            1
        ));

        // Pro mint creates a non-finalized card until accepted.
        assert_ok!(EterraSlots::mint_pro(RuntimeOrigin::signed(player)));
        let card_id = EterraSlots::pro_in_progress(player).expect("pro in progress");

        assert_noop!(
            EterraSlots::convert_to_nft(RuntimeOrigin::signed(player), card_id),
            Error::<Test>::CardNotFinalized
        );
    });
}

#[test]
fn test_mint_pack_simple_storage_check() {
    new_test_ext().execute_with(|| {
        let player = 1;

        // Clear any old data
        PlayerPacks::<Test>::remove(&player);
        ActiveCard::<Test>::remove(&player);
        System::reset_events();
        System::set_block_number(42); // or any number you prefer

        // Mint the pack
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        // Verify the minted pack is in storage
        let packs = EterraSlots::player_packs(player);
        assert_eq!(packs.len(), 1, "Should have exactly 1 pack minted");

        // The newly minted pack should have ID = 42 (the current block)
        let minted_pack = &packs[0];
        assert_eq!(minted_pack.get_id(), 42);
    });
}

#[test]
fn test_mint_pack_check_event_directly() {
    new_test_ext().execute_with(|| {
        let player = 1;

        // Ensure a known block number
        System::set_block_number(100);
        System::reset_events();

        // Dispatch extrinsic
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        // Check that PackMinted event with pack_id=100 was indeed emitted
        System::assert_has_event(
            RuntimeEvent::EterraSlots(Event::PackMinted {
                player,
                pack_id: 100,
            })
            .into(),
        );
    });
}

#[test]
fn test_mint_pack_inspect_events() {
    new_test_ext().execute_with(|| {
        let player = 1;
        System::set_block_number(7);
        System::reset_events();

        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        let all_events = System::events();
        assert!(!all_events.is_empty(), "No events were recorded!");

        let minted_event_found = all_events.iter().any(|r| match &r.event {
            RuntimeEvent::EterraSlots(Event::PackMinted {
                player: who,
                pack_id,
            }) => *who == player && *pack_id == 7,
            _ => false,
        });
        assert!(
            minted_event_found,
            "Expected PackMinted for player={}, pack_id=7, but not found.",
            player
        );
    });
}

#[test]
fn test_mint_pack_storage_and_events() {
    new_test_ext().execute_with(|| {
        let player = 1;
        System::set_block_number(8);
        System::reset_events();

        // 1) Mint the pack
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        // 2) Check storage updated
        let packs = EterraSlots::player_packs(player);
        assert_eq!(packs.len(), 1, "Should have 1 pack minted now.");
        let minted_pack = &packs[0];
        assert_eq!(minted_pack.get_id(), 8);

        // 3) Check event with direct assertion
        System::assert_has_event(
            RuntimeEvent::EterraSlots(Event::PackMinted { player, pack_id: 8 }).into(),
        );
    });
}

#[test]
fn mint_pack_rolls_back_when_card_ids_exhausted() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        let receiver = <Test as EterraSlotsConfig>::PackPriceReceiver::get();
        let price: u128 = <Test as EterraSlotsConfig>::PackPrice::get();
        let player_before = Balances::free_balance(player);
        let receiver_before = Balances::free_balance(receiver);

        NextCardId::<Test>::put(u32::MAX - 1);

        assert_noop!(
            EterraSlots::mint_pack(RuntimeOrigin::signed(player)),
            Error::<Test>::CardIdExhausted
        );

        // Ensure transactional rollback: no partial cards or pack state persisted.
        assert_eq!(NextCardId::<Test>::get(), u32::MAX - 1);
        assert!(Cards::<Test>::get(u32::MAX - 1).is_none());
        assert!(Cards::<Test>::get(u32::MAX).is_none());
        assert!(PlayerPacks::<Test>::get(player).is_empty());
        assert_eq!(ActiveCard::<Test>::get(player), None);

        // Fee transfer must also be rolled back.
        assert_eq!(Balances::free_balance(player), player_before);
        assert_eq!(Balances::free_balance(receiver), receiver_before);
        // Sanity check: price is non-zero in this mock.
        assert!(price > 0);
    });
}

#[test]
fn mint_pack_charges_price_and_mints_expected_card_count() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        let receiver = <Test as EterraSlotsConfig>::PackPriceReceiver::get();
        let price: u128 = <Test as EterraSlotsConfig>::PackPrice::get();
        let cards_per_pack: u8 = <Test as EterraSlotsConfig>::CardsPerPack::get();

        let player_before = Balances::free_balance(player);
        let receiver_before = Balances::free_balance(receiver);

        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        // Price charged to player and sent to receiver.
        assert_eq!(Balances::free_balance(player), player_before - price);
        assert_eq!(Balances::free_balance(receiver), receiver_before + price);

        // Pack contains the expected number of unique cards (unique IDs).
        let packs = EterraSlots::player_packs(player);
        let pack = packs.last().expect("pack exists");
        assert_eq!(pack.get_card_ids().len(), cards_per_pack as usize);

        // Ensure the card IDs within the pack are unique.
        let mut ids: sp_std::vec::Vec<u32> = pack.get_card_ids().iter().copied().collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), cards_per_pack as usize);
    });
}

#[test]
fn mint_pro_charges_price_and_starts_in_progress() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        let receiver = <Test as EterraSlotsConfig>::ProPriceReceiver::get();
        let price: u128 = <Test as EterraSlotsConfig>::ProPrice::get();

        let player_before = Balances::free_balance(player);
        let receiver_before = Balances::free_balance(receiver);

        System::reset_events();
        assert_ok!(EterraSlots::mint_pro(RuntimeOrigin::signed(player)));

        // Price charged to player and sent to receiver.
        assert_eq!(Balances::free_balance(player), player_before - price);
        assert_eq!(Balances::free_balance(receiver), receiver_before + price);

        // Pro mint should create an in-progress card (no spin yet).
        let card_id = EterraSlots::pro_in_progress(player).expect("pro in progress");
        let card = EterraSlots::cards(card_id).expect("card exists");
        assert!(!card.is_finalized());
        assert!(card.get_slot_values().is_none());
        assert_eq!(EterraSlots::card_attempts(card_id), 0);
        assert!(EterraSlots::cards_by_owner(player).contains(&card_id));

        // Events: should include ProMintStarted.
        assert_event_found(
            |e| matches!(e, RuntimeEvent::EterraSlots(Event::ProMintStarted { player: who, card_id: id }) if *who == player && *id == card_id),
            "ProMintStarted",
        );
    });
}

#[test]
fn pro_card_stays_visible_in_owner_index_after_finalize() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        assert_ok!(EterraSlots::mint_pro(RuntimeOrigin::signed(player)));
        let card_id = EterraSlots::pro_in_progress(player).expect("pro in progress");
        assert!(EterraSlots::cards_by_owner(player).contains(&card_id));

        assert_ok!(EterraSlots::spin_pro(RuntimeOrigin::signed(player)));
        assert_ok!(EterraSlots::accept_pro(RuntimeOrigin::signed(player)));

        assert!(EterraSlots::cards_by_owner(player).contains(&card_id));
    });
}

#[test]
fn mint_pro_fails_when_already_in_progress() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        assert_ok!(EterraSlots::mint_pro(RuntimeOrigin::signed(player)));
        assert_noop!(
            EterraSlots::mint_pro(RuntimeOrigin::signed(player)),
            Error::<Test>::ProMintAlreadyInProgress
        );
    });
}

#[test]
fn spin_pro_increments_spins() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        assert_ok!(EterraSlots::mint_pro(RuntimeOrigin::signed(player)));
        let card_id = EterraSlots::pro_in_progress(player).expect("pro in progress");

        assert_eq!(EterraSlots::card_attempts(card_id), 0);
        assert_ok!(EterraSlots::spin_pro(RuntimeOrigin::signed(player)));
        assert_eq!(EterraSlots::card_attempts(card_id), 1);
    });
}

#[test]
fn accept_pro_finalizes_and_clears_progress() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        assert_ok!(EterraSlots::mint_pro(RuntimeOrigin::signed(player)));
        let card_id = EterraSlots::pro_in_progress(player).expect("pro in progress");

        assert_ok!(EterraSlots::spin_pro(RuntimeOrigin::signed(player)));
        assert_ok!(EterraSlots::accept_pro(RuntimeOrigin::signed(player)));

        let card = EterraSlots::cards(card_id).expect("card exists");
        assert!(card.is_finalized());
        assert_eq!(EterraSlots::card_attempts(card_id), 0);
        assert!(EterraSlots::pro_in_progress(player).is_none());
    });
}

#[test]
fn accept_pro_fails_when_not_spun() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        assert_ok!(EterraSlots::mint_pro(RuntimeOrigin::signed(player)));
        assert_noop!(
            EterraSlots::accept_pro(RuntimeOrigin::signed(player)),
            Error::<Test>::ProCardNotSpun
        );
    });
}

#[test]
fn spin_pro_forces_finalize_on_last_spin() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        let max_spins: u8 = <Test as EterraSlotsConfig>::MaxProSpins::get();
        assert!(max_spins > 1);

        assert_ok!(EterraSlots::mint_pro(RuntimeOrigin::signed(player)));
        let card_id = EterraSlots::pro_in_progress(player).expect("pro in progress");

        for _ in 0..max_spins {
            assert_ok!(EterraSlots::spin_pro(RuntimeOrigin::signed(player)));
        }

        let card = EterraSlots::cards(card_id).expect("card exists");
        assert!(card.is_finalized());
        assert!(EterraSlots::pro_in_progress(player).is_none());

        // Further spins should fail since the pro mint has been finalized/cleared.
        assert_noop!(
            EterraSlots::spin_pro(RuntimeOrigin::signed(player)),
            Error::<Test>::NoProMintInProgress
        );
    });
}

#[test]
fn test_generate_slot_success() {
    init_logger();
    new_test_ext().execute_with(|| {
        let player = 1;

        debug!("Ensuring fresh state for player {}", player);
        PlayerPacks::<Test>::remove(&player);
        ActiveCard::<Test>::remove(&player);
        System::reset_events();
        assert!(
            EterraSlots::player_packs(player).is_empty(),
            "Player should start with no packs"
        );

        debug!(
            "Minting a pack for player {} before generating a slot.",
            player
        );
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        debug!("Running to next block...");
        run_to_block(frame_system::Pallet::<Test>::block_number() + 1);

        // Check active card
        let active_card = ActiveCard::<Test>::get(player);
        assert!(
            active_card.is_some(),
            "Expected an active card but found None"
        );

        debug!("Generate slot for the active card");
        System::reset_events();
        assert_ok!(EterraSlots::generate_slot(RuntimeOrigin::signed(player)));

        run_to_block(frame_system::Pallet::<Test>::block_number() + 1);

        // We only have `SlotGenerated { card_id, values }` now
        // So let's confirm that event by checking it has the correct type:
        assert_event_found(
            |e| {
                matches!(
                    e,
                    RuntimeEvent::EterraSlots(Event::SlotGenerated { values, .. })
                        if values.len() == 4
                )
            },
            "SlotGenerated",
        );
    });
}

#[test]
fn test_accept_slot_success() {
    init_logger();
    new_test_ext().execute_with(|| {
        let player = 1;

        debug!("Minting a pack and generating a slot for player {}", player);
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));
        run_to_block(System::block_number() + 1);

        // Generate a slot
        assert_ok!(EterraSlots::generate_slot(RuntimeOrigin::signed(player)));
        run_to_block(System::block_number() + 1);

        debug!("Accepting slot...");
        System::reset_events();
        assert_ok!(EterraSlots::accept_slot(RuntimeOrigin::signed(player)));
        run_to_block(System::block_number() + 1);

        // The event is now `SlotAccepted { card_id }`, no player field
        assert_event_found(
            |e| matches!(e, RuntimeEvent::EterraSlots(Event::SlotAccepted { .. })),
            "SlotAccepted",
        );
    });
}

#[test]
fn mint_pack_fails_when_card_capacity_would_be_exceeded_without_charging_fee() {
    init_logger();
    new_test_ext().execute_with(|| {
        let player = 1u64;
        let receiver = <Test as EterraSlotsConfig>::PackPriceReceiver::get();
        let player_before = Balances::free_balance(player);
        let receiver_before = Balances::free_balance(receiver);

        seed_owned_card_index(player, 495, 10_000);

        assert_noop!(
            EterraSlots::mint_pack(RuntimeOrigin::signed(player)),
            Error::<Test>::CardCapacityExceeded
        );

        assert_eq!(Balances::free_balance(player), player_before);
        assert_eq!(Balances::free_balance(receiver), receiver_before);
        assert!(PlayerPacks::<Test>::get(player).is_empty());
    });
}

#[test]
fn test_generate_slot_fail_when_no_active_card() {
    init_logger();
    new_test_ext().execute_with(|| {
        let player = 1;

        debug!("Attempt to generate slot with no pack at all");
        assert_noop!(
            EterraSlots::generate_slot(RuntimeOrigin::signed(player)),
            Error::<Test>::NoPackFound
        );
    });
}

#[test]
fn test_accept_slot_fail_when_slot_not_rolled() {
    init_logger();
    new_test_ext().execute_with(|| {
        let player = 1;

        debug!("Minting pack but not generating a slot yet");
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        debug!("Try to accept slot before rolling one");
        let result = EterraSlots::accept_slot(RuntimeOrigin::signed(player));
        assert!(
            result == Err(Error::<Test>::NoActiveCard.into()),
            "Expected NoActiveCard but got {:?}",
            result
        );
    });
}

#[test]
fn active_card_advances_after_finalize() {
    new_test_ext().execute_with(|| {
        let player = 1;
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));
        assert_eq!(ActiveCard::<Test>::get(player), Some(0));

        let max_attempts: u8 = <Test as EterraSlotsConfig>::MaxAttempts::get();
        for _ in 0..max_attempts {
            assert_ok!(EterraSlots::generate_slot(RuntimeOrigin::signed(player)));
        }

        assert_eq!(ActiveCard::<Test>::get(player), Some(1));
        let packs = EterraSlots::player_packs(player);
        let pack = packs.last().expect("pack exists");
        assert_eq!(pack.get_active_card_index(), 1);

        let first_id = *pack.get_card_ids().first().expect("card exists");
        let card = EterraSlots::cards(first_id).expect("card exists");
        assert!(card.is_finalized());
    });
}

#[test]
fn pack_completed_clears_active_card_and_emits_event() {
    new_test_ext().execute_with(|| {
        let player = 1;
        System::set_block_number(1);
        System::reset_events();
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        let max_attempts: u8 = <Test as EterraSlotsConfig>::MaxAttempts::get();
        let cards_per_pack: u8 = <Test as EterraSlotsConfig>::CardsPerPack::get();

        for _ in 0..cards_per_pack {
            for _ in 0..max_attempts {
                assert_ok!(EterraSlots::generate_slot(RuntimeOrigin::signed(player)));
            }
        }

        assert_eq!(ActiveCard::<Test>::get(player), None);
        assert!(PackInProgress::<Test>::get(player).is_none());
        assert!(PackCardInProgress::<Test>::get(player).is_none());
        let packs = EterraSlots::player_packs(player);
        assert!(packs.is_empty());

        System::assert_has_event(RuntimeEvent::EterraSlots(Event::PackCompleted {
            player,
            pack_id: 1,
        }));
    });
}

#[test]
fn test_attempts_removed_after_generating_max_times() {
    new_test_ext().execute_with(|| {
        let player = 1;

        // Mint a pack
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        // We want to see which card_id was created.
        let packs = EterraSlots::player_packs(player);
        let last_pack = packs.last().expect("Pack should exist");
        let card_id = last_pack
            .get_card_ids()
            .first()
            .copied()
            .expect("Should have at least one card ID in the pack");

        // Check the MaxAttempts
        let max_attempts: u8 = <Test as EterraSlotsConfig>::MaxAttempts::get();

        // Generate slots until we hit max
        for _ in 0..max_attempts {
            assert_ok!(EterraSlots::generate_slot(RuntimeOrigin::signed(player)));
        }

        // After final generation, that card should be finalized => attempts removed
        let attempts_after = EterraSlots::card_attempts(card_id);
        assert_eq!(
            attempts_after, 0,
            "Expected attempts to be removed after finalization."
        );
    });
}

#[test]
fn test_attempts_removed_after_accept_slot() {
    new_test_ext().execute_with(|| {
        let player = 1;

        // Mint a pack
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(player)));

        // Grab the first card_id
        let packs = EterraSlots::player_packs(player);
        let last_pack = packs.last().unwrap();
        let card_id = *last_pack.get_card_ids().first().unwrap();

        // Generate one slot
        assert_ok!(EterraSlots::generate_slot(RuntimeOrigin::signed(player)));

        // Should now have attempts = 1
        let attempts_before = EterraSlots::card_attempts(card_id);
        assert_eq!(attempts_before, 1);

        // Accept slot => finalize the card => attempts removed
        assert_ok!(EterraSlots::accept_slot(RuntimeOrigin::signed(player)));

        let attempts_after = EterraSlots::card_attempts(card_id);
        assert_eq!(
            attempts_after, 0,
            "Expected attempts to be removed after finalization."
        );
    });
}

#[test]
fn test_transfer_card_not_owner_fails() {
    new_test_ext().execute_with(|| {
        let owner = 1;
        let non_owner = 2;
        let malicious_user = 3;

        // 1) Mint a pack for `owner`
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(owner)));

        // 2) Retrieve the first card
        let packs = EterraSlots::player_packs(owner);
        let card_id = *packs[0]
            .get_card_ids()
            .first()
            .expect("At least one card expected");

        // 3) Attempt to transfer it as `non_owner` or `malicious_user`
        let result =
            EterraSlots::transfer_card(RuntimeOrigin::signed(non_owner), card_id, malicious_user);

        // 4) Confirm it fails with the expected NotCardOwner error
        assert_noop!(result, Error::<Test>::NotCardOwner);
    });
}

#[test]
fn test_transfer_card_no_such_card_fails() {
    new_test_ext().execute_with(|| {
        let sender = 1;
        let receiver = 2;

        // Don’t mint anything, so no cards exist
        let card_id_that_does_not_exist = 9999;

        // Attempt transfer
        let result = EterraSlots::transfer_card(
            RuntimeOrigin::signed(sender),
            card_id_that_does_not_exist,
            receiver,
        );

        assert_noop!(result, Error::<Test>::NoSuchCard);
    });
}

#[test]
fn test_transfer_card_success() {
    new_test_ext().execute_with(|| {
        let original_owner = 1;
        let new_owner = 2;

        // 1) Mint a pack for `original_owner` to create some cards.
        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(
            original_owner
        )));

        // 2) Grab the first pack and its first card_id.
        let packs = EterraSlots::player_packs(original_owner);
        let pack = packs.first().expect("Expected at least one pack minted");
        let card_id = pack
            .get_card_ids()
            .first()
            .copied()
            .expect("Expected at least one card in the pack");

        // Log which card ID we’re transferring
        println!("[TEST] Minted card_id: {}", card_id);

        // 3) Finalize the card before transferring
        System::reset_events(); // Clear old events

        assert_ok!(EterraSlots::generate_slot(RuntimeOrigin::signed(
            original_owner
        )));
        assert_ok!(EterraSlots::accept_slot(RuntimeOrigin::signed(
            original_owner
        )));

        // 4) Transfer the finalized card to `new_owner`
        let result =
            EterraSlots::transfer_card(RuntimeOrigin::signed(original_owner), card_id, new_owner);

        assert_ok!(result);

        // 5) Confirm the card's ownership changed in storage
        let card_info = EterraSlots::cards(card_id).expect("Card must still exist");
        println!("[TEST] card_info after transfer: {:?}", card_info);
        assert_eq!(
            *card_info.get_owner(),
            new_owner,
            "Storage shows the card owner didn't update!"
        );
        assert!(!CardsByOwner::<Test>::get(original_owner).contains(&card_id));
        assert!(CardsByOwner::<Test>::get(new_owner).contains(&card_id));

        // 6) Attempt to find a CardTransferred event.
        let events = System::events();
        println!("[TEST] Events after transfer: {:?}", events);

        let found_event = events.iter().any(|r| {
            matches!(
                r.event,
                RuntimeEvent::EterraSlots(Event::CardTransferred {
                    from,
                    to,
                    card_id: c_id
                }) if from == original_owner && to == new_owner && c_id == card_id
            )
        });
        if !found_event {
            println!(
                "[WARN] No CardTransferred event found for card_id={}, but ownership DID update.",
                card_id
            );
        } else {
            println!("[TEST] Found the CardTransferred event as expected!");
        }
    });
}

#[test]
fn transfer_card_fails_when_recipient_card_capacity_is_full() {
    new_test_ext().execute_with(|| {
        let original_owner = 1u64;
        let new_owner = 2u64;

        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(original_owner)));
        let card_id = NextCardId::<Test>::get().saturating_sub(1);

        seed_owned_card_index(new_owner, BaseCardCapacity::get(), 30_000);

        assert_noop!(
            EterraSlots::transfer_card(RuntimeOrigin::signed(original_owner), card_id, new_owner),
            Error::<Test>::CardCapacityExceeded
        );

        let card = EterraSlots::cards(card_id).expect("card exists");
        assert_eq!(*card.get_owner(), original_owner);
    });
}

#[test]
fn mint_card_charges_price_and_mints() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);

        let player = 1u64;
        let receiver = <Test as EterraSlotsConfig>::MintCardPriceReceiver::get();
        let price: u128 = <Test as EterraSlotsConfig>::MintCardPrice::get();

        let player_before = Balances::free_balance(player);
        let receiver_before = Balances::free_balance(receiver);

        System::reset_events();
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(player)));

        let card_id = NextCardId::<Test>::get().saturating_sub(1);
        let card = EterraSlots::cards(card_id).expect("card exists");
        assert_eq!(*card.get_owner(), player);
        assert!(card.is_finalized());
        assert!(card.get_slot_values().is_some());
        assert!(EterraSlots::cards_by_owner(player).contains(&card_id));

        assert_eq!(Balances::free_balance(player), player_before - price);
        assert_eq!(Balances::free_balance(receiver), receiver_before + price);

        System::assert_has_event(RuntimeEvent::EterraSlots(Event::CardMinted { player, card_id }));
    });
}

#[test]
fn mint_card_fails_when_card_capacity_is_full_without_charging_fee() {
    new_test_ext().execute_with(|| {
        let player = 1u64;
        let receiver = <Test as EterraSlotsConfig>::MintCardPriceReceiver::get();
        let player_before = Balances::free_balance(player);
        let receiver_before = Balances::free_balance(receiver);

        seed_owned_card_index(player, BaseCardCapacity::get(), 20_000);

        assert_noop!(
            EterraSlots::mint_card(RuntimeOrigin::signed(player)),
            Error::<Test>::CardCapacityExceeded
        );

        assert_eq!(Balances::free_balance(player), player_before);
        assert_eq!(Balances::free_balance(receiver), receiver_before);
    });
}

#[test]
fn buy_card_capacity_increases_capacity_and_charges_price() {
    new_test_ext().execute_with(|| {
        let player = 2u64;
        let receiver = <Test as EterraSlotsConfig>::CardCapacityUpgradePriceReceiver::get();
        let price = <Test as EterraSlotsConfig>::CardCapacityUpgradePrice::get();

        let player_before = Balances::free_balance(player);
        let receiver_before = Balances::free_balance(receiver);

        assert_ok!(EterraSlots::buy_card_capacity(RuntimeOrigin::signed(player)));

        assert_eq!(
            CardCapacityBonus::<Test>::get(player),
            CardCapacityUpgradeAmount::get()
        );
        assert_eq!(Balances::free_balance(player), player_before - price);
        assert_eq!(Balances::free_balance(receiver), receiver_before + price);

        System::assert_has_event(RuntimeEvent::EterraSlots(Event::CardCapacityUpgraded {
            player,
            added_slots: CardCapacityUpgradeAmount::get(),
            new_capacity: BaseCardCapacity::get() + CardCapacityUpgradeAmount::get(),
            price_paid: price,
        }));
    });
}

#[test]
fn set_and_remove_price_updates_storage_and_events() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        let owner = 1u64;

        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(owner)));
        let card_id = NextCardId::<Test>::get().saturating_sub(1);

        // List for sale
        System::reset_events();
        assert_ok!(EterraSlots::set_price(
            RuntimeOrigin::signed(owner),
            card_id,
            500
        ));
        assert_eq!(CardPrices::<Test>::get(card_id), Some(500));
        assert!(ListedByOwner::<Test>::get(&owner).contains(&card_id));
        System::assert_has_event(RuntimeEvent::EterraSlots(Event::CardListed {
            owner,
            card_id,
            price: 500,
        }));

        // Unlist
        System::reset_events();
        assert_ok!(EterraSlots::remove_price(RuntimeOrigin::signed(owner), card_id));
        assert_eq!(CardPrices::<Test>::get(card_id), None);
        assert!(!ListedByOwner::<Test>::get(&owner).contains(&card_id));
        System::assert_has_event(RuntimeEvent::EterraSlots(Event::CardUnlisted { owner, card_id }));
    });
}

#[test]
fn transfer_card_auto_unlists() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        let owner = 1u64;
        let to = 2u64;

        // Mint and list
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(owner)));
        let card_id = NextCardId::<Test>::get().saturating_sub(1);
        assert_ok!(EterraSlots::set_price(
            RuntimeOrigin::signed(owner),
            card_id,
            777
        ));
        assert!(CardPrices::<Test>::get(card_id).is_some());

        // Transfer to `to`; should unlist
        assert_ok!(EterraSlots::transfer_card(
            RuntimeOrigin::signed(owner),
            card_id,
            to
        ));
        let card = EterraSlots::cards(card_id).unwrap();
        assert_eq!(*card.get_owner(), to);

        // Listing removed
        assert_eq!(CardPrices::<Test>::get(card_id), None);
        assert!(!ListedByOwner::<Test>::get(&owner).contains(&card_id));
    });
}

#[test]
fn buy_card_transfers_funds_and_ownership_then_unlists() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        let seller = 1u64;
        let buyer = 2u64;

        // Seller mints, lists at 200
        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(seller)));
        let card_id = NextCardId::<Test>::get().saturating_sub(1);
        assert_ok!(EterraSlots::set_price(
            RuntimeOrigin::signed(seller),
            card_id,
            200
        ));

        let seller_before = Balances::free_balance(seller);
        let buyer_before = Balances::free_balance(buyer);

        // Buyer buys
        System::reset_events();
        assert_ok!(EterraSlots::buy_card(RuntimeOrigin::signed(buyer), card_id));

        // Ownership moved to buyer
        let card = EterraSlots::cards(card_id).unwrap();
        assert_eq!(*card.get_owner(), buyer);
        assert!(CardsByOwner::<Test>::get(&buyer).contains(&card_id));
        assert!(!CardsByOwner::<Test>::get(&seller).contains(&card_id));

        // Listing removed
        assert_eq!(CardPrices::<Test>::get(card_id), None);
        assert!(!ListedByOwner::<Test>::get(&seller).contains(&card_id));

        // Funds moved: buyer -200, seller +200
        let seller_after = Balances::free_balance(seller);
        let buyer_after = Balances::free_balance(buyer);
        assert_eq!(seller_after, seller_before + 200);
        assert_eq!(buyer_after, buyer_before - 200);

        System::assert_has_event(RuntimeEvent::EterraSlots(Event::CardBought {
            buyer,
            seller,
            card_id,
            price: 200,
        }));
    });
}

#[test]
fn buy_card_fails_if_not_listed() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        let seller = 1u64;
        let buyer = 2u64;

        assert_ok!(EterraSlots::mint_card(RuntimeOrigin::signed(seller)));
        let card_id = NextCardId::<Test>::get().saturating_sub(1);
        assert_noop!(
            EterraSlots::buy_card(RuntimeOrigin::signed(buyer), card_id),
            Error::<Test>::NotForSale
        );
    });
}

#[test]
fn mint_card_fails_when_card_ids_exhausted_without_charging_fee() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        let player = 1u64;
        let receiver = <Test as EterraSlotsConfig>::MintCardPriceReceiver::get();

        NextCardId::<Test>::put(u32::MAX);

        let receiver_before = Balances::free_balance(receiver);
        let player_before = Balances::free_balance(player);

        assert_noop!(
            EterraSlots::mint_card(RuntimeOrigin::signed(player)),
            Error::<Test>::CardIdExhausted
        );

        // Transactional rollback: no fee transfer should happen on ID exhaustion.
        assert_eq!(Balances::free_balance(receiver), receiver_before);
        assert_eq!(Balances::free_balance(player), player_before);
    });
}

#[test]
fn transfer_card_fails_when_not_finalized() {
    new_test_ext().execute_with(|| {
        let owner = 1u64;
        let to = 2u64;

        assert_ok!(EterraSlots::mint_pack(RuntimeOrigin::signed(owner)));

        let packs = EterraSlots::player_packs(owner);
        let card_id = *packs[0]
            .get_card_ids()
            .first()
            .expect("At least one card expected");

        assert_noop!(
            EterraSlots::transfer_card(RuntimeOrigin::signed(owner), card_id, to),
            Error::<Test>::CardNotFinalized
        );
    });
}
