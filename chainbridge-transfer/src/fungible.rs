use super::*;
use frame_support::traits::fungibles::Mutate;

impl<T: Config> Pallet<T> {
	pub(crate) fn do_burn_assets(
		who: T::AccountId,
		amount: BalanceOf<T>,
		r_id: ResourceId,
		recipient: Vec<u8>,
		dest_id: bridge::ChainId,
	) -> DispatchResult {
		let amount = amount.saturated_into::<u128>();

		let token_id = Self::try_get_asset_id(r_id)?;

		<T::Fungibles as Mutate<T::AccountId>>::burn_from(token_id, &who, amount.into())?;
		<bridge::Pallet<T>>::transfer_fungible(
			dest_id,
			r_id,
			recipient.clone(),
			U256::from(amount.saturated_into::<u128>()),
		)?;

		Ok(())
	}

	pub(crate) fn do_mint_assets(
		who: T::AccountId,
		amount: BalanceOf<T>,
		r_id: ResourceId,
	) -> DispatchResult {
		let amount = amount.saturated_into::<u128>();
		let token_id = Self::try_get_asset_id(r_id)?;
		<T::Fungibles as Mutate<T::AccountId>>::mint_into(token_id, &who, amount.into())?;

		Ok(())
	}
}
