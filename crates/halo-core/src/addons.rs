//! Add-ons: features that download only for the people who switch them on.
//!
//! An add-on's code is part of the app and stays asleep while it is off. The
//! switch downloads the add-on's files (data and sounds) from a pre-release
//! on the app's GitHub repository, and switching it off deletes them. Each
//! file's size and SHA-256 are built into the app, so a changed or broken
//! file is refused; new files come with a new app release (a new `tag`).
//! The apps' `services::addons` (one file, the Windows app's, which the Mac
//! app runs too) does the downloading.

use crate::phonedrop::fmt_bytes;
use crate::settings::Settings;
use crate::sha256;

/// Where the files live: `<RELEASES>/<tag>/<name>`.
pub const RELEASES: &str = "https://github.com/kolijijani/halo-island/releases/download";

/// A public commit of the repository that holds the add-ons' own files byte
/// for byte (`addons/`, the dictionary in `crates/halo-core/data/`). A file
/// whose release asset isn't there (404: the pre-release not published yet)
/// comes from this commit instead, checked against the same size and SHA-256,
/// so the source changes and the bytes can't.
/// ponytail: the mirror goes once every add-on's pre-release is published.
pub const MIRROR: &str = "https://raw.githubusercontent.com/kolijijani/halo-island/47aff8d62e5b52d396409b74d7df9b2195957ab3";

/// Each mirrored file: its release tag and name, and its path in the repository.
const MIRRORED: [(&str, &str, &str); 6] = [
    ("addon-sholat-3", "places.json", "addons/sholat/places.json"),
    ("addon-sholat-3", "adzan.m4a", "addons/sholat/adzan.m4a"),
    ("addon-sholat-3", "NOTICE.txt", "addons/sholat/NOTICE.txt"),
    ("addon-ja-1", JA_READINGS_FILE, "crates/halo-core/data/ja-readings.dic.zst"),
    ("addon-ja-1", "NOTICE.txt", "addons/ja/NOTICE.txt"),
    ("addon-downloader-1", "NOTICE.txt", "addons/downloader/NOTICE.txt"),
];

/// What one download of an add-on file came to ([`verdict`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The exact file: keep it.
    Good,
    /// Not on the release (yet): ask the mirror.
    TryMirror,
    /// No internet, a timeout, the server down or busy: try again later.
    Retry(crate::net::Failure),
    /// Bytes came that aren't the file (a changed or cut-off file, an error
    /// page): thrown away, and not tried again until off and on.
    Wrong,
    /// The server has no such file, or refuses it (this HTTP status).
    Unavailable(u16),
}

/// Judge one download of `file`: `got` is the status and bytes, or why there
/// was no answer; `mirror` whether a mirror is left to ask.
pub fn verdict(file: &AddonFile, mirror: bool, got: Result<(u16, &[u8]), crate::net::Failure>) -> Verdict {
    use crate::net::Failure;
    match got {
        Ok((200, bytes)) if file.matches(bytes) => Verdict::Good,
        Ok((200, _)) => Verdict::Wrong,
        Ok((404 | 403 | 410, _)) if mirror => Verdict::TryMirror,
        Ok((s, _)) if Failure::Status(s).retries() => Verdict::Retry(Failure::Status(s)),
        Ok((s, _)) => Verdict::Unavailable(s),
        Err(f) if f.retries() => Verdict::Retry(f),
        Err(_) => Verdict::Unavailable(0),
    }
}

/// One file an add-on downloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddonFile {
    pub name: &'static str,
    pub size: u64,
    /// SHA-256, lowercase hex.
    pub sha256: &'static str,
    /// Where it comes from when that isn't the add-on's own release: a tool's
    /// own release, unchanged (the Downloader's yt-dlp and QuickJS). Saved
    /// under `name` all the same.
    pub url: Option<&'static str>,
}

impl AddonFile {
    /// True when `bytes` are exactly this file.
    pub fn matches(&self, bytes: &[u8]) -> bool {
        bytes.len() as u64 == self.size && sha256::hex(&sha256::sha256(bytes)) == self.sha256
    }

    /// The same for a file read from `r` 64 KB at a time, so checking one on
    /// disk never holds all of it in memory (the Downloader's yt-dlp is 24 MB).
    pub fn matches_read(&self, mut r: impl std::io::Read) -> bool {
        let (mut hash, mut n, mut buf) = (sha256::Sha256::new(), 0u64, vec![0u8; 64 * 1024]);
        loop {
            match r.read(&mut buf) {
                Ok(0) => break,
                Ok(k) => {
                    n += k as u64;
                    if n > self.size {
                        return false;
                    }
                    hash.update(&buf[..k]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return false,
            }
        }
        n == self.size && sha256::hex(&hash.finish()) == self.sha256
    }

    /// Whether this build of the app knows the file: a size and a SHA-256 to
    /// check it against. A placeholder (size 0, no SHA-256) stands for a file
    /// that isn't published yet, and nothing is downloaded for it.
    pub fn pinned(&self) -> bool {
        self.size > 0 && self.sha256.len() == 64 && self.sha256.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Addon {
    /// Stable id: the key in settings.json and the folder's name.
    pub id: &'static str,
    pub title: &'static str,
    /// What it does, in one line, for the Add-ons page.
    pub blurb: &'static str,
    /// The same in the Mac app, where `blurb` gives Windows' reason (None:
    /// `blurb` there too).
    pub mac_blurb: Option<&'static str>,
    /// The release that holds this version of the files.
    pub tag: &'static str,
    pub files: &'static [AddonFile],
    /// Part of Halo Island Pro: it works only for an Halo account the
    /// server says has Pro (`GET /island/pro`; `AppState::pro`).
    pub pro: bool,
}

impl Addon {
    /// Every file together, in bytes.
    pub fn size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    pub fn url(&self, file: &AddonFile) -> String {
        match file.url {
            Some(url) => url.to_string(),
            None => format!("{RELEASES}/{}/{}", self.tag, file.name),
        }
    }

    /// Where `file` also lives ([`MIRROR`]), when its release asset is missing.
    pub fn mirror_url(&self, file: &AddonFile) -> Option<String> {
        let path = MIRRORED.iter().find(|(tag, name, _)| *tag == self.tag && *name == file.name)?.2;
        Some(format!("{MIRROR}/{path}"))
    }

    pub fn find(id: &str) -> Option<&'static Addon> {
        FETCHED.iter().copied().find(|a| a.id == id)
    }

    /// Whether it has files to download. One without (Script reader) is
    /// already in the app: the switch only shows or hides it, and the
    /// downloader never touches it.
    pub fn downloads(&self) -> bool {
        !self.files.is_empty()
    }

    /// Whether this build can get every file it needs ([`AddonFile::pinned`]).
    /// One that can't stays off ([`Status::Unpinned`]): a build made while a
    /// file is still a placeholder (a new add-on whose files aren't on its
    /// release yet).
    pub fn pinned(&self) -> bool {
        self.files.iter().all(AddonFile::pinned)
    }
}

/// Prayer times: the place list and the adzan (`addons/sholat/` in the repo).
/// `addon-sholat-3` brought a clearer adzan; versions up to 0.2.24 still
/// download `addon-sholat-1`, so never delete it.
pub const PRAYER_TIMES: Addon = Addon {
    id: "sholat",
    title: "Prayer times",
    blurb: "Today's times for your city and a reminder in the island.",
    mac_blurb: None,
    tag: "addon-sholat-3",
    files: &[
        AddonFile { name: "places.json", size: 79_687, sha256: "4d2e1cd6c277b97273f415cf0460579db143fe92ac88f6e4bddb79cdf8cbf69b", url: None },
        AddonFile { name: "adzan.m4a", size: 878_695, sha256: "6b8097fa7dfa43bd61ce65894396518ea65dd9d940e56b251ffc4ae5e78c7420", url: None },
        AddonFile { name: "NOTICE.txt", size: 967, sha256: "f73117285be65ea432411cbf80f4db05bac74c1477b36e829b8fa3a5e7bdcfd1", url: None },
    ],
    pro: false,
};

/// The Japanese readings add-on's dictionary file (`romanize::JaDict`).
pub const JA_READINGS_FILE: &str = "ja-readings.dic.zst";

/// Japanese readings: Halo Island's own reading dictionary, for romaji under
/// kanji on Macs and on PCs where Windows has no Japanese reading data (most
/// PCs outside Japan). Only people who play Japanese songs need it: the first
/// song whose kanji go unread switches it on ([`ja_switches_on`], once;
/// switched off by hand it stays off, and the Music page offers it under the
/// lyrics). The file is
/// `crates/halo-core/data/ja-readings.dic.zst` (tools/make_ja_dict.py), its
/// notice `addons/ja/NOTICE.txt`.
pub const JAPANESE_READINGS: Addon = Addon {
    id: "ja",
    title: "Japanese readings",
    blurb: "Romaji under kanji in Japanese lyrics, where Windows can't read them.",
    mac_blurb: Some("Romaji under kanji in Japanese lyrics."),
    tag: "addon-ja-1",
    files: &[
        AddonFile { name: JA_READINGS_FILE, size: 3_790_036, sha256: "4950e5e42a2a48ad50396304c6bfbb8f9a97b33947e147007943361d65d856c8", url: None },
        AddonFile { name: "NOTICE.txt", size: 4_797, sha256: "bb8fe3b2285d8898234fb6e368554a41e903b7a44fb7f5a6720d14a0e7883cea", url: None },
    ],
    pro: false,
};

/// Script reader: the Script page, a teleprompter under the webcam. Its
/// code is in the app (a few dozen KB), so there is nothing to download; off,
/// the island has no Script tab, no Alt+Shift+S and nothing running for it.
pub const SCRIPT_READER: Addon = Addon {
    id: "script",
    title: "Script reader",
    blurb: "A teleprompter under your webcam, left out of screen shares.",
    mac_blurb: Some("A teleprompter under your webcam."),
    tag: "",
    files: &[],
    pro: false,
};

/// System: the System page (the processor, graphics, memory, drives,
/// network and power, and the apps using them). A tab for everyone up to
/// 0.2.8; an add-on, off, in the versions after it, so the island's own tabs
/// are the everyday ones. Its code is in the app, so there is nothing to download.
/// Off, there is no System tab, and its readings never start at rest
/// (`Settings::system_history` only counts while it is on). Show over games
/// keeps its own switch: its readout reads the counters while a game is in
/// front, System on or off.
pub const SYSTEM: Addon = Addon {
    id: "system",
    title: "System",
    blurb: "Live CPU, GPU, memory, drives, network and power, and the apps using them.",
    mac_blurb: None,
    tag: "",
    files: &[],
    pro: false,
};

/// Stocks: the Stocks page, the FTSE Bursa Malaysia KLCI and up to ten
/// stocks from anywhere with their day's line, from Yahoo Finance
/// (`halo_core::stocks`). Its code is in the
/// app, so there is nothing to download. Off, there is no Stocks tab; on, the
/// prices are asked for only while its page is on screen.
pub const STOCKS: Addon = Addon {
    id: "stocks",
    title: "Stocks",
    blurb: "Bursa Malaysia's KLCI and up to 10 stocks from anywhere, from Yahoo Finance.",
    mac_blurb: None,
    tag: "",
    files: &[],
    pro: false,
};

/// Caffeine: the Caffeine page, drinks the person adds (an espresso drink by
/// its shots and beans, the rest by serving), how much is still in them hour
/// by hour, and a reminder before the day's last call (`halo_core::caffeine`).
/// Its code is in the app, so there is nothing to download. Off, there is no
/// Caffeine tab and no reminder.
pub const CAFFEINE: Addon = Addon {
    id: "caffeine",
    title: "Caffeine",
    blurb: "Add what you drink and see how much caffeine is still in you at bedtime, with a reminder before your last coffee.",
    mac_blurb: None,
    tag: "",
    files: &[],
    pro: false,
};

/// Screen recorder: the Record page and the recording pill, the face of
/// Halo Island Recorder (its own install, about 120 MB). Its code is in the
/// island, so there is nothing to download. It shows on the Add-ons page once
/// the recorder is installed, and is on from then until switched off
/// (`Settings::recorder_on`). Off, there is no Record tab, and the recorder
/// shows its own bar and controls.
pub const SCREEN_RECORDER: Addon = Addon {
    id: "recorder",
    title: "Screen recorder",
    blurb: "Record your screen from the island, with Halo Island Recorder.",
    mac_blurb: None,
    tag: "",
    files: &[],
    pro: false,
};

/// Downloader: the Downloads page, which saves the video or sound of a link
/// with yt-dlp (`ytdlp`, `downloads`). yt-dlp and QuickJS (which yt-dlp needs
/// for YouTube) come from their own releases, unchanged; FFmpeg, which joins
/// a video's picture and sound, is our own small build
/// (`tools/build-ffmpeg-min.sh`, `addons/downloader/`). Not in the Microsoft
/// Store build. Nothing runs at rest: the tools only run while a download
/// does. Part of Studio Pro: its tools download, and it downloads, only for
/// an account the server says has Pro. Each app has its own tools: this
/// one, or [`DOWNLOADER_MAC`] on a Mac.
pub const DOWNLOADER: Addon = if cfg!(target_os = "macos") { DOWNLOADER_MAC } else { DOWNLOADER_WINDOWS };

/// The Downloader on Windows: yt-dlp.exe and QuickJS-ng's Windows build from
/// their own releases, and our ffmpeg.exe.
pub const DOWNLOADER_WINDOWS: Addon = Addon {
    id: "downloader",
    title: "Downloader",
    blurb: "Save the video or sound of a link you paste, with yt-dlp.",
    mac_blurb: None,
    tag: "addon-downloader-1",
    files: &[
        AddonFile {
            name: "yt-dlp.exe",
            size: 23_575_744,
            sha256: "f5389d3698ecde4ef06f4e6b1ce0294fe9ac1396228c8990b8d3416d6d177a7d",
            url: Some("https://github.com/yt-dlp/yt-dlp/releases/download/2026.10.26/yt-dlp.exe"),
        },
        AddonFile {
            name: "qjs.exe",
            size: 2_151_363,
            sha256: "2aeabf0092c3262d6b2609824418f7dd7ed1f1df939f73b2b15645230cac0d77",
            url: Some("https://github.com/quickjs-ng/quickjs/releases/download/v0.17.0/qjs-windows-x86_64.exe"),
        },
        // tools/build-ffmpeg-min.sh's build (FFmpeg 9.0.2, the same bytes every time).
        AddonFile { name: "ffmpeg.exe", size: 2_298_880, sha256: "8a42638bc2c9bed20670a3e89ccb7117345a941e245d8151aa60066cac0b3993", url: None },
        AddonFile { name: "NOTICE.txt", size: 32_606, sha256: "01e2747ec4a5cbba518bbfa229941b70cc72a2cd3f9b0da11c583e209a4185c3", url: None },
    ],
    pro: true,
};

/// The Downloader on a Mac: yt-dlp's own Mac build (one file for Apple
/// silicon and Intel) from its release, unchanged, the same version as
/// Windows'. QuickJS-ng's own Mac builds are one per chip and need macOS 26,
/// so QuickJS-ng (the same version) and FFmpeg (the same build as Windows')
/// are ours, one file each for both chips and macOS 14 and later, from
/// mac.yml's downloader-tools job, on the add-on's own pre-release
/// (`addons/downloader-mac/`).
pub const DOWNLOADER_MAC: Addon = Addon {
    tag: "addon-downloader-mac-1",
    files: &[
        AddonFile {
            name: "yt-dlp",
            size: 37_466_304,
            sha256: "0f922c15ccc0b83eefa1c8bc9104e434318c4bf16e408e1ee93b7745a6efe0aa",
            url: Some("https://github.com/yt-dlp/yt-dlp/releases/download/2026.10.26/yt-dlp_macos"),
        },
        // mac.yml's downloader-tools job (QuickJS-ng v0.17.0; FFmpeg 9.0.2, tools/build-ffmpeg-min.sh darwin).
        AddonFile { name: "qjs", size: 2_456_496, sha256: "d4526ffc81f4251e7e835d789a441e367b03ba1700bd111e9ce1a1a5bec0134a", url: None },
        AddonFile { name: "ffmpeg", size: 4_307_968, sha256: "fb47d9aa82cf1e61aa49b54d1989fffef98571d1cec3d0f9a5133e0b63468724", url: None },
        AddonFile { name: "NOTICE.txt", size: 33_041, sha256: "9cabe008664215a95b89df56038612bb5551442e2383fe33be5b00cc0e5cfe8e", url: None },
    ],
    ..DOWNLOADER_WINDOWS
};

/// The Downloader's tools, as this app saves them in the add-on's folder.
pub const YTDLP_EXE: &str = if cfg!(target_os = "macos") { "yt-dlp" } else { "yt-dlp.exe" };
pub const QJS_EXE: &str = if cfg!(target_os = "macos") { "qjs" } else { "qjs.exe" };
pub const FFMPEG_EXE: &str = if cfg!(target_os = "macos") { "ffmpeg" } else { "ffmpeg.exe" };

/// Whether `name` is one of the Downloader's programs, which a Mac only runs
/// with its execute bit set (`services::addons`).
pub fn is_downloader_tool(name: &str) -> bool {
    [YTDLP_EXE, QJS_EXE, FFMPEG_EXE].contains(&name)
}

/// AI usage: the AI tab (`studio_usage`), what's left of each AI plan on this
/// computer and when it resets. A tab for everyone up to 0.2.25; part of
/// Studio Pro in the versions after it. Its code is in the app, so there is
/// nothing to download. Without Pro there is no AI tab or Settings section,
/// and nothing is read. With Pro it's on until switched off
/// ([`Settings::ai_usage_on`]).
pub const AI_USAGE: Addon = Addon {
    id: "ai",
    title: "AI usage",
    blurb: "Track your AI usage.",
    mac_blurb: None,
    tag: "",
    files: &[],
    pro: true,
};

/// Every add-on, in the order the Add-ons page lists them.
/// Health (steps from a phone) left after 0.2.30: Halo Island has no phone or
/// watch connection to read them from. A settings file that still has it on
/// is left as it is; no add-on answers to that id.
pub const ALL: [&Addon; 9] = [&PRAYER_TIMES, &JAPANESE_READINGS, &SCRIPT_READER, &SYSTEM, &STOCKS, &CAFFEINE, &SCREEN_RECORDER, &AI_USAGE, &DOWNLOADER];

/// The adzan voices besides the add-on's own (`adzan.m4a`): each its own
/// file on `addon-sholat-4`, downloaded only once someone picks it in
/// Prayer times' Sound (`PrayerSound`) and deleted when they pick another.
/// They aren't rows on the Add-ons page; the pick is their switch.
/// `addons/sholat/NOTICE.txt` says where each comes from and its licence.
pub const ADZAN_MAKKAH: Addon = Addon {
    id: "sholat-voice-makkah",
    title: "Adzan: Masjid al-Haram, Makkah",
    blurb: "The Maghrib adzan at Masjid al-Haram, Makkah, 25 Feb 2012.",
    mac_blurb: None,
    tag: "addon-sholat-4",
    files: &[AddonFile { name: "adzan-makkah.m4a", size: 1_822_513, sha256: "bd5a9db9d23eb2fb7e3368c33afeb08603b71b762549ba8247491c5aac24cb59", url: None }],
    pro: false,
};

/// Subuh's own adzan, with "as-salatu khairum min an-naum": Subuh plays it
/// whichever voice the other prayers have.
pub const ADZAN_SUBUH: Addon = Addon {
    id: "sholat-voice-subuh",
    title: "Adzan: Subuh, Besim Azemi",
    blurb: "Subuh's adzan by Besim Azemi, Islamic Center Malmö, 19 Aug 2012.",
    mac_blurb: None,
    tag: "addon-sholat-4",
    files: &[
        AddonFile { name: "adzan-subuh.m4a", size: 2_378_259, sha256: "6cc9e03e40d1460a4b3294bd3e0ccecacbb42346597281295628016f564946e5", url: None },
        // Every recording's credit and licence (addons/sholat/NOTICE.txt), with the voices.
        AddonFile { name: "NOTICE.txt", size: 2_303, sha256: "3b65efad0b8198e8bb4ee6ea07a56d4738a14f039812370d1434bc46ffbe4c4d", url: None },
    ],
    pro: false,
};

/// The voices' add-ons: the switch is Prayer times' Sound, not a row.
pub const VOICES: [&Addon; 2] = [&ADZAN_MAKKAH, &ADZAN_SUBUH];

/// Every add-on the download worker looks after: the Add-ons page's, and
/// the adzan voices.
pub const FETCHED: [&Addon; ALL.len() + VOICES.len()] = {
    let mut out = [&PRAYER_TIMES; ALL.len() + VOICES.len()];
    let mut i = 0;
    while i < ALL.len() {
        out[i] = ALL[i];
        i += 1;
    }
    let mut j = 0;
    while j < VOICES.len() {
        out[ALL.len() + j] = VOICES[j];
        j += 1;
    }
    out
};

/// The recording `sound` plays for `prayer` (add-on and file name), or None
/// for the tone or nothing: Subuh plays Subuh's own adzan whichever voice is
/// picked (the others have no "as-salatu khairum min an-naum").
pub fn adzan_file(sound: crate::settings::PrayerSound, subuh: bool) -> Option<(&'static Addon, &'static str)> {
    use crate::settings::PrayerSound;
    match sound {
        _ if !sound.adzan() => None,
        _ if subuh => Some((&ADZAN_SUBUH, ADZAN_SUBUH.files[0].name)),
        PrayerSound::Makkah => Some((&ADZAN_MAKKAH, ADZAN_MAKKAH.files[0].name)),
        _ => Some((&PRAYER_TIMES, "adzan.m4a")),
    }
}

/// Prayer times' Sound choices as the pickers show them: the sound, its
/// short name, and who recites it (None: no recording).
pub const SOUNDS: [(crate::settings::PrayerSound, &str, Option<&str>); 4] = [
    (crate::settings::PrayerSound::Adzan, "Adzan", Some("Aaqib Azeez")),
    (crate::settings::PrayerSound::Makkah, "Makkah", Some("Masjid al-Haram, Maghrib 2012")),
    (crate::settings::PrayerSound::Tone, "Tone", None),
    (crate::settings::PrayerSound::Silent, "Silent", None),
];

/// The line under the Sound picker for `sound`: who recites it, what it
/// costs to download while it isn't here (`status`: its voice add-on's),
/// and what Subuh plays.
pub fn sound_line(sound: crate::settings::PrayerSound, status: Option<Status>) -> String {
    use crate::settings::PrayerSound;
    let Some(who) = SOUNDS.iter().find(|s| s.0 == sound).and_then(|s| s.2) else {
        return if sound == PrayerSound::Silent { "Nothing plays; the island still says when it's time.".into() } else { "A short tone at each prayer.".into() };
    };
    let cost = match (sound.voice(), status) {
        (Some(_), Some(Status::Downloading { done, total })) => format!(" \u{b7} getting it, {} of {}", fmt_bytes(done), fmt_bytes(total)),
        (Some(_), Some(Status::Unavailable(404))) => " \u{b7} not published yet, the tone plays".into(),
        (Some(_), Some(Status::Waiting(_) | Status::Failed | Status::Unavailable(_))) => " \u{b7} didn't download, the tone plays".into(),
        (Some(v), Some(Status::Off | Status::Checking) | None) => format!(" \u{b7} downloads {}", fmt_bytes(v.size())),
        _ => String::new(),
    };
    format!("{who}{cost} \u{b7} Subuh: its own adzan, Besim Azemi ({})", fmt_bytes(ADZAN_SUBUH.size()))
}

/// Whether Japanese readings switch themselves on for a song whose kanji went
/// unread for want of them (`wants`): only while the person reads
/// pronunciations, and only if they never switched the add-on on or off
/// themselves (one switched off by hand stays off).
pub fn ja_switches_on(settings: &Settings, wants: bool) -> bool {
    wants && settings.lyrics_pronunciation && settings.addon_unset(JAPANESE_READINGS.id)
}

/// What the Music page says under a Japanese song whose kanji went unread for
/// want of Japanese readings, by where that add-on stands, and whether a
/// click gets it (switches it on). None once it's in place.
pub fn ja_offer(status: Status) -> Option<(String, bool)> {
    match status {
        Status::Off => Some((format!("Romaji for kanji: get Japanese readings ({})", fmt_bytes(JAPANESE_READINGS.size())), true)),
        Status::Checking => Some(("Checking Japanese readings\u{2026}".to_string(), false)),
        Status::Downloading { done, total } => Some((format!("Getting Japanese readings \u{b7} {} of {}", fmt_bytes(done), fmt_bytes(total)), false)),
        Status::Waiting(crate::net::Failure::Offline) => Some(("Japanese readings download once you're online".to_string(), false)),
        Status::Waiting(_) => Some(("Japanese readings will try downloading again".to_string(), false)),
        Status::Failed | Status::Unpinned | Status::Unavailable(_) => Some(("Japanese readings didn't download: see Add-ons".to_string(), false)),
        Status::Ready => None,
    }
}

/// Where an add-on stands, as the Add-ons page shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Switched off: none of its files on this PC, nothing running.
    Off,
    /// Switched on; looking at the files already on this PC.
    Checking,
    /// Bytes so far of the add-on's total.
    Downloading { done: u64, total: u64 },
    /// Switched on, but the download couldn't happen now (no internet, a
    /// timeout, the server down): it starts again by itself later.
    Waiting(crate::net::Failure),
    /// Switched on, every file in place.
    Ready,
    /// A file came back different from the one the app expects, so it was
    /// thrown away. Trying again is the person's call (off, then on).
    Failed,
    /// The server has no such file, or refuses it (HTTP status): nothing to
    /// try again by itself.
    Unavailable(u16),
    /// Asked to switch on, but this build can't get its files yet
    /// ([`Addon::pinned`]): it stays off, and nothing downloads.
    Unpinned,
}

impl Status {
    /// The switch shows on for everything but `Off`, and `Unpinned`, which
    /// stays off.
    pub fn is_on(self) -> bool {
        !matches!(self, Status::Off | Status::Unpinned)
    }

    /// The line under the add-on's name; `mac`: in the Mac app.
    pub fn line(self, addon: &Addon, mac: bool) -> String {
        let here = if mac { "this Mac" } else { "this PC" };
        match self {
            Status::Off => addon.mac_blurb.filter(|_| mac).unwrap_or(addon.blurb).to_string(),
            Status::Checking => format!("Checking its files on {here}"),
            Status::Downloading { done, total } => format!("Downloading · {} of {}", fmt_bytes(done), fmt_bytes(total)),
            Status::Waiting(crate::net::Failure::Offline) => "No internet. It downloads by itself once you're back online.".to_string(),
            Status::Waiting(f) => format!("Can't download it right now ({}). It tries again by itself.", f.short().to_lowercase()),
            // GitHub says 404 for a release asset that isn't published yet.
            Status::Unavailable(404) => "Its files aren't published yet. Nothing was installed; try again after the next update.".to_string(),
            Status::Unavailable(s) => format!("Its files aren't available to download (error {s}). Nothing was installed."),
            Status::Ready if !addon.downloads() => "On".to_string(),
            Status::Ready => format!("On · {} on {here}", fmt_bytes(addon.size())),
            Status::Failed => "The download didn't come through right. Switch it off and on to try again.".to_string(),
            Status::Unpinned => "This version can't get its files yet, so it stays off.".to_string(),
        }
    }

    /// Download progress 0..1, while downloading.
    pub fn progress(self) -> Option<f32> {
        match self {
            Status::Downloading { done, total } if total > 0 => Some((done as f64 / total as f64).clamp(0.0, 1.0) as f32),
            Status::Downloading { .. } => Some(0.0),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn japanese_readings_switch_on_once_for_a_song_that_needs_them() {
        let mut s = Settings::default();
        assert!(ja_switches_on(&s, true), "the first Japanese song with unread kanji");
        assert!(!ja_switches_on(&s, false), "a song that reads without them");
        s.lyrics_pronunciation = false;
        assert!(!ja_switches_on(&s, true), "pronunciations off: nothing to read them for");
        s.lyrics_pronunciation = true;
        s.set_addon(JAPANESE_READINGS.id, true);
        assert!(!ja_switches_on(&s, true), "already on");
        s.set_addon(JAPANESE_READINGS.id, false);
        assert!(!ja_switches_on(&s, true), "switched off by hand: stays off");
        // Another add-on's switch doesn't count as an answer.
        let mut s = Settings::default();
        s.set_addon(PRAYER_TIMES.id, false);
        assert!(ja_switches_on(&s, true));
    }

    #[test]
    fn script_reader_is_built_in() {
        assert!(!SCRIPT_READER.downloads() && SCRIPT_READER.size() == 0);
        assert!(PRAYER_TIMES.downloads() && JAPANESE_READINGS.downloads());
        assert_eq!(Status::Ready.line(&SCRIPT_READER, false), "On");
        assert_eq!(Status::Off.line(&SCRIPT_READER, true), "A teleprompter under your webcam.");
        assert_eq!(Addon::find("script"), Some(&SCRIPT_READER));
    }

    #[test]
    fn pro_is_ai_usage_and_the_downloader() {
        let pro: Vec<&str> = ALL.iter().filter(|a| a.pro).map(|a| a.id).collect();
        assert_eq!(pro, vec!["ai", "downloader"]);
    }

    #[test]
    fn ai_usage_comes_with_pro_until_switched_off() {
        assert!(!AI_USAGE.downloads() && AI_USAGE.size() == 0);
        assert_eq!(Status::Off.line(&AI_USAGE, false), "Track your AI usage.");
        let mut s = Settings::default();
        assert!(!s.ai_usage_on(false), "no Pro: no AI tab");
        assert!(s.ai_usage_on(true), "Pro is the switch");
        s.set_addon(AI_USAGE.id, false);
        assert!(!s.ai_usage_on(true), "switched off by hand: stays off");
        s.set_addon(AI_USAGE.id, true);
        assert!(s.ai_usage_on(true) && !s.ai_usage_on(false));
    }

    #[test]
    fn built_in_add_ons_are_off_until_switched_on() {
        for a in [&SYSTEM, &STOCKS, &CAFFEINE] {
            assert!(!a.downloads() && a.size() == 0, "{}", a.id);
            assert_eq!(Addon::find(a.id), Some(a));
            assert_eq!(Status::Ready.line(a, false), "On");
            assert!(!Settings::default().addon_on(a.id), "{} is off for everyone until they switch it on", a.id);
        }
        // Clipboard history left the app after 0.2.8: no add-on by that id.
        assert_eq!(Addon::find("clipboard"), None);
        // Notifications left the add-ons after 0.2.24: it's on wherever Windows
        // lets the copy read notifications, and Settings' tabs row switches it off.
        assert_eq!(Addon::find("notifications"), None);
    }

    #[test]
    fn screen_recorder_is_on_once_the_recorder_is_installed() {
        assert!(!SCREEN_RECORDER.downloads() && SCREEN_RECORDER.size() == 0);
        assert_eq!(Addon::find("recorder"), Some(&SCREEN_RECORDER));
        let mut s = Settings::default();
        assert!(!s.recorder_on(false), "no recorder: nothing to show");
        assert!(s.recorder_on(true), "installing the recorder is the switch");
        s.set_addon(SCREEN_RECORDER.id, false);
        assert!(!s.recorder_on(true), "switched off by hand: stays off");
        s.set_addon(SCREEN_RECORDER.id, true);
        assert!(s.recorder_on(true) && !s.recorder_on(false));
    }

    #[test]
    fn the_adzan_voices_are_the_files_in_the_repo() {
        // Each voice's pin matches addons/sholat/, which is what addon-sholat-4 holds.
        let repo: [(&Addon, &[u8]); 2] = [
            (&ADZAN_MAKKAH, include_bytes!("../../../addons/sholat/adzan-makkah.m4a")),
            (&ADZAN_SUBUH, include_bytes!("../../../addons/sholat/adzan-subuh.m4a")),
        ];
        for (a, bytes) in repo {
            assert_eq!((a.tag, a.pro), ("addon-sholat-4", false), "{}", a.id);
            assert!(a.files[0].matches(bytes), "{}: size {} sha256 {}", a.id, bytes.len(), sha256::hex(&sha256::sha256(bytes)));
            assert_eq!(Addon::find(a.id), Some(a));
            assert!(!ALL.contains(&a), "a voice is no Add-ons row");
            assert!(FETCHED.contains(&a));
        }
        let notice: &[u8] = include_bytes!("../../../addons/sholat/NOTICE.txt");
        assert!(ADZAN_SUBUH.files[1].matches(notice), "NOTICE.txt: size {} sha256 {}", notice.len(), sha256::hex(&sha256::sha256(notice)));
        let text = std::str::from_utf8(notice).unwrap();
        for credit in ["adzan-makkah.m4a", "adzan-subuh.m4a", "CC BY 3.0", "Besim Azemi", "3omar Faruq"] {
            assert!(text.contains(credit), "{credit}");
        }
                // The older tags stay as they are: older versions download them.
        assert_eq!(PRAYER_TIMES.tag, "addon-sholat-3");
        assert_eq!(FETCHED.len(), ALL.len() + VOICES.len());
        assert_eq!(Status::Unavailable(404).line(&ADZAN_MAKKAH, false), "Its files aren't published yet. Nothing was installed; try again after the next update.");
        // Which recording plays: Subuh its own for any voice, the tone and silence none.
        use crate::settings::PrayerSound;
        assert_eq!(adzan_file(PrayerSound::Adzan, false).map(|f| f.1), Some("adzan.m4a"));
        assert_eq!(adzan_file(PrayerSound::Makkah, false).map(|f| f.1), Some("adzan-makkah.m4a"));
        assert_eq!(adzan_file(PrayerSound::Adzan, true).map(|f| f.1), Some("adzan-subuh.m4a"));
        assert_eq!(adzan_file(PrayerSound::Makkah, true).map(|f| f.1), Some("adzan-subuh.m4a"));
        assert_eq!(adzan_file(PrayerSound::Tone, true), None);
        assert_eq!(adzan_file(PrayerSound::Silent, false), None);
        // The picker's line: who, what it costs while it isn't here, and Subuh.
        assert_eq!(sound_line(PrayerSound::Makkah, Some(Status::Off)), "Masjid al-Haram, Maghrib 2012 \u{b7} downloads 1.8 MB \u{b7} Subuh: its own adzan, Besim Azemi (2.4 MB)");
        assert_eq!(sound_line(PrayerSound::Makkah, Some(Status::Ready)), "Masjid al-Haram, Maghrib 2012 \u{b7} Subuh: its own adzan, Besim Azemi (2.4 MB)");
        assert!(sound_line(PrayerSound::Makkah, Some(Status::Unavailable(404))).contains("not published yet, the tone plays"));
        assert!(sound_line(PrayerSound::Adzan, None).starts_with("Aaqib Azeez \u{b7} Subuh"));
        assert_eq!(sound_line(PrayerSound::Tone, None), "A short tone at each prayer.");
        assert_eq!(SOUNDS.iter().map(|s| s.1).collect::<Vec<_>>(), ["Adzan", "Makkah", "Tone", "Silent"]);
    }

    #[test]
    fn prayer_times_files_are_the_ones_in_the_repo() {
        // The pinned fingerprints must match addons/sholat/, which is what the release holds.
        let repo: [(&str, &[u8]); 3] = [
            ("places.json", include_bytes!("../../../addons/sholat/places.json")),
            ("adzan.m4a", include_bytes!("../../../addons/sholat/adzan.m4a")),
            // As the tag holds it: addons/sholat/NOTICE.txt went on to credit the voices.
            ("NOTICE.txt", include_bytes!("../../../addons/sholat/sholat-3/NOTICE.txt")),
        ];
        assert_eq!(PRAYER_TIMES.files.len(), repo.len());
        for (f, (name, bytes)) in PRAYER_TIMES.files.iter().zip(repo) {
            assert_eq!(f.name, name);
            assert!(f.matches(bytes), "{name}: size {} sha256 {}", bytes.len(), sha256::hex(&sha256::sha256(bytes)));
        }
        let mut broken = repo[2].1.to_vec();
        broken[0] ^= 1;
        assert!(!PRAYER_TIMES.files[2].matches(&broken));
        assert!(!PRAYER_TIMES.files[2].matches(&repo[2].1[1..]));
    }

    #[test]
    fn japanese_readings_files_are_the_ones_in_the_repo() {
        // The dictionary make_ja_dict.py builds, and the notice that travels with it.
        let repo: [(&str, &[u8]); 2] = [
            (JA_READINGS_FILE, include_bytes!("../data/ja-readings.dic.zst")),
            ("NOTICE.txt", include_bytes!("../../../addons/ja/NOTICE.txt")),
        ];
        assert_eq!(JAPANESE_READINGS.files.len(), repo.len());
        for (f, (name, bytes)) in JAPANESE_READINGS.files.iter().zip(repo) {
            assert_eq!(f.name, name);
            assert!(f.matches(bytes), "{name}: size {} sha256 {}", bytes.len(), sha256::hex(&sha256::sha256(bytes)));
        }
        let ja = Addon::find("ja").unwrap();
        assert_eq!(ja.url(&ja.files[0]), "https://github.com/kolijijani/halo-island/releases/download/addon-ja-1/ja-readings.dic.zst");
        assert_eq!(Status::Off.line(ja, false), ja.blurb);
        assert_eq!(Status::Ready.line(ja, false), "On · 3.8 MB on this PC");
        // The Mac app has no Windows reason to give, and says Mac.
        assert_eq!(Status::Off.line(ja, true), "Romaji under kanji in Japanese lyrics.");
        assert_eq!(Status::Checking.line(ja, true), "Checking its files on this Mac");
        assert_eq!(Status::Ready.line(ja, true), "On · 3.8 MB on this Mac");
    }

    #[test]
    fn the_music_page_offers_japanese_readings_until_they_are_here() {
        assert_eq!(ja_offer(Status::Off), Some(("Romaji for kanji: get Japanese readings (3.8 MB)".to_string(), true)));
        let (line, click) = ja_offer(Status::Downloading { done: 1_700_000, total: JAPANESE_READINGS.size() }).unwrap();
        assert_eq!(line, "Getting Japanese readings · 1.7 MB of 3.8 MB");
        assert!(!click);
        for s in [Status::Checking, Status::Waiting(crate::net::Failure::Offline), Status::Waiting(crate::net::Failure::Timeout), Status::Failed, Status::Unavailable(404)] {
            assert!(ja_offer(s).is_some_and(|(_, click)| !click), "{s:?} says where it stands, nothing to click");
        }
        assert_eq!(ja_offer(Status::Ready), None);
    }

    /// Prayer times, Japanese readings and the Downloader's files: success,
    /// timeout, offline and malformed answers, and the mirror for a release
    /// asset that isn't published.
    #[test]
    fn a_download_is_judged_honestly() {
        use crate::net::Failure;
        let notice: &[u8] = include_bytes!("../../../addons/sholat/sholat-3/NOTICE.txt");
        let f = PRAYER_TIMES.files.iter().find(|f| f.name == "NOTICE.txt").unwrap();
        assert_eq!(verdict(f, true, Ok((200, notice))), Verdict::Good);
        // Timeout and offline wait and try again, saying which.
        assert_eq!(verdict(f, true, Err(Failure::Timeout)), Verdict::Retry(Failure::Timeout));
        assert_eq!(verdict(f, true, Err(Failure::Offline)), Verdict::Retry(Failure::Offline));
        assert_eq!(verdict(f, false, Ok((503, &[]))), Verdict::Retry(Failure::Status(503)));
        // Malformed: an error page or a file cut short is never kept.
        assert_eq!(verdict(f, true, Ok((200, b"<html>Not here</html>"))), Verdict::Wrong);
        assert_eq!(verdict(f, true, Ok((200, &notice[..notice.len() - 1]))), Verdict::Wrong);
        // Not on the release: the mirror if there is one, else say so.
        assert_eq!(verdict(f, true, Ok((404, b"Not Found"))), Verdict::TryMirror);
        assert_eq!(verdict(f, false, Ok((404, b"Not Found"))), Verdict::Unavailable(404));
        assert_eq!(verdict(f, false, Ok((401, &[]))), Verdict::Unavailable(401));
        // What the add-ons page says for each.
        assert!(Status::Waiting(Failure::Offline).line(&PRAYER_TIMES, false).starts_with("No internet"));
        assert_eq!(Status::Waiting(Failure::Timeout).line(&PRAYER_TIMES, false), "Can't download it right now (timed out). It tries again by itself.");
        assert!(Status::Unavailable(404).line(&PRAYER_TIMES, false).contains("aren't published yet"), "a 404 isn't a network failure");
        assert!(Status::Unavailable(410).line(&PRAYER_TIMES, false).contains("error 410"));
    }

    /// Every file of Prayer times and Japanese readings has a mirror at a
    /// public commit, and the repository's copy is the pinned file.
    #[test]
    fn the_mirror_holds_the_pinned_files() {
        let commit = MIRROR.strip_prefix("https://raw.githubusercontent.com/kolijijani/halo-island/").unwrap();
        assert!(commit.len() == 40 && commit.bytes().all(|b| b.is_ascii_hexdigit()), "pinned to a full commit, not a branch");
        let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for a in [&PRAYER_TIMES, &JAPANESE_READINGS] {
            for f in a.files {
                let url = a.mirror_url(f).unwrap_or_else(|| panic!("{} {}: no mirror", a.id, f.name));
                let path = url.strip_prefix(&format!("{MIRROR}/")).unwrap();
                // The mirror's commit holds sholat-3's notice; the repository's
                // went on to credit the voices, and keeps that copy aside.
                let local = if path == "addons/sholat/NOTICE.txt" { "addons/sholat/sholat-3/NOTICE.txt" } else { path };
                let bytes = std::fs::read(here.join(local)).unwrap();
                assert!(f.matches(&bytes), "{path} isn't the pinned {}", f.name);
            }
        }
        // The Downloader's own ffmpeg isn't in the repository: no mirror.
        let ff = DOWNLOADER_WINDOWS.files.iter().find(|f| f.name == "ffmpeg.exe").unwrap();
        assert_eq!(DOWNLOADER_WINDOWS.mirror_url(ff), None);
    }

    #[test]
    fn files_come_from_the_release() {
        let a = Addon::find("sholat").unwrap();
        assert_eq!(a.url(&a.files[1]), "https://github.com/kolijijani/halo-island/releases/download/addon-sholat-3/adzan.m4a");
        assert_eq!(a.size(), 959_349);
        assert!(Addon::find("nope").is_none());
    }

    #[test]
    fn lines_say_what_is_happening() {
        let a = &PRAYER_TIMES;
        assert_eq!(Status::Off.line(a, false), a.blurb);
        assert_eq!(Status::Off.line(a, true), a.blurb, "no Mac line of its own: the same");
        assert_eq!(Status::Downloading { done: 500_000, total: a.size() }.line(a, false), "Downloading · 500 KB of 959 KB");
        assert_eq!(Status::Ready.line(a, false), "On · 959 KB on this PC");
        assert!(!Status::Off.is_on() && Status::Waiting(crate::net::Failure::Offline).is_on() && Status::Failed.is_on());
        assert_eq!(Status::Downloading { done: 1, total: 4 }.progress(), Some(0.25));
        assert_eq!(Status::Downloading { done: 0, total: 0 }.progress(), Some(0.0));
        assert_eq!(Status::Ready.progress(), None);
    }

    #[test]
    fn add_ons_are_listed_in_order_each_with_its_own_id() {
        // The Add-ons page's order: the free ones, then Studio Pro's.
        let ids: Vec<&str> = ALL.iter().map(|a| a.id).collect();
        assert_eq!(ids, ["sholat", "ja", "script", "system", "stocks", "caffeine", "recorder", "ai", "downloader"]);
        // Health left the app.
        assert_eq!(Addon::find("health"), None);
        for (i, a) in ALL.iter().enumerate() {
            // The key in settings.json and the folder's name.
            assert!(!a.id.is_empty() && a.id.bytes().all(|b| b.is_ascii_lowercase()), "{}", a.id);
            assert!(ALL[..i].iter().all(|b| b.id != a.id && b.title != a.title), "{} twice", a.id);
            assert_eq!(Addon::find(a.id), Some(*a));
            assert_eq!(a.downloads(), !a.tag.is_empty(), "{}: a release exactly when there are files", a.id);
            // Each file a name of its own in the folder, and none of the
            // downloader's own there (its `.ok` stamp, a `.part` on its way).
            for (j, f) in a.files.iter().enumerate() {
                assert!(a.files[..j].iter().all(|g| g.name != f.name), "{}: {} twice", a.id, f.name);
                assert!(!f.name.starts_with('.') && !f.name.ends_with(".part") && !f.name.contains(['/', '\\']), "{}: {}", a.id, f.name);
            }
        }
    }

    #[test]
    fn the_downloader_takes_its_tools_from_their_own_releases() {
        // Each app has its own tools, under the names it runs them by.
        assert_eq!(Addon::find("downloader"), Some(&DOWNLOADER));
        let names = |d: &Addon| d.files.iter().map(|f| f.name).collect::<Vec<_>>();
        assert_eq!(names(&DOWNLOADER), [YTDLP_EXE, QJS_EXE, FFMPEG_EXE, "NOTICE.txt"]);
        assert_eq!(names(&DOWNLOADER_WINDOWS), ["yt-dlp.exe", "qjs.exe", "ffmpeg.exe", "NOTICE.txt"]);
        assert_eq!(names(&DOWNLOADER_MAC), ["yt-dlp", "qjs", "ffmpeg", "NOTICE.txt"]);
        assert!(names(&DOWNLOADER)[..3].iter().all(|n| is_downloader_tool(n)) && !is_downloader_tool("NOTICE.txt"));
        // One add-on in both apps (its switch, its row), each with its own
        // pre-release, so each keeps its own folder.
        assert_eq!(DOWNLOADER_MAC, Addon { tag: DOWNLOADER_MAC.tag, files: DOWNLOADER_MAC.files, ..DOWNLOADER_WINDOWS });
        assert_ne!(DOWNLOADER_MAC.tag, DOWNLOADER_WINDOWS.tag);
        // yt-dlp, and QuickJS on Windows: their projects' own releases,
        // unchanged, each from a fixed tag (a `latest` link would change
        // under the pin); yt-dlp the same version on both.
        let w = &DOWNLOADER_WINDOWS;
        let m = &DOWNLOADER_MAC;
        let mut ytdlp_tags = Vec::new();
        for (d, f, project, asset) in
            [(w, &w.files[0], "yt-dlp/yt-dlp", "yt-dlp.exe"), (w, &w.files[1], "quickjs-ng/quickjs", "qjs-windows-x86_64.exe"), (m, &m.files[0], "yt-dlp/yt-dlp", "yt-dlp_macos")]
        {
            let url = d.url(f);
            let tag = url.strip_prefix(&format!("https://github.com/{project}/releases/download/")).and_then(|rest| rest.strip_suffix(&format!("/{asset}")));
            assert!(tag.is_some_and(|t| !t.is_empty() && !t.contains('/')), "{url}");
            assert!(f.pinned(), "{}: a tool from elsewhere is always pinned", f.name);
            if project == "yt-dlp/yt-dlp" {
                ytdlp_tags.extend(tag.map(str::to_string));
            }
        }
        assert_eq!(ytdlp_tags[0], ytdlp_tags[1], "yt-dlp: the same version on Windows and the Mac");
        // Our own builds and the notices: each add-on's own pre-release.
        let ours = "https://github.com/kolijijani/halo-island/releases/download";
        assert_eq!(w.url(&w.files[2]), format!("{ours}/addon-downloader-1/ffmpeg.exe"));
        assert_eq!(w.url(&w.files[3]), format!("{ours}/addon-downloader-1/NOTICE.txt"));
        for (f, name) in m.files[1..].iter().zip(["qjs", "ffmpeg", "NOTICE.txt"]) {
            assert_eq!(m.url(f), format!("{ours}/addon-downloader-mac-1/{name}"));
        }
    }

    #[test]
    fn an_add_on_with_a_placeholder_file_stays_off() {
        const SOON: Addon = Addon {
            id: "soon",
            title: "Soon",
            blurb: "Not out yet.",
            mac_blurb: None,
            tag: "addon-soon-1",
            files: &[
                AddonFile { name: "tool.exe", size: 2_151_363, sha256: "2aeabf0092c3262d6b2609824418f7dd7ed1f1df939f73b2b15645230cac0d77", url: None },
                AddonFile { name: "NOTICE.txt", size: 0, sha256: "", url: None },
            ],
            pro: false,
        };
        let tool = SOON.files[0];
        assert!(tool.pinned() && !SOON.files[1].pinned() && !SOON.pinned());
        // Half a pin is none.
        for f in [
            AddonFile { sha256: "", ..tool },
            AddonFile { size: 0, ..tool },
            AddonFile { sha256: &tool.sha256[1..], ..tool },
            AddonFile { sha256: "2AEABF0092C3262D6B2609824418F7DD7ED1F1DF939F73B2B15645230CAC0D77", ..tool },
        ] {
            assert!(!f.pinned(), "{f:?}");
        }
        // Everything in the app has its files pinned (built-ins have none to
        // pin), in both apps.
        for a in ALL.iter().copied().chain([&DOWNLOADER_WINDOWS, &DOWNLOADER_MAC]) {
            assert!(a.pinned(), "{} ({})", a.id, a.tag);
        }
        // The switch stays off and the line says why; nothing comes in.
        let s = Status::Unpinned;
        assert!(!s.is_on() && s.progress().is_none());
        assert_eq!(s.line(&SOON, false), "This version can't get its files yet, so it stays off.");
        assert_eq!(s.line(&SOON, true), s.line(&SOON, false));
        assert!(ja_offer(s).is_some_and(|(_, click)| !click));
    }

    #[test]
    fn the_downloaders_notices_are_the_ones_in_the_repo() {
        let windows: &[u8] = include_bytes!("../../../addons/downloader/NOTICE.txt");
        let mac: &[u8] = include_bytes!("../../../addons/downloader-mac/NOTICE.txt");
        for (d, bytes) in [(DOWNLOADER_WINDOWS, windows), (DOWNLOADER_MAC, mac)] {
            let notice = d.files.iter().find(|f| f.name == "NOTICE.txt").unwrap();
            assert!(notice.matches(bytes), "{}: size {} sha256 {}", d.tag, bytes.len(), sha256::hex(&sha256::sha256(bytes)));
            assert_eq!(d.url(notice), format!("https://github.com/kolijijani/halo-island/releases/download/{}/NOTICE.txt", d.tag));
        }
    }

    #[test]
    fn a_file_on_disk_is_checked_a_piece_at_a_time() {
        // The dictionary (3.8 MB) in 64 KB reads, against the same pin.
        let dict: &[u8] = include_bytes!("../data/ja-readings.dic.zst");
        let f = &JAPANESE_READINGS.files[0];
        assert!(f.matches_read(dict));
        let mut changed = dict.to_vec();
        *changed.last_mut().unwrap() ^= 1;
        assert!(!f.matches_read(&changed[..]));
        assert!(!f.matches_read(&dict[..dict.len() - 1]), "a byte short");
        assert!(!f.matches_read(std::io::Read::chain(dict, &b"!"[..])), "a byte more");
        // A file that can't be read to its end isn't the file.
        struct Unplugged;
        impl std::io::Read for Unplugged {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("unplugged"))
            }
        }
        assert!(!f.matches_read(Unplugged));
    }

    #[test]
    fn a_big_add_on_reads_well() {
        // About the Downloader's size with FFmpeg: whole megabytes from 10 MB up.
        const BIG: Addon = Addon {
            id: "big",
            title: "Big",
            blurb: "Big tools.",
            mac_blurb: None,
            tag: "addon-big-1",
            files: &[AddonFile { name: "big.exe", size: 30_100_000, sha256: "f5389d3698ecde4ef06f4e6b1ce0294fe9ac1396228c8990b8d3416d6d177a7d", url: None }],
            pro: false,
        };
        assert_eq!(fmt_bytes(BIG.size()), "30 MB", "its size on the page while it's off");
        assert_eq!(Status::Downloading { done: 12_345_678, total: BIG.size() }.line(&BIG, false), "Downloading · 12 MB of 30 MB");
        assert_eq!(Status::Ready.line(&BIG, false), "On · 30 MB on this PC");
    }
}
