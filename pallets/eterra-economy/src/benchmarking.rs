#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{account, benchmarks};
use frame_system::RawOrigin;
use sp_runtime::traits::Hash as HashT;

const BENCH_GAME: GameId = 1;
const BENCH_PRODUCT: ProductId = 1;
const BENCH_ENTITLEMENT: EntitlementId = 1;
const BENCH_CREDIT: CreditTypeId = 1;
const BENCH_AMOUNT: u64 = 10;
const BENCH_PRICE: Balance = 500;

fn metadata_hash<T: Config>() -> T::Hash {
    T::Hashing::hash(b"eterra-economy-benchmark")
}

fn receipt_hash<T: Config>() -> T::Hash {
    T::Hashing::hash(b"eterra-economy-receipt")
}

fn seed_product<T: Config>(status: ProductStatus) {
    Products::<T>::insert(
        BENCH_GAME,
        BENCH_PRODUCT,
        ProductRecord {
            product_type: ProductType::SeasonPass,
            status,
            price: BENCH_PRICE,
            grants_entitlement: Some(BENCH_ENTITLEMENT),
            grants_credit: Some((BENCH_CREDIT, BENCH_AMOUNT)),
            metadata_hash: metadata_hash::<T>(),
        },
    );
}

benchmarks! {
    create_product {
    }: _(RawOrigin::Root, BENCH_GAME, BENCH_PRODUCT, ProductType::SeasonPass, BENCH_PRICE, Some(BENCH_ENTITLEMENT), Some((BENCH_CREDIT, BENCH_AMOUNT)), metadata_hash::<T>())
    verify {
        assert!(Products::<T>::contains_key(BENCH_GAME, BENCH_PRODUCT));
    }

    set_product_status {
        seed_product::<T>(ProductStatus::Draft);
    }: _(RawOrigin::Root, BENCH_GAME, BENCH_PRODUCT, ProductStatus::Active)
    verify {
        let product = Products::<T>::get(BENCH_GAME, BENCH_PRODUCT).expect("product exists");
        assert_eq!(product.status, ProductStatus::Active);
    }

    grant_entitlement {
        let player: T::AccountId = account("player", 0, 0);
    }: _(RawOrigin::Root, BENCH_GAME, player.clone(), BENCH_ENTITLEMENT)
    verify {
        assert!(Entitlements::<T>::get((BENCH_GAME, &player, BENCH_ENTITLEMENT)));
    }

    revoke_entitlement {
        let player: T::AccountId = account("player", 0, 0);
        Entitlements::<T>::insert((BENCH_GAME, &player, BENCH_ENTITLEMENT), true);
    }: _(RawOrigin::Root, BENCH_GAME, player.clone(), BENCH_ENTITLEMENT)
    verify {
        assert!(!Entitlements::<T>::get((BENCH_GAME, &player, BENCH_ENTITLEMENT)));
    }

    grant_credit {
        let player: T::AccountId = account("player", 0, 0);
    }: _(RawOrigin::Root, BENCH_GAME, player.clone(), BENCH_CREDIT, BENCH_AMOUNT)
    verify {
        assert_eq!(Credits::<T>::get((BENCH_GAME, &player, BENCH_CREDIT)), BENCH_AMOUNT);
    }

    consume_credit {
        let player: T::AccountId = account("player", 0, 0);
        Credits::<T>::insert((BENCH_GAME, &player, BENCH_CREDIT), BENCH_AMOUNT);
    }: _(RawOrigin::Signed(player.clone()), BENCH_GAME, BENCH_CREDIT, BENCH_AMOUNT)
    verify {
        assert_eq!(Credits::<T>::get((BENCH_GAME, &player, BENCH_CREDIT)), 0);
    }

    deposit_sponsor_funds {
    }: _(RawOrigin::Root, BENCH_GAME, BENCH_PRICE)
    verify {
        assert_eq!(SponsorPools::<T>::get(BENCH_GAME), BENCH_PRICE);
    }

    record_revenue {
    }: _(RawOrigin::Root, BENCH_GAME, BENCH_PRICE)
    verify {
        assert_eq!(RevenueEscrow::<T>::get(BENCH_GAME), BENCH_PRICE);
    }

    fulfill_product {
        let player: T::AccountId = account("player", 0, 0);
        let receipt = receipt_hash::<T>();
        seed_product::<T>(ProductStatus::Active);
    }: _(RawOrigin::Root, BENCH_GAME, BENCH_PRODUCT, player.clone(), receipt)
    verify {
        assert!(Entitlements::<T>::get((BENCH_GAME, &player, BENCH_ENTITLEMENT)));
        assert_eq!(Credits::<T>::get((BENCH_GAME, &player, BENCH_CREDIT)), BENCH_AMOUNT);
        assert_eq!(RevenueEscrow::<T>::get(BENCH_GAME), BENCH_PRICE);
        assert!(FulfilledReceipts::<T>::get((BENCH_GAME, receipt)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_benchmarking::impl_benchmark_test_suite;

    impl_benchmark_test_suite!(Pallet, crate::tests::new_test_ext(), crate::tests::Test);
}
