use super::*;
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::traits::{Currency, Get};
use frame_system::pallet_prelude::BlockNumberFor;
use frame_system::RawOrigin;
use parity_scale_codec::Encode;
use sp_core::crypto::KeyTypeId;
use sp_runtime::traits::{One, Saturating};
use sp_std::vec;
use sp_std::vec::Vec;

type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

fn fund<T: Config>(who: &T::AccountId, amount: BalanceOf<T>) {
    let _ = T::Currency::deposit_creating(who, amount);
}

fn short_bytes(len: u32, byte: u8) -> Vec<u8> {
    let len = core::cmp::max(len, 1) as usize;
    vec![byte; len]
}

fn install_linked_profile<T: Config>(account: &T::AccountId, steam_hash: SteamHash, frozen: bool) {
    AccountToSteam::<T>::insert(account, steam_hash);
    SteamToAccount::<T>::insert(steam_hash, account);
    GamerProfiles::<T>::insert(
        account,
        GamerProfile {
            linked_at: frame_system::Pallet::<T>::block_number(),
            frozen,
            freeze_reason: frozen.then_some([9; 32]),
        },
    );
}

fn steam_link_material<T: Config>(
    account: &T::AccountId,
    steam_hash: &SteamHash,
    nonce: &SteamLinkNonce,
    expires_at: &BlockNumberFor<T>,
) -> ([u8; 32], BoundedVec<u8, T::MaxSteamLinkSignatureLen>) {
    const KEY_TYPE: KeyTypeId = KeyTypeId(*b"egbm");

    let public =
        sp_io::crypto::sr25519_generate(KEY_TYPE, Some(b"//EterraGamerBenchmark".to_vec()));
    let mut payload = b"eterra:gamer:steam-link:v1".to_vec();
    account.encode_to(&mut payload);
    steam_hash.encode_to(&mut payload);
    nonce.encode_to(&mut payload);
    expires_at.encode_to(&mut payload);
    let signature = sp_io::crypto::sr25519_sign(KEY_TYPE, &public, &payload)
        .expect("benchmark key is available");
    let bounded_signature = signature
        .0
        .to_vec()
        .try_into()
        .expect("runtime accepts sr25519 signatures");
    (public.0, bounded_signature)
}

benchmarks! {
    set_gamer_tag {
        let caller: T::AccountId = whitelisted_caller();
        install_linked_profile::<T>(&caller, [1; 32], false);
        let tag: BoundedVec<u8, T::MaxTagLen> =
            short_bytes(T::MaxTagLen::get(), b't').try_into().expect("len ok");
        GamerTag::<T>::insert(&caller, tag.clone());

        let fee = T::ChangeFee::get();
        let min = T::Currency::minimum_balance();
        // Ensure faucet account exists so a small fee transfer doesn't fail below ED.
        let faucet = T::FaucetAccount::get();
        fund::<T>(&faucet, min);
        let fund_amount = fee.saturating_add(min);
        fund::<T>(&caller, fund_amount);
    }: _(RawOrigin::Signed(caller.clone()), tag)
    verify {
        assert!(GamerTag::<T>::contains_key(&caller));
    }

    set_arcade_initials {
        let caller: T::AccountId = whitelisted_caller();
        let initials: BoundedVec<u8, T::MaxInitialsLen> =
            b"AB_1".to_vec().try_into().expect("len ok");
        ArcadeInitials::<T>::insert(&caller, initials.clone());

        let fee = T::ChangeFee::get();
        let min = T::Currency::minimum_balance();
        // Ensure faucet account exists so a small fee transfer doesn't fail below ED.
        let faucet = T::FaucetAccount::get();
        fund::<T>(&faucet, min);
        let fund_amount = fee.saturating_add(min);
        fund::<T>(&caller, fund_amount);
    }: _(RawOrigin::Signed(caller.clone()), initials)
    verify {
        assert!(ArcadeInitials::<T>::contains_key(&caller));
    }

    set_avatar {
        let caller: T::AccountId = whitelisted_caller();
        install_linked_profile::<T>(&caller, [2; 32], false);
        let cid: BoundedVec<u8, T::MaxAvatarCidLen> =
            short_bytes(T::MaxAvatarCidLen::get(), b'Q').try_into().expect("len ok");
        AvatarCid::<T>::insert(&caller, cid.clone());

        let fee = T::ChangeFee::get();
        let min = T::Currency::minimum_balance();
        // Ensure faucet account exists so a small fee transfer doesn't fail below ED.
        let faucet = T::FaucetAccount::get();
        fund::<T>(&faucet, min);
        let fund_amount = fee.saturating_add(min);
        fund::<T>(&caller, fund_amount);
    }: _(RawOrigin::Signed(caller.clone()), cid)
    verify {
        assert!(AvatarCid::<T>::contains_key(&caller));
    }

    set_region {
        let caller: T::AccountId = whitelisted_caller();
        install_linked_profile::<T>(&caller, [3; 32], false);
        let region: BoundedVec<u8, T::MaxRegionCodeLen> =
            b"US".to_vec().try_into().expect("len ok");
    }: _(RawOrigin::Signed(caller.clone()), Some(region))
    verify {
        assert_eq!(RegionCode::<T>::get(&caller).map(|code| code.to_vec()), Some(b"US".to_vec()));
    }

    grant_experience {
        let target: T::AccountId = whitelisted_caller();
        let amount: u128 = 1_000;
    }: _(RawOrigin::Root, target.clone(), amount)
    verify {
        assert!(Experience::<T>::get(&target) >= amount);
    }

    redeem_levels {
        let caller: T::AccountId = whitelisted_caller();
        install_linked_profile::<T>(&caller, [4; 32], false);
        Level::<T>::insert(&caller, 0u8);
        Experience::<T>::insert(&caller, 1_000_000_000u128);
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        assert!(Level::<T>::get(&caller) > 0);
    }

    set_steam_link_authority {
        let authority_pubkey = [7; 32];
    }: _(RawOrigin::Root, authority_pubkey)
    verify {
        assert_eq!(SteamLinkAuthority::<T>::get(), Some(authority_pubkey));
    }

    link_steam {
        let caller: T::AccountId = whitelisted_caller();
        let steam_hash = [5; 32];
        let nonce = [6; 32];
        let expires_at =
            frame_system::Pallet::<T>::block_number().saturating_add(One::one());
        let (authority_pubkey, authority_signature) =
            steam_link_material::<T>(&caller, &steam_hash, &nonce, &expires_at);
        SteamLinkAuthority::<T>::put(authority_pubkey);
    }: _(
        RawOrigin::Signed(caller.clone()),
        steam_hash,
        nonce,
        expires_at,
        authority_signature
    )
    verify {
        assert_eq!(AccountToSteam::<T>::get(&caller), Some(steam_hash));
        assert_eq!(SteamToAccount::<T>::get(steam_hash), Some(caller.clone()));
        assert!(GamerProfiles::<T>::contains_key(&caller));
        assert!(UsedSteamLinkNonces::<T>::contains_key(nonce));
    }

    unlink_steam {
        let caller: T::AccountId = whitelisted_caller();
        let steam_hash = [7; 32];
        install_linked_profile::<T>(&caller, steam_hash, false);
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        assert!(!AccountToSteam::<T>::contains_key(&caller));
        assert!(!SteamToAccount::<T>::contains_key(steam_hash));
        assert!(!GamerProfiles::<T>::contains_key(&caller));
    }

    freeze_player {
        let account: T::AccountId = whitelisted_caller();
        install_linked_profile::<T>(&account, [8; 32], false);
        let reason_hash = [10; 32];
    }: _(RawOrigin::Root, account.clone(), reason_hash)
    verify {
        let profile = GamerProfiles::<T>::get(&account).expect("benchmark profile exists");
        assert!(profile.frozen);
        assert_eq!(profile.freeze_reason, Some(reason_hash));
    }

    unfreeze_player {
        let account: T::AccountId = whitelisted_caller();
        install_linked_profile::<T>(&account, [9; 32], true);
    }: _(RawOrigin::Root, account.clone())
    verify {
        let profile = GamerProfiles::<T>::get(&account).expect("benchmark profile exists");
        assert!(!profile.frozen);
        assert_eq!(profile.freeze_reason, None);
    }

    commit_legacy_progression_audit {
        let audit_hash = [11; 32];
    }: _(RawOrigin::Root, audit_hash)
    verify {
        assert_eq!(LegacyProgressionAuditHash::<T>::get(), Some(audit_hash));
    }

    publish_v2_pack_track {
        LegacyProgressionAuditHash::<T>::put([12; 32]);
        let config = PackTrackConfig {
            track_id: 1,
            pack_sku: 2,
            sku_version: 3,
            economy_version: 4,
            threshold: TRAINING_PACK_TRACK_THRESHOLD_V1,
            economic_realm: EconomicRealm::Training,
            config_hash: [13; 32],
        };
        let key = (config.pack_sku, config.sku_version, config.economy_version);
    }: _(RawOrigin::Root, config)
    verify {
        assert_eq!(V2PackTrackConfigs::<T>::get(key), Some(config));
    }

    set_v2_pack_track_activation {
        let config = PackTrackConfig {
            track_id: 1,
            pack_sku: 2,
            sku_version: 3,
            economy_version: 4,
            threshold: TRAINING_PACK_TRACK_THRESHOLD_V1,
            economic_realm: EconomicRealm::Training,
            config_hash: [14; 32],
        };
        let key = (config.pack_sku, config.sku_version, config.economy_version);
        V2PackTrackConfigs::<T>::insert(key, config);
    }: _(
        RawOrigin::Root,
        config.pack_sku,
        config.sku_version,
        config.economy_version,
        true
    )
    verify {
        assert!(V2PackTrackActivation::<T>::get(key));
    }

    allocate_player_xp {
        let c in 1 .. T::MaxPackCreditsPerAllocation::get();
        let caller: T::AccountId = whitelisted_caller();
        let economic_realm = EconomicRealm::Training;
        // Exercise the bounded advancement path. The component keeps
        // the generated WeightInfo signature compatible with the existing
        // `allocate_player_xp(credits)` contract.
        let amount = 10_000_000u128.saturating_add(u128::from(c));
        V2LifetimePlayerXp::<T>::insert(&caller, economic_realm, amount);
        V2UnallocatedPlayerXp::<T>::insert(&caller, economic_realm, amount);
        V2XpConservationByAccount::<T>::insert(
            &caller,
            economic_realm,
            V2XpConservation {
                total_granted: amount,
                advancement_allocated: 0,
                pack_allocated: 0,
            },
        );
        let target = PlayerXpTarget::PlayerAdvancement;
    }: _(
        RawOrigin::Signed(caller.clone()),
        economic_realm,
        amount,
        target
    )
    verify {
        assert_eq!(V2UnallocatedPlayerXp::<T>::get(&caller, economic_realm), 0);
        assert_eq!(
            V2PlayerAdvancementXp::<T>::get(&caller, economic_realm),
            amount
        );
        assert_eq!(V2PlayerLevel::<T>::get(&caller, economic_realm), 100);
        let conservation = V2XpConservationByAccount::<T>::get(&caller, economic_realm);
        assert_eq!(conservation.advancement_allocated, amount);
        assert_eq!(
            conservation.total_granted,
            conservation.advancement_allocated
                + conservation.pack_allocated
                + V2UnallocatedPlayerXp::<T>::get(&caller, economic_realm)
        );
    }
}
