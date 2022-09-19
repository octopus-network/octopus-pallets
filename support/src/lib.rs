#![cfg_attr(not(feature = "std"), no_std)]

use crate::types::Nep171TokenMetadata;
use borsh::{BorshDeserialize, BorshSerialize};
use codec::{Decode, Encode};
use frame_support::dispatch::{DispatchError, DispatchResult};
use scale_info::{prelude::string::String, TypeInfo};
use sp_runtime::{KeyTypeId, RuntimeDebug};
use sp_std::prelude::*;

pub mod traits;
pub mod types;

// syntactic sugar for logging.
#[macro_export]
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: crate::LOG_TARGET,
			concat!("[{:?}] 🐙 ", $patter), <frame_system::Pallet<T>>::block_number() $(, $values)*
		)
	};
}
