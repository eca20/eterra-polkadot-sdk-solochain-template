#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use core::marker::PhantomData;
use frame_support::{traits::Get, weights::Weight};

pub trait WeightInfo {
	fn start_run() -> Weight;
	fn pay_continue() -> Weight;
	fn submit_result() -> Weight;
}

impl WeightInfo for () {
	fn start_run() -> Weight {
		Weight::from_parts(200_000_000_000, 524_288)
	}
	fn pay_continue() -> Weight {
		Weight::from_parts(150_000_000_000, 262_144)
	}
	fn submit_result() -> Weight {
		Weight::from_parts(800_000_000_000, 4_194_304)
	}
}

pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn start_run() -> Weight {
		// Includes ArcadeCore run creation and Economy credit consumption.
		Weight::from_parts(200_000_000_000, 524_288)
			.saturating_add(T::DbWeight::get().reads(20))
			.saturating_add(T::DbWeight::get().writes(15))
	}
	fn pay_continue() -> Weight {
		// This path is runtime-filtered for private alpha, but its declared
		// weight still covers the nested Economy credit mutation.
		Weight::from_parts(150_000_000_000, 262_144)
			.saturating_add(T::DbWeight::get().reads(16))
			.saturating_add(T::DbWeight::get().writes(12))
	}
	fn submit_result() -> Weight {
		// Includes authority checks, replay receipt, best score, the bounded
		// 32-entry leaderboard and the worst Economy/Assets reward path.
		Weight::from_parts(800_000_000_000, 4_194_304)
			.saturating_add(T::DbWeight::get().reads(128))
			.saturating_add(T::DbWeight::get().writes(64))
	}
}
