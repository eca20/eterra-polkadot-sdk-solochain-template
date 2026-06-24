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
		Weight::from_parts(10_000, 0)
	}
	fn pay_continue() -> Weight {
		Weight::from_parts(10_000, 0)
	}
	fn submit_result() -> Weight {
		Weight::from_parts(10_000, 0)
	}
}

pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn start_run() -> Weight {
		Weight::from_parts(28_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(4))
			.saturating_add(T::DbWeight::get().writes(4))
	}
	fn pay_continue() -> Weight {
		Weight::from_parts(26_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(2))
			.saturating_add(T::DbWeight::get().writes(1))
	}
	fn submit_result() -> Weight {
		Weight::from_parts(38_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(6))
			.saturating_add(T::DbWeight::get().writes(6))
	}
}
