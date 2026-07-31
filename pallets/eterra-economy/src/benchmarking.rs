#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{account, benchmarks};
use frame_system::pallet_prelude::BlockNumberFor;
use frame_system::RawOrigin;
use sp_runtime::traits::{Hash as HashT, SaturatedConversion};

const BENCH_GAME: GameId = 1;
const BENCH_PRODUCT: ProductId = 1;
const BENCH_ENTITLEMENT: EntitlementId = 1;
const BENCH_CREDIT: CreditTypeId = 1;
const BENCH_AMOUNT: u64 = 10;
const BENCH_PRICE: Balance = 500;
const BENCH_ARCADE_PACK_CREDIT_SKU: SkuId = 7_001;
const BENCH_PACK_SKU: u32 = 1;
const BENCH_PACK_SKU_VERSION: u32 = 1;

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

fn arcade_pack_credit_sku<T: Config>() -> ArcadePackCreditSkuV2<BlockNumberFor<T>> {
    ArcadePackCreditSkuV2 {
        pack_sku: BENCH_PACK_SKU,
        pack_sku_version: BENCH_PACK_SKU_VERSION,
        economic_realm: EconomicRealm::Training,
        ticket_price: 20,
        policy_version: 1,
        enabled: true,
        total_cap: Some(100),
        per_account_window_cap: 10,
        window_blocks: 100u32.saturated_into(),
        config_version: 1,
    }
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

    upsert_arcade_pack_credit_sku_v2 {
        T::PackCreditIssuer::prepare_benchmark_target(
            BENCH_PACK_SKU,
            BENCH_PACK_SKU_VERSION,
            EconomicRealm::Training,
        );
        let sku = arcade_pack_credit_sku::<T>();
    }: _(RawOrigin::Root, BENCH_ARCADE_PACK_CREDIT_SKU, sku)
    verify {
        assert!(ArcadePackCreditSkusV2::<T>::contains_key(BENCH_ARCADE_PACK_CREDIT_SKU));
    }

    redeem_arcade_pack_credit_with_tickets_v2 {
        let player: T::AccountId = account("arcade-pack-credit-player", 0, 0);
        let redemption_id = [0xA5; 32];
        T::AccountEligibility::prepare_benchmark_account(&player);
        T::PackCreditIssuer::prepare_benchmark_target(
            BENCH_PACK_SKU,
            BENCH_PACK_SKU_VERSION,
            EconomicRealm::Training,
        );
        TicketAsset::<T>::put(TicketAssetConfig { asset_id: 3, config_version: 1 });
        T::TicketAssets::mint(3, &player, 20)?;
        PausedDomains::<T>::insert(PauseDomain::PackCreditRedemptionV2, false);
        ArcadePackCreditSkusV2::<T>::insert(
            BENCH_ARCADE_PACK_CREDIT_SKU,
            arcade_pack_credit_sku::<T>(),
        );
    }: _(
        RawOrigin::Signed(player.clone()),
        BENCH_ARCADE_PACK_CREDIT_SKU,
        1,
        redemption_id
    )
    verify {
        assert!(ArcadePackCreditRedemptionReceiptsV2::<T>::contains_key(redemption_id));
        assert_eq!(ArcadePackCreditSkuSoldV2::<T>::get(BENCH_ARCADE_PACK_CREDIT_SKU), 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_benchmarking::impl_benchmark_test_suite;

    impl_benchmark_test_suite!(Pallet, crate::tests::new_test_ext(), crate::tests::Test);
}
