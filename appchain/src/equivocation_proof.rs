#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use serde::{Serialize, Serializer};
use sp_std::prelude::*;

use codec::{Decode, Encode};
use scale_info::TypeInfo;

pub type RoundNumber = u64;
pub type SetId = u64;
pub type BlockNumber = u32;

#[derive(Serialize, Clone, Debug, Decode, Encode, TypeInfo)]
pub struct Hash(pub [u8; 32]);

#[derive(Serialize, Clone, Debug, Decode, Encode, TypeInfo)]
pub struct PublicKey(pub [u8; 32]);

#[derive(Clone, Debug, Decode, Encode, TypeInfo)]
pub struct SignatureData(pub [u8; 64]);

impl Serialize for SignatureData {
	fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
	where
		S: Serializer,
	{
		serializer.serialize_bytes(&self.0)
	}
}

#[derive(Serialize, Clone, Debug, Decode, Encode, TypeInfo)]
pub struct VoteData {
	pub target_hash: Hash,
	pub target_number: BlockNumber,
}

#[derive(Serialize, Clone, Debug, Decode, Encode, TypeInfo)]
pub struct GrandpaEquivocation {
	pub round_number: RoundNumber,
	pub identity: PublicKey,
	pub first: (VoteData, SignatureData),
	pub second: (VoteData, SignatureData),
}

#[derive(Serialize, Clone, Debug, Decode, Encode, TypeInfo)]
pub enum Equivocation {
	Prevote(GrandpaEquivocation),
	Precommit(GrandpaEquivocation),
}

#[derive(Serialize, Clone, Debug, Decode, Encode, TypeInfo)]
pub struct EquivocationProof {
	pub set_id: SetId,
	pub equivocation: Equivocation,
}
