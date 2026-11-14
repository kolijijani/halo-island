//! Messages from service threads to the UI (main) thread.
//!
//! Services hold an [`EventSink`]. `send` pushes onto an mpsc channel and
//! queues a drain on the main dispatch queue; the main thread then applies
//! every waiting event. The event types match the Windows app's, so the
//! services that arrive in later slices plug in the same way.

use crate::gfx::Image;
use crate::state::{MediaSession, PowerState};
use halo_core::lrc::Lyrics;
use halo_core::halo_account::{Activity, Arrival, Bill};
use halo_core::romanize::Ruby;
use halo_core::timeline::Timeline;
use std::sync::mpsc::Sender;

#[derive(Clone)]
pub struct EventSink {
    tx: Sender<AppEvent>,
}

impl EventSink {
    pub fn new(tx: Sender<AppEvent>) -> Self {
        Self { tx }
    }

    pub fn send(&self, ev: AppEvent) {
        if self.tx.send(ev).is_ok() {
            dispatch2::DispatchQueue::main().exec_async(|| {
                crate::app::drain_events();
            });
        }
    }
}

/// What Apple Music said about the user's token.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppleMusicStatus {
    Accepted,
    Rejected,
    Unreachable,
    NoAccess,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum LyricsResult {
    /// `ruby`: each line's pieces with their readings (empty = none needed).
    /// `wants_ja_readings`: kanji went unread for want of the Japanese
    /// readings add-on (the Music page offers it).
    Found { lyrics: Lyrics, source: String, ruby: Vec<Vec<Ruby>>, wants_ja_readings: bool },
    Instrumental,
    NotFound,
    Error(String),
}

/// What the Halo worker reports (`services::halo`, shared with Windows).
#[derive(Clone, Debug)]
pub enum HaloEvent {
    /// The sign-in saved on this Mac was found at start (no notice).
    Restored { account: String },
    /// A sign-in code to show, the halo.app address that takes it, and how long it lasts.
    Connecting { code: String, url: String, expires_in_s: u64 },
    Connected { account: String },
    /// Signing in stopped without a key: `expired`, `denied`, `offline` or `other`.
    ConnectFailed(String),
    /// Money that just arrived or went out by itself (oldest first).
    Money { arrived: Vec<Arrival> },
    /// The last 7 days of transactions, for the Halo page (while it's open).
    Activity(Activity),
    /// Asked again while the page is open: nothing changed.
    ActivityChecked,
    /// Bills due in the next 30 days, and the server's today (YYYY-MM-DD).
    Bills { today: String, bills: Vec<Bill> },
    /// Whether the account has Studio Pro (a server from before Pro: no).
    Pro(bool),
    /// Signed out by the server (disconnected in halo.app), or by the self-test.
    Disconnected { by_server: bool },
}

#[allow(dead_code)]
pub enum AppEvent {
    /// The current media session changed (None = nothing playing / no session).
    Media(Option<MediaSession>),
    /// Only the timeline/playback state of the current track changed.
    MediaTimeline { track_key: String, timeline: Timeline },
    /// Album art finished decoding for a track.
    MediaArt { track_key: String, art: Option<Image> },
    Lyrics { track_key: String, result: LyricsResult },
    AppleMusic(AppleMusicStatus),
    Halo(HaloEvent),
    /// Master volume changed (0..=1). `switched`: the output device changed
    /// (headphones connected, say), not its volume.
    Volume { level: f32, muted: bool, switched: bool },
    Power(PowerState),
    Bluetooth { name: String, connected: bool, battery: Option<u8>, audio: bool },
    /// A full-screen app took over (or left) the island's screen.
    Fullscreen(bool),
    /// Files dropped on the island.
    ShelfAdd(Vec<String>),
    /// Files are being dragged over the island (true) or left (false).
    DragHover(bool),
    ShelfThumb { path: String, image: Option<Image> },
    /// The System page's latest reading and the minute behind it
    /// (`services::sysmon`, each second while the page is open).
    System(Box<halo_core::sysmon::Monitor>),
    /// An app's icon for the System page, by its path (`Command::LoadAppIcon`).
    AppIcon { path: String, image: Option<Image> },
    /// The displays went to sleep (true) or woke.
    ScreensAsleep(bool),
    /// The Mac was locked (true) or unlocked.
    Locked(bool),
    /// Something happened in Phone drop (a phone visited, asked, sent files).
    Phone(halo_core::phonedrop::Event),
    /// Where an add-on's files stand (`services::addons`).
    Addon { id: &'static str, status: halo_core::addons::Status },
    /// One of yt-dlp's lines for download `id` (`services::downloader`).
    Download { id: u64, line: halo_core::ytdlp::Line },
    /// yt-dlp for download `id` ended (`ok`: it exited with 0).
    DownloadEnded { id: u64, ok: bool },
    /// What the update file said (`services::update`).
    Update(crate::services::update::UpdateEvent),
    /// Where the Mac is, for Prayer times (`services::location`).
    Located(crate::services::location::Located),
    /// One of Prayer times' online city files (`services::cities`).
    Cities { file: String, got: Result<halo_core::prayer::World, halo_core::net::Failure> },
    /// A Halo Pro code redeemed, or why not (`services::license`).
    License(crate::services::license::Redeemed),
    /// The Today page's news (`services::news`).
    News(Result<halo_core::news::News, halo_core::net::Failure>),
    /// The Stocks page's prices, names and years (`services::stocks`).
    Stocks(crate::services::stocks::StocksEvent),
    /// The AI tab's readings, typed keys and heads-up notices (`services::usage`).
    Usage(crate::services::usage::UsageEvent),
    /// Prayer times' adzan played to its end (`services::adzan`).
    AdzanEnded,
    /// Free-form log line from a service thread.
    Log(String),
}
