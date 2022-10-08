use super::*;

impl<T: Config> Pallet<T> {
	/// Emits `Locked` event when successful.
	pub(crate) fn do_lock(
		sender: T::AccountId,
		receiver_id: Vec<u8>,
		amount: BalanceOf<T>,
		fee: BalanceOf<T>,
	) -> DispatchResult {
		let receiver_id =
			String::from_utf8(receiver_id).map_err(|_| Error::<T>::InvalidReceiverId)?;

		let amount_wrapped: u128 = amount.checked_into().ok_or(Error::<T>::AmountOverflow)?;
		let fee_wrapped: u128 = fee.checked_into().ok_or(Error::<T>::AmountOverflow)?;

		T::Currency::transfer(&sender, &Self::account_id(), amount, AllowDeath)?;

		let prefix = String::from("0x");
		let hex_sender = prefix + &hex::encode(sender.encode());
		let message = LockPayload {
			sender: hex_sender.clone(),
			receiver_id: receiver_id.clone(),
			amount: amount_wrapped,
			fee: fee_wrapped,
		};

		let sequence = T::UpwardMessagesInterface::submit(
			Some(sender.clone()),
			PayloadType::Lock,
			&message.try_to_vec().unwrap(),
		)?;
		Self::deposit_event(Event::Locked {
			sender,
			receiver: receiver_id.as_bytes().to_vec(),
			amount,
			fee,
			sequence,
		});

		Ok(())
	}

	/// Emits `Unlocked` event when successful.
	pub(crate) fn do_unlock(
		sender_id: Vec<u8>,
		receiver: T::AccountId,
		amount: u128,
		sequence: u32,
	) -> DispatchResult {
		let amount_unwrapped = amount.checked_into().ok_or(Error::<T>::AmountOverflow)?;
		// unlock native token
		T::Currency::transfer(&Self::account_id(), &receiver, amount_unwrapped, KeepAlive)?;
		Self::deposit_event(Event::Unlocked {
			sender: sender_id,
			receiver,
			amount: amount_unwrapped,
			sequence,
		});

		Ok(())
	}
}
