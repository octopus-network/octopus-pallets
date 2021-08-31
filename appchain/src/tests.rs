use crate::mock::*;
use crate::*;
use sp_core::offchain::{testing, OffchainWorkerExt, TransactionPoolExt};
use sp_core::sr25519::Public as AccountId;
use sp_keystore::{
	testing::KeyStore,
	{KeystoreExt, SyncCryptoStore},
};
use sp_runtime::RuntimeAppPublic;
use std::sync::Arc;

#[test]
fn encode_args_works() {
	let test_data = vec![
		(
			b"octopus".to_vec(),
			0u32,
			0u32,
			Some(b"eyJhcHBjaGFpbl9pZCI6Im9jdG9wdXMiLCJzdGFydCI6MCwibGltaXQiOjB9".to_vec()),
		),
		(
			b"appchain".to_vec(),
			4294967295u32,
			4294967295u32,
			Some(b"eyJhcHBjaGFpbl9pZCI6ImFwcGNoYWluIiwic3RhcnQiOjQyOTQ5NjcyOTUsImxpbWl0Ijo0Mjk0OTY3Mjk1fQ==".to_vec()),
		)
	];

	for (appchain_id, start, limit, expected) in test_data {
		assert_eq!(expected, OctopusAppchain::encode_args(appchain_id, start, limit));
	}
}

fn expected_val_set() -> Observation<AccountId> {
	let id = hex::decode("94f135526ec5fe830e0cbc6fd58683cb2d9ee06522cd9a2c0481268c5c73674f")
		.map(|b| AccountId::decode(&mut &b[..]))
		.unwrap()
		.unwrap();

	let alice = Validator { id, weight: 100000000000000000000000000 };

	let id = hex::decode("c425bbf59c7bf49e4fcc6547539d84ba8ecd2fb171f5b83cde3571d45d0c8224")
		.map(|b| AccountId::decode(&mut &b[..]))
		.unwrap()
		.unwrap();

	let bob = Validator { id, weight: 100000000000000000000000000 };

	let id = hex::decode("d447acbfe7761c0cfba8341e616275caca6401637308ee123b77082a40095331")
		.map(|b| AccountId::decode(&mut &b[..]))
		.unwrap()
		.unwrap();

	let charlie = Validator { id, weight: 100000000000000000000000000 };

	let id = hex::decode("4093acd03283fa2d53d3b684b2a7ce3118ceb047b869f6c000d041578420de22")
		.map(|b| AccountId::decode(&mut &b[..]))
		.unwrap()
		.unwrap();

	let dave = Validator { id, weight: 100000000000000000000000000 };
	Observation::UpdateValidatorSet(ValidatorSet {
		sequence_number: 0,
		set_id: 1,
		validators: vec![alice, bob, charlie, dave],
	})
}

fn validator_set_0_response(state: &mut testing::OffchainState) {
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
				"account_id": "octopus.testnet",
				"method_name": "get_facts",
				"args_base64": "eyJhcHBjaGFpbl9pZCI6ImFwcGNoYWluIiwic3RhcnQiOjAsImxpbWl0IjoxfQ=="
			}
		}"#.to_vec(),
		response: Some(br#"
		{
			"jsonrpc": "2.0",
			"result": {
				"block_hash": "E4URWhvkMobUSkBAxg2PnEXVTuWGpibeSiosruB1rsuF",
				"block_height": 55532920,
				"logs": [],
				"result": [
					91, 123, 34, 85, 112, 100, 97, 116, 101, 86, 97, 108, 105, 100, 97, 116, 111, 114, 83, 101, 116, 34, 58, 123, 34, 115, 101, 113, 95, 110, 117, 109, 34, 58, 48, 44, 34, 115, 101, 116, 95, 105, 100, 34, 58, 49, 44, 34, 118, 97, 108, 105, 100, 97, 116, 111, 114, 115, 34, 58, 91, 123, 34, 105, 100, 34, 58, 34, 48, 120, 57, 52, 102, 49, 51, 53, 53, 50, 54, 101, 99, 53, 102, 101, 56, 51, 48, 101, 48, 99, 98, 99, 54, 102, 100, 53, 56, 54, 56, 51, 99, 98, 50, 100, 57, 101, 101, 48, 54, 53, 50, 50,
					99, 100, 57, 97, 50, 99, 48, 52, 56, 49, 50, 54, 56, 99, 53, 99, 55, 51, 54, 55, 52, 102, 34, 44, 34, 97, 99, 99, 111, 117, 110, 116, 95, 105, 100, 34, 58, 34, 97, 108, 105, 99, 101, 45, 111, 99, 116, 111, 112, 117, 115, 46, 116, 101, 115, 116, 110, 101, 116, 34, 44, 34, 119, 101, 105, 103, 104, 116, 34, 58, 34, 49, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 98, 108, 111, 99, 107, 95, 104, 101, 105, 103, 104, 116, 34,
					58, 53, 51, 51, 52, 56, 56, 51, 52, 44, 34, 100, 101, 108, 101, 103, 97, 116, 111, 114, 115, 34, 58, 91, 93, 125, 44, 123, 34, 105, 100, 34, 58, 34, 48, 120, 99, 52, 50, 53, 98, 98, 102, 53, 57, 99, 55, 98, 102, 52, 57, 101, 52, 102, 99, 99, 54, 53, 52, 55, 53, 51, 57, 100, 56, 52, 98, 97, 56, 101, 99, 100, 50, 102, 98, 49, 55, 49, 102, 53, 98, 56, 51, 99, 100, 101, 51, 53, 55, 49, 100, 52, 53, 100, 48, 99, 56, 50, 50, 52, 34, 44, 34, 97, 99, 99, 111, 117, 110, 116, 95, 105, 100, 34, 58, 34,
					98, 111, 98, 45, 111, 99, 116, 111, 112, 117, 115, 46, 116, 101, 115, 116, 110, 101, 116, 34, 44, 34, 119, 101, 105, 103, 104, 116, 34, 58, 34, 49, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 98, 108, 111, 99, 107, 95, 104, 101, 105, 103, 104, 116, 34, 58, 53, 51, 51, 52, 56, 56, 52, 54, 44, 34, 100, 101, 108, 101, 103, 97, 116, 111, 114, 115, 34, 58, 91, 93, 125, 44, 123, 34, 105, 100, 34, 58, 34, 48, 120, 100, 52, 52,
					55, 97, 99, 98, 102, 101, 55, 55, 54, 49, 99, 48, 99, 102, 98, 97, 56, 51, 52, 49, 101, 54, 49, 54, 50, 55, 53, 99, 97, 99, 97, 54, 52, 48, 49, 54, 51, 55, 51, 48, 56, 101, 101, 49, 50, 51, 98, 55, 55, 48, 56, 50, 97, 52, 48, 48, 57, 53, 51, 51, 49, 34, 44, 34, 97, 99, 99, 111, 117, 110, 116, 95, 105, 100, 34, 58, 34, 99, 104, 97, 114, 108, 105, 101, 45, 111, 99, 116, 111, 112, 117, 115, 46, 116, 101, 115, 116, 110, 101, 116, 34, 44, 34, 119, 101, 105, 103, 104, 116, 34, 58, 34, 49, 48, 48,
					48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 98, 108, 111, 99, 107, 95, 104, 101, 105, 103, 104, 116, 34, 58, 53, 51, 51, 52, 56, 56, 53, 53, 44, 34, 100, 101, 108, 101, 103, 97, 116, 111, 114, 115, 34, 58, 91, 93, 125, 44, 123, 34, 105, 100, 34, 58, 34, 48, 120, 52, 48, 57, 51, 97, 99, 100, 48, 51, 50, 56, 51, 102, 97, 50, 100, 53, 51, 100, 51, 98, 54, 56, 52, 98, 50, 97, 55, 99, 101, 51, 49, 49, 56, 99, 101, 98, 48, 52, 55, 98,
					56, 54, 57, 102, 54, 99, 48, 48, 48, 100, 48, 52, 49, 53, 55, 56, 52, 50, 48, 100, 101, 50, 50, 34, 44, 34, 97, 99, 99, 111, 117, 110, 116, 95, 105, 100, 34, 58, 34, 100, 97, 118, 101, 45, 111, 99, 116, 111, 112, 117, 115, 46, 116, 101, 115, 116, 110, 101, 116, 34, 44, 34, 119, 101, 105, 103, 104, 116, 34, 58, 34, 49, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 34, 44, 34, 98, 108, 111, 99, 107, 95, 104, 101, 105, 103, 104, 116, 34,
					58, 53, 51, 51, 52, 56, 56, 54, 52, 44, 34, 100, 101, 108, 101, 103, 97, 116, 111, 114, 115, 34, 58, 91, 93, 125, 93, 125, 125, 93
				]
			},
			"id": "dontcare"
		}
			"#.to_vec()),
		sent: true,
		..Default::default()
	});
}

#[test]
fn should_make_http_call_and_parse_result() {
	let (offchain, state) = testing::TestOffchainExt::new();
	let mut t = sp_io::TestExternalities::default();
	t.register_extension(OffchainWorkerExt::new(offchain));

	validator_set_0_response(&mut state.write());

	t.execute_with(|| {
		// when
		let val_set =
			OctopusAppchain::fetch_facts(b"octopus.testnet".to_vec(), b"appchain".to_vec(), 0, 1)
				.ok();
		// then
		assert_eq!(val_set, Some(vec![expected_val_set()]));
	});
}

#[test]
fn should_submit_unsigned_transaction_on_chain() {
	const PHRASE: &str =
		"news slush supreme milk chapter athlete soap sausage put clutch what kitten";
	let (offchain, offchain_state) = testing::TestOffchainExt::new();
	let (pool, pool_state) = testing::TestTransactionPoolExt::new();

	let keystore = KeyStore::new();

	SyncCryptoStore::sr25519_generate_new(
		&keystore,
		crate::crypto::Public::ID,
		Some(&format!("{}/hunter1", PHRASE)),
	)
	.unwrap();

	let public_key = SyncCryptoStore::sr25519_public_keys(&keystore, crate::crypto::Public::ID)
		.get(0)
		.unwrap()
		.clone();

	let mut t = sp_io::TestExternalities::default();
	t.register_extension(OffchainWorkerExt::new(offchain));
	t.register_extension(TransactionPoolExt::new(pool));
	t.register_extension(KeystoreExt(Arc::new(keystore)));

	validator_set_0_response(&mut offchain_state.write());

	let obs_payload = ObservationsPayload {
		public: <Test as SigningTypes>::Public::from(public_key),
		block_number: 1,
		next_fact_sequence: 0,
		observations: vec![expected_val_set()],
	};

	t.execute_with(|| {
		// when
		OctopusAppchain::observing_mainchain(
			1,
			b"octopus.testnet".to_vec(),
			b"appchain".to_vec(),
			1,
		)
		.unwrap();
		// then
		let tx = pool_state.write().transactions.pop().unwrap();
		let tx = Extrinsic::decode(&mut &*tx).unwrap();
		assert_eq!(tx.signature, None);
		if let mock::Call::OctopusAppchain(crate::Call::submit_observations(body, signature)) =
			tx.call
		{
			assert_eq!(body, obs_payload);

			let signature_valid = <ObservationsPayload<
				<Test as SigningTypes>::Public,
				<Test as frame_system::Config>::BlockNumber,
				<Test as frame_system::Config>::AccountId,
			> as SignedPayload<Test>>::verify::<<Test as Config>::AuthorityId>(
				&obs_payload, signature
			);

			assert!(signature_valid);
		}
	});
}
