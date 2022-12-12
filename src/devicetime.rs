//! A simple module for keepign an absolute UNIX time
//!
//! The accessor, [unixtime()], does not succeed until time has been set once.
//!
//! Under non-demo circumstances, time should only ever be set from trusted time sources.
//!
//! This expresses UNIX time in unsigned 32-bit integers, which are good until 2076.

use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

/// UNIX timestamp at which the device was booted.
///
/// 0 serves as a sentinel value for "it is unknown", and is fine given that Rust was not invented
/// in 1970. (An `Option<NonzeroU32>` would be more accurate, but is not provided by the default
/// atomics).
static OFFSET: AtomicU32 = AtomicU32::new(0);

/// Error type indicating that no absolute time is known
#[derive(Debug)]
pub struct ClockNotSet;

/// State that the current time is `now` (on the UNIX time scale); future calls to [unixtime()]
/// will return this or a greater value.
pub fn set_unixtime(now: u32) {
    OFFSET.store(now - embassy_time::Instant::now().as_secs() as u32, Relaxed)
}

/// Obtain the current time as UNIX time
pub fn unixtime() -> Result<u32, ClockNotSet> {
    let offset = OFFSET.load(Relaxed);
    match offset {
        0 => Err(ClockNotSet),
        o => Ok(embassy_time::Instant::now().as_secs() as u32 + o),
    }
}
