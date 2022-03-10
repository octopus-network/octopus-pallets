use crate::{mock::*, *};
use frame_support::{assert_noop, assert_ok};
use sp_keyring::AccountKeyring;
use sp_runtime::traits::{BadOrigin, Zero};

#[test]
fn test_set_history_depth() {
	let ferdie: AccountId = AccountKeyring::Ferdie.into();
	let stash: Balance = 100 * 1_000_000_000_000_000_000;
	let validators =
		vec![(AccountKeyring::Alice.into(), stash), (AccountKeyring::Bob.into(), stash)];
	new_tester().execute_with(|| {
		assert_ok!(OctopusLpos::set_history_depth(Origin::root(), EraIndex::zero(), u32::MAX));

		assert_noop!(
			OctopusLpos::set_history_depth(
				Origin::signed(ferdie.clone()),
				EraIndex::zero(),
				u32::MAX
			),
			BadOrigin,
		);

		OctopusLpos::trigger_new_era(1, validators.clone());
		assert_ok!(OctopusLpos::set_history_depth(Origin::root(), EraIndex::zero(), u32::MAX));
	});
}

#[test]
fn test_force_set_era_payout() {
	let ferdie: AccountId = AccountKeyring::Ferdie.into();
	new_tester().execute_with(|| {
		assert_ok!(OctopusLpos::force_set_era_payout(Origin::root(), u128::MAX));

		assert_noop!(
			OctopusLpos::force_set_era_payout(Origin::signed(ferdie.clone()), u128::MAX),
			BadOrigin,
		);
	});
}
