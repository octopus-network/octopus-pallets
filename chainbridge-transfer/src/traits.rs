use crate::ResourceId;

/// A trait handling asset ID and name
pub trait AssetIdResourceIdProvider<AssetId> {
	type Err;

	fn try_get_asset_id(resource_id: ResourceId) -> Result<AssetId, Self::Err>;

	fn try_get_asset_name(asset_id: AssetId) -> Result<ResourceId, Self::Err>;
}
