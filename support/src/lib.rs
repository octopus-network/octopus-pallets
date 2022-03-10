#![cfg_attr(not(feature = "std"), no_std)]

pub mod traits;
pub mod types;

#[allow(dead_code)]
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
