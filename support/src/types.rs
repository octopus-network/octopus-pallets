use codec::{Decode, Encode};
use sp_runtime::RuntimeDebug;

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum PayloadType {
	Lock,
	BurnAsset,
}
