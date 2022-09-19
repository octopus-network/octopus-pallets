#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use frame_support::{
	dispatch::DispatchError,
	ensure,
	traits::{Get, StorageVersion},
};
use pallet_octopus_support::{log, traits::UpwardMessagesInterface, types::PayloadType};
use scale_info::TypeInfo;
use sp_core::H256;
use sp_io::offchain_index;
use sp_runtime::{
	traits::{Hash, Keccak256},
	DigestItem, RuntimeDebug,
};
use sp_std::prelude::*;
pub use weights::WeightInfo;

pub use pallet::*;

pub(crate) const LOG_TARGET: &'static str = "runtime::octopus-upward-messages";

pub mod weights;

#[cfg(test)]
mod tests;

mod benchmarking;

#[derive(Encode, Decode, Clone, PartialEq, RuntimeDebug, TypeInfo)]
pub struct Message {
	nonce: u64,
	payload_type: PayloadType,
	payload: Vec<u8>,
}

/// The current storage version.
const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

#[allow(dead_code)]
#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// The limit for submit messages.
		#[pallet::constant]
		type UpwardMessagesLimit: Get<u32>;

		/// Weight information for extrinsics in this pallet
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	#[pallet::without_storage_info]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	pub(crate) type MessageQueue<T: Config> = StorageValue<_, Vec<Message>, ValueQuery>;

	#[pallet::storage]
	pub(crate) type Nonce<T: Config> = StorageValue<_, u64, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// Nonce overflow.
		NonceOverflow,
		/// Queue size limit reached.
		QueueSizeLimitReached,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Initialization
		fn on_initialize(_now: BlockNumberFor<T>) -> Weight {
			Self::commit()
		}
	}

	// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	// These functions materialize as "extrinsics", which are often compared to transactions.
	// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
	#[pallet::call]
	impl<T: Config> Pallet<T> {}

	impl<T: Config> Pallet<T> {
		fn commit() -> Weight {
			let messages: Vec<Message> = MessageQueue::<T>::take();
			if messages.is_empty() {
				return 0
			}

			let encoded_messages = messages.encode();
			let commitment_hash = Keccak256::hash(&encoded_messages);
			let average_payload_size = Self::average_payload_size(&messages);

			<frame_system::Pallet<T>>::deposit_log(DigestItem::Other(
				commitment_hash.as_bytes().to_vec(),
			));

			let key = Self::make_offchain_key(commitment_hash);
			log!(
				debug,
				"commit cross-chain messages: hash: {:?}, key: {:?}, messages: {:?}",
				commitment_hash,
				key,
				messages
			);
			offchain_index::set(&key, &messages.encode());

			T::WeightInfo::on_initialize(messages.len() as u32, average_payload_size as u32)
		}

		fn make_offchain_key(hash: H256) -> Vec<u8> {
			(b"commitment", hash).encode()
		}

		fn average_payload_size(messages: &[Message]) -> usize {
			let sum: usize = messages.iter().fold(0, |acc, x| acc + x.payload.len());
			(sum / messages.len()).saturating_add(1)
		}
	}
}

impl<T: Config> UpwardMessagesInterface<<T as frame_system::Config>::AccountId> for Pallet<T> {
	fn submit(
		_who: Option<T::AccountId>,
		payload_type: PayloadType,
		payload: &[u8],
	) -> Result<u64, DispatchError> {
		match payload_type {
			PayloadType::Lock | PayloadType::BurnAsset => {
				ensure!(
					MessageQueue::<T>::get().len() < T::UpwardMessagesLimit::get() as usize,
					Error::<T>::QueueSizeLimitReached,
				);
			},
			_ => {},
		}

		Nonce::<T>::try_mutate(|nonce| -> Result<u64, DispatchError> {
			if let Some(v) = nonce.checked_add(1) {
				*nonce = v;
			} else {
				return Err(Error::<T>::NonceOverflow.into())
			}

			MessageQueue::<T>::append(Message {
				nonce: *nonce,
				payload_type,
				payload: payload.to_vec(),
			});
			Ok(*nonce)
		})
	}
}
