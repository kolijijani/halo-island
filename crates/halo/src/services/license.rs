//! Halo Pro licenses from the license server (`server/license-server.mjs`,
//! docs/licenses.md): redeeming a code for this install, and keeping the
//! token it gives. Shared by the Windows and Mac apps.
//!
//! - This install's id: 32 random bytes made once and sealed like the other
//!   secrets (`license-device.bin`); only its SHA-256 ever leaves the PC.
//! - The token: sealed too (`license.bin`). It is checked offline at every
//!   start against the keys built into the app (`halo_core::license`), so
//!   Pro works without the server; what fails a check is Free. Both files
//!   live with the settings, not the cache, so uninstalling keeps them.
//! - One seat: a license is on one install at a time. Redeeming its code on
//!   another moves the seat there; this one learns at its daily look at
//!   `/api/status` ([`standing_job`]) and drops to Free.
//! - Where the server is: `halo_core::license::LICENSE_URL`, built in;
//!   `HALO_LICENSE_URL` (an https address) overrides it for tests.
//!
//! A redemption runs once, on the add-on thread's HTTP client (one job), and
//! answers with [`AppEvent::License`]. The daily look is one more job, only
//! while a server license is on, nobody away, and the server set; it rides
//! the island's minute timer, so nothing new runs at rest.

use crate::events::{AppEvent, EventSink};
use crate::services::addons::Job;
use crate::services::http::{Client, Method};
use crate::state::AppState;
use halo_core::geom::palette;
use halo_core::license::{self, Grant, Refusal, Standing};
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

/// The redeem API's address: `HALO_LICENSE_URL` when set (https only, for
/// tests), otherwise the one built in (`halo_core::license::LICENSE_URL`).
pub fn server() -> Option<String> {
    let Ok(u) = std::env::var("HALO_LICENSE_URL") else { return Some(license::LICENSE_URL.to_string()) };
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

/// Before 0.2.51 Windows kept the token and the install id in the cache
/// (`%LOCALAPPDATA%\Halo`), which uninstalling removes: move them to the
/// settings folder once, at start.
#[cfg(windows)]
pub fn carry_over() {
    for (old, new) in [("license.bin", crate::paths::license_path()), ("license-device.bin", crate::paths::license_device_path())] {
        let old = crate::paths::cache_dir().join(old);
        if old.exists() && !new.exists() {
            if let Err(e) = std::fs::create_dir_all(crate::paths::data_dir()).and_then(|_| std::fs::rename(&old, &new)) {
                crate::pwarn!("license: can't move {} to the settings folder ({e})", old.display());
            }
        }
    }
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

/// A job for the add-on thread: ask `/api/status` whether `grant` still
/// holds this install's seat; the answer comes back as
/// `AppEvent::LicenseStanding`. None without a server.
pub fn standing_job(grant: &Grant, sink: EventSink) -> Option<Job> {
    let base = server()?;
    // `dev` checked out as this install's when the token was read.
    let mut body = serde_json::json!({ "lic": grant.lic, "device": grant.dev });
    if let Some(seat) = grant.seat {
        body["seat"] = seat.into();
    }
    let body = body.to_string();
    let seat = grant.seat;
    Some(Box::new(move |client: Result<&Client, &str>| {
        let s = match client.map(|c| c.send(Method::Post, &format!("{base}/api/status"), &[], Some(&body), TIMEOUT)) {
            Ok(Ok(r)) => license::standing(r.status, &r.body),
            _ => Standing::Unknown,
        };
        crate::pinfo!("license: status {s:?}");
        sink.send(AppEvent::LicenseStanding { seat, standing: s });
    }))
}

/// The daily look ended the license here (another PC took the seat, an admin
/// revoked it, or its days are over): forget the token, Pro off unless the
/// built-in key is in, and say so once.
pub fn ended(st: &mut AppState, r: Refusal) {
    forget();
    st.ui.license = Some(Err(Some(r)));
    st.settings.licensed = false;
    st.settings.pro = license::valid(&st.settings.pro_key);
    let why = if r == Refusal::Expired { "Its time is over" } else { "It was activated on another PC, or revoked" };
    st.notify(crate::icons::Icon::Unlink, palette::ORANGE, "Halo Pro is off on this PC", why, 8.0, None);
    st.cmd(crate::commands::Command::SaveSettings);
}

/// The minute timer's turn: when a server license is on and its daily look
/// is due, the job to run, and `due` moves a day on. Nothing while nobody is
/// there (`away`: locked, display off or battery saver); it runs on the
/// next minute after they're back.
pub fn look_due(st: &AppState, due: &mut i64, away: bool, sink: &EventSink) -> Option<Job> {
    let grant = st.ui.license.as_ref()?.as_ref().ok()?;
    let now = crate::clock::unix_now();
    if away || now < *due {
        return None;
    }
    *due = now + license::STATUS_EVERY_S;
    standing_job(grant, sink.clone())
}

/// What the daily look found. `seat`: the seat it asked about; an answer
/// about a license since deactivated or redeemed again changes nothing.
pub fn on_standing(st: &mut AppState, seat: Option<u32>, s: Standing, due: &mut i64) {
    let asked_about_this = st.ui.license.as_ref().and_then(|l| l.as_ref().ok()).is_some_and(|g| g.seat == seat);
    match s {
        Standing::Ends(r) if asked_about_this => ended(st, r),
        Standing::Unknown => *due = (*due).min(crate::clock::unix_now() + license::STATUS_RETRY_S),
        _ => {}
    }
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
        // A real token, but from a throwaway key this build doesn't trust: refused.
        let fx: serde_json::Value = serde_json::from_str(include_str!("../../../halo-core/tests/fixtures/license/token.json")).unwrap();
        let body = serde_json::json!({ "token": fx["token"] }).to_string();
        assert_eq!(read_answer(200, &body, &dev, 1_794_564_100, yes), Redeemed::Refused("This version can't check that license. Update Halo Island."));
        assert!(matches!(read_answer(200, "<html>", &dev, 0, yes), Redeemed::Trouble(_)));
        assert_eq!(read_answer(410, r#"{"error":"lapsed"}"#, &dev, 0, yes), Redeemed::Refused("That code wasn't used in time."));
        assert_eq!(read_answer(410, r#"{"error":"expired"}"#, &dev, 0, yes), Redeemed::Refused("That license has ended."));
        assert_eq!(read_answer(404, "{}", &dev, 0, yes), Redeemed::Refused("No license with that code."));
        assert!(matches!(read_answer(503, "", &dev, 0, yes), Redeemed::Trouble(t) if t.contains("error 503")));
        assert!(matches!(read_answer(429, "", &dev, 0, yes), Redeemed::Trouble(_)));
    }
}
