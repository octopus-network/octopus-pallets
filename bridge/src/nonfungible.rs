use crate::utils::hex_format;

use super::*;

impl<T: Config> Pallet<T> {
	// nft cross chain transfer:
	// mainchain:mint() <- appchain:lock_nft()
	// mainchain:burn() -> appchain:unlock_nft()
	pub(crate) fn do_lock_nonfungible(
		collection: T::CollectionId,
		item: T::ItemId,
		sender: T::AccountId,
		receiver_id: Vec<u8>,
		fee: BalanceOf<T>,
	) -> DispatchResult {
		let receiver_id =
			String::from_utf8(receiver_id).map_err(|_| Error::<T>::InvalidReceiverId)?;

		let metadata = T::Convertor::convert_into_nep171_metadata(collection, item)
			.ok_or::<Error<T>>(Error::<T>::ConvertorNotImplement.into())?;

		let fee_wrapped: u128 = fee.checked_into().ok_or(Error::<T>::AmountOverflow)?;

		<T::Nonfungibles as nonfungibles::Transfer<T::AccountId>>::transfer(
			&collection,
			&item,
			&Self::account_id(),
		)?;

		let hex_sender = hex_format(&sender.encode());
		let message = LockNftPayload {
			sender: hex_sender.clone(),
			receiver_id: receiver_id.clone(),
			collection: collection.into(),
			item: item.into(),
			metadata,
			fee: fee_wrapped,
		};

		let sequence = T::UpwardMessagesInterface::submit(
			Some(sender.clone()),
			PayloadType::LockNft,
			&message.try_to_vec().unwrap(),
		)?;
		Self::deposit_event(Event::NonfungibleLocked {
			collection,
			item,
			sender,
			receiver: receiver_id.as_bytes().to_vec(),
			fee,
			sequence,
		});

		Ok(())
	}

	pub(crate) fn do_unlock_nonfungible(
		collection: u128,
		item: u128,
		sender_id: Vec<u8>,
		receiver: T::AccountId,
		sequence: u32,
	) -> DispatchResult {
		let collection = collection.checked_into().ok_or(Error::<T>::CollectionOverflow)?;
		let item = item.checked_into().ok_or(Error::<T>::ItemOverflow)?;
		<T::Nonfungibles as nonfungibles::Transfer<T::AccountId>>::transfer(
			&collection,
			&item,
			&receiver,
		)?;

		Self::deposit_event(Event::NonfungibleUnlocked {
			collection,
			item,
			sender: sender_id,
			receiver,
			sequence,
		});

		Ok(())
	}
}
