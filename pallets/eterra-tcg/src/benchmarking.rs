#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_system::RawOrigin;
use sp_runtime::traits::Saturating;

fn fund<T: Config>(who: &T::AccountId) {
    // Ensure the caller can pay either price and still satisfy `KeepAlive`.
    let pack_price = T::PackPrice::get();
    let pro_price = T::ProPrice::get();
    let amount = pack_price
        .saturating_add(pack_price)
        .saturating_add(pro_price)
        .saturating_add(pro_price);
    let _ = T::Currency::deposit_creating(who, amount);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_benchmarking::impl_benchmark_test_suite;

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
