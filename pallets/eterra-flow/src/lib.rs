//! Thin Eterra compatibility adapter for Blockchainia Flow.
//!
//! The runtime keeps its historical `EterraFlow` alias, pallet index `29`,
//! storage version `2`, storage prefixes, dispatch call indices, and Manifest
//! v0 SCALE contract. The implementation is vendored from the exact standalone
//! Blockchainia Flow commit recorded in `vendor/blockchainia-flow.lock.json`.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet_blockchainia_flow::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
