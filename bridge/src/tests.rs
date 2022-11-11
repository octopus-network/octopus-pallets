use super::pallet::{CrosschainTransferFee, OracleAccount, TokenPrice};
use crate::{mock::*, Error, *};
use frame_support::{assert_noop, assert_ok};
use sp_keyring::AccountKeyring;
use sp_runtime::traits::BadOrigin;

#[test]
fn test_set_token_id() {
	let alice: AccountId = AccountKeyring::Alice.into();
	let _origin = RuntimeOrigin::signed(alice);
	new_tester().execute_with(|| {
		assert_noop!(
			OctopusBridge::set_token_id(
				RuntimeOrigin::root(),
				"usdc.testnet".to_string().as_bytes().to_vec(),
				2,
			),
			Error::<Test>::NotActivated
		);

		assert_ok!(OctopusAppchain::force_set_is_activated(RuntimeOrigin::root(), true));

		assert_noop!(
			OctopusBridge::set_token_id(_origin, "usdc.testnet".to_string().as_bytes().to_vec(), 2),
			BadOrigin
		);

		assert_ok!(OctopusBridge::set_token_id(
			RuntimeOrigin::root(),
			"usdc.testnet".to_string().as_bytes().to_vec(),
			2,
		));

		assert_noop!(
			OctopusBridge::set_token_id(
				RuntimeOrigin::root(),
				"usdc.testnet".to_string().as_bytes().to_vec(),
				2,
			),
			Error::<Test>::TokenIdInUse
		);

		assert_noop!(
			OctopusBridge::set_token_id(
				RuntimeOrigin::root(),
				"test.testnet".to_string().as_bytes().to_vec(),
				2,
			),
			Error::<Test>::AssetIdInUse
		);

		assert_ok!(OctopusBridge::set_token_id(
			RuntimeOrigin::root(),
			"test.testnet".to_string().as_bytes().to_vec(),
			1,
		));
	});
}

#[test]
fn test_delete_token_id() {
	let alice: AccountId = AccountKeyring::Alice.into();
	new_tester().execute_with(|| {
		assert_ok!(OctopusAppchain::force_set_is_activated(RuntimeOrigin::root(), true));
		assert_ok!(OctopusBridge::set_token_id(
			RuntimeOrigin::root(),
			"test.testnet".to_string().as_bytes().to_vec(),
			1,
		));
		assert_noop!(
			OctopusBridge::delete_token_id(
				RuntimeOrigin::root(),
				"gg.testnet".to_string().as_bytes().to_vec(),
			),
			Error::<Test>::TokenIdNotExist
		);
		assert_noop!(
			OctopusBridge::delete_token_id(
				RuntimeOrigin::signed(alice),
				"test.testnet".to_string().as_bytes().to_vec(),
			),
			BadOrigin
		);
		assert_ok!(OctopusBridge::delete_token_id(
			RuntimeOrigin::root(),
			"test.testnet".to_string().as_bytes().to_vec(),
		));
	});
}

#[test]
fn test_lock() {
	let alice: AccountId = AccountKeyring::Alice.into();
	let origin = RuntimeOrigin::signed(alice.clone());
	let source = sp_runtime::MultiAddress::Id(alice);
	new_tester().execute_with(|| {
		assert_ok!(Balances::set_balance(
			RuntimeOrigin::root(),
			source.clone(),
			1000000000000000000,
			100000
		));
		let minimum_amount = Balances::minimum_balance();
		let fee = Balances::minimum_balance();
		assert_noop!(
			OctopusBridge::lock(
				origin.clone(),
				"test-account.testnet".to_string().as_bytes().to_vec(),
				minimum_amount,
				fee,
			),
			Error::<Test>::NotActivated
		);

		assert_ok!(OctopusAppchain::force_set_is_activated(RuntimeOrigin::root(), true));
		assert_noop!(
			OctopusBridge::lock(origin.clone(), vec![0, 159], minimum_amount, fee),
			Error::<Test>::InvalidReceiverId
		);

		assert_ok!(OctopusBridge::lock(
			origin,
			"test-account.testnet".to_string().as_bytes().to_vec(),
			minimum_amount,
			fee,
		));
	});
}

#[test]
fn test_burn_nep141() {
	let alice: AccountId = AccountKeyring::Alice.into();
	let origin = RuntimeOrigin::signed(alice.clone());
	let source = sp_runtime::MultiAddress::Id(alice);
	new_tester().execute_with(|| {
		let fee = Balances::minimum_balance();
		assert_ok!(Balances::set_balance(
			RuntimeOrigin::root(),
			source.clone(),
			1000000000000000000,
			100000,
		));
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 0, source.clone(), true, 1));
		assert_ok!(Assets::mint(origin.clone(), 0, source.clone(), 10000000000000000000000));
		assert_noop!(
			OctopusBridge::burn_nep141(
				origin.clone(),
				0,
				"test-account.testnet".to_string().as_bytes().to_vec(),
				10000000000,
				fee,
			),
			Error::<Test>::NotActivated
		);
		assert_ok!(OctopusAppchain::force_set_is_activated(RuntimeOrigin::root(), true));
		assert_ok!(OctopusBridge::set_token_id(
			RuntimeOrigin::root(),
			"test.testnet".to_string().as_bytes().to_vec(),
			0,
		));
		assert_ok!(OctopusBridge::burn_nep141(
			origin,
			0,
			"test-account.testnet".to_string().as_bytes().to_vec(),
			100000000,
			fee,
		));
	});
}

#[test]
pub fn test_lock_nonfungible() {
	let alice: AccountId = AccountKeyring::Alice.into();
	let origin = RuntimeOrigin::signed(alice.clone());
	let source = sp_runtime::MultiAddress::Id(alice);
	new_tester().execute_with(|| {
		let fee = Balances::minimum_balance();
		assert_ok!(Balances::set_balance(
			RuntimeOrigin::root(),
			source.clone(),
			1000000000000000000,
			100000
		));
		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 0, source.clone(), true));
		assert_ok!(Uniques::mint(origin.clone(), 0, 42, source,));
		assert_noop!(
			OctopusBridge::lock_nonfungible(
				origin.clone(),
				0,
				42,
				"test-account.testnet".to_string().as_bytes().to_vec(),
				fee,
				1,
			),
			Error::<Test>::NotActivated
		);
		assert_ok!(OctopusAppchain::force_set_is_activated(RuntimeOrigin::root(), true));
		assert_ok!(OctopusBridge::lock_nonfungible(
			origin,
			0,
			42,
			"test-account.testnet".to_string().as_bytes().to_vec(),
			fee,
			1,
		));
	});
}

#[test]
pub fn test_force_unlock() {
	let alice: AccountId = AccountKeyring::Alice.into();
	let bob: AccountId = AccountKeyring::Bob.into();
	let origin = RuntimeOrigin::signed(bob);
	new_tester().execute_with(|| {
		let account = OctopusBridge::account_id();

		assert_ok!(Balances::set_balance(
			RuntimeOrigin::root(),
			sp_runtime::MultiAddress::Id(account),
			10000000000000000000000,
			100
		));

		assert_noop!(
			OctopusBridge::force_unlock(origin, sp_runtime::MultiAddress::Id(alice.clone()), 10000),
			BadOrigin
		);

		assert_ok!(OctopusBridge::force_unlock(
			RuntimeOrigin::root(),
			sp_runtime::MultiAddress::Id(alice),
			1000000000000000000,
		));
	});
}

#[test]
pub fn test_force_mint_nep141() {
	let ferdie: AccountId = AccountKeyring::Ferdie.into();
	let source = sp_runtime::MultiAddress::Id(ferdie.clone());
	new_tester().execute_with(|| {
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), 0, source.clone(), true, 1));

		assert_ok!(OctopusBridge::force_mint_nep141(
			RuntimeOrigin::root(),
			0,
			source.clone(),
			1000000000
		));

		assert_noop!(
			OctopusBridge::force_mint_nep141(RuntimeOrigin::signed(ferdie), 1, source, 1000000000),
			BadOrigin,
		);
	});
}

#[test]
pub fn test_force_unlock_nft() {
	let alice: AccountId = AccountKeyring::Alice.into();
	let origin = RuntimeOrigin::signed(alice.clone());
	let bob: AccountId = AccountKeyring::Bob.into();
	let origin2 = RuntimeOrigin::signed(bob);
	let source = sp_runtime::MultiAddress::Id(alice);
	new_tester().execute_with(|| {
		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 0, source.clone(), true));
		assert_ok!(Uniques::mint(origin.clone(), 0, 42, source.clone(),));
		assert_ok!(OctopusAppchain::force_set_is_activated(RuntimeOrigin::root(), true));
		let fee = Balances::minimum_balance();
		assert_ok!(Balances::set_balance(
			RuntimeOrigin::root(),
			source.clone(),
			1000000000000000000,
			100000
		));
		assert_ok!(OctopusBridge::lock_nonfungible(
			origin,
			0,
			42,
			"test-account.testnet".to_string().as_bytes().to_vec(),
			fee,
			1,
		));

		assert_ok!(OctopusBridge::force_unlock_nonfungible(
			RuntimeOrigin::root(),
			source.clone(),
			0,
			42
		));

		assert_noop!(OctopusBridge::force_unlock_nonfungible(origin2, source, 0, 42), BadOrigin);
	});
}

#[test]
pub fn test_set_oracle_account() {
	let alice: AccountId = AccountKeyring::Alice.into();
	let bob: AccountId = AccountKeyring::Bob.into();
	let origin = RuntimeOrigin::signed(bob);
	let source = sp_runtime::MultiAddress::Id(alice.clone());
	new_tester().execute_with(|| {
		assert_noop!(OctopusBridge::set_oracle_account(origin, source.clone()), BadOrigin);
		assert_ok!(OctopusBridge::set_oracle_account(RuntimeOrigin::root(), source));
		assert_eq!(OracleAccount::<Test>::get(), alice.into());
	});
}

#[test]
pub fn test_set_token_price() {
	let alice: AccountId = AccountKeyring::Alice.into();
	let origin1 = RuntimeOrigin::signed(alice.clone());
	let source = sp_runtime::MultiAddress::Id(alice);
	let bob: AccountId = AccountKeyring::Bob.into();
	let origin2 = RuntimeOrigin::signed(bob);
	new_tester().execute_with(|| {
		assert_noop!(
			OctopusBridge::set_token_price(origin1.clone(), 1000, 1000),
			Error::<Test>::NoOracleAccount
		);
		assert_ok!(OctopusBridge::set_oracle_account(RuntimeOrigin::root(), source.clone()));

		assert_noop!(
			OctopusBridge::set_token_price(origin2, 1000, 1000),
			Error::<Test>::UpdatePriceWithNoPermissionAccount
		);
		assert_noop!(
			OctopusBridge::set_token_price(origin1.clone(), 0, 0),
			Error::<Test>::NearPriceSetedIsZero
		);
		assert_noop!(
			OctopusBridge::set_token_price(origin1.clone(), 1000, 0),
			Error::<Test>::NativeTokenPriceSetedIsZero
		);
		assert_ok!(OctopusBridge::set_token_price(origin1.clone(), 1000, 1000));

		assert_eq!(TokenPrice::<Test>::get(), Some(1000));
		assert_eq!(
			CrosschainTransferFee::<Test>::get(CrossChainTransferType::Fungible).unwrap(),
			Some(30_000_000_000_000_000)
		);
		assert_eq!(
			CrosschainTransferFee::<Test>::get(CrossChainTransferType::Nonfungible).unwrap(),
			Some(30_000_000_000_000_000)
		);

		assert_ok!(OctopusBridge::set_token_price(origin1.clone(), 3_119_000, 6_620_000));
		assert_eq!(
			CrosschainTransferFee::<Test>::get(CrossChainTransferType::Fungible).unwrap(),
			Some(14_134_441_000_000_000)
		);

		assert_ok!(OctopusBridge::set_token_price(origin1.clone(), 3_119_000, 20_535_200_000));
		assert_eq!(
			CrosschainTransferFee::<Test>::get(CrossChainTransferType::Fungible).unwrap(),
			Some(4_556_000_000_000)
		);

		assert_ok!(OctopusBridge::set_token_price(origin1.clone(), 3_119_000, 41_000));
		assert_eq!(
			CrosschainTransferFee::<Test>::get(CrossChainTransferType::Fungible).unwrap(),
			Some(2_282_195_121_000_000_000)
		);

		assert_ok!(OctopusBridge::set_token_price(origin1.clone(), 3_119_000, 1));
		assert_eq!(
			CrosschainTransferFee::<Test>::get(CrossChainTransferType::Fungible).unwrap(),
			Some(300_000_000_000_000_000_000)
		);

		assert_ok!(OctopusBridge::set_token_price(origin1, 3_119_000, 3_119_000));
		assert_eq!(
			CrosschainTransferFee::<Test>::get(CrossChainTransferType::Fungible).unwrap(),
			Some(30_000_000_000_000_000)
		);
	});
}
