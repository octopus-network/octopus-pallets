use super::*;
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_system::RawOrigin;
// use tests::*;
// use rand::Rng;

#[allow(unused_imports)]
use crate::Pallet as OctopusAppchain;

fn get_public<T: Config>() -> (T::Public, T::Signature) {
	let key = <T::AuthorityId as AppCrypto<
		<T as SigningTypes>::Public,
		<T as SigningTypes>::Signature,
	>>::RuntimeAppPublic::generate_pair(Some("alice".to_string().as_bytes().to_vec()));

	let generic_public = <T::AuthorityId as AppCrypto<
		<T as SigningTypes>::Public,
		<T as SigningTypes>::Signature,
	>>::GenericPublic::from(key);
	let public: <T as SigningTypes>::Public = generic_public.into();
	let sig = <T::AuthorityId as AppCrypto<
		<T as SigningTypes>::Public,
		<T as SigningTypes>::Signature,
	>>::sign(&vec![1, 2, 3], public.clone()).unwrap();
	(public, sig)
}

pub fn expected_burn_notify<T: Config>() -> Observation<T::AccountId> {
	let receiver = hex::decode("94f135526ec5fe830e0cbc6fd58683cb2d9ee06522cd9a2c0481268c5c73674f")
		.map(|b| T::AccountId::decode(&mut &b[..]))
		.unwrap()
		.unwrap();

	Observation::Burn(BurnEvent {
		index: 0,
		sender_id: "andy-pallet-test.testnet".to_string().as_bytes().to_vec(),
		receiver,
		amount: 100000,
	})
}

fn create_payload_and_signature<T: Config>(
) -> (ObservationsPayload<T::Public, T::BlockNumber, T::AccountId>, T::Signature) {
	let (public, msig) = get_public::<T>();

	let obs_payload = ObservationsPayload {
		public,
		block_number: 2u32.into(),
		observations: vec![expected_burn_notify::<T>()],
	};

	(obs_payload, msig)
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
	}: force_set_is_activated(RawOrigin::Root, is_activated)


	force_set_next_set_id {
	  let b in 2 .. 100000u32;
	  let mut next_set_id:u32 = 1;
	  for i in 2 .. b{
		next_set_id = i;
	  }
	}: force_set_next_set_id(RawOrigin::Root, next_set_id)


	force_set_planned_validators {
	  let b in 2 .. 33u32;
	  let mut validators: Vec<(T::AccountId, u128)> = Vec::new();
	  for i in 1..b {
		// let mut rng = rand::thread_rng();
		// let x: u128 = rng.gen();
		// validators.push((whitelisted_caller(), x));
		validators.push((whitelisted_caller(), 100));
	  }
	}: force_set_planned_validators(RawOrigin::Root, validators)

	lock {
	  let account = OctopusAppchain::<T>::pallet_account();
	  let pallet_account = RawOrigin::Signed(account);
	  let min = T::Currency::minimum_balance();
	  <IsActivated<T>>::put(true);
	}: lock(pallet_account, "test-account.testnet".to_string().as_bytes().to_vec(), min)

	// //Note: need to config for this pallet first.
	// submit_observations {
	//   let (obs_payload, msig) = create_payload_and_signature::<T>();
	// }: submit_observations(RawOrigin::None, obs_payload, msig)

	// TODO:
	// mint_asset {
	//   let caller: T::AccountId = whitelisted_caller();
	//   let receiver = T::Lookup::unlookup(caller.clone());
	//   T::Currency::make_free_balance_be(&caller, T::Currency::minimum_balance());
	// }: mint_asset(
	//     RawOrigin::Root,
	//     0,
	//     "test-account.testnet".to_string().as_bytes().to_vec(),
	//     receiver,
	//     100000 )

	//TODO: burn_asset

	impl_benchmark_test_suite!(OctopusAppchain, crate::mock::new_tester(), crate::mock::Test, );
}
