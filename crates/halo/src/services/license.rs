//! Halo Pro licenses from the license server (`server/license-server.mjs`,
//! docs/licenses.md): redeeming a code for this install, and keeping the
//! token it gives. Shared by the Windows and Mac apps.
//!
//! - This install's id: 32 random bytes made once and sealed like the other
//!   secrets (`license-device.bin`); only its SHA-256 ever leaves the PC.
//! - The token: sealed too (`license.bin`). It is checked offline at every
//!   start against the keys built into the app (`halo_core::license`), so
//!   Pro works without the server; what fails a check is Free.
//! - Where the server is: `HALO_LICENSE_URL` (its redeem API's https
//!   address). This build has none built in, a deployment decision: without
//!   it, redeeming says so and nothing is sent.
//!
//! A redemption runs once, on the add-on thread's HTTP client (one job), and
//! answers with [`AppEvent::License`]. Nothing runs at rest.

use crate::events::{AppEvent, EventSink};
use crate::services::addons::Job;
use crate::services::http::{Client, Method};
use halo_core::license::{self, Grant, Refusal};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(15);

/// What a redemption came to.
#[derive(Clone, Debug, PartialEq)]
pub enum Redeemed {
    /// A token that checked out here, and what it grants.
    Token { token: String, grant: Grant },
    /// The server said no, in its words.
    Refused(&'static str),
    /// It couldn't be asked: no internet, a timeout, the server down.
    Trouble(String),
    /// This build has no license server to ask.
    NoServer,
}

/// The redeem API's address, from `HALO_LICENSE_URL`. https only.
pub fn server() -> Option<String> {
    let u = std::env::var("HALO_LICENSE_URL").ok()?;
    let u = u.trim().trim_end_matches('/');
    (u.starts_with("https://") && halo_core::net::configured(u)).then(|| u.to_string())
}

/// This install's id hash (64 hex), made and sealed the first time.
pub fn device() -> Option<String> {
    let path = crate::paths::license_device_path();
    let id = match crate::services::secret::load(&path) {
        Some(id) if id.len() == 64 => id,
        _ => {
            let id = random_hex()?;
            if let Err(e) = crate::services::secret::save(&path, &id) {
                crate::pwarn!("license: can't keep this install's id ({e})");
                return None;
            }
            id
        }
    };
    Some(halo_core::sha256::hex(&halo_core::sha256::sha256(format!("halo-license/1 device\n{id}").as_bytes())))
}

#[cfg(windows)]
fn random_hex() -> Option<String> {
    use windows::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_ALG_HANDLE, BCRYPT_USE_SYSTEM_PREFERRED_RNG};
    let mut b = [0u8; 32];
    unsafe { BCryptGenRandom(BCRYPT_ALG_HANDLE::default(), &mut b, BCRYPT_USE_SYSTEM_PREFERRED_RNG) }.ok().ok()?;
    Some(b.iter().map(|x| format!("{x:02x}")).collect())
}

#[cfg(not(windows))]
fn random_hex() -> Option<String> {
    use std::io::Read;
    let mut b = [0u8; 32];
    std::fs::File::open("/dev/urandom").ok()?.read_exact(&mut b).ok()?;
    Some(b.iter().map(|x| format!("{x:02x}")).collect())
}

/// The kept token, if it still checks out here (`revoked`: the server said
/// so), else why not (`Err(None)`: no token kept), for Pro and Settings ›
/// Halo Pro. A token that fails is deleted, so Pro doesn't hang on it.
pub fn status(revoked: bool) -> Result<Grant, Option<Refusal>> {
    let path = crate::paths::license_path();
    let token = crate::services::secret::load(&path).ok_or(None)?;
    match device().map(|d| license::check(&token, &d, crate::clock::unix_now(), revoked, crate::services::update::verify)) {
        Some(Ok(g)) => Ok(g),
        Some(Err(r)) => {
            crate::pinfo!("license: the kept license no longer unlocks Pro ({r:?})");
            if matches!(r, Refusal::Expired | Refusal::Revoked | Refusal::OtherInstall | Refusal::BadSignature | Refusal::Malformed) {
                let _ = crate::services::secret::delete(&path);
            }
            Err(Some(r))
        }
        None => Err(None),
    }
}

/// Keep `token` (it checked out).
pub fn keep(token: &str) -> bool {
    match crate::services::secret::save(&crate::paths::license_path(), token) {
        Ok(()) => true,
        Err(e) => {
            crate::pwarn!("license: can't keep the license ({e})");
            false
        }
    }
}

/// Forget the license on this install (Deactivate).
pub fn forget() {
    let _ = crate::services::secret::delete(&crate::paths::license_path());
}

/// A job for the add-on thread: redeem `code`; the answer comes back as
/// `AppEvent::License`. The code is sent once, over https, and never logged.
pub fn redeem_job(code: String, sink: EventSink) -> Job {
    Box::new(move |client: Result<&Client, &str>| {
        let r = match (server(), device(), client) {
            (None, _, _) => Redeemed::NoServer,
            (_, None, _) => Redeemed::Trouble("this install's id couldn't be kept".into()),
            (_, _, Err(e)) => Redeemed::Trouble(e.to_string()),
            (Some(base), Some(dev), Ok(c)) => ask(c, &base, &code, &dev),
        };
        crate::pinfo!(
            "license: redeem {}",
            match &r {
                Redeemed::Token { grant, .. } => format!("ok ({})", grant.kind),
                Redeemed::Refused(w) => format!("refused ({w})"),
                Redeemed::Trouble(t) => format!("couldn't ask ({t})"),
                Redeemed::NoServer => "no server in this build".into(),
            }
        );
        sink.send(AppEvent::License(r));
    })
}

fn ask(c: &Client, base: &str, code: &str, dev: &str) -> Redeemed {
    let body = serde_json::json!({ "code": code, "device": dev }).to_string();
    let resp = match c.send(Method::Post, &format!("{base}/api/redeem"), &[], Some(&body), TIMEOUT) {
        Ok(r) => r,
        Err(e) => {
            let f = crate::services::net::failure(&e);
            return Redeemed::Trouble(f.line("The license server"));
        }
    };
    read_answer(resp.status, &resp.body, dev, crate::clock::unix_now(), crate::services::update::verify)
}

/// The server's answer to what the app does: a token is believed only once
/// it checks out here.
pub fn read_answer(status: u16, body: &str, dev: &str, now: i64, verify: impl Fn(&[u8], &[u8; 64], &[u8; 65]) -> bool) -> Redeemed {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    match status {
        200 => {
            let Some(token) = v["token"].as_str() else { return Redeemed::Trouble("The license server sent an answer Halo Island can't read.".into()) };
            match license::check(token, dev, now, false, verify) {
                Ok(grant) => Redeemed::Token { token: token.to_string(), grant },
                Err(r) => Redeemed::Refused(license::refusal_words(r)),
            }
        }
        404 => Redeemed::Refused("No license with that code."),
        409 => Redeemed::Refused("That code was already used on another install."),
        403 => Redeemed::Refused("That code was revoked."),
        410 => Redeemed::Refused(if v["error"] == "lapsed" { "That code wasn't used in time." } else { "That license has ended." }),
        400 => Redeemed::Refused("That isn't a Halo Pro code."),
        429 => Redeemed::Trouble("Too many tries. Wait a minute.".into()),
        s => Redeemed::Trouble(halo_core::net::Failure::Status(s).line("The license server")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every answer the server gives, read the way the app shows it; a 200
    /// whose token doesn't check out here is refused, never Pro.
    #[test]
    fn the_servers_answers_read_honestly() {
        let dev = "d".repeat(64);
        let yes = |_: &[u8], _: &[u8; 64], _: &[u8; 65]| true;
        // A real token, but this build trusts no license key: refused.
        let fx: serde_json::Value = serde_json::from_str(include_str!("../../../halo-core/tests/fixtures/license/token.json")).unwrap();
        let body = serde_json::json!({ "token": fx["token"] }).to_string();
        assert_eq!(read_answer(200, &body, &dev, 1_794_564_100, yes), Redeemed::Refused("This version can't check that license. Update Halo Island."));
        assert!(matches!(read_answer(200, "<html>", &dev, 0, yes), Redeemed::Trouble(_)));
        assert_eq!(read_answer(409, r#"{"error":"already_used"}"#, &dev, 0, yes), Redeemed::Refused("That code was already used on another install."));
        assert_eq!(read_answer(410, r#"{"error":"lapsed"}"#, &dev, 0, yes), Redeemed::Refused("That code wasn't used in time."));
        assert_eq!(read_answer(410, r#"{"error":"expired"}"#, &dev, 0, yes), Redeemed::Refused("That license has ended."));
        assert_eq!(read_answer(404, "{}", &dev, 0, yes), Redeemed::Refused("No license with that code."));
        assert!(matches!(read_answer(503, "", &dev, 0, yes), Redeemed::Trouble(t) if t.contains("error 503")));
        assert!(matches!(read_answer(429, "", &dev, 0, yes), Redeemed::Trouble(_)));
    }
}
