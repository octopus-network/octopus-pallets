#![recursion_limit = "128"]
#![cfg_attr(not(feature = "std"), no_std)]
#![allow(unused_imports)]
#![allow(dead_code)]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;


use codec::{Decode, Encode};
use frame_support::{
    pallet_prelude::*,
    traits::{Currency, Get, OnUnbalanced, StorageVersion, UnixTime},
    PalletId,
};
use frame_system::{ensure_root, offchain::SendTransactionTypes, pallet_prelude::*};
use pallet_octopus_support::{
    log,
    traits::{AppchainInterface, LposInterface, UpwardMessagesInterface, ValidatorsProvider},
    types::{EraPayoutPayload, Offender, PayloadType, PlanNewEraPayload},
};

use scale_info::{
    prelude::string::{String, ToString},
    TypeInfo,
};
use sp_core::U256;
use sp_runtime::{
    traits::{AccountIdConversion, CheckedConversion, Convert, SaturatedConversion},
    KeyTypeId, RuntimeDebug,
};

use sp_std::{collections::btree_map::BTreeMap, convert::From, prelude::*};

pub use pallet::*;

pub(crate) const LOG_TARGET: &'static str = "runtime::octopus-lpos";

type TokenId = U256;

/// The current storage version.
const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct Erc721Token {
    pub id: TokenId,
    pub metadata: Vec<u8>,
}

#[frame_support::pallet]
pub mod pallet {
    use sp_core::U256;
    use sp_runtime::traits::Saturating;
    use super::*;

    #[pallet::pallet]
    #[pallet::generate_store(pub (super) trait Store)]
    #[pallet::without_storage_info]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The overarching event type.
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

        /// Some identifier for this token type, possibly the originating ethereum address.
        /// This is not explicitly used for anything, but may reflect the bridge's notion of resource ID.
        type Identifier: Get<[u8; 32]>;
    }

    /// Maps tokenId to Erc721 object
    #[pallet::storage]
    #[pallet::getter(fn tokens)]
    pub type Tokens<T: Config> = StorageMap<_, Blake2_128Concat, TokenId, Erc721Token>;

    /// Maps tokenId to owner
    #[pallet::storage]
    #[pallet::getter(fn owner_of)]
    pub type TokenOwner<T: Config> = StorageMap<_, Blake2_128Concat, TokenId, T::AccountId>;

    #[pallet::type_value]
    pub fn DefaultTokenCount() -> U256 {
        U256::zero()
    }

    /// Total number of tokens in existence
    #[pallet::storage]
    #[pallet::getter(fn token_count)]
    pub type TokenCount<T: Config> = StorageValue<_, U256, ValueQuery, DefaultTokenCount>;

    #[pallet::event]
    #[pallet::generate_deposit(pub (super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// New token created
        Minted(T::AccountId, TokenId),
        /// Token transfer between two parties
        Transferred(T::AccountId, T::AccountId, TokenId),
        /// Token removed from the system
        Burned(TokenId),
    }

    #[pallet::error]
    pub enum Error<T> {
        /// ID not recognized
        TokenIdDoesNotExist,
        /// Already exists with an owner
        TokenAlreadyExists,
        /// Origin is not owner
        NotOwner,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Creates a new token with the given token ID and metadata, and gives ownership to owner
        #[pallet::weight(195_000_0000)]
        pub fn mint(origin: OriginFor<T>, owner: T::AccountId, id: TokenId, metadata: Vec<u8>) -> DispatchResult {
            ensure_root(origin)?;

            Self::mint_token(owner, id, metadata)?;

            Ok(())
        }

        /// Changes ownership of a token sender owns
        #[pallet::weight(195_000_0000)]
        pub fn transfer(origin: OriginFor<T>, to: T::AccountId, id: TokenId) -> DispatchResult {
            let sender = ensure_signed(origin)?;

            Self::transfer_from(sender, to, id)?;

            Ok(())
        }

        /// Remove token from the system
        #[pallet::weight(195_000_0000)]
        pub fn burn(origin: OriginFor<T>, id: TokenId) -> DispatchResult {
            ensure_root(origin)?;

            let owner = Self::owner_of(id).ok_or(Error::<T>::TokenIdDoesNotExist)?;

            Self::burn_token(owner, id)?;

            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        /// Creates a new token in the system.
        pub fn mint_token(owner: T::AccountId, id: TokenId, metadata: Vec<u8>) -> DispatchResult {
            ensure!(!Tokens::<T>::contains_key(id), Error::<T>::TokenAlreadyExists);

            let new_token = Erc721Token { id, metadata };

            Tokens::<T>::insert(&id, new_token);
            TokenOwner::<T>::insert(&id, owner.clone());
            let new_total = TokenCount::<T>::get().saturating_add(U256::one());
            TokenCount::<T>::put(new_total);

            Self::deposit_event(Event::<T>::Minted(owner, id));

            Ok(().into())
        }

        /// Modifies ownership of a token
        pub fn transfer_from(from: T::AccountId, to: T::AccountId, id: TokenId) -> DispatchResult {
            // Check from is owner and token exists
            let owner = Self::owner_of(id).ok_or(Error::<T>::TokenIdDoesNotExist)?;
            ensure!(owner == from, Error::<T>::NotOwner);
            // Update owner
            TokenOwner::<T>::insert(&id, to.clone());

            Self::deposit_event(Event::<T>::Transferred(from, to, id));

            Ok(().into())
        }

        /// Deletes a token from the system.
        pub fn burn_token(from: T::AccountId, id: TokenId) -> DispatchResult {
            let owner = Self::owner_of(id).ok_or(Error::<T>::TokenIdDoesNotExist)?;
            ensure!(owner == from, Error::<T>::NotOwner);

            Tokens::<T>::remove(&id);
            TokenOwner::<T>::remove(&id);
            let new_total = TokenCount::<T>::get().saturating_add(U256::one());
            TokenCount::<T>::put(new_total);

            Self::deposit_event(Event::<T>::Burned(id));

            Ok(().into())
        }
    }
}