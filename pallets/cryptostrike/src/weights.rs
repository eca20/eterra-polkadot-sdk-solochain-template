#![allow(missing_docs)]

use frame_support::weights::Weight;

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

impl WeightInfo for () {
    fn claim_pending_guap() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn register_server() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn increase_server_stake() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn request_unstake() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn finalize_unstake() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn heartbeat() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn authorize_server_allowance() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn revoke_server_allowance() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn submit_round_settlement() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn start_season() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn end_season() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn set_server_status() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn slash_server() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn set_session_roster_root() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn upsert_active_player() -> Weight {
        Weight::from_parts(10_000, 0)
    }

    fn remove_active_player() -> Weight {
        Weight::from_parts(10_000, 0)
    }
}
