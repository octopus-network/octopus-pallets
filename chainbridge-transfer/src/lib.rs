#![recursion_limit = "128"]
#![cfg_attr(not(feature = "std"), no_std)]
#![allow(dead_code)]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod fungible;
pub mod token;
mod traits;

use frame_support::{
	pallet_prelude::*,
	traits::{Currency, Get, StorageVersion},
};
use frame_system::pallet_prelude::*;
use pallet_chainbridge_erc721 as erc721;

use sp_runtime::traits::SaturatedConversion;

use frame_support::{
	dispatch::DispatchResult,
	ensure,
	traits::{EnsureOrigin, ExistenceRequirement::AllowDeath},
};
use frame_system::ensure_signed;
use pallet_chainbridge as bridge;
use scale_info::prelude::string::String;
use sp_core::U256;
use sp_std::{convert::From, prelude::*};

use crate::traits::AssetIdResourceIdProvider;
pub use pallet::*;

type ResourceId = bridge::ResourceId;

type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

/// The current storage version.
const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use crate::traits::AssetIdResourceIdProvider;
	use frame_support::traits::{
		fungibles::Mutate,
		tokens::{AssetId, Balance as AssetBalance},
	};

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	#[pallet::without_storage_info]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + bridge::Config + erc721::Config {
		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// Specifies the origin check provided by the bridge for calls that can only be called by
		/// the bridge pallet
		type BridgeOrigin: EnsureOrigin<Self::Origin, Success = Self::AccountId>;

		/// The currency mechanism.
		type Currency: Currency<Self::AccountId>;

		/// Identifier for the class of asset.
		type AssetId: AssetId + MaybeSerializeDeserialize;

		/// The units in which we record balances.
		type AssetBalance: AssetBalance + From<u128> + Into<u128>;

		/// Expose customizable associated type of asset transfer, lock and unlock
		type Fungibles: Mutate<
			Self::AccountId,
			AssetId = Self::AssetId,
			Balance = Self::AssetBalance,
		>;

		/// Map of cross-chain asset ID & name
		type AssetIdByName: AssetIdResourceIdProvider<Self::AssetId>;

		/// Max native token value
		type NativeTokenMaxValue: Get<BalanceOf<Self>>;

		/// Ids can be defined by the runtime and passed in, perhaps from blake2b_128 hashes.
		type HashId: Get<ResourceId>;

		type NativeTokenId: Get<ResourceId>;

		type Erc721Id: Get<ResourceId>;
	}

	#[pallet::storage]
	#[pallet::getter(fn resource_id_by_asset_id)]
	pub type ResourceIdOfAssetId<T: Config> =
		StorageMap<_, Blake2_128Concat, ResourceId, (T::AssetId, Vec<u8>)>;

	/// store generic hash
	#[pallet::storage]
	#[pallet::getter(fn assets_stored)]
	pub type AssetsStored<T: Config> = StorageMap<_, Blake2_128Concat, T::Hash, bool>;

	#[pallet::storage]
	#[pallet::getter(fn native_check)]
	pub type NativeCheck<T> = StorageValue<_, bool, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub asset_id_by_resource_id: Vec<(ResourceId, T::AssetId, String)>,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { asset_id_by_resource_id: Vec::new() }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			for (token_id, id, token_name) in self.asset_id_by_resource_id.iter() {
				<ResourceIdOfAssetId<T>>::insert(token_id, (id, token_name.as_bytes().to_vec()));
			}
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// deposit assets
		Deposit {
			sender: T::AccountId,
			recipient: T::AccountId,
			resource_id: ResourceId,
			amount: BalanceOf<T>,
		},
		/// Withdraw assets
		Withdraw {
			sender: T::AccountId,
			recipient: Vec<u8>,
			resource_id: ResourceId,
			amount: BalanceOf<T>,
		},
		Remark(T::Hash),
	}

	#[pallet::error]
	pub enum Error<T> {
		InvalidTransfer,
		InvalidTokenId,
		WrongAssetId,
		InvalidTokenName,
		OverTransferLimit,
		AssetAlreadyExists,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(195_000_0000)]
		pub fn set_token_id(
			origin: OriginFor<T>,
			resource_id: ResourceId,
			token_id: T::AssetId,
			token_name: Vec<u8>,
		) -> DispatchResult {
			ensure_root(origin)?;

			// verify token name is valid
			String::from_utf8(token_name.clone()).map_err(|_| <Error<T>>::InvalidTokenName)?;

			<ResourceIdOfAssetId<T>>::insert(resource_id, (token_id, token_name));

			Ok(())
		}

		#[pallet::weight(195_000_0000)]
		pub fn remove_token_id(origin: OriginFor<T>, resource_id: ResourceId) -> DispatchResult {
			ensure_root(origin)?;

			<ResourceIdOfAssetId<T>>::remove(resource_id);

			Ok(())
		}

		#[pallet::weight(195_000_0000)]
		pub fn native_limit(origin: OriginFor<T>, value: bool) -> DispatchResult {
			ensure_root(origin)?;

			<NativeCheck<T>>::put(value);

			Ok(())
		}

		//
		// Initiation calls. These start a bridge transfer.
		//

		/// Transfers an arbitrary hash to a (whitelisted) destination chain.
		#[pallet::weight(195_000_0000)]
		pub fn transfer_hash(
			origin: OriginFor<T>,
			hash: T::Hash,
			dest_id: bridge::ChainId,
		) -> DispatchResult {
			ensure_signed(origin)?;

			let resource_id = T::HashId::get();
			let metadata: Vec<u8> = hash.as_ref().to_vec();
			<bridge::Pallet<T>>::transfer_generic(dest_id, resource_id, metadata)
		}

		#[pallet::weight(195_000_0000)]
		pub fn transfer_native(
			origin: OriginFor<T>,
			amount: BalanceOf<T>,
			recipient: Vec<u8>,
			dest_id: bridge::ChainId,
		) -> DispatchResult {
			let native_token = T::NativeTokenId::get();

			Self::generic_token_transfer(origin, amount, native_token, recipient, dest_id)
		}

		/// Transfers some amount of the native token to some recipient on a (whitelisted)
		/// destination chain.
		#[pallet::weight(195_000_0000)]
		pub fn generic_token_transfer(
			origin: OriginFor<T>,
			amount: BalanceOf<T>,
			r_id: ResourceId,
			recipient: Vec<u8>,
			dest_id: bridge::ChainId,
		) -> DispatchResult {
			let source = ensure_signed(origin)?;
			ensure!(<bridge::Pallet<T>>::chain_whitelisted(dest_id), <Error<T>>::InvalidTransfer);
			// TODO
			// check recipient address is verify

			match r_id == T::NativeTokenId::get() {
				true => Self::do_lock(source, amount, r_id, recipient, dest_id)?,
				false => Self::do_burn_assets(source, amount, r_id, recipient, dest_id)?,
			}

			Ok(())
		}

		/// Transfer a non-fungible token (erc721) to a (whitelisted) destination chain.
		#[pallet::weight(195_000_0000)]
		pub fn transfer_erc721(
			origin: OriginFor<T>,
			recipient: Vec<u8>,
			token_id: U256,
			dest_id: bridge::ChainId,
		) -> DispatchResult {
			let source = ensure_signed(origin)?;
			ensure!(<bridge::Pallet::<T>>::chain_whitelisted(dest_id), Error::<T>::InvalidTransfer);
			match <erc721::Pallet<T>>::tokens(&token_id) {
				Some(token) => {
					<erc721::Pallet<T>>::burn_token(source, token_id)?;
					let resource_id = T::Erc721Id::get();
					let tid: &mut [u8] = &mut [0; 32];
					token_id.to_big_endian(tid);
					<bridge::Pallet<T>>::transfer_nonfungible(
						dest_id,
						resource_id,
						tid.to_vec(),
						recipient,
						token.metadata,
					)
				},
				None => Err(Error::<T>::InvalidTransfer)?,
			}
		}

		//
		// Executable calls. These can be triggered by a bridge transfer initiated on another chain
		//

		/// Executes a simple currency transfer using the bridge account as the source
		/// Triggered by a initial transfer on source chain, executed by relayer when proposal was
		/// resolved. this function by bridge triggered transfer
		#[pallet::weight(195_000_0000)]
		pub fn transfer(
			origin: OriginFor<T>,
			to: T::AccountId,
			amount: BalanceOf<T>,
			r_id: ResourceId,
		) -> DispatchResult {
			let source = T::BridgeOrigin::ensure_origin(origin)?;

			// this do native transfer
			match r_id == T::NativeTokenId::get() {
				true => Self::do_unlock(source, to, amount.into())?,
				false => {
					Self::do_mint_assets(to, amount, r_id)?;
				},
			}
			Ok(())
		}

		/// This can be called by the bridge to demonstrate an arbitrary call from a proposal.
		#[pallet::weight(195_000_0000)]
		pub fn remark(origin: OriginFor<T>, hash: T::Hash, _r_id: ResourceId) -> DispatchResult {
			T::BridgeOrigin::ensure_origin(origin)?;
			ensure!(!AssetsStored::<T>::contains_key(hash), <Error<T>>::AssetAlreadyExists);
			//store the hash value
			<AssetsStored<T>>::insert(&hash, true);
			Self::deposit_event(Event::Remark(hash));
			Ok(())
		}

		/// Allows the bridge to issue new erc721 tokens
		#[pallet::weight(195_000_0000)]
		pub fn mint_erc721(
			origin: OriginFor<T>,
			recipient: T::AccountId,
			id: U256,
			metadata: Vec<u8>,
			_r_id: ResourceId,
		) -> DispatchResult {
			T::BridgeOrigin::ensure_origin(origin)?;
			<erc721::Pallet<T>>::mint_token(recipient, id, metadata)?;
			Ok(())
		}
	}
}

impl<T: Config> AssetIdResourceIdProvider<T::AssetId> for Pallet<T> {
	type Err = Error<T>;

	fn try_get_asset_id(resource_id: ResourceId) -> Result<<T as Config>::AssetId, Self::Err> {
		let asset_id = <ResourceIdOfAssetId<T>>::try_get(resource_id);
		match asset_id {
			Ok(id) => Ok(id.0),
			_ => Err(<Error<T>>::InvalidTokenId),
		}
	}

	fn try_get_asset_name(asset_id: T::AssetId) -> Result<ResourceId, Self::Err> {
		let token_id = <ResourceIdOfAssetId<T>>::iter().find(|p| p.1 .0 == asset_id).map(|p| p.0);
		match token_id {
			Some(id) => Ok(id),
			_ => Err(<Error<T>>::WrongAssetId),
		}
	}
}
