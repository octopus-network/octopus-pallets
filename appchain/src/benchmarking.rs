use super::*;
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_system::RawOrigin;
use tests::*;
// use rand::Rng;

#[allow(unused_imports)]
use crate::Pallet as OctopusAppchain;

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
	  let mut next_set_id:u32 = 1;
	  for i in 2 .. 100000 {
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

    // TODO:
    submit_observations {
	//   let keyring = AccountKeyring::Alice;
	  let (obs_payload, msig) = mock_payload_for_alice();
	//   let validators =
	//     vec![
	// 		(AccountKeyring::Alice.into(), 100 * 1_000_000_000_000_000_000), 
	// 		(AccountKeyring::Bob.into(), 100 * 1_000_000_000_000_000_000)
	// 	];

	//   OctopusLpos::trigger_new_era(1, validators.clone());
	// 	<ActiveEra<T>>::put(ActiveEraInfo{
	// 		index: 1,
	// 		start:None,
	// 	});
	// 	<ErasTotalStake<T>>::insert(1, 0);
    }: {OctopusAppchain::submit_observations(
			RawOrigin::None,
			obs_payload.clone(),
			msig.clone()
		)}
    
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
