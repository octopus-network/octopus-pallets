#![cfg_attr(not(feature = "std"), no_std)]
mod mock;

extern crate alloc;

use alloc::string::ToString;
use frame_benchmarking::{
	benchmarks,
	frame_support::traits::{Currency, Get},
	whitelisted_caller, Vec,
};
use frame_system::{pallet_prelude::*, RawOrigin};
use pallet_octopus_appchain::{
	Config as AppchainConfig, Event as AppchainEvent, Pallet as AppchainPallet,
};
use pallet_octopus_support::traits::{
	AppchainInterface, TokenIdAndAssetIdProvider, ValidatorsProvider,
};
use sp_runtime::traits::{AccountIdConversion, CheckedConversion, StaticLookup};

pub struct Pallet<T: Config>(pallet_octopus_appchain::Pallet<T>);
pub trait Config:
	AppchainConfig
	+ pallet_assets::Config<pallet_assets::Instance1>
	+ pallet_uniques::Config<pallet_uniques::Instance1>
{
}

fn create_default_asset<T: pallet_assets::Config<I>, I: 'static>(
	is_sufficient: bool,
) -> (T::AccountId, <T::Lookup as StaticLookup>::Source) {
	let caller: T::AccountId = whitelisted_caller();
	let caller_lookup = T::Lookup::unlookup(caller.clone());
	let root = RawOrigin::Root.into();
	assert!(pallet_assets::Pallet::<T, I>::force_create(
		root,
		Default::default(),
		caller_lookup.clone(),
		is_sufficient,
		1u32.into(),
	)
	.is_ok());
	(caller, caller_lookup)
}

fn create_default_nft_class<T: pallet_uniques::Config<I>, I: 'static>(
	class_id: T::ClassId,
	is_free: bool,
) -> (T::AccountId, <T::Lookup as StaticLookup>::Source) {
	let caller: T::AccountId = whitelisted_caller();
	let caller_lookup = T::Lookup::unlookup(caller.clone());
	let root = RawOrigin::Root.into();

	assert!(pallet_uniques::Pallet::<T, I>::force_create(
		root,
		class_id,
		caller_lookup.clone(),
		is_free,
	)
	.is_ok());
	(caller, caller_lookup)
}

fn mint_default_nft<T: pallet_uniques::Config<I>, I: 'static>(
	class_id: T::ClassId,
	instance_id: T::InstanceId,
	owner: <T::Lookup as StaticLookup>::Source,
) {
	let receiver = T::Lookup::lookup(owner.clone()).unwrap();
	let origin = RawOrigin::Signed(receiver.clone());
	assert!(
		pallet_uniques::Pallet::<T, I>::mint(origin.into(), class_id, instance_id, owner).is_ok()
	);
}

fn assert_last_event<T: AppchainConfig>(generic_event: <T as AppchainConfig>::Event) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

benchmarks! {
	force_set_is_activated {
		  let mut is_activated: bool = true;
		  for i in 0 .. 100 {
			if i%2 == 0 {
				  is_activated = true;
			} else {
				  is_activated = false;
			}
		  }
	}: {
		let _ = AppchainPallet::<T>::force_set_is_activated(RawOrigin::Root.into(), is_activated);
	}
	verify {
		assert_eq!(AppchainPallet::<T>::is_activated(), is_activated);
	}

	force_set_next_set_id {
		let b in 2 .. 100000u32;
		let mut next_set_id:u32 = 1;
		for i in 2 .. b{
			next_set_id = i;
		}
	}: {
		let _ = AppchainPallet::<T>::force_set_next_set_id(RawOrigin::Root.into(), next_set_id);
	}
	verify {
		assert_eq!(AppchainPallet::<T>::next_set_id(), next_set_id);
	}

	force_set_planned_validators {
		let b in 2 .. 33u32;
		let mut validators: Vec<(T::AccountId, u128)> = Vec::new();
		for i in 1..b {
			validators.push((whitelisted_caller(), 100));
		}
	}: {
		let _ = AppchainPallet::<T>::force_set_planned_validators(RawOrigin::Root.into(), validators);
	}
	verify {
		assert_eq!(AppchainPallet::<T>::validators().len(), (b-1) as usize);
	}

	lock {
		let account = AppchainPallet::<T>::octopus_pallet_id().unwrap();
		let pallet_account: OriginFor<T> = RawOrigin::Signed(account).into();
		let min = <T as AppchainConfig>::Currency::minimum_balance();
		AppchainPallet::<T>::force_set_is_activated(RawOrigin::Root.into(), true).unwrap();
	}: {
		let _ = AppchainPallet::<T>::lock(
			pallet_account.clone(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
			min);
	}
	verify {
		let who = frame_system::ensure_signed(pallet_account).unwrap();
		assert_last_event::<T>(AppchainEvent::Locked {
			sender: who,
			receiver: "test-account.testnet".to_string().as_bytes().to_vec(),
			amount: min.into(),
			sequence: 1u64,
		}
		.into());
	}

	mint_asset {
		let (caller, receiver) = create_default_asset::<T, pallet_assets::Instance1>(true);
		<T as AppchainConfig>::Currency::make_free_balance_be(&caller, <T as AppchainConfig>::Currency::minimum_balance());
	}: {
		let _ = AppchainPallet::<T>::mint_asset(
			RawOrigin::Root.into(),
			0u32.into(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
			receiver.clone(),
			100000u32.into());
	}
	verify {
		let receiver = T::Lookup::lookup(receiver).unwrap();
		assert_last_event::<T>(AppchainEvent::AssetMinted {
			asset_id: 0u32.into(),
			sender: "test-account.testnet".to_string().as_bytes().to_vec(),
			receiver,
			amount: 100000u32.into(),
			sequence: None,
		}
		.into());
	}

	burn_asset {
		AppchainPallet::<T>::force_set_is_activated(RawOrigin::Root.into(), true).unwrap();
		let _ = AppchainPallet::<T>::set_token_id(
			RawOrigin::Root.into(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
			0u32.into()
		);

		let (caller, receiver) = create_default_asset::<T, pallet_assets::Instance1>(true);
		<T as AppchainConfig>::Currency::make_free_balance_be(&caller, <T as AppchainConfig>::Currency::minimum_balance());
		let _ = AppchainPallet::<T>::mint_asset(
			RawOrigin::Root.into(),
			0u32.into(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
			receiver.clone(),
			100000u32.into());

		let receiver = T::Lookup::lookup(receiver).unwrap();
	}: {
		let origin = RawOrigin::Signed(receiver.clone());
		let _ = AppchainPallet::<T>::burn_asset(
			origin.into(),
			0u32.into(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
			10000u32.into()
		);
	}
	verify {
		assert_last_event::<T>(AppchainEvent::AssetBurned {
			asset_id: 0u32.into(),
			sender: receiver.into(),
			receiver: "test-account.testnet".to_string().as_bytes().to_vec(),
			amount: 10000u32.into(),
			sequence: 1u64,
		}
		.into());
	}

	set_asset_name {
		AppchainPallet::<T>::force_set_is_activated(RawOrigin::Root.into(), true).unwrap();
	}: {

		let _ = AppchainPallet::<T>::set_token_id(
			RawOrigin::Root.into(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
			1u32.into()
		);
	}
	verify {
		let id = match <T as AppchainConfig>::AssetIdByTokenId::try_get_asset_id(
			"test-account.testnet".to_string().as_bytes().to_vec()) {
			Ok(v) => v,
			Err(_) => 1000000u32.into(),
		};
		assert_eq!(id, 1u32.into());
		let name = match <T as AppchainConfig>::AssetIdByTokenId::try_get_token_id(1u32.into()) {
			Ok(v) => v,
			Err(_) => "error.account".to_string().as_bytes().to_vec(),
		};
		assert_eq!(
			name,
			"test-account.testnet".to_string().as_bytes().to_vec(),
		);
	}

	tranfer_from_pallet_account {
		type Balance<T> = <<T as AppchainConfig>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

		let ass: Balance<T> = (1000_000_000_000_0000u128).checked_into().unwrap();
		let amount: Balance<T> = (1000_000_000_000_000_000u128).checked_into().unwrap();
		let account_id = <T as AppchainConfig>::PalletId::get().into_account();
		<T as AppchainConfig>::Currency::deposit_creating(&account_id, ass);

		let caller: T::AccountId = whitelisted_caller();
		let caller_lookup = T::Lookup::unlookup(caller.clone());
	}: {
		let _ = AppchainPallet::<T>::tranfer_from_pallet_account(
			RawOrigin::Root.into(),
			caller_lookup.clone(),
			amount,
		);
	}
	verify {
		let receiver = T::Lookup::lookup(caller_lookup).unwrap();
		assert_last_event::<T>(AppchainEvent::TransferredFromPallet {
			receiver,
			amount,
		}
		.into());
	}

	lock_nft {
		AppchainPallet::<T>::force_set_is_activated(RawOrigin::Root.into(), true).unwrap();
		let (caller, receiver) = create_default_nft_class::<T, pallet_uniques::Instance1>(Default::default(), true);
		mint_default_nft::<T, pallet_uniques::Instance1>(Default::default(), Default::default(), receiver.clone());
		let receiver = T::Lookup::lookup(receiver).unwrap();
	}: {
		let origin = RawOrigin::Signed(receiver.clone());
		let ret = AppchainPallet::<T>::lock_nft(
			origin.into(),
			Default::default(),
			Default::default(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
		);

		assert!(ret.is_ok());

	}
	verify {
		assert_last_event::<T>(AppchainEvent::NftLocked{
			sender: receiver.into(),
			receiver: "test-account.testnet".to_string().as_bytes().to_vec(),
			class: Default::default(),
			instance: Default::default(),
			sequence: 1u64,
		}
		.into());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test, extra = false);
}
