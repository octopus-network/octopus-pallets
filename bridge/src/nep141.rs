use super::*;

impl<T: Config> Pallet<T> {
	pub(crate) fn do_burn_nep141(
		asset_id: T::AssetId,
		sender: T::AccountId,
		receiver_id: Vec<u8>,
		amount: T::AssetBalance,
		fee: BalanceOf<T>,
	) -> DispatchResult {
		let receiver_id =
			String::from_utf8(receiver_id).map_err(|_| Error::<T>::InvalidReceiverId)?;

		let token_id =
			T::AssetIdByTokenId::try_get_token_id(asset_id).map_err(|_| Error::<T>::NoAssetId)?;

		let token_id = String::from_utf8(token_id).map_err(|_| Error::<T>::InvalidTokenId)?;

		let fee_wrapped: u128 = fee.checked_into().ok_or(Error::<T>::AmountOverflow)?;

		<T::Fungibles as fungibles::Mutate<T::AccountId>>::burn_from(asset_id, &sender, amount)?;

		let prefix = String::from("0x");
		let hex_sender = prefix + &hex::encode(sender.encode());
		let message = BurnAssetPayload {
			token_id,
			sender: hex_sender,
			receiver_id: receiver_id.clone(),
			amount: amount.into(),
			fee: fee_wrapped,
		};

		let sequence = T::UpwardMessagesInterface::submit(
			Some(sender.clone()),
			PayloadType::BurnAsset,
			&message.try_to_vec().unwrap(),
		)?;
		Self::deposit_event(Event::Nep141Burned {
			asset_id,
			sender,
			receiver: receiver_id.as_bytes().to_vec(),
			amount,
			fee,
			sequence,
		});

		Ok(())
	}

	pub(crate) fn do_mint_nep141(
		token_id: Vec<u8>,
		sender_id: Vec<u8>,
		receiver: T::AccountId,
		amount: u128,
		sequence: u32,
	) -> DispatchResult {
		let asset_id =
			T::AssetIdByTokenId::try_get_asset_id(token_id).map_err(|_| Error::<T>::NoTokenId)?;

		let amount_unwrapped = amount.checked_into().ok_or(Error::<T>::AmountOverflow)?;
		<T::Fungibles as fungibles::Mutate<T::AccountId>>::mint_into(
			asset_id,
			&receiver,
			amount_unwrapped,
		)?;
		Self::deposit_event(Event::Nep141Minted {
			asset_id,
			sender: sender_id,
			receiver,
			amount: amount_unwrapped,
			sequence,
		});

		Ok(())
	}
}
