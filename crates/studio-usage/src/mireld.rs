//! Mireld AI (https://portal.mireld.my): prepaid credits for many models on
//! one OpenAI-compatible key. The key is the one typed into Halo Island,
//! else `MIRELD_API_KEY`, and it goes to Mireld's API and nowhere else.
//!
//! A refresh asks `api.mireld.my` with `Authorization: Bearer <key>`:
//! `GET /v1/models` first (a 200 with a model list proves the key works and
//! says how many models it opens; 401/403 is a refused key), then
//! `GET /v1/usage`, the key's balance. When `/v1/usage` doesn't answer with
//! one, `GET /v1/credits` stands in.
//!
//! Units: Mireld's credits are tokens. `/v1/usage` counts them one by one
//! (`credits_total` 7375000000 is 7,375,000,000 tokens); `/v1/credits`
//! counts them in millions (`total_credits` 7375 is the same 7,375,000,000).
//! The card shows tokens, in full.

use super::{group, num, reply_json, time, Ctx, Dirs, Fact, KeyAsk, Money, NetFail, Outcome, Provider, ProviderId, Reach, Reply, Request, Snapshot, Source, Step, Trouble, Want, Window, WindowKind};
use serde_json::Value;

const MODELS_URL: &str = "https://api.mireld.my/v1/models";
const USAGE_URL: &str = "https://api.mireld.my/v1/usage";
const CREDITS_URL: &str = "https://api.mireld.my/v1/credits";
/// Where the typed key is kept (sealed, `usage-mireld.bin`), and the variable.
const SECRET: &str = "mireld";
const ENV: &str = "MIRELD_API_KEY";
/// Where people buy and see their key.
const PORTAL: &str = "https://portal.mireld.my/";

pub struct Mireld;

impl Provider for Mireld {
    fn id(&self) -> ProviderId {
        ProviderId::Mireld
    }

    fn env(&self) -> &'static [&'static str] {
        &[ENV]
    }

    fn wants(&self, _dirs: &Dirs) -> Vec<Want> {
        vec![Want::Secret(SECRET)]
    }

    fn next(&self, cx: &Ctx, replies: &[Result<Reply, NetFail>]) -> Step {
        let key = match cx.got(0).text().or_else(|| cx.dirs.var(ENV)).map(str::trim).filter(|k| !k.is_empty()) {
            None => return Step::Done(Err(Trouble::NeedsKey)),
            // Printable ASCII only: a space or line break pasted into it would
            // be refused, or break the header it rides in, so it never leaves.
            Some(k) if !k.bytes().all(|b| b.is_ascii_graphic()) => return Step::Done(Err(Trouble::Refused)),
            Some(k) => k,
        };
        if cx.reach == Reach::Local {
            return Step::Done(Err(Trouble::NeedsOnline));
        }
        match replies {
            [] => Step::Fetch(Request::get(MODELS_URL).bearer(key)),
            [models] => match models_card(cx.now, models) {
                Ok(_) => Step::Fetch(Request::get(USAGE_URL).bearer(key)),
                Err(t) => Step::Done(Err(t)),
            },
            [models, usage] => match models_card(cx.now, models) {
                Ok(s) if reply_json(usage).ok().is_some_and(|v| v["credits_total"].is_number()) => Step::Done(balance(s, usage)),
                Ok(_) => Step::Fetch(Request::get(CREDITS_URL).bearer(key)),
                Err(t) => Step::Done(Err(t)),
            },
            [models, usage, credits, ..] => Step::Done(models_card(cx.now, models).and_then(|s| fallback(s, usage, credits))),
        }
    }

    /// A balance moves with use, but not by much in 5 minutes.
    fn min_gap_s(&self) -> u64 {
        300
    }

    fn takes_key(&self) -> Option<KeyAsk> {
        Some(KeyAsk { secret: SECRET, url: PORTAL, hint: "sk-…" })
    }
}

/// `/v1/models`: `{"object":"list","data":[{"id":…},…]}`. Mireld's own
/// error is `{"error":{"code":"invalid_api_key",…}}` with 401.
fn models_card(now: i64, r: &Result<Reply, NetFail>) -> Outcome {
    let v = reply_json(r)?;
    let models = v["data"].as_array().ok_or_else(|| Trouble::Unreadable("no model list".into()))?;
    let mut s = Snapshot::new(ProviderId::Mireld, now, Source::Online);
    let n = models.iter().filter(|m| m["id"].as_str().is_some_and(|id| !id.trim().is_empty())).count();
    s.plan = Some(match n {
        0 => "Key works · no models open".into(),
        1 => "Key works · 1 model".into(),
        n => format!("Key works · {n} models"),
    });
    s.facts.push(Fact::new("Models", n.to_string()));
    Ok(s)
}

/// Tokens as the card says them: "900,089,926 tokens".
fn tokens(n: u64) -> String {
    format!("{} tokens", group(n))
}

fn whole(v: &Value) -> Option<u64> {
    num(v).filter(|x| *x >= 0.0).map(|x| x.round() as u64)
}

/// Used of total as a bar, and what's left as the balance, in full.
fn spent(s: &mut Snapshot, used: u64, total: u64, pct: Option<f64>) {
    let k = pct.map_or(if total > 0 { used as f64 / total as f64 } else { 0.0 }, |p| p / 100.0);
    s.windows.push(Window::new(WindowKind::Other, "Used", k as f32).counted(used, total));
    let left = total.saturating_sub(used);
    s.balance = Some(Money { micros: (left as i64).saturating_mul(1_000_000), currency: "tokens".into() });
}

/// `/v1/usage` (`"object":"mireld.balance"`): every field a person can
/// read goes on the card.
fn balance(mut s: Snapshot, r: &Result<Reply, NetFail>) -> Outcome {
    let v = reply_json(r)?;
    let (Some(total), Some(used)) = (whole(&v["credits_total"]), whole(&v["credits_used"])) else {
        return Err(Trouble::Unreadable("no balance".into()));
    };
    let pct = num(&v["usage_percent"]);
    spent(&mut s, used, total, pct);
    if let Some(left) = whole(&v["credits_remaining"]) {
        s.balance = Some(Money { micros: (left as i64).saturating_mul(1_000_000), currency: "tokens".into() });
    }
    if let Some(plan) = v["plan"].as_str().map(str::trim).filter(|p| !p.is_empty()) {
        s.plan = Some(plan.to_string());
    }
    // Used of the total is the bar and what's left the balance: the lines
    // carry the rest.
    let mut facts: Vec<Fact> = pct.map(|p| Fact::new("Used", format!("{p:.1}% of the plan"))).into_iter().collect();
    for (key, label) in [("credits_available", "Available now"), ("credits_reserved", "Reserved")] {
        if let Some(n) = whole(&v[key]) {
            facts.push(Fact::new(label, tokens(n)));
        }
    }
    if let Some(at) = time(&v["expires_at"]) {
        facts.push(Fact { until: Some(at), ..Fact::new("Expires", String::new()) });
    }
    let prefix: String = v["key"]["prefix"].as_str().unwrap_or("").chars().take_while(|c| *c != '…').take(12).collect();
    let state = v["key"]["status"].as_str().unwrap_or("").trim();
    let key = match (prefix.is_empty(), state.is_empty()) {
        (false, false) => Some(format!("{prefix}… · {state}")),
        (false, true) => Some(format!("{prefix}…")),
        (true, false) => Some(state.to_string()),
        (true, true) => None,
    };
    facts.extend(key.map(|k| Fact::new("Key", k)));
    if let Some(n) = whole(&v["request_count"]) {
        facts.push(Fact::new("Requests", group(n)));
    }
    let limits: Vec<String> = [("rpm", "a minute"), ("concurrency", "at once")].iter().filter_map(|(k, w)| whole(&v["limits"][k]).map(|n| format!("{} {w}", group(n)))).collect();
    if !limits.is_empty() {
        facts.push(Fact::new("Limits", limits.join(" · ")));
    }
    if v["history"]["available"] == false {
        let why = v["history"]["reason"].as_str().map(str::trim).filter(|r| !r.is_empty()).unwrap_or("Not available");
        facts.push(Fact::new("History", why));
    }
    if let Some(unit) = v["unit"].as_str().filter(|u| *u != "credits") {
        facts.push(Fact::new("Unit", unit));
    }
    s.facts.extend(facts);
    // The numbers were true when Mireld made the answer.
    if let Some(at) = time(&v["generated_at"]).filter(|t| *t <= s.as_of) {
        s.as_of = at;
    }
    Ok(s)
}

/// `/v1/usage` didn't give a balance: `/v1/credits`'
/// `{"data":{"total_credits":7375,"total_usage":6474.910074}}` (millions of
/// tokens). When neither has one, the card says why: no internet and a
/// busy Mireld as themselves, an answer it can't read as that, and a key
/// whose plan has no balance (404 on both) as the key and its models only.
fn fallback(mut s: Snapshot, usage: &Result<Reply, NetFail>, credits: &Result<Reply, NetFail>) -> Outcome {
    if let Ok(v) = reply_json(credits) {
        let d = &v["data"];
        if let (Some(total), Some(used)) = (num(&d["total_credits"]), num(&d["total_usage"])) {
            let (total, used) = ((total.max(0.0) * 1e6).round() as u64, (used.max(0.0) * 1e6).round() as u64);
            spent(&mut s, used, total, None);
            return Ok(s);
        }
    }
    let gone = |r: &Result<Reply, NetFail>| matches!(r, Ok(r) if r.status == 404);
    if gone(usage) && gone(credits) {
        return Ok(s);
    }
    let why = |r| reply_json(r).err().unwrap_or(Trouble::Unreadable("no balance".into()));
    let (a, b) = (why(usage), why(credits));
    Err(if matches!(b, Trouble::Unreadable(_)) { a } else { b })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Got, Os, MAX_FETCHES};

    const MODELS: &str = include_str!("fixtures/mireld/models.json");
    const REFUSED: &str = include_str!("fixtures/mireld/invalid_key.json");
    const USAGE: &str = include_str!("fixtures/mireld/usage.json");
    const CREDITS: &str = include_str!("fixtures/mireld/credits.json");
    const UNSUPPORTED: &str = include_str!("fixtures/mireld/unsupported.json");
    const NOW: i64 = 1_794_564_000;

    fn ok(status: u16, body: &str) -> Result<Reply, NetFail> {
        Ok(Reply { status, body: body.into(), retry_after_s: None })
    }

    fn refresh(dirs: &Dirs, got: &[Got], reach: Reach, answer: impl Fn(&Request) -> Result<Reply, NetFail>) -> (Vec<Request>, Outcome) {
        let cx = Ctx { dirs, got, now: NOW, reach };
        let (mut asked, mut replies) = (Vec::new(), Vec::new());
        loop {
            match Mireld.next(&cx, &replies) {
                Step::Read(w) => panic!("no reads in steps: {w:?}"),
                Step::Run(c) => panic!("no programs: {c:?}"),
                Step::Fetch(r) => {
                    assert!(asked.len() < MAX_FETCHES);
                    replies.push(answer(&r));
                    asked.push(r);
                }
                Step::Done(out) => return (asked, out),
            }
        }
    }

    fn typed(answer: Result<Reply, NetFail>) -> (Vec<Request>, Outcome) {
        refresh(&Dirs::sample(Os::Windows), &[Got::Text(" sk-test-key\n".into())], Reach::Online, |_| answer.clone())
    }

    /// Mireld as it answers: the models, then `usage` for `/v1/usage` and
    /// `credits` for `/v1/credits`.
    fn mireld(usage: Result<Reply, NetFail>, credits: Result<Reply, NetFail>) -> impl Fn(&Request) -> Result<Reply, NetFail> {
        move |r: &Request| match r.url.as_str() {
            MODELS_URL => ok(200, MODELS),
            USAGE_URL => usage.clone(),
            CREDITS_URL => credits.clone(),
            u => panic!("asked {u}"),
        }
    }

    fn fact<'a>(s: &'a Snapshot, label: &str) -> Option<&'a Fact> {
        s.facts.iter().find(|f| f.label == label)
    }

    #[test]
    fn a_working_key_shows_everything_its_balance_says() {
        let key = [Got::Text(" sk-test-key\n".into())];
        let (asked, out) = refresh(&Dirs::sample(Os::Windows), &key, Reach::Online, mireld(ok(200, USAGE), ok(200, CREDITS)));
        assert_eq!(asked, [Request::get(MODELS_URL).bearer("sk-test-key"), Request::get(USAGE_URL).bearer("sk-test-key")], "/v1/credits only when /v1/usage has no balance");
        let s = out.unwrap();
        assert_eq!(s.plan.as_deref(), Some("200M Token Credits / 30 Days"));
        // Tokens in full, never cut short: 7,375,000,000 of them, 900,089,926 left.
        let w = &s.windows[0];
        assert_eq!((w.label.as_str(), w.count), ("Used", Some((6_474_910_074, 7_375_000_000))));
        assert!((w.used - 0.878).abs() < 1e-4, "Mireld's own percent");
        assert_eq!(s.balance, Some(Money { micros: 900_089_926_000_000, currency: "tokens".into() }));
        assert_eq!(fact(&s, "Used").unwrap().value, "87.8% of the plan");
        assert_eq!(fact(&s, "Available now").unwrap().value, "900,089,926 tokens");
        assert_eq!(fact(&s, "Reserved").unwrap().value, "0 tokens");
        assert_eq!(fact(&s, "Expires").unwrap().until, crate::parse_time("2026-12-13T04:57:12.000Z"));
        assert_eq!(fact(&s, "Key").unwrap().value, "sk-AbCd_Ef_G… · active", "the prefix Mireld gives and the key's state");
        assert_eq!(fact(&s, "Requests").unwrap().value, "76,399");
        assert_eq!(fact(&s, "Limits").unwrap().value, "40 a minute · 10 at once");
        assert_eq!(fact(&s, "Models").unwrap().value, "3");
        assert_eq!(fact(&s, "History").unwrap().value, "Historical credit grants are incomplete for this key.");
        // The key goes nowhere but the request's header: not the card, not a Debug print.
        assert!(!serde_json::to_string(&s).unwrap().contains("sk-test"));
        assert!(!format!("{:?}", asked[0]).contains("sk-test"));
    }

    #[test]
    fn credits_stand_in_when_usage_has_no_balance() {
        let key = [Got::Text("sk-x".into())];
        let (asked, out) = refresh(&Dirs::sample(Os::Windows), &key, Reach::Online, mireld(ok(404, UNSUPPORTED), ok(200, CREDITS)));
        assert_eq!(asked.len(), 3);
        let s = out.unwrap();
        // /v1/credits counts millions: 7375 is 7,375,000,000 tokens.
        assert_eq!(s.windows[0].count, Some((6_474_910_074, 7_375_000_000)));
        assert_eq!(s.balance, Some(Money { micros: 900_089_926_000_000, currency: "tokens".into() }));
        assert_eq!(s.plan.as_deref(), Some("Key works · 3 models"), "no plan name without /v1/usage");
        // A plan with neither: the key and its models, never a made-up balance.
        let (_, out) = refresh(&Dirs::sample(Os::Windows), &key, Reach::Online, mireld(ok(404, UNSUPPORTED), ok(404, UNSUPPORTED)));
        let s = out.unwrap();
        assert_eq!((s.balance.is_none(), s.windows.len()), (true, 0), "unknown is not zero");
        // Offline or busy for the balance says so; an answer it can't read is that.
        assert_eq!(refresh(&Dirs::sample(Os::Windows), &key, Reach::Online, mireld(Err(NetFail::Timeout), Err(NetFail::Timeout))).1, Err(Trouble::Offline));
        assert_eq!(refresh(&Dirs::sample(Os::Windows), &key, Reach::Online, mireld(ok(200, "<html>"), ok(429, ""))).1, Err(Trouble::Busy { retry_after_s: None }));
        assert!(matches!(refresh(&Dirs::sample(Os::Windows), &key, Reach::Online, mireld(ok(200, "{}"), ok(200, "{}"))).1, Err(Trouble::Unreadable(_))));
    }

    #[test]
    fn no_key_needs_one_and_the_variable_stands_in() {
        assert_eq!(refresh(&Dirs::sample(Os::Mac), &[Got::Missing], Reach::Online, |_| unreachable!()).1, Err(Trouble::NeedsKey));
        assert_eq!(refresh(&Dirs::sample(Os::Mac), &[Got::Text("  ".into())], Reach::Online, |_| unreachable!()).1, Err(Trouble::NeedsKey));
        let mut dirs = Dirs::sample(Os::Windows);
        dirs.env.push((ENV.into(), "sk-from-env".into()));
        let (asked, out) = refresh(&dirs, &[Got::Missing], Reach::Online, mireld(ok(200, USAGE), ok(200, CREDITS)));
        assert!(out.is_ok());
        assert_eq!(asked[0].headers, [("Authorization".to_string(), "Bearer sk-from-env".to_string())]);
        let ask = Mireld.takes_key().unwrap();
        assert_eq!((ask.secret, ask.url), ("mireld", "https://portal.mireld.my/"));
    }

    #[test]
    fn refused_offline_timeout_busy_and_malformed() {
        assert_eq!(typed(ok(401, REFUSED)).1, Err(Trouble::Refused));
        assert_eq!(typed(Err(NetFail::Timeout)).1, Err(Trouble::Offline));
        assert_eq!(typed(Err(NetFail::Network("no route".into()))).1, Err(Trouble::Offline));
        assert_eq!(typed(Ok(Reply { status: 429, body: String::new(), retry_after_s: Some(20) })).1, Err(Trouble::Busy { retry_after_s: Some(20) }));
        assert_eq!(typed(ok(502, "<html>")).1, Err(Trouble::Offline));
        assert!(matches!(typed(ok(200, "<html>portal</html>")).1, Err(Trouble::Unreadable(_))));
        assert!(matches!(typed(ok(200, r#"{"object":"list"}"#)).1, Err(Trouble::Unreadable(_))));
        // A key that can't ride in a header never leaves.
        let (asked, out) = refresh(&Dirs::sample(Os::Windows), &[Got::Text("sk-12 34".into())], Reach::Online, |_| unreachable!());
        assert_eq!((asked.len(), out), (0, Err(Trouble::Refused)));
        // A local refresh asks nothing.
        let (asked, out) = refresh(&Dirs::sample(Os::Windows), &[Got::Text("sk-x".into())], Reach::Local, |_| unreachable!());
        assert_eq!((asked.len(), out), (0, Err(Trouble::NeedsOnline)));
    }
}
