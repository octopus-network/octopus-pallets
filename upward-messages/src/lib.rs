#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use codec::{Decode, Encode};
use frame_support::dispatch::DispatchResult;
use pallet_octopus_support::{log, traits::UpwardMessagesInterface, types::PayloadType};
use scale_info::TypeInfo;
use sp_core::H256;
use sp_io::offchain_index;
use sp_runtime::{
	traits::{Hash, Keccak256},
	DigestItem, RuntimeDebug,
};
use sp_std::prelude::*;

pub use pallet::*;

pub(crate) const LOG_TARGET: &'static str = "runtime::octopus-upward-messages";

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct Message {
	nonce: u64,
	payload_type: PayloadType,
	payload: Vec<u8>,
}

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

		/// The overarching dispatch call type.
		type Call: From<Call<Self>>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	pub type MessageQueue<T: Config> = StorageValue<_, Vec<Message>, ValueQuery>;

	#[pallet::storage]
	pub type Nonce<T: Config> = StorageValue<_, u64, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// Nonce overflow.
		NonceOverflow,
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
				return 0;
			}

			let commitment_hash = Self::make_commitment_hash(&messages);

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
			offchain_index::set(&*key, &messages.encode());

			0
		}

		fn make_commitment_hash(messages: &[Message]) -> H256 {
			let messages: Vec<_> = messages
				.iter()
				.map(|message| (message.nonce, message.payload.clone()))
				.collect();
			let input = messages.encode();
			Keccak256::hash(&input)
		}

		fn make_offchain_key(hash: H256) -> Vec<u8> {
			(b"commitment", hash).encode()
		}
	}
}

impl<T: Config> UpwardMessagesInterface<<T as frame_system::Config>::AccountId> for Pallet<T> {
	fn submit(_who: &T::AccountId, payload_type: PayloadType, payload: &[u8]) -> DispatchResult {
		Nonce::<T>::try_mutate(|nonce| -> DispatchResult {
			if let Some(v) = nonce.checked_add(1) {
				*nonce = v;
			} else {
				return Err(Error::<T>::NonceOverflow.into());
			}

			MessageQueue::<T>::append(Message {
				nonce: *nonce,
				payload_type,
				payload: payload.to_vec(),
			});
			Ok(())
		})
	}
}
