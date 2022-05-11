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
			return Some(public)
		}

		None
	}

	fn format_challenge_args(
		challenge_proof: challenge::AppchainChallenge,
	) -> Result<Vec<u8>, &'static str> {
		let challenge = challenge::Challenge { appchain_challenge: challenge_proof };
		let challenge = serde_json::to_string(&challenge).map_err(|_| "serialize proof error")?;
		log!(debug, "challenge after serialize: {:?}", challenge.clone());

		let args = challenge.as_bytes().to_vec();
		Ok(args)
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

		Self::format_challenge_args(challenge_proof)
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

		let access_key_info = match ret {
			Ok(proof) => proof,
			Err(_) => {
				log!(debug, "retry with failsafe endpoint to get accesskey reponse");
				Self::query_access_key(
					mainchain_bakup_rpc_endpoint,
					account_id.clone(),
					public_key.clone(),
				)
				.map_err(|_| "Failed to query_access_key")?
			},
		};

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
			return Err("No signer found.")
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
		let ret = Self::send_transaction(mainchain_rpc_endpoint, signed_transaction.clone());
		let tx_hash = match ret {
			Ok(hash) => hash,
			Err(_) => {
				log!(debug, "retry with failsafe endpoint to send transaction");
				Self::send_transaction(mainchain_bakup_rpc_endpoint, signed_transaction)
					.map_err(|_| "Failed to send transaction")?
			},
		};

		Ok(tx_hash)
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

	pub(crate) fn local_store_mmr_root_with_number(
		block_number: T::BlockNumber,
		root: sp_core::H256,
	) {
		let kind = sp_core::offchain::StorageKind::PERSISTENT;
		let key = (b"mmr_root_hash", block_number).encode();
		sp_io::offchain::local_storage_set(kind, &key, &root.encode());
	}

	fn get_mmr_root_with_number(
		block_number: T::BlockNumber,
	) -> Result<sp_core::H256, &'static str> {
		let kind = sp_core::offchain::StorageKind::PERSISTENT;
		let key = (b"mmr_root_hash", block_number).encode();

		match sp_io::offchain::local_storage_get(kind, &key) {
			Some(mmr_root) => sp_core::H256::decode(&mut mmr_root.as_slice())
				.map_err(|_| "Get mmr root hash error."),
			None => Err("Get mmr root hash error."),
		}
	}

	pub(crate) fn should_delt(block_number: u32) -> bool {
		let kind = sp_core::offchain::StorageKind::PERSISTENT;
		let key = (b"octopus_appchain::last_delt_mmr_block_number").encode();

		match sp_io::offchain::local_storage_get(kind, &key) {
			Some(data) => match u32::decode(&mut data.as_slice()) {
				Ok(block) =>
					if block_number <= block {
						false
					} else {
						true
					},
				_ => true,
			},
			None => true,
		}
	}

	fn post_delt(block_number: u32) {
		let kind = sp_core::offchain::StorageKind::PERSISTENT;
		let key = (b"octopus_appchain::last_delt_mmr_block_number").encode();
		sp_io::offchain::local_storage_set(kind, &key, &block_number.encode());
	}

	fn construct_mmr_challenge_args(
		benefit_account: String,
		block_number: u32,
	) -> Result<Vec<u8>, &'static str> {
		let challenge_proof = challenge::AppchainChallenge::ConspiracyMmr {
			submitter_account: benefit_account,
			block_number,
		};

		Self::format_challenge_args(challenge_proof)
	}

	pub(crate) fn check_commitment_behavior(
		mainchain_rpc_endpoint: &str,
		mainchain_bakup_rpc_endpoint: &str,
		benefit_account: Vec<u8>,
		submit_account: Vec<u8>,
		anchor_contract: Vec<u8>,
		block_number_in_appchain: T::BlockNumber,
	) -> Result<String, &'static str> {
		// 0. Get benefit account
		let benefit_account =
			String::from_utf8(benefit_account).map_err(|_| "Parse benefit account error")?;

		// 1.Get commitment from mainchain
		let ret = Self::get_latest_commitment_of_appchain(
			mainchain_rpc_endpoint,
			anchor_contract.clone(),
		);
		let commitment = match ret {
			Ok(commit) => commit,
			Err(_) => {
				log!(debug, "retry with failsafe endpoint to get latest commitment");
				Self::get_latest_commitment_of_appchain(
					mainchain_bakup_rpc_endpoint,
					anchor_contract.clone(),
				)
				.map_err(|_| "Failed to get commitment")?
			},
		};

		let (block_number, mmr_root_hash) =
			(commitment.block_number, sp_core::H256::from_slice(&commitment.mmr_root_hash[..]));
		log!(
			debug,
			"Latest commitment from anchor, block_number: {:?}, mmr_root_hash: {:?}",
			block_number,
			mmr_root_hash
		);

		if block_number_in_appchain >= block_number.into() {
			// 2.If has delt, should not delt it again
			if !Self::should_delt(block_number) {
				return Err("This commitment has been delted")
			}

			// 3.Get mmr root hash from appchain
			let hash_from_appchain = Self::get_mmr_root_with_number(block_number.into())?;
			log!(
				debug,
				"hash_from_appchain: {:?}, hash_from_anchor: {:?}",
				hash_from_appchain,
				mmr_root_hash
			);

			// 4.Compare root hash
			if mmr_root_hash == hash_from_appchain {
				return Err(
					"Mmr root from anchor matched witch from appchain, will not send challenge",
				)
			}
		}

		// 5.Construct mmr challenge args
		let args = Self::construct_mmr_challenge_args(benefit_account, block_number.into())?;

		// 6.Submit challenge
		let hash = Self::submit_challenge(
			mainchain_rpc_endpoint,
			mainchain_bakup_rpc_endpoint,
			submit_account,
			anchor_contract,
			args,
		)?;

		// 7.Modify flag
		Self::post_delt(block_number);

		Ok(hash)
	}
}
