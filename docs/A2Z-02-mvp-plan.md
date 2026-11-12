# A2Z-02 — Pelan MVP Halo Island

## Dapatan utama

Hampir semua ciri MVP **sudah ada dan berfungsi** dalam app upstream 0.2.30. Kerja MVP bukan menulis ciri baru. Kerjanya: rebrand, putuskan pergantungan pada servis upstream yang berisiko untuk fork, dan sahkan setiap ciri di Windows sebenar. Ini selari dengan "Keep it light": tiada kod baru yang berjalan semasa rehat.

## Skop MVP (IN)

| Ciri | Status dalam kod | Kerja untuk MVP |
|---|---|---|
| Island window (notch, hover → expand, spring) | Ada (`window.rs`, `island.rs`, `app.rs`) | Rebrand tajuk window; sahkan di Windows |
| Light/dark + tema sedia ada (Classic, Oyen, Glass, Pill, Diorama) | Ada (`Theme`, `apps_use_light_theme`) | Tiada; sahkan |
| System tray + menu | Ada (`tray.rs`) | Rebrand tooltip, About, Quit, Update |
| Settings (halaman + Advanced settings window) | Ada | Rebrand tajuk window + kad versi |
| Media now-playing (GSMTC) + lirik | Ada (`services/media.rs`, `lyrics.rs`) | Tiada; sahkan |
| Notification listener | Ada (`services/notifications.rs`), **hanya dalam salinan MSIX** (package identity) | Perlu pakej MSIX dengan identiti kolijijani (lihat risiko) |
| Jam / kalendar | Jam rehat ada; grid 5 minggu Today ada | Acara kalendar sebenar bergantung pada akaun upstream: lihat keputusan K3 |

## Luar skop MVP (OUT) dan sebab

| Ciri | Sebab | Tindakan pass ini |
|---|---|---|
| Tab AI (`studio-usage`) | Perlu akaun upstream + Studio Pro; servis berbayar upstream | Tiada; sudah tersorok tanpa Pro. Tidak dibuang |
| Phone drop berpasangan (WebRTC, server drop upstream) | Bergantung pada server upstream; `str0m` berat | Tiada; service tidak membuat panggilan sehingga telefon dipasangkan. Tidak dibuang |
| Kamus Jepun + add-on lain (Prayer, IELTS, Notes, Stocks, Downloader…) | Data besar / servis upstream | Tiada; semua add-on OFF secara default. Fail add-on dimuat turun dari release upstream: perlu keputusan (K4) |
| Build Mac | Brief: Windows dahulu; tiada Mac/notarization kolijijani | Hanya dua guard kecil dalam `mac/app.rs` untuk kekal konsisten; tiada rebrand Mac penuh |
| Akaun upstream (tab upstream, wang, Health steps, Today events) | Ekosistem pihak ketiga (API upstream) | Tiada; kekal pilihan, OFF sehingga log masuk |
| Update automatik | Feed upstream akan memasang app upstream di atas Halo Island | **Dimatikan** (`updates::FEED_ON = false`) sehingga kolijijani ada feed bertandatangan sendiri |
| Website `site/`, Store listing | Milik upstream; brief tidak minta | Tidak disentuh |

## Milestone

Setiap milestone ada kriteria penerimaan yang boleh diuji. M1–M2 disiapkan dalam pass ini; M3–M6 perlu mesin Windows / keputusan.

### M1 — Rebrand nama yang dilihat (siap, pass ini)
- [x] Tooltip tray = `Halo Island 0.2.30`; menu `About Halo Island …`, `Quit Halo Island`, `Update to Halo Island …`.
  Ujian: `tray::tests::the_menu_says_which_version_this_is`, `a_new_version_comes_first_until_it_is_installed` (dikemas kini).
- [x] Tajuk window island/visual = `Halo Island`; Advanced settings = `Halo Island Advanced Settings`.
- [x] Kad versi Advanced settings + teks "Start with Windows" sebut Halo Island.
- [x] Version info `poni.exe` (`ProductName`, `FileDescription`, `CompanyName`) = Halo Island / kolijijani; copyright kekal pemilik upstream (MIT).
- [x] Nama installer (NSIS + Inno): `Halo Island`, publisher kolijijani; nama fail output Setup kekal (CI bergantung padanya).
- [x] Identifier dalaman kekal (`poni.exe`, `Poni.Island`, `%APPDATA%\Poni`, uninstall key `Poni`, `PONI_*`, nama crate).
- Kriteria: `cargo wcheck` lulus; `cargo test --workspace` lulus; `grep` tiada "app upstream" pada rentetan UI teras Windows (tray, window, settings, update).

### M2 — Putuskan pergantungan berisiko (siap, pass ini)
- [x] Update feed upstream dimatikan: tiada `Updater`, tiada timer `TIMER_UPDATE`, tiada `pending_install` semasa start.
- Kriteria: dengan `FEED_ON = false`, `App::updater` = `None` jadi `menu_line` = `UpdateLine::None` (tiada "Check for updates…"); tiada akses rangkaian ke `feed upstream (`latest.json`)` (sahkan dengan log `poni.log`: tiada baris `updates:`).

### M3 — Build Windows + smoke (perlu mesin Windows atau mingw-w64)
- `cargo build -p poni --release` (MSVC) atau `cargo wbuild` (mingw) menghasilkan `poni.exe`.
- `poni.exe --selftest out\` keluar dengan kod 0; `out\report.json` `"ok": true`.
- Kriteria: properties `poni.exe` tunjuk Product name `Halo Island`; Task Manager tunjuk `Halo Island - a living notch for Windows`.

### M4 — Pengesahan ciri MVP di Windows 10/11
| Ujian | Lulus jika |
|---|---|
| Island | Island hitam di tengah atas; hover 220 ms → buka; tinggalkan → tutup |
| Light/dark | Tukar Settings › Personalization › Colors: Advanced settings ikut tanpa restart |
| Tema | Tray › Theme: 5 tema bertukar; pilihan kekal selepas restart |
| Tray | Tooltip `Halo Island 0.2.30`; Quit menutup app |
| Media | Main Spotify/YouTube di browser: pill muzik + kawalan play/pause/skip |
| Jam | Rehat: jam berubah setiap minit; CPU rehat ≈ 0 (`QueryProcessCycleTime`, bukan Task Manager) |
| Notifications | Salinan MSIX: Allow → notifikasi app terpilih muncul di island |

### M5 — Pakej (perlu keputusan + aset Halo Island sendiri)
- NSIS Setup bernama Halo Island; ikon/art Halo Island sendiri (masih seni Oyen upstream).
- MSIX dengan identiti Partner Center kolijijani (untuk notification listener).

### M6 — Feed kemas kini kolijijani (selepas MVP)
- Kunci P-256 kolijijani, `FEED_URL` + `trusted_key` + `RELEASES` ke repo kolijijani, kemudian `FEED_ON = true`.

## Keputusan terbuka untuk Encik Koliji

- **K1** Rename identifier dalaman (`poni.exe`, folder `Poni`, mutex, uninstall key)? Pass ini KEKALKAN. Kalau tukar: Halo Island boleh dipasang sebelah app upstream tanpa bertindih, tapi itu perubahan besar (installer, autostart, paths, CI).
- **K2** Halo Island dan app upstream berkongsi folder pemasangan + settings. Jika pengguna ada kedua-duanya, satu menimpa yang lain. Berkait dengan K1.
- **K3** Kalendar: kekal melalui akaun upstream (pihak ketiga) atau tulis bacaan kalendar Windows tempatan (WinRT `AppointmentStore`, event-driven) sebagai ciri baru?
- **K4** Add-on dan Downloader memuat turun dari release GitHub upstream (repo upstream). Kekal, cermin ke repo kolijijani, atau sorok halaman Add-ons?
- **K5** Jenama Oyen (kucing, tema Oyen, ikon tray/exe, art installer) masih ada. Ganti dengan aset Halo Island sendiri?

## Run 4 — skop diperluas (arahan Encik Koliji, 11 Nov 2026)

Skop kini melebihi MVP: rebrand + mascot + 3-mode hide + lesen + monitor + semua FREE + semua PRO. Pelan pelaksanaan (ikut "Keep it light"):

| # | Kerja | Pendekatan | Kos semasa rehat |
|---|---|---|---|
| 1 | Lesen | `poni-core/src/license.rs`: hash SHA-256 satu kunci, `Settings::pro_key` disimpan, `pro` dikira semula setiap load (`sanitized`). Tiada server/fingerprint/monitor ID | Tiada |
| 2 | Gerbang Pro | `Settings::pro_gate()` (satu tempat) dipanggil semasa load dan setiap perubahan settings | Tiada |
| 3 | 3-mode | Desktop: biasa. Maximized: `Mode::Aside` = handle 34×8 terapung 2 DIP. Fullscreen: slide naik 20 DIP + fade 0.6 s, kemudian `SW_HIDE` | Tiada (hook sedia ada) |
| 4 | Monitor | `MonitorChoice::{Primary, Secondary, All, Cursor}` | All: hanya bila Windows lapor tetingkap aktif bertukar |
| 5 | FREE baru | Tab Widgets (cuaca, kalendar), air, clipboard 5 | Semua OFF/default ringan; lihat A2Z-05 |
| 6 | PRO | Overlay game, sejarah System + watt, Phone Drop berpasangan, clipboard 500 + cari, 6 widget + world clock/bulan/countdown, theme builder, cloud sync (folder OneDrive) | Hanya bila dihidupkan |
