use super::*;

impl<T: Config> Pallet<T> {
	pub(crate) fn derived_key() -> Vec<u8> {
		b"octo::appchain::equivocation".encode()
	}

	pub(crate) fn get_equivocation_proof() -> Result<Vec<u8>, &'static str> {
		let key = Self::derived_key();
		let equivocation_proof_storage = StorageValueRef::persistent(&key);

		if let Ok(Some(data)) = equivocation_proof_storage.get::<Vec<u8>>() {
			log!(debug, "Get proof data = {:?}", data.clone());
			Ok(data)
		} else {
			log!(debug, "No proof get.");
			Err("No equivocation proof need to submit")
		}
	}

	fn get_public() -> Option<<T as SigningTypes>::Public> {
		for key in <T::MainchainAuthorityId as AppCrypto<
			<T as SigningTypes>::Public,
			<T as SigningTypes>::Signature,
		>>::RuntimeAppPublic::all()
		.into_iter()
		{
			let generic_public = <T::MainchainAuthorityId as AppCrypto<
				<T as SigningTypes>::Public,
				<T as SigningTypes>::Signature,
			>>::GenericPublic::from(key);
			let public: <T as SigningTypes>::Public = generic_public.into();
			return Some(public);
		}

		None
	}

	fn construct_equivocation_challenge_args(
		benefit_account: String,
		proof: Vec<u8>,
	) -> Result<Vec<u8>, &'static str> {
		let proof = equivocation_proof::EquivocationProof::decode(&mut proof.as_slice())
			.map_err(|_| "parse equivocation error")?;
		log!(debug, "proof after decode : {:?}", proof.clone());

		let challenge_proof = challenge::AppchainChallenge::EquivocationChallenge {
			submitter_account: benefit_account,
			proof,
		};

		let challenge = challenge::Challenge { appchain_challenge: challenge_proof };
		let challenge = serde_json::to_string(&challenge).map_err(|_| "serialize proof error")?;
		log!(debug, "challenge after serialize: {:?}", challenge.clone());

		let args = challenge.as_bytes().to_vec();
		Ok(args)
	}

	fn submit_challenge(
		mainchain_rpc_endpoint: &str,
		mainchain_bakup_rpc_endpoint: &str,
		account_id: Vec<u8>,
		anchor_contract: Vec<u8>,
		challenge_args: Vec<u8>,
	) -> Result<String, &'static str> {
		// 1. Get key
		let public = Self::get_public()
			.ok_or(0)
			.map_err(|_| "No key used for report challenge to mainchain.")?;
		let public_key: [u8; 32] = public.clone().encode()[1..]
			.try_into()
			.map_err(|_| "Error, Public key length is not 32")?;

		// 2. Query access key
		let ret =
			Self::query_access_key(mainchain_rpc_endpoint, account_id.clone(), public_key.clone());

		let access_key_info: mainchain::AccessKeyResult;
		match ret {
			Ok(proof) => {
				access_key_info = proof;
			},
			Err(_) => {
				log!(debug, "retry with failsafe endpoint to get accesskey reponse");
				access_key_info = Self::query_access_key(
					mainchain_bakup_rpc_endpoint,
					account_id.clone(),
					public_key.clone(),
				)
				.map_err(|_| "Failed to query_access_key")?;
			},
		}

		// 3. Create transaction
		let action = transaction::Action::FunctionCall(transaction::FunctionCallAction {
			method_name: "commit_appchain_challenge".to_string(),
			args: challenge_args,
			gas: 200_000_000_000_000, // 200T
			deposit: 0,
		});
		let nonce = access_key_info.nonce + 1;
		let receiver_id = String::from_utf8(anchor_contract.clone())
			.map_err(|_| "Parse anchor_contract error")?;
		let signer_id = String::from_utf8(account_id).map_err(|_| "Parse account_id error")?;
		let block_hash =
			access_key_info.block_hash.from_base58().map_err(|_| "Parse block hash error")?;
		let block_hash: [u8; 32] = block_hash.encode()[1..]
			.try_into()
			.map_err(|_| "Error, Public key length is not 32")?;
		log!(debug, "Block hash is: {:?}", block_hash.clone());

		let transaction = transaction::Transaction {
			signer_id,
			public_key: transaction::PublicKey(public_key),
			nonce,
			receiver_id,
			block_hash,
			actions: vec![action],
		};

		// 4. Get Signer
		let signer = Signer::<T, T::MainchainAuthorityId>::all_accounts().with_filter(vec![public]);
		if !(signer.can_sign()) {
			log!(warn, "Err, no signer found when submit challenge.");
			return Err("No signer found.");
		}

		// 5. Sign transaction
		let transaction_serialize =
			transaction.try_to_vec().map_err(|_| "Format transaction to string error.")?;
		log!(debug, "transaction_serialize: {:?}", transaction_serialize);

		let tx_hash = sp_io::hashing::sha2_256(&transaction_serialize);
		let sig = signer.sign_message(&tx_hash);
		let sig: [u8; 64] = sig[0].1.encode()[1..]
			.try_into()
			.map_err(|_| "Error, Signature length is not 64")?;
		log!(debug, "signature: {:?}", sig.clone());

		let signed_transaction =
			transaction::SignedTransaction { transaction, signature: transaction::Signature(sig) };

		// 6. Send transaction
		Self::send_transaction(mainchain_rpc_endpoint, signed_transaction)
			.map_err(|_| "Failed to send transaction")
	}

	pub(crate) fn submit_equivocation(
		mainchain_rpc_endpoint: &str,
		mainchain_bakup_rpc_endpoint: &str,
		benefit_account: Vec<u8>,
		submit_account: Vec<u8>,
		anchor_contract: Vec<u8>,
		equivocation_proof: Vec<u8>,
	) -> Result<String, &'static str> {
		// 0. Get benefit account
		let benefit_account =
			String::from_utf8(benefit_account).map_err(|_| "Parse benefit account error")?;

		// 1. Construct equivocation challenge
		let args =
			Self::construct_equivocation_challenge_args(benefit_account, equivocation_proof)?;
		log!(debug, "args after serialize: {:?}", args.clone());

		// 2. submit_challenge
		let hash = Self::submit_challenge(
			mainchain_rpc_endpoint,
			mainchain_bakup_rpc_endpoint,
			submit_account,
			anchor_contract,
			args,
		)?;

		// 3. Clear local storage
		let key = Self::derived_key();
		StorageValueRef::persistent(&key).clear();

		Ok(hash)
	}
}
