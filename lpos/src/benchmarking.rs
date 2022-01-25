#![cfg(feature = "runtime-benchmarks")]

use crate::*;
use frame_benchmarking::benchmarks;
use frame_system::RawOrigin;

#[allow(unused_imports)]
use crate::Pallet as OctopusLpos;

benchmarks! {
	set_history_depth {
		let b in 0..u32::MAX;
		let m in 0..u32::MAX;

		CurrentEra::<T>::put(1);
	}: set_history_depth(RawOrigin::Root, b, m)

	force_set_era_payout {
	}: force_set_era_payout(RawOrigin::Root, u128::from(u32::MAX))

	impl_benchmark_test_suite!(OctopusLpos, crate::mock::new_tester(), crate::mock::Test, );
}
