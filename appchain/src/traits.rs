use sp_npos_elections::Supports;
use sp_runtime::KeyTypeId;

pub trait LposInterface<AccountId> {
	fn in_current_validator_set(id: KeyTypeId, key_data: &[u8]) -> Option<AccountId>;

	fn stake_of(who: &AccountId) -> u128;

	fn total_stake() -> u128;
}

/// Something that can provide a set of stakers for the next era.
pub trait StakersProvider<AccountId> {
	/// A new set of stakers.
	///
	/// The result is returned in a vector of supports.
	fn stakers() -> Supports<AccountId>;
}
