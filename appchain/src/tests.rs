use crate::mock::*;
use crate::Error;
use frame_support::{assert_noop, assert_ok};
use pallet_balances::Error as BalancesError;
use pallet_octopus_support::traits::{AppchainInterface, ValidatorsProvider};
use sp_keyring::AccountKeyring;
use sp_runtime::traits::BadOrigin;

#[test]
fn test_force_set_params() {
	let stash: Balance = 100 * 1_000_000_000_000_000_000;
	let ferdie: AccountId = AccountKeyring::Ferdie.into();
	new_tester().execute_with(|| {
		assert_ok!(OctopusAppchain::force_set_planned_validators(
			Origin::root(),
			vec![
				(AccountKeyring::Alice.into(), stash),
				(AccountKeyring::Bob.into(), stash),
				(AccountKeyring::Charlie.into(), stash),
				(AccountKeyring::Dave.into(), stash),
				(AccountKeyring::Eve.into(), stash),
			],
		));
		assert_eq!(
			OctopusAppchain::validators(),
			vec![
				(AccountKeyring::Alice.into(), stash),
				(AccountKeyring::Bob.into(), stash),
				(AccountKeyring::Charlie.into(), stash),
				(AccountKeyring::Dave.into(), stash),
				(AccountKeyring::Eve.into(), stash),
			],
		);
		assert_noop!(
			OctopusAppchain::force_set_planned_validators(
				Origin::signed(ferdie.clone()),
				vec![(AccountKeyring::Dave.into(), stash), (AccountKeyring::Eve.into(), stash)],
			),
			BadOrigin
		);
		assert_eq!(
			OctopusAppchain::validators(),
			vec![
				(AccountKeyring::Alice.into(), stash),
				(AccountKeyring::Bob.into(), stash),
				(AccountKeyring::Charlie.into(), stash),
				(AccountKeyring::Dave.into(), stash),
				(AccountKeyring::Eve.into(), stash),
			],
		);

		assert_ok!(OctopusAppchain::force_set_is_activated(Origin::root(), true));
		assert_eq!(OctopusAppchain::is_activated(), true);
		assert_noop!(
			OctopusAppchain::force_set_is_activated(Origin::signed(ferdie.clone()), false),
			BadOrigin
		);
		assert_eq!(OctopusAppchain::is_activated(), true);

		assert_ok!(OctopusAppchain::force_set_next_set_id(Origin::root(), 1));
		assert_eq!(OctopusAppchain::next_set_id(), 1);
		assert_noop!(
			OctopusAppchain::force_set_next_set_id(Origin::signed(ferdie.clone()), 2),
			BadOrigin
		);
		assert_eq!(OctopusAppchain::next_set_id(), 1);
	});
}

#[test]
fn test_mint_asset() {
	let ferdie: AccountId = AccountKeyring::Ferdie.into();
	new_tester().execute_with(|| {
		// TODO:
		// assert_ok!(OctopusAppchain::mint_asset(
		// 	Origin::root(),
		// 	0,
		// 	"test-account.testnet".to_string().as_bytes().to_vec(),
		// 	sp_runtime::MultiAddress::Id(ferdie.clone()),
		// 	10000000000
		// ));

		assert_noop!(
			OctopusAppchain::mint_asset(
				Origin::signed(ferdie.clone()),
				1,
				"test-account.testnet".to_string().as_bytes().to_vec(),
				sp_runtime::MultiAddress::Id(ferdie.clone()),
				10000000000
			),
			BadOrigin,
		);

		assert_noop!(
			OctopusAppchain::mint_asset(
				Origin::root(),
				1,
				"test-account.testnet".to_string().as_bytes().to_vec(),
				sp_runtime::MultiAddress::Index(()),
				10000000000
			),
			sp_runtime::DispatchError::CannotLookup,
		);

		assert_noop!(
			OctopusAppchain::mint_asset(
				Origin::root(),
				1,
				"test-account.testnet".to_string().as_bytes().to_vec(),
				sp_runtime::MultiAddress::Id(ferdie),
				10000000000
			),
			sp_runtime::TokenError::UnknownAsset,
		);
	});
}

#[test]
fn test_burn_asset() {
	let alice: AccountId = AccountKeyring::Alice.into();
	let origin = Origin::signed(alice);
	new_tester().execute_with(|| {
		assert_noop!(
			OctopusAppchain::burn_asset(
				origin.clone(),
				0,
				"test-account.testnet".to_string().as_bytes().to_vec(),
				10000000000
			),
			Error::<Test>::NotActivated
		);

		assert_ok!(OctopusAppchain::force_set_is_activated(Origin::root(), true));
		// TODO:
		// assert_ok!(OctopusAppchain::burn_asset(
		// 	origin.clone(),
		// 	0,
		// 	"test-account.testnet".to_string().as_bytes().to_vec(),
		// 	1000000000000000000
		// ));
	});
}

#[test]
fn test_lock() {
	let alice: AccountId = AccountKeyring::Alice.into();
	let origin = Origin::signed(alice);
	new_tester().execute_with(|| {
		assert_noop!(
			OctopusAppchain::lock(
				origin.clone(),
				"test-account.testnet".to_string().as_bytes().to_vec(),
				10000000000
			),
			Error::<Test>::NotActivated
		);

		assert_ok!(OctopusAppchain::force_set_is_activated(Origin::root(), true));
		assert_noop!(
			OctopusAppchain::lock(origin.clone(), vec![0, 159], 10000000000),
			Error::<Test>::InvalidReceiverId
		);

		assert_noop!(
			OctopusAppchain::lock(
				origin.clone(),
				"test-account.testnet".to_string().as_bytes().to_vec(),
				10000000000
			),
			BalancesError::<Test>::InsufficientBalance
		);

		let account = OctopusAppchain::pallet_account();
		let pallet_account = Origin::signed(account);
		assert_ok!(OctopusAppchain::lock(
			pallet_account.clone(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
			10000
		));
	});
}

#[test]
fn test_unlock_inner() {
    
}

#[test]
fn test_submit_observations() {}
