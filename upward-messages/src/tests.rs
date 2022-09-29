use super::*;
use frame_support::{
	assert_noop, assert_ok,
	pallet_prelude::GenesisBuild,
	parameter_types,
	traits::{ConstU32, ConstU64},
};

use sp_core::H256;
use sp_keyring::AccountKeyring;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentifyAccount, IdentityLookup, Keccak256, Verify},
	MultiSignature,
};

use pallet_octopus_support::types::PayloadType;

use crate as pallet_octopus_upward_messages;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system,
		OctopusUpwardMessages: pallet_octopus_upward_messages,
	}
);

pub type Signature = MultiSignature;
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type Origin = Origin;
	type Call = Call;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type Event = Event;
	type BlockHashCount = ConstU64<250>;
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
}

parameter_types! {
	pub const MaxMessagePayloadSize: u32 = 100;
	pub const MaxMessagesPerCommit: u32 = 20;
}

impl Config for Test {
	type Event = Event;
	type Hashing = Keccak256;
	type MaxMessagePayloadSize = MaxMessagePayloadSize;
	type MaxMessagesPerCommit = MaxMessagesPerCommit;
	type WeightInfo = ();
}

pub fn new_tester() -> sp_io::TestExternalities {
	let mut storage = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();
	let config: pallet_octopus_upward_messages::GenesisConfig<Test> =
		pallet_octopus_upward_messages::GenesisConfig { interval: 1u32.into() };
	config.assimilate_storage(&mut storage).unwrap();

	let mut ext: sp_io::TestExternalities = storage.into();

	ext.execute_with(|| System::set_block_number(1));
	ext
}

#[test]
fn test_submit() {
	new_tester().execute_with(|| {
		let who: AccountId = AccountKeyring::Alice.into();
		assert_ok!(OctopusUpwardMessages::submit(
			Some(who.clone()),
			PayloadType::Lock,
			&vec![0, 1, 2]
		));
		assert_eq!(<Nonce<Test>>::get(), 1);
		assert_ok!(OctopusUpwardMessages::submit(
			Some(who),
			PayloadType::BurnAsset,
			&vec![0, 1, 2]
		));
		assert_eq!(<Nonce<Test>>::get(), 2);
	});
}

#[test]
fn test_submit_exceeds_queue_limit() {
	new_tester().execute_with(|| {
		let who: AccountId = AccountKeyring::Bob.into();

		let messages_limit = MaxMessagesPerCommit::get();
		(0..messages_limit).for_each(|_| {
			OctopusUpwardMessages::submit(Some(who.clone()), PayloadType::Lock, &vec![0, 1, 2])
				.unwrap();
		});

		assert_noop!(
			OctopusUpwardMessages::submit(Some(who), PayloadType::BurnAsset, &vec![0, 1, 2]),
			Error::<Test>::QueueSizeLimitReached,
		);
	})
}

#[test]
fn test_submit_exceeds_payload_limit() {
	new_tester().execute_with(|| {
		let who: AccountId = AccountKeyring::Bob.into();

		let max_payload_bytes = MaxMessagePayloadSize::get();
		let payload: Vec<u8> = (0..).take(max_payload_bytes as usize + 1).collect();

		assert_noop!(
			OctopusUpwardMessages::submit(Some(who), PayloadType::BurnAsset, payload.as_slice()),
			Error::<Test>::PayloadTooLarge,
		);
	})
}

#[test]
fn test_submit_fails_on_nonce_overflow() {
	new_tester().execute_with(|| {
		let who: AccountId = AccountKeyring::Bob.into();

		<Nonce<Test>>::set(u64::MAX);
		assert_noop!(
			OctopusUpwardMessages::submit(Some(who), PayloadType::Lock, &vec![0, 1, 2]),
			Error::<Test>::NonceOverflow,
		);
	});
}
