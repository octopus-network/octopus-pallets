#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	dispatch::DispatchError,
	ensure,
	traits::{Get, StorageVersion},
	BoundedVec, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound,
};
use pallet_octopus_support::{log, traits::UpwardMessagesInterface, types::PayloadType};
use scale_info::TypeInfo;
use sp_core::H256;
use sp_io::offchain_index;
use sp_runtime::{
	traits::{Hash, Zero},
	DigestItem,
};
use sp_std::prelude::*;
pub use weights::WeightInfo;

pub use pallet::*;

pub(crate) const LOG_TARGET: &'static str = "runtime::octopus-upward-messages";

pub mod weights;

#[cfg(test)]
mod tests;

mod benchmarking;
pub mod migrations;

#[derive(
	Encode, Decode, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound, MaxEncodedLen, TypeInfo,
)]
#[scale_info(skip_type_params(M))]
#[codec(mel_bound())]
pub struct Message<M: Get<u32>> {
	#[codec(compact)]
	nonce: u64,
	payload_type: PayloadType,
	payload: BoundedVec<u8, M>,
}

/// The current storage version.
const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

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
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type Hashing: Hash<Output = H256>;

		/// Max bytes in a message payload
		#[pallet::constant]
		type MaxMessagePayloadSize: Get<u32>;

		/// Max number of messages per commitment
		#[pallet::constant]
		type MaxMessagesPerCommit: Get<u32>;

		/// Weight information for extrinsics in this pallet
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	/// Interval between commitments
	#[pallet::storage]
	#[pallet::getter(fn interval)]
	pub(crate) type Interval<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

	#[pallet::storage]
	pub(crate) type MessageQueue<T: Config> = StorageValue<
		_,
		BoundedVec<Message<<T as Config>::MaxMessagePayloadSize>, T::MaxMessagesPerCommit>,
		ValueQuery,
	>;

	#[pallet::storage]
	pub(crate) type Nonce<T: Config> = StorageValue<_, u64, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub interval: T::BlockNumber,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { interval: Default::default() }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			<Interval<T>>::put(self.interval);
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		MessageAccepted(u64),
		Committed { hash: H256, data: Vec<Message<T::MaxMessagePayloadSize>> },
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// The message payload exceeds byte limit.
		PayloadTooLarge,
		/// No more messages can be queued for the channel during this commit cycle.
		QueueSizeLimitReached,
		/// Cannot increment nonce
		NonceOverflow,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		// Generate a message commitment every [`Interval`] blocks.
		//
		// The commitment hash is included in an [`AuxiliaryDigestItem`] in the block header,
		// with the corresponding commitment is persisted offchain.
		fn on_initialize(now: T::BlockNumber) -> Weight {
			if (now % Self::interval()).is_zero() {
				Self::commit()
			} else {
				T::WeightInfo::on_initialize_non_interval()
			}
		}
	}

	// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	// These functions materialize as "extrinsics", which are often compared to transactions.
	// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
	#[pallet::call]
	impl<T: Config> Pallet<T> {}

	impl<T: Config> Pallet<T> {
		fn commit() -> Weight {
			let message_queue = <MessageQueue<T>>::take();
			if message_queue.is_empty() {
				return T::WeightInfo::on_initialize_no_messages()
			}

			let message_count = message_queue.len() as u32;
			let average_payload_size = Self::average_payload_size(&message_queue);

			let encoded_messages = message_queue.encode();
			let commitment_hash = <T as Config>::Hashing::hash(&encoded_messages);
			<frame_system::Pallet<T>>::deposit_log(DigestItem::Other(
				commitment_hash.as_bytes().to_vec(),
			));

			Self::deposit_event(Event::Committed {
				hash: commitment_hash,
				data: message_queue.to_vec(),
			});

			log!(
				debug,
				"commit cross-chain messages: hash: {:?}, messages: {:?}",
				commitment_hash,
				message_queue
			);

			offchain_index::set(commitment_hash.as_bytes(), &message_queue.encode());

			T::WeightInfo::on_initialize(message_count, average_payload_size)
		}

		fn average_payload_size(messages: &[Message<T::MaxMessagePayloadSize>]) -> u32 {
			let sum: usize = messages.iter().fold(0, |acc, x| acc + x.payload.len());
			(sum / messages.len()).saturating_add(1) as u32
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
			PayloadType::Lock | PayloadType::BurnAsset | PayloadType::LockNft => {
				ensure!(
					<MessageQueue<T>>::decode_len().unwrap_or(0) <
						T::MaxMessagesPerCommit::get() as usize,
					Error::<T>::QueueSizeLimitReached,
				);
				ensure!(
					payload.len() <= T::MaxMessagePayloadSize::get() as usize,
					Error::<T>::PayloadTooLarge,
				);
			},
			_ => {},
		}

		Nonce::<T>::try_mutate(|nonce| -> Result<u64, DispatchError> {
			*nonce = nonce.checked_add(1).ok_or::<Error<T>>(Error::<T>::NonceOverflow.into())?;

			<MessageQueue<T>>::try_append(Message {
				nonce: *nonce,
				payload_type,
				payload: payload.to_vec().try_into().map_err(|_| Error::<T>::PayloadTooLarge)?,
			})
			.map_err(|_| Error::<T>::QueueSizeLimitReached)?;

			Self::deposit_event(Event::MessageAccepted(*nonce));

			Ok(*nonce)
		})
	}
}
