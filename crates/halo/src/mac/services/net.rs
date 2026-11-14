//! Whether this Mac has the internet. macOS gives no cheap, synchronous
//! answer to that (Network.framework's path monitor is a callback queue), so
//! the Mac says it can't tell, and a request that brought nothing is put down
//! to the server or the line (`halo_core::net::no_answer`), never reported
//! as "no internet" on a guess.
//! ponytail: an NWPathMonitor started with the first online feature would
//! let the Mac tell offline apart too.

use halo_core::net::Failure;

pub fn online() -> Option<bool> {
    None
}

/// A request that brought no response.
pub fn failure(e: &crate::services::http::FetchError) -> Failure {
    use crate::services::http::FetchError;
    halo_core::net::no_answer(matches!(e, FetchError::Timeout), online())
}
