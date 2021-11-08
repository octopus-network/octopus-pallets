#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use borsh::BorshSerialize;
use codec::{Decode, Encode};
use frame_support::{
	traits::{
		tokens::fungibles,
		Currency,
		ExistenceRequirement::{AllowDeath, KeepAlive},
		OneSessionHandler,
	},
	transactional, PalletId,
};
use frame_system::offchain::{
	AppCrypto, CreateSignedTransaction, SendUnsignedTransaction, SignedPayload, Signer,
	SigningTypes,
};
use pallet_octopus_support::{
	log,
	traits::{AppchainInterface, LposInterface, UpwardMessagesInterface, ValidatorsProvider},
	types::{BurnAssetPayload, LockPayload, PayloadType},
};
use scale_info::TypeInfo;
use serde::{de, Deserialize, Deserializer};
use sp_core::crypto::KeyTypeId;
use sp_runtime::RuntimeAppPublic;
use sp_runtime::{
	offchain::{
		http,
		storage::{MutateStorageError, StorageRetrievalError, StorageValueRef},
		Duration,
	},
	traits::{AccountIdConversion, CheckedConversion, IdentifyAccount, StaticLookup},
	RuntimeDebug,
};
use sp_std::prelude::*;

pub use pallet::*;

pub(crate) const LOG_TARGET: &'static str = "runtime::octopus-appchain";

mod mainchain;
mod weights;

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
#[derive(Deserialize, Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct Validator<AccountId> {
	/// The validator's id.
	#[serde(deserialize_with = "deserialize_from_hex_str")]
	#[serde(bound(deserialize = "AccountId: Decode"))]
	validator_id_in_appchain: AccountId,
	/// The total stake of this validator in mainchain's staking system.
	#[serde(deserialize_with = "deserialize_from_str")]
	total_stake: u128,
}

#[derive(Deserialize, Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct ValidatorSet<AccountId> {
	/// The anchor era that this set belongs to.
	set_id: u32,
	/// Validators in this set.
	#[serde(bound(deserialize = "AccountId: Decode"))]
	validators: Vec<Validator<AccountId>>,
}

/// Appchain token burn event.
#[derive(Deserialize, Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct BurnEvent<AccountId> {
	#[serde(default)]
	index: u32,
	#[serde(rename = "sender_id_in_near")]
	#[serde(with = "serde_bytes")]
	sender_id: Vec<u8>,
	#[serde(rename = "receiver_id_in_appchain")]
	#[serde(deserialize_with = "deserialize_from_hex_str")]
	#[serde(bound(deserialize = "AccountId: Decode"))]
	receiver: AccountId,
	#[serde(deserialize_with = "deserialize_from_str")]
	amount: u128,
}

/// Token locked event.
#[derive(Deserialize, Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct LockAssetEvent<AccountId> {
	#[serde(default)]
	index: u32,
	#[serde(rename = "symbol")]
	#[serde(with = "serde_bytes")]
	token_id: Vec<u8>,
	#[serde(rename = "sender_id_in_near")]
	#[serde(with = "serde_bytes")]
	sender_id: Vec<u8>,
	#[serde(rename = "receiver_id_in_appchain")]
	#[serde(deserialize_with = "deserialize_from_hex_str")]
	#[serde(bound(deserialize = "AccountId: Decode"))]
	receiver: AccountId,
	#[serde(deserialize_with = "deserialize_from_str")]
	amount: u128,
}

#[derive(Deserialize, Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub enum AppchainNotification<AccountId> {
	#[serde(rename = "NearFungibleTokenLocked")]
	#[serde(bound(deserialize = "AccountId: Decode"))]
	LockAsset(LockAssetEvent<AccountId>),

	#[serde(rename = "WrappedAppchainTokenBurnt")]
	#[serde(bound(deserialize = "AccountId: Decode"))]
	Burn(BurnEvent<AccountId>),
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

pub fn deserialize_from_str<'de, S, D>(deserializer: D) -> Result<S, D::Error>
where
	S: sp_std::str::FromStr,
	D: Deserializer<'de>,
	<S as sp_std::str::FromStr>::Err: ToString,
{
	let amount_str: String = Deserialize::deserialize(deserializer)?;
	amount_str.parse::<S>().map_err(|e| de::Error::custom(e.to_string()))
}

#[derive(Deserialize, Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub enum Observation<AccountId> {
	#[serde(bound(deserialize = "AccountId: Decode"))]
	UpdateValidatorSet(ValidatorSet<AccountId>),
	#[serde(bound(deserialize = "AccountId: Decode"))]
	LockAsset(LockAssetEvent<AccountId>),
	#[serde(bound(deserialize = "AccountId: Decode"))]
	Burn(BurnEvent<AccountId>),
}

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub enum ObservationType {
	UpdateValidatorSet,
	Burn,
	LockAsset,
}

impl<AccountId> Observation<AccountId> {
	fn observation_index(&self) -> u32 {
		match self {
			Observation::UpdateValidatorSet(set) => set.set_id,
			Observation::LockAsset(event) => event.index,
			Observation::Burn(event) => event.index,
		}
	}
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
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

impl<T: Config> AppchainInterface for Pallet<T> {
	fn is_activated() -> bool {
		IsActivated::<T>::get()
	}

	fn next_set_id() -> u32 {
		NextSetId::<T>::get()
	}
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

		#[pallet::constant]
		type RequestEventLimit: Get<u32>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::type_value]
	pub(super) fn DefaultForAnchorContract() -> Vec<u8> {
		Vec::new()
	}

	#[pallet::storage]
	#[pallet::getter(fn anchor_contract)]
	pub(super) type AnchorContract<T: Config> =
		StorageValue<_, Vec<u8>, ValueQuery, DefaultForAnchorContract>;

	/// Whether the appchain is activated.
	///
	/// Only an active appchain will communicate with the mainchain and pay block rewards.
	#[pallet::storage]
	pub(super) type IsActivated<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::storage]
	pub type NextSetId<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	pub type PlannedValidators<T: Config> = StorageValue<_, Vec<(T::AccountId, u128)>, ValueQuery>;

	#[pallet::storage]
	pub type NextFactSequence<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	pub type Observations<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		ObservationType,
		Twox64Concat,
		u32,
		Vec<Observation<T::AccountId>>,
		ValueQuery,
	>;

	#[pallet::storage]
	pub type Observing<T: Config> =
		StorageMap<_, Twox64Concat, Observation<T::AccountId>, Vec<T::AccountId>, ValueQuery>;

	#[pallet::storage]
	pub type AssetIdByName<T: Config> =
		StorageMap<_, Twox64Concat, Vec<u8>, AssetIdOf<T>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn pallet_account)]
	pub type PalletAccount<T: Config> = StorageValue<_, T::AccountId, ValueQuery>;

	#[pallet::storage]
	pub type NextSubmitObsIndex<T> = StorageValue<_, u32>;

	#[pallet::storage]
	pub type SubmitSequenceNumber<T> = StorageValue<_, u32>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub anchor_contract: String,
		pub asset_id_by_name: Vec<(String, AssetIdOf<T>)>,
		pub validators: Vec<(T::AccountId, u128)>,
		pub premined_amount: u128,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				anchor_contract: String::new(),
				asset_id_by_name: Vec::new(),
				validators: Vec::new(),
				premined_amount: 0,
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			<AnchorContract<T>>::put(self.anchor_contract.as_bytes());

			for (token_id, id) in self.asset_id_by_name.iter() {
				<AssetIdByName<T>>::insert(token_id.as_bytes(), id);
			}
			<PlannedValidators<T>>::put(self.validators.clone());

			let account_id = <Pallet<T>>::account_id();
			let min = T::Currency::minimum_balance();
			let amount =
				self.premined_amount.checked_into().ok_or(Error::<T>::AmountOverflow).unwrap();
			if amount >= min {
				T::Currency::make_free_balance_be(&account_id, amount);
			}

			<PalletAccount<T>>::put(account_id);
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		Locked(T::AccountId, Vec<u8>, BalanceOf<T>),
		Unlocked(Vec<u8>, T::AccountId, BalanceOf<T>),
		AssetMinted(AssetIdOf<T>, Vec<u8>, T::AccountId, AssetBalanceOf<T>),
		AssetBurned(AssetIdOf<T>, T::AccountId, Vec<u8>, AssetBalanceOf<T>),
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// The set id of new validator set was wrong.
		WrongSetId,
		/// Must be a validator.
		NotValidator,
		/// Amount overflow.
		AmountOverflow,
		/// Next fact sequence overflow.
		NextFactSequenceOverflow,
		/// Wrong Asset Id.
		WrongAssetId,
		/// Invalid active total stake.
		InvalidActiveTotalStake,
		/// Next validators submit index overflow.
		NextSubmitObsIndexOverflow,
		/// Appchain is not activated.
		NotActivated,
		/// ReceiverId is not a valid utf8 string.
		InvalidReceiverId,
		/// Token is not a valid utf8 string.
		InvalidTokenId,
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
			if !IsActivated::<T>::get() || anchor_contract.is_empty() {
				return;
			}
			let parent_hash = <frame_system::Pallet<T>>::block_hash(block_number - 1u32.into());
			log!(debug, "Current block: {:?} (parent hash: {:?})", block_number, parent_hash);

			// Only communicate with mainchain if we are validators.
			let mut obser_id: Option<T::AccountId> = None;
			if sp_io::offchain::is_validator() {
				let mut found = false;
				for key in <T::AuthorityId as AppCrypto<
					<T as SigningTypes>::Public,
					<T as SigningTypes>::Signature,
				>>::RuntimeAppPublic::all()
				.into_iter()
				{
					let generic_public = <T::AuthorityId as AppCrypto<
						<T as SigningTypes>::Public,
						<T as SigningTypes>::Signature,
					>>::GenericPublic::from(key);
					let public: <T as SigningTypes>::Public = generic_public.into();

					let val_id = T::LposInterface::is_active_validator(
						KEY_TYPE,
						&public.clone().into_account().encode(),
					);
					log!(debug, "Check if in current set {:?}", val_id);
					if val_id.is_some() {
						found = true;
						obser_id = val_id;
					}
				}
				if !found {
					log!(info, "Skipping communication with mainchain. Not a validator.");
					return;
				}
				if !Self::should_send(block_number) {
					return;
				}
				let obser_id = obser_id.unwrap();
				if let Err(e) = Self::observing_mainchain(block_number, anchor_contract, obser_id) {
					log!(warn, "observing_mainchain: Error: {}", e);
				}
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
			let who = payload.public.clone().into_account();
			let val_id = T::LposInterface::is_active_validator(
				KEY_TYPE,
				&payload.public.clone().into_account().encode(),
			);

			if val_id.is_none() {
				log!(
					warn,
					"Not a validator in current validator set: {:?}",
					payload.public.clone().into_account()
				);
				return Err(Error::<T>::NotValidator.into());
			}
			let val_id = val_id.expect("Validator is valid; qed").clone();

			//
			log!(debug, "️️️observations: {:#?},\nwho: {:?}", payload.observations, who);
			//

			// TODO
			// ensure!(val_set.set_id == validator_set.set_id + 1, Error::<T>::WrongSetId);
			for observation in payload.observations.iter() {
				Self::submit_observation(&val_id, observation.clone())?;
			}

			Ok(().into())
		}

		#[pallet::weight(0)]
		pub fn force_set_is_activated(origin: OriginFor<T>, is_activated: bool) -> DispatchResult {
			ensure_root(origin)?;
			<IsActivated<T>>::put(is_activated);
			Ok(())
		}

		#[pallet::weight(0)]
		pub fn force_set_next_set_id(origin: OriginFor<T>, next_set_id: u32) -> DispatchResult {
			ensure_root(origin)?;
			<NextSetId<T>>::put(next_set_id);
			Ok(())
		}

		// Force set planned validators with sudo permissions.
		#[pallet::weight(0)]
		pub fn force_set_planned_validators(
			origin: OriginFor<T>,
			validators: Vec<(T::AccountId, u128)>,
		) -> DispatchResult {
			ensure_root(origin)?;
			<PlannedValidators<T>>::put(validators);
			Ok(())
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
			ensure!(IsActivated::<T>::get(), Error::<T>::NotActivated);
			let next_set_id = NextSetId::<T>::get();
			ensure!(next_set_id != 0, Error::<T>::NotActivated);

			let receiver_id =
				String::from_utf8(receiver_id).map_err(|_| Error::<T>::InvalidReceiverId)?;

			let current_set_id = next_set_id - 1;
			let amount_wrapped: u128 = amount.checked_into().ok_or(Error::<T>::AmountOverflow)?;

			T::Currency::transfer(&who, &Self::account_id(), amount, AllowDeath)?;

			let prefix = String::from("0x");
			let hex_sender = prefix + &hex::encode(who.encode());
			let message = LockPayload {
				sender: hex_sender.clone(),
				receiver_id: receiver_id.clone(),
				amount: amount_wrapped,
				era: current_set_id,
			};

			T::UpwardMessagesInterface::submit(
				&who,
				PayloadType::Lock,
				&message.try_to_vec().unwrap(),
			)?;
			Self::deposit_event(Event::Locked(who, receiver_id.as_bytes().to_vec(), amount));

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
			ensure!(IsActivated::<T>::get(), Error::<T>::NotActivated);
			let next_set_id = NextSetId::<T>::get();
			ensure!(next_set_id != 0, Error::<T>::NotActivated);

			let receiver_id =
				String::from_utf8(receiver_id).map_err(|_| Error::<T>::InvalidReceiverId)?;

			let current_set_id = next_set_id - 1;

			let token_id = <AssetIdByName<T>>::iter()
				.find(|p| p.1 == asset_id)
				.map(|p| p.0)
				.ok_or(Error::<T>::WrongAssetId)?;

			let token_id = String::from_utf8(token_id).map_err(|_| Error::<T>::InvalidTokenId)?;

			<T::Assets as fungibles::Mutate<T::AccountId>>::burn_from(asset_id, &sender, amount)?;

			let prefix = String::from("0x");
			let hex_sender = prefix + &hex::encode(sender.encode());
			let message = BurnAssetPayload {
				token_id,
				sender: hex_sender,
				receiver_id: receiver_id.clone(),
				amount,
				era: current_set_id,
			};

			T::UpwardMessagesInterface::submit(
				&sender,
				PayloadType::BurnAsset,
				&message.try_to_vec().unwrap(),
			)?;
			Self::deposit_event(Event::AssetBurned(
				asset_id,
				sender,
				receiver_id.as_bytes().to_vec(),
				amount,
			));

			Ok(().into())
		}
	}

	impl<T: Config> Pallet<T> {
		fn account_id() -> T::AccountId {
			T::PalletId::get().into_account()
		}

		fn default_rpc_endpoint() -> String {
			"https://rpc.testnet.near.org".to_string()
		}

		fn get_rpc_endpoint() -> String {
			let kind = sp_core::offchain::StorageKind::PERSISTENT;
			if let Some(data) = sp_io::offchain::local_storage_get(kind, b"rpc") {
				if let Ok(rpc_url) = String::from_utf8(data) {
					log!(debug, "The configure url is {:?} ", rpc_url.clone());
					return rpc_url;
				} else {
					log!(warn, "Parse configure url error, return default rpc url");
					return Self::default_rpc_endpoint();
				}
			} else {
				log!(debug, "No configuration for rpc, return default rpc url");
				return Self::default_rpc_endpoint();
			}
		}

		// TODO: hard to understand, need to simplify this code
		fn should_get_validators(val_id: T::AccountId) -> bool {
			let next_set_id = NextSetId::<T>::get();
			match <NextSubmitObsIndex<T>>::try_get() {
				Ok(next_index) => {
					log!(debug, "next_index: {}, next_set_id: {}", next_index, next_set_id);
					if next_index > next_set_id - 1 {
						return false;
					}
				}
				Err(_) => {
					return true;
				}
			}

			match <SubmitSequenceNumber<T>>::try_get() {
				Ok(sequence_number) => {
					let observations = <Observations<T>>::get(
						ObservationType::UpdateValidatorSet,
						sequence_number,
					);
					log!(debug, "observations: {:#?},\n val_id: {:#?}", observations, val_id);

					let mut found = false;
					for observation in observations.iter() {
						if <Observing<T>>::get(&observation).iter().any(|o| o == &val_id) {
							log!(debug, "obser: {:#?}", <Observing<T>>::get(&observation));
							found = true;
							break;
						}
					}
					return !found;
				}
				Err(_) => {
					return true;
				}
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
			anchor_contract: Vec<u8>,
			val_id: T::AccountId,
		) -> Result<(), &'static str> {
			// Make an external HTTP request to fetch events from mainchain.
			// Note this call will block until response is received.
			let mut obs: Vec<Observation<<T as frame_system::Config>::AccountId>> = Vec::new();
			let next_fact_sequence = NextFactSequence::<T>::get();
			log!(debug, "next_fact_sequence: {}", next_fact_sequence);
			let next_set_id = NextSetId::<T>::get();
			log!(debug, "next_set_id: {}", next_set_id);
			let rpc_url = Self::get_rpc_endpoint();
			log!(debug, "The current rpc_url is {:?}", rpc_url);

			if Self::should_get_validators(val_id) {
				obs = Self::get_validator_list_of(&rpc_url, anchor_contract.clone(), next_set_id)
					.map_err(|_| "Failed to get_validator_list_of")?;
			}

			if obs.len() == 0 {
				log!(debug, "No validat_set updates, try to get appchain notifications.");
				// check cross-chain transfers only if there isn't a validator_set update.
				obs = Self::get_appchain_notification_histories(
					&rpc_url,
					anchor_contract,
					next_fact_sequence,
					T::RequestEventLimit::get(),
				)
				.map_err(|_| "Failed to get_appchain_notification_histories")?;
			}

			if obs.len() == 0 {
				log!(debug, "No messages from mainchain.");
				return Ok(());
			}

			// -- Sign using any account
			let (_, result) = Signer::<T, T::AuthorityId>::any_account()
				.send_unsigned_transaction(
					|account| ObservationsPayload {
						public: account.public.clone(),
						block_number,
						next_fact_sequence,
						observations: obs.clone(),
					},
					|payload, signature| Call::submit_observations { payload, signature },
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
		#[transactional]
		fn submit_observation(
			validator_id: &T::AccountId,
			observation: Observation<T::AccountId>,
		) -> DispatchResultWithPostInfo {
			let observation_type = Self::get_observation_type(&observation);
			<Observations<T>>::mutate(observation_type, observation.observation_index(), |obs| {
				let found = obs.iter().any(|o| o == &observation);
				if !found {
					obs.push(observation.clone())
				}
			});
			<Observing<T>>::mutate(&observation, |vals| {
				let found = vals.iter().any(|id| id == validator_id);
				if !found {
					vals.push(validator_id.clone());
				} else {
					log!(warn, "{:?} submits a duplicate ocw tx", validator_id);
				}
			});
			let total_stake: u128 = T::LposInterface::active_total_stake()
				.ok_or(Error::<T>::InvalidActiveTotalStake)?;
			let stake: u128 = <Observing<T>>::get(&observation)
				.iter()
				.map(|v| T::LposInterface::active_stake_of(v))
				.sum();

			let obs_id = observation.observation_index();
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

			match observation {
				Observation::UpdateValidatorSet(_) => {
					<SubmitSequenceNumber<T>>::put(obs_id);
				}
				_ => log!(debug, "️️️observation not include validator sets"),
			}

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
						let next_set_id = NextSetId::<T>::get();
						<NextSubmitObsIndex<T>>::put(next_set_id);
						<SubmitSequenceNumber<T>>::kill();
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
								log!(warn, "️️️failed to mint asset: {:?}", error);
								return Err(error);
							}
						} else {
							return Err(Error::<T>::WrongAssetId.into());
						}
					}
				}

				let obs = <Observations<T>>::get(observation_type, obs_id);
				for o in obs.iter() {
					<Observing<T>>::remove(o);
				}
				<Observations<T>>::remove(observation_type, obs_id);

				if matches!(observation_type, ObservationType::LockAsset)
					|| matches!(observation_type, ObservationType::Burn)
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

		fn get_observation_type(observation: &Observation<T::AccountId>) -> ObservationType {
			match observation.clone() {
				Observation::UpdateValidatorSet(_) => {
					return ObservationType::UpdateValidatorSet;
				}
				Observation::Burn(_) => {
					return ObservationType::Burn;
				}
				Observation::LockAsset(_) => {
					return ObservationType::LockAsset;
				}
			}
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
					warn,
					"InvalidTransaction => current_block: {:?}, block_number: {:?}",
					current_block,
					block_number
				);
				return InvalidTransaction::Future.into();
			}

			ValidTransaction::with_tag_prefix("OctopusAppchain")
				// We set base priority to 2**20 and hope it's included before any other
				// transactions in the pool. Next we tweak the priority depending on the
				// sequence of the fact that happened on mainchain.
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

		fn on_disabled(_i: u32) {
			// ignore
		}
	}

	impl<T: Config> ValidatorsProvider<T::AccountId> for Pallet<T> {
		fn validators() -> Vec<(T::AccountId, u128)> {
			<PlannedValidators<T>>::get()
		}
	}
}
