#![recursion_limit = "128"]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(any(feature = "runtime-benchmarks", test))]
pub mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(any(feature = "runtime-benchmarks", test))]
pub mod testing_utils;
#[cfg(test)]
mod tests;

pub mod weights;

use codec::{Decode, Encode};
use frame_support::{
	pallet_prelude::*,
	traits::{Currency, EstimateNextNewSession, Get, OnUnbalanced, UnixTime},
	weights::Weight,
};
use frame_system::{ensure_root, offchain::SendTransactionTypes, pallet_prelude::*};
use pallet_octopus_support::traits::{LposInterface, ValidatorsProvider};
use pallet_session::historical;
use sp_runtime::KeyTypeId;
use sp_runtime::{
	traits::{Convert, SaturatedConversion},
	Perbill, RuntimeDebug,
};
use sp_staking::{
	offence::{Offence, OffenceDetails, OffenceError, OnOffenceHandler, ReportOffence},
	SessionIndex,
};
use sp_std::{collections::btree_map::BTreeMap, convert::From, prelude::*};
pub use weights::WeightInfo;

pub use pallet::*;

pub(crate) const LOG_TARGET: &'static str = "runtime::octopus-lpos";

// syntactic sugar for logging.
#[macro_export]
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: crate::LOG_TARGET,
			concat!("[{:?}] 🐙 ", $patter), <frame_system::Pallet<T>>::block_number() $(, $values)*
		)
	};
}

/// Counter for the number of eras that have passed.
pub type EraIndex = u32;

/// Counter for the number of "reward" points earned by a given validator.
pub type RewardPoint = u32;

/// The balance type of this pallet.
pub type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

type PositiveImbalanceOf<T> = <<T as Config>::Currency as Currency<
	<T as frame_system::Config>::AccountId,
>>::PositiveImbalance;

/// Information regarding the active era (era in used in session).
#[derive(Encode, Decode, RuntimeDebug)]
pub struct ActiveEraInfo {
	/// Index of era.
	pub index: EraIndex,
	/// Moment of start expressed as millisecond from `$UNIX_EPOCH`.
	///
	/// Start can be none if start hasn't been set for the era yet,
	/// Start is set on the first on_finalize of the era to guarantee usage of `Time`.
	start: Option<u64>,
}

/// Reward points of an era. Used to split era total payout between validators.
///
/// This points will be used to reward validators and their respective nominators.
#[derive(PartialEq, Encode, Decode, Default, RuntimeDebug)]
pub struct EraRewardPoints<AccountId: Ord> {
	/// Total number of points. Equals the sum of reward points for each validator.
	total: RewardPoint,
	/// The reward points earned by a given validator.
	individual: BTreeMap<AccountId, RewardPoint>,
}

/// Means for interacting with a specialized version of the `session` trait.
///
/// This is needed because `Staking` sets the `ValidatorIdOf` of the `pallet_session::Config`
pub trait SessionInterface<AccountId>: frame_system::Config {
	/// Disable a given validator by stash ID.
	///
	/// Returns `true` if new era should be forced at the end of this session.
	/// This allows preventing a situation where there is too many validators
	/// disabled and block production stalls.
	fn disable_validator(validator: &AccountId) -> Result<bool, ()>;
	/// Get the validators from session.
	fn validators() -> Vec<AccountId>;
	/// Prune historical session tries up to but not including the given index.
	fn prune_historical_up_to(up_to: SessionIndex);

	fn in_current_validator_set(id: KeyTypeId, key_data: &[u8]) -> Option<AccountId>;
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
	fn disable_validator(validator: &<T as frame_system::Config>::AccountId) -> Result<bool, ()> {
		<pallet_session::Pallet<T>>::disable(validator)
	}

	fn validators() -> Vec<<T as frame_system::Config>::AccountId> {
		<pallet_session::Pallet<T>>::validators()
	}

	fn prune_historical_up_to(up_to: SessionIndex) {
		<pallet_session::historical::Pallet<T>>::prune_up_to(up_to);
	}

	fn in_current_validator_set(
		id: KeyTypeId,
		key_data: &[u8],
	) -> Option<<T as frame_system::Config>::AccountId> {
		let who = <pallet_session::Pallet<T>>::key_owner(id, key_data);
		if who.is_none() {
			return None;
		}

		Self::validators().into_iter().find(|v| {
			log!(info, "check {:#?} == {:#?}", v, who);
			T::ValidatorIdOf::convert(v.clone()) == who
		})
	}
}

impl<T: Config> LposInterface<<T as frame_system::Config>::AccountId> for Pallet<T> {
	fn in_current_validator_set(
		id: KeyTypeId,
		key_data: &[u8],
	) -> Option<<T as frame_system::Config>::AccountId> {
		T::SessionInterface::in_current_validator_set(id, key_data)
	}

	fn stake_of(who: &<T as frame_system::Config>::AccountId) -> u128 {
		Self::ledger(who).map_or(0, |v| v)
	}

	fn total_stake() -> u128 {
		T::SessionInterface::validators().iter().map(|v| Self::stake_of(v)).sum()
	}
}

/// Mode of era-forcing.
#[derive(Copy, Clone, PartialEq, Eq, Encode, Decode, RuntimeDebug)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum Forcing {
	/// Not forcing anything - just let whatever happen.
	NotForcing,
	/// Force a new era, then reset to `NotForcing` as soon as it is done.
	/// Note that this will force to trigger an election until a new era is triggered, if the
	/// election failed, the next session end will trigger a new election again, until success.
	ForceNew,
	/// Avoid a new era indefinitely.
	ForceNone,
	/// Force a new era at the end of all sessions indefinitely.
	ForceAlways,
}

impl Default for Forcing {
	fn default() -> Self {
		Forcing::NotForcing
	}
}

// A value placed in storage that represents the current version of the Staking storage. This value
// is used by the `on_runtime_upgrade` logic to determine whether we run storage migration logic.
// This should match directly with the semantic versions of the Rust crate.
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
enum Releases {
	V1_0_0,
}

impl Default for Releases {
	fn default() -> Self {
		Releases::V1_0_0
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + SendTransactionTypes<Call<Self>> {
		/// The currency used to pay for rewards.
		type Currency: Currency<Self::AccountId>;

		/// Time used for computing era duration.
		///
		/// It is guaranteed to start being called from the first `on_finalize`. Thus value at genesis
		/// is not used.
		type UnixTime: UnixTime;

		/// Something that provides the election functionality.
		type ValidatorsProvider: ValidatorsProvider<Self::AccountId>;

		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// Handler for the unbalanced increment when rewarding a staker.
		type Reward: OnUnbalanced<PositiveImbalanceOf<Self>>;

		/// Number of sessions per era.
		#[pallet::constant]
		type SessionsPerEra: Get<SessionIndex>;

		/// Number of eras that staked funds must remain bonded for.
		#[pallet::constant]
		type BondingDuration: Get<EraIndex>;

		/// Interface for interacting with a session pallet.
		type SessionInterface: self::SessionInterface<Self::AccountId>;

		/// Something that can estimate the next session change, accurately or as a best effort guess.
		type NextNewSession: EstimateNextNewSession<Self::BlockNumber>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
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

	/// The ledger of a (bonded) stash.
	#[pallet::storage]
	#[pallet::getter(fn ledger)]
	pub type Ledger<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, u128>;

	/// The current era index.
	///
	/// This is the latest planned era, depending on how the Session pallet queues the validator
	/// set, it might be active or not.
	#[pallet::storage]
	#[pallet::getter(fn current_era)]
	pub type CurrentEra<T> = StorageValue<_, EraIndex>;

	/// The active era information, it holds index and start.
	///
	/// The active era is the era being currently rewarded. Validator set of this era must be
	/// equal to [`SessionInterface::validators`].
	#[pallet::storage]
	#[pallet::getter(fn active_era)]
	pub type ActiveEra<T> = StorageValue<_, ActiveEraInfo>;

	/// The session index at which the era start for the last `HISTORY_DEPTH` eras.
	///
	/// Note: This tracks the starting session (i.e. session index when era start being active)
	/// for the eras in `[CurrentEra - HISTORY_DEPTH, CurrentEra]`.
	#[pallet::storage]
	#[pallet::getter(fn eras_start_session_index)]
	pub type ErasStartSessionIndex<T> = StorageMap<_, Twox64Concat, EraIndex, SessionIndex>;

	/// Exposure of validator at era.
	///
	/// This is keyed first by the era index to allow bulk deletion and then the stash account.
	///
	/// Is it removed after `HISTORY_DEPTH` eras.
	/// If stakers hasn't been set or has been removed then empty exposure is returned.
	#[pallet::storage]
	#[pallet::getter(fn eras_stakers)]
	pub type ErasStakers<T: Config> =
		StorageDoubleMap<_, Twox64Concat, EraIndex, Twox64Concat, T::AccountId, u128, ValueQuery>;

	/// The total validator era payout for the last `HISTORY_DEPTH` eras.
	///
	/// Eras that haven't finished yet or has been removed doesn't have reward.
	#[pallet::storage]
	#[pallet::getter(fn eras_validator_reward)]
	pub type ErasValidatorReward<T: Config> = StorageMap<_, Twox64Concat, EraIndex, u128>;

	/// Rewards for the last `HISTORY_DEPTH` eras.
	/// If reward hasn't been set or has been removed then 0 reward is returned.
	#[pallet::storage]
	#[pallet::getter(fn eras_reward_points)]
	pub type ErasRewardPoints<T: Config> =
		StorageMap<_, Twox64Concat, EraIndex, EraRewardPoints<T::AccountId>, ValueQuery>;

	/// The total amount staked for the last `HISTORY_DEPTH` eras.
	/// If total hasn't been set or has been removed then 0 stake is returned.
	#[pallet::storage]
	#[pallet::getter(fn eras_total_stake)]
	pub type ErasTotalStake<T: Config> = StorageMap<_, Twox64Concat, EraIndex, u128, ValueQuery>;

	/// Mode of era forcing.
	#[pallet::storage]
	#[pallet::getter(fn force_era)]
	pub type ForceEra<T> = StorageValue<_, Forcing, ValueQuery>;

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
	pub type CurrentPlannedSession<T> = StorageValue<_, SessionIndex, ValueQuery>;

	/// The payout for validators and the system for the current era.
	#[pallet::storage]
	#[pallet::getter(fn era_payout)]
	pub type EraPayout<T> = StorageValue<_, u128, ValueQuery>;

	/// True if network has been upgraded to this version.
	/// Storage version of the pallet.
	#[pallet::storage]
	pub(crate) type StorageVersion<T: Config> = StorageValue<_, Releases, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub history_depth: u32,
		pub force_era: Forcing,
		pub canceled_payout: BalanceOf<T>,
		pub era_payout: u128,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			GenesisConfig {
				history_depth: 84u32,
				force_era: Default::default(),
				canceled_payout: Default::default(),
				era_payout: 0,
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			HistoryDepth::<T>::put(self.history_depth);
			ForceEra::<T>::put(self.force_era);
			EraPayout::<T>::put(self.era_payout);
			StorageVersion::<T>::put(Releases::V1_0_0);
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	#[pallet::metadata(T::AccountId = "AccountId", BalanceOf<T> = "Balance")]
	pub enum Event<T: Config> {
		/// The era payout has been set; the first balance is the validator-payout; the second is
		/// the remainder from the maximum amount of reward.
		/// \[era_index, validator_payout, remainder\]
		EraPayout(EraIndex, u128),
		/// The staker has been rewarded by this amount. \[stash, amount\]
		Reward(T::AccountId, BalanceOf<T>),
		/// One validator (and its nominators) has been slashed by the given amount.
		/// \[validator, amount\]
		Slash(T::AccountId, BalanceOf<T>),
		/// An old slashing report from a prior era was discarded because it could
		/// not be processed. \[session_index\]
		OldSlashingReportDiscarded(SessionIndex),
		/// A new set of stakers was elected.
		StakingElection,
		/// An account has bonded this amount. \[stash, amount\]
		///
		/// NOTE: This event is only emitted when funds are bonded via a dispatchable. Notably,
		/// it will not be emitted for staking rewards when they are added to stake.
		Bonded(T::AccountId, u128),
		/// An account has unbonded this amount. \[stash, amount\]
		Unbonded(T::AccountId, BalanceOf<T>),
		/// An account has called `withdraw_unbonded` and removed unbonding chunks worth `Balance`
		/// from the unlocking queue. \[stash, amount\]
		Withdrawn(T::AccountId, BalanceOf<T>),
		/// A nominator has been kicked from a validator. \[nominator, stash\]
		Kicked(T::AccountId, T::AccountId),
		/// The election failed. No new era is planned.
		StakingElectionFailed,
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
		/// There are too many nominators in the system. Governance needs to adjust the staking settings
		/// to keep things safe for the runtime.
		TooManyNominators,
		/// There are too many validators in the system. Governance needs to adjust the staking settings
		/// to keep things safe for the runtime.
		TooManyValidators,
		/// There are not claimed rewards for this validator.
		NoClaimedRewards,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_runtime_upgrade() -> Weight {
			T::DbWeight::get().reads(1)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<(), &'static str> {
			Ok(())
		}

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
					// This write only ever happens once, we don't include it in the weight in general
					ActiveEra::<T>::put(active_era);
				}
			}
			// `on_finalize` weight is tracked in `on_initialize`
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Force there to be no new eras indefinitely.
		///
		/// The dispatch origin must be Root.
		///
		/// # Warning
		///
		/// The election process starts multiple blocks before the end of the era.
		/// Thus the election process may be ongoing when this is called. In this case the
		/// election will continue until the next era is triggered.
		///
		/// # <weight>
		/// - No arguments.
		/// - Weight: O(1)
		/// - Write: ForceEra
		/// # </weight>
		#[pallet::weight(T::WeightInfo::force_no_eras())]
		pub fn force_no_eras(origin: OriginFor<T>) -> DispatchResult {
			ensure_root(origin)?;
			ForceEra::<T>::put(Forcing::ForceNone);
			Ok(())
		}

		/// Force there to be a new era at the end of the next session. After this, it will be
		/// reset to normal (non-forced) behaviour.
		///
		/// The dispatch origin must be Root.
		///
		/// # Warning
		///
		/// The election process starts multiple blocks before the end of the era.
		/// If this is called just before a new era is triggered, the election process may not
		/// have enough blocks to get a result.
		///
		/// # <weight>
		/// - No arguments.
		/// - Weight: O(1)
		/// - Write ForceEra
		/// # </weight>
		#[pallet::weight(T::WeightInfo::force_new_era())]
		pub fn force_new_era(origin: OriginFor<T>) -> DispatchResult {
			ensure_root(origin)?;
			ForceEra::<T>::put(Forcing::ForceNew);
			Ok(())
		}

		/// Force there to be a new era at the end of sessions indefinitely.
		///
		/// The dispatch origin must be Root.
		///
		/// # Warning
		///
		/// The election process starts multiple blocks before the end of the era.
		/// If this is called just before a new era is triggered, the election process may not
		/// have enough blocks to get a result.
		///
		/// # <weight>
		/// - Weight: O(1)
		/// - Write: ForceEra
		/// # </weight>
		#[pallet::weight(T::WeightInfo::force_new_era_always())]
		pub fn force_new_era_always(origin: OriginFor<T>) -> DispatchResult {
			ensure_root(origin)?;
			ForceEra::<T>::put(Forcing::ForceAlways);
			Ok(())
		}

		/// Set `HistoryDepth` value. This function will delete any history information
		/// when `HistoryDepth` is reduced.
		///
		/// Parameters:
		/// - `new_history_depth`: The new history depth you would like to set.
		/// - `era_items_deleted`: The number of items that will be deleted by this dispatch.
		///    This should report all the storage items that will be deleted by clearing old
		///    era history. Needed to report an accurate weight for the dispatch. Trusted by
		///    `Root` to report an accurate number.
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
		///     - Writes Each: ErasValidatorReward, ErasRewardPoints, ErasTotalStake, ErasStartSessionIndex
		/// # </weight>
		#[pallet::weight(T::WeightInfo::set_history_depth(*_era_items_deleted))]
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
	}
}

impl<T: Config> Pallet<T> {
	fn bond_and_validate(controller: <T as frame_system::Config>::AccountId, value: u128) {
		<Ledger<T>>::insert(&controller, value);
		Self::deposit_event(Event::<T>::Bonded(controller.clone(), value));
	}

	fn do_payout_stakers(controller: T::AccountId, era: EraIndex) {
		// // Validate input data
		// let current_era = CurrentEra::<T>::get().ok_or(
		// 	Error::<T>::InvalidEraToReward
		// 		.with_weight(T::WeightInfo::payout_stakers_alive_staked(0)),
		// )?;
		// let history_depth = Self::history_depth();
		// ensure!(
		// 	era <= current_era && era >= current_era.saturating_sub(history_depth),
		// 	Error::<T>::InvalidEraToReward
		// 		.with_weight(T::WeightInfo::payout_stakers_alive_staked(0))
		// );

		// // Note: if era has no reward to be claimed, era may be future. better not to update
		// // `ledger.claimed_rewards` in this case.
		// let era_payout = <ErasValidatorReward<T>>::get(&era).ok_or_else(|| {
		// 	Error::<T>::InvalidEraToReward
		// 		.with_weight(T::WeightInfo::payout_stakers_alive_staked(0))
		// })?;

		// let mut claimed_rewards =
		// 	<ClaimedRewards<T>>::get(&controller).ok_or_else(|| Error::<T>::NoClaimedRewards)?;

		// claimed_rewards.retain(|&x| x >= current_era.saturating_sub(history_depth));
		// match claimed_rewards.binary_search(&era) {
		// 	Ok(_) => Err(Error::<T>::AlreadyClaimed
		// 		.with_weight(T::WeightInfo::payout_stakers_alive_staked(0)))?,
		// 	Err(pos) => claimed_rewards.insert(pos, era),
		// }

		// let exposure = <ErasStakersClipped<T>>::get(&era, &controller);

		// /* Input data seems good, no errors allowed after this point */

		// <ClaimedRewards<T>>::insert(&controller, claimed_rewards);

		// // Get Era reward points. It has TOTAL and INDIVIDUAL
		// // Find the fraction of the era reward that belongs to the validator
		// // Take that fraction of the eras rewards to split to nominator and validator
		// //
		// // Then look at the validator, figure out the proportion of their reward
		// // which goes to them and each of their nominators.

		// let era_reward_points = <ErasRewardPoints<T>>::get(&era);
		// let total_reward_points = era_reward_points.total;
		// let validator_reward_points = era_reward_points
		// 	.individual
		// 	.get(&controller)
		// 	.map(|points| *points)
		// 	.unwrap_or_else(|| Zero::zero());

		// // Nothing to do if they have no reward points.
		// if validator_reward_points.is_zero() {
		// 	return Ok(Some(T::WeightInfo::payout_stakers_alive_staked(0)).into());
		// }

		// // This is the fraction of the total reward that the validator and the
		// // nominators will get.
		// let validator_total_reward_part =
		// 	Perbill::from_rational(validator_reward_points, total_reward_points);

		// // This is how much validator + nominators are entitled to.
		// let validator_total_payout = validator_total_reward_part * era_payout;

		// let validator_prefs = Self::eras_validator_prefs(&era, &controller);
		// // Validator first gets a cut off the top.
		// let validator_commission = COMMISSION;
		// let validator_commission_payout = validator_commission * validator_total_payout;

		// let validator_leftover_payout = validator_total_payout - validator_commission_payout;
		// // Now let's calculate how this is split to the validator.
		// let validator_exposure_part = Perbill::from_rational(exposure.own, exposure.total);
		// let validator_staking_payout = validator_exposure_part * validator_leftover_payout;

		// // We can now make total validator payout:
		// if let Some(imbalance) =
		// 	Self::make_payout(&controller, validator_staking_payout + validator_commission_payout)
		// {
		// 	Self::deposit_event(Event::<T>::Reward(controller, imbalance.peek()));
		// }

		// // Track the number of payout ops to nominators. Note: `WeightInfo::payout_stakers_alive_staked`
		// // always assumes at least a validator is paid out, so we do not need to count their payout op.
		// let mut nominator_payout_count: u32 = 0;

		// // Lets now calculate how this is split to the nominators.
		// // Reward only the clipped exposures. Note this is not necessarily sorted.
		// for nominator in exposure.others.iter() {
		// 	let nominator_exposure_part = Perbill::from_rational(nominator.value, exposure.total);

		// 	let nominator_reward: BalanceOf<T> =
		// 		nominator_exposure_part * validator_leftover_payout;
		// 	// We can now make nominator payout:
		// 	if let Some(imbalance) = Self::make_payout(&nominator.who, nominator_reward) {
		// 		// Note: this logic does not count payouts for `RewardDestination::None`.
		// 		nominator_payout_count += 1;
		// 		Self::deposit_event(Event::<T>::Reward(nominator.who.clone(), imbalance.peek()));
		// 	}
		// }

		// debug_assert!(nominator_payout_count <= T::MaxNominatorRewardedPerValidator::get());
		// Ok(Some(T::WeightInfo::payout_stakers_alive_staked(nominator_payout_count)).into())
	}

	/// Actually make a payment to a staker. This uses the currency's reward function
	/// to pay the right payee for the given staker account.
	fn make_payout(
		controller: &T::AccountId,
		amount: BalanceOf<T>,
	) -> Option<PositiveImbalanceOf<T>> {
		// let dest = Self::payee(controller);
		// match dest {
		// 	RewardDestination::Controller => {
		// 		Some(T::Currency::deposit_creating(&controller, amount))
		// 	}
		// 	RewardDestination::Account(dest_account) => {
		// 		Some(T::Currency::deposit_creating(&dest_account, amount))
		// 	}
		// 	RewardDestination::None => None,
		// }
		None
	}

	/// Plan a new session potentially trigger a new era.
	fn new_session(session_index: SessionIndex, is_genesis: bool) -> Option<Vec<T::AccountId>> {
		if let Some(current_era) = Self::current_era() {
			// Initial era has been set.
			let current_era_start_session_index = Self::eras_start_session_index(current_era)
				.unwrap_or_else(|| {
					frame_support::print("Error: start_session_index must be set for current_era");
					0
				});

			let era_length =
				session_index.checked_sub(current_era_start_session_index).unwrap_or(0); // Must never happen.

			match ForceEra::<T>::get() {
				// Will be set to `NotForcing` again if a new era has been triggered.
				Forcing::ForceNew => (),
				// Short circuit to `try_trigger_new_era`.
				Forcing::ForceAlways => (),
				// Only go to `try_trigger_new_era` if deadline reached.
				Forcing::NotForcing if era_length >= T::SessionsPerEra::get() => (),
				_ => {
					// Either `Forcing::ForceNone`,
					// or `Forcing::NotForcing if era_length < T::SessionsPerEra::get()`.
					return None;
				}
			}

			// New era.
			let maybe_new_era_validators = Self::try_trigger_new_era(session_index);
			if maybe_new_era_validators.is_some()
				&& matches!(ForceEra::<T>::get(), Forcing::ForceNew)
			{
				ForceEra::<T>::put(Forcing::NotForcing);
			}

			maybe_new_era_validators
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
			let new_index = active_era.as_ref().map(|info| info.index + 1).unwrap_or(0);
			*active_era = Some(ActiveEraInfo {
				index: new_index,
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
				let n_to_prune =
					bonded.iter().take_while(|&&(era_idx, _)| era_idx < first_kept).count();

				if let Some(&(_, first_session)) = bonded.first() {
					T::SessionInterface::prune_historical_up_to(first_session);
				}
			}
		});
	}

	/// Compute payout for era.
	fn end_era(active_era: ActiveEraInfo, _session_index: SessionIndex) {
		// Note: active_era_start can be None if end era is called during genesis config.
		if let Some(active_era_start) = active_era.start {
			let now_as_millis_u64 = T::UnixTime::now().as_millis().saturated_into::<u64>();

			let _era_duration = (now_as_millis_u64 - active_era_start).saturated_into::<u64>();
			let _staked = Self::eras_total_stake(&active_era.index);
			let _issuance = T::Currency::total_issuance();

			let validator_payout = Self::era_payout();

			Self::deposit_event(Event::<T>::EraPayout(active_era.index, validator_payout));

			// Set ending era reward.
			<ErasValidatorReward<T>>::insert(&active_era.index, validator_payout);
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
	/// Get election result from `T::ElectionProvider`.
	/// In case election result has more than [`MinimumValidatorCount`] validator trigger a new era.
	///
	/// In case a new era is planned, the new validator set is returned.
	fn try_trigger_new_era(start_session_index: SessionIndex) -> Option<Vec<T::AccountId>> {
		let validators = T::ValidatorsProvider::validators();
		log!(info, "Next validator set: {:?}", validators);

		<Ledger<T>>::remove_all(None);
		validators.iter().for_each(|(who, weight)| {
			Self::bond_and_validate(who.clone(), *weight);
		});

		Self::deposit_event(Event::StakingElection);
		Some(Self::trigger_new_era(start_session_index, validators))
	}

	/// Process the output of the election.
	///
	/// Store staking information for the new planned era
	pub fn store_stakers_info(
		validators: Vec<(T::AccountId, u128)>,
		new_planned_era: EraIndex,
	) -> Vec<T::AccountId> {
		let elected_stashes = validators.iter().cloned().map(|(x, _)| x).collect::<Vec<_>>();

		// Populate stakers, exposures, and the snapshot of validator prefs.
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
				"new validator set of size {:?} has been processed for era {:?}",
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

	/// Add reward points to validators using their stash account ID.
	///
	/// Validators are keyed by stash account ID and must be in the current elected set.
	///
	/// For each element in the iterator the given number of points in u32 is added to the
	/// validator, thus duplicates are handled.
	///
	/// At the end of the era each the total payout will be distributed among validator
	/// relatively to their points.
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

	/// Ensures that at the end of the current session there will be a new era.
	fn ensure_new_era() {
		match ForceEra::<T>::get() {
			Forcing::ForceAlways | Forcing::ForceNew => (),
			_ => ForceEra::<T>::put(Forcing::ForceNew),
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
/// * 20 points to the block producer for producing a (non-uncle) block in the relay chain,
/// * 2 points to the block producer for each reference to a previously unreferenced uncle, and
/// * 1 point to the producer of each referenced uncle block.
impl<T> pallet_authorship::EventHandler<T::AccountId, T::BlockNumber> for Pallet<T>
where
	T: Config + pallet_authorship::Config + pallet_session::Config,
{
	fn note_author(author: T::AccountId) {
		log!(info, "note_author: {:?}", author);
		Self::reward_by_ids(vec![(author, 20)])
	}
	fn note_uncle(author: T::AccountId, _age: T::BlockNumber) {
		log!(
			info,
			"note_uncle: {:?} {:?} - {:?}",
			author,
			_age,
			<pallet_authorship::Pallet<T>>::author()
		);
		Self::reward_by_ids(vec![(<pallet_authorship::Pallet<T>>::author(), 2), (author, 1)])
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

/// This is intended to be used with `FilterHistoricalOffences`.
impl<T: Config>
	OnOffenceHandler<T::AccountId, pallet_session::historical::IdentificationTuple<T>, Weight>
	for Pallet<T>
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
	fn on_offence(
		offenders: &[OffenceDetails<
			T::AccountId,
			pallet_session::historical::IdentificationTuple<T>,
		>],
		slash_fraction: &[Perbill],
		slash_session: SessionIndex,
	) -> Weight {
		let mut consumed_weight: Weight = 0;
		consumed_weight
	}
}

/// Filter historical offences out and only allow those from the bonding period.
pub struct FilterHistoricalOffences<T, R> {
	_inner: sp_std::marker::PhantomData<(T, R)>,
}

impl<T, Reporter, Offender, R, O> ReportOffence<Reporter, Offender, O>
	for FilterHistoricalOffences<Pallet<T>, R>
where
	T: Config,
	R: ReportOffence<Reporter, Offender, O>,
	O: Offence<Offender>,
{
	fn report_offence(reporters: Vec<Reporter>, offence: O) -> Result<(), OffenceError> {
		// Disallow any slashing from before the current bonding period.
		let offence_session = offence.session_index();
		let bonded_eras = BondedEras::<T>::get();

		if bonded_eras.first().filter(|(_, start)| offence_session >= *start).is_some() {
			R::report_offence(reporters, offence)
		} else {
			<Pallet<T>>::deposit_event(Event::<T>::OldSlashingReportDiscarded(offence_session));
			Ok(())
		}
	}

	fn is_known_offence(offenders: &[Offender], time_slot: &O::TimeSlot) -> bool {
		R::is_known_offence(offenders, time_slot)
	}
}
