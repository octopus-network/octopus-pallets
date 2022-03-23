use super::*;
use frame_support::traits::tokens::{
	fungibles, nonfungibles, DepositConsequence, WithdrawConsequence,
};
use sp_runtime::{DispatchError, DispatchResult};

pub struct UnImplementUniques<T>(sp_std::marker::PhantomData<T>);

impl<T, AccountId> nonfungibles::Inspect<AccountId> for UnImplementUniques<T>
where
	T: Config,
{
	type InstanceId = T::InstanceId;
	type ClassId = T::ClassId;

	fn owner(_class: &Self::ClassId, _instance: &Self::InstanceId) -> Option<AccountId> {
		None
	}
}

impl<T, AccountId> nonfungibles::Transfer<AccountId> for UnImplementUniques<T>
where
	T: Config,
{
	fn transfer(
		_class: &Self::ClassId,
		_instance: &Self::InstanceId,
		_destination: &AccountId,
	) -> DispatchResult {
		log!(debug, "Should not go there for every: not impl trait nonfungibles::Transfer.");
		Err(sp_runtime::DispatchError::Other("NoUniquesImpl"))
	}
}

pub struct UnImplementAssets<T>(sp_std::marker::PhantomData<T>);

impl<T, AccountId> fungibles::Inspect<AccountId> for UnImplementAssets<T>
where
	T: Config,
{
	type AssetId = T::AssetId;
	type Balance = T::AssetBalance;

	fn total_issuance(_asset: Self::AssetId) -> Self::Balance {
		0u32.into()
	}

	fn minimum_balance(_asset: Self::AssetId) -> Self::Balance {
		0u32.into()
	}

	fn balance(_asset: Self::AssetId, _who: &AccountId) -> Self::Balance {
		0u32.into()
	}

	fn reducible_balance(
		_asset: Self::AssetId,
		_who: &AccountId,
		_keep_alive: bool,
	) -> Self::Balance {
		0u32.into()
	}

	fn can_deposit(
		_asset: Self::AssetId,
		_who: &AccountId,
		_amount: Self::Balance,
	) -> DepositConsequence {
		DepositConsequence::CannotCreate
	}

	fn can_withdraw(
		_asset: Self::AssetId,
		_who: &AccountId,
		_amount: Self::Balance,
	) -> WithdrawConsequence<Self::Balance> {
		WithdrawConsequence::UnknownAsset
	}
}

impl<T, AccountId> fungibles::Mutate<AccountId> for UnImplementAssets<T>
where
	T: Config,
{
	fn mint_into(
		_asset: Self::AssetId,
		_who: &AccountId,
		_amount: Self::Balance,
	) -> DispatchResult {
		log!(debug, "Should not go there for every: not impl trait fungibles::Mutate.");
		Err(sp_runtime::DispatchError::Other("NoAssetsImpl"))
	}

	fn burn_from(
		_asset: Self::AssetId,
		_who: &AccountId,
		_amount: Self::Balance,
	) -> Result<Self::Balance, DispatchError> {
		log!(debug, "Should not go there for every: not impl trait fungibles::Mutate.");
		Err(sp_runtime::DispatchError::Other("NoAssetsImpl"))
	}
}

pub struct ExampleConvertor<T>(sp_std::marker::PhantomData<T>);
impl<T> ConvertIntoNep171 for ExampleConvertor<T>
where
	T: Config,
{
	type ClassId = <T as Config>::ClassId;
	type InstanceId = <T as Config>::InstanceId;

	fn convert_into_nep171_metadata(
		class: Self::ClassId,
		instance: Self::InstanceId,
	) -> Option<Nep171TokenMetadata> {
		let mut data: Vec<u8> = Vec::new();
		if let Some(class_attribute) =
			<T::Uniques as nonfungibles::Inspect<T::AccountId>>::class_attribute(&class, &vec![])
		{
			data.extend(class_attribute);
		}
		if let Some(attribute) = <T::Uniques as nonfungibles::Inspect<T::AccountId>>::attribute(
			&class,
			&instance,
			&vec![],
		) {
			data.extend(attribute);
		}

		if data.is_empty() {
			data.extend("example hash".to_string().as_bytes().to_vec());
		}

		let metadata = Nep171TokenMetadata {
			title: Some("example nft title".to_string()),
			description: Some("example nft description".to_string()),
			media: Some("example nft media".to_string()),
			media_hash: Some(data),
			copies: None,
			issued_at: None,
			expires_at: None,
			starts_at: None,
			updated_at: None,
			extra: None,
			reference: None,
			reference_hash: None,
		};

		Some(metadata)
	}
}
