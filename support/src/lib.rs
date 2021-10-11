#![cfg_attr(not(feature = "std"), no_std)]

pub(crate) const LOG_TARGET: &'static str = "runtime::octopus-support";

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

pub mod traits;
pub mod types;
