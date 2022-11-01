#![cfg(feature = "runtime-benchmarks")]
#![cfg_attr(not(feature = "std"), no_std)]
mod mock;

use codec::Decode;
use frame_benchmarking::{
	benchmarks,
	frame_support::traits::{Currency, Get},
	whitelisted_caller, Vec,
};
use frame_system::RawOrigin;
use pallet_octopus_appchain::{Config as AppchainConfig, Pallet as AppchainPallet};
use pallet_octopus_bridge::{Config as BridgeConfig, Event as BridgeEvent, Pallet as BridgePallet};
use pallet_octopus_support::traits::TokenIdAndAssetIdProvider;
use pallet_uniques::BenchmarkHelper;
use scale_info::prelude::{string::ToString, vec};
use sp_runtime::traits::{AccountIdConversion, CheckedConversion, StaticLookup};

pub struct Pallet<T: Config>(pallet_octopus_bridge::Pallet<T>);
pub trait Config:
	BridgeConfig
	+ AppchainConfig
	+ pallet_assets::Config<pallet_assets::Instance1>
	+ pallet_uniques::Config<pallet_uniques::Instance1>
{
}

fn create_default_asset<T: pallet_assets::Config<I>, I: 'static>(
	is_sufficient: bool,
) -> (T::AccountId, <T::Lookup as StaticLookup>::Source) {
	let caller: T::AccountId = whitelisted_caller();
	let caller_lookup = T::Lookup::unlookup(caller.clone());
	let data: Vec<u8> = vec![1, 0, 0, 0];
	let asset_id = T::AssetId::decode(&mut &*data).unwrap();
	let root = RawOrigin::Root.into();
	assert!(pallet_assets::Pallet::<T, I>::force_create(
		root,
		asset_id,
		caller_lookup.clone(),
		is_sufficient,
		1u32.into(),
	)
	.is_ok());
	(caller, caller_lookup)
}

fn create_default_nft_collection<T: pallet_uniques::Config<I>, I: 'static>(
	collection: u16,
	is_free: bool,
) -> (T::AccountId, <T::Lookup as StaticLookup>::Source) {
	let caller: T::AccountId = whitelisted_caller();
	let caller_lookup = T::Lookup::unlookup(caller.clone());
	let root = RawOrigin::Root.into();

	assert!(pallet_uniques::Pallet::<T, I>::force_create(
		root,
		T::Helper::collection(collection),
		caller_lookup.clone(),
		is_free,
	)
	.is_ok());
	(caller, caller_lookup)
}

fn mint_nft<T: pallet_uniques::Config<I>, I: 'static>(
	owner: <T::Lookup as StaticLookup>::Source,
	collection: u16,
	item: u16,
) {
	let receiver = T::Lookup::lookup(owner.clone()).unwrap();
	let origin = RawOrigin::Signed(receiver.clone());
	assert!(pallet_uniques::Pallet::<T, I>::mint(
		origin.into(),
		T::Helper::collection(collection),
		T::Helper::item(item),
		owner
	)
	.is_ok());
}

fn assert_last_event<T: BridgeConfig>(generic_event: <T as BridgeConfig>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

fn get_asset_id<T: BridgeConfig>(asset_id: Vec<u8>) -> T::AssetId {
	T::AssetId::decode(&mut &*asset_id).unwrap()
}

fn set_oracle_account_and_update_token_price<T: BridgeConfig>() {
	let caller: T::AccountId = whitelisted_caller();
	let source = T::Lookup::unlookup(caller.clone());
	let ret = BridgePallet::<T>::set_oracle_account(RawOrigin::Root.into(), source);
	assert!(ret.is_ok());
	let origin = RawOrigin::Signed(caller.clone());
	let ret = BridgePallet::<T>::set_token_price(origin.into(), 11, 1_000_000_000_000);
	assert!(ret.is_ok());
}

benchmarks! {
	lock {
		set_oracle_account_and_update_token_price::<T>();
		let min = <T as BridgeConfig>::Currency::minimum_balance();
		let caller = whitelisted_caller();
		let balance = 1_000_000_000_000_000_000_000_000u128;
		let _ = <T as BridgeConfig>::Currency::make_free_balance_be(&caller, balance.checked_into().unwrap());
		let origin = RawOrigin::Signed(caller.clone());

		let _ = AppchainPallet::<T>::force_set_is_activated(RawOrigin::Root.into(), true);
	}: {
		let ret = BridgePallet::<T>::lock(
			origin.into(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
			min,
			min,
		);
		assert!(ret.is_ok());
	}
	verify {
		assert_last_event::<T>(BridgeEvent::Locked {
			sender: caller,
			receiver: "test-account.testnet".to_string().as_bytes().to_vec(),
			amount: min.into(),
			fee: min.checked_into().unwrap(),
			sequence: 1u64,
		}
		.into());
	}

	burn_nep141 {
		set_oracle_account_and_update_token_price::<T>();
		AppchainPallet::<T>::force_set_is_activated(RawOrigin::Root.into(), true).unwrap();
		let asset_id = get_asset_id::<T>(vec![1,0,0,0]);
		let _ = BridgePallet::<T>::set_token_id(
			RawOrigin::Root.into(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
			asset_id,
		);

		let fee = <T as BridgeConfig>::Currency::minimum_balance();

		let (caller, receiver) = create_default_asset::<T, pallet_assets::Instance1>(true);
		<T as BridgeConfig>::Currency::make_free_balance_be(&caller, <T as BridgeConfig>::Currency::minimum_balance());
		let _ = BridgePallet::<T>::force_mint_nep141(
			RawOrigin::Root.into(),
			asset_id,
			receiver,
			100_000u32.into());
	}: {
		let origin = RawOrigin::Signed(caller.clone().into());
		let _ = BridgePallet::<T>::burn_nep141(
			origin.into(),
			asset_id,
			"test-account.testnet".to_string().as_bytes().to_vec(),
			10_000u32.into(),
			fee,
		);
	}
	verify {
		assert_last_event::<T>(BridgeEvent::Nep141Burned {
			asset_id,
			sender: caller,
			receiver: "test-account.testnet".to_string().as_bytes().to_vec(),
			amount: 10_000u32.into(),
			sequence: 1u64,
			fee,
		}
		.into());
	}

	//should use example convertor match this
	lock_nonfungible {
		set_oracle_account_and_update_token_price::<T>();
		AppchainPallet::<T>::force_set_is_activated(RawOrigin::Root.into(), true).unwrap();
		let (caller, receiver) = create_default_nft_collection::<T, pallet_uniques::Instance1>(0u16, true);
		mint_nft::<T, pallet_uniques::Instance1>(receiver, 0u16, 0u16);
		<T as BridgeConfig>::Currency::make_free_balance_be(&caller, <T as BridgeConfig>::Currency::minimum_balance());
		let fee = <T as BridgeConfig>::Currency::minimum_balance();
	}: {
		let origin = RawOrigin::Signed(caller.clone());
		let ret = BridgePallet::<T>::lock_nonfungible(
			origin.into(),
			0u128.into(),
			0u128.into(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
			fee,
			100u32,
		);

		assert!(ret.is_ok());

	}
	verify {
		assert_last_event::<T>(BridgeEvent::NonfungibleLocked{
			collection: 0u128.into(),
			item: 0u128.into(),
			sender: caller,
			receiver: "test-account.testnet".to_string().as_bytes().to_vec(),
			fee,
			sequence: 1u64,
		}
		.into());
	}

	set_token_id {
		AppchainPallet::<T>::force_set_is_activated(RawOrigin::Root.into(), true).unwrap();
		let asset_id = get_asset_id::<T>(vec![1,0,0,0]);
	}: {

		let _ = BridgePallet::<T>::set_token_id(
			RawOrigin::Root.into(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
			asset_id,
		);
	}
	verify {
		let id = match <T as BridgeConfig>::AssetIdByTokenId::try_get_asset_id(
			"test-account.testnet".to_string().as_bytes().to_vec()) {
			Ok(v) => v,
			Err(_) => get_asset_id::<T>(vec![100, 0, 0, 0]),
		};
		assert_eq!(id, asset_id);
		let name = match <T as BridgeConfig>::AssetIdByTokenId::try_get_token_id(asset_id) {
			Ok(v) => v,
			Err(_) => "error.account".to_string().as_bytes().to_vec(),
		};
		assert_eq!(
			name,
			"test-account.testnet".to_string().as_bytes().to_vec(),
		);
	}

	delete_token_id {
		AppchainPallet::<T>::force_set_is_activated(RawOrigin::Root.into(), true).unwrap();
		let asset_id = get_asset_id::<T>(vec![1,0,0,0]);
		let _ = BridgePallet::<T>::set_token_id(
			RawOrigin::Root.into(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
			asset_id
		);
	}: {
		let _ = BridgePallet::<T>::delete_token_id(
			RawOrigin::Root.into(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
		);
	}
	verify {
		let name = match <T as BridgeConfig>::AssetIdByTokenId::try_get_token_id(asset_id) {
			Ok(v) => v,
			Err(_) => "empty".to_string().as_bytes().to_vec(),
		};
		assert_eq!(
			name,
			"empty".to_string().as_bytes().to_vec(),
		);
	}

	force_unlock {
		type Balance<T> = <<T as BridgeConfig>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

		let ass: Balance<T> = (10_000_000_000_000_000u128).checked_into().unwrap();
		let amount: Balance<T> = (1_000_000_000_000_000_000u128).checked_into().unwrap();
		let account_id = <T as BridgeConfig>::PalletId::get().into_account_truncating();
		<T as BridgeConfig>::Currency::deposit_creating(&account_id, ass);

		let caller: T::AccountId = whitelisted_caller();
		let caller_lookup = T::Lookup::unlookup(caller.clone());
	}: {
		let ret = BridgePallet::<T>::force_unlock(
			RawOrigin::Root.into(),
			caller_lookup,
			amount,
		);

		assert!(ret.is_ok());
	}
	verify {
		assert_last_event::<T>(BridgeEvent::ForceUnlocked {
			who: caller,
			amount,
		}
		.into());
	}

	force_mint_nep141 {
		let (caller, receiver) = create_default_asset::<T, pallet_assets::Instance1>(true);
		<T as BridgeConfig>::Currency::make_free_balance_be(&caller, <T as BridgeConfig>::Currency::minimum_balance());
		let asset_id = get_asset_id::<T>(vec![1,0,0,0]);
	}: {
		let ret = BridgePallet::<T>::force_mint_nep141(
			RawOrigin::Root.into(),
			asset_id,
			receiver,
			100_000u32.into());
		assert!(ret.is_ok());
	}
	verify {
		assert_last_event::<T>(BridgeEvent::ForceNep141Minted {
			asset_id,
			who: caller,
			amount: 100_000u32.into(),
		}
		.into());
	}

	force_unlock_nonfungible {
		set_oracle_account_and_update_token_price::<T>();
		AppchainPallet::<T>::force_set_is_activated(RawOrigin::Root.into(), true).unwrap();
		let (caller, receiver) = create_default_nft_collection::<T, pallet_uniques::Instance1>(0u16, true);
		mint_nft::<T, pallet_uniques::Instance1>(receiver.clone(), 0u16, 0u16);
		let origin = RawOrigin::Signed(caller.clone());
		let fee = <T as BridgeConfig>::Currency::minimum_balance();
		let ret = BridgePallet::<T>::lock_nonfungible(
			origin.into(),
			0u128.into(),
			0u128.into(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
			fee,
			100u32,
		);
		let fee = <T as BridgeConfig>::Currency::minimum_balance();
	}: {
		let ret = BridgePallet::<T>::force_unlock_nonfungible(
			RawOrigin::Root.into(),
			receiver,
			0u128.into(),
			0u128.into(),
		);
		assert!(ret.is_ok());
	}
	verify {
		assert_last_event::<T>(BridgeEvent::ForceNonfungibleUnlocked{
			collection: 0u128.into(),
			item: 0u128.into(),
			who: caller,
		}
		.into());
	}

	set_oracle_account {
		let caller: T::AccountId = whitelisted_caller();
		let source = T::Lookup::unlookup(caller.clone());
	}: {
		let ret = BridgePallet::<T>::set_oracle_account(RawOrigin::Root.into(), source);
		assert!(ret.is_ok());
	}
	verify {
		assert_last_event::<T>(BridgeEvent::OracleAccountHasBeenSet{
			who: caller,
		}
		.into());
	}

	set_token_price {
		let caller: T::AccountId = whitelisted_caller();
		let source = T::Lookup::unlookup(caller.clone());
		let _ = BridgePallet::<T>::set_oracle_account(RawOrigin::Root.into(), source);
		let origin = RawOrigin::Signed(caller.clone());
	}: {
		let ret = BridgePallet::<T>::set_token_price(origin.into(), 1_000_000, 2000_000);
	}
	verify {
		assert_last_event::<T>(BridgeEvent::PriceUpdated{
			who: caller,
			near_price: 1_000_000,
			native_token_price: 2000_000,
		}
		.into());
	}
	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test, extra = false);
}
