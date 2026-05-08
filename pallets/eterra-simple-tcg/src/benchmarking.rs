#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::traits::Currency;
use frame_system::RawOrigin;
use sp_runtime::traits::Saturating;
use sp_runtime::DispatchError;

fn fund<T: Config>(who: &T::AccountId, amount: BalanceOf<T>) {
    let _ = T::Currency::deposit_creating(who, amount);
}

fn seed_card<T: Config>(owner: &T::AccountId, card_id: CardId) {
    let info = CardInfo::<T> {
        owner: owner.clone(),
        finalized: true,
        slot_values: Some([1, 1, 1, 1]),
        name: BoundedVec::default(),
        north: 1,
        east: 1,
        south: 1,
        west: 1,
        card_id,
        minted_at: frame_system::Pallet::<T>::block_number(),
        price: 0u128,
        edition: CardEdition::Base,
        rarity: RarityType::Common,
    };
    Cards::<T>::insert(card_id, info);
    OwnedCards::<T>::try_mutate(owner, |list| -> Result<(), DispatchError> {
        list.try_push(card_id)
            .map_err(|_| Error::<T>::OwnedListFull)?;
        Ok(())
    })
    .expect("owned list not full; qed");
}

fn list_card<T: Config>(owner: &T::AccountId, card_id: CardId, price: BalanceOf<T>) {
    CardPrices::<T>::insert(card_id, price);
    ListedByOwner::<T>::try_mutate(owner, |list| -> Result<(), DispatchError> {
        if !list.iter().any(|id| *id == card_id) {
            list.try_push(card_id)
                .map_err(|_| Error::<T>::OwnedListFull)?;
        }
        Ok(())
    })
    .expect("listed-by-owner not full; qed");
}

benchmarks! {
    mint_card {
        let caller: T::AccountId = whitelisted_caller();
        let fee = T::MintFee::get();
        let min = T::Currency::minimum_balance();
        let endowment = fee.saturating_add(min).saturating_add(min);
        fund::<T>(&caller, endowment);
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        let owned = OwnedCards::<T>::get(&caller);
        assert!(!owned.is_empty());
    }

    transfer_card {
        let from: T::AccountId = whitelisted_caller();
        let to: T::AccountId = account("to", 0, 0);
        seed_card::<T>(&from, 0);
        list_card::<T>(&from, 0, T::MintFee::get());
    }: _(RawOrigin::Signed(from.clone()), 0, to.clone())
    verify {
        let card = Cards::<T>::get(0).expect("card exists");
        assert_eq!(card.owner, to);
    }

    set_price {
        let owner: T::AccountId = whitelisted_caller();
        seed_card::<T>(&owner, 0);
        let price = T::MintFee::get();
    }: _(RawOrigin::Signed(owner.clone()), 0, price)
    verify {
        assert_eq!(CardPrices::<T>::get(0), Some(price));
    }

    remove_price {
        let owner: T::AccountId = whitelisted_caller();
        let price = T::MintFee::get();
        seed_card::<T>(&owner, 0);
        list_card::<T>(&owner, 0, price);
    }: _(RawOrigin::Signed(owner.clone()), 0)
    verify {
        assert!(CardPrices::<T>::get(0).is_none());
    }

    buy_card {
        let seller: T::AccountId = whitelisted_caller();
        let buyer: T::AccountId = account("buyer", 0, 0);
        let price = T::MintFee::get();
        let min = T::Currency::minimum_balance();
        let endowment = price.saturating_add(min).saturating_add(min);
        fund::<T>(&buyer, endowment);
        seed_card::<T>(&seller, 0);
        list_card::<T>(&seller, 0, price);
    }: _(RawOrigin::Signed(buyer.clone()), 0)
    verify {
        let card = Cards::<T>::get(0).expect("card exists");
        assert_eq!(card.owner, buyer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_benchmarking::impl_benchmark_test_suite;

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
