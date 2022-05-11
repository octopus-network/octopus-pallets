#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use crate::equivocation_proof::EquivocationProof;
use scale_info::TypeInfo;
use serde::Serialize;
use sp_std::prelude::*;

#[derive(Serialize, Clone, Debug, TypeInfo)]
pub enum AppchainChallenge {
	EquivocationChallenge { submitter_account: String, proof: EquivocationProof },
	ConspiracyMmr { submitter_account: String, block_number: u32 },
}

#[derive(Serialize, Clone, Debug, TypeInfo)]
pub struct Challenge {
	pub appchain_challenge: AppchainChallenge,
}
