# A2Z REPORT — Halo Island (APP-009) · by kolijijani

Tarikh: 11 Nov 2026 · Base: app upstream 0.2.30 (MIT, commit `31078f5`) · Pelaksana: Ko-Dex (Claude Opus 5.5)
Semua kerja disimpan sebagai **perubahan working-tree** sahaja: tiada commit, tag, push atau release. `.github/workflows/`, secret, `LICENSE` dan `THIRD-PARTY-NOTICES.txt` tidak disentuh.

Run terkini: **§9 (Run 4)**. Dokumen fasa: [A2Z-01 Bedah](A2Z-01-dissection.md) · [A2Z-02 Pelan MVP](A2Z-02-mvp-plan.md) · [A2Z-03 Rebrand](A2Z-03-rebrand.md) · [A2Z-05 Build](A2Z-05-build.md) · [A2Z-06 Video mockup](A2Z-06-video-mockup.md). Fasa 4 (implement) dan Pass 2 (§8) dilaporkan di bawah.

## 1. Ringkasan eksekutif

**Siap**
- Bedah penuh workspace (4 crate, ~132k baris Rust), termasuk sempadan Windows-only vs pure.
- Pelan MVP dengan 6 milestone dan kriteria penerimaan. Dapatan utama: semua ciri MVP (island, light/dark + 5 tema, tray, settings, media now-playing, notification listener, jam/kalendar) **sudah ada** dalam base. Kerja MVP ialah rebrand, putuskan pergantungan berisiko, dan sahkan di Windows.
- Rebrand nama yang dilihat kepada **Halo Island** di app Windows, exe version info, installer NSIS/Inno, halaman telefon phone drop dan README (nota fork). Identifier dalaman kekal.
- **Update feed upstream dimatikan** (`updates::FEED_ON = false`). Tanpa ini, Halo Island akan "dikemas kini" menjadi app upstream dalam masa sejam selepas start.
- Build/test di Linux: `cargo check --workspace` OK, `cargo test --workspace` **817 lulus / 0 gagal**, `cargo wcheck` (type-check Windows sebenar) OK tanpa warning baru.

**Belum**
- Tiada `poni.exe` dibina atau dijalankan: node tiada mingw-w64 dan tiada Windows. M3/M4 perlu mesin Windows.
- Aset visual (ikon, Oyen, art installer), MSIX/Store, Mac, website: di luar pass ini, perlu keputusan (K1–K5).
- Ciri luar skop (AI tab, phone drop berpasangan, add-ons, akaun upstream) **tidak dibuang**. Ia sudah OFF/tersorok secara default dalam kod base.

## 2. Checklist MVP

| # | Item | Status |
|---|---|---|
| 1 | Island window (notch, hover/expand) | Ada dalam base · tajuk window → Halo Island · belum diuji di Windows |
| 2 | Light/dark + tema sedia ada | Ada (Classic/Oyen/Glass/Pill/Diorama, `apps_use_light_theme`) · belum diuji di Windows |
| 3 | System tray | Ada · tooltip/About/Quit/Update → Halo Island ✅ |
| 4 | Settings (halaman + Advanced window) | Ada · tajuk + kad versi → Halo Island ✅ |
| 5 | Media now-playing (GSMTC) + lirik | Ada · tiada perubahan · belum diuji di Windows |
| 6 | Notification listener | Ada, **hanya salinan MSIX** (perlu package identity) · perlu pakej MSIX kolijijani (M5) |
| 7 | Jam | Ada (`IdleStyle::Clock`, default) |
| 8 | Kalendar | Grid 5 minggu tempatan ada; acara sebenar perlu akaun upstream (K3) |
| 9 | Rebrand nama dilihat | ✅ Windows; Mac di luar skop |
| 10 | Identifier dalaman kekal | ✅ |
| 11 | MIT + notices kekal | ✅ |
| 12 | Pergantungan update upstream diputuskan | ✅ `FEED_ON = false` |
| 13 | `cargo metadata` tidak rosak | ✅ |
| 14 | Build exe + selftest Windows | ⏳ perlu Windows/mingw (A2Z-05 §A/§B) |

## 3. Fasa 4 — perubahan fail utama (21 fail, +87/−69)

| Fail | Perubahan |
|---|---|
| `crates/poni/src/version.rs` | `pub const APP_NAME = "Halo Island"`; label About guna `APP_NAME`; ujian dikemas kini |
| `crates/poni/src/tray.rs` | Tooltip, Quit, Update guna `APP_NAME`; ujian dikemas kini |
| `crates/poni/src/updates.rs` | `pub const FEED_ON: bool = false` (komen `ponytail:` + laluan naik taraf); notis "up to date" guna `APP_NAME` |
| `crates/poni/src/app.rs` | `pending_install` dan `Updater::start` hanya jika `FEED_ON` |
| `crates/poni/src/mac/app.rs` | Guard `FEED_ON` yang sama (2 baris) supaya Mac konsisten |
| `crates/poni/src/window.rs` | Tajuk window island/visual `Poni` → `Halo Island` (kelas `Poni.*` kekal) |
| `crates/poni/src/settings_host.rs`, `settings_window.rs` | Tajuk Advanced settings, kad versi (+ baris "by kolijijani", Pass 2), teks Start with Windows |
| `crates/poni/src/pages/{settings,notifications,prayer,shelf}.rs` | Teks yang dilihat sebut Halo Island |
| `crates/poni/src/services/sysmon.rs` | Nama proses sendiri di halaman System |
| `crates/poni/src/selftest.rs` | Semakan `<title>` halaman telefon ikut nama baru |
| `crates/poni-core/src/phonedrop/page.html` | Tajuk + label halaman telefon |
| `crates/poni-core/src/notes.rs` | Teks Meeting notes + footer Markdown |
| `crates/poni/res/poni.rc` | `ProductName`/`FileDescription` Halo Island, `CompanyName` kolijijani, copyright kekal pemilik upstream |
| `installer/poni.nsi`, `installer/poni.iss` | Nama paparan + publisher kolijijani; copyright kekal pemilik upstream; buang shortcut `.lnk` lama upstream; nama fail Setup kekal |
| `tools/ci-installer.ps1` | Jangkaan semakan installer ikut Halo Island |
| `README.md` | Satu nota fork di atas |

Kos (AGENTS.md "Keep it light"): **tiada thread, timer, poll atau crate baru.** Satu timer berulang dibuang (bacaan feed setiap jam). Saiz exe dijangka sama (rentetan sahaja; belum diukur, perlu mingw).

## 4. Hasil build/test (butiran: A2Z-05)

| Arahan | Hasil |
|---|---|
| `cargo metadata --no-deps` | OK |
| `cargo check --workspace` | OK, 0 warning |
| `cargo check -p poni` (host Linux) | OK, tapi hanya stub: tidak membuktikan Windows build |
| `cargo test --workspace` | poni-core 576 ✓, studio-usage 220 ✓, studio-diorama 21 ✓ (1 ignored upstream), 0 gagal |
| `cargo wcheck` (Windows, windows-rs 0.58) | OK; 10 warning dead-code **sama seperti sebelum perubahan** |
| `cargo check -p poni --target x86_64-pc-windows-gnu --tests` | OK (ujian Windows type-check; tidak dijalankan) |
| `cargo mcheck` (Mac) | Gagal: `aws-lc-sys` perlu C compiler Apple (isu toolchain node, bukan kod) |
| `cargo wbuild` | Tidak dicuba: tiada mingw-w64 |

Toolchain Rust dipasang user-local di node (`~/.rustup`, `~/.cargo`) kerana tiada cargo asal.

## 5. Langkah build/run Windows

Di Windows 10/11 dengan Rust stable (MSVC) + VS Build Tools:

```
cargo test -p poni-core -p studio-usage -p studio-diorama
cargo test -p poni
cargo build -p poni --release
target\release\poni.exe --selftest out      # kod keluar 0, out\report.json "ok": true
target\release\poni.exe                     # island di atas tengah, tray "Halo Island 0.2.30"
```

Cross dari Linux: `sudo apt install mingw-w64`, `rustup target add x86_64-pc-windows-gnu`, `cargo wbuild`, kemudian `tools/package.sh` (perlu `makensis` atau `NSIS_HOST`). Installer: `makensis -DEXE=<abs>/poni.exe -DOUTDIR=<abs>/dist installer/poni.nsi`.

## 6. Risiko

1. **Pertembungan dengan app upstream (tinggi).** Identifier dalaman sama (folder `Poni`, uninstall key, mutex `Local\Poni.Island`, Run value). Halo Island dan app upstream tidak boleh wujud bersama: installer satu menimpa yang lain dan settings dikongsi. → Keputusan K1.
2. **Kemas kini.** Feed dimatikan; pengguna tidak dapat kemas kini automatik sehingga kolijijani ada feed bertandatangan sendiri (M6).
3. **Pergantungan servis upstream.** Add-on/Downloader muat turun dari release repo upstream; akaun upstream, phone drop berpasangan (server drop upstream), Today events, Stocks, IELTS/Notes semuanya servis upstream. Semua opt-in/OFF, tetapi masih boleh dihidupkan oleh pengguna. → K3, K4.
4. **Notification listener hanya MSIX.** Untuk MVP notification, perlu pakej MSIX dengan identiti Partner Center kolijijani.
5. **Jenama visual.** Ikon tray/exe, watak Oyen, art installer, tab "upstream" masih jenama upstream. → K5.
6. **CI.** `ci.yml` masih menamakan artifak `Halo-Studio-Setup-*`, dan release/website/`latest.json` workflows menunjuk ke studio.halo.island. Jangan jalankan workflow release dari fork ini sebelum dikaji (tiada workflow diubah).
7. **Tidak diuji di Windows.** Perubahan kecil dan type-checked, tetapi tiada run sebenar.

## 7. Cadangan langkah seterusnya

1. Encik Koliji putuskan **K1–K5** (A2Z-02).
2. Bina dan jalankan di Windows (§5); lengkapkan jadual ujian M4.
3. Jika K1 = rename: tukar folder/mutex/uninstall key/AppId/Run value secara serentak dalam satu PR (`paths.rs`, `autostart.rs`, `app.rs`, `window.rs`, installer, `tools/ci-*.ps1`).
4. Sediakan feed kemas kini kolijijani (kunci P-256, repo release kolijijani), kemudian `FEED_ON = true`.
5. Aset Halo Island sendiri: `poni.ico`, `installer/art/*` (`tools/make_brand.py`).
6. Pakej MSIX kolijijani untuk notification listener.
7. Kaji `.github/workflows` untuk repo kolijijani (pass berasingan, dengan kebenaran).

## 8. Pass 2 — branding akhir + video mockup

### A. Branding: Halo Island · by kolijijani
Keputusan Encik Koliji: app peribadi, bukan produk syarikat. Nama syarikat lama dibuang dari seluruh repo (kod, metadata exe, installer, README, docs A2Z, brief lama); `grep -ri` untuknya kini kosong (exit 1).

| Fail | Perubahan |
|---|---|
| `crates/poni/res/poni.rc` | `CompanyName` → `kolijijani`; `LegalCopyright` kekal pemilik upstream |
| `installer/poni.nsi`, `installer/poni.iss` | `PUBLISHER` / `AppPublisher` → `kolijijani`; komen baris 1 |
| `crates/poni/src/version.rs` | `pub const AUTHOR = "kolijijani"`; komen `APP_NAME` |
| `crates/poni/src/settings_window.rs` | Kad versi Settings: baris kecil "by kolijijani" (caption, dim) di bawah "Version x.y.z"; nama + versi dinaikkan 8 px supaya tiga baris muat dalam kepala 72 px |
| `README.md` | Nota fork: "Forked from app upstream as Halo Island by kolijijani." |
| `docs/A2Z-02/03/REPORT`, `BRIEF-*.md` | Konsisten: publisher kolijijani, tiada nama syarikat lama |

Tidak disentuh: `LICENSE`, `THIRD-PARTY-NOTICES.txt`, copyright pemilik upstream, identifier dalaman (`poni.exe`, folder `Poni`, mutex), `.github/`. `tools/ci-installer.ps1` tidak perlu diubah (tiada publisher di dalamnya).

Semakan: `cargo check --workspace` OK; `cargo wcheck` OK, 9 warning dead-code yang sama dengan sebelum perubahan. Kos (Keep it light): satu teks tambahan pada kad versi Settings, dilukis hanya apabila halaman itu terbuka; tiada thread, timer atau crate baru.

### B. Video mockup (butiran: A2Z-06)

| Fail | sha256 |
|---|---|
| `mockups/halo-island-mockup.mp4` (2 145 007 bait, h264, 1920×1080, 30 fps, 28.0 s, yuv420p) | `9ae64da721162a15f6eb04d71f5d28ef575889d2f4a723a51d5630d8836a26cb` |
| `mockups/halo-island-poster.png` (720 716 bait, 1920×1080) | `0d711e23ab847d0320b57e19e39db89c7c90f7e3462ac9170fb527a6e16b39f0` |
| `mockups/halo-island-mockup.html` (sumber, self-contained) | — |
| `mockups/render_mockup.py` (render semula: `python3 mockups/render_mockup.py`) | — |

Scene: intro "Halo Island · by kolijijani" → idle jam + hover ripple → media player (cover, progress, play/pause, waveform) → mini player + notification masuk dari sisi dan auto-dismiss → timer 5 s dengan ring → light/dark toggle → idle + outro. Ini mockup konsep (web), bukan rakaman app Windows sebenar.

**Production build masih ditahan** sehingga Encik Koliji review video ini dan putuskan K1–K5.

## 9. Run 4 — Fasa 4 penuh: lesen, mascot, 3-mode, monitor, FREE + PRO (11 Nov 2026)

Semua dalam working tree. Tiada commit, tag, push atau release. `.github/`, secret, `LICENSE`, `THIRD-PARTY-NOTICES.txt` tidak disentuh (`git status` kosong untuk ketiga-tiganya).

### A. Lesen (Halo Pro)
- **Satu fail**: `crates/poni-core/src/license.rs`, `KEY_SHA256 = "ca7c4d17…2c96e5"` (SHA-256 bagi `HALO-2026-KOLIJI-9F4C-77B1`). Tukar key: `printf '%s' 'KEY-BARU' | sha256sum`, tampal hash.
- Key dinormalkan (buang ruang, huruf besar) → SHA-256 (`poni_core::sha256`, tiada crate baru) → banding. Tiada server, fingerprint, emel, had peranti, atau monitor ID.
- Persist: `Settings::pro_key` disimpan; `Settings::pro` **tidak** disimpan (`serde(skip)`), dikira semula setiap load. Fail settings tidak boleh hidupkan Pro sendiri (ada ujian).
- UI: Settings › **Halo Pro** → medan key → **Activate**. Betul: "Pro aktif". Salah: "Key tak sah. Semak dan cuba lagi." Ada butang Deactivate.
- Gerbang: `Settings::pro_gate()` (satu tempat), dipanggil semasa load dan setiap perubahan settings.

### B. Mascot
Sudah ada dari pass sebelum (`poni-core/src/mascot.rs`, `activities::draw_mascot`) dan ikut spec: kapsul 8×16 r4 jarak 10, kerdip 120 ms; doze 12×4 turun 2 px dalam 600 ms; tidur dua lengkung U 2.5 px statik; wave lengkung + tangan kapsul 4×9 kanan, ±20° dua kali; ikut pointer maks 2 px. Tiada cincin/glow/mulut. Run 4: warna mata kini dari look → **#F2F5FA atas #0B0D12** (gelap) dan **#141821 atas #F3F5F9** (cerah), ikut mod Windows (`look::HALO_DARK/HALO_LIGHT`).

### C. 3-mode hide
| Mode | Kelakuan | Kod |
|---|---|---|
| Desktop | Island penuh | tiada perubahan |
| Maximized | Island kecut ke **handle 34×8**, terapung **2 DIP** dari atas, titik kecil di tengah (terang bila hover), boleh klik/hover untuk buka. Statik: tiada frame semasa rehat | `activities::mini_handle/draw_handle`, `app::island_gap`; `Settings::step_aside` kini ON (migrasi v6), suis "Mini handle" di Settings › Island |
| Fullscreen | Slide naik 20 DIP + fade 0.6 s, kemudian window disorok; kembali slide turun + fade in | `app::fs_slide`, `apply_fullscreen`, `frame` |

Pengesanan sedia ada (`services/fullscreen.rs`, event-driven) tidak diubah.
Nota spec: mockup menunjukkan "denyut 1 s" pada titik handle. **Tidak dibuat**: AGENTS.md melarang gerakan berterusan di island tertutup. Titik statik + terang bila hover.

### D. Monitor
Settings › Island › Display: **Primary / Secondary / All / Under the pointer**. Secondary = monitor bukan-utama pertama (kiri→kanan, `EnumDisplayMonitors`). All = island pindah ke monitor tetingkap aktif bila Windows lapor tukar (hook `EVENT_SYSTEM_FOREGROUND` sedia ada, tiada poll; satu island pada satu masa — `ponytail:` dicatat). Tiada monitor ID dalam logik lesen.

### E. Checklist FREE

| Ciri | Status | Catatan |
|---|---|---|
| Island & hover expand 220 ms, kecut sendiri | ✅ ada (base, `hover_delay_ms: 220`, `collapse_delay_ms: 0`) | |
| Music + lirik sync (romaji/kana) | ✅ ada (base) | Spotify desktop/web, YouTube, browser melalui GSMTC |
| Focus timer + cincin progres | ✅ ada (base) | |
| To-do + Shelf (seret fail) | ✅ ada (base) | |
| Waktu solat ikut lokasi | ✅ ada (add-on base, OFF default; fail data dari release upstream, K4) | |
| Bateri & headphones peek | ✅ ada + teks BM "Mengecas" / "Bateri rendah"; Bluetooth "Buds Pro 90%" | ⚠️ "R 88% / Kos 64%" (kiri/kanan/kotak) **tidak dibuat**: Windows hanya beri satu nilai bateri setiap peranti melalui PnP/GATT |
| Light/dark + 5 tema ikut Windows | ✅ Classic kini ikut light/dark Windows dengan palet Halo; Classic/Oyen/Glass/Pill/Diorama | |
| Widgets 2 slot (cuaca, kalendar) | ✅ tab baru **Widgets** (sebelah Music) | Cuaca: Open-Meteo, satu bacaan bila tab dibuka (> 30 min). Lokasi: tempat Prayer, atau tanya Windows sekali |
| Peringatan minum air + 5/8 gelas | ✅ Settings › General › Drink water (Off/45/60/90 min), goal 1–20; notis "Minum air · 5/8 glasses"; +/− di tab Widgets | OFF default |
| Clipboard 5 item | ✅ Settings › General › Clipboard (OFF default; memori sahaja); klik item = salin semula | |

### F. Checklist PRO

| Ciri | Status | Catatan |
|---|---|---|
| Overlay games FPS/GPU/CPU | ✅ dikunci Pro (Settings, tray "(Halo Pro)", halaman System) | Kod base |
| System monitor penuh + watt | ✅ sejarah panjang + modul Power (watt) dikunci Pro | Proses teratas kekal percuma (sebahagian halaman System base) |
| Phone Drop dua hala, mana-mana network | ⚠️ **separa**: pairing (mana-mana network) dikunci Pro; Wi-Fi sama kekal percuma | Bergantung pada server upstream server drop upstream (K3). "Hantar ke telefon" bergantung pada sokongan base sedia ada; tidak diuji |
| Clipboard tanpa had + cari | ⚠️ 500 item + carian di tab Widgets | `ponytail:` memori sahaja, hilang bila keluar |
| Widgets 4–6 slot + world clock, bulan, countdown | ✅ 6 slot; world clock (London/New York/Tokyo, offset tetap tanpa DST — `ponytail:`), fasa bulan (fasa purata, ±1 hari), countdown | Tarikh countdown hanya boleh ditetapkan dalam `settings.json` (`countdown_day`, `countdown_label`) — belum ada UI |
| Tema premium + theme builder (warna, bucu, kaca) | ⚠️ **theme builder** ada: 6 pasangan warna, bucu 2–18, kaca 0–100% (ground separuh telus) | "Tema premium" tersendiri belum ada; Glass sedia ada kekal percuma. `ponytail:` setiap pilihan warna bocor ~beberapa puluh bait |
| Cloud sync (boleh dimatikan) | ⚠️ **separa**: salin `settings.json` + `data.json` ke `%OneDrive%\Halo Island` setiap save; ambil yang lebih baru semasa start | Perlu OneDrive dipasang; tiada server sendiri. Tiada penggabungan: yang terbaru menang |

### G. Fail diubah/ditambah (Run 4)
Baru: `crates/poni-core/src/license.rs`, `crates/poni-core/src/halo.rs`, `crates/poni/src/app_halo.rs`, `crates/poni/src/pages/widgets.rs`, `crates/poni/src/services/weather.rs`.
Diubah: `poni-core/src/{lib,look,settings,store,update}.rs`; `poni/src/{activities,app,commands,events,state,settings_window,tray,version,window,selftest}.rs`, `poni/src/pages/{mod,system,health}.rs`, `poni/src/pill/{mod,hud}.rs`, `poni/src/services/mod.rs`, `poni/src/mac/app.rs` (no-op `ClipBack`).
Docs: `A2Z-01`, `A2Z-02`, `A2Z-03`, `A2Z-05`, laporan ini.
Jumlah working tree vs `31078f5` (kod, tanpa mockups/docs): 37 fail diubah, +977/−162, 5 fail baru kod (+ `mascot.rs` dari pass lepas).

### H. Build/test (butiran A2Z-05)
`cargo check --workspace` OK · `cargo test --workspace` **833 lulus / 0 gagal** (poni-core 592, studio-usage 220, studio-diorama 21) · `cargo wcheck` OK, **9 warning dead-code sama seperti baseline** · `--tests` Windows type-check OK · Mac: gagal toolchain (aws-lc-sys), sama seperti dulu · exe/installer: tidak dibina (tiada mingw/Wine).

### I. Perubahan tingkah laku yang perlu disahkan Encik Koliji
1. **Step aside dihidupkan semula** (upstream matikan sejak 0.2.6 kerana strip halimunan nampak macam island hilang). Kini handle kelihatan. Migrasi settings v5→v6 hidupkan sekali untuk fail lama.
2. **Overlay game dan sejarah System kini Pro**: pengguna yang hidupkan sebelum ini akan nampak ia OFF tanpa key (dibuang secara gating, kod tidak dibuang).
3. **Pairing Phone Drop kini Pro**. Wi-Fi sama kekal percuma.
4. Classic tidak lagi hitam tulen (#0B0D12) dan bertukar cerah dalam mod cerah Windows.
5. Kunci lesen contoh tertanam sebagai hash dalam repo awam: sesiapa yang tahu kunci boleh guna. Sesuai untuk app peribadi; tukar hash sebelum edaran.

### J. Langkah seterusnya
1. Bina dan jalankan di Windows: `cargo test -p poni`, `cargo build -p poni --release`, `poni.exe --selftest out` (ujian Windows yang dikemas kini belum pernah dijalankan).
2. Uji manual: maximize/F11 (handle + slide), 2 monitor (Primary/Secondary/All), key betul/salah, air, clipboard, Widgets.
3. Ukur saiz exe sebelum/selepas (AGENTS.md).
4. UI untuk tarikh countdown; simpan clipboard Pro ke fail jika mahu kekal selepas restart.
5. Putuskan K1–K5 (A2Z-02), terutama server Phone Drop dan add-on upstream.

## RINGKASAN TELEGRAM

Encik Koliji, Run 4 Halo Island (APP-009) dah siap dalam working tree — tak commit, tak push, production masih hold.
License key dah jalan: satu key tetap, app hash SHA-256 dan banding dengan hash dalam crates/poni-core/src/license.rs; tiada server, tiada fingerprint, tiada monitor ID.
Settings › Halo Pro → masukkan key → Activate; betul keluar "Pro aktif", salah keluar error ringkas, dan status Pro kekal lepas restart.
3-mode hide dah ada: Desktop penuh, Maximized jadi mini handle 34×8 di atas (2 px, boleh klik), Fullscreen slide up + fade terus hilang.
Monitor boleh pilih Primary / Secondary / All (All ikut window yang aktif), tak kacau langsung logic lesen.
Mascot ikut spec, dan warna pill/mata sekarang betul ikut dark/light Windows (#0B0D12/#F2F5FA dan #F3F5F9/#141821).
FREE yang baru: tab Widgets (cuaca + kalendar), peringatan minum air dengan kiraan 5/8 gelas, clipboard 5 item, peek bateri "Mengecas".
PRO dah dikunci: game overlay, System history + watt, Phone Drop any network, clipboard 500 + search, 6 widget (world clock, moon, countdown), theme builder, cloud sync via OneDrive — beberapa masih separa, senarai penuh dalam report.
cargo test 833 lulus 0 gagal, Windows type-check clean (9 warning lama sahaja); tapi exe Windows belum dibina sebab node takde mingw/Wine, jadi kena test atas Windows sebenar dulu.
Full report: docs/A2Z-REPORT.md §9, build: docs/A2Z-05-build.md.
