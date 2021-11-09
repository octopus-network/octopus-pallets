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
	pub sender: String,
	pub receiver_id: String,
	pub amount: u128,
	pub current_era_number: u32, // the message is submitted in this anchor era.
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct BurnAssetPayload {
	pub token_id: String,
	pub sender: String,
	pub receiver_id: String,
	pub amount: u128,
	pub current_era_number: u32, // the message is submitted in this anchor era.
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct PlanNewEraPayload {
	pub new_era: u32,
	pub current_era_number: u32, // the message is submitted in this anchor era.
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct EraPayoutPayload {
	pub end_era: u32,
	pub excluded_validators: Vec<String>,
	pub current_era_number: u32, // the message is submitted in this anchor era.
}
