use borsh::{BorshDeserialize, BorshSerialize};
use codec::{Decode, Encode};
use scale_info::{
	TypeInfo,
	prelude::string::String
};
use sp_runtime::RuntimeDebug;
use sp_std::prelude::*;

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub enum PayloadType {
	Lock,
	BurnAsset,
	PlanNewEra,
	EraPayout,
	LockNft,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct LockPayload {
	pub sender: String,
	pub receiver_id: String,
	pub amount: u128,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct BurnAssetPayload {
	pub token_id: String,
	pub sender: String,
	pub receiver_id: String,
	pub amount: u128,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct PlanNewEraPayload {
	pub new_era: u32,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct EraPayoutPayload {
	pub end_era: u32,
	pub excluded_validators: Vec<String>,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct LockNftPayload {
	pub sender: String,
	pub receiver_id: String,
	pub class: u128,
	pub instance: u128,
	pub metadata: Nep171TokenMetadata,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Nep171TokenMetadata {
	// ex. "Arch Nemesis: Mail Carrier" or "Parcel #5055"
	pub title: Option<String>,
	// free-form description
	pub description: Option<String>,
	// URL to associated media, preferably to decentralized, content-addressed storage
	pub media: Option<String>,
	// Base64-encoded sha256 hash of content referenced by the `media` field. Required if `media`
	// is included.
	pub media_hash: Option<Vec<u8>>,
	// number of copies of this set of metadata in existence when token was minted.
	pub copies: Option<u64>,
	// ISO 8601 datetime when token was issued or minted
	pub issued_at: Option<String>,
	// ISO 8601 datetime when token expires
	pub expires_at: Option<String>,
	// ISO 8601 datetime when token starts being valid
	pub starts_at: Option<String>,
	// ISO 8601 datetime when token was last updated
	pub updated_at: Option<String>,
	// anything extra the NFT wants to store on-chain. Can be stringified JSON.
	pub extra: Option<String>,
	// URL to an off-chain JSON file with more info.
	pub reference: Option<String>,
	// Base64-encoded sha256 hash of JSON from reference field. Required if `reference` is
	// included.
	pub reference_hash: Option<Vec<u8>>,
}
