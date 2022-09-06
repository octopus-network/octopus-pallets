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
use sp_runtime::{
    traits::{AccountIdConversion, CheckedConversion, Convert, SaturatedConversion},
    KeyTypeId, RuntimeDebug,
};

use sp_std::{collections::btree_map::BTreeMap, convert::From, prelude::*};

pub use pallet::*;

pub(crate) const LOG_TARGET: &'static str = "runtime::octopus-lpos";


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
    pub trait Config: frame_system::Config {
        /// The overarching event type.
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub (super) fn deposit_event)]
    pub enum Event<T: Config> {}

    #[pallet::error]
    pub enum Error<T> {}

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    #[pallet::call]
    impl<T: Config> Pallet<T> {}
}