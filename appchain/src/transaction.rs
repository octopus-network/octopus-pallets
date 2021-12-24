#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use serde::Serialize;
use sp_runtime::RuntimeDebug;
use sp_std::prelude::*;

use borsh::maybestd::io::{Error, Write};
use borsh::BorshSerialize;

pub type Balance = u128;
pub type Gas = u64;
pub type Nonce = u64;

#[derive(BorshSerialize, Serialize, PartialEq, Eq, Clone, RuntimeDebug)]
pub struct CreateAccountAction {}

#[derive(BorshSerialize, Serialize, PartialEq, Eq, Clone, RuntimeDebug)]
pub struct FunctionCallAction {
	pub method_name: String,
	pub args: Vec<u8>,
	pub gas: Gas,
	pub deposit: Balance,
}

#[derive(BorshSerialize, PartialEq, Eq, Clone, RuntimeDebug)]
pub struct DeployContractAction {
	pub code: Vec<u8>,
}

#[derive(BorshSerialize, PartialEq, Eq, Clone, RuntimeDebug)]
pub struct StakeAction {
	pub stake: Balance,
	pub public_key: PublicKey,
}

#[derive(BorshSerialize, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct TransferAction {
	pub deposit: Balance,
}

#[derive(BorshSerialize, PartialEq, Eq, Clone, RuntimeDebug)]
pub struct DeleteAccountAction {
	pub beneficiary_id: String,
}

#[derive(BorshSerialize, PartialEq, Eq, Clone, RuntimeDebug)]
pub struct DeleteKeyAction {
	pub public_key: PublicKey,
}

#[derive(BorshSerialize, PartialEq, Eq, Clone, RuntimeDebug)]
pub struct AddKeyAction {}

#[derive(BorshSerialize, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum Action {
	_CreateAccount(CreateAccountAction),
	_DeployContract(DeployContractAction),
	FunctionCall(FunctionCallAction),
	_Transfer(TransferAction),
	_Stake(StakeAction),
	_AddKey(AddKeyAction),
	_DeleteKey(DeleteKeyAction),
	_DeleteAccount(DeleteAccountAction),
}

pub const PUBLIC_KEY_LENGTH: usize = 32;

#[derive(Clone, PartialEq, Eq, RuntimeDebug)]
pub struct PublicKey(pub [u8; PUBLIC_KEY_LENGTH]);

impl BorshSerialize for PublicKey {
	fn serialize<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
		BorshSerialize::serialize(&0u8, writer)?;
		writer.write_all(&self.0)?;
		Ok(())
	}
}

#[derive(BorshSerialize, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Transaction {
	pub signer_id: String,
	pub public_key: PublicKey,
	pub nonce: Nonce,
	pub receiver_id: String,
	pub block_hash: [u8; 32],
	pub actions: Vec<Action>,
}

/// Length of an Ed25519 signature
pub const SIGNATURE_LENGTH: usize = 64;

#[derive(Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Signature(pub [u8; SIGNATURE_LENGTH]);

impl BorshSerialize for Signature {
	fn serialize<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
		BorshSerialize::serialize(&0u8, writer)?;
		writer.write_all(&self.0)?;
		Ok(())
	}
}

#[derive(BorshSerialize, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct SignedTransaction {
	pub transaction: Transaction,
	pub signature: Signature,
}
