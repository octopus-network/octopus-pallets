use frame_support::dispatch::DispatchResult;
use sp_npos_elections::Supports;
use sp_runtime::{KeyTypeId, Perbill};

/// Means for interacting with a specialized version of the `session` trait.
pub trait SessionInterface<AccountId>: frame_system::Config {
	fn same_validator(id: KeyTypeId, key_data: &[u8], validator: AccountId) -> bool;
}

pub trait LposInterface<AccountId> {
	fn bond_and_validate(
		controller: AccountId,
		value: u128,
		commission: Perbill,
		blocked: bool,
	) -> DispatchResult;
}

pub trait ElectionProvider<AccountId> {
	/// Elect a new set of winners.
	///
	/// The result is returned in a target major format, namely as vector of supports.
	fn elect() -> Supports<AccountId>;
}
