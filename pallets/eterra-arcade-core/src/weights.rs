#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use core::marker::PhantomData;
use frame_support::{traits::Get, weights::Weight};

pub trait WeightInfo {
	fn configure_game() -> Weight;
	fn start_run() -> Weight;
	fn abandon_run() -> Weight;
	fn expire_run() -> Weight;
	fn pay_continue() -> Weight;
}

impl WeightInfo for () {
	fn configure_game() -> Weight {
		Weight::from_parts(10_000, 0)
	}
	fn start_run() -> Weight {
		Weight::from_parts(10_000, 0)
	}
	fn abandon_run() -> Weight {
		Weight::from_parts(10_000, 0)
	}
	fn expire_run() -> Weight {
		Weight::from_parts(10_000, 0)
	}
	fn pay_continue() -> Weight {
		Weight::from_parts(10_000, 0)
	}
}

pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn configure_game() -> Weight {
		Weight::from_parts(12_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
	}
	fn start_run() -> Weight {
		Weight::from_parts(25_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(4))
			.saturating_add(T::DbWeight::get().writes(4))
	}
	fn abandon_run() -> Weight {
		Weight::from_parts(16_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(2))
	}
	fn expire_run() -> Weight {
		Weight::from_parts(16_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(2))
	}
	fn pay_continue() -> Weight {
		Weight::from_parts(25_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(2))
			.saturating_add(T::DbWeight::get().writes(1))
	}
}
