use super::*;
use frame_support::{migration::storage_key_iter, Twox64Concat};

impl<T: Config> Pallet<T> {
	pub(crate) fn migration_to_v1() -> u64 {
		let mut translated = 0;

		for (token_id, asset_id) in storage_key_iter::<Vec<u8>, T::AssetId, Twox64Concat>(
			b"OctopusAppchain",
			b"AssetIdByTokenId",
		)
		.drain()
		{
			log!(info, "Set ( key: {:?}, value: {:?} ) in AssetIdByTokenId!", &token_id, &asset_id);
			AssetIdByTokenId::<T>::insert(token_id, asset_id);
			translated += 1u64;
		}

		translated
	}
}
