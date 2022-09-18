#![cfg_attr(not(feature = "std"), no_std)]
mod mock;

use codec::Decode;
use frame_benchmarking::{
	benchmarks,
	frame_support::traits::{Currency, Get},
	whitelisted_caller, Vec,
};
use frame_system::{
	offchain::{AppCrypto, SigningTypes},
	RawOrigin,
};
use pallet_octopus_appchain::{
	BurnEvent, Config as AppchainConfig, Event as AppchainEvent, LockAssetEvent, Observation,
	ObservationsPayload, Pallet as AppchainPallet, Validator, ValidatorSet,
};
use pallet_octopus_support::traits::{AppchainInterface, TokenIdAndAssetIdProvider};
use pallet_uniques::BenchmarkHelper;
use scale_info::prelude::{format, string::ToString, vec};
use sp_runtime::{
	traits::{AccountIdConversion, CheckedConversion, StaticLookup},
	RuntimeAppPublic,
};

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
	is_free: bool,
) -> (T::AccountId, <T::Lookup as StaticLookup>::Source) {
	let caller: T::AccountId = whitelisted_caller();
	let caller_lookup = T::Lookup::unlookup(caller.clone());
	let root = RawOrigin::Root.into();

	assert!(
		pallet_uniques::Pallet::<T, I>::force_create(root, caller_lookup.clone(), is_free,).is_ok()
	);
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

fn assert_last_event<T: AppchainConfig>(generic_event: <T as AppchainConfig>::Event) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

fn produce_validator_set<T: AppchainConfig>(index: u32) -> Observation<T::AccountId> {
	let receiver: T::AccountId = whitelisted_caller();
	let validator = Validator { validator_id_in_appchain: receiver, total_stake: 100 };
	Observation::UpdateValidatorSet(ValidatorSet { set_id: index, validators: vec![validator] })
}

fn produce_burn_notification<T: AppchainConfig>(index: u32) -> Observation<T::AccountId> {
	let receiver: T::AccountId = whitelisted_caller();
	Observation::Burn(BurnEvent {
		index,
		sender_id: "pallet-test.testnet".to_string().as_bytes().to_vec(),
		receiver,
		amount: 10000000000000000000,
	})
}

fn produce_lock_asset_notification<T: AppchainConfig>(
	index: u32,
	token_id: Vec<u8>,
) -> Observation<T::AccountId> {
	let receiver: T::AccountId = whitelisted_caller();
	Observation::LockAsset(LockAssetEvent {
		index,
		token_id,
		sender_id: "pallet-test.testnet".to_string().as_bytes().to_vec(),
		receiver,
		amount: 100000,
	})
}

fn get_submitter_information<T: AppchainConfig>(
) -> (<T as SigningTypes>::Public, <T as SigningTypes>::Signature, Vec<u8>) {
	const PHRASE: &str = "bottom drive obey lake curtain smoke basket hold race lonely fit walk";
	let key = <T::AppCrypto as AppCrypto<
		<T as SigningTypes>::Public,
		<T as SigningTypes>::Signature,
	>>::RuntimeAppPublic::generate_pair(Some(
		format!("{}//Alice", PHRASE).as_bytes().to_vec(),
	));
	let key_data = key.to_raw_vec();

	let generic_public = <T::AppCrypto as AppCrypto<
		<T as SigningTypes>::Public,
		<T as SigningTypes>::Signature,
	>>::GenericPublic::from(key);
	let public: <T as SigningTypes>::Public = generic_public.into();
	let sig = <T::AppCrypto as AppCrypto<
		<T as SigningTypes>::Public,
		<T as SigningTypes>::Signature,
	>>::sign(&frame_benchmarking::vec![1, 2, 3], public.clone())
	.unwrap();
	(public, sig, key_data)
}

fn get_asset_id<T: AppchainConfig>(asset_id: Vec<u8>) -> T::AssetId {
	T::AssetId::decode(&mut &*asset_id).unwrap()
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
		let next_set_id:u32 = 1;
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
		let min = <T as AppchainConfig>::Currency::minimum_balance();
		AppchainPallet::<T>::force_set_is_activated(RawOrigin::Root.into(), true).unwrap();

		let caller = whitelisted_caller();

		let balance = 1000000000000000000000000u128;
		let _ = <T as AppchainConfig>::Currency::make_free_balance_be(&caller, balance.checked_into().unwrap());
		let origin = RawOrigin::Signed(caller.clone());
	}: {
		let ret = AppchainPallet::<T>::lock(
			origin.into(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
			min);
		assert!(ret.is_ok());
	}
	verify {
		assert_last_event::<T>(AppchainEvent::Locked {
			sender: caller,
			receiver: "test-account.testnet".to_string().as_bytes().to_vec(),
			amount: min.into(),
			sequence: 1u64,
		}
		.into());
	}

	mint_asset {
		let (caller, receiver) = create_default_asset::<T, pallet_assets::Instance1>(true);
		<T as AppchainConfig>::Currency::make_free_balance_be(&caller, <T as AppchainConfig>::Currency::minimum_balance());
		let asset_id = get_asset_id::<T>(vec![1,0,0,0]);
	}: {
		let _ = AppchainPallet::<T>::mint_asset(
			RawOrigin::Root.into(),
			asset_id,
			"test-account.testnet".to_string().as_bytes().to_vec(),
			receiver,
			100000u32.into());
	}
	verify {
		assert_last_event::<T>(AppchainEvent::AssetMinted {
			asset_id,
			sender: "test-account.testnet".to_string().as_bytes().to_vec(),
			receiver: caller,
			amount: 100000u32.into(),
			sequence: None,
		}
		.into());
	}

	burn_asset {
		AppchainPallet::<T>::force_set_is_activated(RawOrigin::Root.into(), true).unwrap();
		let asset_id = get_asset_id::<T>(vec![1,0,0,0]);
		let _ = AppchainPallet::<T>::set_token_id(
			RawOrigin::Root.into(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
			asset_id,
		);

		let (caller, receiver) = create_default_asset::<T, pallet_assets::Instance1>(true);
		<T as AppchainConfig>::Currency::make_free_balance_be(&caller, <T as AppchainConfig>::Currency::minimum_balance());
		let _ = AppchainPallet::<T>::mint_asset(
			RawOrigin::Root.into(),
			asset_id,
			"test-account.testnet".to_string().as_bytes().to_vec(),
			receiver,
			100000u32.into());
	}: {
		let origin = RawOrigin::Signed(caller.clone().into());
		let _ = AppchainPallet::<T>::burn_asset(
			origin.into(),
			asset_id,
			"test-account.testnet".to_string().as_bytes().to_vec(),
			10000u32.into()
		);
	}
	verify {
		assert_last_event::<T>(AppchainEvent::AssetBurned {
			asset_id,
			sender: caller,
			receiver: "test-account.testnet".to_string().as_bytes().to_vec(),
			amount: 10000u32.into(),
			sequence: 1u64,
		}
		.into());
	}

	set_token_id {
		AppchainPallet::<T>::force_set_is_activated(RawOrigin::Root.into(), true).unwrap();
		let asset_id = get_asset_id::<T>(vec![1,0,0,0]);
	}: {

		let _ = AppchainPallet::<T>::set_token_id(
			RawOrigin::Root.into(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
			asset_id,
		);
	}
	verify {
		let id = match <T as AppchainConfig>::AssetIdByTokenId::try_get_asset_id(
			"test-account.testnet".to_string().as_bytes().to_vec()) {
			Ok(v) => v,
			Err(_) => get_asset_id::<T>(vec![100, 0, 0, 0]),
		};
		assert_eq!(id, asset_id);
		let name = match <T as AppchainConfig>::AssetIdByTokenId::try_get_token_id(asset_id) {
			Ok(v) => v,
			Err(_) => "error.account".to_string().as_bytes().to_vec(),
		};
		assert_eq!(
			name,
			"test-account.testnet".to_string().as_bytes().to_vec(),
		);
	}

	force_unlock {
		type Balance<T> = <<T as AppchainConfig>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

		let ass: Balance<T> = (1000_000_000_000_0000u128).checked_into().unwrap();
		let amount: Balance<T> = (1000_000_000_000_000_000u128).checked_into().unwrap();
		let account_id = <T as AppchainConfig>::PalletId::get().into_account_truncating();
		<T as AppchainConfig>::Currency::deposit_creating(&account_id, ass);

		let caller: T::AccountId = whitelisted_caller();
		let caller_lookup = T::Lookup::unlookup(caller.clone());
	}: {
		let ret = AppchainPallet::<T>::force_unlock(
			RawOrigin::Root.into(),
			caller_lookup,
			amount,
		);

		assert!(ret.is_ok());
	}
	verify {
		assert_last_event::<T>(AppchainEvent::ForceUnlock {
			who: caller,
			amount,
		}
		.into());
	}

	//should use example convertor match this
	lock_nft {
		AppchainPallet::<T>::force_set_is_activated(RawOrigin::Root.into(), true).unwrap();
		let (caller, receiver) = create_default_nft_collection::<T, pallet_uniques::Instance1>(true);
		mint_nft::<T, pallet_uniques::Instance1>(receiver, 0u16, 0u16);
	}: {
		let origin = RawOrigin::Signed(caller.clone());
		let ret = AppchainPallet::<T>::lock_nft(
			origin.into(),
			0u128.into(),
			0u128.into(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
		);

		assert!(ret.is_ok());

	}
	verify {
		assert_last_event::<T>(AppchainEvent::NftLocked{
			sender: caller,
			receiver: "test-account.testnet".to_string().as_bytes().to_vec(),
			collection: 0u128.into(),
			item: 0u128.into(),
			sequence: 1u64,
		}
		.into());
	}

	delete_token_id {
		AppchainPallet::<T>::force_set_is_activated(RawOrigin::Root.into(), true).unwrap();
		let asset_id = get_asset_id::<T>(vec![1,0,0,0]);
		let _ = AppchainPallet::<T>::set_token_id(
			RawOrigin::Root.into(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
			asset_id
		);
	}: {
		let _ = AppchainPallet::<T>::delete_token_id(
			RawOrigin::Root.into(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
		);
	}
	verify {
		let name = match <T as AppchainConfig>::AssetIdByTokenId::try_get_token_id(asset_id) {
			Ok(v) => v,
			Err(_) => "empty".to_string().as_bytes().to_vec(),
		};
		assert_eq!(
			name,
			"empty".to_string().as_bytes().to_vec(),
		);
	}

	submit_observations {
		let b in 2 .. 10u32;
		let mut obs: Vec<Observation<<T as frame_system::Config>::AccountId>> = Vec::new();

		let (caller, _receiver) = create_default_asset::<T, pallet_assets::Instance1>(true);
		<T as AppchainConfig>::Currency::make_free_balance_be(&caller, <T as AppchainConfig>::Currency::minimum_balance());

		obs.push(produce_validator_set::<T>(1u32));

		for i in 1..b {
			if i%2 == 1 {
				obs.push(produce_lock_asset_notification::<T>(i, "usdn.testnet".to_string().as_bytes().to_vec()));
			} else {
				obs.push(produce_burn_notification::<T>(i));
			}
		}

		let (public, signature, key_data) = get_submitter_information::<T>();
		let payload = ObservationsPayload {
			public,
			key_data,
			block_number: 1u32.into(),
			observations: obs,
		};

	}: {
		let ret = AppchainPallet::<T>::submit_observations(RawOrigin::None.into(), payload, signature);
		assert!(ret.is_ok());
	}

	force_set_next_notification_id {
		let next_notification_id:u32 = 1;
	}: {
		let ret = AppchainPallet::<T>::force_set_next_notification_id(RawOrigin::Root.into(), next_notification_id);
		assert!(ret.is_ok());
	}

	force_mint_asset {
		let (caller, receiver) = create_default_asset::<T, pallet_assets::Instance1>(true);
		<T as AppchainConfig>::Currency::make_free_balance_be(&caller, <T as AppchainConfig>::Currency::minimum_balance());
		let asset_id = get_asset_id::<T>(vec![1,0,0,0]);
	}: {
		let ret = AppchainPallet::<T>::force_mint_asset(
			RawOrigin::Root.into(),
			asset_id,
			receiver,
			100000u32.into());
		assert!(ret.is_ok());
	}
	verify {
		assert_last_event::<T>(AppchainEvent::ForceAssetMinted{
			asset_id,
			who: caller,
			amount: 100000u32.into(),
		}
		.into());
	}

	force_unlock_nft {
		AppchainPallet::<T>::force_set_is_activated(RawOrigin::Root.into(), true).unwrap();
		let (caller, receiver) = create_default_nft_collection::<T, pallet_uniques::Instance1>(true);
		mint_nft::<T, pallet_uniques::Instance1>(receiver.clone(), 0u16, 0u16);
		let origin = RawOrigin::Signed(caller.clone());
		let ret = AppchainPallet::<T>::lock_nft(
			origin.into(),
			0u128.into(),
			0u128.into(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
		);
	}: {
		let ret = AppchainPallet::<T>::force_unlock_nft(
			RawOrigin::Root.into(),
			receiver,
			0u128.into(),
			0u128.into());
		assert!(ret.is_ok());
	}
	verify {
		assert_last_event::<T>(AppchainEvent::ForceNftUnlock{
			collection: 0u128.into(),
			item: 0u128.into(),
			who: caller,
		}
		.into());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test, extra = false);
}
