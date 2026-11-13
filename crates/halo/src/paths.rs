//! Where Halo keeps its files.
//!
//! - Settings + data: `%APPDATA%\Halo\` (roams with the user profile)
//! - Cache + log:     `%LOCALAPPDATA%\Halo\`
//!
//! `HALO_HOME` overrides both (used by `--selftest` to stay isolated).

use std::path::PathBuf;

fn home_override() -> Option<PathBuf> {
    std::env::var_os("HALO_HOME").map(PathBuf::from)
}

fn env_dir(var: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn data_dir() -> PathBuf {
    home_override().unwrap_or_else(|| env_dir("APPDATA").join("Halo"))
}

pub fn cache_dir() -> PathBuf {
    home_override().map(|h| h.join("cache")).unwrap_or_else(|| env_dir("LOCALAPPDATA").join("Halo"))
}

pub fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

pub fn store_path() -> PathBuf {
    data_dir().join("data.json")
}

pub fn log_path() -> PathBuf {
    cache_dir().join("halo.log")
}

pub fn lyrics_cache_dir() -> PathBuf {
    cache_dir().join("lyrics")
}

/// The user's Apple Music token, sealed with DPAPI (never roams; uninstalling
/// removes it with the cache).
pub fn apple_music_token_path() -> PathBuf {
    cache_dir().join("apple-music-token.bin")
}

/// The Apple Music web player's (public) developer token, cached until it expires.
pub fn apple_music_web_token_path() -> PathBuf {
    cache_dir().join("apple-music-web.json")
}

/// Where files sent from a phone land: `Downloads\Halo Island` (wherever the
/// user moved Downloads to). Under `HALO_HOME` for the self-test.
pub fn phone_inbox_dir() -> PathBuf {
    if let Some(h) = home_override() {
        return h.join("phone-drop");
    }
    downloads_dir().unwrap_or_else(|| env_dir("USERPROFILE").join("Downloads")).join("Halo Island")
}

/// Where the Downloader saves videos and sound: the user's Downloads folder
/// (wherever they moved it), as a browser would. Under `HALO_HOME` for the
/// self-test.
pub fn media_downloads_dir() -> PathBuf {
    if let Some(h) = home_override() {
        return h.join("downloads");
    }
    downloads_dir().unwrap_or_else(|| env_dir("USERPROFILE").join("Downloads"))
}

fn downloads_dir() -> Option<PathBuf> {
    known_folder(&windows::Win32::UI::Shell::FOLDERID_Downloads)
}

fn known_folder(id: &windows::core::GUID) -> Option<PathBuf> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, HANDLE::default()).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.filter(|s| !s.is_empty()).map(PathBuf::from)
    }
}

/// The Halo sign-in (this PC's key and the masked account), sealed with
/// DPAPI. Signing out or uninstalling removes it.
pub fn halo_link_path() -> PathBuf {
    cache_dir().join("halo-link.bin")
}

/// Add-on files, one folder per add-on (`services::addons`). Local, never
/// roaming: a PC that hasn't them downloads them again. Switching an add-on
/// off or uninstalling removes them.
pub fn addons_dir() -> PathBuf {
    cache_dir().join("addons")
}

/// Paired phones (their pairing secrets and names), sealed with DPAPI.
/// Removing every phone deletes the file.
pub fn paired_phones_path() -> PathBuf {
    cache_dir().join("paired-phones.bin")
}

/// A key signed out while Halo couldn't be reached, sealed, kept only to
/// tell the server once it can.
pub fn halo_revoke_path() -> PathBuf {
    cache_dir().join("halo-revoke.bin")
}

/// A key typed into the AI tab for one service (`usage-<name>.bin`), sealed with DPAPI: never roams, and removing the
/// key or uninstalling deletes it.
pub fn usage_key_path(name: &str) -> PathBuf {
    cache_dir().join(format!("usage-{name}.bin"))
}
