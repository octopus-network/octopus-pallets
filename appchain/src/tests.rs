use crate::{mock::*, Error, *};
use frame_support::{assert_noop, assert_ok};
use pallet_balances::Error as BalancesError;
use pallet_octopus_support::traits::{AppchainInterface, ValidatorsProvider};
use sp_core::offchain::{testing, OffchainWorkerExt, TransactionPoolExt};
use sp_keyring::{sr25519::Keyring, AccountKeyring};
use sp_keystore::{testing::KeyStore, KeystoreExt, SyncCryptoStore};
use sp_runtime::{
	traits::{BadOrigin, Verify},
	MultiSigner,
};
use std::sync::Arc;

type Public = <Signature as Verify>::Signer;
const KEY_TYPE_ID: KeyTypeId = <Test as Config>::AuthorityId::ID;

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

		assert_ok!(OctopusAppchain::force_set_next_notification_id(Origin::root(), 10));
		assert_noop!(
			OctopusAppchain::force_set_next_notification_id(Origin::signed(ferdie.clone()), 12),
			BadOrigin
		);
	});
}

#[test]
fn test_set_asset_name() {
	let alice: AccountId = AccountKeyring::Alice.into();
	let _origin = Origin::signed(alice.clone());
	new_tester().execute_with(|| {
		assert_noop!(
			OctopusAppchain::set_token_id(
				Origin::root(),
				"usdc.testnet".to_string().as_bytes().to_vec(),
				2,
			),
			Error::<Test>::NotActivated
		);

		assert_ok!(OctopusAppchain::force_set_is_activated(Origin::root(), true));
		assert_noop!(
			OctopusAppchain::set_token_id(
				Origin::root(),
				"usdc.testnet".to_string().as_bytes().to_vec(),
				2,
			),
			Error::<Test>::TokenIdInUse
		);

		assert_noop!(
			OctopusAppchain::set_token_id(
				Origin::root(),
				"test.testnet".to_string().as_bytes().to_vec(),
				2,
			),
			Error::<Test>::AssetIdInUse
		);

		assert_ok!(OctopusAppchain::set_token_id(
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
		assert_ok!(OctopusAppchain::set_token_id(
			Origin::root(),
			"test.testnet".to_string().as_bytes().to_vec(),
			1,
		));
		assert_noop!(
			OctopusAppchain::delete_token_id(
				Origin::root(),
				"gg.testnet".to_string().as_bytes().to_vec(),
			),
			Error::<Test>::TokenIdNotExist
		);
		assert_noop!(
			OctopusAppchain::delete_token_id(
				Origin::signed(alice),
				"test.testnet".to_string().as_bytes().to_vec(),
			),
			BadOrigin
		);
		assert_ok!(OctopusAppchain::delete_token_id(
			Origin::root(),
			"test.testnet".to_string().as_bytes().to_vec(),
		));
	});
}

#[test]
fn test_mint_asset() {
	let ferdie: AccountId = AccountKeyring::Ferdie.into();
	new_tester().execute_with(|| {
		assert_ok!(Assets::force_create(
			Origin::root(),
			0,
			sp_runtime::MultiAddress::Id(ferdie.clone()),
			true,
			1
		));

		assert_ok!(OctopusAppchain::mint_asset(
			Origin::root(),
			0,
			"test-account.testnet".to_string().as_bytes().to_vec(),
			sp_runtime::MultiAddress::Id(ferdie.clone()),
			1000000000
		));

		// check balance
		assert_eq!(Assets::balance(0, ferdie.clone()), 1000000000);

		assert_noop!(
			OctopusAppchain::mint_asset(
				Origin::signed(ferdie.clone()),
				1,
				"test-account.testnet".to_string().as_bytes().to_vec(),
				sp_runtime::MultiAddress::Id(ferdie.clone()),
				1000000000
			),
			BadOrigin,
		);

		assert_noop!(
			OctopusAppchain::mint_asset(
				Origin::root(),
				1,
				"test-account.testnet".to_string().as_bytes().to_vec(),
				sp_runtime::MultiAddress::Index(()),
				1000000000
			),
			sp_runtime::DispatchError::CannotLookup,
		);

		assert_noop!(
			OctopusAppchain::mint_asset(
				Origin::root(),
				1,
				"test-account.testnet".to_string().as_bytes().to_vec(),
				sp_runtime::MultiAddress::Id(ferdie),
				1000000000
			),
			sp_runtime::TokenError::UnknownAsset,
		);
	});
}

#[test]
fn test_burn_asset() {
	let alice: AccountId = AccountKeyring::Alice.into();
	let origin = Origin::signed(alice.clone());
	new_tester().execute_with(|| {
		assert_ok!(Assets::force_create(
			Origin::root(),
			0,
			sp_runtime::MultiAddress::Id(alice.clone()),
			true,
			1
		));

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
		assert_ok!(OctopusAppchain::mint_asset(
			Origin::root(),
			0,
			"test-account.testnet".to_string().as_bytes().to_vec(),
			sp_runtime::MultiAddress::Id(alice.clone()),
			1000000000000000000
		));
		assert_ok!(OctopusAppchain::burn_asset(
			origin.clone(),
			0,
			"test-account.testnet".to_string().as_bytes().to_vec(),
			100000000
		));

		// check balance
		assert_eq!(Assets::balance(0, alice.clone()), 1000000000000000000 - 100000000);
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
				1000000000
			),
			Error::<Test>::NotActivated
		);

		assert_ok!(OctopusAppchain::force_set_is_activated(Origin::root(), true));
		assert_noop!(
			OctopusAppchain::lock(origin.clone(), vec![0, 159], 1000000000),
			Error::<Test>::InvalidReceiverId
		);

		assert_noop!(
			OctopusAppchain::lock(
				origin.clone(),
				"test-account.testnet".to_string().as_bytes().to_vec(),
				1000000000
			),
			BalancesError::<Test>::InsufficientBalance
		);

		let account = OctopusAppchain::octopus_pallet_id().unwrap();
		let pallet_account = Origin::signed(account.clone());

		assert_ok!(OctopusAppchain::lock(
			pallet_account.clone(),
			"test-account.testnet".to_string().as_bytes().to_vec(),
			10000
		));
	});
}

/* Not finished.
#[test]
pub fn test_lock_nft() {
	let alice: AccountId = AccountKeyring::Alice.into();
	let origin = Origin::signed(alice.clone());
	new_tester().execute_with(|| {
		assert_ok!(Uniques::force_create(
			Origin::root(),
			0,
			sp_runtime::MultiAddress::Id(alice.clone()),
			true
		));
		assert_ok!(Uniques::mint(
			origin.clone(),
			0,
			42,
			sp_runtime::MultiAddress::Id(alice.clone()),
		));
		assert_noop!(
			OctopusAppchain::lock_nft(
				origin.clone(),
				0,
				42,
				"test-account.testnet".to_string().as_bytes().to_vec(),
			),
			Error::<Test>::NotActivated
		);
		assert_ok!(OctopusAppchain::force_set_is_activated(Origin::root(), true));
		assert_ok!(OctopusAppchain::lock_nft(
			origin.clone(),
			0,
			42,
			"test-account.testnet".to_string().as_bytes().to_vec(),
		));
	});
}*/

pub fn mock_payload_and_signature(
	keyring: Keyring,
) -> (ObservationsPayload<Public, BlockNumber, AccountId>, Signature) {
	let public = MultiSigner::from(keyring);
	let obs_payload = ObservationsPayload {
		public: public.clone(),
		block_number: 2,
		key_data: public.clone().into_account().encode(),
		observations: vec![expected_burn_notify()],
	};
	let sig = keyring.sign(&vec![1, 2]);
	let msig = sp_runtime::MultiSignature::from(sig);
	(obs_payload, msig)
}

#[test]
fn test_submit_observations() {
	let keyring = AccountKeyring::Alice;
	let (obs_payload1, msig1) = mock_payload_and_signature(keyring);

	let keyring = AccountKeyring::Bob;
	let (obs_payload2, msig2) = mock_payload_and_signature(keyring);

	let keyring = AccountKeyring::Charlie;
	let (obs_payload3, msig3) = mock_payload_and_signature(keyring);

	let stash: Balance = 100 * 1_000_000_000_000_000_000; // 100 OCT with 18 decimals
	let validators =
		vec![(AccountKeyring::Alice.into(), stash), (AccountKeyring::Bob.into(), stash)];

	new_tester().execute_with(|| {
		assert_ok!(OctopusAppchain::submit_observations(
			Origin::none(),
			obs_payload1.clone(),
			msig1.clone()
		));

		OctopusLpos::trigger_new_era(1, validators.clone());
		advance_session();
		assert_ok!(OctopusAppchain::submit_observations(
			Origin::none(),
			obs_payload2.clone(),
			msig2.clone()
		));

		assert_noop!(
			OctopusAppchain::submit_observations(
				Origin::none(),
				obs_payload3.clone(),
				msig3.clone()
			),
			Error::<Test>::NotValidator
		);
	});
}

#[test]
fn test_encode_args_works() {
	let test_get_validators_data = vec![
		(0u32, Some(b"eyJlcmFfbnVtYmVyIjoiMCJ9".to_vec())),
		(4294967295u32, Some(b"eyJlcmFfbnVtYmVyIjoiNDI5NDk2NzI5NSJ9".to_vec())),
	];

	for (set_id, expected) in test_get_validators_data {
		assert_eq!(expected, OctopusAppchain::encode_get_validator_args(set_id));
	}

	let test_get_notify_data = vec![
		(0u32, 0u32, Some(b"eyJzdGFydF9pbmRleCI6IjAiLCJxdWFudGl0eSI6IjAifQ==".to_vec())),
		(
			4294967295u32,
			4294967295u32,
			Some(
				b"eyJzdGFydF9pbmRleCI6IjQyOTQ5NjcyOTUiLCJxdWFudGl0eSI6IjQyOTQ5NjcyOTUifQ=="
					.to_vec(),
			),
		),
	];

	for (start, limit, expected) in test_get_notify_data {
		assert_eq!(expected, OctopusAppchain::encode_get_notification_args(start, limit));
	}
}

fn expected_val_set() -> Observation<AccountId> {
	let id = hex::decode("d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d")
		.map(|b| AccountId::decode(&mut &b[..]))
		.unwrap()
		.unwrap();
	let alice = Validator { validator_id_in_appchain: id, total_stake: 10000000000 };

	let id = hex::decode("8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48")
		.map(|b| AccountId::decode(&mut &b[..]))
		.unwrap()
		.unwrap();
	let bob = Validator { validator_id_in_appchain: id, total_stake: 10000000000 };

	let id = hex::decode("90b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe22")
		.map(|b| AccountId::decode(&mut &b[..]))
		.unwrap()
		.unwrap();
	let charlie = Validator { validator_id_in_appchain: id, total_stake: 100000000000 };

	let id = hex::decode("306721211d5404bd9da88e0204360a1a9ab8b87c66c1bc2fcdd37f3c2222cc20")
		.map(|b| AccountId::decode(&mut &b[..]))
		.unwrap()
		.unwrap();
	let dave = Validator { validator_id_in_appchain: id, total_stake: 10000000000 };

	Observation::UpdateValidatorSet(ValidatorSet {
		set_id: 1,
		validators: vec![alice, bob, charlie, dave],
	})
}

fn validator_set_1_response(state: &mut testing::OffchainState) {
	state.expect_request(testing::PendingRequest {
		method: "POST".into(),
		uri: "https://rpc.testnet.near.org".into(),
		headers: vec![("Content-Type".into(), "application/json".into())],
		body: br#"
		{
			"jsonrpc": "2.0",
			"id": "dontcare",
			"method": "query",
			"params": {
				"request_type": "call_function",
				"finality": "final",
				"account_id": "oct-test.testnet",
				"method_name": "get_validator_list_of",
				"args_base64": "eyJlcmFfbnVtYmVyIjoiMSJ9"
			}
		}"#.to_vec(),
		response: Some(br#"
		{
			"jsonrpc": "2.0",
			"result": {
				"block_hash": "EczErquQLMpUvTQpKupoQp5yNkgNbniMSHq1gVvhAf84",
				"block_height": 1,
				"logs": [],
		 		"result": [
					91,123,34,118,97,108,105,100,97,116,111,114,95,105,100,95,105,110,95,97,112,112,99,104,97,105,110,34,58,34,48,120,100,52,51,53,57,51,99,55,49,53,102,100,100,51,49,99,54,49,49,52,49,97,98,100,48,52,97,57,57,102,100,54,56,50,50,99,56,53,53,56,56,53,52,99,99,100,101,51,57,97,53,54,56,52,101,55,97,53,54,100,97,50,55,100,34,44,34,116,111,116,97,108,95,115,116,97,107,101,34,58,34,49,48,48,48,48,48,48,48,48,48,48,34,125,44,123,34,118,97,108,105,100,97,116,111,114,95,105,100,95,105,110,95,97,112,
					112,99,104,97,105,110,34,58,34,48,120,56,101,97,102,48,52,49,53,49,54,56,55,55,51,54,51,50,54,99,57,102,101,97,49,55,101,50,53,102,99,53,50,56,55,54,49,51,54,57,51,99,57,49,50,57,48,57,99,98,50,50,54,97,97,52,55,57,52,102,50,54,97,52,56,34,44,34,116,111,116,97,108,95,115,116,97,107,101,34,58,34,49,48,48,48,48,48,48,48,48,48,48,34,125,44,123,34,118,97,108,105,100,97,116,111,114,95,105,100,95,105,110,95,97,112,112,99,104,97,105,110,34,58,34,48,120,57,48,98,53,97,98,50,48,53,99,54,57,55,52,99,
					57,101,97,56,52,49,98,101,54,56,56,56,54,52,54,51,51,100,99,57,99,97,56,97,51,53,55,56,52,51,101,101,97,99,102,50,51,49,52,54,52,57,57,54,53,102,101,50,50,34,44,34,116,111,116,97,108,95,115,116,97,107,101,34,58,34,49,48,48,48,48,48,48,48,48,48,48,48,34,125,44,123,34,118,97,108,105,100,97,116,111,114,95,105,100,95,105,110,95,97,112,112,99,104,97,105,110,34,58,34,48,120,51,48,54,55,50,49,50,49,49,100,53,52,48,52,98,100,57,100,97,56,56,101,48,50,48,52,51,54,48,97,49,97,57,97,98,56,98,56,55,99,
					54,54,99,49,98,99,50,102,99,100,100,51,55,102,51,99,50,50,50,50,99,99,50,48,34,44,34,116,111,116,97,108,95,115,116,97,107,101,34,58,34,49,48,48,48,48,48,48,48,48,48,48,34,125,93
				]
			},
			"id": "dontcare"
		}
			"#.to_vec()),
		sent: true,
		..Default::default()
	});
}

fn empty_validator_set_1_response(state: &mut testing::OffchainState) {
	state.expect_request(testing::PendingRequest {
		method: "POST".into(),
		uri: "https://rpc.testnet.near.org".into(),
		headers: vec![("Content-Type".into(), "application/json".into())],
		body: br#"
		{
			"jsonrpc": "2.0",
			"id": "dontcare",
			"method": "query",
			"params": {
				"request_type": "call_function",
				"finality": "final",
				"account_id": "oct-test.testnet",
				"method_name": "get_validator_list_of",
				"args_base64": "eyJlcmFfbnVtYmVyIjoiMSJ9"
			}
		}"#
		.to_vec(),
		response: Some(
			br#"
		{
			"jsonrpc": "2.0",
			"result": {
				"block_hash": "EczErquQLMpUvTQpKupoQp5yNkgNbniMSHq1gVvhAf84",
				"block_height": 1,
				"logs": [],
		 		"result": [91,93]
			},
			"id": "dontcare"
		}
			"#
			.to_vec(),
		),
		sent: true,
		..Default::default()
	});
}

fn expected_burn_notify() -> Observation<AccountId> {
	let receiver = hex::decode("94f135526ec5fe830e0cbc6fd58683cb2d9ee06522cd9a2c0481268c5c73674f")
		.map(|b| AccountId::decode(&mut &b[..]))
		.unwrap()
		.unwrap();

	Observation::Burn(BurnEvent {
		index: 0,
		sender_id: "andy-pallet-test.testnet".to_string().as_bytes().to_vec(),
		receiver,
		amount: 100000,
	})
}

fn burn_notify_response(state: &mut testing::OffchainState) {
	state.expect_request(testing::PendingRequest {
		method: "POST".into(),
		uri: "https://rpc.testnet.near.org".into(),
		headers: vec![("Content-Type".into(), "application/json".into())],
		body: br#"
		{
			"jsonrpc": "2.0",
			"id": "dontcare",
			"method": "query",
			"params": {
				"request_type": "call_function",
				"finality": "final",
				"account_id": "oct-test.testnet",
				"method_name": "get_appchain_notification_histories",
				"args_base64": "eyJzdGFydF9pbmRleCI6IjAiLCJxdWFudGl0eSI6IjEwIn0="
			}
		}"#
		.to_vec(),
		response: Some(br#"
		{
			"jsonrpc": "2.0",
    		"result": {
        		"result": [91,123,34,97,112,112,99,104,97,105,110,95,110,111,116,105,102,105,99,97,116,105,111,110,34,58,123,34,87,114,97,112,112,101,100,65,112,112,99,104,97,105,110,84,111,107,101,110,66,117,114,110,116,34,58,123,34,115,101,110,100,101,114,95,105,100,95,105,110,95,110,101,97,114,34,58,34,97,110,100,121,45,112,97,108,108,101,116,45,116,101,115,116,46,116,101,115,116,110,101,116,34,44,34,114,101,99,101,105,118,101,114,95,105,100,95,105,110,95,97,112,112,99,104,97,105,110,34,58,34,48,120,57,52,102,49,51,
				53,53,50,54,101,99,53,102,101,56,51,48,101,48,99,98,99,54,102,100,53,56,54,56,51,99,98,50,100,57,101,101,48,54,53,50,50,99,100,57,97,50,99,48,52,56,49,50,54,56,99,53,99,55,51,54,55,52,102,34,44,34,97,109,111,117,110,116,34,58,34,49,48,48,48,48,48,34,125,125,44,34,98,108,111,99,107,95,104,101,105,103,104,116,34,58,55,49,53,56,57,49,56,54,44,34,116,105,109,101,115,116,97,109,112,34,58,49,54,51,55,48,55,50,53,50,50,50,49,50,54,51,53,51,50,57,44,34,105,110,100,101,120,34,58,34,48,34,125,93],
        		"logs": [],
        		"block_height": 73434388,
        		"block_hash": "9VhYFRLCvQfSz6TRrjnb8MvEtRQ46w4d5PDMRijZSzWj"
    		},
    		"id": "dontcare"
		}
			"#
			.to_vec(),
		),
		sent: true,
		..Default::default()
	});
}

#[test]
fn test_make_http_call_and_parse_result() {
	let (offchain, state) = testing::TestOffchainExt::new();
	let mut t = new_tester();
	t.register_extension(OffchainWorkerExt::new(offchain));

	validator_set_1_response(&mut state.write());
	burn_notify_response(&mut state.write());
	validator_set_1_response(&mut state.write());
	burn_notify_response(&mut state.write());

	t.execute_with(|| {
		let val_set = OctopusAppchain::get_validator_list_of(
			"https://rpc.testnet.near.org",
			b"oct-test.testnet".to_vec(),
			1,
		)
		.ok();
		assert_eq!(val_set, Some(vec![expected_val_set()]));

		let burn_notify = OctopusAppchain::get_appchain_notification_histories(
			"https://rpc.testnet.near.org",
			b"oct-test.testnet".to_vec(),
			0,
			10,
		)
		.ok();
		assert_eq!(burn_notify, Some(vec![expected_burn_notify()]));

		let val_set = OctopusAppchain::get_validator_list_of(
			"https://rpc.testnet.near.org",
			b"oct-test.testnet".to_vec(),
			1,
		)
		.ok();
		assert_eq!(val_set, Some(vec![expected_val_set()]));

		let burn_notify = OctopusAppchain::get_appchain_notification_histories(
			"https://rpc.testnet.near.org",
			b"oct-test.testnet".to_vec(),
			0,
			10,
		)
		.ok();
		assert_eq!(burn_notify, Some(vec![expected_burn_notify()]));
	});
}

#[test]
fn test_submit_validator_sets_on_chain() {
	const PHRASE: &str =
		"news slush supreme milk chapter athlete soap sausage put clutch what kitten";
	let (offchain, offchain_state) = testing::TestOffchainExt::new();
	let (pool, pool_state) = testing::TestTransactionPoolExt::new();

	let keystore = KeyStore::new();

	SyncCryptoStore::sr25519_generate_new(
		&keystore,
		KEY_TYPE_ID,
		Some(&format!("{}/hunter1", PHRASE)),
	)
	.unwrap();

	let public_key = SyncCryptoStore::sr25519_public_keys(&keystore, KEY_TYPE_ID)
		.get(0)
		.unwrap()
		.clone();

	let mut t = new_tester();
	t.register_extension(OffchainWorkerExt::new(offchain));
	t.register_extension(TransactionPoolExt::new(pool));
	t.register_extension(KeystoreExt(Arc::new(keystore)));

	validator_set_1_response(&mut offchain_state.write());

	let public = <Test as SigningTypes>::Public::from(public_key);
	let account = public.clone().into_account();
	let obs_payload = ObservationsPayload {
		public: public.clone(),
		block_number: 2,
		key_data: public.clone().into_account().encode(),
		observations: vec![expected_val_set()],
	};

	t.execute_with(|| {
		assert_ok!(OctopusAppchain::force_set_next_set_id(Origin::root(), 1));
		OctopusAppchain::observing_mainchain(
			2,
			"https://rpc.testnet.near.org",
			"https://rpc.testnet.near.org",
			b"oct-test.testnet".to_vec(),
			public.clone(),
			public.into_account().encode(), // default value.
			account,
		)
		.unwrap();

		let tx = pool_state.write().transactions.pop().unwrap();
		let tx = Extrinsic::decode(&mut &*tx).unwrap();
		assert_eq!(tx.signature, None);
		if let mock::Call::OctopusAppchain(crate::Call::submit_observations {
			payload: body,
			signature,
		}) = tx.call
		{
			assert_eq!(body, obs_payload);

			let signature_valid = <ObservationsPayload<
				<Test as SigningTypes>::Public,
				<Test as frame_system::Config>::BlockNumber,
				<Test as frame_system::Config>::AccountId,
			> as SignedPayload<Test>>::verify::<<Test as Config>::AppCrypto>(
				&obs_payload, signature
			);

			assert!(signature_valid);
		}
	});
}

#[test]
fn test_submit_notifies_on_chain() {
	const PHRASE: &str =
		"news slush supreme milk chapter athlete soap sausage put clutch what kitten";
	let (offchain, offchain_state) = testing::TestOffchainExt::new();
	let (pool, pool_state) = testing::TestTransactionPoolExt::new();

	let keystore = KeyStore::new();

	SyncCryptoStore::sr25519_generate_new(
		&keystore,
		KEY_TYPE_ID,
		Some(&format!("{}/hunter1", PHRASE)),
	)
	.unwrap();

	let public_key = SyncCryptoStore::sr25519_public_keys(&keystore, KEY_TYPE_ID)
		.get(0)
		.unwrap()
		.clone();

	let mut t = new_tester();
	t.register_extension(OffchainWorkerExt::new(offchain));
	t.register_extension(TransactionPoolExt::new(pool));
	t.register_extension(KeystoreExt(Arc::new(keystore)));

	empty_validator_set_1_response(&mut offchain_state.write());
	burn_notify_response(&mut offchain_state.write());

	let public = <Test as SigningTypes>::Public::from(public_key);
	let account = public.clone().into_account();
	let obs_payload = ObservationsPayload {
		public: public.clone(),
		block_number: 2,
		key_data: public.clone().into_account().encode(),
		observations: vec![expected_burn_notify()],
	};

	t.execute_with(|| {
		assert_ok!(OctopusAppchain::force_set_next_set_id(Origin::root(), 1));
		OctopusAppchain::observing_mainchain(
			2,
			"https://rpc.testnet.near.org",
			"https://rpc.testnet.near.org",
			b"oct-test.testnet".to_vec(),
			public.clone(),
			public.into_account().encode(),
			account,
		)
		.unwrap();

		let tx = pool_state.write().transactions.pop().unwrap();
		let tx = Extrinsic::decode(&mut &*tx).unwrap();
		assert_eq!(tx.signature, None);
		if let mock::Call::OctopusAppchain(crate::Call::submit_observations {
			payload: body,
			signature,
		}) = tx.call
		{
			assert_eq!(body, obs_payload);

			let signature_valid = <ObservationsPayload<
				<Test as SigningTypes>::Public,
				<Test as frame_system::Config>::BlockNumber,
				<Test as frame_system::Config>::AccountId,
			> as SignedPayload<Test>>::verify::<<Test as Config>::AppCrypto>(
				&obs_payload, signature
			);

			assert!(signature_valid);
		}
	});
}
