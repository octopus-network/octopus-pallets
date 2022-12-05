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
use codec::{Decode, Encode};
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
use scale_info::{
	prelude::string::{String, ToString},
	TypeInfo,
};
use serde::Deserialize;
use serde_json::json;
use sp_runtime::{
	traits::{AccountIdConversion, CheckedConversion, StaticLookup},
	Perbill, RuntimeDebug,
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
pub mod weights;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

pub(crate) const LOG_TARGET: &'static str = "runtime::octopus-bridge";

/// The current storage version.
const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub enum CrossChainTransferType {
	Fungible,
	Nonfungible,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

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

		#[pallet::constant]
		type NativeTokenDecimals: Get<u128>;

		#[pallet::constant]
		type Threshold: Get<u64>;

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
		#[pallet::weight(<T as Config>::WeightInfo::lock())]
		#[transactional]
		pub fn lock(
			origin: OriginFor<T>,
			receiver_id: Vec<u8>,
			amount: BalanceOf<T>,
			fee: BalanceOf<T>,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			ensure!(T::AppchainInterface::is_activated(), Error::<T>::NotActivated);

			Self::do_lock_fungible_transfer_fee(sender.clone(), fee)?;
			Self::do_lock(sender, receiver_id, amount, fee)?;

			Ok(())
		}

		#[pallet::weight(<T as Config>::WeightInfo::burn_nep141())]
		#[transactional]
		pub fn burn_nep141(
			origin: OriginFor<T>,
			asset_id: T::AssetId,
			receiver_id: Vec<u8>,
			amount: T::AssetBalance,
			fee: BalanceOf<T>,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			ensure!(T::AppchainInterface::is_activated(), Error::<T>::NotActivated);

			Self::do_lock_fungible_transfer_fee(sender.clone(), fee)?;
			Self::do_burn_nep141(asset_id, sender, receiver_id, amount, fee)?;

			Ok(())
		}

		#[pallet::weight(<T as Config>::WeightInfo::lock_nonfungible())]
		#[transactional]
		pub fn lock_nonfungible(
			origin: OriginFor<T>,
			collection_id: T::CollectionId,
			item_id: T::ItemId,
			receiver_id: Vec<u8>,
			fee: BalanceOf<T>,
			metadata_length: u32,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			ensure!(T::AppchainInterface::is_activated(), Error::<T>::NotActivated);

			Self::do_lock_nonfungible_transfer_fee(sender.clone(), fee, metadata_length)?;
			Self::do_lock_nonfungible(collection_id, item_id, sender, receiver_id, fee)?;

			Ok(())
		}

		#[pallet::weight(<T as Config>::WeightInfo::set_token_id())]
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
				log!(debug, "asset_id: {:?} exists in {:?}", asset_id, asset);
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

		#[pallet::weight(<T as Config>::WeightInfo::force_mint_nep141())]
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

		#[pallet::weight(<T as Config>::WeightInfo::force_unlock_nonfungible())]
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

		#[pallet::weight(<T as Config>::WeightInfo::set_oracle_account())]
		pub fn set_oracle_account(
			origin: OriginFor<T>,
			who: <T::Lookup as StaticLookup>::Source,
		) -> DispatchResult {
			ensure_root(origin)?;
			let who = T::Lookup::lookup(who)?;

			<OracleAccount<T>>::put(who.clone());
			Self::deposit_event(Event::OracleAccountHasBeenSet { who });

			Ok(())
		}

		#[pallet::weight(<T as Config>::WeightInfo::set_token_price())]
		pub fn set_token_price(
			origin: OriginFor<T>,
			near_price: u64,
			native_token_price: u64,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			ensure!(near_price != 0, Error::<T>::NearPriceSetedIsZero);
			ensure!(native_token_price != 0, Error::<T>::NativeTokenPriceSetedIsZero);

			let oracle_account = match <OracleAccount<T>>::try_get() {
				Ok(account) => account,
				Err(_) => return Err(Error::<T>::NoOracleAccount.into()),
			};

			ensure!(who == oracle_account, Error::<T>::UpdatePriceWithNoPermissionAccount);

			<TokenPrice<T>>::put(Some(native_token_price));
			<NearPrice<T>>::put(Some(near_price));

			Self::calculate_and_update_fee(near_price, native_token_price);

			Self::deposit_event(Event::PriceUpdated { who, near_price, native_token_price });

			Ok(())
		}

		#[pallet::weight(0)]
		pub fn set_coef_for_calculate_fee(origin: OriginFor<T>, coef: u32) -> DispatchResult {
			ensure_root(origin)?;
			ensure!(coef > 0 && coef <= 100, Error::<T>::InvalidCoef);

			<Coef<T>>::put(coef);
			Self::deposit_event(Event::CoefUpdated { coef });
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
			fee: BalanceOf<T>,
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
			fee: BalanceOf<T>,
			sequence: u64,
		},
		NonfungibleLocked {
			collection: T::CollectionId,
			item: T::ItemId,
			sender: T::AccountId,
			receiver: Vec<u8>,
			fee: BalanceOf<T>,
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
		OracleAccountHasBeenSet {
			who: T::AccountId,
		},
		PriceUpdated {
			who: T::AccountId,
			near_price: u64,
			native_token_price: u64,
		},
		CoefUpdated {
			coef: u32,
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
		/// Not set oracle account.
		NoOracleAccount,
		/// Near price setted by oracle is zero.
		NearPriceSetedIsZero,
		/// Native token price setted by oracle is zero.
		NativeTokenPriceSetedIsZero,
		/// Update token price must use oracle account.
		UpdatePriceWithNoPermissionAccount,
		/// Invalid fee.
		InvalidFee,
		/// Invalid coef.
		InvalidCoef,
	}

	/// A map from NEAR token account ID to appchain asset ID.
	#[pallet::storage]
	pub(crate) type AssetIdByTokenId<T: Config> =
		StorageMap<_, Twox64Concat, Vec<u8>, T::AssetId, OptionQuery, GetDefault>;

	/// The oracle account.
	#[pallet::storage]
	pub(crate) type OracleAccount<T: Config> = StorageValue<_, T::AccountId>;

	/// A map store the transfer fee for different cross chain transactions.
	#[pallet::storage]
	pub(crate) type CrosschainTransferFee<T: Config> =
		StorageMap<_, Twox64Concat, CrossChainTransferType, Option<BalanceOf<T>>, OptionQuery>;

	/// Token price setted by oracle account.
	#[pallet::storage]
	pub(crate) type TokenPrice<T: Config> = StorageValue<_, Option<u64>, ValueQuery>;

	/// Near price setted by oracle account.
	#[pallet::storage]
	pub(crate) type NearPrice<T: Config> = StorageValue<_, Option<u64>, ValueQuery>;

	#[pallet::type_value]
	pub(crate) fn DefaultForCoef() -> u32 {
		3u32
	}

	/// Coef used to calculate fee for cross chain transactions.
	#[pallet::storage]
	pub(crate) type Coef<T: Config> = StorageValue<_, u32, ValueQuery, DefaultForCoef>;

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

	// Calcute:
	// 		coef = <Coef<T>>::get() / 100,
	// 		quantity = coef * near_native_token_price / wrapped_appchain_token_price,
	// If quantity > threshold, then quantity = threshold.
	fn calculate_quantity_of_native_tokens(
		near_price: u64,
		native_token_price: u64,
	) -> (u64, Perbill) {
		// (integer, fraction) = near_native_token_price / wrapped_appchain_token_price
		let integer =
			if near_price >= native_token_price { near_price / native_token_price } else { 0u64 };

		let remain = near_price % native_token_price;
		let fraction = Perbill::from_rational(remain, native_token_price);

		// (integer1, fraction1) = integer * coef
		let value1 = integer * (<Coef<T>>::get() as u64);
		let integer1 = if value1 >= 100 { value1 / 100 } else { 0u64 };

		if integer >= T::Threshold::get() {
			return (T::Threshold::get(), Perbill::zero())
		}

		let remain1 = value1 % 100;
		let fraction1 = Perbill::from_rational(remain1, 100);

		// fraction2 = fraction * coef
		let fraction2 = fraction * Perbill::from_rational(<Coef<T>>::get() as u64, 100);

		// result = (coef * integer) + (coef * fraction)
		// 		  = (integer1, fraction1) + (0, fraction2)
		(integer1, fraction1 + fraction2)
	}

	fn calculate_and_update_fee(near_price: u64, native_token_price: u64) {
		let (integer, fraction) =
			Self::calculate_quantity_of_native_tokens(near_price, native_token_price);

		let fee_integer = T::NativeTokenDecimals::get() * (integer as u128);
		let fee_fraction =
			T::NativeTokenDecimals::get() * (fraction.deconstruct() as u128) / 1_000_000_000;
		let ft_fee: BalanceOf<T> = (fee_integer + fee_fraction).checked_into().unwrap();
		// Notes: should modify later.
		let nft_fee = ft_fee;

		<CrosschainTransferFee<T>>::insert(CrossChainTransferType::Fungible, Some(ft_fee));
		<CrosschainTransferFee<T>>::insert(CrossChainTransferType::Nonfungible, Some(nft_fee));
	}

	fn do_lock_fungible_transfer_fee(sender: T::AccountId, fee: BalanceOf<T>) -> DispatchResult {
		let min_fee: Option<BalanceOf<T>> = <CrosschainTransferFee<T>>::try_get(
			CrossChainTransferType::Fungible,
		)
		.unwrap_or_else(|_| {
			// Need Check.
			log!(warn, "Storage CrosschainTransferFee is empty, default ft fee is zero.");
			0u128.checked_into()
		});

		if Some(fee) < min_fee {
			return Err(Error::<T>::InvalidFee.into())
		}

		T::Currency::transfer(&sender, &Self::account_id(), fee, AllowDeath)?;

		Ok(())
	}

	fn do_lock_nonfungible_transfer_fee(
		sender: T::AccountId,
		fee: BalanceOf<T>,
		_metadata_length: u32,
	) -> DispatchResult {
		let min_fee: Option<BalanceOf<T>> =
			<CrosschainTransferFee<T>>::try_get(CrossChainTransferType::Nonfungible)
				.unwrap_or_else(|_| {
					// Need Check.
					log!(warn, "Storage CrosschainTransferFee is empty, default ft fee is zero.");
					0u128.checked_into()
				});

		// The min_fee will be related to metadata_length in future.

		if Some(fee) < min_fee {
			return Err(Error::<T>::InvalidFee.into())
		}

		T::Currency::transfer(&sender, &Self::account_id(), fee, AllowDeath)?;

		Ok(())
	}
}
