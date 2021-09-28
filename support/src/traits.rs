use frame_support::dispatch::DispatchResult;
use sp_runtime::KeyTypeId;
use sp_std::prelude::*;

pub trait LposInterface<AccountId> {
	fn is_active_validator(id: KeyTypeId, key_data: &[u8]) -> Option<AccountId>;

	fn active_stake_of(who: &AccountId) -> u128;

	fn active_total_stake() -> Option<u128>;
}

/// Something that can provide a set of validators for the next era.
pub trait ValidatorsProvider<AccountId> {
	/// A new set of validators.
	fn validators() -> Vec<(AccountId, u128)>;
}

pub trait UpwardMessagesInterface<AccountId> {
	fn submit(
		who: &AccountId,
		payload_type: crate::types::PayloadType,
		payload: &[u8],
	) -> DispatchResult;
}
