//! The Today page's news: the top stories of one feed the person picks
//! (Settings › General › News on Today), read while the page is open.
//!
//! A feed is RSS 2.0 or Atom, as every news site publishes. The page shows
//! its first [`MAX_STORIES`] stories from the last [`RECENT_S`], in the
//! feed's own order (its editors' order of importance), each as its
//! headline, its source and time, and one key point: the first sentence of
//! its summary. Nothing is invented: a story without a summary shows its
//! headline alone, one without a date shows no time, and an answer that
//! isn't a feed is `Malformed`.
//!
//! Pure: the presets, reading the answer and the words. The HTTP is the
//! apps' (`services::news`), only while the Today page is on screen.

use crate::datetime::days_from_civil;

/// The feeds offered in Settings: (name, address). The first is the default.
/// Each is a news organisation's own public feed: no key, no account.
pub const PRESETS: [(&str, &str); 4] = [
    ("BBC World", "https://feeds.bbci.co.uk/news/world/rss.xml"),
    ("CNA", "https://www.channelnewsasia.com/api/v1/rss-outbound-feed?_format=xml"),
    ("Al Jazeera", "https://www.aljazeera.com/xml/rss/all.xml"),
    ("Bernama", "https://www.bernama.com/en/rssfeed.php"),
];

/// Stories shown: the feed's first ones, which its editors put on top.
pub const MAX_STORIES: usize = 4;
/// Only stories from the last day and a half count as current.
pub const RECENT_S: i64 = 36 * 3600;
/// Read again when the page opens and the last read is this old.
pub const FRESH_S: i64 = 15 * 60;
/// After a failure, while the page stays open: 30 s doubling to 10 minutes.
pub const RETRY_FIRST_S: u64 = 30;
pub const RETRY_LAST_S: u64 = 600;
/// A key point is at most this many characters.
const POINT_MAX: usize = 160;
/// Never read more than this many stories from one answer.
const READ_MAX: usize = 60;

/// The feed's address: the setting (a preset's name, or an http(s) address
/// typed into settings.json), else the first preset.
pub fn feed_url(setting: &str) -> String {
    let s = setting.trim();
    if let Some((_, url)) = PRESETS.iter().find(|(name, _)| name.eq_ignore_ascii_case(s)) {
        return url.to_string();
    }
    if s.is_empty() {
        return PRESETS[0].1.to_string();
    }
    s.to_string()
}

/// The setting's place among the presets (None: an address of its own).
pub fn preset_index(setting: &str) -> Option<usize> {
    let s = setting.trim();
    if s.is_empty() {
        return Some(0);
    }
    PRESETS.iter().position(|(name, url)| name.eq_ignore_ascii_case(s) || *url == s)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Story {
    pub title: String,
    /// Opened on a click; https only, else None.
    pub link: Option<String>,
    /// When it was published (unix), if the feed says.
    pub at: Option<i64>,
    /// Who published it: the item's own source (an aggregator's), else the feed's name.
    pub source: String,
    /// The first sentence of its summary, if it has one that says more than the headline.
    pub point: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct News {
    /// The feed's own name.
    pub source: String,
    pub stories: Vec<Story>,
}

/// Read a feed's answer (its bytes, in the encoding it declares) and keep the
/// current top stories at `now` (unix). `Err(())`: not an RSS or Atom feed.
pub fn parse(bytes: &[u8], now: i64) -> Result<News, ()> {
    let text = decode(bytes);
    let rss = text.contains("<rss") || text.contains("<channel");
    let atom = !rss && blocks(&text, "feed").next().is_some();
    let (item_tag, head_end) = if atom { ("entry", "<entry") } else { ("item", "<item") };
    if !atom && !rss {
        return Err(());
    }
    let head = &text[..text.find(head_end).unwrap_or(text.len())];
    let feed_name = element(head, "title").map(|t| clean(&t)).filter(|t| !t.is_empty()).unwrap_or_else(|| "News".into());
    // A site's name, without its slogan: "Al Jazeera – Breaking News, …".
    let feed_name = feed_name.split(" – ").next().unwrap_or(&feed_name).split(" - ").next().unwrap_or(&feed_name).trim().to_string();
    let mut stories = Vec::new();
    for block in blocks(&text, item_tag).take(READ_MAX) {
        let Some(title) = element(block, "title").map(|t| clean(&t)).filter(|t| !t.is_empty()) else { continue };
        let link = if atom { atom_link(block) } else { element(block, "link").map(|l| clean(&l)) };
        let at = if atom {
            element(block, "updated").or_else(|| element(block, "published")).and_then(|d| rfc3339(d.trim()))
        } else {
            element(block, "pubDate").or_else(|| element(block, "dc:date")).and_then(|d| rfc822(d.trim()).or_else(|| rfc3339(d.trim())))
        };
        // An aggregator's item says who published it, and puts it after the headline too.
        let own = element(block, "source").map(|s| clean(&s)).filter(|s| !s.is_empty());
        let title = match &own {
            Some(src) => title.strip_suffix(&format!(" - {src}")).unwrap_or(&title).to_string(),
            None => title,
        };
        let summary = element(block, if atom { "summary" } else { "description" }).or_else(|| element(block, "content")).map(|s| clean(&s));
        let point = summary.and_then(|s| key_point(&s, &title));
        stories.push(Story { link: link.filter(|l| l.starts_with("https://") && !l.contains(char::is_whitespace)), at, source: own.unwrap_or_else(|| feed_name.clone()), point, title });
    }
    // Current only: a dated story older than RECENT_S (or from the future,
    // a clock gone wrong) is left out; an undated one is the feed's say.
    stories.retain(|s| s.at.is_none_or(|t| now - t <= RECENT_S && t - now <= 3600));
    stories.truncate(MAX_STORIES);
    Ok(News { source: feed_name, stories })
}

/// The bytes as text: Latin-1 when the XML declaration says so, else UTF-8.
fn decode(bytes: &[u8]) -> String {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]).to_ascii_lowercase();
    if head.contains("encoding=\"iso-8859-1\"") || head.contains("encoding='iso-8859-1'") {
        bytes.iter().map(|&b| b as char).collect()
    } else {
        String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)).into_owned()
    }
}

/// Every `<tag …>…</tag>` block, in order.
fn blocks<'a>(text: &'a str, tag: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut rest = text;
    std::iter::from_fn(move || loop {
        let at = rest.find(&open)?;
        let after = &rest[at + open.len()..];
        // `<item>` or `<item …>`, not `<itemFoo>`.
        if !after.starts_with(['>', ' ', '\n', '\r', '\t']) {
            rest = after;
            continue;
        }
        let end = after.find(&close)?;
        let block = &after[..end];
        rest = &after[end + close.len()..];
        return Some(block);
    })
}

/// The text inside the first `<tag …>…</tag>` (CDATA unwrapped), raw.
fn element(block: &str, tag: &str) -> Option<String> {
    let inner = blocks(block, tag).next().map(|b| b.split_once('>').map_or("", |(_, rest)| rest))?;
    let mut out = String::new();
    let mut rest = inner;
    while let Some(i) = rest.find("<![CDATA[") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 9..];
        let end = after.find("]]>").unwrap_or(after.len());
        out.push_str(&after[..end].replace('&', "&amp;").replace('<', "&lt;"));
        rest = after.get(end + 3..).unwrap_or("");
    }
    out.push_str(rest);
    Some(out)
}

/// An Atom entry's link: the `alternate` one, else the first.
fn atom_link(block: &str) -> Option<String> {
    let mut first = None;
    let mut rest = block;
    while let Some(i) = rest.find("<link") {
        let tag_end = rest[i..].find('>').map_or(rest.len(), |e| i + e);
        let tag = &rest[i..tag_end];
        let href = attr(tag, "href").map(|h| unescape(&h));
        if attr(tag, "rel").is_none_or(|r| r == "alternate") && href.is_some() {
            return href;
        }
        first = first.or(href);
        rest = &rest[tag_end..];
    }
    first
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!("{name}=");
    let i = tag.find(&key)?;
    let rest = &tag[i + key.len()..];
    let q = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let rest = &rest[1..];
    Some(rest[..rest.find(q)?].to_string())
}

/// Entity-escaped feed text as plain text: entities decoded (twice where a
/// feed escapes its HTML), tags dropped, spaces collapsed.
fn clean(raw: &str) -> String {
    let once = unescape(raw);
    let text = if once.contains('<') { strip_tags(&once) } else { once };
    let text = unescape(&text);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let end = after.find(';').filter(|e| *e <= 10);
        let decoded = end.and_then(|e| {
            let name = &after[..e];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                "ndash" => Some('–'),
                "mdash" => Some('—'),
                "hellip" => Some('…'),
                "rsquo" => Some('’'),
                "lsquo" => Some('‘'),
                "ldquo" => Some('“'),
                "rdquo" => Some('”'),
                n if n.starts_with("#x") || n.starts_with("#X") => u32::from_str_radix(&n[2..], 16).ok().and_then(char::from_u32),
                n if n.starts_with('#') => n[1..].parse().ok().and_then(char::from_u32),
                _ => None,
            }?;
            Some((c, e))
        });
        match decoded {
            Some((c, e)) => {
                out.push(c);
                rest = &after[e + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The summary's first sentence, when it says more than the headline: a
/// summary that only repeats it, or is a list of links (an aggregator's),
/// gives none.
fn key_point(summary: &str, title: &str) -> Option<String> {
    let s = summary.trim();
    // Too short, the headline again, or (an aggregator's) a list of links
    // that opens with the headline itself.
    if s.len() < 25 || s.to_lowercase().starts_with(&title.to_lowercase()) {
        return None;
    }
    // Bernama-style datelines: "KUALA LUMPUR, Nov 13 (Bernama) -- ".
    let s = s.split_once(" -- ").filter(|(head, _)| head.len() < 60 && head.contains('(')).map_or(s, |(_, rest)| rest).trim();
    let mut end = s.len();
    for (i, c) in s.char_indices() {
        if matches!(c, '.' | '!' | '?') && i > 20 && s[i + c.len_utf8()..].starts_with(' ') {
            end = i + c.len_utf8();
            break;
        }
    }
    let sentence = s[..end].trim();
    let point = if sentence.chars().count() > POINT_MAX {
        let cut: String = sentence.chars().take(POINT_MAX).collect();
        format!("{}…", cut[..cut.rfind(' ').unwrap_or(cut.len())].trim_end_matches([',', ';', ':']))
    } else {
        sentence.to_string()
    };
    (!point.eq_ignore_ascii_case(title) && point.len() >= 20).then_some(point)
}

/// "Fri, 13 Nov 2026 04:43:48 -0500" (RSS) → unix. Zones: GMT, UT, UTC, Z,
/// the US letters, or ±hhmm.
pub fn rfc822(s: &str) -> Option<i64> {
    let s = s.split_once(',').map_or(s, |(_, rest)| rest).trim();
    let mut p = s.split_whitespace();
    let day: u8 = p.next()?.parse().ok()?;
    let month = month(p.next()?)?;
    let year: i32 = p.next()?.parse().ok()?;
    let year = if year < 100 { 2000 + year } else { year };
    let mut hms = p.next()?.split(':');
    let (h, m): (i64, i64) = (hms.next()?.parse().ok()?, hms.next()?.parse().ok()?);
    let sec: i64 = hms.next().map_or(Some(0), |x| x.parse().ok())?;
    let zone = match p.next().unwrap_or("GMT") {
        "GMT" | "UT" | "UTC" | "Z" => 0,
        "EST" => -5 * 60,
        "EDT" => -4 * 60,
        "CST" => -6 * 60,
        "CDT" => -5 * 60,
        "MST" => -7 * 60,
        "MDT" => -6 * 60,
        "PST" => -8 * 60,
        "PDT" => -7 * 60,
        z => offset(z)?,
    };
    valid(year, month, day, h, m, sec)?;
    Some(days_from_civil(year, month, day) * 86_400 + h * 3600 + m * 60 + sec - zone * 60)
}

/// "2026-11-13T02:15:00Z" or "…+08:00" (Atom) → unix.
pub fn rfc3339(s: &str) -> Option<i64> {
    let (date, time) = s.split_once(['T', ' '])?;
    let mut d = date.split('-');
    let (year, month, day): (i32, u8, u8) = (d.next()?.parse().ok()?, d.next()?.parse().ok()?, d.next()?.parse().ok()?);
    let zone_at = time.find(['Z', 'z', '+', '-']).unwrap_or(time.len());
    let (clock, zone) = time.split_at(zone_at);
    let mut hms = clock.split(':');
    let (h, m): (i64, i64) = (hms.next()?.parse().ok()?, hms.next()?.parse().ok()?);
    let sec: i64 = hms.next().map_or(Some(0), |x| x.split('.').next()?.parse().ok())?;
    let zone = match zone {
        "" | "Z" | "z" => 0,
        z => offset(&z.replace(':', ""))?,
    };
    valid(year, month, day, h, m, sec)?;
    Some(days_from_civil(year, month, day) * 86_400 + h * 3600 + m * 60 + sec - zone * 60)
}

/// "+0800" / "-0500" → minutes east of UTC.
fn offset(z: &str) -> Option<i64> {
    let sign = match z.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let digits = &z[1..];
    if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (h, m): (i64, i64) = (digits[..2].parse().ok()?, digits[2..].parse().ok()?);
    Some(sign * (h * 60 + m))
}

fn month(m: &str) -> Option<u8> {
    const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let m = m.get(..3)?.to_ascii_lowercase();
    MONTHS.iter().position(|x| *x == m).map(|i| i as u8 + 1)
}

fn valid(year: i32, month: u8, day: u8, h: i64, m: i64, s: i64) -> Option<()> {
    ((1970..=9999).contains(&year) && (1..=12).contains(&month) && (1..=31).contains(&day) && (0..24).contains(&h) && (0..60).contains(&m) && (0..=60).contains(&s)).then_some(())
}

/// Made-up stories for the self-test's and the Mac renders' pictures, around
/// `now` (the self-test asks no feed). Never shown by a running app.
pub fn sample(now: i64) -> News {
    let story = |title: &str, mins: i64, point: Option<&str>| Story {
        title: title.into(),
        link: Some("https://news.example.org/story".into()),
        at: Some(now - mins * 60),
        source: "Example News".into(),
        point: point.map(str::to_string),
    };
    News {
        source: "Example News".into(),
        stories: vec![
            story("Central bank holds its rate as inflation eases", 18, Some("The bank kept its benchmark rate unchanged for a third meeting.")),
            story("Flood warnings lifted along the east coast", 64, Some("Rivers fell overnight and the last evacuation centres closed.")),
            story("Regional leaders open talks on a shared power grid", 150, None),
            story("New rail link cuts the cross-border trip to 90 minutes", 300, Some("Trains start next month, four a day in each direction.")),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BBC: &[u8] = include_bytes!("../tests/fixtures/news/bbc-world.xml");
    const ALJAZEERA: &[u8] = include_bytes!("../tests/fixtures/news/aljazeera.xml");
    const GOOGLE: &[u8] = include_bytes!("../tests/fixtures/news/google-news-my.xml");
    const BERNAMA: &[u8] = include_bytes!("../tests/fixtures/news/bernama.xml");
    const ATOM: &[u8] = include_bytes!("../tests/fixtures/news/atom.xml");
    /// 2026-11-13 10:00 UTC, when the fixtures were taken.
    const NOW: i64 = 1_794_564_000;

    #[test]
    fn dates_in_every_zone() {
        assert_eq!(rfc822("Fri, 13 Nov 2026 04:43:48 -0500"), rfc822("Fri, 13 Nov 2026 09:43:48 GMT"));
        assert_eq!(rfc822("Fri, 13 Nov 2026 09:43:30 +0000"), Some(1_794_563_010));
        assert_eq!(rfc822("13 Nov 2026 09:43:30 EST"), rfc822("13 Nov 2026 14:43:30 GMT"));
        assert_eq!(rfc3339("2026-11-12T20:00:00+08:00"), rfc3339("2026-11-12T12:00:00Z"));
        assert_eq!(rfc3339("2026-11-13T09:43:30.123Z"), Some(1_794_563_010));
        for bad in ["", "yesterday", "Fri, 32 Nov 2026 04:43:48 GMT", "Fri, 13 Nov 2026 25:00:00 GMT", "Fri, 13 Nov 2026 04:43:48 +5", "2026-13-01T00:00:00Z"] {
            assert!(rfc822(bad).is_none() && rfc3339(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn a_real_rss_feed_reads_to_its_top_stories() {
        let n = parse(BBC, NOW).unwrap();
        assert_eq!(n.source, "BBC News");
        assert!(!n.stories.is_empty() && n.stories.len() <= MAX_STORIES);
        let s = &n.stories[0];
        assert!(s.title.starts_with("Ex-Nvidia CEO Jensen Huang"), "{}", s.title);
        assert!(!s.title.contains("CDATA"));
        assert!(s.link.as_deref().is_some_and(|l| l.starts_with("https://www.bbc.")));
        assert_eq!((s.at, s.source.as_str()), (rfc822("Fri, 13 Nov 2026 03:57:01 GMT"), "BBC News"));
        assert!(s.point.as_deref().is_some_and(|p| p.starts_with("Huang abruptly quit")), "{:?}", s.point);
        // The site's slogan goes, entities are decoded.
        let aj = parse(ALJAZEERA, NOW).unwrap();
        assert_eq!(aj.source, "Al Jazeera");
        assert!(aj.stories.iter().all(|s| !s.title.contains('&') || s.title.contains(" & ")));
    }

    #[test]
    fn an_aggregators_items_keep_their_own_source_and_no_link_soup() {
        let g = parse(GOOGLE, NOW).unwrap();
        let s = &g.stories[0];
        assert_eq!(s.source, "CNA");
        assert_eq!(s.title, "Malaysia PM Anwar yet to accept Loke’s resignation as transport minister");
        // Its summary is only links to the same story: no key point.
        assert!(g.stories.iter().all(|s| s.point.is_none()), "{:?}", g.stories);
    }

    #[test]
    fn latin_1_and_undated_stories() {
        let b = parse(BERNAMA, NOW).unwrap();
        assert_eq!(b.source, "BERNAMA");
        assert!(!b.stories.is_empty() && b.stories.iter().all(|s| s.at.is_none()));
        let p = b.stories[0].point.as_deref().unwrap();
        assert!(!p.contains("Bernama) --") && !p.contains('<'), "{p}");
    }

    #[test]
    fn atom_feeds_read_too() {
        let a = parse(ATOM, rfc3339("2026-11-13T03:00:00Z").unwrap()).unwrap();
        assert_eq!(a.source, "Example Atom feed");
        assert_eq!(a.stories[0].title, "Rates & markets: central bank holds");
        assert_eq!(a.stories[0].link.as_deref(), Some("https://news.example.org/a/1"));
        assert_eq!(a.stories[0].point.as_deref(), Some("The bank kept its rate at 3%."));
        assert_eq!(a.stories[1].at, rfc3339("2026-11-12T12:00:00Z"));
        assert_eq!(a.stories[1].point, None, "no summary: the headline alone");
    }

    #[test]
    fn only_current_stories_and_safe_links() {
        // Two days on, every dated story is old: an honest empty list.
        assert!(parse(BBC, NOW + 2 * 86_400).unwrap().stories.is_empty());
        let feed = br#"<rss><channel><title>T</title>
            <item><title>A</title><link>javascript:alert(1)</link></item>
            <item><title>B</title><link>http://plain.example/b</link></item>
            <item><title>  </title><link>https://x.example/c</link></item>
            <item><title>D</title><link>https://x.example/d</link></item></channel></rss>"#;
        let n = parse(feed, NOW).unwrap();
        assert_eq!(n.stories.iter().map(|s| (s.title.as_str(), s.link.is_some())).collect::<Vec<_>>(), [("A", false), ("B", false), ("D", true)]);
    }

    #[test]
    fn what_isnt_a_feed_is_malformed() {
        for bad in [&b"<html><body>Not found</body></html>"[..], b"", b"{\"error\":1}", b"<feedback/>"] {
            assert_eq!(parse(bad, NOW), Err(()), "{}", String::from_utf8_lossy(bad));
        }
        // A real feed with no items is empty, not malformed.
        assert_eq!(parse(b"<rss><channel><title>Quiet</title></channel></rss>", NOW).unwrap().stories, vec![]);
    }

    #[test]
    fn the_setting_picks_a_preset_or_an_address() {
        assert_eq!(feed_url(""), PRESETS[0].1);
        assert_eq!(feed_url("cna"), PRESETS[1].1);
        assert_eq!(feed_url("https://example.org/feed.xml"), "https://example.org/feed.xml");
        assert_eq!((preset_index(""), preset_index("Bernama"), preset_index(PRESETS[2].1), preset_index("https://example.org/f")), (Some(0), Some(3), Some(2), None));
        for (_, url) in PRESETS {
            assert!(crate::net::configured(url) && url.starts_with("https://"), "{url}");
        }
    }
}
