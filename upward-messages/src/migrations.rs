use super::*;
use frame_support::{
	traits::{Get, GetStorageVersion},
	weights::Weight,
};

pub fn migration_to_v1<T: Config>(interval: T::BlockNumber) -> Weight {
	let current = Pallet::<T>::current_storage_version();
	let onchain = Pallet::<T>::on_chain_storage_version();

	log!(
				info,
				"Running migration in octopus-upward-messages pallet with current storage version {:?} / onchain {:?}",
				current,
				onchain
			);

	if current == 1 && onchain == 0 {
		<Interval<T>>::put(interval);
		log!(info, "upward-messages updating to version 1 ");

		current.put::<Pallet<T>>();
		T::DbWeight::get().reads_writes(2, 2)
	} else {
		log!(
					info,
					"The storageVersion of upward-messages is already the matching version, and the migration is not repeated."
				);
		T::DbWeight::get().reads(1)
	}
}
