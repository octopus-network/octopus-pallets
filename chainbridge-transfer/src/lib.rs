#![recursion_limit = "128"]
#![cfg_attr(not(feature = "std"), no_std)]
// #![allow(unused_imports)]
// #![allow(dead_code)]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

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

pub use pallet::*;

type ResourceId = bridge::ResourceId;

type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

/// The current storage version.
const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

#[frame_support::pallet]
pub mod pallet {
	use super::*;

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

		/// Ids can be defined by the runtime and passed in, perhaps from blake2b_128 hashes.
		type HashId: Get<ResourceId>;
		type NativeTokenId: Get<ResourceId>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		Remark(T::Hash),
	}

	#[pallet::error]
	pub enum Error<T> {
		InvalidTransfer,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		//
		// Initiation calls. These start a bridge transfer.
		//

		/// Transfers some amount of the native token to some recipient on a (whitelisted)
		/// destination chain.
		#[pallet::weight(195_000_0000)]
		pub fn transfer_native(
			origin: OriginFor<T>,
			amount: BalanceOf<T>,
			recipient: Vec<u8>,
			dest_id: bridge::ChainId,
		) -> DispatchResult {
			let source = ensure_signed(origin)?;
			ensure!(<bridge::Pallet::<T>>::chain_whitelisted(dest_id), Error::<T>::InvalidTransfer);
			let bridge_id = <bridge::Pallet<T>>::account_id();
			<T as Config>::Currency::transfer(&source, &bridge_id, amount.into(), AllowDeath)?;

			let resource_id = T::NativeTokenId::get();
			<bridge::Pallet<T>>::transfer_fungible(
				dest_id,
				resource_id,
				recipient,
				U256::from(amount.saturated_into::<u128>()),
			)
		}

		//
		// Executable calls. These can be triggered by a bridge transfer initiated on another chain
		//

		/// Executes a simple currency transfer using the bridge account as the source
		/// Triggered by a initial transfer on source chain, executed by relayer when proposal was resolved.
		/// this function by bridge triggered transfer
		/// so have two token transfer
		/// - native token transfer
		/// - no-native token transfer
		#[pallet::weight(195_000_0000)]
		pub fn transfer(
			origin: OriginFor<T>,
			to: T::AccountId,
			amount: BalanceOf<T>,
			r_id: ResourceId,
		) -> DispatchResult {
			let source = T::BridgeOrigin::ensure_origin(origin)?;

			// this do native transfer
			// if r_id == T::NativeTokenId::get() {
				<T as Config>::Currency::transfer(&source, &to, amount.into(), AllowDeath)?;
			// } else {
				// do asset mint
			// }

			Ok(())
		}

		/// This can be called by the bridge to demonstrate an arbitrary call from a proposal.
		#[pallet::weight(195_000_0000)]
		pub fn remark(origin: OriginFor<T>, hash: T::Hash, _r_id: ResourceId) -> DispatchResult {
			T::BridgeOrigin::ensure_origin(origin)?;
			Self::deposit_event(Event::<T>::Remark(hash));
			Ok(())
		}
	}
}
