#![recursion_limit = "128"]
#![cfg_attr(not(feature = "std"), no_std)]
#![allow(deprecated)]

pub mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

// pub mod weights;

use borsh::BorshSerialize;
use codec::{Decode, Encode};
use frame_support::{
	pallet_prelude::*,
	traits::{Currency, Get, StorageVersion, UnixTime},
	PalletId,
};
use frame_system::{ensure_root, offchain::SendTransactionTypes, pallet_prelude::*};
use pallet_octopus_support::{
	log,
	traits::{AppchainInterface, LposInterface, UpwardMessagesInterface},
	types::{EraPayoutPayload, Offender, PayloadType, PlanNewEraPayload},
};
use pallet_session::historical;
use scale_info::{
	prelude::string::{String, ToString},
	TypeInfo,
};
use sp_runtime::{
	traits::{AccountIdConversion, CheckedConversion, Convert, SaturatedConversion},
	KeyTypeId, RuntimeDebug,
};
use sp_staking::{
	offence::{Kind, Offence, OffenceError, ReportOffence},
	SessionIndex,
};
use sp_std::{collections::btree_map::BTreeMap, convert::From, prelude::*};
// pub use weights::WeightInfo;

pub use pallet::*;

pub(crate) const LOG_TARGET: &'static str = "runtime::octopus-lpos";

/// Counter for the number of eras that have passed.
pub type EraIndex = u32;

/// Counter for the number of "reward" points earned by a given validator.
pub type RewardPoint = u32;

/// Information regarding the active era (era in used in session).
#[derive(Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ActiveEraInfo {
	/// Index of era.
	pub index: EraIndex,
	/// Anchor era number of this era.
	pub set_id: u32,
	/// Moment of start expressed as millisecond from `$UNIX_EPOCH`.
	///
	/// Start can be none if start hasn't been set for the era yet,
	/// Start is set on the first on_finalize of the era to guarantee usage of `Time`.
	start: Option<u64>,
}

/// Reward points of an era. Used to split era total payout between validators.
///
/// This points will be used to reward validators and their respective nominators.
#[derive(PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct EraRewardPoints<AccountId: Ord> {
	/// Total number of points. Equals the sum of reward points for each validator.
	pub total: RewardPoint,
	/// The reward points earned by a given validator.
	pub individual: BTreeMap<AccountId, RewardPoint>,
}

impl<AccountId: Ord> Default for EraRewardPoints<AccountId> {
	fn default() -> Self {
		EraRewardPoints { total: Default::default(), individual: BTreeMap::new() }
	}
}

/// Means for interacting with a specialized version of the `session` trait.
///
/// This is needed because `Staking` sets the `ValidatorIdOf` of the `pallet_session::Config`
pub trait SessionInterface<AccountId> {
	/// Disable the validator at the given index, returns `false` if the validator was already
	/// disabled or the index is out of bounds.
	fn disable_validator(validator_index: u32) -> bool;
	/// Get the validators from session.
	fn validators() -> Vec<AccountId>;
	/// Prune historical session tries up to but not including the given index.
	fn prune_historical_up_to(up_to: SessionIndex);

	fn is_active_validator(id: KeyTypeId, key_data: &[u8]) -> Option<AccountId>;
}

impl<T: Config> SessionInterface<<T as frame_system::Config>::AccountId> for T
where
	T: pallet_session::Config<ValidatorId = <T as frame_system::Config>::AccountId>,
	T: pallet_session::historical::Config<
		FullIdentification = u128,
		FullIdentificationOf = ExposureOf<T>,
	>,
	T::SessionHandler: pallet_session::SessionHandler<<T as frame_system::Config>::AccountId>,
	T::SessionManager: pallet_session::SessionManager<<T as frame_system::Config>::AccountId>,
	T::ValidatorIdOf: Convert<
		<T as frame_system::Config>::AccountId,
		Option<<T as frame_system::Config>::AccountId>,
	>,
{
	fn disable_validator(validator_index: u32) -> bool {
		<pallet_session::Pallet<T>>::disable_index(validator_index)
	}

	fn validators() -> Vec<<T as frame_system::Config>::AccountId> {
		<pallet_session::Pallet<T>>::validators()
	}

	fn prune_historical_up_to(up_to: SessionIndex) {
		<pallet_session::historical::Pallet<T>>::prune_up_to(up_to);
	}

	fn is_active_validator(
		id: KeyTypeId,
		key_data: &[u8],
	) -> Option<<T as frame_system::Config>::AccountId> {
		let who = <pallet_session::Pallet<T>>::key_owner(id, key_data);
		if who.is_none() {
			return None
		}

		Self::validators().into_iter().find(|v| {
			log!(debug, "check {:#?} == {:#?}", v, who);
			T::ValidatorIdOf::convert(v.clone()) == who
		})
	}
}

impl<AccountId> SessionInterface<AccountId> for () {
	fn disable_validator(_: u32) -> bool {
		true
	}

	fn validators() -> Vec<AccountId> {
		Vec::new()
	}

	fn prune_historical_up_to(_: SessionIndex) {
		()
	}

	fn is_active_validator(_id: KeyTypeId, _key_data: &[u8]) -> Option<AccountId> {
		None
	}
}

impl<T: Config> LposInterface<<T as frame_system::Config>::AccountId> for Pallet<T> {
	fn is_active_validator(
		id: KeyTypeId,
		key_data: &[u8],
	) -> Option<<T as frame_system::Config>::AccountId> {
		T::SessionInterface::is_active_validator(id, key_data)
	}

	fn active_stake_of(who: &<T as frame_system::Config>::AccountId) -> u128 {
		Self::active_era()
			.map(|active_era| Self::eras_stakers(active_era.index, who))
			.map_or(0, |v| v)
	}

	fn active_total_stake() -> Option<u128> {
		Self::active_era().map(|active_era| Self::eras_total_stake(active_era.index))
	}
}

/// The current storage version.
const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	#[pallet::without_storage_info]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + SendTransactionTypes<Call<Self>> {
		/// The currency used to pay for rewards.
		type Currency: Currency<Self::AccountId>;

		/// Time used for computing era duration.
		///
		/// It is guaranteed to start being called from the first `on_finalize`. Thus value at
		/// genesis is not used.
		type UnixTime: UnixTime;

		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Number of sessions per era.
		#[pallet::constant]
		type SessionsPerEra: Get<SessionIndex>;

		/// Number of eras that staked funds must remain bonded for.
		#[pallet::constant]
		type BondingDuration: Get<EraIndex>;

		/// Interface for interacting with a session pallet.
		type SessionInterface: self::SessionInterface<Self::AccountId>;

		// /// Weight information for extrinsics in this pallet.
		// type WeightInfo: WeightInfo;

		type AppchainInterface: AppchainInterface<Self::AccountId>;

		type UpwardMessagesInterface: UpwardMessagesInterface<Self::AccountId>;

		type PalletId: Get<PalletId>;
	}

	#[pallet::type_value]
	pub(crate) fn HistoryDepthOnEmpty() -> u32 {
		84u32
	}

	/// Number of eras to keep in history.
	///
	/// Information is kept for eras in `[current_era - history_depth; current_era]`.
	///
	/// Must be more than the number of eras delayed by session otherwise. I.e. active era must
	/// always be in history. I.e. `active_era > current_era - history_depth` must be
	/// guaranteed.
	#[pallet::storage]
	#[pallet::getter(fn history_depth)]
	pub(crate) type HistoryDepth<T> = StorageValue<_, u32, ValueQuery, HistoryDepthOnEmpty>;

	/// The current era index.
	///
	/// This is the latest planned era, depending on how the Session pallet queues the validator
	/// set, it might be active or not.
	#[pallet::storage]
	#[pallet::getter(fn current_era)]
	pub(crate) type CurrentEra<T> = StorageValue<_, EraIndex>;

	/// The active era information, it holds index and start.
	///
	/// The active era is the era being currently rewarded. Validator set of this era must be
	/// equal to [`SessionInterface::validators`].
	#[pallet::storage]
	#[pallet::getter(fn active_era)]
	pub(crate) type ActiveEra<T> = StorageValue<_, ActiveEraInfo>;

	/// The session index at which the era start for the last `HISTORY_DEPTH` eras.
	///
	/// Note: This tracks the starting session (i.e. session index when era start being active)
	/// for the eras in `[CurrentEra - HISTORY_DEPTH, CurrentEra]`.
	#[pallet::storage]
	#[pallet::getter(fn eras_start_session_index)]
	pub(crate) type ErasStartSessionIndex<T> = StorageMap<_, Twox64Concat, EraIndex, SessionIndex>;

	/// Exposure of validator at era.
	///
	/// This is keyed first by the era index to allow bulk deletion and then the stash account.
	///
	/// Is it removed after `HISTORY_DEPTH` eras.
	/// If stakers hasn't been set or has been removed then empty exposure is returned.
	#[pallet::storage]
	#[pallet::getter(fn eras_stakers)]
	pub(crate) type ErasStakers<T: Config> =
		StorageDoubleMap<_, Twox64Concat, EraIndex, Twox64Concat, T::AccountId, u128, ValueQuery>;

	/// The total validator era payout for the last `HISTORY_DEPTH` eras.
	///
	/// Eras that haven't finished yet or has been removed doesn't have reward.
	#[pallet::storage]
	#[pallet::getter(fn eras_validator_reward)]
	pub(crate) type ErasValidatorReward<T: Config> = StorageMap<_, Twox64Concat, EraIndex, u128>;

	/// Rewards for the last `HISTORY_DEPTH` eras.
	/// If reward hasn't been set or has been removed then 0 reward is returned.
	#[pallet::storage]
	#[pallet::getter(fn eras_reward_points)]
	pub(crate) type ErasRewardPoints<T: Config> =
		StorageMap<_, Twox64Concat, EraIndex, EraRewardPoints<T::AccountId>, ValueQuery>;

	/// The total amount staked for the last `HISTORY_DEPTH` eras.
	/// If total hasn't been set or has been removed then 0 stake is returned.
	#[pallet::storage]
	#[pallet::getter(fn eras_total_stake)]
	pub(crate) type ErasTotalStake<T: Config> =
		StorageMap<_, Twox64Concat, EraIndex, u128, ValueQuery>;

	/// A mapping from still-bonded eras to the first session index of that era.
	///
	/// Must contains information for eras for the range:
	/// `[active_era - bounding_duration; active_era]`
	#[pallet::storage]
	pub(crate) type BondedEras<T: Config> =
		StorageValue<_, Vec<(EraIndex, SessionIndex)>, ValueQuery>;

	/// The last planned session scheduled by the session pallet.
	///
	/// This is basically in sync with the call to [`SessionManager::new_session`].
	#[pallet::storage]
	#[pallet::getter(fn current_planned_session)]
	pub(crate) type CurrentPlannedSession<T> = StorageValue<_, SessionIndex, ValueQuery>;

	/// The payout for validators and the system for the current era.
	#[pallet::storage]
	#[pallet::getter(fn era_payout)]
	pub(crate) type EraPayout<T> = StorageValue<_, u128, ValueQuery>;

	/// Offenders that to be reported to mainchain.
	#[pallet::storage]
	#[pallet::getter(fn offenders)]
	pub(crate) type Offenders<T: Config> =
		StorageDoubleMap<_, Twox64Concat, Kind, Twox64Concat, T::AccountId, u32, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig {
		pub history_depth: u32,
		pub era_payout: u128,
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			GenesisConfig { history_depth: 84u32, era_payout: 0 }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			HistoryDepth::<T>::put(self.history_depth);
			EraPayout::<T>::put(self.era_payout);
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Notifies the mainchain to prepare the next era.
		PlanNewEra { era_index: u32 },
		/// Failed to notify the mainchain to prepare the next era.
		PlanNewEraFailed,
		/// Trigger new era.
		TriggerNewEra,
		/// Notifies the mainchain to pay the validator rewards of `era_index`.
		/// `excluded_validators` were excluded because they were not working properly.
		EraPayout { era_index: EraIndex, excluded_validators: Vec<T::AccountId> },
		/// Failed to notify the mainchain to pay the validator rewards of `era_index`.
		EraPayoutFailed { era_index: EraIndex },
		/// An old slashing report from a prior era was discarded because it could
		/// not be processed. \[session_index\]
		OldSlashingReportDiscarded(SessionIndex),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Not a controller account.
		NotController,
		/// Not a stash account.
		NotStash,
		/// Stash is already bonded.
		AlreadyBonded,
		/// Controller is already paired.
		AlreadyPaired,
		/// Targets cannot be empty.
		EmptyTargets,
		/// Duplicate index.
		DuplicateIndex,
		/// Slash record index out of bounds.
		InvalidSlashIndex,
		/// Can not bond with value less than minimum required.
		InsufficientBond,
		/// Can not schedule more unlock chunks.
		NoMoreChunks,
		/// Can not rebond without unlocking chunks.
		NoUnlockChunk,
		/// Attempting to target a stash that still has funds.
		FundedTarget,
		/// Invalid era to reward.
		InvalidEraToReward,
		/// Invalid number of nominations.
		InvalidNumberOfNominations,
		/// Items are not sorted and unique.
		NotSortedAndUnique,
		/// Rewards for this era have already been claimed for this validator.
		AlreadyClaimed,
		/// Incorrect previous history depth input provided.
		IncorrectHistoryDepth,
		/// Incorrect number of slashing spans provided.
		IncorrectSlashingSpans,
		/// Internal state has become somehow corrupted and the operation cannot continue.
		BadState,
		/// Too many nomination targets supplied.
		TooManyTargets,
		/// A nomination target was supplied that was blocked or otherwise not a validator.
		BadTarget,
		/// The user has enough bond and thus cannot be chilled forcefully by an external person.
		CannotChillOther,
		/// There are too many nominators in the system. Governance needs to adjust the staking
		/// settings to keep things safe for the runtime.
		TooManyNominators,
		/// There are too many validators in the system. Governance needs to adjust the staking
		/// settings to keep things safe for the runtime.
		TooManyValidators,
		/// There are not claimed rewards for this validator.
		NoClaimedRewards,
		/// Amount overflow.
		AmountOverflow,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_now: BlockNumberFor<T>) -> Weight {
			// just return the weight of the on_finalize.
			T::DbWeight::get().reads(1)
		}

		fn on_finalize(_n: BlockNumberFor<T>) {
			// Set the start of the first era.
			if let Some(mut active_era) = Self::active_era() {
				if active_era.start.is_none() {
					let now_as_millis_u64 = T::UnixTime::now().as_millis().saturated_into::<u64>();
					active_era.start = Some(now_as_millis_u64);
					// This write only ever happens once, we don't include it in the weight in
					// general
					ActiveEra::<T>::put(active_era);
				}
			}
			// `on_finalize` weight is tracked in `on_initialize`
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set `HistoryDepth` value. This function will delete any history information
		/// when `HistoryDepth` is reduced.
		///
		/// Parameters:
		/// - `new_history_depth`: The new history depth you would like to set.
		/// - `era_items_deleted`: The number of items that will be deleted by this dispatch. This
		///   should report all the storage items that will be deleted by clearing old era history.
		///   Needed to report an accurate weight for the dispatch. Trusted by `Root` to report an
		///   accurate number.
		///
		/// Origin must be root.
		///
		/// # <weight>
		/// - E: Number of history depths removed, i.e. 10 -> 7 = 3
		/// - Weight: O(E)
		/// - DB Weight:
		///     - Reads: Current Era, History Depth
		///     - Writes: History Depth
		///     - Clear Prefix Each: Era Stakers, EraStakersClipped, ErasValidatorPrefs
		///     - Writes Each: ErasValidatorReward, ErasRewardPoints, ErasTotalStake,
		///       ErasStartSessionIndex
		/// # </weight>
		// #[pallet::weight(<T as Config>::WeightInfo::set_history_depth(*new_history_depth,
		// *_era_items_deleted))]
		#[pallet::weight(0)]
		pub fn set_history_depth(
			origin: OriginFor<T>,
			#[pallet::compact] new_history_depth: EraIndex,
			#[pallet::compact] _era_items_deleted: u32,
		) -> DispatchResult {
			ensure_root(origin)?;
			if let Some(current_era) = Self::current_era() {
				HistoryDepth::<T>::mutate(|history_depth| {
					let last_kept = current_era.checked_sub(*history_depth).unwrap_or(0);
					let new_last_kept = current_era.checked_sub(new_history_depth).unwrap_or(0);
					for era_index in last_kept..new_last_kept {
						Self::clear_era_information(era_index);
					}
					*history_depth = new_history_depth
				})
			}
			Ok(())
		}

		// Force set era rewards with sudo permissions.
		// #[pallet::weight(<T as Config>::WeightInfo::force_set_era_payout())]
		#[pallet::weight(0)]
		pub fn force_set_era_payout(origin: OriginFor<T>, era_payout: u128) -> DispatchResult {
			ensure_root(origin)?;
			<EraPayout<T>>::put(era_payout);
			log!(debug, "force set EraPayout: {:?}", era_payout);
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	fn account_id() -> T::AccountId {
		T::PalletId::get().into_account_truncating()
	}

	/// Plan a new session potentially trigger a new era.
	fn new_session(session_index: SessionIndex, _is_genesis: bool) -> Option<Vec<T::AccountId>> {
		if let Some(current_era) = Self::current_era() {
			// Initial era has been set.
			let current_era_start_session_index = Self::eras_start_session_index(current_era)
				.unwrap_or_else(|| {
					frame_support::print("Error: start_session_index must be set for current_era");
					0
				});

			let era_length = session_index.saturating_sub(current_era_start_session_index); // Must never happen.

			log!(info, "Era length: {:?}", era_length);
			if era_length < T::SessionsPerEra::get() {
				// The 5th session of the era.
				if T::AppchainInterface::is_activated() &&
					(era_length == T::SessionsPerEra::get() - 1)
				{
					let next_set_id = T::AppchainInterface::next_set_id();
					let message = PlanNewEraPayload { new_era: next_set_id };

					let res = T::UpwardMessagesInterface::submit(
						None,
						PayloadType::PlanNewEra,
						&message.try_to_vec().unwrap(),
					);
					log!(info, "UpwardMessage::PlanNewEra: {:?}", res);
					if res.is_ok() {
						Self::deposit_event(Event::<T>::PlanNewEra { era_index: next_set_id });
					} else {
						Self::deposit_event(Event::<T>::PlanNewEraFailed);
					}
				}
				return None
			}

			// New era.
			Self::try_trigger_new_era(session_index)
		} else {
			// Set initial era.
			log!(debug, "Starting the first era.");
			Self::try_trigger_new_era(session_index)
		}
	}

	/// Start a session potentially starting an era.
	fn start_session(start_session: SessionIndex) {
		let next_active_era = Self::active_era().map(|e| e.index + 1).unwrap_or(0);
		// This is only `Some` when current era has already progressed to the next era, while the
		// active era is one behind (i.e. in the *last session of the active era*, or *first session
		// of the new current era*, depending on how you look at it).
		if let Some(next_active_era_start_session_index) =
			Self::eras_start_session_index(next_active_era)
		{
			if next_active_era_start_session_index == start_session {
				Self::start_era(start_session);
			} else if next_active_era_start_session_index < start_session {
				// This arm should never happen, but better handle it than to stall the staking
				// pallet.
				frame_support::print("Warning: A session appears to have been skipped.");
				Self::start_era(start_session);
			}
		}
	}

	/// End a session potentially ending an era.
	fn end_session(session_index: SessionIndex) {
		if let Some(active_era) = Self::active_era() {
			if let Some(next_active_era_start_session_index) =
				Self::eras_start_session_index(active_era.index + 1)
			{
				if next_active_era_start_session_index == session_index + 1 {
					Self::end_era(active_era, session_index);
				}
			}
		}
	}

	/// * Increment `active_era.index`,
	/// * reset `active_era.start`,
	/// * update `BondedEras` and apply slashes.
	fn start_era(start_session: SessionIndex) {
		let active_era = ActiveEra::<T>::mutate(|active_era| {
			let next_set_id = T::AppchainInterface::next_set_id();
			let new_index = active_era.as_ref().map(|info| info.index + 1).unwrap_or(0);
			*active_era = Some(ActiveEraInfo {
				index: new_index,
				set_id: next_set_id - 1,
				// Set new active era start in next `on_finalize`. To guarantee usage of `Time`
				start: None,
			});
			new_index
		});

		let bonding_duration = T::BondingDuration::get();

		BondedEras::<T>::mutate(|bonded| {
			bonded.push((active_era, start_session));

			if active_era > bonding_duration {
				let first_kept = active_era - bonding_duration;

				// Prune out everything that's from before the first-kept index.
				let _n_to_prune =
					bonded.iter().take_while(|&&(era_idx, _)| era_idx < first_kept).count();

				if let Some(&(_, first_session)) = bonded.first() {
					T::SessionInterface::prune_historical_up_to(first_session);
				}
			}
		});
	}

	/// Get exclude validators.
	fn get_exclude_validators(index: EraIndex) -> Vec<T::AccountId> {
		let mut validators = <ErasStakers<T>>::iter_prefix(index)
			.map(|(k, _)| k)
			.collect::<Vec<T::AccountId>>();

		log!(debug, "All validators: {:?}", validators.clone());

		let era_reward_points = <ErasRewardPoints<T>>::get(index);

		// All validators did not produce blocks, so they should be excluded.
		if era_reward_points.individual.is_empty() {
			log!(warn, "Era {:?}, no validator produce block", index);
			return validators
		}

		let expect_points = era_reward_points.total / validators.len() as u32 * 80 / 100;
		log!(
			debug,
			"Era {:?}, total points: {:?}, worked validators len: {:?}",
			index,
			era_reward_points.total,
			era_reward_points.individual.len()
		);

		let qualified_validators = era_reward_points
			.individual
			.into_iter()
			.filter_map(
				|(validator, points)| {
					if points >= expect_points {
						Some(validator)
					} else {
						None
					}
				},
			)
			.collect::<Vec<T::AccountId>>();

		validators.retain(|v| !(qualified_validators.iter().any(|val| val == v)));
		validators
	}

	/// Compute payout for era.
	fn end_era(active_era: ActiveEraInfo, _session_index: SessionIndex) {
		if !T::AppchainInterface::is_activated() || <EraPayout<T>>::get() == 0 {
			return
		}

		// Note: active_era_start can be None if end era is called during genesis config.
		if let Some(active_era_start) = active_era.start {
			if <ErasValidatorReward<T>>::get(&active_era.index).is_some() {
				log!(warn, "era reward {:?} has already been paid", active_era.index);
				return
			}

			let now_as_millis_u64 = T::UnixTime::now().as_millis().saturated_into::<u64>();
			let _era_duration = (now_as_millis_u64 - active_era_start).saturated_into::<u64>();
			let validator_payout = Self::era_payout();

			// Set ending era reward.
			<ErasValidatorReward<T>>::insert(&active_era.index, validator_payout);

			let excluded_validators = Self::get_exclude_validators(active_era.index);

			let excluded_validators_str = excluded_validators
				.iter()
				.map(|validator| {
					let prefix = String::from("0x");
					let hex_validator = prefix + &hex::encode(validator.encode());
					hex_validator
				})
				.collect::<Vec<String>>();
			log!(debug, "Exclude validators: {:?}", excluded_validators_str.clone());

			let offenders = <Offenders<T>>::drain()
				.map(|offender| {
					let prefix = String::from("0x");
					let hex_id = prefix + &hex::encode(offender.1.encode());
					Offender {
						kind: String::from_utf8_lossy(&offender.0).trim().to_string(),
						who: hex_id,
						offences: offender.2,
					}
				})
				.collect::<Vec<Offender>>();
			log!(debug, "Offenders: {:?}", offenders);

			let message = EraPayoutPayload {
				end_era: active_era.set_id,
				excluded_validators: excluded_validators_str.clone(),
				offenders,
			};

			let amount = validator_payout.checked_into().ok_or(Error::<T>::AmountOverflow).unwrap();
			T::Currency::deposit_creating(&Self::account_id(), amount);
			log!(debug, "Will send EraPayout message, era_payout is {:?}", <EraPayout<T>>::get());

			let res = T::UpwardMessagesInterface::submit(
				None,
				PayloadType::EraPayout,
				&message.try_to_vec().unwrap(),
			);
			log!(info, "UpwardMessage::EraPayout: {:?}", res);
			if res.is_ok() {
				Self::deposit_event(Event::<T>::EraPayout {
					era_index: active_era.set_id,
					excluded_validators,
				});
			} else {
				Self::deposit_event(Event::<T>::EraPayoutFailed { era_index: active_era.set_id });
			}
		}
	}

	/// Plan a new era.
	///
	/// * Bump the current era storage (which holds the latest planned era).
	/// * Store start session index for the new planned era.
	/// * Clean old era information.
	/// * Store staking information for the new planned era
	///
	/// Returns the new validator set.
	pub fn trigger_new_era(
		start_session_index: SessionIndex,
		validators: Vec<(T::AccountId, u128)>,
	) -> Vec<T::AccountId> {
		// Increment or set current era.
		let new_planned_era = CurrentEra::<T>::mutate(|s| {
			*s = Some(s.map(|s| s + 1).unwrap_or(0));
			s.unwrap()
		});
		ErasStartSessionIndex::<T>::insert(&new_planned_era, &start_session_index);

		// Clean old era information.
		if let Some(old_era) = new_planned_era.checked_sub(Self::history_depth() + 1) {
			Self::clear_era_information(old_era);
		}

		// Set staking information for the new era.
		Self::store_stakers_info(validators, new_planned_era)
	}

	/// Potentially plan a new era.
	///
	/// Get planned validator set from `T::AppchainInterface`.
	fn try_trigger_new_era(start_session_index: SessionIndex) -> Option<Vec<T::AccountId>> {
		let validators = T::AppchainInterface::planned_validators();
		log!(info, "Next validator set: {:?}", validators);

		<Pallet<T>>::deposit_event(Event::<T>::TriggerNewEra);
		Some(Self::trigger_new_era(start_session_index, validators))
	}

	/// Process the output of the validators provider.
	///
	/// Store staking information for the new planned era
	pub fn store_stakers_info(
		validators: Vec<(T::AccountId, u128)>,
		new_planned_era: EraIndex,
	) -> Vec<T::AccountId> {
		let elected_stashes = validators.iter().cloned().map(|(x, _)| x).collect::<Vec<_>>();

		let mut total_stake: u128 = 0;
		validators.into_iter().for_each(|(who, weight)| {
			total_stake = total_stake.saturating_add(weight);
			<ErasStakers<T>>::insert(new_planned_era, &who, weight);
		});

		// Insert current era staking information
		<ErasTotalStake<T>>::insert(&new_planned_era, total_stake);

		if new_planned_era > 0 {
			log!(
				info,
				"New validator set of size {:?} has been processed for era {:?}",
				elected_stashes.len(),
				new_planned_era,
			);
		}

		elected_stashes
	}

	/// Clear all era information for given era.
	fn clear_era_information(era_index: EraIndex) {
		<ErasStakers<T>>::remove_prefix(era_index, None);
		<ErasValidatorReward<T>>::remove(era_index);
		<ErasRewardPoints<T>>::remove(era_index);
		<ErasTotalStake<T>>::remove(era_index);
		ErasStartSessionIndex::<T>::remove(era_index);
	}

	/// Add reward points to validators.
	///
	/// For each element in the iterator the given number of points in u32 is added to the
	/// validator, thus duplicates are handled.
	///
	/// COMPLEXITY: Complexity is `number_of_validator_to_reward x current_elected_len`.
	pub fn reward_by_ids(validators_points: impl IntoIterator<Item = (T::AccountId, u32)>) {
		if let Some(active_era) = Self::active_era() {
			<ErasRewardPoints<T>>::mutate(active_era.index, |era_rewards| {
				for (validator, points) in validators_points.into_iter() {
					*era_rewards.individual.entry(validator).or_default() += points;
					era_rewards.total += points;
				}
			});
		}
	}
}

/// In this implementation `new_session(session)` must be called before `end_session(session-1)`
/// i.e. the new session must be planned before the ending of the previous session.
///
/// Once the first new_session is planned, all session must start and then end in order, though
/// some session can lag in between the newest session planned and the latest session started.
impl<T: Config> pallet_session::SessionManager<T::AccountId> for Pallet<T> {
	fn new_session(new_index: SessionIndex) -> Option<Vec<T::AccountId>> {
		log!(trace, "planning new session {}", new_index);
		CurrentPlannedSession::<T>::put(new_index);
		Self::new_session(new_index, false)
	}
	fn new_session_genesis(new_index: SessionIndex) -> Option<Vec<T::AccountId>> {
		log!(trace, "planning new session {} at genesis", new_index);
		CurrentPlannedSession::<T>::put(new_index);
		Self::new_session(new_index, true)
	}
	fn start_session(start_index: SessionIndex) {
		log!(trace, "starting session {}", start_index);
		Self::start_session(start_index)
	}
	fn end_session(end_index: SessionIndex) {
		log!(trace, "ending session {}", end_index);
		Self::end_session(end_index)
	}
}

impl<T: Config> historical::SessionManager<T::AccountId, u128> for Pallet<T> {
	fn new_session(new_index: SessionIndex) -> Option<Vec<(T::AccountId, u128)>> {
		<Self as pallet_session::SessionManager<_>>::new_session(new_index).map(|validators| {
			let current_era = Self::current_era()
				// Must be some as a new era has been created.
				.unwrap_or(0);

			validators
				.into_iter()
				.map(|v| {
					let exposure = Self::eras_stakers(current_era, &v);
					(v, exposure)
				})
				.collect()
		})
	}
	fn new_session_genesis(new_index: SessionIndex) -> Option<Vec<(T::AccountId, u128)>> {
		<Self as pallet_session::SessionManager<_>>::new_session_genesis(new_index).map(
			|validators| {
				let current_era = Self::current_era()
					// Must be some as a new era has been created.
					.unwrap_or(0);

				validators
					.into_iter()
					.map(|v| {
						let exposure = Self::eras_stakers(current_era, &v);
						(v, exposure)
					})
					.collect()
			},
		)
	}
	fn start_session(start_index: SessionIndex) {
		<Self as pallet_session::SessionManager<_>>::start_session(start_index)
	}
	fn end_session(end_index: SessionIndex) {
		<Self as pallet_session::SessionManager<_>>::end_session(end_index)
	}
}

/// Add reward points to block authors:
/// * 1 points to the block producer for producing a (non-uncle) block in the relay chain,
/// * 1 points to the block producer for each reference to a previously unreferenced uncle, and
/// * 1 point to the producer of each referenced uncle block.
impl<T> pallet_authorship::EventHandler<T::AccountId, T::BlockNumber> for Pallet<T>
where
	T: Config + pallet_authorship::Config + pallet_session::Config,
{
	fn note_author(author: T::AccountId) {
		Self::reward_by_ids(vec![(author, 1)])
	}
	fn note_uncle(uncle_author: T::AccountId, _age: T::BlockNumber) {
		// defensive-only: block author must exist.
		if let Some(block_author) = <pallet_authorship::Pallet<T>>::author() {
			Self::reward_by_ids(vec![(block_author, 1), (uncle_author, 1)])
		} else {
			crate::log!(warn, "block author not set, this should never happen");
		}
	}
}

/// A typed conversion from stash account ID to the active exposure of nominators
/// on that account.
///
/// Active exposure is the exposure of the validator set currently validating, i.e. in
/// `active_era`. It can differ from the latest planned exposure in `current_era`.
pub struct ExposureOf<T>(sp_std::marker::PhantomData<T>);

impl<T: Config> Convert<T::AccountId, Option<u128>> for ExposureOf<T> {
	fn convert(validator: T::AccountId) -> Option<u128> {
		<Pallet<T>>::active_era()
			.map(|active_era| <Pallet<T>>::eras_stakers(active_era.index, &validator))
	}
}

/// Filter historical offences out and only allow those from the bonding period.
pub struct FilterHistoricalOffences<T, R> {
	_inner: sp_std::marker::PhantomData<(T, R)>,
}

impl<T, R, O> ReportOffence<T::AccountId, pallet_session::historical::IdentificationTuple<T>, O>
	for FilterHistoricalOffences<Pallet<T>, R>
where
	T: Config,
	R: ReportOffence<T::AccountId, pallet_session::historical::IdentificationTuple<T>, O>,
	O: Offence<pallet_session::historical::IdentificationTuple<T>>,
	T: pallet_session::Config<ValidatorId = <T as frame_system::Config>::AccountId>,
	T: pallet_session::historical::Config<
		FullIdentification = u128,
		FullIdentificationOf = ExposureOf<T>,
	>,
{
	fn report_offence(reporters: Vec<T::AccountId>, offence: O) -> Result<(), OffenceError> {
		// Disallow any slashing from before the current bonding period.
		let offence_session = offence.session_index();
		let bonded_eras = BondedEras::<T>::get();

		if bonded_eras.first().filter(|(_, start)| offence_session >= *start).is_some() {
			let time_slot = offence.time_slot();
			let offenders: Vec<_> = offence.offenders().iter().map(|x| x.0.clone()).collect();

			log!(
				info,
				"report offence: kind: {:?}, time_slot: {:?}, offenders: {:?}",
				O::ID,
				time_slot.encode(),
				offenders
			);
			let result = R::report_offence(reporters.clone(), offence);
			if result.is_ok() {
				offenders.iter().for_each(|offender| {
					// TODO: check max length
					Offenders::<T>::mutate(O::ID, offender, |offences| *offences += 1);
				});
			}
			result
		} else {
			<Pallet<T>>::deposit_event(Event::<T>::OldSlashingReportDiscarded(offence_session));
			Ok(())
		}
	}

	fn is_known_offence(
		offenders: &[pallet_session::historical::IdentificationTuple<T>],
		time_slot: &O::TimeSlot,
	) -> bool {
		R::is_known_offence(offenders, time_slot)
	}
}
