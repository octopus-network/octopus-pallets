use super::*;

impl<T: Config> Pallet<T> {
	pub(crate) fn do_lock(
		sender: T::AccountId,
		amount: BalanceOf<T>,
		r_id: ResourceId,
		recipient: Vec<u8>,
		dest_id: bridge::ChainId,
	) -> DispatchResult {
		log::info!("transfer native token");
		let bridge_id = <bridge::Pallet<T>>::account_id();

		<T as Config>::Currency::transfer(&sender, &bridge_id, amount.into(), AllowDeath)?;

		log::info!("transfer native token successful");
		<bridge::Pallet<T>>::transfer_fungible(
			dest_id,
			r_id,
			recipient.clone(),
			U256::from(amount.saturated_into::<u128>()),
		)?;

		Ok(())
	}

	pub(crate) fn do_unlock(
		sender: T::AccountId,
		to: T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		<T as Config>::Currency::transfer(&sender, &to, amount.into(), AllowDeath)?;

		Ok(())
	}
}
