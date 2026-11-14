//! Where Halo keeps its files on a Mac.
//!
//! - Settings + data: `~/Library/Application Support/Halo/`
//! - Cache:           `~/Library/Caches/Halo/`
//! - Log:             `~/Library/Logs/Halo/halo.log` (Console.app lists it)
//!
//! `HALO_HOME` overrides all three (used by `--render` and `--smoke` to stay
//! isolated).

use std::path::PathBuf;

fn home_override() -> Option<PathBuf> {
    std::env::var_os("HALO_HOME").map(PathBuf::from)
}

fn library() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join("Library")
}

pub fn data_dir() -> PathBuf {
    home_override().unwrap_or_else(|| library().join("Application Support").join("Halo"))
}

pub fn cache_dir() -> PathBuf {
    home_override().map(|h| h.join("cache")).unwrap_or_else(|| library().join("Caches").join("Halo"))
}

pub fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

pub fn store_path() -> PathBuf {
    data_dir().join("data.json")
}

pub fn log_path() -> PathBuf {
    home_override().map(|h| h.join("halo.log")).unwrap_or_else(|| library().join("Logs").join("Halo").join("halo.log"))
}

/// Held while Halo runs, so a second copy quits at start.
pub fn lock_path() -> PathBuf {
    cache_dir().join("halo.lock")
}

/// Where files from a phone land (Phone drop): `~/Downloads/Halo Island`.
pub fn phone_inbox_dir() -> PathBuf {
    if let Some(h) = home_override() {
        return h.join("phone-drop");
    }
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join("Downloads").join("Halo Island")
}

/// Where the Downloader saves videos and sound: `~/Downloads`, as a browser
/// would. An app may create files there without asking (macOS asks before
/// it reads one it didn't make). Under `HALO_HOME` for the smoke run.
pub fn media_downloads_dir() -> PathBuf {
    if let Some(h) = home_override() {
        return h.join("downloads");
    }
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join("Downloads")
}

pub fn lyrics_cache_dir() -> PathBuf {
    cache_dir().join("lyrics")
}

/// Add-on files, one folder per add-on (`services::addons`), only there
/// while the add-on is switched on. A Mac that clears its caches downloads
/// them again.
pub fn addons_dir() -> PathBuf {
    cache_dir().join("addons")
}

/// The Halo sign-in (this Mac's key and the masked account), in a file
/// only this Mac account can read (`services::secret`). Signing out removes
/// it. With the settings, not the cache: macOS may empty caches by itself.
pub fn halo_link_path() -> PathBuf {
    data_dir().join("halo-link.bin")
}

/// A key signed out while Halo couldn't be reached, kept only to tell the
/// server once it can.
pub fn halo_revoke_path() -> PathBuf {
    data_dir().join("halo-revoke.bin")
}

/// The Halo Pro license token (owner-only file, `services::license`).
pub fn license_path() -> PathBuf {
    data_dir().join("license.bin")
}

/// This install's own random id for licenses (owner-only file).
pub fn license_device_path() -> PathBuf {
    data_dir().join("license-device.bin")
}

/// A key typed into the AI tab for one service (`usage-<name>.bin`), in a file only this Mac account can read
/// (`services::secret`). With the settings, not the cache: macOS may empty
/// caches by itself.
pub fn usage_key_path(name: &str) -> PathBuf {
    data_dir().join(format!("usage-{name}.bin"))
}

/// The person's Apple Music token (owner-only file; uninstalling with the
/// cache folder removes it).
pub fn apple_music_token_path() -> PathBuf {
    cache_dir().join("apple-music-token.bin")
}

/// The Apple Music web player's (public) developer token, cached until it expires.
pub fn apple_music_web_token_path() -> PathBuf {
    cache_dir().join("apple-music-web.json")
}
