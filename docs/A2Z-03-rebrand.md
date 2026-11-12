# A2Z-03 — Rebrand: Halo Island (by kolijijani)

## Prinsip

1. Hanya nama yang **dilihat pengguna** bertukar. Satu pemalar: `crate::version::APP_NAME = "Halo Island"` (`crates/poni/src/version.rs`). Kod yang membina teks dengan nama app memanggilnya.
2. Identifier dalaman **kekal**, ikut AGENTS.md ("Names that stay") dan brief. Keputusan ini perlu pengesahan kemudian (K1 dalam A2Z-02).
3. Atribusi MIT kekal: `LICENSE` dan `THIRD-PARTY-NOTICES.txt` tidak disentuh. Copyright dalam version info exe dan installer kekal `pemilik upstream`. Hanya `CompanyName`/publisher jadi kolijijani.
4. README: satu nota di atas sahaja ("Forked from app upstream as Halo Island by kolijijani").

## Apa yang bertukar

| Tempat | Dulu | Sekarang | Fail |
|---|---|---|---|
| Tooltip tray | `app upstream 0.2.30` | `Halo Island 0.2.30` | `tray.rs` |
| Menu tray / klik kanan | `About app upstream …`, `Quit app upstream`, `Update to app upstream …` | `About Halo Island …`, `Quit Halo Island`, `Update to Halo Island …` | `version.rs`, `tray.rs` |
| Tajuk window island + visual (Alt+Tab tiada, tapi alat aksesibiliti/Spy++ nampak) | `Poni` | `Halo Island` | `window.rs` |
| Tajuk tetingkap Advanced settings | `app upstream Advanced Settings` | `Halo Island Advanced Settings` | `settings_host.rs` |
| Kad versi Settings (About) | `app upstream` | `Halo Island` | `settings_window.rs` |
| Start with Windows (penerangan) | `Opens app upstream when you sign in` | `Opens Halo Island …` | `settings_window.rs` |
| Helaian kemas kini | `A new version of app upstream …`, `See what's new in app upstream …` | Halo Island | `pages/settings.rs` |
| Notis "up to date" | `app upstream is up to date` | `Halo Island is up to date` | `updates.rs` |
| Teks bantuan izin | Notifications, lokasi (Prayer), rangkaian (Shelf phone drop), pautan Shelf | Halo Island | `pages/notifications.rs`, `pages/prayer.rs`, `pages/shelf.rs` |
| Nama proses sendiri dalam halaman System | `app upstream` | `Halo Island` | `services/sysmon.rs` |
| Halaman telefon (phone drop) | `<title>app upstream · Phone drop` | `Halo Island · Phone drop` | `poni-core/src/phonedrop/page.html`, semakan `selftest.rs` |
| Meeting notes | `…when app upstream closed`, footer `Notes by Oyen, in app upstream` | Halo Island | `poni-core/src/notes.rs` |
| Version info `poni.exe` (Properties, Task Manager) | `ProductName` app upstream, `CompanyName` pemilik upstream | `Halo Island`, `kolijijani`; copyright kekal pemilik upstream | `crates/poni/res/poni.rc` |
| Installer NSIS (installer CI) | Nama, welcome, finish, section, publisher | Halo Island / kolijijani; buang juga shortcut lama `app upstream.lnk` | `installer/poni.nsi` |
| Installer Inno | `AppName`, publisher | Halo Island / kolijijani; buang `app upstream.lnk` | `installer/poni.iss` |
| Semakan installer CI | Jangka `app upstream` | Jangka `Halo Island` | `tools/ci-installer.ps1` (skrip, bukan workflow) |
| README | — | Nota fork di atas | `README.md` |

## Apa yang KEKAL (sengaja)

| Identifier | Nilai | Sebab |
|---|---|---|
| Nama exe / bin | `poni.exe`, `[[bin]] name = "poni"` | Brief + AGENTS.md; CI, installer, autostart bergantung |
| Nama crate | `poni`, `poni-core`, `studio-usage`, `studio-diorama` | Brief: jangan rename dalam pass ini |
| Kelas window | `Poni.Island`, `Poni.Helper`, `Poni.Visual`, `Studio.Settings` | Installer (`FindWindow "Poni.Island"`) dan skrip smoke CI cari window ikut kelas |
| Mutex | `Local\Poni.Island` | Satu salinan sahaja |
| Folder | `%LOCALAPPDATA%\Programs\Poni`, `%APPDATA%\Poni`, `%LOCALAPPDATA%\Poni` | Settings + data |
| Registry | uninstall key `Poni`, Run value `Poni`, `HKCU\Software\Poni`, Inno `AppId` | Upgrade + uninstall |
| Env | `PONI_HOME`, `PONI_RENDERER`, `PONI_SELFTEST_*`, `PONI_INFERENCE_URL` | Skrip + CI |
| Mac | bundle id upstream, `Info.plist` | Mac di luar skop MVP |
| MSIX | `AppxManifest.xml` (DisplayName app upstream) | Perlu identiti Partner Center kolijijani dahulu (M5) |
| Nama fail output Setup | `Halo-Studio-Setup-<v>.exe` | `ci.yml` merujuk nama ini; brief larang sentuh workflow |
| URL upstream | `REPO`, `addons::RELEASES`, `update::FEED_URL`, server drop upstream, `(API upstream)` | Brief: jangan ubah URL upstream. Feed dimatikan (di bawah) |
| Jenama perkhidmatan pihak ketiga | "upstream" (akaun, tab upstream), "app upstream Recorder", `Documents\app upstream\Notes`, `Downloads\app upstream` | Nama produk/folder lain, bukan nama app ini. Tukar folder = data lama hilang |
| Log dalaman | `pinfo!("Poni … starting")` | Log, bukan UI |
| Teks Mac (`src/mac/*`) | app upstream | Mac di luar skop MVP |
| Watak Oyen, tema Oyen, ikon `poni.ico`, art installer | Upstream | Perlu aset Halo Island sendiri (K5) |

## Update feed dimatikan

`crates/poni/src/updates.rs`: `pub const FEED_ON: bool = false;` dengan komen `ponytail:`. Ia diperiksa di 4 tempat (`app.rs` ×2, `mac/app.rs` ×2): tiada `Updater`, tiada timer, tiada `pending_install`.

Sebab: feed `feed upstream (`latest.json`)` menawarkan Setup app upstream yang memasang ke folder `Poni` yang sama. Kalau feed hidup, Halo Island akan "dikemas kini" menjadi app upstream dalam masa sejam selepas start. Mematikannya juga mengurangkan kerja semasa rehat (satu bacaan sejam hilang). Untuk menghidupkan semula: feed kolijijani bertandatangan sendiri (A2Z-02 M6).

## Run 4 — tambahan

| Tempat | Sekarang | Fail |
|---|---|---|
| Settings › About | Tagline "A living notch for Windows: by kolijijani" dan "Free dah lengkap — Pro untuk power user." | `version.rs` (`TAGLINE`, `TAGLINE_PRO`), `settings_window.rs` |
| Settings › Halo Pro | Seksyen baru: input key, Activate, "Pro aktif", ralat "Key tak sah. Semak dan cuba lagi." | `settings_window.rs` |
| Peek bateri | "Mengecas", "Bateri rendah" (Classic + Pill) | `activities.rs`, `pill/hud.rs` |
| Tajuk kemas kini (feed) | "Halo Island x.y.z" | `poni-core/src/update.rs` |
| Self-test | "Halo Island's own frames" | `selftest.rs` |
| Classic | Pill #0B0D12 / mata #F2F5FA (gelap), #F3F5F9 / #141821 (cerah), ikut Windows | `poni-core/src/look.rs`, `pill/mod.rs` |

Sengaja kekal (identifier/folder, AGENTS.md "Names that stay"): `Downloads\app upstream`, `Documents\app upstream\Notes` (folder yang wujud pada PC pengguna), nama app pihak ketiga "app upstream Recorder", "Studio Pro" milik akaun upstream (berbeza daripada Halo Pro), log dalaman Mac.
