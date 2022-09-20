#![recursion_limit = "128"]
#![cfg_attr(not(feature = "std"), no_std)]
#![allow(dead_code)]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

mod traits;

use frame_support::{
	pallet_prelude::*,
	traits::{Currency, Get, StorageVersion},
};
use frame_system::pallet_prelude::*;

use sp_runtime::traits::SaturatedConversion;

use frame_support::{
	dispatch::DispatchResult,
	ensure,
	traits::{EnsureOrigin, ExistenceRequirement::AllowDeath},
};
use frame_system::ensure_signed;
use pallet_chainbridge as bridge;
use sp_core::U256;
use sp_std::{convert::From, prelude::*};
use scale_info::prelude::string::String;

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
	use codec::Codec;
	use core::fmt::Debug;
	use frame_support::traits::fungibles::{Inspect, Mutate, Transfer};
	use sp_arithmetic::traits::AtLeast32BitUnsigned;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	#[pallet::without_storage_info]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + bridge::Config {
		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// Specifies the origin check provided by the bridge for calls that can only be called by
		/// the bridge pallet
		type BridgeOrigin: EnsureOrigin<Self::Origin, Success = Self::AccountId>;

		/// The currency mechanism.
		type Currency: Currency<Self::AccountId>;

		type NativeTokenId: Get<ResourceId>;

		/// Identifier for the class of asset.
		type AssetId: Member
			+ Parameter
			+ AtLeast32BitUnsigned
			+ Codec
			+ Copy
			+ Debug
			+ Default
			+ MaybeSerializeDeserialize;

		/// The units in which we record balances.
		type AssetBalance: Parameter
			+ Member
			+ AtLeast32BitUnsigned
			+ Codec
			+ Default
			+ From<u128>
			+ Into<u128>
			+ Copy
			+ MaybeSerializeDeserialize
			+ Debug;

		/// Expose customizable associated type of asset transfer, lock and unlock
		type Assets: Transfer<Self::AccountId, AssetId = Self::AssetId, Balance = Self::AssetBalance>
			+ Mutate<Self::AccountId, AssetId = Self::AssetId, Balance = Self::AssetBalance>
			+ Inspect<Self::AccountId, AssetId = Self::AssetId, Balance = Self::AssetBalance>;

		/// Map of cross-chain asset ID & name
		type AssetIdByName: AssetIdResourceIdProvider<Self::AssetId>;
	}

	#[pallet::storage]
	#[pallet::getter(fn resource_id_by_asset_id)]
	pub type ResourceIdOfAssetId<T: Config> =
		StorageMap<_, Blake2_128Concat, ResourceId, (T::AssetId, Vec<u8>)>;

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
		}
	}

	#[pallet::error]
	pub enum Error<T> {
		InvalidTransfer,
		InvalidTokenId,
		WrongAssetId,
		InvalidTokenName,
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
			String::from_utf8(token_name.clone()).map_err(|_| Error::<T>::InvalidTokenName)?;

			ResourceIdOfAssetId::<T>::insert(resource_id, (token_id, token_name));

			Ok(())
		}

		#[pallet::weight(195_000_0000)]
		pub fn remove_token_id(origin: OriginFor<T>, resource_id: ResourceId) -> DispatchResult {
			ensure_root(origin)?;

			ResourceIdOfAssetId::<T>::remove(resource_id);

			Ok(())
		}

		//
		// Initiation calls. These start a bridge transfer.
		//

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
			ensure!(<bridge::Pallet::<T>>::chain_whitelisted(dest_id), Error::<T>::InvalidTransfer);
			// TODO
			// check recipient address is verify

			match r_id == T::NativeTokenId::get() {
				true => {
					log::info!("transfer native token");
					let bridge_id = <bridge::Pallet<T>>::account_id();
					<T as Config>::Currency::transfer(
						&source,
						&bridge_id,
						amount.into(),
						AllowDeath,
					)?;
					log::info!("transfer native token successful");

					<bridge::Pallet<T>>::transfer_fungible(
						dest_id,
						r_id,
						recipient.clone(),
						U256::from(amount.saturated_into::<u128>()),
					)?;
				},
				false => {
					log::info!("transfer non-native_token: burn assets");
					let amount = amount.saturated_into::<u128>();
					let token_id = Self::try_get_asset_id(r_id)?;
					<T::Assets as Mutate<T::AccountId>>::burn_from(
						token_id,
						&source,
						amount.into(),
					)?;
					log::info!("transfer non-native_token: burn successful!");
					<bridge::Pallet<T>>::transfer_fungible(
						dest_id,
						r_id,
						recipient.clone(),
						U256::from(amount.saturated_into::<u128>()),
					)?;
				},
			}

			Ok(())
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
				true => {
					<T as Config>::Currency::transfer(&source, &to, amount.into(), AllowDeath)?;
				},
				false => {
					log::info!("mint assets");
					let amount = amount.saturated_into::<u128>();
					let token_id = Self::try_get_asset_id(r_id)?;
					<T::Assets as Mutate<T::AccountId>>::mint_into(token_id, &to, amount.into())?;
					log::info!("mint assets successful!");
				},
			}
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
			_ => Err(Error::<T>::InvalidTokenId),
		}
	}

	fn try_get_asset_name(asset_id: T::AssetId) -> Result<ResourceId, Self::Err> {
		let token_id = <ResourceIdOfAssetId<T>>::iter().find(|p| p.1.0 == asset_id).map(|p| p.0);
		match token_id {
			Some(id) => Ok(id),
			_ => Err(Error::<T>::WrongAssetId),
		}
	}
}
