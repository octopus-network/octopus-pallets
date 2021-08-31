#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use crate::traits::{LposInterface, SessionInterface};
use borsh::{BorshDeserialize, BorshSerialize};
use codec::{Decode, Encode};
use frame_support::{
	traits::{
		tokens::fungibles,
		Currency,
		ExistenceRequirement::{AllowDeath, KeepAlive},
		OneSessionHandler,
	},
	transactional,
	{sp_runtime::traits::AccountIdConversion, PalletId},
};
use frame_system::offchain::{
	AppCrypto, CreateSignedTransaction, SendUnsignedTransaction, SignedPayload, Signer,
	SigningTypes,
};
use serde::{de, Deserialize, Deserializer};
use sp_core::{crypto::KeyTypeId, H256};
use sp_io::offchain_index;
use sp_npos_elections::{to_supports, StakedAssignment, Supports};
use sp_runtime::{
	offchain::{
		http,
		storage::{MutateStorageError, StorageRetrievalError, StorageValueRef},
		Duration,
	},
	traits::{CheckedConversion, Convert, Hash, IdentifyAccount, Keccak256, StaticLookup},
	DigestItem, Perbill, RuntimeDebug,
};
use sp_std::prelude::*;

pub use pallet::*;

pub(crate) const LOG_TARGET: &'static str = "runtime::octopus-appchain";

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

mod mainchain;
pub mod traits;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

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
mod crypto {
	use super::KEY_TYPE;
	use sp_runtime::app_crypto::{app_crypto, sr25519};
	app_crypto!(sr25519, KEY_TYPE);
}

/// Identity of an appchain authority.
pub type AuthorityId = crypto::Public;

type AssetId = u32;
type AssetBalance = u128;

type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

type AssetBalanceOf<T> =
	<<T as Config>::Assets as fungibles::Inspect<<T as frame_system::Config>::AccountId>>::Balance;

type AssetIdOf<T> =
	<<T as Config>::Assets as fungibles::Inspect<<T as frame_system::Config>::AccountId>>::AssetId;

/// Validator of appchain.
#[derive(Deserialize, Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Validator<AccountId> {
	/// The validator's id.
	#[serde(deserialize_with = "deserialize_from_hex_str")]
	#[serde(bound(deserialize = "AccountId: Decode"))]
	id: AccountId,
	/// The weight of this validator in main chain's staking system.
	#[serde(deserialize_with = "deserialize_from_str")]
	weight: u128,
}

fn deserialize_from_hex_str<'de, S, D>(deserializer: D) -> Result<S, D::Error>
where
	S: Decode,
	D: Deserializer<'de>,
{
	let account_id_str: String = Deserialize::deserialize(deserializer)?;
	let account_id_hex =
		hex::decode(&account_id_str[2..]).map_err(|e| de::Error::custom(e.to_string()))?;
	S::decode(&mut &account_id_hex[..]).map_err(|e| de::Error::custom(e.to_string()))
}

/// The validator set of appchain.
#[derive(Deserialize, Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct ValidatorSet<AccountId> {
	/// The sequence number of this fact on the mainchain.
	#[serde(rename = "seq_num")]
	sequence_number: u32,
	set_id: u32,
	/// Validators in this set.
	#[serde(bound(deserialize = "AccountId: Decode"))]
	validators: Vec<Validator<AccountId>>,
}

#[derive(Deserialize, Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct BurnEvent<AccountId> {
	/// The sequence number of this fact on the mainchain.
	#[serde(rename = "seq_num")]
	sequence_number: u32,
	#[serde(with = "serde_bytes")]
	sender_id: Vec<u8>,
	#[serde(deserialize_with = "deserialize_from_hex_str")]
	#[serde(bound(deserialize = "AccountId: Decode"))]
	receiver: AccountId,
	#[serde(deserialize_with = "deserialize_from_str")]
	amount: u128,
}

#[derive(Deserialize, Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct LockEvent<AccountId> {
	/// The sequence number of this fact on the mainchain.
	#[serde(rename = "seq_num")]
	sequence_number: u32,
	#[serde(with = "serde_bytes")]
	token_id: Vec<u8>,
	#[serde(with = "serde_bytes")]
	sender_id: Vec<u8>,
	#[serde(deserialize_with = "deserialize_from_hex_str")]
	#[serde(bound(deserialize = "AccountId: Decode"))]
	receiver: AccountId,
	#[serde(deserialize_with = "deserialize_from_str")]
	amount: u128,
}

pub fn deserialize_from_str<'de, S, D>(deserializer: D) -> Result<S, D::Error>
where
	S: sp_std::str::FromStr,
	D: Deserializer<'de>,
	<S as sp_std::str::FromStr>::Err: ToString,
{
	let amount_str: String = Deserialize::deserialize(deserializer)?;
	amount_str.parse::<S>().map_err(|e| de::Error::custom(e.to_string()))
}

#[derive(Deserialize, Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum Observation<AccountId> {
	#[serde(bound(deserialize = "AccountId: Decode"))]
	UpdateValidatorSet(ValidatorSet<AccountId>),
	#[serde(bound(deserialize = "AccountId: Decode"))]
	Burn(BurnEvent<AccountId>),
	#[serde(bound(deserialize = "AccountId: Decode"))]
	LockAsset(LockEvent<AccountId>),
}

impl<AccountId> Observation<AccountId> {
	fn sequence_number(&self) -> u32 {
		match self {
			Observation::UpdateValidatorSet(val_set) => val_set.sequence_number,
			Observation::LockAsset(event) => event.sequence_number,
			Observation::Burn(event) => event.sequence_number,
		}
	}
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct ObservationsPayload<Public, BlockNumber, AccountId> {
	public: Public,
	block_number: BlockNumber,
	next_fact_sequence: u32,
	observations: Vec<Observation<AccountId>>,
}

impl<T: SigningTypes> SignedPayload<T>
	for ObservationsPayload<T::Public, T::BlockNumber, <T as frame_system::Config>::AccountId>
{
	fn public(&self) -> T::Public {
		self.public.clone()
	}
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct LockPayload {
	pub sender: Vec<u8>,
	pub receiver_id: Vec<u8>,
	pub amount: u128,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct BurnAssetPayload {
	pub token_id: Vec<u8>,
	pub sender: Vec<u8>,
	pub receiver_id: Vec<u8>,
	pub amount: u128,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum PayloadType {
	Lock,
	BurnAsset,
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
	pub trait Config: CreateSignedTransaction<Call<Self>> + frame_system::Config {
		/// The identifier type for an offchain worker.
		type AuthorityId: AppCrypto<Self::Public, Self::Signature>;

		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// The overarching dispatch call type.
		type Call: From<Call<Self>>;

		type PalletId: Get<PalletId>;

		type Currency: Currency<Self::AccountId>;

		type Assets: fungibles::Mutate<
			<Self as frame_system::Config>::AccountId,
			AssetId = AssetId,
			Balance = AssetBalance,
		>;
		type SessionInterface: traits::SessionInterface<Self::AccountId>;
		type LposInterface: traits::LposInterface<Self::AccountId>;

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
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::type_value]
	pub(super) fn DefaultForAppchainId() -> Vec<u8> {
		Vec::new()
	}

	#[pallet::type_value]
	pub(super) fn DefaultForRelayContract() -> Vec<u8> {
		b"octopus-relay.testnet".to_vec()
	}

	#[pallet::storage]
	#[pallet::getter(fn appchain_id)]
	pub(super) type AppchainId<T: Config> =
		StorageValue<_, Vec<u8>, ValueQuery, DefaultForAppchainId>;

	#[pallet::storage]
	#[pallet::getter(fn relay_contract)]
	pub(super) type RelayContract<T: Config> =
		StorageValue<_, Vec<u8>, ValueQuery, DefaultForRelayContract>;

	/// The current set of validators of this appchain.
	#[pallet::storage]
	pub type CurrentValidatorSet<T: Config> =
		StorageValue<_, ValidatorSet<T::AccountId>, OptionQuery>;

	#[pallet::storage]
	pub type NextValidatorSet<T: Config> = StorageValue<_, ValidatorSet<T::AccountId>, OptionQuery>;

	#[pallet::storage]
	pub type NextFactSequence<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	pub type Observations<T: Config> =
		StorageMap<_, Twox64Concat, u32, Vec<Observation<T::AccountId>>, ValueQuery>;

	#[pallet::storage]
	pub type Observing<T: Config> = StorageMap<
		_,
		Twox64Concat,
		Observation<T::AccountId>,
		Vec<Validator<T::AccountId>>,
		ValueQuery,
	>;

	#[pallet::storage]
	pub type AssetIdByName<T: Config> =
		StorageMap<_, Twox64Concat, Vec<u8>, AssetIdOf<T>, ValueQuery>;

	#[pallet::storage]
	pub type MessageQueue<T: Config> = StorageValue<_, Vec<Message>, ValueQuery>;

	#[pallet::storage]
	pub type Nonce<T: Config> = StorageValue<_, u64, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub appchain_id: String,
		pub relay_contract: String,
		pub validators: Vec<(T::AccountId, u128)>,
		pub asset_id_by_name: Vec<(String, AssetIdOf<T>)>,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				appchain_id: String::new(),
				relay_contract: String::new(),
				validators: Vec::new(),
				asset_id_by_name: Vec::new(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			<AppchainId<T>>::put(self.appchain_id.as_bytes());
			<RelayContract<T>>::put(self.relay_contract.as_bytes());

			Pallet::<T>::initialize_validators(&self.validators);

			for (token_id, id) in self.asset_id_by_name.iter() {
				<AssetIdByName<T>>::insert(token_id.as_bytes(), id);
			}
		}
	}

	#[pallet::event]
	#[pallet::metadata(
		T::AccountId = "AccountId",
		BalanceOfAsset<T> = "Balance",
		AssetIdOfAsset<T> = "AssetId"
	)]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Event generated when a new voter votes on a validator set.
		/// \[validator_set, voter\]
		NewVoterFor(ValidatorSet<T::AccountId>, T::AccountId),
		Locked(T::AccountId, Vec<u8>, BalanceOf<T>),
		Unlocked(Vec<u8>, T::AccountId, BalanceOf<T>),
		AssetMinted(AssetIdOf<T>, Vec<u8>, T::AccountId, AssetBalanceOf<T>),
		AssetBurned(AssetIdOf<T>, T::AccountId, Vec<u8>, AssetBalanceOf<T>),
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// No CurrentValidatorSet.
		NoCurrentValidatorSet,
		/// The set id of new validator set was wrong.
		WrongSetId,
		/// Must be a validator.
		NotValidator,
		/// Nonce overflow.
		NonceOverflow,
		/// Amount overflow.
		AmountOverflow,
		/// Next fact sequence overflow.
		NextFactSequenceOverflow,
		/// Wrong Asset Id.
		WrongAssetId,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Initialization
		fn on_initialize(now: BlockNumberFor<T>) -> Weight {
			Self::commit()
		}

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
			let appchain_id = Self::appchain_id();
			if appchain_id.len() == 0 {
				// detach appchain from main chain when appchain_id == ""
				return;
			}
			let parent_hash = <frame_system::Pallet<T>>::block_hash(block_number - 1u32.into());
			log!(info, "Current block: {:?} (parent hash: {:?})", block_number, parent_hash);

			if !Self::should_send(block_number) {
				return;
			}

			let relay_contract = Self::relay_contract();
			// TODO: move limit to trait
			let limit = 10;
			if let Err(e) =
				Self::observing_mainchain(block_number, relay_contract, appchain_id.clone(), limit)
			{
				log!(info, "observing_mainchain: Error: {}", e);
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
			if let Call::submit_observations(ref payload, ref signature) = call {
				let signature_valid =
					SignedPayload::<T>::verify::<T::AuthorityId>(payload, signature.clone());
				if !signature_valid {
					return InvalidTransaction::BadProof.into();
				}
				Self::validate_transaction_parameters(
					&payload.block_number,
					payload.next_fact_sequence,
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
			let cur_val_set =
				<CurrentValidatorSet<T>>::get().ok_or(Error::<T>::NoCurrentValidatorSet)?;
			let who = payload.public.clone().into_account();

			let val = cur_val_set.validators.iter().find(|v| {
				T::SessionInterface::same_validator(
					KEY_TYPE,
					&payload.public.clone().into_account().encode(),
					v.id.clone(),
				)
			});
			if val.is_none() {
				log!(
					info,
					"Not a validator in current validator set: {:?}",
					payload.public.clone().into_account()
				);
				return Err(Error::<T>::NotValidator.into());
			}
			let val = val.expect("Validator is valid; qed").clone();

			//
			log!(info, "️️️observations: {:#?},\nwho: {:?}", payload.observations, who);
			//

			for observation in payload.observations.iter() {
				Self::submit_observation(observation.clone(), &cur_val_set, &val)?;
			}

			Ok(().into())
		}

		// cross chain transfer

		// There are 2 kinds of assets:
		// 1. native token on appchain
		// mainchain:mint() <- appchain:lock()
		// mainchain:burn() -> appchain:unlock()
		//
		// 2. NEP141 asset on mainchain
		// mainchain:lock_asset()   -> appchain:mint_asset()
		// mainchain:unlock_asset() <- appchain:burn_asset()

		#[pallet::weight(0)]
		#[transactional]
		pub fn lock(
			origin: OriginFor<T>,
			receiver_id: Vec<u8>,
			amount: BalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			T::Currency::transfer(&who, &Self::account_id(), amount, AllowDeath)?;

			let amount_wrapped: u128 = amount.checked_into().ok_or(Error::<T>::AmountOverflow)?;
			let prefix = String::from("0x");
			let hex_sender = prefix + &hex::encode(who.encode());
			let message = LockPayload {
				sender: hex_sender.into_bytes(),
				receiver_id: receiver_id.clone(),
				amount: amount_wrapped,
			};

			Self::submit(&who, PayloadType::Lock, &message.try_to_vec().unwrap())?;
			Self::deposit_event(Event::Locked(who, receiver_id, amount));

			Ok(().into())
		}

		#[pallet::weight(0)]
		#[transactional]
		pub fn mint_asset(
			origin: OriginFor<T>,
			asset_id: AssetIdOf<T>,
			sender_id: Vec<u8>,
			receiver: <T::Lookup as StaticLookup>::Source,
			amount: AssetBalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			let receiver = T::Lookup::lookup(receiver)?;
			Self::mint_asset_inner(asset_id, sender_id, receiver, amount)
		}

		#[pallet::weight(0)]
		#[transactional]
		pub fn burn_asset(
			origin: OriginFor<T>,
			asset_id: AssetIdOf<T>,
			receiver_id: Vec<u8>,
			amount: AssetBalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			let sender = ensure_signed(origin)?;

			let token_id = <AssetIdByName<T>>::iter()
				.find(|p| p.1 == asset_id)
				.map(|p| p.0)
				.ok_or(Error::<T>::WrongAssetId)?;

			<T::Assets as fungibles::Mutate<T::AccountId>>::burn_from(asset_id, &sender, amount)?;

			let prefix = String::from("0x");
			let hex_sender = prefix + &hex::encode(sender.encode());
			let message = BurnAssetPayload {
				token_id,
				sender: hex_sender.into_bytes(),
				receiver_id: receiver_id.clone(),
				amount,
			};

			Self::submit(&sender, PayloadType::BurnAsset, &message.try_to_vec().unwrap())?;
			Self::deposit_event(Event::AssetBurned(asset_id, sender, receiver_id, amount));

			Ok(().into())
		}
	}

	impl<T: Config> Pallet<T> {
		fn account_id() -> T::AccountId {
			T::PalletId::get().into_account()
		}

		fn initialize_validators(vals: &Vec<(<T as frame_system::Config>::AccountId, u128)>) {
			if vals.len() != 0 {
				assert!(
					<CurrentValidatorSet<T>>::get().is_none(),
					"CurrentValidatorSet is already initialized!"
				);
				<CurrentValidatorSet<T>>::put(ValidatorSet {
					sequence_number: 0, // unused
					set_id: 0,
					validators: vals
						.iter()
						.map(|x| Validator { id: x.0.clone(), weight: x.1 })
						.collect::<Vec<_>>(),
				});
			}
		}

		fn should_send(block_number: T::BlockNumber) -> bool {
			/// A friendlier name for the error that is going to be returned in case we are in the grace
			/// period.
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
						// If we already have a value in storage and the block number is recent enough
						// we avoid sending another transaction at this time.
						Ok(Some(block)) if block_number < block + T::GracePeriod::get() => {
							Err(RECENTLY_SENT)
						}
						// In every other case we attempt to acquire the lock and send a transaction.
						_ => Ok(block_number),
					}
				});

			// The result of `mutate` call will give us a nested `Result` type.
			// The first one matches the return of the closure passed to `mutate`, i.e.
			// if we return `Err` from the closure, we get an `Err` here.
			// In case we return `Ok`, here we will have another (inner) `Result` that indicates
			// if the value has been set to the storage correctly - i.e. if it wasn't
			// written to in the meantime.
			match res {
				// The value has been set correctly, which means we can safely send a transaction now.
				Ok(_block_number) => true,
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

		pub(crate) fn observing_mainchain(
			block_number: T::BlockNumber,
			relay_contract: Vec<u8>,
			appchain_id: Vec<u8>,
			limit: u32,
		) -> Result<(), &'static str> {
			log!(info, "in observing_mainchain");

			let next_fact_sequence = NextFactSequence::<T>::get();
			log!(info, "next_fact_sequence: {}", next_fact_sequence);

			// Make an external HTTP request to fetch facts from main chain.
			// Note this call will block until response is received.

			let mut obs = Self::fetch_facts(relay_contract, appchain_id, next_fact_sequence, limit)
				.map_err(|_| "Failed to fetch facts")?;

			if obs.len() == 0 {
				return Ok(());
			}
			// sort observations by sequence_number
			obs.sort_by(|a, b| a.sequence_number().cmp(&b.sequence_number()));

			// there can only be one update_validator_set at most
			let mut iter =
				obs.split_inclusive(|o| matches! {o, Observation::UpdateValidatorSet(_)});
			let obs = iter.next().unwrap().to_vec();

			// -- Sign using any account
			let (_, result) = Signer::<T, T::AuthorityId>::any_account()
				.send_unsigned_transaction(
					|account| ObservationsPayload {
						public: account.public.clone(),
						block_number,
						next_fact_sequence,
						observations: obs.clone(),
					},
					|payload, signature| Call::submit_observations(payload, signature),
				)
				.ok_or("No local accounts accounts available.")?;
			result.map_err(|()| "Unable to submit transaction")?;

			Ok(())
		}

		fn unlock_inner(
			sender_id: Vec<u8>,
			receiver: T::AccountId,
			amount: u128,
		) -> DispatchResultWithPostInfo {
			let amount_unwrapped = amount.checked_into().ok_or(Error::<T>::AmountOverflow)?;
			// unlock native token
			T::Currency::transfer(&Self::account_id(), &receiver, amount_unwrapped, KeepAlive)?;
			Self::deposit_event(Event::Unlocked(sender_id, receiver, amount_unwrapped));

			Ok(().into())
		}

		fn mint_asset_inner(
			asset_id: AssetIdOf<T>,
			sender_id: Vec<u8>,
			receiver: T::AccountId,
			amount: AssetBalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			<T::Assets as fungibles::Mutate<T::AccountId>>::mint_into(asset_id, &receiver, amount)?;
			Self::deposit_event(Event::AssetMinted(asset_id, sender_id, receiver, amount));

			Ok(().into())
		}

		/// If the observation already exists in the Observations, then the only thing
		/// to do is vote for this observation.
		fn submit_observation(
			observation: Observation<T::AccountId>,
			validator_set: &ValidatorSet<T::AccountId>,
			validator: &Validator<T::AccountId>,
		) -> DispatchResultWithPostInfo {
			<Observations<T>>::mutate(observation.sequence_number(), |obs| {
				let found = obs.iter().any(|o| o == &observation);
				if !found {
					obs.push(observation.clone())
				}
			});
			<Observing<T>>::mutate(&observation, |vals| {
				let found = vals.iter().any(|v| v.id == validator.id);
				if !found {
					vals.push(validator.clone());
				} else {
					log!(info, "{:?} submits a duplicate ocw tx", validator.id);
				}
			});
			let total_weight: u128 = validator_set.validators.iter().map(|v| v.weight).sum();
			let weight: u128 = <Observing<T>>::get(&observation).iter().map(|v| v.weight).sum();

			//
			log!(
				info,
				"️️️observations: {:#?}",
				<Observations<T>>::get(observation.sequence_number())
			);
			log!(info, "️️️observer: {:#?}", <Observing<T>>::get(&observation));
			log!(info, "️️️total_weight: {:?}, weight: {:?}", total_weight, weight);
			//

			let seq_num = observation.sequence_number();

			if 3 * weight > 2 * total_weight {
				match observation.clone() {
					Observation::UpdateValidatorSet(val_set) => {
						ensure!(val_set.set_id == validator_set.set_id + 1, Error::<T>::WrongSetId);

						<NextValidatorSet<T>>::put(val_set);
					}
					Observation::Burn(event) => {
						if let Err(error) =
							Self::unlock_inner(event.sender_id, event.receiver, event.amount)
						{
							log!(info, "️️️failed to unlock native token: {:?}", error);
							return Err(error);
						}
					}
					Observation::LockAsset(event) => {
						if let Ok(asset_id) = <AssetIdByName<T>>::try_get(&event.token_id) {
							log!(
								info,
								"️️️mint asset:{:?}, sender_id:{:?}, receiver:{:?}, amount:{:?}",
								asset_id,
								event.sender_id,
								event.receiver,
								event.amount,
							);
							if let Err(error) = Self::mint_asset_inner(
								asset_id,
								event.sender_id,
								event.receiver,
								event.amount,
							) {
								log!(info, "️️️failed to mint asset: {:?}", error);
								return Err(error);
							}
						} else {
							return Err(Error::<T>::WrongAssetId.into());
						}
					}
				}

				let obs = <Observations<T>>::get(seq_num);
				for o in obs.iter() {
					<Observing<T>>::remove(o);
				}
				<Observations<T>>::remove(seq_num);

				if matches!(observation, Observation::LockAsset(_))
					|| matches!(observation, Observation::Burn(_))
				{
					NextFactSequence::<T>::try_mutate(|next_seq| -> DispatchResultWithPostInfo {
						if let Some(v) = next_seq.checked_add(1) {
							*next_seq = v;
						} else {
							return Err(Error::<T>::NextFactSequenceOverflow.into());
						}
						Ok(().into())
					})?;
				}
			}

			Ok(().into())
		}

		fn validate_transaction_parameters(
			block_number: &T::BlockNumber,
			next_fact_sequence: u32,
			account_id: <T as frame_system::Config>::AccountId,
		) -> TransactionValidity {
			// Let's make sure to reject transactions from the future.
			let current_block = <frame_system::Pallet<T>>::block_number();
			if &current_block < block_number {
				log!(
					info,
					"InvalidTransaction => current_block: {:?}, block_number: {:?}",
					current_block,
					block_number
				);
				return InvalidTransaction::Future.into();
			}

			ValidTransaction::with_tag_prefix("OctopusAppchain")
				// We set base priority to 2**20 and hope it's included before any other
				// transactions in the pool. Next we tweak the priority depending on the
				// sequence of the fact that happened on main chain.
				.priority(T::UnsignedPriority::get().saturating_add(next_fact_sequence as u64))
				// This transaction does not require anything else to go before into the pool.
				//.and_requires()
				// One can only vote on the validator set with the same seq_num once.
				.and_provides((next_fact_sequence, account_id))
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

	impl<T: Config> sp_runtime::BoundToRuntimeAppPublic for Pallet<T> {
		type Public = AuthorityId;
	}

	impl<T: Config> OneSessionHandler<T::AccountId> for Pallet<T> {
		type Key = AuthorityId;

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

		fn on_disabled(_i: usize) {
			// ignore
		}
	}

	impl<T: Config> traits::SessionInterface<<T as frame_system::Config>::AccountId> for T
	where
		T: pallet_session::Config<ValidatorId = <T as frame_system::Config>::AccountId>,
	{
		fn same_validator(
			id: KeyTypeId,
			key_data: &[u8],
			validator: <T as frame_system::Config>::AccountId,
		) -> bool {
			let who = <pallet_session::Pallet<T>>::key_owner(id, key_data);
			if who.is_none() {
				return false;
			}
			log!(info, "check {:#?} == {:#?}", validator, who);

			T::ValidatorIdOf::convert(validator) == who
		}
	}

	impl<T: Config> traits::ElectionProvider<T::AccountId> for Pallet<T> {
		fn elect() -> Supports<T::AccountId> {
			let mut staked = vec![];
			let mut winners = vec![];
			let next_val_set = <NextValidatorSet<T>>::get();
			match next_val_set {
				Some(new_val_set) => {
					// TODO
					let res = NextFactSequence::<T>::try_mutate(
						|next_seq| -> DispatchResultWithPostInfo {
							if let Some(v) = next_seq.checked_add(1) {
								*next_seq = v;
							} else {
								log!(info, "fact sequence overflow: {:?}", next_seq);
								return Err(Error::<T>::NextFactSequenceOverflow.into());
							}
							Ok(().into())
						},
					);

					// TODO: ugly
					if let Ok(_) = res {
						<CurrentValidatorSet<T>>::put(new_val_set.clone());
						<NextValidatorSet<T>>::kill();
						log!(info, "validator set changed to: {:#?}", new_val_set.clone());
						staked = new_val_set
							.clone()
							.validators
							.into_iter()
							.map(|vals| StakedAssignment {
								who: vals.id.clone(),
								distribution: vec![(vals.id, vals.weight)],
							})
							.collect();
						winners = new_val_set
							.validators
							.into_iter()
							.map(|vals| {
								T::LposInterface::bond_and_validate(
									vals.id.clone(),
									vals.weight,
									Perbill::zero(),
									false,
								);
								vals.id
							})
							.collect();
					}
				}
				None => {
					log!(info, "validator set has't changed");

					let current_val_set = <CurrentValidatorSet<T>>::get()
						.expect("Current validator set is valid; qed");
					staked = current_val_set
						.clone()
						.validators
						.into_iter()
						.map(|vals| StakedAssignment {
							who: vals.id.clone(),
							distribution: vec![(vals.id, vals.weight)],
						})
						.collect();
					winners = current_val_set
						.validators
						.into_iter()
						.map(|vals| {
							T::LposInterface::bond_and_validate(
								vals.id.clone(),
								vals.weight,
								Perbill::zero(),
								false,
							);
							vals.id
						})
						.collect();
				}
			}

			to_supports(&winners, &staked).unwrap()
		}
	}
}
