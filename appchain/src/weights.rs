// This file is part of Substrate.

// Copyright (C) 2021 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Autogenerated weights for pallet_octopus_appchain
//!
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 4.0.0-dev
//! DATE: 2021-12-06, STEPS: `20`, REPEAT: 10, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! EXECUTION: Some(Wasm), WASM-EXECUTION: Compiled, CHAIN: Some("dev"), DB CACHE: 128

// Executed Command:
// ./target/debug/node-template
// benchmark
// --chain
// dev
// --execution
// wasm
// --wasm-execution
// compiled
// --pallet
// pallet_octopus_appchain
// --extrinsic
// *
// --steps
// 20
// --repeat
// 10
// --raw
// --output
// ./bin/node-template/octopus-pallets/appchain/src/weights.rs
// --template
// ./.maintain/frame-weight-template.hbs


#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use sp_std::marker::PhantomData;

/// Weight functions needed for pallet_octopus_appchain.
pub trait WeightInfo {
	fn force_set_is_activated() -> Weight;
	fn force_set_next_set_id(b: u32, ) -> Weight;
	fn force_set_planned_validators(b: u32, ) -> Weight;
	fn lock() -> Weight;
}

/// Weights for pallet_octopus_appchain using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	// Storage: OctopusAppchain IsActivated (r:0 w:1)
	fn force_set_is_activated() -> Weight {
		(83_976_000 as Weight)
			.saturating_add(T::DbWeight::get().writes(1 as Weight))
	}
	// Storage: OctopusAppchain NextSetId (r:0 w:1)
	fn force_set_next_set_id(_b: u32, ) -> Weight {
		(88_980_000 as Weight)
			.saturating_add(T::DbWeight::get().writes(1 as Weight))
	}
	// Storage: OctopusAppchain PlannedValidators (r:0 w:1)
	fn force_set_planned_validators(b: u32, ) -> Weight {
		(103_462_000 as Weight)
			// Standard Error: 17_000
			.saturating_add((32_000 as Weight).saturating_mul(b as Weight))
			.saturating_add(T::DbWeight::get().writes(1 as Weight))
	}
	// Storage: OctopusAppchain IsActivated (r:1 w:0)
	// Storage: OctopusUpwardMessages MessageQueue (r:1 w:1)
	// Storage: OctopusUpwardMessages Nonce (r:1 w:1)
	fn lock() -> Weight {
		(1_161_121_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(3 as Weight))
			.saturating_add(T::DbWeight::get().writes(2 as Weight))
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	// Storage: OctopusAppchain IsActivated (r:0 w:1)
	fn force_set_is_activated() -> Weight {
		(83_976_000 as Weight)
			.saturating_add(RocksDbWeight::get().writes(1 as Weight))
	}
	// Storage: OctopusAppchain NextSetId (r:0 w:1)
	fn force_set_next_set_id(_b: u32, ) -> Weight {
		(88_980_000 as Weight)
			.saturating_add(RocksDbWeight::get().writes(1 as Weight))
	}
	// Storage: OctopusAppchain PlannedValidators (r:0 w:1)
	fn force_set_planned_validators(b: u32, ) -> Weight {
		(103_462_000 as Weight)
			// Standard Error: 17_000
			.saturating_add((32_000 as Weight).saturating_mul(b as Weight))
			.saturating_add(RocksDbWeight::get().writes(1 as Weight))
	}
	// Storage: OctopusAppchain IsActivated (r:1 w:0)
	// Storage: OctopusUpwardMessages MessageQueue (r:1 w:1)
	// Storage: OctopusUpwardMessages Nonce (r:1 w:1)
	fn lock() -> Weight {
		(1_161_121_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(3 as Weight))
			.saturating_add(RocksDbWeight::get().writes(2 as Weight))
	}
}
