//#![cfg_attr(not(feature = "std"), no_std)]
//mod mock;
//
//use codec::Decode;
//use frame_benchmarking::{benchmarks, whitelisted_caller, Vec};
//use frame_system::{
//	offchain::{AppCrypto, SigningTypes},
//	RawOrigin,
//};
//use pallet_octopus_appchain::{
//	types::{BurnEvent, LockAssetEvent, Observation, ObservationsPayload, Validator, ValidatorSet},
//	Config as AppchainConfig, Pallet as AppchainPallet,
//};
//use pallet_octopus_support::traits::AppchainInterface;
//use scale_info::prelude::{format, string::ToString, vec};
//use sp_runtime::{traits::StaticLookup, RuntimeAppPublic};
//
//pub struct Pallet<T: Config>(pallet_octopus_appchain::Pallet<T>);
//pub trait Config: AppchainConfig + pallet_assets::Config<pallet_assets::Instance1> {}
//
//fn create_default_asset<T: pallet_assets::Config<I>, I: 'static>(
//	is_sufficient: bool,
//) -> (T::AccountId, <T::Lookup as StaticLookup>::Source) {
//	let caller: T::AccountId = whitelisted_caller();
//	let caller_lookup = T::Lookup::unlookup(caller.clone());
//	let data: Vec<u8> = vec![1, 0, 0, 0];
//	let asset_id = T::AssetId::decode(&mut &*data).unwrap();
//	let root = RawOrigin::Root.into();
//	assert!(pallet_assets::Pallet::<T, I>::force_create(
//		root,
//		asset_id,
//		caller_lookup.clone(),
//		is_sufficient,
//		1u32.into(),
//	)
//	.is_ok());
//	(caller, caller_lookup)
//}
//
//fn produce_validator_set<T: AppchainConfig>(index: u32) -> Observation<T::AccountId> {
//	let receiver: T::AccountId = whitelisted_caller();
//	let validator = Validator { validator_id_in_appchain: receiver, total_stake: 100 };
//	Observation::UpdateValidatorSet(ValidatorSet { set_id: index, validators: vec![validator] })
//}
//
//fn produce_burn_notification<T: AppchainConfig>(index: u32) -> Observation<T::AccountId> {
//	let receiver: T::AccountId = whitelisted_caller();
//	Observation::Burn(BurnEvent {
//		index,
//		sender_id: "pallet-test.testnet".to_string().as_bytes().to_vec(),
//		receiver,
//		amount: 10000000000000000000,
//	})
//}
//
//fn produce_lock_asset_notification<T: AppchainConfig>(
//	index: u32,
//	token_id: Vec<u8>,
//) -> Observation<T::AccountId> {
//	let receiver: T::AccountId = whitelisted_caller();
//	Observation::LockAsset(LockAssetEvent {
//		index,
//		token_id,
//		sender_id: "pallet-test.testnet".to_string().as_bytes().to_vec(),
//		receiver,
//		amount: 100000,
//	})
//}
//
//fn get_submitter_information<T: AppchainConfig>(
//) -> (<T as SigningTypes>::Public, <T as SigningTypes>::Signature, Vec<u8>) {
//	const PHRASE: &str = "bottom drive obey lake curtain smoke basket hold race lonely fit walk";
//	let key = <T::AppCrypto as AppCrypto<
//		<T as SigningTypes>::Public,
//		<T as SigningTypes>::Signature,
//	>>::RuntimeAppPublic::generate_pair(Some(
//		format!("{}//Alice", PHRASE).as_bytes().to_vec(),
//	));
//	let key_data = key.to_raw_vec();
//
//	let generic_public = <T::AppCrypto as AppCrypto<
//		<T as SigningTypes>::Public,
//		<T as SigningTypes>::Signature,
//	>>::GenericPublic::from(key);
//	let public: <T as SigningTypes>::Public = generic_public.into();
//	let sig = <T::AppCrypto as AppCrypto<
//		<T as SigningTypes>::Public,
//		<T as SigningTypes>::Signature,
//	>>::sign(&frame_benchmarking::vec![1, 2, 3], public.clone())
//	.unwrap();
//	(public, sig, key_data)
//}
//
//benchmarks! {
//	force_set_is_activated {
//		  let mut is_activated: bool = true;
//		  for i in 0 .. 100 {
//			if i%2 == 0 {
//				  is_activated = true;
//			} else {
//				  is_activated = false;
//			}
//		  }
//	}: {
//		let _ = AppchainPallet::<T>::force_set_is_activated(RawOrigin::Root.into(), is_activated);
//	}
//	verify {
//		assert_eq!(AppchainPallet::<T>::is_activated(), is_activated);
//	}
//
//	force_set_next_set_id {
//		let next_set_id:u32 = 1;
//	}: {
//		let _ = AppchainPallet::<T>::force_set_next_set_id(RawOrigin::Root.into(), next_set_id);
//	}
//	verify {
//		assert_eq!(AppchainPallet::<T>::next_set_id(), next_set_id);
//	}
//
//	force_set_planned_validators {
//		let b in 2 .. 100u32;
//		let mut validators: Vec<(T::AccountId, u128)> = Vec::new();
//		for i in 1..b {
//			validators.push((whitelisted_caller(), 100));
//		}
//	}: {
//		let _ = AppchainPallet::<T>::force_set_planned_validators(RawOrigin::Root.into(), validators);
//	}
//	verify {
//		assert_eq!(AppchainPallet::<T>::planned_validators().len(), (b-1) as usize);
//	}
//
//	force_set_next_notification_id {
//		let next_notification_id:u32 = 1;
//	}: {
//		let ret = AppchainPallet::<T>::force_set_next_notification_id(RawOrigin::Root.into(), next_notification_id);
//		assert!(ret.is_ok());
//	}
//
//	submit_observations {
//		let b in 2 .. 10u32;
//		let mut obs: Vec<Observation<<T as frame_system::Config>::AccountId>> = Vec::new();
//		obs.push(produce_validator_set::<T>(1u32));
//		create_default_asset::<T, pallet_assets::Instance1>(true);
//
//		for i in 1..b {
//			if i%2 == 1 {
//				obs.push(produce_lock_asset_notification::<T>(i, "usdn.testnet".to_string().as_bytes().to_vec()));
//			} else {
//				obs.push(produce_burn_notification::<T>(i));
//			}
//		}
//
//		let (public, signature, key_data) = get_submitter_information::<T>();
//		let payload = ObservationsPayload {
//			public,
//			key_data,
//			block_number: 1u32.into(),
//			observations: obs,
//		};
//
//	}: {
//		let ret = AppchainPallet::<T>::submit_observations(RawOrigin::None.into(), payload, signature);
//		assert!(ret.is_ok());
//	}
//
//	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test, extra = false);
//}
//