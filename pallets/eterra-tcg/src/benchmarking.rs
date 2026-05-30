#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::BoundedVec;
use frame_system::RawOrigin;
use sp_runtime::traits::Saturating;

const BENCHMARK_SEASON_ID: SeasonId = 1;
const BENCHMARK_COLLECTION_ID: SeasonCollectionId = 0;

fn ensure_benchmark_season<T: Config>() {
    let now = <frame_system::Pallet<T>>::block_number();
    let season_name: BoundedVec<u8, <T as pallet_eterra_seasons::Config>::MaxSeasonNameLen> =
        b"Benchmark Season"
            .to_vec()
            .try_into()
            .expect("benchmark season name fits");
    let season_desc: BoundedVec<u8, <T as pallet_eterra_seasons::Config>::MaxSeasonDescLen> =
        b"Benchmark"
            .to_vec()
            .try_into()
            .expect("benchmark season description fits");

    pallet_eterra_seasons::Seasons::<T>::insert(
        BENCHMARK_SEASON_ID,
        pallet_eterra_seasons::SeasonInfo {
            name: season_name,
            description: season_desc,
            status: pallet_eterra_seasons::SeasonStatus::Active,
            created_at: now,
            activated_at: Some(now),
            closed_at: None,
        },
    );
    pallet_eterra_seasons::ActiveSeasonId::<T>::put(Some(BENCHMARK_SEASON_ID));
    pallet_eterra_seasons::NextSeasonId::<T>::mutate(|next| {
        if *next <= BENCHMARK_SEASON_ID {
            *next = BENCHMARK_SEASON_ID.saturating_add(1);
        }
    });

    let mut collection_ids: BoundedVec<SeasonCollectionId, T::MaxSeasonCollections> =
        BoundedVec::default();
    collection_ids
        .try_push(BENCHMARK_COLLECTION_ID)
        .expect("benchmark collection id fits");
    SeasonCollectionIds::<T>::insert(BENCHMARK_SEASON_ID, collection_ids);

    let collection_name: BoundedVec<u8, T::MaxSeasonCollectionNameLen> = b"Benchmark Set"
        .to_vec()
        .try_into()
        .expect("benchmark collection name fits");
    SeasonCollections::<T>::insert(
        BENCHMARK_SEASON_ID,
        BENCHMARK_COLLECTION_ID,
        SeasonCollectionInfo {
            name: collection_name,
            status: SeasonCollectionStatus::Published,
            created_at: now,
            published_at: Some(now),
        },
    );

    let mut assets = SeasonCollectionAssets::<T>::get(BENCHMARK_SEASON_ID, BENCHMARK_COLLECTION_ID);
    assets.borders = BoundedVec::try_from(sp_std::vec![0]).expect("benchmark border fits");
    assets.backgrounds = BoundedVec::try_from(sp_std::vec![1]).expect("benchmark background fits");
    assets.subjects = BoundedVec::try_from(sp_std::vec![2]).expect("benchmark subject fits");
    assets.backs = BoundedVec::try_from(sp_std::vec![3]).expect("benchmark back fits");
    assets.packaging_fronts =
        BoundedVec::try_from(sp_std::vec![4]).expect("benchmark packaging front fits");
    assets.packaging_backs =
        BoundedVec::try_from(sp_std::vec![5]).expect("benchmark packaging back fits");
    SeasonCollectionAssets::<T>::insert(BENCHMARK_SEASON_ID, BENCHMARK_COLLECTION_ID, assets);
}

fn fund<T: Config>(who: &T::AccountId) {
    ensure_benchmark_season::<T>();

    // Ensure the caller can pay either price and still satisfy `KeepAlive`.
    let pack_price = T::PackPrice::get();
    let pro_price = T::ProPrice::get();
    let mint_price = T::MintCardPrice::get();
    let storage_price = T::CardCapacityUpgradePrice::get();
    let amount = pack_price
        .saturating_add(pack_price)
        .saturating_add(pro_price)
        .saturating_add(pro_price)
        .saturating_add(mint_price)
        .saturating_add(mint_price)
        .saturating_add(storage_price)
        .saturating_add(storage_price);
    let _ = T::PaymentCurrency::deposit_creating(who, amount);
}

fn setup_finalized_card<T: Config>(player: &T::AccountId) -> u32 {
    fund::<T>(player);
    Pallet::<T>::mint_card(RawOrigin::Signed(player.clone()).into()).expect("mint card succeeds");
    // NextCardId is incremented after minting; the minted id is previous value.
    NextCardId::<T>::get().saturating_sub(1)
}

fn active_card_id<T: Config>(player: &T::AccountId) -> u32 {
    let packs = PlayerPacks::<T>::get(player);
    let pack = packs.last().expect("pack exists");
    let idx = ActiveCard::<T>::get(player).expect("active card index");
    *pack
        .get_card_ids()
        .get(idx as usize)
        .expect("card id exists")
}

fn setup_pack<T: Config>(player: &T::AccountId) -> u32 {
    fund::<T>(player);
    Pallet::<T>::mint_pack(RawOrigin::Signed(player.clone()).into()).expect("mint pack succeeds");
    active_card_id::<T>(player)
}

fn setup_generated_slot<T: Config>(player: &T::AccountId) -> u32 {
    let card_id = setup_pack::<T>(player);
    Pallet::<T>::generate_slot(RawOrigin::Signed(player.clone()).into())
        .expect("generate slot succeeds");
    card_id
}

fn setup_pro<T: Config>(player: &T::AccountId) -> u32 {
    fund::<T>(player);
    Pallet::<T>::mint_pro(RawOrigin::Signed(player.clone()).into()).expect("mint pro succeeds");
    ProInProgress::<T>::get(player).expect("pro in progress")
}

fn sample_progression_node() -> ProgressionNode {
    ProgressionNode {
        node_id: 1,
        node_kind: ProgressionNodeKind::Weapon,
        required_level: 1,
        required_item_template_id: 77,
        gear_slot_type: Some(GearSlotType::Weapon),
        power_delta: 5,
        config_version: 1,
    }
}

fn setup_progression_tree<T: Config>() {
    Pallet::<T>::set_progression_tree(
        RawOrigin::Root.into(),
        1,
        2,
        None,
        sp_std::vec![sample_progression_node()],
        1,
    )
    .expect("set progression tree succeeds");
}

fn setup_progression_card<T: Config>(player: &T::AccountId) -> u32 {
    setup_progression_tree::<T>();
    let card_id = setup_finalized_card::<T>(player);
    if CardProgressions::<T>::get(card_id).is_none() {
        Pallet::<T>::assign_progression_tree_to_card(RawOrigin::Root.into(), card_id, 1)
            .expect("assign progression succeeds");
    }
    card_id
}

fn setup_progression_gear<T: Config>(owner: &T::AccountId, gear_id: GearId) {
    NexusGearItems::<T>::insert(
        gear_id,
        GearItem {
            owner: owner.clone(),
            gear_id,
            slot_type: GearSlotType::Weapon,
            tier: GearTier::Basic,
            power: 1,
            spell_slots: BoundedVec::<SpellSlotEntry, T::MaxNexusSpellSlotsPerCard>::default(),
            equipped_card_id: None,
            season_id: BENCHMARK_SEASON_ID,
            config_version: 1,
        },
    );
    GearItemTemplates::<T>::insert(gear_id, 77);
}

fn setup_progression_spell<T: Config>(owner: &T::AccountId, spell_id: SpellId) {
    NexusSpellbook::<T>::insert(
        spell_id,
        SpellEntry {
            owner: owner.clone(),
            spell_id,
            element: Element::Fire,
            power: 3,
            slotted_to: None,
            config_version: 1,
        },
    );
}

benchmarks! {
    mint_pack {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        let packs = PlayerPacks::<T>::get(&caller);
        assert!(!packs.is_empty());
        assert!(PackInProgress::<T>::get(&caller).is_some());
        assert!(PackCardInProgress::<T>::get(&caller).is_some());
        assert!(CardsByOwner::<T>::get(&caller).len() > 0);
    }

    mint_pro {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        let card_id = ProInProgress::<T>::get(&caller).expect("pro in progress");
        let card = Cards::<T>::get(card_id).expect("card exists");
        assert!(card.get_slot_values().is_none());
        assert_eq!(CardAttempts::<T>::get(card_id), 0);
        assert!(CardsByOwner::<T>::get(&caller).contains(&card_id));
    }

    generate_slot {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = setup_pack::<T>(&caller);
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        assert!(CardAttempts::<T>::get(card_id) > 0);
    }

    spin_pro {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = setup_pro::<T>(&caller);

        // Pre-spin up to just before the last allowed spin so the benchmarked
        // call exercises the "forced finalize" path (worst case).
        let max = T::MaxProSpins::get();
        let pre_spins = max.saturating_sub(1);
        for _ in 0..pre_spins {
            Pallet::<T>::spin_pro(RawOrigin::Signed(caller.clone()).into()).expect("pre spin succeeds");
        }
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        let card = Cards::<T>::get(card_id).expect("card exists");
        assert!(card.is_finalized());
        assert!(ProInProgress::<T>::get(&caller).is_none());
    }

    accept_slot {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = setup_generated_slot::<T>(&caller);
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        let card = Cards::<T>::get(card_id).expect("card exists");
        assert!(card.is_finalized());
    }

    accept_pro {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = setup_pro::<T>(&caller);
        Pallet::<T>::spin_pro(RawOrigin::Signed(caller.clone()).into()).expect("spin pro succeeds");
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        let card = Cards::<T>::get(card_id).expect("card exists");
        assert!(card.is_finalized());
        assert!(ProInProgress::<T>::get(&caller).is_none());
    }

    transfer_card {
        let from: T::AccountId = whitelisted_caller();
        let to: T::AccountId = account("to", 0, 0);
        let card_id = setup_generated_slot::<T>(&from);
        Pallet::<T>::accept_slot(RawOrigin::Signed(from.clone()).into())
            .expect("accept slot succeeds");
    }: _(RawOrigin::Signed(from.clone()), card_id, to.clone())
    verify {
        let card = Cards::<T>::get(card_id).expect("card exists");
        assert_eq!(card.get_owner(), &to);
        assert!(CardsByOwner::<T>::get(&to).contains(&card_id));
    }

    mint_card {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        let card_id = NextCardId::<T>::get().saturating_sub(1);
        let card = Cards::<T>::get(card_id).expect("card exists");
        assert!(card.is_finalized());
        assert!(card.get_slot_values().is_some());
        assert!(CardsByOwner::<T>::get(&caller).contains(&card_id));
    }

    set_price {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = setup_finalized_card::<T>(&caller);
        let price = T::MintCardPrice::get(); // arbitrary non-zero
    }: _(RawOrigin::Signed(caller.clone()), card_id, price)
    verify {
        assert_eq!(CardPrices::<T>::get(card_id), Some(price));
        assert!(ListedByOwner::<T>::get(&caller).contains(&card_id));
    }

    remove_price {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = setup_finalized_card::<T>(&caller);
        let price = T::MintCardPrice::get();
        Pallet::<T>::set_price(RawOrigin::Signed(caller.clone()).into(), card_id, price)
            .expect("set price succeeds");
    }: _(RawOrigin::Signed(caller.clone()), card_id)
    verify {
        assert!(CardPrices::<T>::get(card_id).is_none());
        assert!(!ListedByOwner::<T>::get(&caller).contains(&card_id));
    }

    buy_card_capacity {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        assert_eq!(
            CardCapacityBonus::<T>::get(&caller),
            T::CardCapacityUpgradeAmount::get()
        );
    }

    buy_card {
        let seller: T::AccountId = whitelisted_caller();
        let buyer: T::AccountId = account("buyer", 0, 0);
        fund::<T>(&seller);
        fund::<T>(&buyer);

        let card_id = setup_finalized_card::<T>(&seller);
        let price = T::MintCardPrice::get();
        Pallet::<T>::set_price(RawOrigin::Signed(seller.clone()).into(), card_id, price)
            .expect("set price succeeds");
    }: _(RawOrigin::Signed(buyer.clone()), card_id)
    verify {
        let card = Cards::<T>::get(card_id).expect("card exists");
        assert_eq!(card.get_owner(), &buyer);
        assert!(CardPrices::<T>::get(card_id).is_none());
    }

    set_progression_tree {
        let node = sample_progression_node();
    }: _(RawOrigin::Root, 1, 2, None, sp_std::vec![node], 1)
    verify {
        assert!(ProgressionTrees::<T>::get(1).is_some());
    }

    assign_progression_tree_to_card {
        let caller: T::AccountId = whitelisted_caller();
        setup_progression_tree::<T>();
        let card_id = setup_finalized_card::<T>(&caller);
    }: _(RawOrigin::Root, card_id, 1)
    verify {
        assert!(CardProgressions::<T>::get(card_id).is_some());
    }

    grant_card_experience {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = setup_progression_card::<T>(&caller);
    }: _(RawOrigin::Signed(caller.clone()), 10, 7, 8, card_id, 100)
    verify {
        let progression = CardProgressions::<T>::get(card_id).expect("progression exists");
        assert!(progression.experience >= 100);
        assert!(progression.level >= 2);
    }

    forge_progression_node {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = setup_progression_card::<T>(&caller);
        setup_progression_gear::<T>(&caller, 100);
    }: _(RawOrigin::Signed(caller.clone()), card_id, 1, 100)
    verify {
        assert!(CardEquipmentAttachments::<T>::get(card_id, 1).is_some());
    }

    set_card_magic_loadout {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = setup_progression_card::<T>(&caller);
        setup_progression_spell::<T>(&caller, 200);
    }: _(RawOrigin::Signed(caller.clone()), card_id, sp_std::vec![200])
    verify {
        let loadout = CardMagicLoadouts::<T>::get(card_id).expect("loadout exists");
        assert_eq!(loadout.spells.len(), 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_benchmarking::impl_benchmark_test_suite;

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
