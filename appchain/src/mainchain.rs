use super::*;

#[derive(Deserialize, RuntimeDebug)]
struct Response {
	jsonrpc: String,
	result: ResponseResult,
	id: String,
}

#[derive(Deserialize, RuntimeDebug)]
struct ResponseResult {
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
}
