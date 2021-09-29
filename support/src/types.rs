use borsh::{BorshDeserialize, BorshSerialize};
use codec::{Decode, Encode};
use sp_runtime::RuntimeDebug;
use sp_std::prelude::*;

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum PayloadType {
	Lock,
	BurnAsset,
	PlannedEraSwitch,
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
pub struct PlannedEraSwitchPayload {
	pub era: u32,
}
