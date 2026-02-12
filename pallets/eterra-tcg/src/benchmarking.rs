#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_system::RawOrigin;

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
    Pallet::<T>::mint_pack(RawOrigin::Signed(player.clone()).into())
        .expect("mint pack succeeds");
    active_card_id::<T>(player)
}

fn setup_generated_slot<T: Config>(player: &T::AccountId) -> u32 {
    let card_id = setup_pack::<T>(player);
    Pallet::<T>::generate_slot(RawOrigin::Signed(player.clone()).into())
        .expect("generate slot succeeds");
    card_id
}

benchmarks! {
    mint_pack {
        let caller: T::AccountId = whitelisted_caller();
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        let packs = PlayerPacks::<T>::get(&caller);
        assert!(!packs.is_empty());
    }

    generate_slot {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = setup_pack::<T>(&caller);
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        assert!(CardAttempts::<T>::get(card_id) > 0);
    }

    accept_slot {
        let caller: T::AccountId = whitelisted_caller();
        let card_id = setup_generated_slot::<T>(&caller);
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        let card = Cards::<T>::get(card_id).expect("card exists");
        assert!(card.is_finalized());
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_benchmarking::impl_benchmark_test_suite;

    impl_benchmark_test_suite!(
        Pallet,
        crate::mock::new_test_ext(),
        crate::mock::Test
    );
}
