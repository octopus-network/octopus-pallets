use sp_npos_elections::Supports;
use sp_runtime::KeyTypeId;

pub trait LposInterface<AccountId> {
	fn in_current_validator_set(id: KeyTypeId, key_data: &[u8]) -> Option<AccountId>;

	fn stake_of(who: &AccountId) -> u128;

	fn total_stake() -> u128;
}

pub trait ElectionProvider<AccountId> {
	/// Elect a new set of winners.
	///
	/// The result is returned in a target major format, namely as vector of supports.
	fn elect() -> Supports<AccountId>;
}
