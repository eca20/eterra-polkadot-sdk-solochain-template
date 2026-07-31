#![allow(missing_docs)]

use core::marker::PhantomData;
use frame_support::{traits::Get, weights::Weight};

pub trait WeightInfo {
    fn claim_pending_guap() -> Weight;
    fn register_server() -> Weight;
    fn increase_server_stake() -> Weight;
    fn request_unstake() -> Weight;
    fn finalize_unstake() -> Weight;
    fn heartbeat() -> Weight;
    fn authorize_server_allowance() -> Weight;
    fn revoke_server_allowance() -> Weight;
    fn submit_round_settlement() -> Weight;
    fn start_season() -> Weight;
    fn end_season() -> Weight;
    fn set_server_status() -> Weight;
    fn slash_server() -> Weight;
    fn set_session_roster_root() -> Weight;
    fn upsert_active_player() -> Weight;
    fn remove_active_player() -> Weight;
}

/// Conservative private-alpha weights for the legacy CryptoStrike surface.
/// Production remains blocked on regenerated hardware benchmarks.
pub struct SubstrateWeight<T>(PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    fn claim_pending_guap() -> Weight {
        Weight::from_parts(100_000_000_000, 32_768)
            .saturating_add(T::DbWeight::get().reads(4))
            .saturating_add(T::DbWeight::get().writes(3))
    }

    fn register_server() -> Weight {
        Weight::from_parts(200_000_000_000, 65_536)
            .saturating_add(T::DbWeight::get().reads(8))
            .saturating_add(T::DbWeight::get().writes(6))
    }

    fn increase_server_stake() -> Weight {
        Weight::from_parts(150_000_000_000, 49_152)
            .saturating_add(T::DbWeight::get().reads(6))
            .saturating_add(T::DbWeight::get().writes(4))
    }

    fn request_unstake() -> Weight {
        Weight::from_parts(100_000_000_000, 32_768)
            .saturating_add(T::DbWeight::get().reads(4))
            .saturating_add(T::DbWeight::get().writes(3))
    }

    fn finalize_unstake() -> Weight {
        Weight::from_parts(200_000_000_000, 65_536)
            .saturating_add(T::DbWeight::get().reads(8))
            .saturating_add(T::DbWeight::get().writes(8))
    }

    fn heartbeat() -> Weight {
        Weight::from_parts(50_000_000_000, 16_384)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(1))
    }

    fn authorize_server_allowance() -> Weight {
        Weight::from_parts(75_000_000_000, 32_768)
            .saturating_add(T::DbWeight::get().reads(5))
            .saturating_add(T::DbWeight::get().writes(1))
    }

    fn revoke_server_allowance() -> Weight {
        Weight::from_parts(50_000_000_000, 16_384)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(1))
    }

    fn submit_round_settlement() -> Weight {
        // Fixed at the worst configured 64-entry bounds across each settlement
        // vector, including signature verification and all economic mutations.
        Weight::from_parts(1_200_000_000_000, 2_000_000)
            .saturating_add(T::DbWeight::get().reads(512))
            .saturating_add(T::DbWeight::get().writes(384))
    }

    fn start_season() -> Weight {
        Weight::from_parts(100_000_000_000, 32_768)
            .saturating_add(T::DbWeight::get().reads(4))
            .saturating_add(T::DbWeight::get().writes(3))
    }

    fn end_season() -> Weight {
        Weight::from_parts(100_000_000_000, 32_768)
            .saturating_add(T::DbWeight::get().reads(4))
            .saturating_add(T::DbWeight::get().writes(3))
    }

    fn set_server_status() -> Weight {
        Weight::from_parts(50_000_000_000, 16_384)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(1))
    }

    fn slash_server() -> Weight {
        Weight::from_parts(200_000_000_000, 65_536)
            .saturating_add(T::DbWeight::get().reads(8))
            .saturating_add(T::DbWeight::get().writes(8))
    }

    fn set_session_roster_root() -> Weight {
        Weight::from_parts(75_000_000_000, 32_768)
            .saturating_add(T::DbWeight::get().reads(3))
            .saturating_add(T::DbWeight::get().writes(1))
    }

    fn upsert_active_player() -> Weight {
        Weight::from_parts(150_000_000_000, 65_536)
            .saturating_add(T::DbWeight::get().reads(8))
            .saturating_add(T::DbWeight::get().writes(6))
    }

    fn remove_active_player() -> Weight {
        Weight::from_parts(100_000_000_000, 49_152)
            .saturating_add(T::DbWeight::get().reads(6))
            .saturating_add(T::DbWeight::get().writes(5))
    }
}

impl WeightInfo for () {
    fn claim_pending_guap() -> Weight {
        Weight::from_parts(100_000_000_000, 32_768)
    }

    fn register_server() -> Weight {
        Weight::from_parts(200_000_000_000, 65_536)
    }

    fn increase_server_stake() -> Weight {
        Weight::from_parts(150_000_000_000, 49_152)
    }

    fn request_unstake() -> Weight {
        Weight::from_parts(100_000_000_000, 32_768)
    }

    fn finalize_unstake() -> Weight {
        Weight::from_parts(200_000_000_000, 65_536)
    }

    fn heartbeat() -> Weight {
        Weight::from_parts(50_000_000_000, 16_384)
    }

    fn authorize_server_allowance() -> Weight {
        Weight::from_parts(75_000_000_000, 32_768)
    }

    fn revoke_server_allowance() -> Weight {
        Weight::from_parts(50_000_000_000, 16_384)
    }

    fn submit_round_settlement() -> Weight {
        Weight::from_parts(1_200_000_000_000, 2_000_000)
    }

    fn start_season() -> Weight {
        Weight::from_parts(100_000_000_000, 32_768)
    }

    fn end_season() -> Weight {
        Weight::from_parts(100_000_000_000, 32_768)
    }

    fn set_server_status() -> Weight {
        Weight::from_parts(50_000_000_000, 16_384)
    }

    fn slash_server() -> Weight {
        Weight::from_parts(200_000_000_000, 65_536)
    }

    fn set_session_roster_root() -> Weight {
        Weight::from_parts(75_000_000_000, 32_768)
    }

    fn upsert_active_player() -> Weight {
        Weight::from_parts(150_000_000_000, 65_536)
    }

    fn remove_active_player() -> Weight {
        Weight::from_parts(100_000_000_000, 49_152)
    }
}
