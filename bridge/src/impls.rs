use super::*;

pub struct UnImplementUniques<T>(sp_std::marker::PhantomData<T>);

impl<T, AccountId> nonfungibles::Inspect<AccountId> for UnImplementUniques<T>
where
	T: Config,
{
	/// Type for identifying an item.
	type ItemId = T::ItemId;

	/// Type for identifying a collection (an identifier for an independent collection of
	/// items).
	type CollectionId = T::CollectionId;

	/// Returns the owner of `item` of `collection`, or `None` if the item doesn't exist
	/// (or somehow has no owner).
	fn owner(_collection: &Self::CollectionId, _item: &Self::ItemId) -> Option<AccountId> {
		None
	}
}

impl<T, AccountId> nonfungibles::Transfer<AccountId> for UnImplementUniques<T>
where
	T: Config,
{
	/// Transfer `item` of `collection` into `destination` account.
	fn transfer(
		_collection: &Self::CollectionId,
		_item: &Self::ItemId,
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
		_mint: bool,
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
	type CollectionId = <T as Config>::CollectionId;
	type ItemId = <T as Config>::ItemId;

	fn convert_into_nep171_metadata(
		collection: Self::CollectionId,
		item: Self::ItemId,
	) -> Option<Nep171TokenMetadata> {
		let mut data: Vec<u8> = Vec::new();
		if let Some(collection_attribute) = <T::Nonfungibles as nonfungibles::Inspect<
			T::AccountId,
		>>::collection_attribute(&collection, &vec![])
		{
			data.extend(collection_attribute);
		}
		if let Some(attribute) = <T::Nonfungibles as nonfungibles::Inspect<T::AccountId>>::attribute(
			&collection,
			&item,
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

// For the definition of base metadata, please refer to the following document:
// 		https://github.com/rmrk-team/rmrk-spec/blob/master/standards/rmrk2.0.0/entities/metadata.md#schema-definition
#[derive(Deserialize, RuntimeDebug)]
struct RmrkBaseMetadata {
	// NFT name, required
	name: String,
	// General notes, abstracts, or summaries about the contents of an NFT.
	#[serde(default)]
	description: String,
	// A broad definition of the type of content of the NFT.
	#[serde(default)]
	types: String,
	// Metadata locale in ISO 639-1 format. For translations and localisation. e.g. en-GB, en, fr
	#[serde(default)]
	locale: String,
	// A statement about the NFT license.
	#[serde(default)]
	license: String,
	// A URI to a statement of license.
	#[serde(default)]
	#[serde(rename = "licenseUri")]
	license_uri: String,
	// A URI to a main media file of the NFT.
	#[serde(default)]
	#[serde(rename = "mediaUri")]
	media_uri: String,
	// A URI to an image of the NFT for wallets and client applications to have a scaled down image
	// to present to end-users.
	#[serde(default)]
	#[serde(rename = "thumbnailUri")]
	thumbnail_uri: String,
	// A URI with additional information about the subject or content of the NFT.
	#[serde(default)]
	#[serde(rename = "externalUri")]
	external_uri: String,
	// Custom attributes about the subject or content of the asset.
	// properties (object)
}

pub struct RmrkBaseMetadataConvertor<T>(sp_std::marker::PhantomData<T>);
impl<T> ConvertIntoNep171 for RmrkBaseMetadataConvertor<T>
where
	T: Config,
{
	type CollectionId = <T as Config>::CollectionId;
	type ItemId = <T as Config>::ItemId;

	fn convert_into_nep171_metadata(
		collection: Self::CollectionId,
		item: Self::ItemId,
	) -> Option<Nep171TokenMetadata> {
		let mut data: Vec<u8> = Vec::new();
		if let Some(attribute) = <T::Nonfungibles as nonfungibles::Inspect<T::AccountId>>::attribute(
			&collection,
			&item,
			&vec![],
		) {
			data.extend(attribute);
		}

		if data.is_empty() {
			return None
		}

		// parse vec to rmrk base metadata
		let rmrk_metadata: RmrkBaseMetadata = match serde_json::from_slice(&data) {
			Ok(metadata) => metadata,
			Err(_) => {
				log!(warn, "data : {:?}", data);
				log!(warn, "Failed to parse data to rmrk base metadata");
				return None
			},
		};
		log!(debug, "rmrk metadata is : {:?}", rmrk_metadata);

		// Need Check:
		// 		Can the name field be empty?
		let title = (rmrk_metadata.name.len() != 0).then_some(rmrk_metadata.name);
		let description =
			(rmrk_metadata.description.len() != 0).then_some(rmrk_metadata.description);
		let media_uri = (rmrk_metadata.media_uri.len() != 0).then_some(rmrk_metadata.media_uri);

		let extra = json!({
			"types": rmrk_metadata.types,
			"locale": rmrk_metadata.locale,
			"license": rmrk_metadata.license,
			"licenseUri": rmrk_metadata.license_uri,
			"thumbnailUri": rmrk_metadata.thumbnail_uri,
			"externalUri": rmrk_metadata.external_uri,
		});

		// parse rmrk base metadata to nep171 format
		let metadata = Nep171TokenMetadata {
			title,
			description,
			media: media_uri,
			media_hash: None,
			copies: None,
			issued_at: None,
			expires_at: None,
			starts_at: None,
			updated_at: None,
			extra: Some(extra.to_string()),
			reference: None,
			reference_hash: None,
		};
		log!(debug, "After, the Nep171 media data is {:?} ", metadata.clone());

		Some(metadata)
	}
}

impl<T: Config> BridgeInterface<<T as frame_system::Config>::AccountId> for Pallet<T> {
	fn unlock(
		sender_id: Vec<u8>,
		receiver: T::AccountId,
		amount: u128,
		sequence: u32,
	) -> DispatchResult {
		Self::do_unlock(sender_id, receiver, amount, sequence)
	}

	fn mint_nep141(
		token_id: Vec<u8>,
		sender_id: Vec<u8>,
		receiver: T::AccountId,
		amount: u128,
		sequence: u32,
	) -> DispatchResult {
		Self::do_mint_nep141(token_id, sender_id, receiver, amount, sequence)
	}

	fn unlock_nonfungible(
		collection: u128,
		item: u128,
		sender_id: Vec<u8>,
		receiver: T::AccountId,
		sequence: u32,
	) -> DispatchResult {
		Self::do_unlock_nonfungible(collection, item, sender_id, receiver, sequence)
	}
}
