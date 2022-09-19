use super::*;

pub trait AppchainInterface<AccountId> {
	fn is_activated() -> bool;

	fn next_set_id() -> u32;

	/// A new set of validators.
	fn planned_validators() -> Vec<(AccountId, u128)>;
}

pub trait BridgeInterface<AccountId> {
	fn unlock(
		sender_id: Vec<u8>,
		receiver: AccountId,
		amount: u128,
		sequence: u32,
	) -> DispatchResult;

	fn mint_nep141(
		token_id: Vec<u8>,
		sender_id: Vec<u8>,
		receiver: AccountId,
		amount: u128,
		sequence: u32,
	) -> DispatchResult;

	fn unlock_nonfungible(
		collection: u128,
		item: u128,
		sender_id: Vec<u8>,
		receiver: AccountId,
		sequence: u32,
	) -> DispatchResult;
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

pub trait TokenIdAndAssetIdProvider<AssetId> {
	type Err;

	fn try_get_asset_id(token_id: impl AsRef<[u8]>) -> Result<AssetId, Self::Err>;

	fn try_get_token_id(asset_id: AssetId) -> Result<Vec<u8>, Self::Err>;
}

pub trait ConvertIntoNep171 {
	type CollectionId;
	type ItemId;
	fn convert_into_nep171_metadata(
		collection: Self::CollectionId,
		item: Self::ItemId,
	) -> Option<Nep171TokenMetadata>;
}

impl ConvertIntoNep171 for () {
	type CollectionId = u128;
	type ItemId = u128;
	fn convert_into_nep171_metadata(
		_collection: Self::CollectionId,
		_item: Self::ItemId,
	) -> Option<Nep171TokenMetadata> {
		None
	}
}
