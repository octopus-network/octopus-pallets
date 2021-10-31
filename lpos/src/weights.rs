use sp_std::marker::PhantomData;

/// Weight functions needed for pallet_octopus_lpos.
pub trait WeightInfo {}

/// Weights for pallet_octopus_lpos using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {}

// For backwards compatibility and tests
impl WeightInfo for () {}
