// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use frame_support::{
	dispatch::{GetDispatchInfo, PostDispatchInfo},
	traits::{ConstU32, OneSessionHandler, StorageVersion},
	transactional, BoundedVec,
};
use frame_system::offchain::{
	AppCrypto, CreateSignedTransaction, SendUnsignedTransaction, SignedPayload, Signer,
	SigningTypes,
};
use pallet_octopus_support::{
	log,
	traits::{AppchainInterface, BridgeInterface, LposInterface, UpwardMessagesInterface},
};
use scale_info::{
	prelude::string::{String, ToString},
	TypeInfo,
};
use serde::{de, Deserialize, Deserializer};
use sp_core::crypto::KeyTypeId;
use sp_runtime::{
	offchain::{
		http,
		storage::{MutateStorageError, StorageRetrievalError, StorageValueRef},
		Duration,
	},
	traits::{Dispatchable, IdentifyAccount},
	RuntimeAppPublic, RuntimeDebug,
};
use sp_std::prelude::*;
use types::{
	AppchainNotification, AppchainNotificationHistory, NotificationResult, Observation,
	ObservationType, ObservationsPayload, Validator, ValidatorSet,
};
// pub use weights::WeightInfo;

// Re-export pallet items so that they can be accessed from the crate namespace.
pub use pallet::*;

pub(crate) const LOG_TARGET: &'static str = "runtime::octopus-appchain";

pub(crate) const GIT_VERSION: &str = include_str!(concat!(env!("OUT_DIR"), "/git_version"));

mod mainchain;
pub mod types;
// pub mod weights;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

/// Defines application identifier for crypto keys of this module.
///
/// Every module that deals with signatures needs to declare its unique identifier for
/// its crypto keys.
/// When offchain worker is signing transactions it's going to request keys of type
/// `KeyTypeId` from the keystore and use the ones it finds to sign the transaction.
/// The keys can be inserted manually via RPC (see `author_insertKey`).
pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"octo");

/// Based on the above `KeyTypeId` we need to generate a pallet-specific crypto type wrappers.
/// We can use from supported crypto kinds (`sr25519`, `ed25519` and `ecdsa`) and augment
/// the types with this pallet-specific identifier.
pub mod sr25519 {
	mod app_sr25519 {
		use super::super::KEY_TYPE;
		use sp_runtime::app_crypto::{app_crypto, sr25519};
		app_crypto!(sr25519, KEY_TYPE);
	}

	sp_application_crypto::with_pair! {
		/// An octopus keypair using sr25519 as its crypto.
		pub type AuthorityPair = app_sr25519::Pair;
	}

	/// An octopus signature using sr25519 as its crypto.
	pub type AuthoritySignature = app_sr25519::Signature;

	/// An octopus identifier using sr25519 as its crypto.
	pub type AuthorityId = app_sr25519::Public;
}

pub mod ecdsa {
	mod app_ecdsa {
		use super::super::KEY_TYPE;
		use sp_runtime::app_crypto::{app_crypto, ecdsa};
		app_crypto!(ecdsa, KEY_TYPE);
	}

	sp_application_crypto::with_pair! {
		/// An octopus keypair using ecdsa as its crypto.
		pub type AuthorityPair = app_ecdsa::Pair;
	}

	/// An octopus signature using ecdsa as its crypto.
	pub type AuthoritySignature = app_ecdsa::Signature;

	/// An octopus identifier using ecdsa as its crypto.
	pub type AuthorityId = app_ecdsa::Public;
}

impl<T: Config> AppchainInterface<T::AccountId> for Pallet<T> {
	fn is_activated() -> bool {
		IsActivated::<T>::get()
	}

	fn next_set_id() -> u32 {
		NextSetId::<T>::get()
	}

	fn planned_validators() -> Vec<(T::AccountId, u128)> {
		<PlannedValidators<T>>::get()
	}
}

/// The current storage version.
const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: CreateSignedTransaction<Call<Self>> + frame_system::Config {
		/// Identity of an appchain authority.
		type AuthorityId: Member + Parameter + RuntimeAppPublic + MaybeSerializeDeserialize;

		/// The identifier type for an offchain worker.
		type AppCrypto: AppCrypto<Self::Public, Self::Signature>;

		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The overarching dispatch call type.
		type RuntimeCall: Parameter
			+ Dispatchable<RuntimeOrigin = Self::RuntimeOrigin, PostInfo = PostDispatchInfo>
			+ GetDispatchInfo
			+ From<frame_system::Call<Self>>;

		type BridgeInterface: BridgeInterface<Self::AccountId>;

		type LposInterface: LposInterface<Self::AccountId>;

		type UpwardMessagesInterface: UpwardMessagesInterface<Self::AccountId>;

		// Configuration parameters
		/// A grace period after we send transaction.
		///
		/// To avoid sending too many transactions, we only attempt to send one
		/// every `GRACE_PERIOD` blocks. We use Local Storage to coordinate
		/// sending between distinct runs of this offchain worker.
		#[pallet::constant]
		type GracePeriod: Get<Self::BlockNumber>;

		/// A configuration for base priority of unsigned transactions.
		///
		/// This is exposed so that it can be tuned for particular runtime, when
		/// multiple pallets send unsigned transactions.
		#[pallet::constant]
		type UnsignedPriority: Get<TransactionPriority>;

		/// A configuration for limit of request notification.
		///
		/// This is the limits for request notification histories.
		#[pallet::constant]
		type RequestEventLimit: Get<u32>;

		#[pallet::constant]
		type MaxValidators: Get<u32>;

		// type WeightInfo: WeightInfo;
	}

	type MaxObservations = ConstU32<100>;

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	#[pallet::without_storage_info]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::type_value]
	pub(crate) fn DefaultForAnchorContract() -> Vec<u8> {
		Vec::new()
	}

	#[pallet::storage]
	#[pallet::getter(fn anchor_contract)]
	pub(crate) type AnchorContract<T: Config> =
		StorageValue<_, Vec<u8>, ValueQuery, DefaultForAnchorContract>;

	// /// A storage discarded after StorageVersion 2.
	// #[pallet::storage]
	// pub(crate) type AssetIdByName<T: Config> =
	// 	StorageMap<_, Twox64Concat, Vec<u8>, T::AssetId, ValueQuery>;

	/// Whether the appchain is activated.
	///
	/// Only an active appchain will communicate with the mainchain and pay block rewards.
	#[pallet::storage]
	pub(crate) type IsActivated<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::storage]
	pub(crate) type NextSetId<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	pub(crate) type PlannedValidators<T: Config> =
		StorageValue<_, Vec<(T::AccountId, u128)>, ValueQuery>;

	#[pallet::storage]
	pub(crate) type NextNotificationId<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	pub(crate) type Observations<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		ObservationType,
		Twox64Concat,
		u32,
		BoundedVec<Observation<T::AccountId>, MaxObservations>,
		ValueQuery,
	>;

	#[pallet::storage]
	pub(crate) type Observing<T: Config> = StorageMap<
		_,
		Twox64Concat,
		Observation<T::AccountId>,
		BoundedVec<T::AccountId, T::MaxValidators>,
		ValueQuery,
	>;

	#[pallet::storage]
	pub(crate) type NotificationHistory<T: Config> =
		StorageMap<_, Twox64Concat, u32, Option<NotificationResult>, ValueQuery>;

	#[pallet::type_value]
	pub(crate) fn DefaultForGitVersion() -> Vec<u8> {
		hex::decode(GIT_VERSION).unwrap_or_default()
	}

	#[pallet::storage]
	pub(crate) type GitVersion<T: Config> =
		StorageValue<_, Vec<u8>, ValueQuery, DefaultForGitVersion>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub anchor_contract: String,
		pub validators: Vec<(T::AccountId, u128)>,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { anchor_contract: String::new(), validators: Vec::new() }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			<AnchorContract<T>>::put(self.anchor_contract.as_bytes());

			<NextSetId<T>>::put(1); // set 0 is already in the genesis
			<PlannedValidators<T>>::put(self.validators.clone());
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new set of validators is waiting to be changed.
		NewPlannedValidators { set_id: u32, validators: Vec<(T::AccountId, u128)> },
		/// An `amount` unlock to `receiver` from `sender` failed.
		UnlockFailed { sender: Vec<u8>, receiver: T::AccountId, amount: u128, sequence: u32 },
		MintNep141Failed {
			token_id: Vec<u8>,
			sender: Vec<u8>,
			receiver: T::AccountId,
			amount: u128,
			sequence: u32,
		},
		UnlockNonfungibleFailed {
			collection: u128,
			item: u128,
			sender: Vec<u8>,
			receiver: T::AccountId,
			sequence: u32,
		},
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// The set id of new validator set was wrong.
		WrongSetId,
		/// Invalid notification id of observation.
		InvalidNotificationId,
		/// Must be a validator.
		NotValidator,
		/// Next notification Id overflow.
		NextNotificationIdOverflow,
		/// Invalid active total stake.
		InvalidActiveTotalStake,
		/// Appchain is not activated.
		NotActivated,
		/// ReceiverId is not a valid utf8 string.
		InvalidReceiverId,
		/// Next set Id overflow.
		NextSetIdOverflow,
		/// Observations exceeded limit.
		ObservationsExceededLimit,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Offchain Worker entry point.
		///
		/// By implementing `fn offchain_worker` you declare a new offchain worker.
		/// This function will be called when the node is fully synced and a new best block is
		/// succesfuly imported.
		/// Note that it's not guaranteed for offchain workers to run on EVERY block, there might
		/// be cases where some blocks are skipped, or for some the worker runs twice (re-orgs),
		/// so the code should be able to handle that.
		/// You can use `Local Storage` API to coordinate runs of the worker.
		fn offchain_worker(block_number: T::BlockNumber) {
			let anchor_contract = Self::anchor_contract();
			if !sp_io::offchain::is_validator() ||
				!IsActivated::<T>::get() ||
				anchor_contract.is_empty()
			{
				return
			}

			let parent_hash = <frame_system::Pallet<T>>::block_hash(block_number - 1u32.into());
			log!(debug, "Current block: {:?} (parent hash: {:?})", block_number, parent_hash);

			if !Self::should_send(block_number) {
				return
			}

			// Only communicate with mainchain if we are validators.
			match Self::get_validator_id() {
				Some((public, key_data, validator_id)) => {
					log!(
						debug,
						"public: {:?}, key_data: {:?}, validator_id: {:?}",
						public,
						key_data,
						validator_id,
					);

					let mainchain_rpc_endpoint = Self::get_mainchain_rpc_endpoint(
						anchor_contract[anchor_contract.len() - 1] == 116,
					); // last byte is 't'
					log!(debug, "current mainchain_rpc_endpoint {:?}", mainchain_rpc_endpoint);

					let secondary_mainchain_rpc_endpoint = Self::secondary_rpc_endpoint(
						anchor_contract[anchor_contract.len() - 1] == 116,
					); // last byte is 't'
					log!(
						debug,
						"current secondary_mainchain_rpc_endpoint {:?}",
						secondary_mainchain_rpc_endpoint
					);

					if let Err(e) = Self::observing_mainchain(
						block_number,
						&mainchain_rpc_endpoint,
						&secondary_mainchain_rpc_endpoint,
						anchor_contract,
						public,
						key_data,
						validator_id,
					) {
						log!(warn, "observing_mainchain: Error: {}", e);
					}
				},
				None => {
					log!(warn, "Not a validator, skipping offchain worker");
				},
			}
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;

		/// Validate unsigned call to this module.
		///
		/// By default unsigned transactions are disallowed, but implementing the validator
		/// here we make sure that some particular calls (the ones produced by offchain worker)
		/// are being whitelisted and marked as valid.
		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			// Firstly let's check that we call the right function.
			if let Call::submit_observations { ref payload, ref signature } = call {
				let signature_valid =
					SignedPayload::<T>::verify::<T::AppCrypto>(payload, signature.clone());
				if !signature_valid {
					return InvalidTransaction::BadProof.into()
				}
				Self::validate_transaction_parameters(
					&payload.block_number,
					payload.public.clone().into_account(),
				)
			} else {
				InvalidTransaction::Call.into()
			}
		}
	}

	// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	// These functions materialize as "extrinsics", which are often compared to transactions.
	// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Submit observations.
		// #[pallet::weight(<T as
		// Config>::WeightInfo::submit_observations(payload.observations.len() as u32))]
		#[pallet::weight(0)]
		pub fn submit_observations(
			origin: OriginFor<T>,
			payload: ObservationsPayload<
				T::Public,
				T::BlockNumber,
				<T as frame_system::Config>::AccountId,
			>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			// This ensures that the function can only be called via unsigned transaction.
			ensure_none(origin)?;
			let who = payload.public.clone().into_account();

			let val_id = T::LposInterface::is_active_validator(KEY_TYPE, &payload.key_data);
			if val_id.is_none() {
				log!(
					warn,
					"Not a validator in current validator set, key_data: {:?}",
					payload.key_data
				);
				return Err(Error::<T>::NotValidator.into())
			}
			let val_id = val_id.expect("Validator is valid; qed").clone();

			//
			log!(debug, "️️️observations: {:#?},\nwho: {:?}", payload.observations, who);
			//

			for observation in payload.observations.iter() {
				if let Err(e) = Self::submit_observation(&val_id, observation.clone()) {
					log!(warn, "OCTOPUS-ALERT submit_observation: Error: {:?}", e);
				}
			}

			Ok(().into())
		}

		// #[pallet::weight(<T as Config>::WeightInfo::force_set_is_activated())]
		#[pallet::weight(0)]
		pub fn force_set_is_activated(origin: OriginFor<T>, is_activated: bool) -> DispatchResult {
			ensure_root(origin)?;
			<IsActivated<T>>::put(is_activated);
			Ok(())
		}

		// #[pallet::weight(<T as Config>::WeightInfo::force_set_next_set_id())]
		#[pallet::weight(0)]
		pub fn force_set_next_set_id(origin: OriginFor<T>, next_set_id: u32) -> DispatchResult {
			ensure_root(origin)?;
			<NextSetId<T>>::put(next_set_id);
			log!(info, "️️️force set next_set_id, next_set_id is : {:?} ", NextSetId::<T>::get());
			Ok(())
		}

		// Force set planned validators with sudo permissions.
		// #[pallet::weight(<T as Config>::WeightInfo::force_set_planned_validators(validators.len()
		// as u32))]
		#[pallet::weight(0)]
		pub fn force_set_planned_validators(
			origin: OriginFor<T>,
			validators: Vec<(T::AccountId, u128)>,
		) -> DispatchResult {
			ensure_root(origin)?;
			<PlannedValidators<T>>::put(validators);
			Ok(())
		}

		// #[pallet::weight(<T as Config>::WeightInfo::force_set_next_notification_id())]
		#[pallet::weight(0)]
		pub fn force_set_next_notification_id(
			origin: OriginFor<T>,
			next_notification_id: u32,
		) -> DispatchResult {
			ensure_root(origin)?;
			<NextNotificationId<T>>::put(next_notification_id);
			log!(
				info,
				"️️️force set next_notification_id, next_notification_id is : {:?} ",
				NextNotificationId::<T>::get()
			);
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		fn secondary_rpc_endpoint(is_testnet: bool) -> String {
			if is_testnet {
				"https://rpc.testnet.near.org".to_string()
			} else {
				"https://near-mainnet.infura.io/v3/dabe9e95376540b083ae09909ea7c576".to_string()
			}
		}

		fn primary_rpc_endpoint(is_testnet: bool) -> String {
			if is_testnet {
				"https://near-testnet.infura.io/v3/dabe9e95376540b083ae09909ea7c576".to_string()
			} else {
				"https://rpc.mainnet.near.org".to_string()
			}
		}

		fn get_mainchain_rpc_endpoint(is_testnet: bool) -> String {
			let kind = sp_core::offchain::StorageKind::PERSISTENT;
			if let Some(data) = sp_io::offchain::local_storage_get(
				kind,
				b"octopus_appchain::mainchain_rpc_endpoint",
			) {
				if let Ok(rpc_url) = String::from_utf8(data) {
					log!(debug, "The configure url is {:?} ", rpc_url.clone());
					return rpc_url
				} else {
					log!(warn, "Parse configure url error, return default rpc url");
					return Self::primary_rpc_endpoint(is_testnet)
				}
			} else {
				log!(debug, "No configuration for rpc, return default rpc url");
				return Self::primary_rpc_endpoint(is_testnet)
			}
		}

		fn should_send(block_number: T::BlockNumber) -> bool {
			/// A friendlier name for the error that is going to be returned in case we are in the
			/// grace period.
			const RECENTLY_SENT: () = ();

			// Start off by creating a reference to Local Storage value.
			// Since the local storage is common for all offchain workers, it's a good practice
			// to prepend your entry with the module name.
			let val = StorageValueRef::persistent(b"octopus_appchain::last_send");
			// The Local Storage is persisted and shared between runs of the offchain workers,
			// and offchain workers may run concurrently. We can use the `mutate` function, to
			// write a storage entry in an atomic fashion. Under the hood it uses `compare_and_set`
			// low-level method of local storage API, which means that only one worker
			// will be able to "acquire a lock" and send a transaction if multiple workers
			// happen to be executed concurrently.
			let res =
				val.mutate(|last_send: Result<Option<T::BlockNumber>, StorageRetrievalError>| {
					match last_send {
						// If we already have a value in storage and the block number is recent
						// enough we avoid sending another transaction at this time.
						Ok(Some(block)) if block_number < block + T::GracePeriod::get() =>
							Err(RECENTLY_SENT),
						// In every other case we attempt to acquire the lock and send a
						// transaction.
						_ => Ok(block_number),
					}
				});

			match res {
				// The value has been set correctly, which means we can safely send a transaction
				// now.
				Ok(_) => true,
				// We are in the grace period, we should not send a transaction this time.
				Err(MutateStorageError::ValueFunctionFailed(RECENTLY_SENT)) => false,
				// We wanted to send a transaction, but failed to write the block number (acquire a
				// lock). This indicates that another offchain worker that was running concurrently
				// most likely executed the same logic and succeeded at writing to storage.
				// Thus we don't really want to send the transaction, knowing that the other run
				// already did.
				Err(MutateStorageError::ConcurrentModification(_)) => false,
			}
		}

		fn get_validator_id() -> Option<(<T as SigningTypes>::Public, Vec<u8>, T::AccountId)> {
			for key in <T::AppCrypto as AppCrypto<
				<T as SigningTypes>::Public,
				<T as SigningTypes>::Signature,
			>>::RuntimeAppPublic::all()
			.into_iter()
			{
				let key_data = key.to_raw_vec();
				log!(trace, "local key: {:?}", key_data);

				let val_id = T::LposInterface::is_active_validator(KEY_TYPE, &key_data);
				if val_id.is_none() {
					continue
				}
				let generic_public = <T::AppCrypto as AppCrypto<
					<T as SigningTypes>::Public,
					<T as SigningTypes>::Signature,
				>>::GenericPublic::from(key);
				let public: <T as SigningTypes>::Public = generic_public.into();
				log!(trace, "local public key: {:?}", public);

				return Some((public, key_data, val_id.unwrap()))
			}
			None
		}

		pub fn observing_mainchain(
			block_number: T::BlockNumber,
			mainchain_rpc_endpoint: &str,
			secondary_mainchain_rpc_endpoint: &str,
			anchor_contract: Vec<u8>,
			public: <T as SigningTypes>::Public,
			key_data: Vec<u8>,
			_validator_id: T::AccountId,
		) -> Result<(), &'static str> {
			let next_notification_id = NextNotificationId::<T>::get();
			log!(debug, "next_notification_id: {}", next_notification_id);
			let next_set_id = NextSetId::<T>::get();
			log!(debug, "next_set_id: {}", next_set_id);

			// Make an external HTTP request to fetch the current price.
			// Note this call will block until response is received.
			let ret = Self::get_validator_list_of(
				mainchain_rpc_endpoint,
				anchor_contract.clone(),
				next_set_id,
			);

			let mut obs = match ret {
				Ok(observations) => observations,
				Err(_) => {
					log!(debug, "retry with failsafe endpoint to get validators");
					Self::get_validator_list_of(
						secondary_mainchain_rpc_endpoint,
						anchor_contract.clone(),
						next_set_id,
					)
					.map_err(|_| "Failed to get_validator_list_of")?
				},
			};

			// check cross-chain transfers only if there isn't a validator_set update.
			if obs.len() == 0 {
				log!(debug, "No validat_set updates, try to get appchain notifications.");
				// Make an external HTTP request to fetch the current price.
				// Note this call will block until response is received.
				let ret = Self::get_appchain_notification_histories(
					mainchain_rpc_endpoint,
					anchor_contract.clone(),
					next_notification_id,
					T::RequestEventLimit::get(),
				);

				obs = match ret {
					Ok(observations) => observations,
					Err(_) => {
						log!(debug, "retry with failsafe endpoint to get notify");
						Self::get_appchain_notification_histories(
							secondary_mainchain_rpc_endpoint,
							anchor_contract,
							next_notification_id,
							T::RequestEventLimit::get(),
						)
						.map_err(|_| "Failed to get_appchain_notification_histories")?
					},
				};
			}

			if obs.len() == 0 {
				log!(debug, "No messages from mainchain.");
				return Ok(())
			}

			let result = Signer::<T, T::AppCrypto>::all_accounts()
				.with_filter(vec![public])
				.send_unsigned_transaction(
					|account| ObservationsPayload {
						public: account.public.clone(),
						key_data: key_data.clone(),
						block_number,
						observations: obs.clone(),
					},
					|payload, signature| Call::submit_observations { payload, signature },
				);
			if result.len() != 1 {
				return Err("No account found")
			}
			if result[0].1.is_err() {
				log!(warn, "OCTOPUS-ALERT Failed to submit observations: {:?}", result[0].1);

				return Err("Failed to submit observations")
			}

			Ok(())
		}

		fn increase_next_notification_id() -> DispatchResultWithPostInfo {
			NextNotificationId::<T>::try_mutate(|next_id| -> DispatchResultWithPostInfo {
				if let Some(v) = next_id.checked_add(1) {
					*next_id = v;
					log!(debug, "️️️increase next_notification_id: {:?} ", v);
				} else {
					return Err(Error::<T>::NextNotificationIdOverflow.into())
				}
				Ok(().into())
			})
		}

		fn increase_next_set_id() -> DispatchResultWithPostInfo {
			NextSetId::<T>::try_mutate(|next_id| -> DispatchResultWithPostInfo {
				if let Some(v) = next_id.checked_add(1) {
					*next_id = v;
					log!(debug, "️️️increase next_set_id: {:?} ", v);
				} else {
					return Err(Error::<T>::NextSetIdOverflow.into())
				}
				Ok(().into())
			})
		}

		fn check_observation(
			observation_type: ObservationType,
			obs_id: u32,
		) -> DispatchResultWithPostInfo {
			match observation_type {
				ObservationType::UpdateValidatorSet => {
					let next_set_id = NextSetId::<T>::get();
					if obs_id != next_set_id {
						log!(
							warn,
							"wrong set id for update validator set: {:?}, expected: {:?}",
							obs_id,
							next_set_id
						);
						return Err(Error::<T>::WrongSetId.into())
					}
				},
				_ => {
					let next_notification_id = NextNotificationId::<T>::get();
					let limit = T::RequestEventLimit::get();
					if (obs_id < next_notification_id) || (obs_id >= next_notification_id + limit) {
						log!(
							warn,
							"invalid notification id for observation: {:?}, expected: [{:?}, {:?})",
							obs_id,
							next_notification_id,
							next_notification_id + limit
						);
						return Err(Error::<T>::InvalidNotificationId.into())
					}
				},
			}

			Ok(().into())
		}

		fn prune_old_histories() {
			// let next_notification_id = NextNotificationId::<T>::get();
			// if next_notification_id <= T::NotificationHistoryDepth::get() {
			// 	return;
			// }

			// let prune_index = next_notification_id - T::NotificationHistoryDepth::get();

			// // prune observations
			// let prune_obs = <Observations<T>>::iter_prefix(observation_type)
			// 	.filter(|(index, _)| *index < prune_index)
			// 	.collect::<Vec<(u32, Vec<Observation<T::AccountId>>)>>();

			// log!(debug, "will delete old observations: {:#?}", prune_obs.clone());
			// let _ = prune_obs
			// 	.iter()
			// 	.map(|(index, obs)| {
			// 		for o in obs.iter() {
			// 			<Observing<T>>::remove(o);
			// 		}
			// 		<Observations<T>>::remove(observation_type, index);
			// 	})
			// 	.collect::<Vec<_>>();

			// // prune records
			// // TODO: use simple code
			// let prune_records = <ObservationRecords<T>>::iter_prefix(record_type)
			// 	.filter(|(index, _)| *index <= prune_index)
			// 	.collect::<Vec<(u32, ObservationRecord<T::AccountId>)>>();

			// log!(debug, "will delete old records: {:#?}", prune_records.clone());
			// let _ = prune_records
			// 	.iter()
			// 	.map(|(index, _)| {
			// 		<ObservationRecords<T>>::remove(record_type, index);
			// 	})
			// 	.collect::<Vec<_>>();
		}

		/// If the observation already exists in the Observations, then the only thing
		/// to do is vote for this observation.
		#[transactional]
		fn submit_observation(
			validator_id: &T::AccountId,
			observation: Observation<T::AccountId>,
		) -> DispatchResultWithPostInfo {
			let observation_type = Self::get_observation_type(&observation);
			let obs_id = observation.observation_index();
			Self::check_observation(observation_type, obs_id)?;

			<Observations<T>>::mutate(observation_type, obs_id, |obs| {
				let found = obs.iter().any(|o| o == &observation);
				if !found {
					obs.try_push(observation.clone())
				} else {
					Ok(())
				}
			})
			.map_err(|_| Error::<T>::ObservationsExceededLimit)?;

			<Observing<T>>::mutate(&observation, |vals| {
				let found = vals.iter().any(|id| id == validator_id);
				if !found {
					vals.try_push(validator_id.clone())
				} else {
					log!(warn, "{:?} submits a duplicate ocw tx", validator_id);
					Ok(())
				}
			})
			.map_err(|_| Error::<T>::ObservationsExceededLimit)?;
			let total_stake: u128 = T::LposInterface::active_total_stake()
				.ok_or(Error::<T>::InvalidActiveTotalStake)?;
			let stake: u128 = <Observing<T>>::get(&observation)
				.iter()
				.map(|v| T::LposInterface::active_stake_of(v))
				.sum();

			//
			log!(debug, "observations type: {:#?}", observation_type);
			log!(
				debug,
				"️️️observations content: {:#?}",
				<Observations<T>>::get(observation_type, obs_id)
			);
			log!(debug, "️️️observer: {:#?}", <Observing<T>>::get(&observation));
			log!(debug, "️️️total_stake: {:?}, stake: {:?}", total_stake, stake);
			//

			if 3 * stake > 2 * total_stake {
				match observation.clone() {
					Observation::UpdateValidatorSet(val_set) => {
						let validators: Vec<(T::AccountId, u128)> = val_set
							.validators
							.iter()
							.map(|v| (v.validator_id_in_appchain.clone(), v.total_stake))
							.collect();
						<PlannedValidators<T>>::put(validators.clone());
						log!(debug, "new PlannedValidators: {:?}", validators);
						let set_id = NextSetId::<T>::get();
						Self::deposit_event(Event::NewPlannedValidators { set_id, validators });
						Self::increase_next_set_id()?;
						log!(
							info,
							"️️️processed updata validator set, next_set_id is : {:?} ",
							NextSetId::<T>::get()
						);
					},
					Observation::Burn(event) => {
						Self::increase_next_notification_id()?;
						let mut result = NotificationResult::Success;
						if let Err(error) = T::BridgeInterface::unlock(
							event.sender_id.clone(),
							event.receiver.clone(),
							event.amount,
							event.index,
						) {
							log!(warn, "️️️failed to unlock native token, sequence: {:?}, send_id: {:?}, receiver: {:?}, amount: {:?}, error: {:?}",
							event.index,
							event.sender_id.clone(),
							event.receiver.clone(),
							event.amount,
							error);
							Self::deposit_event(Event::UnlockFailed {
								sender: event.sender_id,
								receiver: event.receiver,
								amount: event.amount,
								sequence: event.index,
							});
							result = NotificationResult::UnlockFailed;
						}
						NotificationHistory::<T>::insert(obs_id, Some(result.clone()));
						log!(
							info,
							"️️️processed burn observation, obs_id is: {:?}, result is: {:?}, next_notification_id is: {:?} ",
							obs_id,
							result,
							NextNotificationId::<T>::get()
						);
					},
					Observation::LockAsset(event) => {
						Self::increase_next_notification_id()?;
						let mut result = NotificationResult::Success;

						log!(
								info,
								"️️️sequence: {:?}, mint asset:{:?}, sender_id:{:?}, receiver:{:?}, amount:{:?}",
								event.index,
								event.token_id,
								event.sender_id,
								event.receiver,
								event.amount,
							);
						let sequence = event.index;
						if let Err(error) = T::BridgeInterface::mint_nep141(
							event.token_id.clone(),
							event.sender_id.clone(),
							event.receiver.clone(),
							event.amount,
							sequence,
						) {
							log!(warn, "️️️failed to mint asset, sequence: {:?}, asset: {:?}, sender_id: {:?}, receiver: {:?}, amount: {:?}, error: {:?}",
								event.index,
								event.token_id,
								event.sender_id.clone(),
								event.receiver.clone(),
								event.amount,
								error);
							Self::deposit_event(Event::MintNep141Failed {
								token_id: event.token_id,
								sender: event.sender_id,
								receiver: event.receiver,
								amount: event.amount.into(),
								sequence,
							});
							result = NotificationResult::AssetMintFailed;
						}

						NotificationHistory::<T>::insert(obs_id, Some(result.clone()));
						log!(
							info,
							"️️️processed lock observation, obs_id is: {:?}, result is: {:?}, next_notification_id is : {:?} ",
							obs_id,
							result,
							NextNotificationId::<T>::get()
						);
					},
					Observation::BurnNft(event) => {
						Self::increase_next_notification_id()?;
						let mut result = NotificationResult::Success;

						if let Err(error) = T::BridgeInterface::unlock_nonfungible(
							event.collection,
							event.item,
							event.sender_id.clone(),
							event.receiver.clone(),
							event.index,
						) {
							log!(warn, "️️️failed to unlock nft, sequence: {:?}, send_id: {:?}, receiver: {:?}, collection: {:?}, item: {:?}, error: {:?}",
							event.index,
							event.sender_id.clone(),
							event.receiver.clone(),
							event.collection,
							event.item,
							error);

							Self::deposit_event(Event::UnlockNonfungibleFailed {
								collection: event.collection,
								item: event.item,
								sender: event.sender_id,
								receiver: event.receiver,
								sequence: event.index,
							});

							result = NotificationResult::NftUnlockFailed;
						}

						NotificationHistory::<T>::insert(obs_id, Some(result.clone()));
						log!(
							info,
							"️️️processed burn_nft observation, obs_id is: {:?}, result is: {:?}, next_notification_id is: {:?}",
							obs_id,
							result,
							NextNotificationId::<T>::get(),
						);
					},
				}

				Self::prune_old_histories();
			}

			Ok(().into())
		}

		fn get_observation_type(observation: &Observation<T::AccountId>) -> ObservationType {
			match observation.clone() {
				Observation::UpdateValidatorSet(_) => ObservationType::UpdateValidatorSet,
				Observation::Burn(_) => ObservationType::Burn,
				Observation::LockAsset(_) => ObservationType::LockAsset,
				Observation::BurnNft(_) => ObservationType::BurnNft,
			}
		}

		fn validate_transaction_parameters(
			block_number: &T::BlockNumber,
			account_id: <T as frame_system::Config>::AccountId,
		) -> TransactionValidity {
			// Let's make sure to reject transactions from the future.
			let current_block = <frame_system::Pallet<T>>::block_number();
			if &current_block < block_number {
				log!(
					warn,
					"InvalidTransaction => current_block: {:?}, block_number: {:?}",
					current_block,
					block_number
				);
				return InvalidTransaction::Future.into()
			}

			ValidTransaction::with_tag_prefix("OctopusAppchain")
				// We set base priority to 2**21 and hope it's included before any other
				// transactions in the pool.
				.priority(T::UnsignedPriority::get())
				// This transaction does not require anything else to go before into the pool.
				//.and_requires()
				// We set the `provides` tag to `account_id`. This makes
				// sure only one transaction produced by current validator will ever
				// get to the transaction pool and will end up in the block.
				// We can still have multiple transactions compete for the same "spot",
				// and the one with higher priority will replace other one in the pool.
				.and_provides(account_id)
				// The transaction is only valid for next 5 blocks. After that it's
				// going to be revalidated by the pool.
				.longevity(5)
				// It's fine to propagate that transaction to other peers, which means it can be
				// created even by nodes that don't produce blocks.
				// Note that sometimes it's better to keep it for yourself (if you are the block
				// producer), since for instance in some schemes others may copy your solution and
				// claim a reward.
				.propagate(true)
				.build()
		}

		// fn migration_to_v1() {
		// 	let account = <Pallet<T>>::account_id();
		// 	OctopusPalletId::<T>::put(Some(account));
		// 	log!(info, "updating to version 1 ");
		// }

		// fn migration_to_v2(translated: u64) -> u64 {
		// 	let mut translated = translated;
		// 	let _ = AssetIdByName::<T>::iter()
		// 		.map(|(token_id, asset_id)| {
		// 			AssetIdByTokenId::<T>::insert(token_id, asset_id);
		// 			translated += 1u64;
		// 		})
		// 		.collect::<Vec<_>>();

		// 	log!(info, "updating to version 2 ",);
		// 	translated
		// }
	}

	impl<T: Config> sp_runtime::BoundToRuntimeAppPublic for Pallet<T> {
		type Public = T::AuthorityId;
	}

	impl<T: Config> OneSessionHandler<T::AccountId> for Pallet<T> {
		type Key = T::AuthorityId;

		fn on_genesis_session<'a, I: 'a>(_authorities: I)
		where
			I: Iterator<Item = (&'a T::AccountId, Self::Key)>,
		{
			// ignore
		}

		fn on_new_session<'a, I: 'a>(_changed: bool, _validators: I, _queued_validators: I)
		where
			I: Iterator<Item = (&'a T::AccountId, Self::Key)>,
		{
			// ignore
		}

		fn on_disabled(_i: u32) {
			// ignore
		}
	}
}
