#![cfg_attr(not(feature = "std"), no_std)]

// cross chain transfer

// There are 2 kinds of assets:
// 1. native token on appchain
// mainchain:mint() <- appchain:lock()
// mainchain:burn() -> appchain:unlock()
//
// 2. NEP141 asset on mainchain
// mainchain:lock_asset()   -> appchain:mint_asset()
// mainchain:unlock_asset() <- appchain:burn_asset()
use borsh::BorshSerialize;
use codec::Encode;
use frame_support::{
	dispatch::{DispatchError, DispatchResult},
	traits::{
		tokens::{
			fungibles::{self, Mutate},
			nonfungibles, AssetId, Balance as AssetBalance, DepositConsequence,
			WithdrawConsequence,
		},
		Currency,
		ExistenceRequirement::{AllowDeath, KeepAlive},
		Get, StorageVersion,
	},
	transactional, PalletId,
};
use frame_system::ensure_signed;
use pallet_octopus_support::{
	log,
	traits::{
		AppchainInterface, BridgeInterface, ConvertIntoNep171, TokenIdAndAssetIdProvider,
		UpwardMessagesInterface,
	},
	types::{BurnAssetPayload, LockNftPayload, LockPayload, Nep171TokenMetadata, PayloadType},
};
use scale_info::prelude::string::{String, ToString};
use serde::Deserialize;
use serde_json::json;
use sp_runtime::{
	traits::{AccountIdConversion, CheckedConversion, StaticLookup},
	RuntimeDebug,
};
use sp_std::prelude::*;
use weights::WeightInfo;

pub use pallet::*;

mod fungible;
pub mod impls;
mod migration;
mod near;
mod nep141;
mod nep171;
mod nonfungible;
mod token;
mod weights;

type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

pub(crate) const LOG_TARGET: &'static str = "runtime::octopus-bridge";

/// The current storage version.
const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		#[pallet::constant]
		type PalletId: Get<PalletId>;

		type Currency: Currency<Self::AccountId>;

		type AppchainInterface: AppchainInterface<Self::AccountId>;

		type UpwardMessagesInterface: UpwardMessagesInterface<Self::AccountId>;

		type AssetIdByTokenId: TokenIdAndAssetIdProvider<Self::AssetId>;

		type AssetId: AssetId + MaybeSerializeDeserialize;

		type AssetBalance: AssetBalance + From<u128> + Into<u128>;

		type Fungibles: fungibles::Mutate<
			<Self as frame_system::Config>::AccountId,
			AssetId = Self::AssetId,
			Balance = Self::AssetBalance,
		>;

		/// Identifier for the collection of nft item.
		type CollectionId: Parameter + Copy + From<u128> + Into<u128>;

		/// The type used to identify a unique item within a collection.
		type ItemId: Parameter + Copy + From<u128> + Into<u128>;

		type Nonfungibles: nonfungibles::Inspect<
				Self::AccountId,
				ItemId = Self::ItemId,
				CollectionId = Self::CollectionId,
			> + nonfungibles::Transfer<
				Self::AccountId,
				ItemId = Self::ItemId,
				CollectionId = Self::CollectionId,
			>;

		type Convertor: ConvertIntoNep171<CollectionId = Self::CollectionId, ItemId = Self::ItemId>;

		/// Type representing the weight of this pallet
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub(super) trait Store)]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_runtime_upgrade() -> Weight {
			let current = Pallet::<T>::current_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			log!(
				info,
				"Running migration in octopus bridge pallet with current storage version {:?} / onchain {:?}",
				current,
				onchain
			);

			if current == 1 && onchain == 0 {
				let translated = Self::migration_to_v1();
				current.put::<Pallet<T>>();
				T::DbWeight::get().reads_writes(translated + 1, translated + 1)
			} else {
				log!(
					info,
					"The storageVersion is already the matching version, and the migration is not repeated."
				);
				T::DbWeight::get().reads(1)
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(0)]
		#[transactional]
		pub fn lock(
			origin: OriginFor<T>,
			receiver_id: Vec<u8>,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			ensure!(T::AppchainInterface::is_activated(), Error::<T>::NotActivated);

			Self::do_lock(sender, receiver_id, amount)?;

			Ok(())
		}

		#[pallet::weight(0)]
		#[transactional]
		pub fn burn_nep141(
			origin: OriginFor<T>,
			asset_id: T::AssetId,
			receiver_id: Vec<u8>,
			amount: T::AssetBalance,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			ensure!(T::AppchainInterface::is_activated(), Error::<T>::NotActivated);

			Self::do_burn_nep141(asset_id, sender, receiver_id, amount)?;

			Ok(())
		}

		#[pallet::weight(0)]
		#[transactional]
		pub fn lock_nonfungible(
			origin: OriginFor<T>,
			collection_id: T::CollectionId,
			item_id: T::ItemId,
			receiver_id: Vec<u8>,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			ensure!(T::AppchainInterface::is_activated(), Error::<T>::NotActivated);

			Self::do_lock_nonfungible(collection_id, item_id, sender, receiver_id)?;

			Ok(())
		}

		#[pallet::weight(0)]
		pub fn set_token_id(
			origin: OriginFor<T>,
			token_id: Vec<u8>,
			asset_id: T::AssetId,
		) -> DispatchResult {
			ensure_root(origin)?;
			ensure!(T::AppchainInterface::is_activated(), Error::<T>::NotActivated);

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

		#[pallet::weight(<T as Config>::WeightInfo::force_unlock())]
		pub fn force_unlock(
			origin: OriginFor<T>,
			who: <T::Lookup as StaticLookup>::Source,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let who = T::Lookup::lookup(who)?;
			T::Currency::transfer(&Self::account_id(), &who, amount, KeepAlive)?;

			Self::deposit_event(Event::ForceUnlocked { who, amount });

			Ok(())
		}

		#[pallet::weight(<T as Config>::WeightInfo::force_mint_asset())]
		pub fn force_mint_nep141(
			origin: OriginFor<T>,
			asset_id: T::AssetId,
			who: <T::Lookup as StaticLookup>::Source,
			amount: T::AssetBalance,
		) -> DispatchResult {
			ensure_root(origin)?;
			let who = T::Lookup::lookup(who)?;
			T::Fungibles::mint_into(asset_id, &who, amount)?;

			Self::deposit_event(Event::ForceNep141Minted { asset_id, who, amount });

			Ok(())
		}

		#[pallet::weight(<T as Config>::WeightInfo::force_unlock_nft())]
		pub fn force_unlock_nonfungible(
			origin: OriginFor<T>,
			who: <T::Lookup as StaticLookup>::Source,
			collection: T::CollectionId,
			item: T::ItemId,
		) -> DispatchResult {
			ensure_root(origin)?;

			let who = T::Lookup::lookup(who)?;
			<T::Nonfungibles as nonfungibles::Transfer<T::AccountId>>::transfer(
				&collection,
				&item,
				&who,
			)?;

			Self::deposit_event(Event::ForceNonfungibleUnlocked { collection, item, who });

			Ok(())
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
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
		Nep141Minted {
			asset_id: T::AssetId,
			sender: Vec<u8>,
			receiver: T::AccountId,
			amount: T::AssetBalance,
			sequence: u32,
		},
		Nep141Burned {
			asset_id: T::AssetId,
			sender: T::AccountId,
			receiver: Vec<u8>,
			amount: T::AssetBalance,
			sequence: u64,
		},
		NonfungibleLocked {
			collection: T::CollectionId,
			item: T::ItemId,
			sender: T::AccountId,
			receiver: Vec<u8>,
			sequence: u64,
		},
		NonfungibleUnlocked {
			collection: T::CollectionId,
			item: T::ItemId,
			sender: Vec<u8>,
			receiver: T::AccountId,
			sequence: u32,
		},
		ForceUnlocked {
			who: T::AccountId,
			amount: BalanceOf<T>,
		},
		/// Some asset was force-minted.
		ForceNep141Minted {
			asset_id: T::AssetId,
			who: T::AccountId,
			amount: T::AssetBalance,
		},
		ForceNonfungibleUnlocked {
			collection: T::CollectionId,
			item: T::ItemId,
			who: T::AccountId,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Amount overflow.
		AmountOverflow,
		/// Collection overflow.
		CollectionOverflow,
		/// Item overflow.
		ItemOverflow,
		/// Token Id not exist.
		NoTokenId,
		/// Asset Id not exist.
		NoAssetId,
		/// Appchain is not activated.
		NotActivated,
		/// ReceiverId is not a valid utf8 string.
		InvalidReceiverId,
		/// Token is not a valid utf8 string.
		InvalidTokenId,
		/// Token Id in use.
		TokenIdInUse,
		/// Asset Id in use.
		AssetIdInUse,
		/// Token Id Not Exist.
		TokenIdNotExist,
		/// Not implement nep171 convertor.
		ConvertorNotImplement,
	}

	/// A map from NEAR token account ID to appchain asset ID.
	#[pallet::storage]
	pub(super) type AssetIdByTokenId<T: Config> =
		StorageMap<_, Twox64Concat, Vec<u8>, T::AssetId, OptionQuery, GetDefault>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub premined_amount: u128,
		pub asset_id_by_token_id: Vec<(String, T::AssetId)>,
	}

	// The default value for the genesis config type.
	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { premined_amount: Default::default(), asset_id_by_token_id: Default::default() }
		}
	}

	// The build of genesis for the pallet.
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			let account_id = <Pallet<T>>::account_id();

			let min = T::Currency::minimum_balance();
			let amount =
				self.premined_amount.checked_into().ok_or(Error::<T>::AmountOverflow).unwrap();
			if amount >= min {
				T::Currency::make_free_balance_be(&account_id, amount);
			}
			for (token_id, asset_id) in self.asset_id_by_token_id.iter() {
				<AssetIdByTokenId<T>>::insert(token_id.as_bytes(), asset_id);
			}
		}
	}
}

impl<T: Config> TokenIdAndAssetIdProvider<T::AssetId> for Pallet<T> {
	type Err = Error<T>;

	fn try_get_asset_id(token_id: impl AsRef<[u8]>) -> Result<<T as Config>::AssetId, Self::Err> {
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
	fn account_id() -> T::AccountId {
		T::PalletId::get().into_account_truncating()
	}
}
