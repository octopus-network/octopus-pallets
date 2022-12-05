use super::*;

#[derive(Serialize, Deserialize, RuntimeDebug, Default)]
pub struct Response {
	jsonrpc: String,
	result: ResponseResult,
	id: String,
}

impl Response {
	pub fn with_jsonrpc(mut self, jsonrpc: impl Into<String>) -> Self {
		self.jsonrpc = jsonrpc.into();
		self
	}

	pub fn with_response_result(mut self, result: ResponseResult) -> Self {
		self.result = result;
		self
	}

	pub fn with_id(mut self, id: impl Into<String>) -> Self {
		self.id = id.into();
		self
	}
}

#[derive(Serialize, Deserialize, RuntimeDebug, Default)]
pub struct ResponseResult {
	result: Vec<u8>,
	logs: Vec<String>,
	block_height: u64,
	block_hash: String,
}

impl ResponseResult {
	pub fn with_result(mut self, value: impl Into<Vec<u8>>) -> Self {
		self.result = value.into();
		self
	}

	pub fn with_logs(mut self, logs: impl Into<Vec<String>>) -> Self {
		self.logs = logs.into();
		self
	}

	pub fn with_block_height(mut self, block_height: u64) -> Self {
		self.block_height = block_height;
		self
	}

	pub fn with_block_hash(mut self, block_hash: impl Into<String>) -> Self {
		self.block_hash = block_hash.into();
		self
	}
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct HttpBody {
	jsonrpc: String,
	id: String,
	method: String,
	params: Params,
}

impl HttpBody {
	pub fn with_jsonrpc(mut self, value: impl Into<String>) -> Self {
		self.jsonrpc = value.into();
		self
	}

	pub fn with_id(mut self, value: impl Into<String>) -> Self {
		self.id = value.into();
		self
	}

	pub fn with_method(mut self, value: impl Into<String>) -> Self {
		self.method = value.into();
		self
	}

	pub fn with_params(mut self, value: Params) -> Self {
		self.params = value;
		self
	}
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Params {
	request_type: String,
	finality: String,
	account_id: Vec<u8>,
	method_name: String,
	args_base64: Vec<u8>,
}

impl Params {
	pub fn with_request_type(mut self, value: impl Into<String>) -> Self {
		self.request_type = value.into();
		self
	}

	pub fn with_finality(mut self, value: impl Into<String>) -> Self {
		self.finality = value.into();
		self
	}

	pub fn with_account_id(mut self, value: impl Into<Vec<u8>>) -> Self {
		self.account_id = value.into();
		self
	}

	pub fn with_method_name(mut self, value: impl Into<String>) -> Self {
		self.method_name = value.into();
		self
	}

	pub fn with_args_base64(mut self, value: impl Into<Vec<u8>>) -> Self {
		self.args_base64 = value.into();
		self
	}
}

impl<T: Config> Pallet<T> {
	/// Gets a validator set by the specified era number.
	/// Returns an empty list if the validator set has not been generated.
	pub(super) fn get_validator_list_of(
		rpc_endpoint: &str,
		anchor_contract: &[u8],
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
		let args = Self::encode_get_validator_args(set_id);

		let params = Params::default()
			.with_request_type("call_function")
			.with_finality("final")
			.with_account_id(anchor_contract)
			.with_method_name("get_validator_list_of")
			.with_args_base64(args);

		let body = HttpBody::default()
			.with_jsonrpc("2.0")
			.with_id("dontcare")
			.with_method("query")
			.with_params(params);

		let body = serde_json::to_string(&body)
			.map_err(|_| {
				log!(warn, "serde http body error");
				http::Error::Unknown
			})?
			.as_bytes()
			.to_vec();

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

	pub(crate) fn encode_get_validator_args(era: u32) -> Vec<u8> {
		let a = String::from("{\"era_number\":\"");
		let era = era.to_string();
		let b = String::from("\"}");
		let json = format!("{}{}{}", a, era, b);
		let res = base64::encode(json).into_bytes();
		res
	}

	/// Fetch the notifications from anchor contract.
	pub(super) fn get_appchain_notification_histories(
		rpc_endpoint: &str,
		anchor_contract: &[u8],
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
		let args = Self::encode_get_notification_args(index, limit);

		let params = Params::default()
			.with_request_type("call_function")
			.with_finality("final")
			.with_account_id(anchor_contract)
			.with_method_name("get_appchain_notification_histories")
			.with_args_base64(args);

		let body = HttpBody::default()
			.with_jsonrpc("2.0")
			.with_id("dontcare")
			.with_method("query")
			.with_params(params);

		let body = serde_json::to_string(&body)
			.map_err(|_| {
				log!(warn, "serde http body error");
				http::Error::Unknown
			})?
			.as_bytes()
			.to_vec();

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

	pub(crate) fn encode_get_notification_args(start: u32, limit: u32) -> Vec<u8> {
		let a = String::from("{\"start_index\":\"");
		let start_index = start.to_string();
		let b = String::from("\",\"quantity\":\"");
		let quantity = limit.to_string();
		let c = String::from("\"}");
		let json = format!("{}{}{}{}{}", a, start_index, b, quantity, c);
		let res = base64::encode(json).into_bytes();
		res
	}
}
