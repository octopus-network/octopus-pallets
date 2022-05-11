use super::*;
use base58::ToBase58;

#[derive(Deserialize, RuntimeDebug)]
struct Response {
	jsonrpc: String,
	result: ResponseResult,
	id: String,
}

#[derive(Deserialize, RuntimeDebug)]
pub struct ResponseResult {
	result: Vec<u8>,
	logs: Vec<String>,
	block_height: u64,
	block_hash: String,
}

#[derive(Deserialize, RuntimeDebug)]
pub struct AppchainNotificationHistory<AccountId> {
	#[serde(bound(deserialize = "AccountId: Decode"))]
	appchain_notification: AppchainNotification<AccountId>,
	block_height: u64,
	timestamp: u64,
	#[serde(deserialize_with = "deserialize_from_str")]
	index: u32,
}

#[derive(Deserialize, RuntimeDebug)]
struct AccessKeyResponse {
	jsonrpc: String,
	result: AccessKeyResult,
	id: String,
}

#[derive(Deserialize, RuntimeDebug)]
pub struct AccessKeyResult {
	pub nonce: transaction::Nonce,
	pub block_height: u64,
	pub block_hash: String,
}

#[derive(Deserialize, RuntimeDebug, Clone)]
pub struct SendTransactionResponse {
	jsonrpc: String,
	result: SendTransactionResult,
	id: String,
}

#[derive(Deserialize, RuntimeDebug, Clone)]
pub struct SendTransactionResult {
	pub status: TransactionExecuteSuccess,
	pub transaction: TransactionResult,
}

#[derive(Deserialize, RuntimeDebug, Clone)]
pub struct TransactionExecuteSuccess {
	#[serde(rename = "SuccessValue")]
	pub success_value: String,
}

#[derive(Deserialize, RuntimeDebug, Clone)]
pub struct TransactionResult {
	pub hash: String,
}

#[derive(Deserialize, RuntimeDebug, Clone)]
pub struct AppchainCommitment {
	#[serde(rename = "payload")]
	pub mmr_root_hash: [u8; 32],
	pub block_number: u32,
}

impl<T: Config> Pallet<T> {
	/// Gets a validator set by the specified era number.
	/// Returns an empty list if the validator set has not been generated.
	pub(super) fn get_validator_list_of(
		rpc_endpoint: &str,
		anchor_contract: Vec<u8>,
		set_id: u32,
	) -> Result<Vec<Observation<<T as frame_system::Config>::AccountId>>, http::Error> {
		// We want to keep the offchain worker execution time reasonable, so we set a hard-coded
		// deadline to 2s to complete the external call.
		// You can also wait idefinitely for the response, however you may still get a timeout
		// coming from the host machine.
		let deadline = sp_io::offchain::timestamp().add(Duration::from_millis(2_000));
		// Initiate an external HTTP GET request.
		// This is using high-level wrappers from `sp_runtime`, for the low-level calls that
		// you can find in `sp_io`. The API is trying to be similar to `reqwest`, but
		// since we are running in a custom WASM execution environment we can't simply
		// import the library here.
		let args = Self::encode_get_validator_args(set_id).ok_or_else(|| {
			log!(warn, "Encode get_validator_list_of args error");
			http::Error::Unknown
		})?;

		let mut body = br#"
		{
			"jsonrpc": "2.0",
			"id": "dontcare",
			"method": "query",
			"params": {
				"request_type": "call_function",
				"finality": "final",
				"account_id": ""#
			.to_vec();
		body.extend(&anchor_contract);
		body.extend(
			br#"",
				"method_name": "get_validator_list_of",
				"args_base64": ""#,
		);
		body.extend(&args);
		body.extend(
			br#""
			}
		}"#,
		);
		let request = http::Request::default()
			.method(http::Method::Post)
			.url(rpc_endpoint)
			.body(vec![body])
			.add_header("Content-Type", "application/json");
		// We set the deadline for sending of the request, note that awaiting response can
		// have a separate deadline. Next we send the request, before that it's also possible
		// to alter request headers or stream body content in case of non-GET requests.
		let pending = request.deadline(deadline).send().map_err(|_| http::Error::IoError)?;

		// The request is already being processed by the host, we are free to do anything
		// else in the worker (we can send multiple concurrent requests too).
		// At some point however we probably want to check the response though,
		// so we can block current thread and wait for it to finish.
		// Note that since the request is being driven by the host, we don't have to wait
		// for the request to have it complete, we will just not read the response.
		let response = pending.try_wait(deadline).map_err(|_| http::Error::DeadlineReached)??;
		// Let's check the status code before we proceed to reading the response.
		if response.code != 200 {
			log!(warn, "Unexpected status code when get validator: {}", response.code);
			return Err(http::Error::Unknown)
		}

		// Next we want to fully read the response body and collect it to a vector of bytes.
		// Note that the return object allows you to read the body in chunks as well
		// with a way to control the deadline.
		let body = response.body().collect::<Vec<u8>>();
		log!(debug, "body: {:?}", body);

		let json_response: Response = serde_json::from_slice(&body).map_err(|_| {
			log::warn!("Failed to decode http body");
			http::Error::Unknown
		})?;
		log!(debug, "{:?}", json_response);

		let mut obs: Vec<Observation<<T as frame_system::Config>::AccountId>> = vec![];
		let validators: Vec<Validator<<T as frame_system::Config>::AccountId>> =
			serde_json::from_slice(&json_response.result.result).map_err(|_| {
				log!(warn, "Failed to decode validators");
				http::Error::Unknown
			})?;
		if validators.len() > 0 {
			let val_set = ValidatorSet { set_id, validators };
			obs.push(Observation::UpdateValidatorSet(val_set));
		}

		log!(debug, "Got observations: {:?}", obs);

		Ok(obs)
	}

	pub(crate) fn encode_get_validator_args(era: u32) -> Option<Vec<u8>> {
		let a = String::from("{\"era_number\":\"");
		let era = era.to_string();
		let b = String::from("\"}");
		let json = a + &era + &b;
		let res = base64::encode(json).into_bytes();
		Some(res)
	}

	/// Fetch the notifications from anchor contract.
	pub(super) fn get_appchain_notification_histories(
		rpc_endpoint: &str,
		anchor_contract: Vec<u8>,
		index: u32,
		limit: u32,
	) -> Result<Vec<Observation<<T as frame_system::Config>::AccountId>>, http::Error> {
		// We want to keep the offchain worker execution time reasonable, so we set a hard-coded
		// deadline to 2s to complete the external call.
		// You can also wait idefinitely for the response, however you may still get a timeout
		// coming from the host machine.
		let deadline = sp_io::offchain::timestamp().add(Duration::from_millis(2_000));
		// Initiate an external HTTP GET request.
		// This is using high-level wrappers from `sp_runtime`, for the low-level calls that
		// you can find in `sp_io`. The API is trying to be similar to `reqwest`, but
		// since we are running in a custom WASM execution environment we can't simply
		// import the library here.
		let args = Self::encode_get_notification_args(index, limit).ok_or_else(|| {
			log!(info, "Encode get_appchain_notification_histories args error");
			http::Error::Unknown
		})?;

		let mut body = br#"
		{
			"jsonrpc": "2.0",
			"id": "dontcare",
			"method": "query",
			"params": {
				"request_type": "call_function",
				"finality": "final",
				"account_id": ""#
			.to_vec();
		body.extend(&anchor_contract);
		body.extend(
			br#"",
				"method_name": "get_appchain_notification_histories",
				"args_base64": ""#,
		);
		body.extend(&args);
		body.extend(
			br#""
			}
		}"#,
		);
		let request = http::Request::default()
			.method(http::Method::Post)
			.url(rpc_endpoint)
			.body(vec![body])
			.add_header("Content-Type", "application/json");
		// We set the deadline for sending of the request, note that awaiting response can
		// have a separate deadline. Next we send the request, before that it's also possible
		// to alter request headers or stream body content in case of non-GET requests.
		let pending = request.deadline(deadline).send().map_err(|_| http::Error::IoError)?;

		// The request is already being processed by the host, we are free to do anything
		// else in the worker (we can send multiple concurrent requests too).
		// At some point however we probably want to check the response though,
		// so we can block current thread and wait for it to finish.
		// Note that since the request is being driven by the host, we don't have to wait
		// for the request to have it complete, we will just not read the response.
		let response = pending.try_wait(deadline).map_err(|_| http::Error::DeadlineReached)??;
		// Let's check the status code before we proceed to reading the response.
		if response.code != 200 {
			log!(warn, "Unexpected status code when get notification: {}", response.code);
			return Err(http::Error::Unknown)
		}

		// Next we want to fully read the response body and collect it to a vector of bytes.
		// Note that the return object allows you to read the body in chunks as well
		// with a way to control the deadline.
		let body = response.body().collect::<Vec<u8>>();
		log!(debug, "body: {:?}", body);

		let json_response: Response = serde_json::from_slice(&body).map_err(|_| {
			log::warn!("Failed to decode http body");
			http::Error::Unknown
		})?;
		log!(debug, "{:?}", json_response);

		let mut obs: Vec<Observation<<T as frame_system::Config>::AccountId>> = vec![];
		let notifications: Vec<
			AppchainNotificationHistory<<T as frame_system::Config>::AccountId>,
		> = serde_json::from_slice(&json_response.result.result).map_err(|_| {
			log!(warn, "Failed to decode appchain notification histories");
			http::Error::Unknown
		})?;
		log!(debug, "appchain notifications: {:#?}", notifications);

		for n in notifications.iter() {
			match n.appchain_notification.clone() {
				AppchainNotification::Burn(mut event) => {
					event.index = n.index;
					obs.push(Observation::Burn(event));
				},
				AppchainNotification::LockAsset(mut event) => {
					event.index = n.index;
					obs.push(Observation::LockAsset(event));
				},
				AppchainNotification::BurnNft(mut event) => {
					event.index = n.index;
					obs.push(Observation::BurnNft(event));
				},
			}
		}

		log!(debug, "Got observations: {:?}", obs);

		Ok(obs)
	}

	pub(crate) fn encode_get_notification_args(start: u32, limit: u32) -> Option<Vec<u8>> {
		let a = String::from("{\"start_index\":\"");
		let start_index = start.to_string();
		let b = String::from("\",\"quantity\":\"");
		let quantity = limit.to_string();
		let c = String::from("\"}");
		let json = a + &start_index + &b + &quantity + &c;
		let res = base64::encode(json).into_bytes();
		Some(res)
	}

	pub(super) fn query_access_key(
		rpc_endpoint: &str,
		account_id: Vec<u8>,
		public_key: [u8; 32],
	) -> Result<AccessKeyResult, http::Error> {
		let deadline = sp_io::offchain::timestamp().add(Duration::from_millis(2_000));
		let public_key = Self::encode_public_key_in_base58(public_key);
		log!(debug, "query_access_key, public key is {:?}", public_key.clone());

		let mut body = br#"
		{
			"jsonrpc": "2.0",
			"id": "dontcare",
			"method": "query",
			"params": {
				"request_type": "view_access_key",
				"finality": "final",
				"account_id": ""#
			.to_vec();
		body.extend(&account_id);
		body.extend(
			br#"",
				"public_key": "ed25519:"#,
		);
		body.extend(&public_key.as_bytes().to_vec());
		body.extend(
			br#""
			}
		}"#,
		);

		let request = http::Request::default()
			.method(http::Method::Post)
			.url(rpc_endpoint)
			.body(vec![body])
			.add_header("Content-Type", "application/json");
		log!(debug, "Access key request: {:?}", request);

		let pending = request.deadline(deadline).send().map_err(|_| http::Error::IoError)?;

		let response = pending.try_wait(deadline).map_err(|_| http::Error::DeadlineReached)??;
		if response.code != 200 {
			log!(warn, "Unexpected status code: {}", response.code);
			return Err(http::Error::Unknown)
		}

		let body = response.body().collect::<Vec<u8>>();
		log!(debug, "Access key body: {:?}", body);

		let json_response: AccessKeyResponse = serde_json::from_slice(&body).map_err(|_| {
			log::warn!("Failed to decode http body when call query_access_key.");
			http::Error::Unknown
		})?;
		log!(debug, "Query access key, json_response: {:?}", json_response);

		Ok(json_response.result)
	}

	pub(crate) fn encode_public_key_in_base58(public_key: [u8; 32]) -> String {
		public_key[..].to_base58()
	}

	pub(crate) fn send_transaction(
		rpc_endpoint: &str,
		signed_transaction: transaction::SignedTransaction,
	) -> Result<String, http::Error> {
		let deadline = sp_io::offchain::timestamp().add(Duration::from_millis(30_000));
		let args = Self::encode_signed_transaction(signed_transaction).ok_or_else(|| {
			log!(info, "Encode send transaction args error");
			http::Error::Unknown
		})?;

		let mut body = br#"
		{
			"jsonrpc": "2.0",
			"id": "dontcare",
			"method": "broadcast_tx_commit",
			"params": [
				""#
		.to_vec();
		body.extend(&args);
		body.extend(
			br#""
			]
		}"#,
		);

		let request = http::Request::default()
			.method(http::Method::Post)
			.url(rpc_endpoint)
			.body(vec![body])
			.add_header("Content-Type", "application/json");
		log!(debug, "send transaction request: {:?}", request);

		let pending = request.deadline(deadline).send().map_err(|_| http::Error::IoError)?;

		let response = pending.try_wait(deadline).map_err(|_| http::Error::DeadlineReached)??;
		if response.code != 200 {
			log!(warn, "Unexpected status code: {}", response.code);
			return Err(http::Error::Unknown)
		}

		let body = response.body().collect::<Vec<u8>>();
		log!(debug, "Send transaction, response body: {:?}", body);

		let json_response: SendTransactionResponse =
			serde_json::from_slice(&body).map_err(|_| {
				log::warn!("Failed to decode http body when call send_transaction.");
				http::Error::Unknown
			})?;
		log!(debug, "Send transaction, json_response: {:?}", json_response.clone());

		Ok(json_response.result.transaction.hash)
	}

	pub(crate) fn encode_signed_transaction(
		signed_transaction: transaction::SignedTransaction,
	) -> Option<Vec<u8>> {
		if let Ok(signed_tx) = signed_transaction.try_to_vec() {
			let res = base64::encode(&signed_tx).into_bytes();
			Some(res)
		} else {
			None
		}
	}

	pub(super) fn get_latest_commitment_of_appchain(
		rpc_endpoint: &str,
		anchor_contract: Vec<u8>,
	) -> Result<AppchainCommitment, http::Error> {
		// We want to keep the offchain worker execution time reasonable, so we set a hard-coded
		// deadline to 2s to complete the external call.
		// You can also wait idefinitely for the response, however you may still get a timeout
		// coming from the host machine.
		let deadline = sp_io::offchain::timestamp().add(Duration::from_millis(2_000));
		// Initiate an external HTTP GET request.
		// This is using high-level wrappers from `sp_runtime`, for the low-level calls that
		// you can find in `sp_io`. The API is trying to be similar to `reqwest`, but
		// since we are running in a custom WASM execution environment we can't simply
		// import the library here.

		let mut body = br#"
		{
			"jsonrpc": "2.0",
			"id": "dontcare",
			"method": "query",
			"params": {
				"request_type": "call_function",
				"finality": "final",
				"account_id": ""#
			.to_vec();
		body.extend(&anchor_contract);
		body.extend(
			br#"",
				"method_name": "get_latest_commitment_of_appchain",
				"args_base64": ""#,
		);
		body.extend(
			br#""
			}
		}"#,
		);
		let request = http::Request::default()
			.method(http::Method::Post)
			.url(rpc_endpoint)
			.body(vec![body])
			.add_header("Content-Type", "application/json");
		// We set the deadline for sending of the request, note that awaiting response can
		// have a separate deadline. Next we send the request, before that it's also possible
		// to alter request headers or stream body content in case of non-GET requests.
		let pending = request.deadline(deadline).send().map_err(|_| http::Error::IoError)?;

		// The request is already being processed by the host, we are free to do anything
		// else in the worker (we can send multiple concurrent requests too).
		// At some point however we probably want to check the response though,
		// so we can block current thread and wait for it to finish.
		// Note that since the request is being driven by the host, we don't have to wait
		// for the request to have it complete, we will just not read the response.
		let response = pending.try_wait(deadline).map_err(|_| http::Error::DeadlineReached)??;
		// Let's check the status code before we proceed to reading the response.
		if response.code != 200 {
			log!(warn, "Unexpected status code: {}", response.code);
			return Err(http::Error::Unknown)
		}

		// Next we want to fully read the response body and collect it to a vector of bytes.
		// Note that the return object allows you to read the body in chunks as well
		// with a way to control the deadline.
		let body = response.body().collect::<Vec<u8>>();
		log!(debug, "commitment body: {:?}", body);

		let json_response: Response = serde_json::from_slice(&body).map_err(|_| {
			log::warn!("Failed to decode http body");
			http::Error::Unknown
		})?;
		log!(debug, "{:?}", json_response);

		let commitment: AppchainCommitment = serde_json::from_slice(&json_response.result.result)
			.map_err(|_| {
			log!(warn, "Failed to decode appchain commitment");
			http::Error::Unknown
		})?;
		log!(debug, "Got appchain commitment: {:?}", commitment);

		Ok(commitment)
	}
}
