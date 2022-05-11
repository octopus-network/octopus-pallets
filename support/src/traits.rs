use crate::types::Nep171TokenMetadata;
use frame_support::dispatch::DispatchError;
use sp_runtime::KeyTypeId;
use sp_std::prelude::*;

pub trait AppchainInterface {
	fn is_activated() -> bool;

	fn next_set_id() -> u32;
}

/// Something that can provide a set of validators for the next era.
pub trait ValidatorsProvider<AccountId> {
	/// A new set of validators.
	fn validators() -> Vec<(AccountId, u128)>;
}

pub trait AssetIdAndNameProvider<AssetId> {
	type Err;

	fn try_get_asset_id(name: impl AsRef<[u8]>) -> Result<AssetId, Self::Err>;

	fn try_get_asset_name(asset_id: AssetId) -> Result<Vec<u8>, Self::Err>;
}

pub trait LposInterface<AccountId> {
	fn is_active_validator(id: KeyTypeId, key_data: &[u8]) -> Option<AccountId>;

	fn active_stake_of(who: &AccountId) -> u128;

	fn active_total_stake() -> Option<u128>;
}

pub trait UpwardMessagesInterface<AccountId> {
	fn submit(
		who: Option<AccountId>,
		payload_type: crate::types::PayloadType,
		payload: &[u8],
	) -> Result<u64, DispatchError>;
}

pub trait ConvertIntoNep171 {
	type ClassId;
	type InstanceId;
	fn convert_into_nep171_metadata(
		class: Self::ClassId,
		instance: Self::InstanceId,
	) -> Option<Nep171TokenMetadata>;
}

impl ConvertIntoNep171 for () {
	type ClassId = u128;
	type InstanceId = u128;
	fn convert_into_nep171_metadata(
		_class: Self::ClassId,
		_instance: Self::InstanceId,
	) -> Option<Nep171TokenMetadata> {
		None
	}
}

pub trait GetMmrRootHash {
	fn get_mmr_root_hash() -> sp_core::H256;
}
