#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use borsh::{BorshDeserialize, BorshSerialize};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_std::prelude::*;

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub enum PayloadType {
	Lock,
	BurnAsset,
	PlanNewEra,
	EraPayout,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct LockPayload {
	pub sender: Vec<u8>,
	pub receiver_id: Vec<u8>,
	pub amount: u128,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct BurnAssetPayload {
	pub token_id: Vec<u8>,
	pub sender: Vec<u8>,
	pub receiver_id: Vec<u8>,
	pub amount: u128,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct PlanNewEraPayload {
	pub new_planned_era: u32,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct EraPayoutPayload {
	pub era: u32,
	pub is_payout_created: bool,
	pub exclude: Vec<String>,
}
