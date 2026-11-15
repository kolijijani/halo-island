# Grill 0.3.00: stress-test skop sebelum kod

Tarikh: 15 Nov 2026. Status: **Round 1, soalan frontier untuk pemilik.** Belum ada kod.

Sumber: `AGENTS.md`, `docs/backlog.md` (Senarai kerja 0.3.00 FINAL), `docs/ANIMATION-PLAN-0.3.00.md`, dan codebase pada `d3bc830`.

Cara baca: setiap item ada **Fakta** (yang dah disemak dalam code, dengan fail:baris), **Design tree** (keputusan dan apa yang bergantung padanya), **Frontier** (soalan yang boleh dijawab SEKARANG, setiap satu ada cadangan) dan **Terbuka** (soalan round seterusnya, bergantung pada jawapan frontier). Jawab "ya" bermaksud setuju dengan cadangan.

---

## 0. Release dan versi (merentas semua item)

### Fakta

- **`0.3.00` tak sah untuk Cargo.** Dah diuji: `version = "0.3.00"` gagal dengan `error: invalid leading zero in patch version number`. Workspace version ada di `Cargo.toml:6` (`0.2.41`), `Cargo.lock` (4 crate), `installer/halo.nsi`, `installer/halo.iss` dan `.github/workflows/ci.yml:20`.
- **0.2.41 belum dirilis.** Tag terakhir `v0.2.40` (GitHub release 14 Nov). Commit `9b781e4` ialah snapshot 0.2.41 (ikon baru + auto-update), tiada tag. Laman web masih `0.2.40` (`site/index.html:871`).
- Nota 0.2.41 sendiri kata: "0.2.40 can't look for updates yet, so install this one by hand". Pengguna 0.2.40 tak akan tahu ada 0.3.x kecuali dengan pasang sendiri.
- Mac: `mac` workflow masih wujud, tapi `ANIMATION-PLAN` kata "CI Mac sedang paused". Popup Caps Lock/Wi-Fi tiada di Mac (`mac/app.rs` hanya ada Volume, Charging, LowBattery, Bluetooth).

### Design tree

```
Versi (0.3.0?) ──> tag, Cargo.lock, installer, site "New in X"
0.2.41 dirilis dulu? ──> siapa dapat auto-update ke 0.3.0
Platform (Windows sahaja?) ──> item 1 (Mac weather), item 2 (popup baru di Mac), item 3
```

### Frontier

- **Q0.1 Versi `0.3.0`, bukan `0.3.00`.** Cargo menolak `0.3.00`, dan tag jadi `v0.3.0`. Teks pemasaran boleh tulis "0.3". **Cadangan: ya, 0.3.0.** Sebab: build gagal dengan `0.3.00`.
- **Q0.2 Rilis 0.2.41 dahulu (sebagai rilis kecil), baru kerja 0.3.0?** **Cadangan: ya.** Sebab: auto-update hanya ada dari 0.2.41. Lebih awal pengguna dapat 0.2.41, lebih ramai yang terima 0.3.0 sendiri.
- **Q0.3 Skop 0.3.0 Windows sahaja, Mac tunggu CI Mac hidup semula?** **Cadangan: ya.** Sebab: popup Caps/Wi-Fi pun belum ada di Mac, dan tiada cara untuk menguji Mac sekarang.

---

## 1. Weather widget tak berfungsi

### Fakta

- Pembacaan: Open-Meteo, tanpa key (`halo-core/src/halo.rs:82` `weather_url`). **Diuji 15 Nov dari Linux: HTTP 200, `temperature_2m 27.3`.** Fallback IP: ipwho.is (`halo.rs:104`), **diuji: OK, "Hulu Langat"**. Kedua-dua provider hidup.
- Weather hanya dibaca **semasa tab Widgets dibuka** (`app_halo.rs:163` `widgets_shown`, dipanggil dari `app.rs:3711-3719`). Bila tab tak pernah dibuka, tile tunjuk "Opens with the tab" (`halo.rs:137`). Ini sengaja (AGENTS.md: tiada network semasa rehat).
- Rantai tempat (`state.rs:1039` `weather_place`): city pilih sendiri, kemudian lokasi Windows (`weather_at`), tempat Prayer, dan akhir sekali IP. Bila Windows `Denied`/`Unknown`, IP ditanya sekali satu run (`app_halo.rs:181`).
- Fix 0.2.40 (`55beab4`) **dah ada dalam rilis v0.2.40**. Jadi aduan pemilik datang dari build yang sudah ada fix itu.
- **Bug pasti #1: Mac tak pernah baca weather.** `mac/app.rs` tiada `widgets_shown`, `weather::fetch` atau `on_weather` (grep kosong). Tab Widgets di Mac kekal "Opens with the tab".
- **Bug pasti #2: self-test bocor ke network.** `app.rs:1916` ambil `self.selftest.take()` semasa `step()`, jadi guard `self.selftest.is_some()` di `app_halo.rs:164` jadi false sepanjang step. Bukti: `out-halo-loop.log:5704-5706`, tiga kali `weather: no reading (Unreachable)` di dalam self-test bawah Wine.
- Semua kegagalan rangkaian yang bukan offline atau timeout jadi `Unreachable` = "Can't reach it" (`services/net.rs:23`, `halo-core/src/net.rs:74`). DNS, TLS dan proxy tak dapat dibezakan.
- Retry hanya semasa tab terbuka (`pages/widgets.rs:31` `next_wake`). Tile hanya dikemas kini bila tab dibuka. Tiada cara lain untuk lihat weather (tiada di island tertutup).

### Design tree

```
Simptom sebenar di PC pemilik (fakta milik pemilik)
 ├─ "Opens with the tab" ──> salah faham: widget tak baca sendiri ──> ubah UX?
 ├─ "Reading…" lama ──> Geolocator tersangkut (30 s) ──> pendekkan / terus IP
 ├─ "Can't reach it" ──> WinRT HttpClient gagal (proxy/TLS/AV) ──> log HRESULT sebenar
 ├─ "Location off · click to pick a city" ──> berfungsi seperti direka, UX tak jelas
 └─ nombor salah/lama ──> tempat salah (IP) ──> paparkan tempat lebih jelas
Mac ──> wiring sama (bergantung Q0.3)
```

### Frontier

- **Q1.1 Apa tepatnya tile Weather tunjuk di PC pemilik, dan boleh hantar baris `weather:` dan `location:` dari `%LOCALAPPDATA%\Halo\halo.log`?** **Cadangan: pemilik hantar screenshot tile + 20 baris log itu.** Sebab: kedua-dua provider hidup dari sini. Tanpa simptom sebenar, kita akan "baiki" benda yang salah. Ini satu-satunya fakta yang hanya ada di PC pemilik.
- **Q1.2 Log `HRESULT` sebenar untuk setiap kegagalan weather (bukan hanya `Unreachable`)?** **Cadangan: ya.** Sebab: kosnya satu baris log, dan ia membezakan DNS, TLS dan proxy pada laporan seterusnya.
- **Q1.3 Baiki kebocoran self-test (guard selftest semasa `step()`)?** **Cadangan: ya.** Sebab: CI tak sepatutnya memanggil network sebenar. Tanpa fix ini, ujian weather di CI tak deterministik.
- **Q1.4 Kekalkan peraturan "baca hanya bila tab Widgets dibuka"?** **Cadangan: ya.** Sebab: AGENTS.md "nothing runs at rest". Kalau pemilik mahu weather di island tertutup, itu ciri baru dan bukan "baiki".

### Terbuka (round 2, bergantung Q1.1)

- Fix sebenar ikut simptom: Geolocator timeout 30 s → 10 s? terus ke IP bila desktop-app location off? teks tile?
- Mac weather wiring (bergantung Q0.3).

---

## 2. Popup OSD: semua free, Wi-Fi trigger, 7 popup baru

### Fakta (sedia ada)

- `HudKind` (`state.rs:107`): Volume, Charging, Bluetooth, LowBattery, CapsLock, Wifi. **Tiada Pro gating pada mana-mana popup.** "Semua free" sudah benar hari ini, jadi cukup pastikan popup baru juga tiada gating.
- Queue (`halo-core/src/popups.rs`): satu demi satu, dedup ikut `same_slot`, `MAX = 3` menunggu (yang paling lama dibuang).
- **Wi-Fi trigger WUJUD dan dah dirilis dalam v0.2.40** (`e66d0a3`). `services/network.rs` mendengar `NetworkStatusChanged`, settle 700 ms, kemudian `wifi_change` (`popups.rs:105`), `AppEvent::Wifi` dan `app.rs:2537`. Dimulakan dalam `services/mod.rs:92`. Self-test scene `wifi-hud` melukisnya.
- Bila popup Wi-Fi **tak akan muncul** (ikut reka bentuk code):
  1. **Session locked atau display off** (`app.rs:2539`). Laptop yang bangun dari sleep sambung Wi-Fi semasa skrin lock, jadi popup dibuang. Ini kes paling biasa.
  2. **Island sedang terbuka** (`app.rs:3455` `target_mode` pulangkan `Expanded`, jadi HUD diabaikan dan tamat masa dalam 2.6 s).
  3. **App full-screen** menyorok island.
  4. **Wi-Fi sudah bersambung sebelum Halo mula** (`network.rs:44` "Where it stands at start is not news").
  5. **Ethernet**: hanya profil WLAN dikira (`network.rs:111`).
  6. Profil dengan `NetworkConnectivityLevel::None` dilangkau. Sambungan yang masih "Identifying…" selepas 700 ms boleh terlepas kalau Windows tak hantar event susulan.
  7. Suis `hud_wifi` off.
- Num/Scroll Lock: laluan raw input Caps (`app.rs:3522-3560`, `popups.rs:124` `CapsLock`) boleh terus diluaskan. `VK_NUMLOCK = 0x90`, `VK_SCROLL = 0x91` (windows 0.58). Kos baru: tiada (keyboard raw input sudah didaftar bila `hud_caps` on).
- Headphone: `services/audio.rs:140` `IMMNotificationClient`. `OnDeviceStateChanged`/`OnDeviceAdded`/`OnDeviceRemoved` sekarang **no-op**, dan hanya `OnDefaultDeviceChanged` bangunkan worker. Event sedia ada boleh dipakai tanpa thread baru. Mac sudah ada logik `switched` untuk elak popup Volume bertindih dengan popup headphone (`mac/app.rs:1632`). Windows tiada.
- Monitor: `WM_DISPLAYCHANGE` sudah ditangkap (`app.rs:1437`, `refresh_monitor`), tetapi ia juga berlaku semasa tukar resolusi/DPI. Perlu banding bilangan monitor.
- Mic in-use: tiada API "app guna mic" yang rasmi. Pilihan: (a) `IAudioSessionManager2` pada endpoint capture (rasmi, berasaskan event); (b) registry `CapabilityAccessManager\ConsentStore\microphone` + `RegNotifyChangeKeyValue` (tak didokumenkan, boleh pecah).
- Camera in-use: (a) registry ConsentStore `webcam` (tak didokumenkan); (b) Media Foundation sensor activity monitor (rasmi, Windows 11 sahaja).
- Focus/DND: `Windows.UI.Shell.FocusSessionManager` (`IsFocusActiveChanged`, `IsSupported`) ada dalam windows 0.58, tetapi feature `UI_Shell` **tak dihidupkan** dalam `crates/halo/Cargo.toml`. Ini Focus session Windows 11 (Clock app). **"Do not disturb" notifikasi tiada API awam** (hanya WNF tak didokumenkan). **Nama bertembung:** Halo sendiri ada tab "Focus" (Pomodoro).
- Airplane mode: tiada API "airplane mode" terus. `Windows.Devices.Radios` (`Radio.StateChanged`) ada dalam windows 0.58, tetapi feature `Devices_Radios` **tak dihidupkan**. Airplane ON akan juga mencetuskan Wi-Fi disconnected + Bluetooth disconnected, jadi 3 popup serentak.
- Settings: setiap popup ada suis `hud_*` (`settings.rs:115-122`), baris di `settings_window.rs:605-611`, `INVENTORY` (`:806-810`) dan `docs/advanced-settings.md`. Ada test yang mengikat ketiga-tiganya.

### Design tree

```
Wi-Fi "tak tunjuk" ──> senario sebenar (fakta milik pemilik) ──> ubah peraturan lock/expanded?
Popup baru
 ├─ Num/Scroll ──> (tiada keputusan tinggi)
 ├─ Headphone ──> wired sahaja atau termasuk BT? ──> dedup dengan Bluetooth + Volume
 ├─ Monitor ──> connect/disconnect sahaja (bukan resolusi)
 ├─ Mic/Camera ──> API rasmi vs registry ──> popup sekali vs penunjuk berterusan
 ├─ Focus/DND ──> Windows Focus session sahaja? ──> label (bertembung dengan tab Focus)
 └─ Airplane ──> tekan cascade Wi-Fi/BT?
Default on/off setiap jenis ──> Settings, INVENTORY, docs
Queue MAX=3 ──> cukup untuk dock unplug (monitor + headphone + Wi-Fi + ...)?
```

### Frontier

- **Q2.1 Dalam situasi apa pemilik sambung Wi-Fi dan popup tak keluar?** (bangun dari sleep / island terbuka / full-screen / PC guna Ethernet / sambung selepas Halo dibuka dengan skrin terbuka). **Cadangan: pemilik pilih satu. Kalau "bangun dari sleep", tunjuk popup Wi-Fi sekali selepas unlock jika ia bersambung semasa lock.** Sebab: code sudah ada trigger, dan setiap senario di atas perlukan fix yang berbeza.
- **Q2.2 Popup yang berlaku semasa island terbuka: kekal diabaikan?** **Cadangan: ya, kekal.** Sebab: orang sedang tengok page itu, dan popup akan melompatkan layout.
- **Q2.3 Mic dan camera: popup sekali bila mula dan tamat (bukan penunjuk berterusan di island)?** **Cadangan: ya, popup sahaja.** Sebab: Windows 11 sudah ada ikon mic/camera di taskbar, dan penunjuk berterusan ialah state baru pada island.
- **Q2.4 Mic guna API rasmi `IAudioSessionManager2`; camera guna MF sensor activity monitor (Windows 11 sahaja, Windows 10 tiada popup camera)?** **Cadangan: ya. Elak registry ConsentStore yang tak didokumenkan.** Sebab: registry boleh berubah tanpa notis dan nampak seperti "mengintip" app lain.
- **Q2.5 "Focus/DND" = Windows 11 Focus session (`FocusSessionManager`) sahaja, dengan label "Windows Focus"; DND notifikasi digugurkan?** **Cadangan: ya.** Sebab: DND tiada API awam, dan label "Focus" sahaja keliru dengan tab Focus Halo.
- **Q2.6 Airplane ON/OFF menekan popup Wi-Fi dan Bluetooth yang datang dalam ~3 s selepasnya?** **Cadangan: ya.** Sebab: satu tindakan sepatutnya satu popup, bukan tiga.
- **Q2.7 Headphone = jack berwayar (endpoint state berubah) sahaja; headphone Bluetooth kekal popup Bluetooth sedia ada?** **Cadangan: ya.** Sebab: elak dua popup untuk satu headset; Bluetooth popup sudah tunjuk bateri.
- **Q2.8 Monitor: popup hanya bila bilangan monitor berubah (bukan resolusi/DPI)?** **Cadangan: ya.** Sebab: `WM_DISPLAYCHANGE` juga berlaku semasa main game dan tukar resolusi.
- **Q2.9 Default: Num/Scroll, headphone, monitor, airplane ON; mic, camera, Windows Focus OFF?** **Cadangan: ya.** Sebab: yang pertama sama sifatnya dengan Caps (pengguna sendiri tekan). Mic/camera/Focus lebih kerap dan lebih peribadi, jadi pengguna pilih sendiri.
- **Q2.10 Satu suis per jenis dalam Advanced › Pop-ups (bukan satu suis "lain-lain")?** **Cadangan: ya.** Sebab: pola sedia ada, dan test INVENTORY sudah mengikatnya.

### Terbuka (round 2)

- Kos saiz: feature `UI_Shell`, `Devices_Radios`, Media Foundation. Ukur dengan `nm --size-sort` (AGENTS.md) selepas Q2.4/Q2.5 dijawab.
- Ikon baru (`icons.rs` tiada ikon NumLock, Monitor, Airplane; Mic, Camera dan Headphones sudah ada).
- Queue `MAX = 3` cukup atau tidak, selepas Q2.6 (cascade).
- Lebar popup `toggle_w` untuk label lebih panjang ("Scroll Lock", nama monitor).

---

## 3. Animasi menyeluruh

### Fakta

- **Plan bercanggah dengan skop FINAL.** `ANIMATION-PLAN-0.3.00.md` §2 kata "Tiga item cukup. Tak perlu cari item ke-4" dan §3 kata "Pop-up jenis baru: tambah bila ada permintaan khusus". Skop FINAL item 3 kata "semak semua bahagian, buat animasi untuk tempat yang boleh", dan item 2 sudah minta 7 popup baru.
- Video rujukan (`DfwFscEiVeB`) **tak dapat ditonton dari sini** (halaman JS; tiada alat video). Release notes rujukan hanya ada checksum, tiada senarai ciri. Laman rujukan (CSS) tunjuk: `--spring: cubic-bezier(0.34,1.32,0.5,1)` pada transisi saiz notch (0.62 s), lirik `ol` slide dengan spring, equalizer `bars` (loop tanpa henti), cincin Pomodoro dengan `pglow` radial merah, dan `animation: blink 4.2s infinite`.
- Senarai rujukan dalam backlog vs Halo hari ini:
  - mascot doze dan wave: **sudah ada** (`mascot.rs` Pose Doze/Sleep/Wave).
  - lirik line-by-line: **sudah ada** (`pill/lyric.rs`).
  - Glass/floating pill: **sudah ada** (tema Glass, Pill).
  - status ring bateri/air keliling jam: **pernah dibuang dengan sengaja**. `activities.rs` komen `clock_badge`: "no ring: it read as a spinner".
  - Pomodoro "breathing": **belum ada**. Timer sekarang redraw sekali sesaat sahaja (`next_second_boundary`). Breathing = frame setiap refresh selama 25 minit, jadi bercanggah dengan AGENTS.md "Motion costs a frame every refresh".
- Sedia ada yang boleh dipakai semula: `Spring` (`halo-core/src/spring.rs`), `media::glow` (`pill/media.rs:377`, `fn` peribadi), animasi senarai Tasks (`pages/tasks.rs:54` `ListAnim`), animasi Shelf (`pages/shelf.rs:347`).
- Volume bar `cur + (target-cur)*0.32` per frame **di dua tempat**: `pill/hud.rs:122` dan `activities.rs:919`.
- Reduced motion (`app.rs:3697`) hanya snap morph HUD. Battery saver **belum** menjejaskan animasi HUD (`st.power.saver` hanya dipakai untuk lirik dan sysmon).

### Design tree

```
Takrif "menyeluruh" ──> senarai bahagian ──> setiap satu: kos frame?
 ├─ Popup family (pop/settle/glow) ──> termasuk 7 popup baru (item 2)
 ├─ Rujukan video ──> perlukan penerangan pemilik
 ├─ Breathing Pomodoro ──> langgar AGENTS.md? ──> versi terhad?
 └─ Status ring ──> pernah dibuang ──> buka semula?
Peraturan gerak global: reduced motion, battery saver, locked/display off
```

### Frontier

- **Q3.1 "Menyeluruh" = senarai yang ditetapkan dalam GRILL ini (bukan "setiap tempat yang boleh")?** **Cadangan: ya. Senarai: (1) keluarga popup (6 lama + 7 baru); (2) glow notice tiba; (3) volume bar spring; (4) tab row/selection pill spring di header; (5) suis (knob) spring dalam Settings page; (6) tile Widgets fade-in nilai baru.** Semuanya sekali jalan, kemudian berhenti. Sebab: "setiap tempat" tiada titik siap, dan setiap gerak ada kos frame.
- **Q3.2 Pemilik terangkan 2-3 momen dari video Threads yang paling dia mahu (atau hantar 3 screenshot)?** **Cadangan: ya.** Sebab: video tak dapat dibuka dari sini. Meneka isinya bermakna membina benda yang salah.
- **Q3.3 Tolak "breathing" Pomodoro berterusan; ganti dengan satu pulse di setiap peralihan fasa (focus ke break)?** **Cadangan: ya.** Sebab: breathing selama 25 minit bermakna frame setiap refresh sepanjang masa itu (AGENTS.md).
- **Q3.4 Status ring bateri/air keliling jam kekal ditolak?** **Cadangan: ya.** Sebab: pernah dibuang kerana "read as a spinner".
- **Q3.5 Battery saver langkau pop/glow (terus ke state akhir) untuk semua animasi baru, sama seperti reduced motion?** **Cadangan: ya.** Sebab: AGENTS.md "rests when battery saver is on", dan plan §4 dah cadangkan.
- **Q3.6 Kemas kini `ANIMATION-PLAN-0.3.00.md` dahulu supaya selari dengan skop FINAL (buang §3 "pop-up baru tak perlu")?** **Cadangan: ya.** Sebab: dua dokumen yang bercanggah akan disalah baca oleh agent seterusnya.

### Terbuka (round 2)

- Spring dan timing untuk setiap item yang Q3.1 terima.
- Subjek pop untuk setiap popup baru (contoh: Mic on = pop + glow merah? Monitor = settle?), selepas Q2.x.

---

## 4. Buang menu "Every setting"

### Fakta

- Ia section dalam sidebar Advanced settings: `Section::Every` (`settings_window.rs:162,181,199,219,238,258`), `Block::Inventory`, `draw_inventory` (`:918`), `stands()`/`Stands` (`:752-900`), `INVENTORY` (`:782`).
- Test `the_inventory_is_every_row_the_window_draws` (`:2397-2457`) mengikat `INVENTORY` dengan baris yang benar-benar dilukis. `docs/advanced-settings.md` "is the same list".
- Mac: `mac/render.rs:554` guna `Section::ALL`.
- Laman web: `site/index.html:1932` (sheet "Everything new" di bawah 0.2.40) dan kata kunci carian `:772`. Nota rilis v0.2.40 (sudah terbit) memaut ke `docs/advanced-settings.md`.

### Design tree

```
Buang section (UI) ──> buang draw_inventory, stands, Stands
 └─ INVENTORY + test + docs/advanced-settings.md: buang atau simpan sebagai pengawal docs?
Laman web: baris sejarah 0.2.40 kekal?
```

### Frontier

- **Q4.1 "Every setting" = section sidebar "Every setting" dalam Advanced settings (bukan baris "Advanced settings" di Settings page)?** **Cadangan: ya.** Sebab: itu satu-satunya UI yang bernama begitu.
- **Q4.2 Buang section, `draw_inventory`, `stands`/`Stands`; simpan `INVENTORY` + test (hanya `#[cfg(test)]`) supaya `docs/advanced-settings.md` kekal tepat?** **Cadangan: ya.** Sebab: nota rilis 0.2.40 yang dah terbit memaut ke doc itu, dan test jadi pengawal percuma.
- **Q4.3 Baris sejarah "Every setting, in one list" di bawah "New in 0.2.40" pada laman web kekal (ia sejarah), tapi buang kata kunci carian?** **Cadangan: ya.** Sebab: sejarah tak berubah, tetapi carian tak patut membawa orang ke ciri yang dah tiada.

---

## 5. Menu FPS dalam Advanced settings

### Fakta

- "Show FPS, GPU and CPU over games" **sudah ada** dalam Advanced › Island, **tetapi hanya dilukis bila Pro** (`settings_window.rs:572-575` `if s.pro`). Pengguna Free tak nampak langsung dalam Advanced.
- Tray tunjuk item yang sama kepada semua orang, dengan label "(Halo Pro)" (`tray.rs:146`). Pengguna Free yang klik akan dimatikan semula oleh `pro_gate` (`halo-core/src/settings.rs:470`).
- Tiada tetapan "had FPS island" atau "FPS counter island". Island memang dilukis ikut refresh rate paparan (`frametime.rs` `VsyncTimeline`, `ambient_divisor`).

### Design tree

```
Maksud "menu FPS"
 ├─ (a) overlay game sedia ada: tunjuk kepada semua dalam Advanced ──> Free atau Pro?
 ├─ (b) had FPS animasi island (30/60/Max) ──> setting baru, kesan pada spring
 └─ (c) FPS counter island sendiri (debug)
Bergantung: item 6 (tray buang item FPS?)
```

### Frontier

- **Q5.1 "Menu FPS" = overlay "Show FPS, GPU and CPU over games" sedia ada (pilihan a)?** **Cadangan: ya.** Sebab: ia sudah wujud tapi tersorok daripada Free dalam Advanced, dan hanya tray yang menunjukkannya kepada semua.
- **Q5.2 Overlay FPS kekal Halo Pro (Free nampak baris berkunci "Needs Halo Pro" yang membuka page Halo Pro)?** **Cadangan: ya, kekal Pro, tapi barisnya sentiasa kelihatan.** Sebab: "semua free" dalam skop hanya untuk popup. Membuka Pro di sini keputusan harga, bukan pembetulan.

### Terbuka (round 2)

- Kalau Q5.1 = (b): nilai pilihan dan sama ada ia mengehadkan morph (spring guna `dt`, jadi selamat), atau hanya ambient.

---

## 6. Kurangkan menu tray (hidden icon)

### Fakta

- **Menu yang sama dipakai untuk klik kanan tray DAN klik kanan island** (`app.rs:354-362` `Deferred::Menu` panggil `tray::menu_items`). Memotong tray juga memotong menu island.
- README mengajar laluan klik kanan: `README.md:129` (Keep System page history), `:187` (In full-screen apps › Hide), `:195` (Move island to the centre, Idle look › Hidden until hover), `:223` (Theme › Halo).
- Isi sekarang (`tray.rs:113-195`) dan di mana lagi ia wujud:

| Item tray | Ada juga di |
|---|---|
| Update to… / Check for updates… | Settings page (Check now) |
| Record this screen (bersyarat) | Script page |
| Advanced settings… | (pintu masuk) |
| Keep island open | pin di header island |
| Move island to the centre (bersyarat) | seret ke tengah (snap) |
| Theme ›, Idle look › | Settings page + Advanced |
| Halo Mascot | Advanced › Appearance |
| Synced lyrics, Lyrics pronunciation, Sounds | Settings page + Advanced |
| Keep System page history (bersyarat) | Advanced › General |
| **Apple Music lyrics ›** | **TIADA tempat lain** |
| In full-screen apps › (+ FPS, peek) | Settings page + Advanced › Island |
| Start with Windows | Settings page + Advanced |
| Open data folder, What's New, Lyrics by LRCLIB | Advanced › About |
| About Halo Island x.y.z | — |
| Halo Pro › (8 baris) | Advanced › Halo Pro |
| Quit | — |

- Test yang mengikat susunan: `tray.rs:402-570` (≈10 test: update line, centre, mascot, system history, record, theme, full-screen submenu).
- Mac ada `mac/menubar.rs` sendiri (tak terjejas).

### Design tree

```
Senarai kekal ──> Apple Music lyrics pindah ke mana?
             ──> README/site copy "right-click ›" dikemas kini
Bergantung: Q5.1 (item FPS dalam submenu full-screen)
```

### Frontier

- **Q6.1 Menu jadi: [Update to… bila ada] · [Record this screen bila ada] · Advanced settings… · Keep island open · [Move island to the centre bila dipindah] · In full-screen apps › (Hide / Stay on top sahaja) · Halo Pro… (satu baris, bukan submenu 8 baris) · About Halo Island x.y.z · Check for updates… · Quit?** **Cadangan: ya.** Sebab: setiap yang dibuang ada di Settings page atau Advanced. Yang kekal ialah tindakan segera (pin, full-screen, centre) dan pintu masuk.
- **Q6.2 Apple Music lyrics (token dari clipboard) pindah ke Advanced › Music and lyrics sebelum dibuang dari tray?** **Cadangan: ya.** Sebab: ia satu-satunya item tanpa tempat lain. Membuangnya terus = ciri hilang.
- **Q6.3 Menu klik kanan island ikut menu tray yang dipotong (satu senarai, bukan dua)?** **Cadangan: ya.** Sebab: satu `menu_items` lebih kurang code. README dikemas kini untuk tunjuk laluan Settings.

---

## 7. Butang Settings (gear) tersorok

### Fakta

- `pages/mod.rs:504` `draw_header`: tab dilukis dari kiri (`TAB_W 30 + TAB_GAP 2`, tab aktif lebih lebar dengan label). Gear hanya dilukis **jika** `gear_r.x > tabs_end + 8.0` (`:603`), pin juga (`:589`). Date digugurkan dulu (`:573`). **Tab tidak dipotong**, jadi dengan banyak tab ia juga boleh bertindih jam.
- Anggaran pada lebar default 620 (`settings.rs:347`): lebar header ≈ 592-596 DIP, kluster kanan (jam + pin + gear + jarak) ≈ 150-200 DIP. **Gear hilang bila kira-kira ≥ 10-11 tab.** Free default ada 7 tab (Music, Widgets, Today, Shelf, Tasks, Focus, Add-ons), dan setiap add-on yang dihidupkan tambah satu. Maksimum 16 tab (`PageKind::ALL` tolak Settings).
- Settings **bukan tab** (`tabbed()` = false). Bila gear hilang, Settings page island **tiada jalan masuk** kecuali tray "Advanced settings…" (yang membuka window, bukan page).

### Design tree

```
Keutamaan ruang: gear > pin > jam > date > label tab aktif > lebar tab
 ├─ mampat tab (TAB_W menyusut) ──> had minimum boleh klik
 └─ overflow "…" untuk tab berlebihan ──> menu baru
```

### Frontier

- **Q7.1 Gear (dan pin) sentiasa ada ruang terpelihara di kanan; bila sesak, gugurkan date, kemudian label tab aktif, kemudian mampatkan lebar tab hingga 24 DIP?** **Cadangan: ya.** Sebab: kurang code berbanding menu overflow, dan semua tab kekal satu klik.
- **Q7.2 Kalau 16 tab pada lebar minimum (480) masih tak muat walaupun dimampatkan, tab paling hujung masuk butang "…" (overflow)?** **Cadangan: ya, tapi hanya sebagai jalan terakhir.** Sebab: 16 × 26 = 416 DIP tak muat dalam ≈ 300 DIP yang tinggal pada lebar 480.
- **Q7.3 "Banyak menu di atas notch" = banyak tab (add-on yang dihidupkan)?** **Cadangan: ya.** Sebab: itu satu-satunya benda yang menolak gear ke luar.

---

## 8. License: L1, L2, R1

### Fakta (paling penting dulu)

- **Aliran license-server belum hidup dalam build yang dirilis.** `crates/halo-core/data/license-keys.txt` **kosong** (0 key; "Empty until the maintainer runs init-key"), dan `HALO_LICENSE_URL` tiada dalam build atau CI (`services/license.rs:38`; grep `.github` kosong). Hasilnya: setiap token ditolak, dan Activate dengan kod jawab "This version has no license server set up". **L1 dan L2 tak boleh berlaku (dan tak boleh diuji) sehingga ini dihidupkan.** Portal 8790 hidup (HTTP 303 ke login).
- **L1:** `installer/halo.nsi:157` `RMDir /r "$LOCALAPPDATA\Halo"`, `installer/halo.iss:93` (check sahaja; rilis guna NSIS). `license.bin` dan `license-device.bin` ada dalam `cache_dir()` = `%LOCALAPPDATA%\Halo` (`paths.rs:118-125`). `%APPDATA%\Halo` (settings, data) **kekal** semasa uninstall. Mac sudah simpan dalam `data_dir` (`mac/paths.rs:88-94`), jadi L1 hanya Windows. Upgrade (`/S /reopen`, `services/update.rs:122`) tak menjalankan uninstaller.
- Server sudah menyokong "same install asks again" (200 + audit `reissue`, `license-server.mjs:236-242`). Jadi kalau install id kekal, kod lama boleh ditebus semula.
- **L2:** `services::license::status(false)` hardcoded di `app.rs:181` dan `mac/app.rs:569`. Server `POST /api/status` wujud (`license-server.mjs:248`). **Bahaya:** server jawab `revoked` untuk license **yang tak dikenali** (`:252`). Kalau `licenses.json` hilang atau rosak, setiap app yang bertanya akan jatuh ke Free. Backup belum diputuskan (`docs/licenses.md` "Backups").
- Keputusan lama: GRILL-0.2.40 §10 "No daily `/api/status` poll" (pemilik). L2 memerlukan keputusan itu diterbalikkan.
- Kitaran sedia ada yang boleh ditumpangi: update check (`TIMER_UPDATE`), ~2 minit selepas mula, kemudian setiap 30 minit sehingga berjaya, paling banyak sekali sehari berjaya, rehat semasa locked/display off/saver (nota v0.2.41).
- **R1:** server simpan hanya `SHA-256` kod (`codeHash`, `license-server.mjs:104,189`). Test `server/test/license.test.mjs:38` menegaskan "the code itself never reaches the disk". **Kod yang sudah dibuat tidak boleh dipaparkan semula, selamanya**, apa pun keputusan. Portal hanya tunjuk 8 aksara id (`:445`).

### Design tree

```
Hidupkan license-server (key + HALO_LICENSE_URL + host redeem awam) ──> prasyarat L1, L2 boleh diuji
L1: install id kekal? ──> token juga kekal? ──> migrasi dari LOCALAPPDATA
L2: terbalikkan "no /api/status"? ──> bila tanya ──> gagal rangkaian = ? ──> backup server
R1: simpan kod (encrypted) vs "kod baru" ──> kod lama tak boleh dipulihkan
```

### Frontier

- **Q8.1 Hidupkan license-server dalam 0.3.0 (jalankan `init-key`, commit public key, pilih host https untuk redeem API, masukkan `HALO_LICENSE_URL` ke build CI)?** **Cadangan: ya, sebagai langkah pertama item 8.** Sebab: tanpa ini L1/L2 tak bermakna dan tak boleh diuji. Pemilik perlu pilih host (keputusan deployment).
- **Q8.2 L1: pindah `license-device.bin` ke `%APPDATA%\Halo` (kekal semasa uninstall), dengan migrasi sekali dari lokasi lama semasa start?** **Cadangan: ya.** Sebab: satu path, dan server sudah reissue untuk install yang sama. PC baru atau Windows baru tetap dapat id baru (DPAPI ikat pada user).
- **Q8.3 L1: `license.bin` (token) juga dipindah ke `%APPDATA%\Halo`, supaya reinstall terus Pro tanpa taip kod?** **Cadangan: ya.** Sebab: kalau id kekal, token sudah sah untuk install itu, dan pengguna tak perlu cari kod lama (yang R1 tak boleh tunjuk). Ini terbalikkan nota backlog "token removed on uninstall", jadi perlu persetujuan pemilik.
- **Q8.4 L2: terbalikkan keputusan 0.2.40 dan tanya `/api/status` sekali sehari, menumpang kitaran update check yang sedia ada (tiada timer baru), hanya bila Pro datang dari license server?** **Cadangan: ya.** Sebab: kos rehat tetap sifar, kerana ia ikut timer yang sudah ada dan rehat semasa locked/saver.
- **Q8.5 L2: app hanya jatuh ke Free bila server jelas jawab `revoked` atau `expired`; gagal rangkaian/server = Pro kekal?** **Cadangan: ya.** Sebab: pengguna berbayar tak patut hilang Pro kerana Wi-Fi.
- **Q8.6 L2: server diubah supaya license tak dikenali jawab `unknown` (bukan `revoked`), dan app abaikan `unknown`?** **Cadangan: ya.** Sebab: `licenses.json` yang hilang tak sepatutnya menjadikan semua orang Free. `lic` ialah 128-bit rawak, jadi menebak tetap tak membocorkan apa-apa.
- **Q8.7 R1: terima bahawa kod lama tak boleh dipaparkan; untuk masa depan, tambah butang "New code" di portal (jana kod baru untuk license yang sama, kod lama mati, kod baru ditunjuk sekali), bukan simpan kod?** **Cadangan: ya.** Sebab: kes sebenar (pengguna hilang kod, reinstall) diselesaikan tanpa menyimpan rahsia baru. Test "never reaches the disk" kekal.

### Terbuka (round 2)

- Kalau Q8.7 = tidak (pemilik mahu "view" sebenar): kod disulitkan AES-256-GCM dengan key file berasingan (0600, seperti signing key), butang View dengan pengesahan semula + audit `view`. Kod lama tetap tak boleh dipulihkan.
- Kalau Q8.4 = ya: kemas kini `PRIVACY.md` (`lic` + hash device dihantar sekali sehari) dan `docs/licenses.md`.
- Kalau Q8.2 = ya: `tools/ci-installer.ps1` tambah check "uninstall keeps license-device.bin".
- Pengguna yang **sudah** uninstall di 0.2.x: id hilang. Perlukan butang portal "Release install" (lepaskan ikatan device)?

---

## Ringkasan risiko (cari masalah, bukan setuju)

1. **Item 1 mungkin bukan bug code di sisi Windows.** Provider hidup dan fix 0.2.40 sudah dirilis. Tanpa log pemilik, risiko "baiki" yang tak membaiki apa-apa tinggi. Mac pasti rosak.
2. **Item 2 "Wi-Fi trigger missing" juga bukan trigger yang hilang.** Trigger ada dan dirilis. Popup sengaja dibuang semasa lock, island terbuka dan full-screen.
3. **Item 3 dan plan bercanggah.** Plan kata 3 item cukup, tapi skop kata "menyeluruh". Video rujukan tak dapat dilihat.
4. **Item 8 berdiri atas infrastruktur yang belum hidup.** Key kosong dan tiada URL. L2 juga boleh mematikan semua pengguna Pro jika data server hilang (`unknown` = `revoked`).
5. **Versi `0.3.00` gagal build.**
6. **Item 6 juga memotong menu klik kanan island** (satu senarai), dan README mengajar laluan yang akan hilang.
7. **Kos (AGENTS.md):** popup baru menambah feature `windows` (`UI_Shell`, `Devices_Radios`, Media Foundation) dan listener. Semuanya berasaskan event (tiada poll), tetapi saiz exe mesti diukur sebelum dan selepas.
