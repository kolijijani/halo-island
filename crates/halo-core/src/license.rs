//! Halo Pro: one permanent key, checked on this PC alone. The key typed in
//! Settings › Halo Pro is hashed (SHA-256) and compared with [`KEY_SHA256`].
//! No server, no machine fingerprint, no device limit, no monitor id.
//!
//! To change the key: put the new key's `sha256sum` here
//! (`printf '%s' 'NEW-KEY' | sha256sum`). Copies already activated with the
//! old key go back to Free until the new one is typed.

/// SHA-256 of the Halo Pro key, lowercase hex.
pub const KEY_SHA256: &str = "ca7c4d170fc8ce338cfc526a260d6ed7997e7ffc6aac1b2688ee47661f2c96e5";

/// The key as people type it: spaces dropped, letters upper-cased, so
/// "halo-2026-koliji-9f4c-77b1 " still counts.
pub fn normalize(key: &str) -> String {
    key.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase()
}

/// Whether `key` unlocks Pro.
pub fn valid(key: &str) -> bool {
    let k = normalize(key);
    !k.is_empty() && crate::sha256::hex(&crate::sha256::sha256(k.as_bytes())) == KEY_SHA256
}

// ---------------------------------------------------------------- tokens

/// The license server's keys this build trusts (`data/license-keys.txt`).
const TOKEN_KEYS: &str = include_str!("../data/license-keys.txt");

/// A trusted license key's public point, by id.
pub fn token_key(id: &str) -> Option<[u8; 65]> {
    trusted(TOKEN_KEYS, id)
}

fn trusted(list: &str, id: &str) -> Option<[u8; 65]> {
    list.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).find_map(|l| {
        let mut parts = l.split_whitespace();
        (parts.next()? == id).then_some(())?;
        let key: [u8; 65] = crate::update::hex(parts.next()?)?.try_into().ok()?;
        (key[0] == 4).then_some(key)
    })
}

/// What a license token grants (`server/license-server.mjs`'s `signToken`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct Grant {
    pub v: u32,
    /// The signing key's id.
    pub kid: String,
    /// The license's id (for `/api/status`).
    pub lic: String,
    /// "pro" or "trial".
    pub kind: String,
    pub iat: i64,
    /// Unix seconds it ends; None: no end.
    pub exp: Option<i64>,
    /// The install it was redeemed for: SHA-256 of its own random id, hex.
    pub dev: String,
}

/// A token split up for checking: the signed text, its signature (r || s)
/// and what it grants. Nothing in it is believed until [`check`] passes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub signed: String,
    pub signature: [u8; 64],
    pub grant: Grant,
}

/// `HL1.<payload>.<signature>` (base64url) to its parts.
pub fn parse_token(token: &str) -> Option<Token> {
    let t = token.trim();
    let rest = t.strip_prefix("HL1.")?;
    let (payload, sig) = rest.split_once('.')?;
    let grant: Grant = serde_json::from_slice(&base64url(payload)?).ok()?;
    let signature: [u8; 64] = base64url(sig)?.try_into().ok()?;
    Some(Token { signed: format!("HL1.{payload}"), signature, grant })
}

fn base64url(s: &str) -> Option<Vec<u8>> {
    if s.bytes().any(|b| matches!(b, b'+' | b'/' | b'=')) {
        return None;
    }
    crate::update::base64(&s.replace('-', "+").replace('_', "/"))
}

/// Why a token doesn't unlock Pro.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Not a token at all.
    Malformed,
    /// Signed with a key this build doesn't trust (or none is set up).
    UnknownKey,
    /// The signature doesn't match.
    BadSignature,
    /// Redeemed for another install.
    OtherInstall,
    /// Its time is over.
    Expired,
    /// The server said it was revoked.
    Revoked,
}

/// Whether `token` unlocks Pro on this install at `now` (unix seconds).
/// `verify(message, signature, key)` is the platform's ECDSA P-256 check
/// (`services::update`'s). `device`: this install's id hash. `revoked`: the
/// server said so at the last look. Fails closed: anything unknown refuses.
pub fn check(token: &str, device: &str, now: i64, revoked: bool, verify: impl Fn(&[u8], &[u8; 64], &[u8; 65]) -> bool) -> Result<Grant, Refusal> {
    check_with(TOKEN_KEYS, token, device, now, revoked, verify)
}

fn check_with(keys: &str, token: &str, device: &str, now: i64, revoked: bool, verify: impl Fn(&[u8], &[u8; 64], &[u8; 65]) -> bool) -> Result<Grant, Refusal> {
    let t = parse_token(token).ok_or(Refusal::Malformed)?;
    if t.grant.v != 1 || !matches!(t.grant.kind.as_str(), "pro" | "trial") {
        return Err(Refusal::Malformed);
    }
    let key = trusted(keys, &t.grant.kid).ok_or(Refusal::UnknownKey)?;
    if !verify(t.signed.as_bytes(), &t.signature, &key) {
        return Err(Refusal::BadSignature);
    }
    if t.grant.dev != device {
        return Err(Refusal::OtherInstall);
    }
    if revoked {
        return Err(Refusal::Revoked);
    }
    if t.grant.exp.is_some_and(|e| now >= e) {
        return Err(Refusal::Expired);
    }
    Ok(t.grant)
}

/// The words for a refusal, on the Halo Pro page.
pub fn refusal_words(r: Refusal) -> &'static str {
    match r {
        Refusal::Malformed => "That isn't a Halo Pro license.",
        Refusal::UnknownKey => "This version can't check that license. Update Halo Island.",
        Refusal::BadSignature => "That license didn't check out.",
        Refusal::OtherInstall => "That license belongs to another install.",
        Refusal::Expired => "That license has ended.",
        Refusal::Revoked => "That license was revoked.",
    }
}

// ---------------------------------------------------------------- the page

/// Where Halo Pro stands on this install, as Settings › Halo Pro shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// No key and no license: Free.
    Free,
    /// The built-in key: Halo Pro, no end, checked on this PC alone.
    Key,
    /// A license from the license server: Halo Pro, with or without an end.
    Licensed,
    /// A trial license.
    Trial,
    /// A license whose time is over.
    Expired,
    /// A license the server revoked.
    Revoked,
    /// This build has no license server, so no code can be redeemed.
    NoServer,
}

impl State {
    /// Where things stand: `grant` is the kept license that checked out
    /// here, else `refused` says why the kept one didn't; `key`: the
    /// built-in key is in; `server`: this build has a license server.
    pub fn of(grant: Option<&Grant>, refused: Option<Refusal>, key: bool, server: bool) -> State {
        match (grant, refused) {
            (Some(g), _) if g.kind == "trial" => State::Trial,
            (Some(_), _) => State::Licensed,
            _ if key => State::Key,
            (None, Some(Refusal::Expired)) => State::Expired,
            (None, Some(Refusal::Revoked)) => State::Revoked,
            _ if !server => State::NoServer,
            _ => State::Free,
        }
    }

    /// The badge: what kind of license this is, at a glance.
    pub fn badge(self) -> &'static str {
        match self {
            State::Free | State::NoServer => "FREE",
            State::Key => "PRO \u{b7} PERMANENT",
            State::Licensed => "PRO",
            State::Trial => "TRIAL",
            State::Expired => "EXPIRED",
            State::Revoked => "REVOKED",
        }
    }

    /// One line under the badge, in plain words.
    pub fn line(self) -> &'static str {
        match self {
            State::Free => "Free is whole. Pro adds the power parts.",
            State::Key => "Halo Pro with the built-in key: no end, on this PC.",
            State::Licensed => "Halo Pro from a license code, bound to this PC.",
            State::Trial => "Trial Pro from a license code, bound to this PC.",
            State::Expired => "This license has ended. Pro parts are off until a new code.",
            State::Revoked => "This license was revoked. Pro parts are off.",
            State::NoServer => "This version has no license server: only the built-in key unlocks Pro.",
        }
    }

    /// Pro is on.
    pub fn pro(self) -> bool {
        matches!(self, State::Key | State::Licensed | State::Trial)
    }
}

/// How long until `exp` from `now` (unix seconds), as the page counts it
/// down: "ends in 23 days, 4 hours", "ends in 5 hours, 12 minutes",
/// "ends in 3 minutes", "ended". No end: "no end".
pub fn countdown(exp: Option<i64>, now: i64) -> String {
    let Some(exp) = exp else { return "no end".to_string() };
    let left = exp - now;
    if left <= 0 {
        return "ended".to_string();
    }
    let plural = |n: i64, one: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {one}s") };
    let (d, h, m) = (left / 86_400, left % 86_400 / 3600, left % 3600 / 60);
    match (d, h) {
        (0, 0) => format!("ends in {}", plural(((left + 59) / 60).max(1), "minute")),
        (0, h) => format!("ends in {}, {}", plural(h, "hour"), plural(m, "minute")),
        (d, h) => format!("ends in {}, {}", plural(d, "day"), plural(h, "hour")),
    }
}

/// The moment `exp` falls on, at UTC offset `offset_s`: "13 Dec 2026, 12:57".
pub fn end_date(exp: i64, offset_s: i64) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let t = exp + offset_s;
    let (y, m, d) = crate::datetime::civil_from_days(t.div_euclid(86_400));
    let s = t.rem_euclid(86_400);
    format!("{d} {} {y}, {:02}:{:02}", MONTHS[(m.clamp(1, 12) - 1) as usize], s / 3600, s % 3600 / 60)
}

/// This install's id as the page shows it: the first 12 of its hash, in
/// groups of four ("8f3a-21c9-77d0"). The hash is all that leaves the PC.
pub fn short_device(hash: &str) -> String {
    let h: String = hash.chars().filter(char::is_ascii_hexdigit).take(12).collect();
    h.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A token made the way the server makes it (ECDSA P-256, `HL1.` +
    /// base64url payload), checked with a stand-in for the platform's verify
    /// that accepts exactly one signature: success, expired, another install,
    /// revoked, an untrusted key, a bad signature and malformed tokens.
    #[test]
    fn a_token_unlocks_only_when_every_check_passes() {
        let point = format!("04{}", "11".repeat(64));
        let keys = format!("# test\nabcd1234abcd1234 {point}\n");
        let dev = "d".repeat(64);
        let b64u = |b: &[u8]| {
            const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
            let mut out = String::new();
            for chunk in b.chunks(3) {
                let n = chunk.iter().fold(0u32, |acc, x| acc << 8 | *x as u32) << (8 * (3 - chunk.len()));
                for i in 0..=chunk.len() {
                    out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
                }
            }
            out
        };
        let token = |grant: &str, sig: [u8; 64]| format!("HL1.{}.{}", b64u(grant.as_bytes()), b64u(&sig));
        let good_sig = [7u8; 64];
        let verify = |_: &[u8], s: &[u8; 64], k: &[u8; 65]| *s == good_sig && k[1] == 0x11;
        let grant = |exp: &str, dev: &str, kid: &str| format!(r#"{{"v":1,"kid":"{kid}","lic":"{}","kind":"trial","iat":100,"exp":{exp},"dev":"{dev}"}}"#, "a".repeat(32));
        let ok = token(&grant("1000", &dev, "abcd1234abcd1234"), good_sig);
        let g = check_with(&keys, &ok, &dev, 500, false, verify).unwrap();
        assert_eq!((g.kind.as_str(), g.exp), ("trial", Some(1000)));
        assert_eq!(check_with(&keys, &ok, &dev, 1000, false, verify), Err(Refusal::Expired));
        assert_eq!(check_with(&keys, &ok, &"e".repeat(64), 500, false, verify), Err(Refusal::OtherInstall));
        assert_eq!(check_with(&keys, &ok, &dev, 500, true, verify), Err(Refusal::Revoked));
        assert_eq!(check_with(&keys, &token(&grant("null", &dev, "ffff0000ffff0000"), good_sig), &dev, 500, false, verify), Err(Refusal::UnknownKey));
        assert_eq!(check_with(&keys, &token(&grant("null", &dev, "abcd1234abcd1234"), [8u8; 64]), &dev, 500, false, verify), Err(Refusal::BadSignature));
        // No end: never expires.
        assert!(check_with(&keys, &token(&grant("null", &dev, "abcd1234abcd1234"), good_sig), &dev, i64::MAX, false, verify).is_ok());
        for bad in ["", "HALO-2026-KOLIJI-9F4C-77B1", "HL1.", "HL1.e30.AAAA", "HL1.not+base64url.x", &ok.replace("HL1.", "HL2.")] {
            assert_eq!(check_with(&keys, bad, &dev, 500, false, verify), Err(Refusal::Malformed), "{bad}");
        }
        // This build trusts no license server yet: every token fails closed.
        assert_eq!(check(&ok, &dev, 500, false, |_, _, _| true), Err(Refusal::UnknownKey));
    }

    #[test]
    fn the_page_says_where_pro_stands() {
        let g = |kind: &str| Grant { v: 1, kid: "k".into(), lic: "l".into(), kind: kind.into(), iat: 0, exp: None, dev: "d".into() };
        assert_eq!(State::of(Some(&g("trial")), None, false, true), State::Trial);
        assert_eq!(State::of(Some(&g("pro")), None, true, true), State::Licensed, "a license wins over the key");
        assert_eq!(State::of(None, None, true, false), State::Key);
        assert_eq!(State::of(None, Some(Refusal::Expired), false, true), State::Expired);
        assert_eq!(State::of(None, Some(Refusal::Revoked), false, true), State::Revoked);
        assert_eq!(State::of(None, Some(Refusal::Malformed), false, true), State::Free);
        assert_eq!(State::of(None, None, false, false), State::NoServer);
        assert_eq!(State::of(None, None, false, true), State::Free);
        // Trial and permanent look different at a glance.
        assert_eq!((State::Trial.badge(), State::Key.badge()), ("TRIAL", "PRO \u{b7} PERMANENT"));
        assert!(State::Trial.pro() && State::Key.pro() && State::Licensed.pro());
        assert!(!State::Expired.pro() && !State::Revoked.pro() && !State::Free.pro() && !State::NoServer.pro());
    }

    #[test]
    fn the_countdown_reads_like_people_say_it() {
        let now = 1_794_564_000;
        assert_eq!(countdown(Some(now + 23 * 86_400 + 4 * 3600 + 120), now), "ends in 23 days, 4 hours");
        assert_eq!(countdown(Some(now + 86_400), now), "ends in 1 day, 0 hours");
        assert_eq!(countdown(Some(now + 5 * 3600 + 12 * 60), now), "ends in 5 hours, 12 minutes");
        assert_eq!(countdown(Some(now + 3600 + 60), now), "ends in 1 hour, 1 minute");
        assert_eq!(countdown(Some(now + 170), now), "ends in 3 minutes");
        assert_eq!(countdown(Some(now + 1), now), "ends in 1 minute");
        assert_eq!(countdown(Some(now), now), "ended", "expired");
        assert_eq!(countdown(Some(now - 86_400), now), "ended");
        assert_eq!(countdown(None, now), "no end");
        // 13 Dec 2026 04:57:12 UTC, at UTC+8.
        assert_eq!(end_date(1_797_137_832, 8 * 3600), "13 Dec 2026, 12:57");
        assert_eq!(short_device(&"8f3a21c977d0ab".repeat(4)), "8f3a-21c9-77d0");
        assert_eq!(short_device(""), "");
    }

    #[test]
    fn the_key_unlocks_and_nothing_else_does() {
        assert!(valid("HALO-2026-KOLIJI-9F4C-77B1"));
        assert!(valid(" halo-2026-koliji-9f4c-77b1\n"));
        assert!(!valid(""));
        assert!(!valid("HALO-2026-KOLIJI-9F4C-77B2"));
        assert!(!valid(KEY_SHA256), "the hash itself is no key");
    }
}
