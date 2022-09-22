#![cfg(test)]

use super::{
	mock::{
		assert_events, expect_event, new_test_ext, Assets, Balances, Bridge, Call,
		ChainBridgeTransfer, Erc721, Erc721Id, Event, HashId, NativeTokenId, Origin,
		ProposalLifetime, Test, ENDOWED_BALANCE, RELAYER_A, RELAYER_B, RELAYER_C,
	},
	*,
};
use frame_support::{assert_noop, assert_ok, dispatch::DispatchError};
use pallet_assets as assets;

use crate::{
	mock::{event_exists, AccountId, Balance, DOLLARS},
	Event as ChainBridgeTransferEvent,
};
use pallet_chainbridge_erc721::Erc721Token;
use sp_core::{blake2_256, crypto::AccountId32, H256};
use sp_keyring::AccountKeyring;

const TEST_THRESHOLD: u32 = 2;

fn make_remark_proposal(hash: H256) -> Call {
	let resource_id = HashId::get();
	Call::ChainBridgeTransfer(crate::Call::remark { hash, r_id: resource_id })
}

fn make_transfer_proposal(resource_id: ResourceId, to: AccountId32, amount: u64) -> Call {
	Call::ChainBridgeTransfer(crate::Call::transfer {
		to,
		amount: amount.into(),
		r_id: resource_id,
	})
}

#[test]
fn transfer_hash() {
	new_test_ext().execute_with(|| {
		let dest_chain = 0;
		let resource_id = HashId::get();
		let hash: H256 = "ABC".using_encoded(blake2_256).into();

		assert_ok!(Bridge::set_threshold(Origin::root(), TEST_THRESHOLD,));

		assert_ok!(Bridge::whitelist_chain(Origin::root(), dest_chain.clone()));
		assert_ok!(ChainBridgeTransfer::transfer_hash(
			Origin::signed(AccountId32::new([1u8; 32])),
			hash.clone(),
			dest_chain,
		));

		expect_event(bridge::Event::GenericTransfer(
			dest_chain,
			1,
			resource_id,
			hash.as_ref().to_vec(),
		));
	})
}

#[test]
fn transfer_native() {
	new_test_ext().execute_with(|| {
		let dest_chain = 0;
		let resource_id = NativeTokenId::get();
		let amount: Balance = 1 * DOLLARS;
		let recipient = b"davirain.xyz".to_vec(); // recipient account

		assert_ok!(Bridge::whitelist_chain(Origin::root(), dest_chain.clone()));
		assert_ok!(ChainBridgeTransfer::transfer_native(
			Origin::signed(RELAYER_A),
			amount.clone(),
			recipient.clone(),
			dest_chain,
		));

		expect_event(bridge::Event::FungibleTransfer(
			dest_chain,
			1,
			resource_id,
			amount.into(),
			recipient,
		));
	})
}

#[test]
fn transfer_erc721() {
	new_test_ext().execute_with(|| {
		let dest_chain = 0;
		let resource_id = Erc721Id::get();
		let token_id: U256 = U256::from(100);
		let token_id_slice: &mut [u8] = &mut [0; 32];
		token_id.to_big_endian(token_id_slice);
		let metadata: Vec<u8> = vec![1, 2, 3, 4];
		let recipient = vec![99];

		// Create a token
		assert_ok!(Erc721::mint(Origin::root(), RELAYER_A, token_id, metadata.clone()));
		assert_eq!(
			Erc721::tokens(token_id).unwrap(),
			Erc721Token { id: token_id, metadata: metadata.clone() }
		);

		// Whitelist destination and transfer
		assert_ok!(Bridge::whitelist_chain(Origin::root(), dest_chain.clone()));
		assert_ok!(ChainBridgeTransfer::transfer_erc721(
			Origin::signed(RELAYER_A),
			recipient.clone(),
			token_id,
			dest_chain,
		));

		expect_event(bridge::Event::NonFungibleTransfer(
			dest_chain,
			1,
			resource_id,
			token_id_slice.to_vec(),
			recipient.clone(),
			metadata,
		));

		// Ensure token no longer exists
		assert_eq!(Erc721::tokens(token_id), None);

		// Transfer should fail as token doesn't exist
		assert_noop!(
			ChainBridgeTransfer::transfer_erc721(
				Origin::signed(RELAYER_A),
				recipient.clone(),
				token_id,
				dest_chain,
			),
			Error::<Test>::InvalidTransfer
		);
	})
}

#[test]
fn mint_erc721() {
	new_test_ext().execute_with(|| {
		let token_id = U256::from(99);
		let recipient = RELAYER_A;
		let metadata = vec![1, 1, 1, 1];
		let bridge_id: AccountId32 = Bridge::account_id();
		let resource_id = HashId::get();
		// Token doesn't yet exist
		assert_eq!(Erc721::tokens(token_id), None);
		// Mint
		assert_ok!(ChainBridgeTransfer::mint_erc721(
			Origin::signed(bridge_id.clone()),
			recipient.clone(),
			token_id,
			metadata.clone(),
			resource_id,
		));
		// Ensure token exists
		assert_eq!(
			Erc721::tokens(token_id).unwrap(),
			Erc721Token { id: token_id, metadata: metadata.clone() }
		);
		// Cannot mint same token
		assert_noop!(
			ChainBridgeTransfer::mint_erc721(
				Origin::signed(bridge_id),
				recipient,
				token_id,
				metadata.clone(),
				resource_id,
			),
			erc721::Error::<Test>::TokenAlreadyExists
		);
	})
}

#[test]
fn transfer_non_native() {
	new_test_ext().execute_with(|| {
		let dest_chain = 0;
		// get resource id
		let resource_id = bridge::derive_resource_id(dest_chain, b"DENOM");
		let ferdie: AccountId = AccountKeyring::Ferdie.into();
		let recipient = vec![99];
		// set token_id
		assert_ok!(ChainBridgeTransfer::set_token_id(
			Origin::root(),
			resource_id.clone(),
			0,
			b"DENOM".to_vec()
		));

		// force_create Assets token_id 0
		assert_ok!(Assets::force_create(
			Origin::root(),
			0,
			sp_runtime::MultiAddress::Id(ferdie.clone()),
			true,
			1
		));

		let amount: Balance = 1 * DOLLARS;
		assert_ok!(Assets::mint(
			Origin::signed(ferdie.clone()),
			0,
			sp_runtime::MultiAddress::Id(ferdie.clone()),
			amount
		));

		// make sure have some  amount after mint
		assert_eq!(Assets::balance(0, ferdie.clone()), amount);

		assert_ok!(Bridge::whitelist_chain(Origin::root(), dest_chain.clone()));
		assert_ok!(ChainBridgeTransfer::generic_token_transfer(
			Origin::signed(ferdie.clone()),
			amount,
			resource_id,
			recipient.clone(),
			dest_chain,
		));

		// maket sure transfer have 0 amount
		assert_eq!(Assets::balance(0, ferdie), 0);

		assert_events(vec![Event::Bridge(bridge::Event::FungibleTransfer(
			0,
			1,
			resource_id,
			U256::from(amount),
			recipient,
		))]);
	})
}

#[test]
fn transfer() {
	new_test_ext().execute_with(|| {
		// Check inital state
		let bridge_id: AccountId32 = Bridge::account_id();
		let resource_id = NativeTokenId::get();
		assert_eq!(Balances::free_balance(&bridge_id), ENDOWED_BALANCE);
		// Transfer and check result
		assert_ok!(ChainBridgeTransfer::transfer(
			Origin::signed(Bridge::account_id()),
			RELAYER_A,
			10,
			resource_id,
		));
		assert_eq!(Balances::free_balance(&bridge_id), ENDOWED_BALANCE - 10);
		assert_eq!(Balances::free_balance(RELAYER_A), ENDOWED_BALANCE + 10);

		assert_events(vec![Event::Balances(pallet_balances::Event::Transfer {
			from: Bridge::account_id(),
			to: RELAYER_A,
			amount: 10,
		})]);
	})
}

#[test]
fn execute_remark() {
	new_test_ext().execute_with(|| {
		let hash: H256 = "ABC".using_encoded(blake2_256).into();
		let proposal = make_remark_proposal(hash.clone());
		let prop_id = 1;
		let src_id = 1;
		let r_id = bridge::derive_resource_id(src_id, b"hash");
		let resource = b"Example.remark".to_vec();

		assert_ok!(Bridge::set_threshold(Origin::root(), TEST_THRESHOLD,));
		assert_ok!(Bridge::add_relayer(Origin::root(), RELAYER_A));
		assert_ok!(Bridge::add_relayer(Origin::root(), RELAYER_B));
		assert_ok!(Bridge::whitelist_chain(Origin::root(), src_id));
		assert_ok!(Bridge::set_resource(Origin::root(), r_id, resource));

		assert_ok!(Bridge::acknowledge_proposal(
			Origin::signed(RELAYER_A),
			prop_id,
			src_id,
			r_id,
			Box::new(proposal.clone())
		));
		assert_ok!(Bridge::acknowledge_proposal(
			Origin::signed(RELAYER_B),
			prop_id,
			src_id,
			r_id,
			Box::new(proposal.clone())
		));

		event_exists(ChainBridgeTransferEvent::Remark(hash));
	})
}

#[test]
fn execute_remark_bad_origin() {
	new_test_ext().execute_with(|| {
		let hash: H256 = "ABC".using_encoded(blake2_256).into();
		let resource_id = HashId::get();
		assert_ok!(ChainBridgeTransfer::remark(
			Origin::signed(Bridge::account_id()),
			hash,
			resource_id
		));
		// Don't allow any signed origin except from bridge addr
		assert_noop!(
			ChainBridgeTransfer::remark(Origin::signed(RELAYER_A), hash, resource_id),
			DispatchError::BadOrigin
		);
		// Don't allow root calls
		assert_noop!(
			ChainBridgeTransfer::remark(Origin::root(), hash, resource_id),
			DispatchError::BadOrigin
		);
	})
}

#[test]
fn create_sucessful_transfer_proposal_non_native_token() {
	new_test_ext().execute_with(|| {
		let prop_id = 1;
		let src_id = 1;
		let r_id = bridge::derive_resource_id(src_id, b"transfer");
		let resource = b"ChainBridgeTransfer.transfer".to_vec();
		// let resource_id = NativeTokenId::get();
		let resource_id = bridge::derive_resource_id(src_id, b"DENOM");
		let proposal = make_transfer_proposal(resource_id, RELAYER_A, 10);
		let ferdie: AccountId = AccountKeyring::Ferdie.into();
		// set token_id
		assert_ok!(ChainBridgeTransfer::set_token_id(
			Origin::root(),
			resource_id.clone(),
			0,
			b"DENOM".to_vec()
		));

		// force_create Assets token_id 0
		assert_ok!(Assets::force_create(
			Origin::root(),
			0,
			sp_runtime::MultiAddress::Id(ferdie.clone()),
			true,
			1
		));

		assert_ok!(Bridge::set_threshold(Origin::root(), TEST_THRESHOLD,));
		assert_ok!(Bridge::add_relayer(Origin::root(), RELAYER_A));
		assert_ok!(Bridge::add_relayer(Origin::root(), RELAYER_B));
		assert_ok!(Bridge::add_relayer(Origin::root(), RELAYER_C));
		assert_ok!(Bridge::whitelist_chain(Origin::root(), src_id));
		assert_ok!(Bridge::set_resource(Origin::root(), r_id, resource));

		// Create proposal (& vote)
		assert_ok!(Bridge::acknowledge_proposal(
			Origin::signed(RELAYER_A),
			prop_id,
			src_id,
			r_id,
			Box::new(proposal.clone())
		));
		let prop = Bridge::votes(src_id, (prop_id.clone(), proposal.clone())).unwrap();
		let expected = bridge::ProposalVotes {
			votes_for: vec![RELAYER_A],
			votes_against: vec![],
			status: bridge::ProposalStatus::Initiated,
			expiry: ProposalLifetime::get() + 1,
		};
		assert_eq!(prop, expected);

		// Second relayer votes against
		assert_ok!(Bridge::reject_proposal(
			Origin::signed(RELAYER_B),
			prop_id,
			src_id,
			r_id,
			Box::new(proposal.clone())
		));
		let prop = Bridge::votes(src_id, (prop_id.clone(), proposal.clone())).unwrap();
		let expected = bridge::ProposalVotes {
			votes_for: vec![RELAYER_A],
			votes_against: vec![RELAYER_B],
			status: bridge::ProposalStatus::Initiated,
			expiry: ProposalLifetime::get() + 1,
		};
		assert_eq!(prop, expected);

		// Third relayer votes in favour
		assert_ok!(Bridge::acknowledge_proposal(
			Origin::signed(RELAYER_C),
			prop_id,
			src_id,
			r_id,
			Box::new(proposal.clone())
		));
		let prop = Bridge::votes(src_id, (prop_id.clone(), proposal.clone())).unwrap();
		let expected = bridge::ProposalVotes {
			votes_for: vec![RELAYER_A, RELAYER_C],
			votes_against: vec![RELAYER_B],
			status: bridge::ProposalStatus::Approved,
			expiry: ProposalLifetime::get() + 1,
		};
		assert_eq!(prop, expected);

		// mint 10 resource_id to RELAYER_A
		assert_eq!(Assets::balance(0, RELAYER_A), 10);

		assert_events(vec![
			Event::Bridge(bridge::Event::VoteFor(src_id, prop_id, RELAYER_A)),
			Event::Bridge(bridge::Event::VoteAgainst(src_id, prop_id, RELAYER_B)),
			Event::Bridge(bridge::Event::VoteFor(src_id, prop_id, RELAYER_C)),
			Event::Bridge(bridge::Event::ProposalApproved(src_id, prop_id)),
			Event::Assets(assets::Event::Issued {
				asset_id: 0,
				owner: RELAYER_A,
				total_supply: 10,
			}),
			Event::Bridge(bridge::Event::ProposalSucceeded(src_id, prop_id)),
		]);
	})
}

#[test]
fn create_sucessful_transfer_proposal_native_token() {
	new_test_ext().execute_with(|| {
		let prop_id = 1;
		let src_id = 1;
		let r_id = bridge::derive_resource_id(src_id, b"transfer");
		let resource = b"ChainBridgeTransfer.transfer".to_vec();
		let resource_id = NativeTokenId::get();
		let proposal = make_transfer_proposal(resource_id, RELAYER_A, 10);

		assert_ok!(Bridge::set_threshold(Origin::root(), TEST_THRESHOLD));
		assert_ok!(Bridge::add_relayer(Origin::root(), RELAYER_A));
		assert_ok!(Bridge::add_relayer(Origin::root(), RELAYER_B));
		assert_ok!(Bridge::add_relayer(Origin::root(), RELAYER_C));
		assert_ok!(Bridge::whitelist_chain(Origin::root(), src_id));
		assert_ok!(Bridge::set_resource(Origin::root(), r_id, resource));

		// Create proposal (& vote)
		assert_ok!(Bridge::acknowledge_proposal(
			Origin::signed(RELAYER_A),
			prop_id,
			src_id,
			r_id,
			Box::new(proposal.clone())
		));
		let prop = Bridge::votes(src_id, (prop_id.clone(), proposal.clone())).unwrap();
		let expected = bridge::ProposalVotes {
			votes_for: vec![RELAYER_A],
			votes_against: vec![],
			status: bridge::ProposalStatus::Initiated,
			expiry: ProposalLifetime::get() + 1,
		};
		assert_eq!(prop, expected);

		// Second relayer votes against
		assert_ok!(Bridge::reject_proposal(
			Origin::signed(RELAYER_B),
			prop_id,
			src_id,
			r_id,
			Box::new(proposal.clone())
		));
		let prop = Bridge::votes(src_id, (prop_id.clone(), proposal.clone())).unwrap();
		let expected = bridge::ProposalVotes {
			votes_for: vec![RELAYER_A],
			votes_against: vec![RELAYER_B],
			status: bridge::ProposalStatus::Initiated,
			expiry: ProposalLifetime::get() + 1,
		};
		assert_eq!(prop, expected);

		// Third relayer votes in favour
		assert_ok!(Bridge::acknowledge_proposal(
			Origin::signed(RELAYER_C),
			prop_id,
			src_id,
			r_id,
			Box::new(proposal.clone())
		));
		let prop = Bridge::votes(src_id, (prop_id.clone(), proposal.clone())).unwrap();
		let expected = bridge::ProposalVotes {
			votes_for: vec![RELAYER_A, RELAYER_C],
			votes_against: vec![RELAYER_B],
			status: bridge::ProposalStatus::Approved,
			expiry: ProposalLifetime::get() + 1,
		};
		assert_eq!(prop, expected);

		assert_eq!(Balances::free_balance(RELAYER_A), ENDOWED_BALANCE + 10);
		assert_eq!(Balances::free_balance(Bridge::account_id()), ENDOWED_BALANCE - 10);

		assert_events(vec![
			Event::Bridge(bridge::Event::VoteFor(src_id, prop_id, RELAYER_A)),
			Event::Bridge(bridge::Event::VoteAgainst(src_id, prop_id, RELAYER_B)),
			Event::Bridge(bridge::Event::VoteFor(src_id, prop_id, RELAYER_C)),
			Event::Bridge(bridge::Event::ProposalApproved(src_id, prop_id)),
			Event::Balances(pallet_balances::Event::Transfer {
				from: Bridge::account_id(),
				to: RELAYER_A,
				amount: 10,
			}),
			Event::Bridge(bridge::Event::ProposalSucceeded(src_id, prop_id)),
		]);
	})
}
