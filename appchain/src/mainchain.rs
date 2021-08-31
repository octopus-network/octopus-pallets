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

impl<T: Config> Pallet<T> {
	/// Fetch the facts of a specified appchain from relay contract.
	pub(super) fn fetch_facts(
		relay_contract: Vec<u8>,
		appchain_id: Vec<u8>,
		start: u32,
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
		let args = Self::encode_args(appchain_id, start, limit).ok_or_else(|| {
			log!(info, "Encode args error");
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
		body.extend(&relay_contract);
		body.extend(
			br#"",
				"method_name": "get_facts",
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
			.url("https://rpc.testnet.near.org")
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
			log!(info, "Unexpected status code: {}", response.code);
			return Err(http::Error::Unknown);
		}

		// Next we want to fully read the response body and collect it to a vector of bytes.
		// Note that the return object allows you to read the body in chunks as well
		// with a way to control the deadline.
		let body = response.body().collect::<Vec<u8>>();
		log!(info, "body: {:?}", body);

		// TODO
		let json_response: Response = serde_json::from_slice(&body).unwrap();
		log!(info, "json_response: {:?}", json_response);

		let obs: Vec<Observation<<T as frame_system::Config>::AccountId>> =
			serde_json::from_slice(&json_response.result.result).unwrap();

		log!(info, "Got observations: {:?}", obs);

		Ok(obs)
	}

	pub(crate) fn encode_args(appchain_id: Vec<u8>, start: u32, limit: u32) -> Option<Vec<u8>> {
		let a = String::from("{\"appchain_id\":\"");
		let appchain_id = sp_std::str::from_utf8(&appchain_id).expect("octopus team will ensure that the appchain_id of a live appchain is a valid UTF8 string; qed");
		let b = String::from("\",\"start\":");
		let start = start.to_string();
		let c = String::from(",\"limit\":");
		let limit = limit.to_string();
		let d = String::from("}");
		let json = a + &appchain_id + &b + &start + &c + &limit + &d;
		let res = base64::encode(json).into_bytes();
		Some(res)
	}
}
