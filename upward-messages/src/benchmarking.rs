#![cfg(feature = "runtime-benchmarks")]

use crate::*;
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_support::traits::OnInitialize;

#[allow(unused_imports)]
use crate::Pallet as OctopusUpwardMessages;

benchmarks! {
	on_initialize {
		let m in 1 .. T::UpwardMessagesLimit::get() as u32;
		// Note: The message with the largest length may be erapayoutpayload,
		// so the maximum possible length may be:
		//      len(end_era) + len(excluded_validators) = 32 + 32 * 33 = 1088.
		// However, the test found that if the value exceeds 255, an error will be reported.
		let p in 0 .. 255u32;

		for i in 0 .. m {
			let payload: Vec<u8> = (0..).take(p as usize).collect();
			let payload_type;
			if i%4 == 0 {
				payload_type = PayloadType::Lock;
			} else if i%4 == 1 {
				payload_type = PayloadType::BurnAsset;
			} else if i%4 == 2{
				payload_type = PayloadType::PlanNewEra;
			} else {
				payload_type = PayloadType::EraPayout;
			}
			<MessageQueue<T>>::append(Message {
				nonce: 0u64,
				payload_type,
				payload,
			});
		}

		let block_number: T::BlockNumber = 1u32.into();

	}: { OctopusUpwardMessages::<T>::on_initialize(block_number) }
	verify {
		assert_eq!(<MessageQueue<T>>::get().len(), 0);
	}
}

impl_benchmark_test_suite!(OctopusUpwardMessages, crate::tests::new_tester(), crate::tests::Test,);
