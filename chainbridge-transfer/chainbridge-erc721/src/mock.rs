use super::*;
use crate as chainbridge_erc721;
use sp_runtime::{
	generic,
	traits::{AccountIdLookup, BlakeTwo256, IdentifyAccount, Verify},
	MultiSignature,
};

pub use frame_support::{
	construct_runtime,
	pallet_prelude::GenesisBuild,
	parameter_types,
	traits::{
		ConstU128, ConstU32, Hooks, KeyOwnerProofSystem, OnFinalize, OnInitialize, Randomness,
		StorageInfo,
	},
	weights::{IdentityFee, Weight},
	PalletId, StorageValue,
};
use sp_core::blake2_128;
use sp_runtime::AccountId32;

pub use pallet_balances as balances;
use pallet_chainbridge as bridge;

pub(crate) type BlockNumber = u32;
pub type Signature = MultiSignature;
pub type Balance = u128;
pub type Index = u64;
pub type Hash = sp_core::H256;
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

pub const MILLICENTS: Balance = 10_000_000_000;
pub const CENTS: Balance = 1_000 * MILLICENTS;
pub const DOLLARS: Balance = 100 * CENTS;

parameter_types! {
	pub const BlockHashCount: BlockNumber = 2400;
	pub const SS58Prefix: u16 = 42;
}
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type AccountId = AccountId;
	type Call = Call;
	type Lookup = AccountIdLookup<AccountId, ()>;
	type Index = Index;
	type BlockNumber = BlockNumber;
	type Hash = Hash;
	type Hashing = BlakeTwo256;
	type Header = generic::Header<BlockNumber, BlakeTwo256>;
	type Event = Event;
	type Origin = Origin;
	type BlockHashCount = BlockHashCount;
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type AccountData = pallet_balances::AccountData<Balance>;
	type SystemWeightInfo = ();
	type SS58Prefix = SS58Prefix;
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
}

parameter_types! {
	pub const ExistentialDeposit: Balance = 1 * DOLLARS;
	pub const MaxLocks: u32 = 50;
	pub const MaxReserves: u32 = 50;
}

impl pallet_balances::Config for Test {
	type MaxLocks = MaxLocks;
	type MaxReserves = MaxReserves;
	type ReserveIdentifier = [u8; 8];
	type Balance = Balance;
	type Event = Event;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = ();
}

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic
	{
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Config<T>, Storage, Event<T>},
		Erc721: chainbridge_erc721::{Pallet, Call, Storage, Event<T>},
	}
);

parameter_types! {
	pub Erc721Id: bridge::ResourceId = bridge::derive_resource_id(1, &blake2_128(b"NFT"));
}

impl Config for Test {
	type Event = Event;
	type Identifier = Erc721Id;
}

pub const USER_A: AccountId32 = AccountId32::new([1u8; 32]);
pub const USER_B: AccountId32 = AccountId32::new([2u8; 32]);
pub const USER_C: AccountId32 = AccountId32::new([3u8; 32]);
pub const ENDOWED_BALANCE: Balance = 100 * DOLLARS;

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();

	pallet_balances::GenesisConfig::<Test> { balances: vec![(USER_A, ENDOWED_BALANCE)] }
		.assimilate_storage(&mut t)
		.unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}
