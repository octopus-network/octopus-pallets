#![cfg_attr(not(feature = "std"), no_std)]
#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(unused_variables)]

use codec::{Decode, Encode};
use frame_support::{
    dispatch::DispatchError,
    ensure,
    traits::{EnsureOrigin, Get, StorageVersion},
    PalletId,
};
use pallet_octopus_support::log;
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::{
    traits::{AccountIdConversion, Dispatchable, Hash, Keccak256},
    DigestItem, RuntimeDebug,
};
use sp_std::prelude::*;

pub use pallet::*;

pub(crate) const LOG_TARGET: &'static str = "runtime::chainbridge";

#[cfg(test)]
mod tests;

mod benchmarking;

const DEFAULT_RELAYER_THRESHOLD: u32 = 1;
const MODULE_ID: PalletId = PalletId(*b"oc/bridg");

pub type ChainId = u8;
pub type DepositNonce = u64;
pub type ResourceId = [u8; 32];

pub fn derive_resource_id(chain: u8, id: &[u8]) -> ResourceId {
    let mut r_id: ResourceId = [0; 32];
    r_id[31] = chain; // last byte is chain id
    let range = if id.len() > 31 { 31 } else { id.len() }; // Use at most 31 bytes
    for i in 0..range {
        r_id[30 - i] = id[range - 1 - i]; // Ensure left padding for eth compatibilit
    }

    r_id
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum ProposalStatus {
    Initiated,
    Approved,
    Rejected,
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct ProposalVotes<AccountId, BlockNumber> {
    pub votes_for: Vec<AccountId>,
    pub votes_against: Vec<AccountId>,
    pub status: ProposalStatus,
    pub expiry: BlockNumber,
}

impl<AccountId: PartialEq, BlockNumber: PartialOrd + Default>
    ProposalVotes<AccountId, BlockNumber>
{
    /// Attempts to mark the proposal as approve or rejected.
    /// Returns true if the status changes from active.
    fn try_to_complete(&mut self, threshold: u32, total: u32) -> ProposalStatus {
        if threshold as usize <= self.votes_for.len() {
            self.status = ProposalStatus::Approved;
            ProposalStatus::Approved
        } else if threshold <= total && total < self.votes_against.len() as u32 + threshold {
            self.status = ProposalStatus::Rejected;
            ProposalStatus::Rejected
        } else {
            ProposalStatus::Initiated
        }
    }

    /// Returns true if the prorosal has been rejected or approved, otherwise false.
    fn is_complete(&self) -> bool {
        self.status != ProposalStatus::Initiated
    }

    /// Returns true if `who` has voted for or against the proposal
    fn hash_voted(&self, who: &AccountId) -> bool {
        self.votes_for.contains(&who) || self.votes_against.contains(&who)
    }

    /// Return true is the expiry time has been reached
    fn is_expired(&self, now: BlockNumber) -> bool {
        self.expiry <= now
    }
}

impl<AccountId, BlockNumber: Default> Default for ProposalVotes<AccountId, BlockNumber> {
    fn default() -> Self {
        Self {
            votes_for: vec![],
            votes_against: vec![],
            status: ProposalStatus::Initiated,
            expiry: BlockNumber::default(),
        }
    }
}

/// The current storage version.
const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

#[allow(dead_code)]
#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use codec::EncodeLike;
    use frame_support::{pallet_prelude::*, weights::GetDispatchInfo, Blake2_128Concat};
    use frame_system::pallet_prelude::*;

    /// Configure the pallet by specifying the parameters and types on which it depends.
    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The overarching event type.
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

        /// Origin used to administer the pallet
        type AdminOrigin: EnsureOrigin<Self::Origin>;

        /// Proposed dispatchable call
        type Proposal: Parameter
            + Dispatchable<Origin = Self::Origin>
            + EncodeLike
            + GetDispatchInfo;

        /// This identifier for this chain.
        /// This must be unique and must not collide with existing IDs within a set of bridged chains.
        type ChainId: Get<ChainId>;

        type ProposalLifetime: Get<Self::BlockNumber>;
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    #[pallet::without_storage_info]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub type ChainNonces<T: Config> =
        StorageMap<_, Blake2_128Concat, ChainId, Option<DepositNonce>>;

    #[pallet::storage]
    pub type RelayerCount<T: Config> = StorageValue<_, u32>;

    #[pallet::storage]
    pub type Votes<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        ChainId,
        Blake2_128Concat,
        (DepositNonce, T::Proposal),
        Option<ProposalVotes<T::AccountId, T::BlockNumber>>,
    >;

    #[pallet::storage]
    pub type Resources<T: Config> = StorageMap<_, Blake2_128Concat, ResourceId, Option<Vec<u8>>>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Vote threshold has changed (new_threshold)
        RelayerThresholdChanged(u32),
        /// Chain now available for transfers (chain_id)
        ChainWhitelisted(ChainId),
        /// Relayer added to set
        RelayerAddedT(T::AccountId),
        /// Relayer removed from set
        RelayerRemoved(T::AccountId),
        /// FunglibleTransfer is for relaying fungibles (dest_id, nonce, resource_id, amount, recipient, metadata)
        FougibleTransfer(ChainId, DepositNonce, ResourceId, H256, Vec<u8>),
        /// NonFungibleTransfer is for relaying NFTS (dest_id, nonce, resource_id, token_id, recipient, metadata)
        NonFougibleTransfer(ChainId, DepositNonce, ResourceId, H256, Vec<u8>),
        /// GenericTransfer is for a generic data payload(dest_id, nonce, resource_id, metadata)
        GenericTransfer(ChainId, DepositNonce, ResourceId, Vec<u8>),
        /// Vote submitted in favour of proposal
        VoteFor(ChainId, DepositNonce, T::AccountId),
        /// Vote submitted against proposal
        VoteAgainst(ChainId, DepositNonce, T::AccountId),
        /// Voting successful for a proposal
        ProposalApproved(ChainId, DepositNonce),
        /// Voting rejected a proposal
        ProposalRejected(ChainId, DepositNonce),
        /// Execution of call succeeded
        ProposalSucceeded(ChainId, DepositNonce),
        /// Execution of call failed
        ProposalFailed(ChainId, DepositNonce),
    }

    // Errors inform users that something went wrong.
    #[pallet::error]
    pub enum Error<T> {
        /// Relayer threshold not set
        ThresholdNotSet,
        /// Provide chain Id is not valid
        InvalidChainId,
        /// Relayer threshold cannot be 0
        InvalidThreshold,
        /// Interactions with this chai is not permitted
        ChainNotWhitelisted,
        /// Chain has already been enabled
        ChainAlreadyWhitelisted,
        /// Resource ID provided isn't mapped to anything
        ResourceDoesNotExist,
        /// Relayer already in set
        RelayerAlreadyExists,
        /// Provided accountId is not a relayer
        RelayerInvalid,
        /// Protected operation, must be performed by relayer
        MustBeRelayer,
        /// Relayer has already submitted some vote for this proposal
        RelayerAlreadyVoted,
        /// A proposal with these parameters has already been submitted
        ProposalAlreadyExists,
        /// No proposal with the ID was found
        ProposalDoesNotExist,
        /// Cannot complete proposal, needs more votes
        ProposalNotComplete,
        /// Proposal has either failed or succeeded
        ProposalAlreadyComplete,
        /// Lifetime of proposal has been exceeded
        ProposalExpired,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    // Dispatchable functions allows users to interact with the pallet and invoke state changes.
    // These functions materialize as "extrinsics", which are often compared to transactions.
    // Dispatchable functions must be annotated with a weight and must return a DispatchResult.
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(195_000_0000)]
        pub fn set_threshold(origin: OriginFor<T>, threshold: u32) -> DispatchResult {
            todo!()
        }

        #[pallet::weight(195_000_0000)]
        pub fn set_resource(
            origin: OriginFor<T>,
            id: ResourceId,
            method: Vec<u8>,
        ) -> DispatchResult {
            todo!()
        }

        #[pallet::weight(195_000_0000)]
        pub fn whitelist_chain(origin: OriginFor<T>, id: ChainId) -> DispatchResult {
            todo!()
        }

        #[pallet::weight(195_000_0000)]
        pub fn add_relayer(origin: OriginFor<T>, v: T::AccountId) -> DispatchResult {
            todo!()
        }

        #[pallet::weight(195_000_0000)]
        pub fn remove_relayer(origin: OriginFor<T>, v: T::AccountId) -> DispatchResult {
            todo!()
        }

        // TODO: need handle weight
        #[pallet::weight(195_000_0000)]
        pub fn acknowledge_proposal(
            origin: OriginFor<T>,
            nonce: DepositNonce,
            src_id: ChainId,
            r_id: ResourceId,
            call: Box<<T as Config>::Proposal>,
        ) -> DispatchResult {
            todo!()
        }

        #[pallet::weight(195_000_0000)]
        pub fn reject_proposal(
            origin: OriginFor<T>,
            nonce: DepositNonce,
            src_id: ChainId,
            r_id: ResourceId,
            call: Box<<T as Config>::Proposal>,
        ) -> DispatchResult {
            todo!()
        }

        #[pallet::weight(195_000_0000)]
        pub fn eval_vote_state(
            origin: OriginFor<T>,
            nonce: DepositNonce,
            src_id: ChainId,
            r_id: ResourceId,
            call: Box<<T as Config>::Proposal>,
        ) -> DispatchResult {
            todo!()
        }
    }

    impl<T: Config> Pallet<T> {
        // *** Utility methods ***

        pub fn ensure_admin(o: T::Origin) -> DispatchResult {
            todo!()
        }

        pub fn is_relayer(who: &T::AccountId) -> bool {
            todo!()
        }

        pub fn account_id() -> T::AccountId {
            todo!()
        }

        pub fn resource_exists(id: ResourceId) -> bool {
            todo!()
        }

        pub fn chain_whitelisted(id: ChainId) -> bool {
            todo!()
        }

        fn bump_nonce(id: ChainId) -> DispatchResult {
            todo!()
        }

        // *** Admin methods ***

        pub fn set_relayer_threshold(threshold: u32) -> DispatchResult {
            todo!()
        }

        pub fn register_resource(id: ResourceId, method: Vec<u8>) -> DispatchResult {
            todo!()
        }

        pub fn unregister_resource(id: ResourceId) -> DispatchResult {
            todo!()
        }

        pub fn whitelist(id: ChainId) -> DispatchResult {
            todo!()
        }

        pub fn register_relayer(relayer: T::AccountId) -> DispatchResult {
            todo!()
        }

        pub fn unregister_relayer(relayer: T::AccountId) -> DispatchResult {
            todo!()
        }

        // ** Proposal voting and execution methods ***

        fn commit_vote(
            who: T::AccountId,
            nonce: DepositNonce,
            src_id: ChainId,
            prop: Box<T::Proposal>,
            in_favour: bool,
        ) -> DispatchResult {
            todo!()
        }

        fn try_resolve_proposal(
            nonce: DepositNonce,
            src_id: ChainId,
            prop: Box<T::Proposal>,
        ) -> DispatchResult {
            todo!()
        }

        fn vote_for(
            who: T::AccountId,
            nonce: DepositNonce,
            src_id: ChainId,
            prop: Box<T::Proposal>,
        ) -> DispatchResult {
            todo!()
        }

        fn vote_against(
            who: T::AccountId,
            nonce: DepositNonce,
            src_id: ChainId,
            prop: Box<T::Proposal>,
        ) -> DispatchResult {
            todo!()
        }

        fn cancel_execution(src_id: ChainId, nonce: DepositNonce) -> DispatchResult {
            todo!()
        }

        pub fn transfer_fungible(
            dest_id: ChainId,
            resource_id: ResourceId,
            to: Vec<u8>,
            amount: H256,
        ) -> DispatchResult {
            todo!()
        }

        pub fn transfer_nonfungible(
            dest_id: ChainId,
            resource_id: ResourceId,
            token_id: Vec<u8>,
            to: Vec<u8>,
            metadata: Vec<u8>,
        ) -> DispatchResult {
            todo!()
        }

        pub fn transfer_generic(
            dest_id: ChainId,
            resource_id: ResourceId,
            metadata: Vec<u8>,
        ) -> DispatchResult {
            todo!()
        }
    }
}

pub struct EnsureBridge<T>(sp_std::marker::PhantomData<T>);
impl<T: Config> EnsureOrigin<T::Origin> for EnsureBridge<T> {
    type Success = T::AccountId;

    fn try_origin(o: T::Origin) -> Result<Self::Success, T::Origin> {
        todo!()
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn successful_origin() -> T::Origin {
        T::Origin::from(frame_system::RawOrigin::Signed(<Module<T>>::account_id()))
    }
}
