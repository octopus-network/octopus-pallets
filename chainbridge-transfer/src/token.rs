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

		if <NativeCheck<T>>::get() {
			let free_balance = <T as Config>::Currency::free_balance(&bridge_id);
			let total_balance = free_balance + amount;

			let right_balance = T::NativeTokenMaxValue::get() / 3u8.into();
			if total_balance > right_balance {
				return Err(Error::<T>::OverTransferLimit)?;
			}
		}

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
