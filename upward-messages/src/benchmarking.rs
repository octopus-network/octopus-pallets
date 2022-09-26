#![cfg(feature = "runtime-benchmarks")]

use crate::*;
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_support::traits::OnInitialize;

#[allow(unused_imports)]
use crate::Pallet as OctopusUpwardMessages;

benchmarks! {
	on_initialize {
		let m in 1 .. T::MaxMessagesPerCommit::get() as u32;
		let p in 0 .. T::MaxMessagePayloadSize::get() as u32;

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
			<MessageQueue<T>>::try_append(Message {
				nonce: 0u64,
				payload_type,
				payload: payload.try_into().unwrap(),
			}).unwrap();
		}

		let block_number = Interval::<T>::get();

	}: { OctopusUpwardMessages::<T>::on_initialize(block_number) }
	verify {
		assert_eq!(<MessageQueue<T>>::get().len(), 0);
	}


	on_initialize_non_interval {
			<MessageQueue<T>>::try_append(Message {
				nonce: 0u64,
				payload_type: PayloadType::EraPayout,
				payload: vec![1u8; T::MaxMessagePayloadSize::get() as usize].try_into().unwrap(),
			}).unwrap();

		Interval::<T>::put::<T::BlockNumber>(10u32.into());
		let block_number: T::BlockNumber = 11u32.into();

	}: { OctopusUpwardMessages::<T>::on_initialize(block_number) }
	verify {
		assert_eq!(<MessageQueue<T>>::get().len(), 1);
	}


	on_initialize_no_messages {
		<MessageQueue<T>>::kill();

		let block_number = Interval::<T>::get();

	}: { OctopusUpwardMessages::<T>::on_initialize(block_number) }
}

impl_benchmark_test_suite!(OctopusUpwardMessages, crate::tests::new_tester(), crate::tests::Test,);
