# A2Z-01 — Bedah (dissection) · Halo Island / base app upstream 0.2.30

Sumber: `AGENTS.md`, `ARCHITECTURE.md`, `README.md`, `Cargo.toml`, `crates/*/Cargo.toml`, dan bacaan terus kod (`crates/poni/src`). Commit asas: `31078f5 Release 0.2.30`.

## 1. Peta workspace

| Laluan | Apa | Saiz (baris .rs) | OS |
|---|---|---|---|
| `crates/poni` | Bin `poni.exe`: app Windows (Win32 + Direct2D + DirectComposition + WinRT, `windows` =0.58.0) dan app Mac (`src/mac/`, objc2) | 85,630 (18,365 daripadanya `src/mac/`) | Windows / macOS sahaja. Di Linux, `main()` cuma cetak mesej (lihat `main.rs`, hujung fail) |
| `crates/poni-core` | Logik tulen: geometri/warna, spring, LRC/LRCLIB, settings + JSON store, Pomodoro, tasks, shelf, add-ons (saiz + SHA-256), prayer times, phone drop (server LAN + WebRTC `str0m`), update feed rules, sysmon arithmetic, Oyen face | 34,110 | Mana-mana OS |
| `crates/studio-usage` | Pembaca tab AI (Codex, Claude Code, Cursor, Copilot…), dibina `opt-level="s"` | 10,362 | Mana-mana OS |
| `crates/studio-diorama` | Gambar tema Diorama (pixel-art), tiada dependency | 1,946 | Mana-mana OS |
| `addons/` | Manifest add-on (`downloader`, `downloader-mac`, `ja`, `sholat`); fail besar duduk di GitHub pre-release, bukan dalam exe | — | — |
| `server/` | Meeting server + halaman telefon untuk paired phone drop (server drop upstream), Node 20, tiada dependency | — | Node |
| `site/` | Landing page `studio.halo.island` (`index.html`, `creators.json`, `places/`) | — | Web |
| `installer/` | `poni.nsi` (NSIS, installer utama CI), `poni.iss` (Inno Setup), `msix/` (Microsoft Store), `mac/` (Info.plist, entitlements) | — | — |
| `tools/` | Skrip build/CI: `package.sh`, `ci-*.ps1` (smoke, installer, weight), `site_shots.py`, `make_*` (brand, notices, update feed, DMG, MSIX) | — | — |
| `third_party/` | `mediaremote-adapter` (BSD-3) untuk Now Playing di Mac | — | macOS |

Dependency antara crate:

```
poni (bin) ──► poni-core
     ├──────► studio-usage ──► poni-core
     └──────► studio-diorama   (tiada dependency)
```

`poni-core` tarik `str0m` (WebRTC; Windows guna CNG `wincrypto`, lain guna RustCrypto), `vibrato` + `ruzstd` (kamus Jepun), `qrcodegen`, `sha2`, `hmac`. `poni` tarik `windows` + `windows-core` (Windows), `objc2*` + `p256` (Mac), `embed-resource` (build).

## 2. Entry point dan kitaran app

- `crates/poni/src/main.rs`: deklarasi modul ikut `cfg` (Windows vs Mac via `#[path = "mac/…"]`), `main()` panggil `app::main_entry()`.
- `app.rs::main_entry` (Windows): parse argumen (`--selftest`, `--allow-fps` …), mutex tunggal `Local\Poni.Island`, `pending_install()` (Setup kemas kini yang menunggu), kemudian `run()`.
- Satu UI thread memegang `App` (window, `AppState`, renderer, services). Services lapor melalui `EventSink` (mpsc + `PostMessage WM_APP_EVENT`). Pages minta kesan sampingan melalui `Command`.

## 3. Island / notch window

- `window.rs::create_island_window`: `WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`, kelas `Poni.Island`. Tetingkap bersaiz tetap untuk island terbesar; island dilukis di tengah, jadi morph tak pernah resize window.
- Window kedua `Poni.Visual` (DirectComposition) untuk GPU renderer; window island cuma bawa hit mask (alpha 1/255).
- `window.rs` juga ada frame clock: thread tunggu vblank (`IDXGIOutput::WaitForVBlank`) dan post `WM_APP_FRAME` **hanya semasa animasi**. Rehat = tiada frame.
- `gfx.rs`: Direct2D atas D3D11 (fallback WARP, fallback software DIB + `UpdateLayeredWindowIndirect`, atau `PONI_RENDERER=software`).
- `island.rs`: geometri + spring. `activities.rs`: kandungan pill tertutup (media, timer, HUD, notis). `pages/*.rs`: halaman terbuka (immediate-mode `ui.rs`).
- Helper window tersembunyi `Poni.Helper`: tray, AppBar (fullscreen), `WM_POWERBROADCAST`.

## 4. Tema (light/dark + tema)

- Tema island: `poni_core::settings::Theme` = `Classic` (default), `Oyen`, `Glass`, `Pill`, `Diorama`. Gaya rehat `IdleStyle` = `Clock` (default), `Minimal`, `Eyes`, `Hidden`, `Status`.
- Island sentiasa hitam (`geom::palette`). Light/dark sistem dibaca oleh `services::apps_use_light_theme()` (WinRT `UISettings`), dipakai oleh tema Glass dan tetingkap Settings (`settings_host.rs`, dibaca semula pada `WM_SETTINGCHANGE`; tiada polling).
- Aksen = warna aksen Windows atau warna album art (`st.media_accent()`).

## 5. System tray

- `tray.rs`: `Shell_NotifyIconW` dengan tooltip `"<nama app> <versi>"`; menu: Advanced settings, Keep island open, Theme, Idle look, lyrics, Apple Music, upstream, Sounds, full-screen, Start with Windows, fail settings, About, Quit. Menu sama muncul pada klik kanan island.
- Dibina semula selepas `TaskbarCreated` (Explorer restart).

## 6. Settings

- Fail: `%APPDATA%\Poni\settings.json` (`poni_core::settings::Settings`, `serde(default)`, migrasi `settings_version`). Data: `data.json`. Cache/log: `%LOCALAPPDATA%\Poni\`. `PONI_HOME` override.
- Halaman Settings (gear di header, `pages/settings.rs`) untuk tetapan harian; Advanced settings = tetingkap sendiri (`settings_window.rs` kandungan dikongsi Win/Mac, `settings_host.rs` hos Windows), dilukis hanya bila input/kandungan berubah.
- `Settings::hidden_tabs` boleh sorok Music, Today, Shelf, Tasks, Focus, Notifications.

## 7. Media / SMTC

- `services/media.rs`: GSMTC (`Windows.Media.Control`), event-driven. `services/applemusic_ui.rs`: butang Apple Music via UI Automation. `services/lyrics.rs`: LRCLIB (+ Apple Music opt-in). `services/audio.rs`: notifikasi volume (Core Audio). `pages/media.rs` + `pill/media.rs`, `pill/lyric.rs`: paparan.

## 8. Notification listener

- `services/notifications.rs`: `UserNotificationListener` + event `NotificationChanged` (tiada polling).
- **Kekangan Windows**: listener perlukan package identity, jadi hanya salinan MSIX/Store yang mendengar (`AppState::notify_can = autostart::packaged()`, `app.rs:646`). Salinan installer biasa: tab Notifications tak muncul. `pages/notifications.rs` papar status akses + suis per-app.

## 9. Jam / kalendar

- Jam rehat: `IdleStyle::Clock` (default) dalam `activities.rs`, bangun dengan timer sekali-tembak setiap minit (tiada frame clock).
- Today (`pages/today.rs`): tarikh + 5 minggu grid tempatan. Acara kalendar sebenar (Google Calendar / Gmail flights) datang dari akaun upstream (`GET /island/day`), hanya semasa halaman terbuka + setiap 5 min. Tanpa akaun: grid tarikh + kad "Sign in with upstream".

## 10. Tab AI

- `pages/usage.rs` + `services/usage.rs` (worker satu thread, condvar, tidur bila tiada siapa perlu) + crate `studio-usage`. Hanya tampil untuk akaun upstream dengan Studio Pro (`AppState::ai_usage_on`). Tiada thread sehingga tab dibuka pertama kali.

## 11. Add-ons download

- `poni_core::addons` (senarai, saiz, SHA-256, `RELEASES` = GitHub pre-release upstream) + `services/addons.rs` (thread yang tidur; muat turun bila dihidupkan, padam bila dimatikan). Add-on: Prayer times, Japanese readings, Script reader, System, Health, Stocks, Caffeine, Screen recorder, AI usage, Meeting notes, IELTS, Downloader. Semua OFF secara default (`Settings::addon_on` = false jika tiada).

## 12. Update feed

- `poni_core::update::FEED_URL = feed upstream (`latest.json`)`, ditandatangani (ECDSA P-256, `trusted_key`). `updates.rs` (sisi app) baca selepas start dan setiap jam (rehat bila locked/display off/battery saver), muat turun Setup, pasang dengan `/S /reopen`.
- **Risiko fork**: feed upstream menawarkan Setup app upstream yang berkongsi folder pemasangan `Poni`, jadi ia akan menimpa Halo Island. Dalam pass ini feed dimatikan (lihat A2Z-03/04).

## 13. Modul utama dan tanggungjawab (Windows)

| Modul | Tanggungjawab |
|---|---|
| `app.rs` (5.9k) | Message loop, state machine island, input, timer, laksana `Command` |
| `state.rs` | `AppState` dikongsi pages/activities/app |
| `events.rs`, `commands.rs` | Kontrak service→UI dan page→app |
| `activities.rs`, `pill/*` | Island tertutup + tema Pill/Diorama |
| `gfx.rs`, `glass.rs`, `ui.rs`, `icons.rs`, `island.rs`, `window.rs` | Asas rendering/UI |
| `pages/*` | 20 halaman (Music, Today, Shelf, Tasks, Focus, AI, upstream, add-ons, Settings) |
| `services/*` | 38 service OS (media, lyrics, audio, bluetooth, fullscreen, power, notifications, addons, update, http, …) |
| `tray.rs`, `autostart.rs`, `shell.rs`, `dragdrop.rs` | Integrasi OS |
| `settings_window.rs`, `settings_host.rs` | Tetingkap Advanced settings |
| `updates.rs`, `version.rs` | Versi baru, label About |
| `selftest.rs` (5.6k) | `poni.exe --selftest <dir>`: lawatan skrip semua state + screenshot untuk CI |

## 14. Windows-only vs boleh compile di Linux

| Komponen | Linux `cargo check`/`test` | Nota |
|---|---|---|
| `poni-core`, `studio-usage`, `studio-diorama` | Ya (ujian penuh) | Pure Rust |
| `poni` (target host Linux) | Compile, tapi jadi stub `main()` sahaja; 0 ujian | Semua modul sebenar di belakang `cfg(windows)` / `cfg(target_os="macos")` |
| `poni` untuk Windows | `cargo wcheck` (type-check) jalan di Linux tanpa linker; `cargo wbuild` perlu `mingw-w64` | Disahkan lulus di node ini (A2Z-05) |
| `poni` untuk Mac | `cargo mcheck` gagal di node ini: `aws-lc-sys` (melalui `str0m`→`dimpl`) perlu C cross-compiler Apple | Masalah toolchain node, bukan kod |
| `server/` | Node 20 | Tidak disentuh |

## Run 4 — titik sambungan untuk skop Encik Koliji

| Keperluan | Di mana dalam base | Nota |
|---|---|---|
| 3-mode hide | `services/fullscreen.rs` (event-driven: `ABN_FULLSCREENAPP`, WinEvent hooks, tiada poll) lapor `AppEvent::Fullscreen` dan `AppEvent::TopWindows`; `app.rs::apply_fullscreen`, `sync_aside`; `activities::Mode::Aside` | Upstream sudah ada "Step aside" (strip halimunan) tapi dipaksa OFF sejak 0.2.6. Halo guna semula pengesan yang sama, tukar strip kepada handle 34×8 yang kelihatan |
| Monitor | `Settings::monitor: MonitorChoice` (Primary, Cursor), `window::pick_monitor`, `App::refresh_monitor` | Tambah Secondary + All; All ikut `EVENT_SYSTEM_FOREGROUND` yang sudah ada (`AppEvent::Foreground`) |
| Lesen | `poni-core::sha256` (SHA-256 sendiri, tiada crate) | `poni-core::license` guna ini |
| Light/dark | `services::apps_use_light_theme`, `WM_SETTINGCHANGE` | Classic kini ikut Windows dengan palet Halo (`look::HALO_DARK/LIGHT`) |
| Teks input dalam Settings window | `ui::text_input` wujud, tetapi window Settings tiada laluan papan kekunci | Tambah `WM_CHAR`/`WM_KEYDOWN` ke `View::input` bila medan fokus |
| Clipboard | `services/clipboard.rs` (baca/tulis sahaja; sejarah upstream dibuang 0.2.8) | Halo tambah senarai dalam memori, `WM_CLIPBOARDUPDATE` pada helper window |
| Bluetooth/bateri peek | `services/bluetooth.rs`, `services/power.rs`, `HudKind::{Charging, Bluetooth}` | Sudah ada; satu nilai bateri setiap peranti (Windows tidak beri L/R/kotak berasingan) |
