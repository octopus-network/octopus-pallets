#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use crate::traits::LposInterface;
use codec::{Decode, Encode};
use sp_io::offchain_index;
use sp_std::prelude::*;

pub use pallet::*;

pub(crate) const LOG_TARGET: &'static str = "runtime::octopus-downlink";

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

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
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

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Initialization
		fn on_initialize(now: BlockNumberFor<T>) -> Weight {
			Self::commit()
		}
	}

	// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	// These functions materialize as "extrinsics", which are often compared to transactions.
	// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
	#[pallet::call]
	impl<T: Config> Pallet<T> {}

	impl<T: Config> Pallet<T> {
		fn submit(
			_who: &T::AccountId,
			payload_type: PayloadType,
			payload: &[u8],
		) -> DispatchResultWithPostInfo {
			Nonce::<T>::try_mutate(|nonce| -> DispatchResultWithPostInfo {
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
				Ok(().into())
			})
		}

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
