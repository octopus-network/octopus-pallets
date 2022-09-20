use crate::{mock::*, Error, *};
use frame_support::{assert_noop, assert_ok};
use sp_keyring::AccountKeyring;
use sp_runtime::traits::{BadOrigin, Verify};

#[test]
fn test_set_asset_name() {
	let alice: AccountId = AccountKeyring::Alice.into();
	let _origin = Origin::signed(alice);
	new_tester().execute_with(|| {
		assert_noop!(
			OctopusBridge::set_token_id(
				Origin::root(),
				"usdc.testnet".to_string().as_bytes().to_vec(),
				2,
			),
			Error::<Test>::NotActivated
		);

		assert_ok!(OctopusAppchain::force_set_is_activated(Origin::root(), true));
		assert_noop!(
			OctopusBridge::set_token_id(
				Origin::root(),
				"usdc.testnet".to_string().as_bytes().to_vec(),
				2,
			),
			Error::<Test>::TokenIdInUse
		);

		assert_noop!(
			OctopusBridge::set_token_id(
				Origin::root(),
				"test.testnet".to_string().as_bytes().to_vec(),
				2,
			),
			Error::<Test>::AssetIdInUse
		);

		assert_ok!(OctopusBridge::set_token_id(
			Origin::root(),
			"test.testnet".to_string().as_bytes().to_vec(),
			1,
		));
	});
}

#[test]
fn test_delete_token_id() {
	let alice: AccountId = AccountKeyring::Alice.into();
	new_tester().execute_with(|| {
		assert_ok!(OctopusAppchain::force_set_is_activated(Origin::root(), true));
		assert_ok!(OctopusBridge::set_token_id(
			Origin::root(),
			"test.testnet".to_string().as_bytes().to_vec(),
			1,
		));
		assert_noop!(
			OctopusBridge::delete_token_id(
				Origin::root(),
				"gg.testnet".to_string().as_bytes().to_vec(),
			),
			Error::<Test>::TokenIdNotExist
		);
		assert_noop!(
			OctopusBridge::delete_token_id(
				Origin::signed(alice),
				"test.testnet".to_string().as_bytes().to_vec(),
			),
			BadOrigin
		);
		assert_ok!(OctopusBridge::delete_token_id(
			Origin::root(),
			"test.testnet".to_string().as_bytes().to_vec(),
		));
	});
}


// #[test]
// fn test_mint_asset() {
// 	let ferdie: AccountId = AccountKeyring::Ferdie.into();
// 	new_tester().execute_with(|| {
// 		assert_ok!(Assets::force_create(
// 			Origin::root(),
// 			0,
// 			sp_runtime::MultiAddress::Id(ferdie.clone()),
// 			true,
// 			1
// 		));

// 		assert_ok!(OctopusBridge::mint_asset(
// 			Origin::root(),
// 			0,
// 			"test-account.testnet".to_string().as_bytes().to_vec(),
// 			sp_runtime::MultiAddress::Id(ferdie.clone()),
// 			1000000000
// 		));

// 		assert_noop!(
// 			OctopusBridge::mint_asset(
// 				Origin::signed(ferdie.clone()),
// 				1,
// 				"test-account.testnet".to_string().as_bytes().to_vec(),
// 				sp_runtime::MultiAddress::Id(ferdie.clone()),
// 				1000000000
// 			),
// 			BadOrigin,
// 		);

// 		assert_noop!(
// 			OctopusBridge::mint_asset(
// 				Origin::root(),
// 				1,
// 				"test-account.testnet".to_string().as_bytes().to_vec(),
// 				sp_runtime::MultiAddress::Index(()),
// 				1000000000
// 			),
// 			sp_runtime::DispatchError::CannotLookup,
// 		);

// 		assert_noop!(
// 			OctopusAppchain::mint_asset(
// 				Origin::root(),
// 				1,
// 				"test-account.testnet".to_string().as_bytes().to_vec(),
// 				sp_runtime::MultiAddress::Id(ferdie),
// 				1000000000
// 			),
// 			sp_runtime::TokenError::UnknownAsset,
// 		);
// 	});
// }

// #[test]
// fn test_burn_asset() {
// 	let alice: AccountId = AccountKeyring::Alice.into();
// 	let origin = Origin::signed(alice.clone());
// 	new_tester().execute_with(|| {
// 		assert_ok!(Assets::force_create(
// 			Origin::root(),
// 			0,
// 			sp_runtime::MultiAddress::Id(alice.clone()),
// 			true,
// 			1
// 		));
// 		assert_noop!(
// 			OctopusBridge::burn_nep141(
// 				origin.clone(),
// 				0,
// 				"test-account.testnet".to_string().as_bytes().to_vec(),
// 				10000000000
// 			),
// 			Error::<Test>::NotActivated
// 		);

// 		assert_ok!(OctopusAppchain::force_set_is_activated(Origin::root(), true));
// 		assert_ok!(OctopusBridge::mint_asset(
// 			Origin::root(),
// 			0,
// 			"test-account.testnet".to_string().as_bytes().to_vec(),
// 			sp_runtime::MultiAddress::Id(alice),
// 			1000000000000000000
// 		));
// 		assert_ok!(OctopusBridge::burn_nep141(
// 			origin.clone(),
// 			0,
// 			"test-account.testnet".to_string().as_bytes().to_vec(),
// 			100000000
// 		));
// 	});
// }

#[test]
fn test_lock() {
	let alice: AccountId = AccountKeyring::Alice.into();
	let origin = Origin::signed(alice.clone());
	let source = sp_runtime::MultiAddress::Id(alice);
	new_tester().execute_with(|| {
		assert_ok!(Balances::set_balance(Origin::root(), source, 10000000000000000000, 100));

		assert_noop!(
			OctopusBridge::lock(
				origin.clone(),
				"test-account.testnet".to_string().as_bytes().to_vec(),
				1000000000
			),
			Error::<Test>::NotActivated
		);

		assert_ok!(OctopusAppchain::force_set_is_activated(Origin::root(), true));
		assert_noop!(
			OctopusBridge::lock(origin.clone(), vec![0, 159], 1000000000),
			Error::<Test>::InvalidReceiverId
		);

		assert_ok!(OctopusBridge::lock(
			origin,
			"test-account.testnet".to_string().as_bytes().to_vec(),
			100000
		));
	});
}

#[test]
pub fn test_lock_nft() {
	let alice: AccountId = AccountKeyring::Alice.into();
	let origin = Origin::signed(alice.clone());
	new_tester().execute_with(|| {
		assert_ok!(Uniques::force_create(
			Origin::root(),
			sp_runtime::MultiAddress::Id(alice.clone()),
			true
		));
		assert_ok!(Uniques::mint(origin.clone(), 0, 42, sp_runtime::MultiAddress::Id(alice),));
		assert_noop!(
			OctopusBridge::lock_nonfungible(
				origin.clone(),
				0,
				42,
				"test-account.testnet".to_string().as_bytes().to_vec(),
			),
			Error::<Test>::NotActivated
		);
		assert_ok!(OctopusAppchain::force_set_is_activated(Origin::root(), true));
		assert_ok!(OctopusBridge::lock_nonfungible(
			origin,
			0,
			42,
			"test-account.testnet".to_string().as_bytes().to_vec(),
		));
	});
}

#[test]
pub fn test_force_unlock() {
	let alice: AccountId = AccountKeyring::Alice.into();
	let bob: AccountId = AccountKeyring::Bob.into();
	let origin = Origin::signed(bob);
	new_tester().execute_with(|| {
		let account = OctopusBridge::account_id();

		assert_ok!(Balances::set_balance(
			Origin::root(),
			sp_runtime::MultiAddress::Id(account),
			10000000000000000000000,
			100
		));

		assert_noop!(
			OctopusBridge::force_unlock(
				origin,
				sp_runtime::MultiAddress::Id(alice.clone()),
				10000
			),
			BadOrigin
		);

		assert_ok!(OctopusBridge::force_unlock(
			Origin::root(),
			sp_runtime::MultiAddress::Id(alice),
			1000000000000000000,
		));
	});
}

#[test]
pub fn test_force_mint_nep141() {
	let ferdie: AccountId = AccountKeyring::Ferdie.into();
	new_tester().execute_with(|| {
		assert_ok!(Assets::force_create(
			Origin::root(),
			0,
			sp_runtime::MultiAddress::Id(ferdie.clone()),
			true,
			1
		));

		assert_ok!(OctopusBridge::force_mint_nep141(
			Origin::root(),
			0,
			sp_runtime::MultiAddress::Id(ferdie.clone()),
			1000000000
		));

		assert_noop!(
			OctopusBridge::force_mint_nep141(
				Origin::signed(ferdie.clone()),
				1,
				sp_runtime::MultiAddress::Id(ferdie),
				1000000000
			),
			BadOrigin,
		);
	});
}

#[test]
pub fn test_force_unlock_nft() {
	let alice: AccountId = AccountKeyring::Alice.into();
	let origin = Origin::signed(alice.clone());
	let bob: AccountId = AccountKeyring::Bob.into();
	let origin2 = Origin::signed(bob);
	new_tester().execute_with(|| {
		assert_ok!(Uniques::force_create(
			Origin::root(),
			sp_runtime::MultiAddress::Id(alice.clone()),
			true
		));
		assert_ok!(Uniques::mint(
			origin.clone(),
			0,
			42,
			sp_runtime::MultiAddress::Id(alice.clone()),
		));
		assert_ok!(OctopusAppchain::force_set_is_activated(Origin::root(), true));
		assert_ok!(OctopusBridge::lock_nonfungible(
			origin,
			0,
			42,
			"test-account.testnet".to_string().as_bytes().to_vec(),
		));

		assert_ok!(OctopusBridge::force_unlock_nonfungible(
			Origin::root(),
			sp_runtime::MultiAddress::Id(alice.clone()),
			0,
			42,
		));

		assert_noop!(
			OctopusBridge::force_unlock_nonfungible(origin2, sp_runtime::MultiAddress::Id(alice), 0, 42,),
			BadOrigin,
		);
	});
}
