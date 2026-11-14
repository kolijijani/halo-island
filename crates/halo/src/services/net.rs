//! Whether this PC has the internet, as Windows judges it (its connection
//! profile's connectivity level, the same answer as the taskbar's network
//! icon). Asked once per failed request, never polled, so a request that
//! brought nothing can say "no internet" or blame the server honestly
//! (`halo_core::net`).

use halo_core::net::Failure;
use windows::Networking::Connectivity::{NetworkConnectivityLevel, NetworkInformation};

/// Some(true) with internet access, Some(false) with none (no network, or
/// local access only), None when Windows couldn't say.
pub fn online() -> Option<bool> {
    let level = match NetworkInformation::GetInternetConnectionProfile() {
        Ok(p) => p.GetNetworkConnectivityLevel().ok()?,
        // No profile for the internet: nothing connects this PC to it.
        Err(_) => return Some(false),
    };
    // Constrained internet access (a captive portal) still answers some requests.
    Some(level == NetworkConnectivityLevel::InternetAccess || level == NetworkConnectivityLevel::ConstrainedInternetAccess)
}

/// A request that brought no response: no internet, or the server's fault.
pub fn failure(e: &crate::services::http::FetchError) -> Failure {
    use crate::services::http::FetchError;
    halo_core::net::no_answer(matches!(e, FetchError::Timeout), online())
}
