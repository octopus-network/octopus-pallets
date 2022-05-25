use super::*;
use frame_support::{
	traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
	weights::Weight,
};

pub mod v1 {
	use super::*;

	pub struct MigrateToV1<T>(sp_std::marker::PhantomData<T>);
	impl<T: Config> OnRuntimeUpgrade for MigrateToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let current = Pallet::<T>::current_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			log!(
				info,
				"Running migration in octopus appchain pallet with current storage version {:?} / onchain {:?}",
				current,
				onchain
			);

			if current == 1 && onchain == 0 {
				let translated = 1u64;
				let account = <Pallet<T>>::account_id();
				OctopusPalletId::<T>::put(Some(account));

				log!(info, "updating to version 1 ",);

				current.put::<Pallet<T>>();
				T::DbWeight::get().reads_writes(translated + 1, translated + 1)
			} else {
				log!(
					info,
					"MigrateToV1 being executed on the wrong storage version, expected V0_0_0"
				);
				T::DbWeight::get().reads(1)
			}
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade() -> Result<(), &'static str> {
			assert_eq!(Pallet::<T>::on_chain_storage_version(), 1);
			Ok(())
		}
	}
}
