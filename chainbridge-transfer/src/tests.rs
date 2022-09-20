#![cfg(test)]

use super::{
	mock::{
		assert_events, expect_event, new_test_ext, Assets, Balances, Bridge, Call,
		ChainBridgeTransfer, Event, NativeTokenId, Origin, ProposalLifetime,
		ENDOWED_BALANCE, RELAYER_A, RELAYER_B, RELAYER_C,
	},
	*,
};
use frame_support::assert_ok;
use pallet_assets as assets;

use crate::{
	mock::{AccountId, Balance, DOLLARS}
};
use sp_core::crypto::AccountId32;
use sp_keyring::AccountKeyring;

const TEST_THRESHOLD: u32 = 2;


fn make_transfer_proposal(resource_id: ResourceId, to: AccountId32, amount: u64) -> Call {
	Call::ChainBridgeTransfer(crate::Call::transfer {
		to,
		amount: amount.into(),
		r_id: resource_id,
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
fn transfer_non_native() {
	new_test_ext().execute_with(|| {
		let dest_chain = 0;
		// get resource id
		let resource_id = bridge::derive_resource_id(dest_chain, b"DENOM");
		let ferdie: AccountId = AccountKeyring::Ferdie.into();
		let recipient = vec![99];
		// set token_id
		assert_ok!(ChainBridgeTransfer::set_token_id(Origin::root(), resource_id.clone(), 0, b"DENOM".to_vec()));

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
		assert_ok!(ChainBridgeTransfer::set_token_id(Origin::root(), resource_id.clone(), 0, b"DENOM".to_vec()));

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
