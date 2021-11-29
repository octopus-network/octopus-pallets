use crate::mock::*;
use frame_support::{assert_noop, assert_ok};
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
