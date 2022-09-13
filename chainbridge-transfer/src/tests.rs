#![cfg(test)]

use super::{
	mock::{
		assert_events, event_exists, expect_event, new_test_ext, Balances, Bridge, Call,
		Event, ChainBridgeTransfer, HashId, NativeTokenId, Origin, ProposalLifetime,
		ENDOWED_BALANCE, RELAYER_A, RELAYER_B, RELAYER_C,
	},
	*,
};
use frame_support::{assert_noop, assert_ok, dispatch::DispatchError};

use crate::{
	mock::{Balance, DOLLARS},
	Event as ChainBridgeTransferEvent,
};
use codec::Encode;
use sp_core::{blake2_256, crypto::AccountId32, H256};

const TEST_THRESHOLD: u32 = 2;

fn make_remark_proposal(hash: H256) -> Call {
	let resource_id = HashId::get();
	Call::ChainBridgeTransfer(crate::Call::remark { hash, r_id: resource_id })
}

fn make_transfer_proposal(to: AccountId32, amount: u64) -> Call {
	let resource_id = HashId::get();
	Call::ChainBridgeTransfer(crate::Call::transfer { to, amount: amount.into(), r_id: resource_id })
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
		let recipient = vec![99];

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
fn execute_remark() {
	new_test_ext().execute_with(|| {
		let hash: H256 = "ABC".using_encoded(blake2_256).into();
		let proposal = make_remark_proposal(hash.clone());
		let prop_id = 1;
		let src_id = 1;
		let r_id = bridge::derive_resource_id(src_id, b"hash");
		let resource = b"ChainBridgeTransfer.remark".to_vec();

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
		assert_ok!(ChainBridgeTransfer::remark(Origin::signed(Bridge::account_id()), hash, resource_id));
		// Don't allow any signed origin except from bridge addr
		assert_noop!(
			ChainBridgeTransfer::remark(Origin::signed(RELAYER_A), hash, resource_id),
			DispatchError::BadOrigin
		);
		// Don't allow root calls
		assert_noop!(ChainBridgeTransfer::remark(Origin::root(), hash, resource_id), DispatchError::BadOrigin);
	})
}

#[test]
fn transfer() {
	new_test_ext().execute_with(|| {
		// Check inital state
		let bridge_id: AccountId32 = Bridge::account_id();
		let resource_id = HashId::get();
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
fn create_sucessful_transfer_proposal() {
	new_test_ext().execute_with(|| {
		let prop_id = 1;
		let src_id = 1;
		let r_id = bridge::derive_resource_id(src_id, b"transfer");
		let resource = b"ChainBridgeTransfer.transfer".to_vec();
		let proposal = make_transfer_proposal(RELAYER_A, 10);

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