// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

use borsh::BorshSerialize;
use codec::{Codec, Decode, Encode, HasCompact};
use frame_support::{
	sp_runtime::traits::AtLeast32BitUnsigned,
	sp_std::fmt::Debug,
	traits::{
		tokens::{fungibles, nonfungibles},
		Currency,
		ExistenceRequirement::{AllowDeath, KeepAlive},
		OneSessionHandler, StorageVersion,ConstU32
	},
	BoundedSlice, BoundedVec ,
	transactional, PalletId,
};
use frame_system::offchain::{
	AppCrypto, CreateSignedTransaction, SendUnsignedTransaction, SignedPayload, Signer,
	SigningTypes,
};
use pallet_octopus_support::{
	log,
	traits::{
		AppchainInterface, ConvertIntoNep171, LposInterface, TokenIdAndAssetIdProvider,
		UpwardMessagesInterface, ValidatorsProvider,
	},
	types::{BurnAssetPayload, LockNftPayload, LockPayload, Nep171TokenMetadata, PayloadType},
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
	traits::{AccountIdConversion, CheckedConversion, IdentifyAccount, StaticLookup},
	RuntimeAppPublic, RuntimeDebug,
};
use sp_std::prelude::*;
pub use weights::WeightInfo;

// Re-export pallet items so that they can be accessed from the crate namespace.
pub use pallet::*;

pub(crate) const LOG_TARGET: &'static str = "runtime::octopus-appchain";

pub(crate) const GIT_VERSION: &str = include_str!(concat!(env!("OUT_DIR"), "/git_version"));

mod mainchain;
pub mod traits_default_impl;
pub mod weights;

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

type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

type AnchorContractLen = ConstU32<100> ;

/// Validator of appchain.
#[derive(Deserialize, Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct Validator<AccountId> {
	/// The validator's id.
	#[serde(deserialize_with = "account_deserialize_from_hex_str")]
	#[serde(bound(deserialize = "AccountId: Decode"))]
	pub validator_id_in_appchain: AccountId,
	/// The total stake of this validator in mainchain's staking system.
	#[serde(deserialize_with = "deserialize_from_str")]
	pub total_stake: u128,
}

#[derive(Deserialize, Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct ValidatorSet<AccountId> {
	/// The anchor era that this set belongs to.
	pub set_id: u32,
	/// Validators in this set.
	#[serde(bound(deserialize = "AccountId: Decode"))]
	pub validators: Vec<Validator<AccountId>>,
}

/// Appchain token burn event.
#[derive(Deserialize, Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct BurnEvent<AccountId> {
	#[serde(default)]
	pub index: u32,
	#[serde(rename = "sender_id_in_near")]
	#[serde(with = "serde_bytes")]
	pub sender_id: Vec<u8>,
	#[serde(rename = "receiver_id_in_appchain")]
	#[serde(deserialize_with = "account_deserialize_from_hex_str")]
	#[serde(bound(deserialize = "AccountId: Decode"))]
	pub receiver: AccountId,
	#[serde(deserialize_with = "deserialize_from_str")]
	pub amount: u128,
}

// /// Appchain token burn event.
// #[derive(Deserialize, Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
// pub struct BurnNftEvent<AccountId> {
// 	#[serde(default)]
// 	index: u32,
// 	#[serde(rename = "sender_id_in_near")]
// 	#[serde(with = "serde_bytes")]
// 	sender_id: Vec<u8>,
// 	#[serde(rename = "receiver_id_in_appchain")]
// 	#[serde(deserialize_with = "account_deserialize_from_hex_str")]
// 	#[serde(bound(deserialize = "AccountId: Decode"))]
// 	receiver: AccountId,
// 	#[serde(rename = "class_id")]
// 	#[serde(deserialize_with = "deserialize_from_str")]
// 	class: u128,
// 	#[serde(rename = "instance_id")]
// 	#[serde(deserialize_with = "deserialize_from_str")]
// 	instance: u128,
// }

/// Token locked event.
#[derive(Deserialize, Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct LockAssetEvent<AccountId> {
	#[serde(default)]
	pub index: u32,
	#[serde(rename = "contract_account")]
	#[serde(with = "serde_bytes")]
	pub token_id: Vec<u8>,
	#[serde(rename = "sender_id_in_near")]
	#[serde(with = "serde_bytes")]
	pub sender_id: Vec<u8>,
	#[serde(rename = "receiver_id_in_appchain")]
	#[serde(deserialize_with = "account_deserialize_from_hex_str")]
	#[serde(bound(deserialize = "AccountId: Decode"))]
	pub receiver: AccountId,
	#[serde(deserialize_with = "deserialize_from_str")]
	pub amount: u128,
}

/// Appchain token lock event.
#[derive(Deserialize, Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct BurnNftEvent<AccountId> {
	#[serde(default)]
	pub index: u32,
	#[serde(rename = "sender_id_in_near")]
	#[serde(with = "serde_bytes")]
	pub sender_id: Vec<u8>,
	#[serde(rename = "receiver_id_in_appchain")]
	#[serde(deserialize_with = "account_deserialize_from_hex_str")]
	#[serde(bound(deserialize = "AccountId: Decode"))]
	pub receiver: AccountId,
	#[serde(rename = "class_id")]
	#[serde(deserialize_with = "deserialize_from_str")]
	pub class: u128,
	#[serde(rename = "token_id")]
	#[serde(deserialize_with = "deserialize_from_str")]
	pub instance: u128,
}

#[derive(Deserialize, Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub enum AppchainNotification<AccountId> {
	#[serde(rename = "NearFungibleTokenLocked")]
	#[serde(bound(deserialize = "AccountId: Decode"))]
	LockAsset(LockAssetEvent<AccountId>),

	#[serde(rename = "WrappedAppchainTokenBurnt")]
	#[serde(bound(deserialize = "AccountId: Decode"))]
	Burn(BurnEvent<AccountId>),

	// #[serde(rename = "WrappedNonFungibleTokenBurnt")]
	#[serde(rename = "WrappedAppchainNFTLocked")]
	#[serde(bound(deserialize = "AccountId: Decode"))]
	BurnNft(BurnNftEvent<AccountId>),
}

#[derive(PartialEq, Encode, Decode, Clone, RuntimeDebug, TypeInfo)]
pub enum NotificationResult {
	Success,
	UnlockFailed,
	AssetMintFailed,
	AssetGetFailed,
	NftUnlockFailed,
}

fn account_deserialize_from_hex_str<'de, S, D>(deserializer: D) -> Result<S, D::Error>
where
	S: Decode,
	D: Deserializer<'de>,
{
	let account_id_str: String = Deserialize::deserialize(deserializer)?;
	let account_id_hex =
		hex::decode(&account_id_str[2..]).map_err(|e| de::Error::custom(e.to_string()))?;
	S::decode(&mut &account_id_hex[..]).map_err(|e| de::Error::custom(e.to_string()))
}

fn deserialize_from_str<'de, S, D>(deserializer: D) -> Result<S, D::Error>
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
	#[serde(bound(deserialize = "AccountId: Decode"))]
	BurnNft(BurnNftEvent<AccountId>),
}

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub enum ObservationType {
	UpdateValidatorSet,
	Burn,
	LockAsset,
	BurnNft,
}

impl<AccountId> Observation<AccountId> {
	fn observation_index(&self) -> u32 {
		match self {
			Observation::UpdateValidatorSet(set) => set.set_id,
			Observation::LockAsset(event) => event.index,
			Observation::Burn(event) => event.index,
			Observation::BurnNft(event) => event.index,
		}
	}
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct ObservationsPayload<Public, BlockNumber, AccountId> {
	pub public: Public,
	pub key_data: Vec<u8>,
	pub block_number: BlockNumber,
	pub observations: Vec<Observation<AccountId>>,
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
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// The overarching dispatch call type.
		type Call: From<Call<Self>>;

		#[pallet::constant]
		type PalletId: Get<PalletId>;

		type Currency: Currency<Self::AccountId>;

		type AssetId: Member
			+ Parameter
			+ AtLeast32BitUnsigned
			+ Codec
			+ Copy
			+ Debug
			+ Default
			+ MaxEncodedLen
			+ MaybeSerializeDeserialize;

		type AssetBalance: Parameter
			+ Member
			+ AtLeast32BitUnsigned
			+ Codec
			+ Default
			+ From<u128>
			+ Into<u128>
			+ Copy
			+ MaybeSerializeDeserialize
			+ MaxEncodedLen
			+ Debug;

		type Assets: fungibles::Mutate<
			<Self as frame_system::Config>::AccountId,
			AssetId = Self::AssetId,
			Balance = Self::AssetBalance,
		>;

		type AssetIdByTokenId: TokenIdAndAssetIdProvider<Self::AssetId>;

		type LposInterface: LposInterface<Self::AccountId>;

		type UpwardMessagesInterface: UpwardMessagesInterface<Self::AccountId>;

		/// Identifier for the class of nft asset.
		type ClassId: Member + Parameter + Default + Copy + HasCompact + From<u128> + Into<u128>;

		/// The type used to identify a unique nft asset within an asset class.
		type InstanceId: Member + Parameter + Default + Copy + HasCompact + From<u128> + Into<u128>;

		type Uniques: nonfungibles::Inspect<Self::AccountId>
			+ nonfungibles::Transfer<Self::AccountId>;

		type Convertor: ConvertIntoNep171<ClassId = Self::ClassId, InstanceId = Self::InstanceId>;

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

		/// A configuration for limit of plannedValidators
		#[pallet::constant]
		type MaxValidators: Get<u32>;

		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	#[pallet::without_storage_info]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::type_value]
	pub(crate) fn DefaultForAnchorContract() -> BoundedVec< u8 , AnchorContractLen > {
		Vec::new().try_into().unwrap()
	}

	#[pallet::storage]
	#[pallet::getter(fn anchor_contract)]
	pub(crate) type AnchorContract<T: Config> =
		StorageValue<_, BoundedVec< u8 , AnchorContractLen >, ValueQuery, DefaultForAnchorContract>;

	/// A map from NEAR token account ID to appchain asset ID.
	#[pallet::storage]
	pub(super) type AssetIdByTokenId<T: Config> =
		StorageMap<_, Twox64Concat, Vec<u8>, T::AssetId, OptionQuery, GetDefault>;

	/// A storage discarded after StorageVersion 2.
	#[pallet::storage]
	pub(crate) type AssetIdByName<T: Config> =
		StorageMap<_, Twox64Concat, Vec<u8>, T::AssetId, ValueQuery>;

	/// Whether the appchain is activated.
	///
	/// Only an active appchain will communicate with the mainchain and pay block rewards.
	#[pallet::storage]
	pub(crate) type IsActivated<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::storage]
	pub(crate) type NextSetId<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	pub(crate) type PlannedValidators<T: Config> =
		StorageValue<_, BoundedVec<(T::AccountId, u128), T::MaxValidators>, ValueQuery>;

	#[pallet::storage]
	pub(crate) type NextNotificationId<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	pub(crate) type Observations<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		ObservationType,
		Twox64Concat,
		u32,
		Vec<Observation<T::AccountId>>,
		ValueQuery,
	>;

	#[pallet::storage]
	pub(crate) type Observing<T: Config> =
		StorageMap<_, Twox64Concat, Observation<T::AccountId>, Vec<T::AccountId>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn octopus_pallet_id)]
	pub(crate) type OctopusPalletId<T: Config> = StorageValue<_, Option<T::AccountId>, ValueQuery>;

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
		pub premined_amount: u128,
		pub asset_id_by_token_id: Vec<(String, T::AssetId)>,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				anchor_contract: String::new(),
				validators: Vec::new(),
				premined_amount: 0,
				asset_id_by_token_id: Vec::new(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			let bounded_anchor_contract = BoundedSlice::<u8, AnchorContractLen>::try_from( self.anchor_contract.as_bytes())
				.expect("Exceed the limit of max validators.");
			<AnchorContract<T>>::put(bounded_anchor_contract);

			<NextSetId<T>>::put(1); // set 0 is already in the genesis
			let bounded_validators = BoundedSlice::<(T::AccountId, u128), T::MaxValidators>::try_from( self.validators.as_slice())
				.expect("Exceed the limit of max validators.");
			<PlannedValidators<T>>::put(bounded_validators);
			let account_id = <Pallet<T>>::account_id();
			let min = T::Currency::minimum_balance();
			let amount =
				self.premined_amount.checked_into().ok_or(Error::<T>::AmountOverflow).unwrap();
			if amount >= min {
				T::Currency::make_free_balance_be(&account_id, amount);
			}
			<OctopusPalletId<T>>::put(Some(account_id));
			for (token_id, asset_id) in self.asset_id_by_token_id.iter() {
				<AssetIdByTokenId<T>>::insert(token_id.as_bytes(), asset_id);
			}
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new set of validators is waiting to be changed.
		NewPlannedValidators {
			set_id: u32,
			validators: Vec<(T::AccountId, u128)>,
		},
		/// An `amount` of native token has been locked in the appchain to indicate that
		/// it will be cross-chain transferred to the mainchain.
		Locked {
			sender: T::AccountId,
			receiver: Vec<u8>,
			amount: BalanceOf<T>,
			sequence: u64,
		},
		/// An `amount` was unlocked to `receiver` from `sender`.
		Unlocked {
			sender: Vec<u8>,
			receiver: T::AccountId,
			amount: BalanceOf<T>,
			sequence: u32,
		},
		/// An `amount` unlock to `receiver` from `sender` failed.
		UnlockFailed {
			sender: Vec<u8>,
			receiver: T::AccountId,
			amount: BalanceOf<T>,
			sequence: u32,
		},

		AssetMinted {
			asset_id: T::AssetId,
			sender: Vec<u8>,
			receiver: T::AccountId,
			amount: T::AssetBalance,
			sequence: Option<u32>,
		},
		AssetBurned {
			asset_id: T::AssetId,
			sender: T::AccountId,
			receiver: Vec<u8>,
			amount: T::AssetBalance,
			sequence: u64,
		},
		AssetMintFailed {
			asset_id: T::AssetId,
			sender: Vec<u8>,
			receiver: T::AccountId,
			amount: T::AssetBalance,
			sequence: Option<u32>,
		},
		AssetIdGetFailed {
			token_id: Vec<u8>,
			sender: Vec<u8>,
			receiver: T::AccountId,
			amount: T::AssetBalance,
			sequence: u32,
		},

		TransferredFromPallet {
			receiver: T::AccountId,
			amount: BalanceOf<T>,
		},

		NftLocked {
			sender: T::AccountId,
			receiver: Vec<u8>,
			class: T::ClassId,
			instance: T::InstanceId,
			sequence: u64,
		},

		NftUnlocked {
			sender: Vec<u8>,
			receiver: T::AccountId,
			class: T::ClassId,
			instance: T::InstanceId,
			sequence: u32,
		},

		NftUnlockFailed {
			sender: Vec<u8>,
			receiver: T::AccountId,
			class: T::ClassId,
			instance: T::InstanceId,
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
		/// Amount overflow.
		AmountOverflow,
		/// Next notification Id overflow.
		NextNotificationIdOverflow,
		/// Token Id not exist.
		NoTokenId,
		/// Asset Id not exist.
		NoAssetId,
		/// Invalid active total stake.
		InvalidActiveTotalStake,
		/// Appchain is not activated.
		NotActivated,
		/// ReceiverId is not a valid utf8 string.
		InvalidReceiverId,
		/// Token is not a valid utf8 string.
		InvalidTokenId,
		/// Next set Id overflow.
		NextSetIdOverflow,
		/// Observations exceeded limit.
		ObservationsExceededLimit,
		/// Token Id in use.
		TokenIdInUse,
		/// Asset Id in use.
		AssetIdInUse,
		/// Token Id Not Exist.
		TokenIdNotExist,
		/// Not implement nep171 convertor.
		ConvertorNotImplement,
		/// Exceed the limit of BoundedVec
		BoundedVecExeceededLimit ,
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
			let anchor_contract = Self::anchor_contract().to_vec();
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

		fn on_runtime_upgrade() -> Weight {
			let current = Pallet::<T>::current_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			log!(
				info,
				"Running migration in octopus appchain pallet with current storage version {:?} / onchain {:?}",
				current,
				onchain
			);

			if current == 2 && onchain == 1 {
				let translated = Self::migration_to_v2(1u64);
				current.put::<Pallet<T>>();
				T::DbWeight::get().reads_writes(translated, translated)
			} else if current == 2 && onchain == 0 {
				Self::migration_to_v1();
				let translated = Self::migration_to_v2(1u64);
				current.put::<Pallet<T>>();
				T::DbWeight::get().reads_writes(translated + 1, translated + 1)
			} else {
				log!(
					info,
					"Migration being executed on the wrong storage version, expected V0 or V1"
				);
				T::DbWeight::get().reads(1)
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
		#[pallet::weight(<T as Config>::WeightInfo::submit_observations(payload.observations.len() as u32))]
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

		#[pallet::weight(<T as Config>::WeightInfo::force_set_is_activated())]
		pub fn force_set_is_activated(origin: OriginFor<T>, is_activated: bool) -> DispatchResult {
			ensure_root(origin)?;
			<IsActivated<T>>::put(is_activated);
			Ok(())
		}

		#[pallet::weight(<T as Config>::WeightInfo::force_set_next_set_id())]
		pub fn force_set_next_set_id(origin: OriginFor<T>, next_set_id: u32) -> DispatchResult {
			ensure_root(origin)?;
			<NextSetId<T>>::put(next_set_id);
			log!(info, "️️️force set next_set_id, next_set_id is : {:?} ", NextSetId::<T>::get());
			Ok(())
		}

		// Force set planned validators with sudo permissions.
		#[pallet::weight(<T as Config>::WeightInfo::force_set_planned_validators(validators.len() as u32))]
		pub fn force_set_planned_validators(
			origin: OriginFor<T>,
			validators: Vec<(T::AccountId, u128)>,
		) -> DispatchResult {
			ensure_root(origin)?;
			let bounded_validators = match BoundedSlice::<(T::AccountId, u128), T::MaxValidators>::try_from( validators.as_slice()) {
				Ok(v) => v ,
				Err(_) => return Err(Error::<T>::BoundedVecExeceededLimit.into())  
			} ;
			<PlannedValidators<T>>::put(bounded_validators) ;
			
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

		/// Emits `Locked` event when successful.
		#[pallet::weight(<T as Config>::WeightInfo::lock())]
		#[transactional]
		pub fn lock(
			origin: OriginFor<T>,
			receiver_id: Vec<u8>,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(IsActivated::<T>::get(), Error::<T>::NotActivated);

			let receiver_id =
				String::from_utf8(receiver_id).map_err(|_| Error::<T>::InvalidReceiverId)?;

			let amount_wrapped: u128 = amount.checked_into().ok_or(Error::<T>::AmountOverflow)?;

			T::Currency::transfer(&who, &Self::account_id(), amount, AllowDeath)?;

			let prefix = String::from("0x");
			let hex_sender = prefix + &hex::encode(who.encode());
			let message = LockPayload {
				sender: hex_sender.clone(),
				receiver_id: receiver_id.clone(),
				amount: amount_wrapped,
			};

			let sequence = T::UpwardMessagesInterface::submit(
				Some(who.clone()),
				PayloadType::Lock,
				&message.try_to_vec().unwrap(),
			)?;
			Self::deposit_event(Event::Locked {
				sender: who,
				receiver: receiver_id.as_bytes().to_vec(),
				amount,
				sequence,
			});

			Ok(())
		}

		#[pallet::weight(<T as Config>::WeightInfo::mint_asset())]
		#[transactional]
		pub fn mint_asset(
			origin: OriginFor<T>,
			asset_id: T::AssetId,
			sender_id: Vec<u8>,
			receiver: <T::Lookup as StaticLookup>::Source,
			amount: T::AssetBalance,
		) -> DispatchResult {
			ensure_root(origin)?;

			let receiver = T::Lookup::lookup(receiver)?;
			Self::mint_asset_inner(asset_id, sender_id, receiver, amount, None)
		}

		#[pallet::weight(<T as Config>::WeightInfo::burn_asset())]
		#[transactional]
		pub fn burn_asset(
			origin: OriginFor<T>,
			asset_id: T::AssetId,
			receiver_id: Vec<u8>,
			amount: T::AssetBalance,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			ensure!(IsActivated::<T>::get(), Error::<T>::NotActivated);

			let receiver_id =
				String::from_utf8(receiver_id).map_err(|_| Error::<T>::InvalidReceiverId)?;

			let token_id = T::AssetIdByTokenId::try_get_token_id(asset_id)
				.map_err(|_| Error::<T>::NoAssetId)?;

			let token_id = String::from_utf8(token_id).map_err(|_| Error::<T>::InvalidTokenId)?;

			<T::Assets as fungibles::Mutate<T::AccountId>>::burn_from(asset_id, &sender, amount)?;

			let prefix = String::from("0x");
			let hex_sender = prefix + &hex::encode(sender.encode());
			let message = BurnAssetPayload {
				token_id,
				sender: hex_sender,
				receiver_id: receiver_id.clone(),
				amount: amount.into(),
			};

			let sequence = T::UpwardMessagesInterface::submit(
				Some(sender.clone()),
				PayloadType::BurnAsset,
				&message.try_to_vec().unwrap(),
			)?;
			Self::deposit_event(Event::AssetBurned {
				asset_id,
				sender,
				receiver: receiver_id.as_bytes().to_vec(),
				amount,
				sequence,
			});

			Ok(())
		}

		#[pallet::weight(<T as Config>::WeightInfo::set_asset_name())]
		pub fn set_token_id(
			origin: OriginFor<T>,
			token_id: Vec<u8>,
			asset_id: T::AssetId,
		) -> DispatchResult {
			ensure_root(origin)?;
			ensure!(IsActivated::<T>::get(), Error::<T>::NotActivated);

			ensure!(
				!AssetIdByTokenId::<T>::contains_key(token_id.clone()),
				Error::<T>::TokenIdInUse
			);
			let asset = <AssetIdByTokenId<T>>::iter().find(|p| p.1 == asset_id);
			if asset.is_some() {
				log!(debug, "asset_id: {:?} exists in {:?}", asset_id, asset.unwrap(),);
				return Err(Error::<T>::AssetIdInUse.into())
			}

			<AssetIdByTokenId<T>>::insert(token_id, asset_id);

			Ok(())
		}

		#[pallet::weight(<T as Config>::WeightInfo::tranfer_from_pallet_account())]
		pub fn tranfer_from_pallet_account(
			origin: OriginFor<T>,
			receiver: <T::Lookup as StaticLookup>::Source,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let receiver = T::Lookup::lookup(receiver)?;

			T::Currency::transfer(&Self::account_id(), &receiver, amount, KeepAlive)?;
			log!(debug, "Transfer from pallet, receiver: {:?}, amount: {:?} ", receiver, amount);

			Self::deposit_event(Event::TransferredFromPallet { receiver, amount });

			Ok(())
		}

		// nft cross chain transfer:
		// mainchain:mint() <- appchain:lock_nft()
		// mainchain:burn() -> appchain:unlock_nft()
		#[pallet::weight(<T as Config>::WeightInfo::lock_nft())]
		#[transactional]
		pub fn lock_nft(
			origin: OriginFor<T>,
			class: T::ClassId,
			instance: T::InstanceId,
			receiver_id: Vec<u8>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(IsActivated::<T>::get(), Error::<T>::NotActivated);

			let receiver_id =
				String::from_utf8(receiver_id).map_err(|_| Error::<T>::InvalidReceiverId)?;

			let metadata = match T::Convertor::convert_into_nep171_metadata(class, instance) {
				Some(data) => data,
				None => return Err(Error::<T>::ConvertorNotImplement.into()),
			};

			// <T::Uniques as nonfungibles::Transfer<T::AccountId>>::transfer(
			// 	&class,
			// 	&instance,
			// 	&Self::account_id(),
			// )?;

			let prefix = String::from("0x");
			let hex_sender = prefix + &hex::encode(who.encode());

			let message = LockNftPayload {
				sender: hex_sender.clone(),
				receiver_id: receiver_id.clone(),
				class: class.into(),
				instance: instance.into(),
				metadata,
			};

			let sequence = T::UpwardMessagesInterface::submit(
				Some(who.clone()),
				PayloadType::LockNft,
				&message.try_to_vec().unwrap(),
			)?;
			Self::deposit_event(Event::NftLocked {
				sender: who,
				receiver: receiver_id.as_bytes().to_vec(),
				class,
				instance,
				sequence,
			});

			Ok(())
		}

		#[pallet::weight(<T as Config>::WeightInfo::delete_token_id())]
		pub fn delete_token_id(origin: OriginFor<T>, token_id: Vec<u8>) -> DispatchResult {
			ensure_root(origin)?;

			ensure!(
				AssetIdByTokenId::<T>::contains_key(token_id.clone()),
				Error::<T>::TokenIdNotExist
			);

			AssetIdByTokenId::<T>::remove(token_id);

			Ok(())
		}

		#[pallet::weight(<T as Config>::WeightInfo::force_set_next_notification_id())]
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

	impl<T: Config> TokenIdAndAssetIdProvider<T::AssetId> for Pallet<T> {
		type Err = Error<T>;

		fn try_get_asset_id(
			token_id: impl AsRef<[u8]>,
		) -> Result<<T as Config>::AssetId, Self::Err> {
			<AssetIdByTokenId<T>>::get(token_id.as_ref().to_vec()).ok_or(Error::<T>::NoTokenId)
		}

		fn try_get_token_id(asset_id: T::AssetId) -> Result<Vec<u8>, Self::Err> {
			let token_id = <AssetIdByTokenId<T>>::iter().find(|p| p.1 == asset_id).map(|p| p.0);
			match token_id {
				Some(id) => Ok(id),
				_ => Err(Error::<T>::NoAssetId),
			}
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn account_id() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}

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

		/// Emits `Unlocked` event when successful.
		fn unlock_inner(
			sender_id: Vec<u8>,
			receiver: T::AccountId,
			amount: u128,
			sequence: u32,
		) -> DispatchResultWithPostInfo {
			let amount_unwrapped = amount.checked_into().ok_or(Error::<T>::AmountOverflow)?;
			// unlock native token
			T::Currency::transfer(&Self::account_id(), &receiver, amount_unwrapped, KeepAlive)?;
			Self::deposit_event(Event::Unlocked {
				sender: sender_id,
				receiver,
				amount: amount_unwrapped,
				sequence,
			});

			Ok(().into())
		}

		fn mint_asset_inner(
			asset_id: T::AssetId,
			sender_id: Vec<u8>,
			receiver: T::AccountId,
			amount: T::AssetBalance,
			sequence: Option<u32>,
		) -> DispatchResult {
			<T::Assets as fungibles::Mutate<T::AccountId>>::mint_into(asset_id, &receiver, amount)?;
			Self::deposit_event(Event::AssetMinted {
				asset_id,
				sender: sender_id,
				receiver,
				amount,
				sequence,
			});

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

			// The maximum number of observation for the same obs_id is the number of validators
			// (100), that is, each validator submits a observation.
			let obs = <Observations<T>>::try_get(observation_type, obs_id);
			if let Ok(obs) = obs {
				if obs.len() > 100 {
					log!(
						warn,
						"the number of observations with ({:?}, {:?}) exceeded the upper limit",
						obs_id,
						observation_type
					);
					return Err(Error::<T>::ObservationsExceededLimit.into())
				}
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
						log!(debug, "new PlannedValidators: {:?}", validators.clone());
						let bounded_validators = match BoundedSlice::<(T::AccountId, u128), T::MaxValidators>::try_from( validators.as_slice()) {
							Ok(v) => v ,
							Err(_) => return Err(Error::<T>::BoundedVecExeceededLimit.into())  
						} ;
						<PlannedValidators<T>>::put( bounded_validators );
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
						if let Err(error) = Self::unlock_inner(
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
							let min = T::Currency::minimum_balance();
							let amount_unwrapped = event.amount.checked_into().unwrap_or(min); //Check: should not return error.
							Self::deposit_event(Event::UnlockFailed {
								sender: event.sender_id,
								receiver: event.receiver,
								amount: amount_unwrapped,
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
						if let Ok(asset_id) = T::AssetIdByTokenId::try_get_asset_id(&event.token_id)
						{
							log!(
								info,
								"️️️sequence: {:?}, mint asset:{:?}, sender_id:{:?}, receiver:{:?}, amount:{:?}",
								event.index,
								asset_id,
								event.sender_id,
								event.receiver,
								event.amount,
							);
							let sequence = event.index;
							if let Err(error) = Self::mint_asset_inner(
								asset_id,
								event.sender_id.clone(),
								event.receiver.clone(),
								event.amount.into(),
								Some(sequence),
							) {
								log!(warn, "️️️failed to mint asset, sequence: {:?}, asset: {:?}, sender_id: {:?}, receiver: {:?}, amount: {:?}, error: {:?}",
								event.index,
								asset_id,
								event.sender_id.clone(),
								event.receiver.clone(),
								event.amount,
								error);
								Self::deposit_event(Event::AssetMintFailed {
									asset_id,
									sender: event.sender_id,
									receiver: event.receiver,
									amount: event.amount.into(),
									sequence: Some(sequence),
								});
								result = NotificationResult::AssetMintFailed;
							}
						} else {
							log!(
								warn,
								"️️️failed to mint asset, sequence: {:?}, token_id: {:?}, error: AssetIdGetFailed",
								event.index,
								event.token_id
							);
							Self::deposit_event(Event::AssetIdGetFailed {
								token_id: event.token_id,
								sender: event.sender_id,
								receiver: event.receiver,
								amount: event.amount.into(),
								sequence: event.index,
							});
							result = NotificationResult::AssetGetFailed;
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

						if let Err(error) = Self::unlock_nft_inner(
							event.sender_id.clone(),
							event.receiver.clone(),
							event.class.into(),
							event.instance.into(),
							event.index,
						) {
							log!(warn, "️️️failed to unlock nft, sequence: {:?}, send_id: {:?}, receiver: {:?}, class: {:?}, instance: {:?}, error: {:?}",
							event.index,
							event.sender_id.clone(),
							event.receiver.clone(),
							event.class,
							event.instance,
							error);

							Self::deposit_event(Event::NftUnlockFailed {
								sender: event.sender_id,
								receiver: event.receiver,
								class: event.class.into(),
								instance: event.instance.into(),
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

		fn unlock_nft_inner(
			sender_id: Vec<u8>,
			receiver: T::AccountId,
			class: T::ClassId,
			instance: T::InstanceId,
			sequence: u32,
		) -> DispatchResultWithPostInfo {
			// <T::Uniques as nonfungibles::Transfer<T::AccountId>>::transfer(
			// 	&class, &instance, &receiver,
			// )?;

			Self::deposit_event(Event::NftUnlocked {
				sender: sender_id,
				receiver,
				class,
				instance,
				sequence,
			});

			Ok(().into())
		}

		fn migration_to_v1() {
			let account = <Pallet<T>>::account_id();
			OctopusPalletId::<T>::put(Some(account));
			log!(info, "updating to version 1 ");
		}

		fn migration_to_v2(translated: u64) -> u64 {
			let mut translated = translated;
			let _ = AssetIdByName::<T>::iter()
				.map(|(token_id, asset_id)| {
					AssetIdByTokenId::<T>::insert(token_id, asset_id);
					translated += 1u64;
				})
				.collect::<Vec<_>>();

			log!(info, "updating to version 2 ",);
			translated
		}
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

	impl<T: Config> ValidatorsProvider<T::AccountId> for Pallet<T> {
		fn validators() -> Vec<(T::AccountId, u128)> {
			<PlannedValidators<T>>::get().to_vec()
		}
	}
}
