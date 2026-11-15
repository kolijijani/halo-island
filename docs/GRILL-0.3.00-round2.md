# Grill 0.2.50 (dulu "0.3.00"): round 2

Tarikh: 15 Nov 2026. Status: **Round 2, soalan frontier untuk pemilik.** Belum ada kod.

Sumber: `docs/GRILL-0.3.00.md` (round 1), `docs/GRILL-0.3.00-answers.md` (jawapan pemilik), `docs/backlog.md`, `docs/ANIMATION-PLAN-0.3.00.md`, codebase pada `d801213`, dan license server yang sedang jalan di PC ini.

Cara baca sama dengan round 1: **Fakta** (fail:baris, disemak), **Design tree**, **Frontier** (soalan yang boleh dijawab SEKARANG; "ya" = setuju dengan cadangan) dan **Terbuka** (round 3, bergantung pada jawapan di sini). Nombor soalan bersambung dari round 1, supaya tiada ID yang bertindih.

Status item selepas round 1:

| Item | Status |
|---|---|
| 0 Versi | Terbuka: 0.2.50, nota 0.2.41 yang tak pernah dirilis |
| 1 Weather | Menunggu log pemilik, satu soalan baru |
| 2 Popup | **Paling banyak terbuka**: semua keadaan, island terbuka, headphone BT |
| 3 Animasi | Subjek pop untuk popup baru, spring |
| 4 Every setting | **Selesai** (Q4.1-Q4.3) |
| 5 FPS | Satu soalan kecil |
| 6 Tray | Satu soalan kecil (README) |
| 7 Butang Settings | Dua soalan kecil |
| 8 License | Terbuka: hidupkan server, View kod, lifecycle |

---

## 0. Versi: 0.2.50

### Fakta

- **`0.2.50` sah untuk Cargo.** Diuji 15 Nov dalam crate kosong di `/tmp`: `version = "0.2.50"` diterima (`cargo metadata` pulangkan `0.2.50`), manakala `0.3.00` masih gagal dengan `invalid leading zero in patch version number`. "50" tiada leading zero, jadi tiada masalah.
- Perbandingan versi selamat: `halo_core::update::newer` banding nombor, bukan teks (`update.rs:170-184`, test `versions_compare_as_numbers`), jadi `0.2.50 > 0.2.41 > 0.2.40`. `tools/ci-site-check.sh:37` guna `sort -V`, yang juga betul.
- Tempat versi (semua masih `0.2.41`): `Cargo.toml:6`, `Cargo.lock:684,709,1570,1574` (4 crate), `.github/workflows/ci.yml:21` (`HALO_VERSION`), `crates/halo/res/halo.manifest:3` (`0.2.41.0`), `crates/halo/res/halo.rc:16`, `installer/halo.nsi:15,34`, `installer/halo.iss:6,14`.
- `docs/release-notes/v0.2.41.md` wujud, tetapi 0.2.41 tak pernah dirilis (tag terakhir `v0.2.40`). Laman web masih tunjuk `0.2.40` (`site/index.html:871`, band `#new` di `:890`).
- Pengguna 0.2.40 tak boleh semak update (nota v0.2.41:10). Jadi 0.2.50 mesti dipasang sendiri sekali, dan selepas itu auto-update berjalan.
- `mac.yml` sekarang hanya `workflow_dispatch` (`.github/workflows/mac.yml:13-19`), jadi tag tak membina Mac. Ini selaras dengan Q0.3 (Windows sahaja).

### Design tree

```
0.2.50 (sah) ──> semua fail versi + tag v0.2.50
nota 0.2.41 tak dirilis ──> gabung ke v0.2.50.md? ──> baris "Upgrading" pindah
nama dokumen *-0.3.00.md ──> tukar nama atau kekal
site "New in 0.2.50" ──> 5 kad: termasuk ciri 0.2.41 (auto-update)?
```

### Frontier

- **Q0.4 Tag `v0.2.50`, dan setiap fail dalam senarai Fakta ditukar serentak (manifest `0.2.50.0`)?** **Cadangan: ya.** Sebab: CI `cargo fetch --locked` gagal jika satu `Cargo.lock` tertinggal (AGENTS.md).
- **Q0.5 Isi `v0.2.41.md` (ikon baru, auto-update, baris "Upgrading: install by hand") digabung ke `v0.2.50.md`, dan `v0.2.41.md` dibuang?** **Cadangan: ya.** Sebab: tiada rilis 0.2.41, jadi nota itu tak akan diterbitkan. Baris "Upgrading" masih benar untuk pengguna 0.2.40.
- **Q0.6 Nama fail `GRILL-0.3.00*.md` dan `ANIMATION-PLAN-0.3.00.md` kekal, dengan satu baris di atas setiap satu: "Versi sebenar: 0.2.50"?** **Cadangan: ya.** Sebab: backlog dan dokumen jawapan memaut nama itu. Tukar nama hanya memecahkan pautan.
- **Q0.7 Band "New in 0.2.50": auto-update (ciri 0.2.41) dapat salah satu daripada lima kad; ikon baru bukan kad?** **Cadangan: ya.** Sebab: auto-update ialah perubahan yang orang akan perasan. Ikon tak mengubah screenshot laman (nota handoff 14 Nov).

---

## 1. Weather

### Fakta (baru)

- Pemilik lapor tile tunjuk **"Can't reach it"** = `Failure::Unreachable` (`halo-core/src/net.rs:38`).
- `Unreachable` datang dari tiga punca yang **tak dapat dibezakan dalam log**: (a) `FetchError::Network` (DNS, connect, TLS), (b) `FetchError::Other`, (c) `Client::new` sendiri gagal (`services/weather.rs:40,56` `map_err(|_| Failure::Unreachable)`). Log hanya cetak `weather: no reading (Unreachable)` (`weather.rs:23`).
- Butiran sebenar sudah ada dalam code, tapi dibuang: `http.rs:464` `network_detail` menukar kod WinINet (12007 server not found, 12175 secure connection failed, 12037-12045 certificate) kepada teks. `services/net.rs:23` `failure()` hanya simpan `Timeout` atau tidak.
- Provider hidup dari sini (round 1). Jadi punca yang paling mungkin ada pada laluan rangkaian PC pemilik (proxy, antivirus yang memintas TLS, DNS) atau pada `HttpClient` WinRT.

### Frontier

- **Q1.5 Pada PC yang sama, pemilik buka dua URL ini dalam Edge dan beritahu sama ada keluar teks JSON: `https://api.open-meteo.com/v1/forecast?latitude=3.1&longitude=101.7&current=temperature_2m` dan `https://ipwho.is/`?** **Cadangan: ya, bersama log `weather:` yang dijanjikan.** Sebab: kalau Edge pun gagal, puncanya rangkaian atau antivirus, bukan Halo. Kalau Edge berjaya, bug ada dalam laluan `HttpClient` Halo. Fakta ini hanya ada di PC pemilik.
- **Q1.6 Butiran kegagalan (contoh "secure connection failed") hanya masuk log, tile kekal "Can't reach it"?** **Cadangan: ya.** Sebab: tile kecil, dan orang biasa tak boleh berbuat apa-apa dengan "12175". Log sudah cukup untuk laporan.

### Terbuka (round 3, bergantung Q1.5 + log)

- Fix sebenar: proxy (`HttpBaseProtocolFilter`), fallback, atau teks tile.

---

## 2. Popup dalam SEMUA keadaan

Pemilik terbalikkan reka bentuk asal: popup mesti keluar dalam semua situasi (Q2.1), termasuk semasa island terbuka (Q2.2). Headphone termasuk Bluetooth (Q2.7).

### 2A. Di mana popup hilang hari ini

#### Fakta

- `target_mode` (`app.rs:3454-3459`): bila `self.expanded`, island ialah `Mode::Expanded`, dan `st.hud` tak dilukis langsung. `hud_tick` (`app.rs:3574`, `state.rs:1220-1230`) tetap menamatkan popup ikut masa dan mengambil popup seterusnya dari queue. Hasilnya, popup yang datang semasa island terbuka **habis tanpa dilihat**, termasuk yang menunggu dalam queue.
- **Notice menang atas popup.** `activities.rs:205-207` `collapsed_mode`: bila ada notice (yang bukan jenis "stays"), mode ialah `Notice`, jadi popup di belakangnya tamat tanpa dilihat. Ini punca ke-8 yang round 1 tak senaraikan.
- **Locked / display off:**
  - Wi-Fi dibuang di sumber (`app.rs:2539`), Caps juga (`app.rs:3558`).
  - Bluetooth **tak** ditapis (`app.rs:2532-2535`). Ia masuk `st.hud` dan tamat tanpa dilihat.
  - `Queue::clear` wujud (`popups.rs:87`) tapi **tiada siapa memanggilnya**.
  - Skrin lock Windows ialah desktop berasingan (secure desktop), jadi window topmost Halo memang tak kelihatan di situ. "Papar semasa lock" secara fizikal mustahil. Yang paling hampir: papar selepas unlock.
- **Full-screen:** `apply_fullscreen` buat `ShowWindow(SW_HIDE)` (`app.rs:2937-2960`). Popup tamat tanpa dilihat. Mekanisme "peek" sudah ada untuk Show over games: `peek_game` (`app.rs:2977-2991`) tunjuk island selama `PEEK_S = 5.0` s (`app.rs:74`) dengan `TIMER_PEEK`, kemudian sorok semula. Mod presentation dikesan berasingan (`services/fullscreen.rs:43` `QUNS_PRESENTATION_MODE`).
- **Volume memintas queue.** `show_hud` (`state.rs:1197`) terus menimpa `st.hud` (`app.rs:2518`). Popup Wi-Fi/BT yang sedang dipaparkan **hilang** dan tak kembali.
- **Caps Lock boleh lari dari keadaan sebenar selepas unlock.** `raw_key` (`app.rs:3546-3560`) flip `self.caps` untuk setiap tekanan yang sampai. Tekanan di skrin lock (secure desktop) berkemungkinan besar tak sampai ke raw input Halo, jadi selepas unlock popup boleh kata "On" padahal Caps Off. `caps_lock_on()` hanya dibaca bila mula mendengar (`app.rs:3529`). *Perlu disahkan pada Windows sebenar; CI tak boleh tekan kekunci.*

#### Design tree

```
Peraturan umum: masa popup dikira bila ia DILIHAT, bukan bila ia datang
 ├─ Locked/display off ──> tahan, papar selepas unlock ──> buang yang basi (Caps, Volume)
 ├─ Notice di skrin ──> popup tunggu di belakang notice
 ├─ Island terbuka ──> cara papar tanpa lompat layout (2B)
 ├─ Full-screen ──> peek island (mekanisme game peek) ──> kecuali presentation?
 └─ Volume menimpa ──> popup yang ditimpa kembali ke depan queue
```

#### Frontier

- **Q2.11 Satu peraturan: saat popup (1.6 s / 2.6 s) mula dikira hanya bila ia benar-benar kelihatan. Selagi tak kelihatan (locked, display off, notice di atas, full-screen tanpa peek), popup menunggu dalam queue (MAX 3, yang terbaru bagi setiap slot menggantikan yang lama)?** **Cadangan: ya.** Sebab: satu peraturan menyelesaikan lima punca "tak keluar" sekaligus, dan ia menggunakan semula `Queue` yang sedia ada (`same_slot` sudah simpan keadaan terakhir sahaja). Kos semasa rehat: tiada. Tiada timer baru, kerana `schedule_wake` sudah bangun pada `h.until` (`app.rs:4118`).
- **Q2.12 Locked/display off: popup ditahan dan dipaparkan selepas unlock, KECUALI Volume dan kekunci toggle (Caps/Num/Scroll), yang dibuang. Pada unlock, Caps Lock dibaca semula dari Windows (`GetKeyState`)?** **Cadangan: ya.** Sebab: "Connected · Home" selepas unlock masih berita. "Caps Lock On" yang ditaip di skrin lock bukan berita, dan mungkin salah (Fakta: tracker boleh lari). Satu baris re-seed menutup risiko itu.
- **Q2.13 Volume kekal terus (tanpa queue, sebab kekunci ditekan lama), tetapi popup yang ditimpanya kembali ke depan queue dan dipaparkan selepas bar volume hilang?** **Cadangan: ya.** Sebab: hari ini popup Wi-Fi/BT yang ditimpa hilang terus. Kosnya satu `push_front`.
- **Q2.14 Full-screen: popup "peek" island (window ditunjuk selama tempoh popup, kemudian disorok semula, guna mekanisme `peek_game`) dalam semua app full-screen dan game, KECUALI mod presentation (`QUNS_PRESENTATION_MODE`)?** **Cadangan: ya.** Sebab: Show over games sudah membuktikan island boleh terapung atas game. Semasa slide presentation di depan orang ramai, popup Wi-Fi/Caps di skrin projektor memalukan. Kalau pemilik mahu presentation juga, jawab "tidak, termasuk presentation".

### 2B. Popup semasa island terbuka, tanpa melompatkan layout

#### Fakta

- Island terbuka dilukis oleh `pages::draw_header` (`pages/mod.rs:504-640`), yang dikongsi Classic dan Pill (`pill/expanded.rs:49-55`). Header: tab di kiri. Di kanan, dari tepi: jam (`:560-566`), date (`:568-578`, digugurkan bila tak muat), gear, pin, dan muka Halo (`:585-594`, hanya bila muat).
- Window dibentuk **sekali** untuk page paling tinggi (`app.rs:876-884` `layout_surface`). Island dilukis di tengah window, jadi morph tak menggerakkan window. Membesarkan island melebihi saiz itu memanggil `ensure_room` (`app.rs:1028-1041`), yang membesarkan surface.
- Hit test: di luar card ialah `HTNOWHERE`, dan touch ikut alpha hit mask (`window.rs:120-128`). Sebarang lukisan di luar card perlukan hit mask dan surface yang lebih besar.
- Anggaran ruang header pada lebar default 620 dengan 7 tab: tab berakhir sekitar 290-300 DIP. Kluster kanan (jam + gear + pin + jarak) ≈ 110-120 DIP. Ruang kosong antara tab dan pin ≈ 180-200 DIP (date ≈ 50 DIP termasuk dalamnya). Ruang ini mengecil dengan setiap tab add-on (item 7).
- Lukisan popup sedia ada dalam pill: capsule `H = 32` (`pill/hud.rs:19`), lebar 216-360 (`:20-25`), `toggle_w` 220-360 (`activities.rs:988-992`).

#### Pilihan teknikal

| | **A. Chip dalam header** (cadangan) | **B. Toast di atas page** | **C. Capsule di bawah card** |
|---|---|---|---|
| Apa | Selama popup, kawasan date + jam di header bertukar (cross-fade) kepada chip kecil: glyph + satu baris pendek ("Harbor Cafe · Connected", bar volume mini). Lepas tempoh, jam kembali. | Capsule popup yang sama (`pill::hud::draw`) dilukis sebagai layer di atas kandungan page, di tepi atas atau bawah card, fade masuk dan keluar. | Capsule kedua tergantung 8 DIP di bawah card yang terbuka, seperti island kedua. |
| Layout lompat? | Tidak. Geometri card tak berubah, page tak bergerak. | Tidak, tapi ia **menutup** baris page selama 1.6-2.6 s. | Tidak, card tak berubah. |
| Kos code | Kecil. Satu cabang dalam `draw_header` yang membaca `st.hud`. Tiada perubahan geometri, hit test atau surface. | Sederhana. Layer + kedudukan ikut tema, tapi mesti tak menelan klik (atau menelan klik dengan sengaja). | Besar. Dua body dalam satu morph, hit mask baru, surface lebih tinggi (`layout_surface_for` + ~40 DIP). |
| Kos runtime | Hanya content fade sedia ada. Tiada frame selepas fade. | Sama dengan A, ditambah satu layer semasa fade. | Surface GPU lebih besar sepanjang masa (≈ 620×40 DIP; ~220 KB pada 150%), jadi memori rehat naik sedikit. |
| Risiko | Ruang header sempit bila banyak tab (item 7). Perlu fallback glyph sahaja. Bersaing dengan muka Halo dan date. | Menutup butang di bawahnya. Klik ke butang tertutup sama ada hilang atau "tembus" ke butang yang tak kelihatan. | Touch/klik di bawah card mesti tembus. Lebih banyak cara untuk pecah (Classic notch, Glass, overshoot). |
| Kelihatan? | Sederhana: dekat jam, di mana mata sudah melihat. | Paling jelas. | Jelas, tetapi di luar island. |

Pilihan D, **tutup island untuk papar popup**, ditolak. Itulah "lompat layout" yang pemilik mahu elak.

#### Frontier

- **Q2.15 Popup semasa island terbuka = Pilihan A (chip dalam header, ganti date + jam selama tempoh popup, tanpa klik); bila ruang sempit, glyph sahaja?** **Cadangan: ya, A.** Sebab: kos paling kecil, tiada perubahan geometri/hit/surface, tiada kandungan page yang ditutup, dan satu tempat (`draw_header`) menampung semua tema. Peraturan Q2.11 terpakai: popup dikira "dilihat" bila chip dilukis.

### 2C. Wi-Fi: kes yang masih tertinggal

#### Fakta

- Mula app: keadaan semasa sengaja tak dikira berita (`network.rs:44`).
- Ethernet: hanya profil WLAN dikira (`network.rs:111-113`).
- Profil WLAN dengan `NetworkConnectivityLevel::None` dilangkau (`network.rs:115-118`). Sambungan yang masih "Identifying…" selepas `SETTLE = 700 ms` (`:25`) dikira "tak bersambung", dan join itu terlepas jika Windows tak hantar event susulan.

#### Frontier

- **Q2.16 Tiada popup Wi-Fi semasa Halo mula (Wi-Fi yang sudah bersambung bukan berita)?** **Cadangan: ya.** Sebab: setiap login akan tunjuk "Connected · Home". Itu bunyi bising, bukan berita.
- **Q2.17 Ethernet tiada popup ("Wi-Fi" bukan kabel)?** **Cadangan: ya.** Sebab: pemilik minta popup Wi-Fi. Popup "Wired network" ialah jenis baru, dan boleh ditambah kemudian jika diminta.
- **Q2.18 Profil WLAN yang bersambung dikira "joined" walaupun tahapnya masih `None` (popup "Connected · No internet"), diikuti satu bacaan semula 3 s kemudian yang mengemas kini popup di tempatnya jika internet sudah sampai (tanpa popup baru)?** **Cadangan: ya.** Sebab: menutup kes "Identifying…" tanpa poll. Bacaan semula berlaku sekali selepas join sahaja, kemudian berhenti.

### 2D. Headphone Bluetooth vs popup Bluetooth

#### Fakta

- `services/bluetooth.rs`:
  - Hanya peranti **audio** (headphone, earbud, speaker) atau yang tak beritahu jenisnya diberi popup (`:14-16`, `report` di `:663-666`).
  - Endpoint classic + LE dikumpul ikut **container id** (`:10-11`).
  - Perubahan mesti kekal 3 s (`SETTLE`, `:42`), ditambah lookup bateri sehingga 2.5 s (`:44`).
  - Popup sudah melukis **glyph Headphones** bila `audio` (`activities.rs:961`), lengkap dengan nama dan bateri.
- `same_slot` (`state.rs:122-127`): Bluetooth dikira slot yang sama hanya bila namanya sama. Popup headphone baru (jenis lain) dan popup Bluetooth **tak akan** dedup sesama sendiri dalam queue. Dua popup akan keluar.
- Laluan audio (`services/audio.rs:140-170`): `OnDeviceStateChanged`/`OnDeviceAdded`/`OnDeviceRemoved` no-op. Headset BT yang bersambung juga menjadi endpoint render/capture yang **ACTIVE** dalam Core Audio. Jadi popup headphone berasaskan endpoint akan tercetus untuk headset BT juga, serta-merta (tanpa 3 s).
- **Popup Volume tercetus hari ini bila headset BT bersambung.** `OnDefaultDeviceChanged` membuat `rebind` (`audio.rs:279-320`), yang membaca paras endpoint baru dan `publish` (`:81`). Paras yang berbeza menghasilkan `AppEvent::Volume` dan `show_hud(Volume)` (`app.rs:2514-2519`). Satu headset = popup Volume pada ~0 s, kemudian popup Bluetooth pada ~3-5.5 s. Mac sudah ada `switched` untuk menyekat ini (`mac/app.rs:1626-1633`). Windows tiada.
- Property untuk mengenal pasti endpoint BT ada dalam `windows` 0.58: `PKEY_Device_EnumeratorName` (BTHENUM / BTHLEDEVICE / BTHHFENUM) dan `PKEY_Device_ContainerId` (dalam `Win32_Devices_FunctionDiscovery`, yang **belum** dihidupkan dalam `crates/halo/Cargo.toml`: satu feature kecil, atau tulis GUID PROPERTYKEY itu sebagai pemalar sendiri), dan `PKEY_AudioEndpoint_FormFactor` (`Win32_Media_Audio`, sudah dihidupkan, `Cargo.toml:83`).

#### Pilihan

1. **Satu sumber bagi setiap jenis peranti (cadangan).** Peranti BT hanya dari `bluetooth.rs`. Laluan endpoint audio (popup headphone baru) **melangkau** endpoint yang enumerator-nya Bluetooth. Tiada tetingkap masa dan tiada race. Popup BT sudah bergaya headphone (glyph + bateri).
2. **Padanan container id dengan tetingkap masa.** Popup headphone keluar segera. Bila laporan BT dengan container yang sama tiba dalam ~6 s, ia menggantikan popup itu (slot sama). Risikonya race: BT settle 3 s + lookup 2.5 s melebihi tempoh popup headphone 2.6 s, jadi pengguna mungkin nampak dua popup berturut-turut.
3. **Endpoint sahaja, BT popup dibuang untuk audio.** Kehilangan bateri dan nama peranti BT. Ditolak.

#### Frontier

- **Q2.19 Dedup = Pilihan 1: headset Bluetooth hanya diumumkan oleh popup Bluetooth (glyph headphone, nama, bateri); popup headphone dari endpoint audio melangkau endpoint yang enumerator-nya BTHENUM / BTHLEDEVICE / BTHHFENUM?** **Cadangan: ya.** Sebab: satu headset = satu popup, tanpa tetingkap masa. Code BT yang sudah dirilis (settle, container, bateri) dikekalkan.
- **Q2.20 Headset Bluetooth dipaparkan bila suis Headphones ATAU suis Bluetooth dihidupkan (sebab "headphone termasuk BT")? Headphone berwayar hanya ikut suis Headphones.** **Cadangan: ya.** Sebab: pengguna yang matikan Bluetooth tapi hidupkan Headphones tetap nampak headset BT, seperti yang pemilik minta. Satu syarat `||`.
- **Q2.21 Port logik `switched` Mac ke Windows: bila peranti output default bertukar, laporan paras dari `rebind` tak menunjukkan popup Volume?** **Cadangan: ya.** Sebab: hari ini setiap sambungan headset BT menghasilkan popup Volume yang tak diminta, dan dengan popup headphone ia jadi dua. Satu bendera dalam `Gate`.
- **Q2.22 Popup headphone berwayar dicetuskan oleh `OnDeviceStateChanged` endpoint render/capture yang `FormFactor`-nya Headphones/Headset (tanpa thread baru, guna worker audio sedia ada)? Terima bahawa sesetengah jack (Realtek dengan "jack detection" dimatikan) tak melapor, jadi tiada popup?** **Cadangan: ya.** Sebab: itu satu-satunya event rasmi. Jack yang tak melapor ialah had driver, bukan bug Halo.

### 2E. Popup baru: butiran yang kini boleh diputuskan

#### Fakta

- Mic: `IAudioSessionManager2` memberi sesi beserta process id (`IAudioSessionControl2::GetProcessId`). Camera: laporan MF sensor activity juga ada process id. Halo sendiri merakam audio (Meeting notes, recorder), jadi sesi Halo akan dikira "mic in use".

#### Frontier

- **Q2.23 Popup mic/camera menamakan app ("Microphone · Zoom"), dan sesi Halo sendiri diabaikan?** **Cadangan: ya.** Sebab: "mic in use" tanpa nama app tak menjawab soalan pengguna ("siapa?"). Popup Halo untuk rakaman Halo sendiri hanya bising.

### Terbuka (round 3, bergantung jawapan item 2)

- Kandungan chip header (Q2.15): bar volume mini atau peratus sahaja? Lebar maksimum? Turutan gugur dengan muka Halo dan date (bergantung Q7.4).
- Kos saiz feature: `UI_Shell` (WinRT, untuk `FocusSessionManager`), `Devices_Radios`, `Win32_Media_MediaFoundation`, dan mungkin `Win32_Devices_FunctionDiscovery`. Ukur dengan `nm --size-sort` (AGENTS.md).
- `Radio.RequestAccessAsync` untuk app Win32 tak berpakej: perlu disahkan pada Windows sebelum airplane dijanjikan.
- Queue `MAX = 3` selepas peraturan Q2.11: dock unplug boleh hasilkan monitor + headphone + Wi-Fi + BT. Yang paling lama dibuang. Cukup?
- Ikon baru: NumLock, ScrollLock, Monitor, Airplane, Focus (Mic, Camera, Headphones sudah ada, `icons.rs:42,100,101`).

---

## 3. Animasi

### Fakta

- Q3.1 menerima senarai tetap (keluarga popup, glow notice, spring bar volume, spring pill tab, spring knob suis, fade tile Widgets). Q3.2 (video) masih menunggu pemilik.
- Pill tab sekarang guna `ease_out_cubic` lerp, bukan spring (`pages/mod.rs:509`).
- Plan §2.1 sudah menetapkan bahasa gerak: pop (`Spring::new(0.6, 520.0, 30.0)`), settle (`Spring::smooth`) dan glow 600 ms. Peraturan: "pop + glow bila sesuatu datang, settle tanpa glow bila pergi, statik bila amaran/berulang".
- Palette ada `ORANGE` dan `GREEN` (`geom.rs:185-186`).

### Frontier

- **Q3.7 Subjek gerak popup baru ikut peraturan plan:**

  | Popup | ON / datang | OFF / pergi | Warna glow |
  |---|---|---|---|
  | Num Lock, Scroll Lock | pop disc | settle | accent |
  | Headphone (berwayar) | pop disc (plug) | settle (unplug) | accent |
  | Monitor | pop (connect) | settle (disconnect) | accent |
  | Mic in use | pop | settle (tamat) | **oren** |
  | Camera in use | pop | settle (tamat) | **hijau** |
  | Windows Focus | pop (mula) | settle (tamat) | accent |
  | Airplane | pop (ON) | settle (OFF) | accent |

  **Cadangan: ya.** Sebab: satu keluarga, satu peraturan. Oren untuk mic dan hijau untuk camera ikut konvensi penunjuk privasi yang orang sudah kenal (iOS/macOS), jadi mic dan camera dibezakan bukan oleh warna sahaja, tapi juga oleh glyph.
- **Q3.8 Spring: popup ikut plan §4; pill tab dan knob suis guna `Spring::smooth` (tanpa overshoot, ganti `ease_out_cubic`); tile Widgets fade 200 ms sekali bila nilai baru tiba; semua berhenti bila settle (`is_settled_eps(0.005)`)?** **Cadangan: ya.** Sebab: satu set spring yang sudah ada dalam `halo-core/src/spring.rs`, tiada nilai baru untuk ditala. Kerana spring guna `dt`, rasanya sama di 60 Hz dan 144 Hz.

### Terbuka (round 3)

- Animasi chip header (Q2.15): cadangan awal fade sahaja (orang sedang membaca page), tanpa pop/glow. Bergantung Q2.15.
- Momen dari video rujukan (Q3.2), selepas pemilik terangkan.

---

## 4. Every setting

**Selesai.** Q4.1-Q4.3 semua diterima. Tiada soalan round 2.

---

## 5. FPS

### Fakta

- Baris overlay hanya dilukis bila Pro (`settings_window.rs:572-575`). Q6.1 membuang item FPS dari submenu full-screen tray (hanya Hide / Stay on top kekal), jadi Advanced menjadi satu-satunya tempat.

### Frontier

- **Q5.3 Bagi Free, baris "Show FPS, GPU and CPU over games" dilukis berkunci dengan lencana "Halo Pro"; klik membuka section Halo Pro dalam Advanced settings?** **Cadangan: ya.** Sebab: itu maksud "sentiasa kelihatan, berkunci" (Q5.2), dan pintu masuk Pro yang sedia ada sudah ada di situ.

---

## 6. Tray

### Frontier

- **Q6.4 Baris README yang mengajar laluan klik kanan (`README.md:129,187,195,223`) ditulis semula ke laluan Settings / Advanced dalam PR yang sama dengan potongan menu?** **Cadangan: ya.** Sebab: README yang mengajar menu yang sudah tiada ialah bug dokumentasi pada hari rilis.

---

## 7. Butang Settings

### Fakta

- Selain date, **muka Halo** juga mengambil ruang header (`pages/mod.rs:585-594`, `HALO_W`, hanya bila `halo_cx - HALO_W/2 > tabs_end + 12`). Round 1 tak senaraikannya dalam turutan gugur.

### Frontier

- **Q7.4 Turutan gugur bila sesak: muka Halo dulu, kemudian date, label tab aktif, dan akhir sekali mampatkan tab hingga 24 DIP (gear + pin tak pernah gugur)?** **Cadangan: ya.** Sebab: muka Halo hiasan, date ada dalam tab Today, dan label ada dalam tooltip.
- **Q7.5 Butang overflow "…" membuka menu native Windows (`TrackPopupMenu`, sama seperti tray), bukan menu yang dilukis sendiri?** **Cadangan: ya.** Sebab: code menu tray sudah ada, dan menu native mendapat keyboard, screen reader dan DPI secara percuma.

### Terbuka (round 3)

- Interaksi dengan chip popup (Q2.15): chip hanya mengambil kawasan date + jam, tak pernah ruang gear/pin. Bergantung Q2.15.

---

## 8. License

### 8A. Hidupkan license server

#### Fakta (baru, penting)

- **`HALO_LICENSE_URL` dibaca semasa RUNTIME, bukan semasa build.** `services/license.rs:40` `std::env::var("HALO_LICENSE_URL")`. `build.rs` tak menulisnya (`crates/halo/build.rs:32-34` hanya `HALO_BUILD_*`). Jadi "masukkan ke build CI" (Q8.1) **tak akan sampai ke pengguna**, kerana PC pengguna tiada pemboleh ubah itu. Bandingkan dengan feed update yang ditulis terus dalam code: `halo-core/src/update.rs:11` `FEED_URL`.
- License server **sudah jalan di PC ini**: `serve --key ~/.halo-license/keys/signing-key.pem --data ~/.halo-license/data --admin-host 100.99.255.105 --admin-port 8790 --redeem-host 127.0.0.1 --redeem-port 8791 --trust-proxy`. Redeem API hanya di loopback dengan `--trust-proxy`, jadi ia memang direka untuk duduk di belakang proxy https.
- Signing key **sudah wujud** (fail 0600), tetapi baris public-nya tak pernah masuk ke `crates/halo-core/data/license-keys.txt` (fail itu hanya komen). `init-key` enggan menulis ganti key yang wujud (`license-server.mjs:524`), jadi baris public mesti diterbitkan daripada PEM yang sedia ada.
- **Folder `~/.halo-license/keys` ialah `drwxrwxr-x` (775).** Fail key 0600, tetapi folder yang boleh ditulis group membolehkan ahli group menggantikan fail itu (rename). `readPrivateKey` hanya menyemak mod fail (`:513`).
- Data sekarang: 3 license (1 redeemed, 2 revoked).
- `v0.2.40` dirilis dengan `license-keys.txt` kosong (disemak `git show v0.2.40:…`), jadi **tiada build yang dirilis pernah menerima token**.

#### Frontier

- **Q8.11 Alamat redeem API ditulis sebagai pemalar dalam code (seperti `FEED_URL`), dengan `HALO_LICENSE_URL` kekal hanya sebagai override untuk ujian?** **Cadangan: ya.** Sebab: Fakta di atas. Tanpa ini, Activate dalam 0.2.50 tetap akan jawab "no license server set up" untuk setiap pengguna.
- **Q8.12 Host redeem awam = Tailscale Funnel pada PC ini (https → `127.0.0.1:8791`, yang sudah `--trust-proxy`)?** **Cadangan: ya.** Sebab: tiada infrastruktur baru, sijil TLS diurus Tailscale, dan admin portal kekal tailnet sahaja. Risiko: bila PC ini mati, Activate gagal ("couldn't ask"), tetapi Pro yang sudah aktif kekal (semakan offline, dan Q8.5 kata gagal rangkaian = Pro kekal). Alternatif: Cloudflare Tunnel atau VPS, jika uptime penting.
- **Q8.13 Baris public signing key yang sedia ada diterbitkan dari PEM dan di-commit ke `license-keys.txt`, dan folder `keys` ditukar ke 700?** **Cadangan: ya.** Sebab: key baru akan memaksa key lama dibuang tanpa sebab. Folder 775 ialah lubang kecil yang murah untuk ditutup.

### 8B. View kod penuh di portal (Q8.8)

Bercanggah dengan Q8.7 ("kod hanya SHA-256"). Q8.8 lebih baru, jadi ia menang untuk kod **baru**. Kod lama kekal tak boleh dipulihkan.

#### Fakta

- Kod: 100 bit rawak (`license-server.mjs:15`). Disimpan hanya sebagai `code_sha256` (`:189`), dan ditunjuk sekali (`:447-449`).
- Test `server/test/license.test.mjs:37` menegaskan "the code itself never reaches the disk", iaitu **teks biasa** kod. Ciphertext tak melanggar test itu, jadi ia kekal sah tanpa diubah.
- Kod yang sudah ditebus hanya berguna pada install yang sama: install lain dapat `409 already_used` (`:232`). Jadi kebocoran kod yang sudah ditebus hampir tak bernilai. **Risiko sebenar ialah kod yang belum ditebus** (sesiapa yang menebus dulu, dia dapat).
- Node `crypto` ada `aes-256-gcm` terbina dalam, jadi tiada dependency baru (server "no dependencies", `:4`).
- Pengesahan admin = satu admin token (`LICENSE_ADMIN_TOKEN`), dengan rate limit login 5 / 15 min (`:31-32`), CSRF dan audit (`:147-150`). Halaman sudah `Cache-Control: no-store` (`:51`).

#### Reka bentuk yang dicadangkan

```
init-code-key --key-out ~/.halo-license/keys/code-key   (32 bait rawak, 0600; enggan tulis ganti)
serve ... --code-key ~/.halo-license/keys/code-key      (wajib; enggan mula tanpanya)

makeKeys:  code_enc = { v:1, iv: 12 bait rawak, ct, tag }   AES-256-GCM
           AAD = "halo-license/1 code\n" + id    (ciphertext terikat pada barisnya; tukar baris = gagal)
           code_sha256 kekal (lookup redeem tak berubah)

Portal:    baris dengan code_enc  -> butang "View"
           baris tanpa code_enc   -> "Not stored (made before 0.2.50)" + butang "New code" (Q8.7)
POST /view {csrf, id, token}:  admin token ditaip SEMULA setiap kali (timingSafeEqual),
           kongsi limiter login (5 / 15 min), audit {action:'view', actor, id} — tiada kod dalam audit,
           jawapan no-store, kod ditunjuk pada halaman itu sahaja.
```

- **Risiko:**
  1. Kalau `licenses.json` **dan** `code-key` bocor bersama, semua kod yang belum ditebus boleh dicuri. Mitigasi: fail di folder berasingan dengan mod 700, dan backup berasingan (Q8.17).
  2. `code-key` hilang: View mati untuk semua kod, tetapi redeem tetap jalan (hash), jadi tiada pengguna terjejas.
  3. Sesi admin yang dicuri (cookie) tak cukup untuk View, kerana token mesti ditaip semula.
- **Migrasi:** 3 license sedia ada tiada `code_enc`, jadi ia papar "Not stored" dan boleh dapat "New code". Tiada penukaran data, dan fail lama kekal boleh dibaca (`code_enc` pilihan).
- **Alternatif yang ditimbang:** kod diterbitkan semula sebagai `HMAC(code-key, id)`, tanpa menyimpan apa-apa. Ia lebih sedikit data, tetapi key tak boleh diputar tanpa menukar setiap kod, dan "New code" perlukan kaunter generasi. AES-GCM membolehkan putaran key (enkripsi semula) dan memang diminta pemilik.

#### Frontier

- **Q8.14 View = AES-256-GCM dengan key file berasingan (`--code-key`, 32 bait, 0600, dibuat oleh `init-code-key`), AAD = id license, `code_sha256` kekal untuk redeem?** **Cadangan: ya.** Sebab: inilah yang pemilik minta. Tiada dependency, dan test "plaintext never reaches the disk" kekal benar.
- **Q8.15 Setiap View perlukan admin token ditaip semula, berkongsi rate limit login (5 / 15 min), dan diaudit sebagai `view` (id sahaja, tiada kod)?** **Cadangan: ya.** Sebab: View ialah tindakan paling sensitif di portal. Cookie sesi sahaja tak cukup.
- **Q8.16 `serve` enggan mula tanpa `--code-key` (bukan diam-diam tak simpan kod)?** **Cadangan: ya.** Sebab: kalau flag terlupa, kod yang dibuat hari itu tak akan boleh dilihat lagi, dan tiada siapa tahu sehingga terlambat.
- **Q8.17 Ciphertext kekal selepas kod ditebus (tak dipadam)?** **Cadangan: ya.** Sebab: pemilik mahu View kod yang dijana, dan kod yang ditebus hanya sah pada install yang sama (409 untuk yang lain), jadi nilai curinya rendah.
- **Q8.18 Backup: `licenses.json` + `audit.jsonl` dalam satu backup; `code-key` + `signing-key.pem` dalam backup lain (luar talian), tak pernah bersama?** **Cadangan: ya.** Sebab: backup data yang bocor tanpa key hanya ciphertext. Ini juga menjawab "Backups" yang belum diputuskan dalam `docs/licenses.md:81`.

### 8C. Lifecycle: update dan reinstall (Q8.9, Q8.10)

#### Fakta

- **Q8.9 (update kekal) sudah benar hari ini.** Update menjalankan `Setup /S /reopen` (`services/update.rs:122`). Seksyen install NSIS tak memadam `%LOCALAPPDATA%\Halo`. Hanya seksyen Uninstall berbuat begitu (`installer/halo.nsi:157`). Token juga tak dipadam bila key tak dikenali: `status()` hanya memadam untuk `Expired/Revoked/OtherInstall/BadSignature/Malformed` (`services/license.rs:90`), bukan `UnknownKey`.
- **Q8.10 (reinstall kekal ikut validity)** ditampung oleh L1 (Q8.2 + Q8.3). Kedua-dua fail di `%APPDATA%\Halo`, yang tak disentuh uninstaller (`halo.nsi:158`), jadi reinstall terus Pro. "Ikut validity" dipenuhi oleh `exp` dalam token, yang disemak offline (`halo-core/src/license.rs:126`). `expires_at` ditetapkan **sekali** semasa tebusan pertama (`license-server.mjs:237`), jadi reinstall atau reissue **tak** menetapkan semula trial.
- **PC baru / Windows baru / akaun Windows baru:** `secret::save` guna DPAPI skop pengguna (`services/secret.rs:56`). Fail yang disalin ke PC lain tak boleh dibuka (`load` = None), jadi `device()` membuat id baru (`license.rs:47-60`), dan kod lama dapat `409 already_used`. **Tiada jalan pulih hari ini.**
- Server tak boleh "lepaskan" install: `redeem` menolak sebarang device yang berbeza bila `redeemed_at` sudah ada (`license-server.mjs:232`). Device `null` pun dikira berbeza.
- `/api/status` untuk license yang dikenali tetapi device berbeza menjawab `revoked` (`:252`). Q8.6 hanya mengubah kes license **tak dikenali** kepada `unknown`.
- **Migrasi sekali dari `%LOCALAPPDATA%` (Q8.2) tiada penerima.** Tiada build yang dirilis pernah menerima token (Fakta 8A). Fail `license-device.bin` yang mungkin wujud pada PC 0.2.40 tak terikat pada sebarang tebusan. Satu-satunya tebusan di server ialah ujian pemilik.
- `%APPDATA%` ialah folder "Roaming". Dalam domain dengan roaming profile, fail dan kunci DPAPI pengguna turut merayau, jadi satu id boleh muncul pada dua PC dalam domain yang sama.

#### Design tree

```
L1 path %APPDATA%\Halo (Q8.2/8.3 diterima)
 ├─ migrasi dari LOCALAPPDATA ──> tiada penerima ──> gugurkan?
 ├─ roaming profile ──> satu license di beberapa PC domain ──> terima?
 └─ uninstall kekalkan license ──> privasi / dokumen
PC baru / Windows baru ──> id baru ──> 409
 └─ "Release install" di portal ──> redeem terima device null
     └─ install lama masih Pro offline ──> status jawab `moved` ──> jatuh ke Free (L2)
Putaran signing key ──> jangan buang baris lama
```

#### Frontier

- **Q8.19 Gugurkan migrasi sekali dari `%LOCALAPPDATA%`; fail baru terus ke `%APPDATA%\Halo`?** **Cadangan: ya.** Sebab: tiada build yang dirilis pernah menerima token, jadi migrasi itu ialah code untuk pengguna yang tak wujud.
- **Q8.20 Terima risiko roaming profile (satu license di beberapa PC dalam domain yang sama)?** **Cadangan: ya.** Sebab: pengguna Halo ialah PC peribadi, dan settings sudah tinggal di `%APPDATA%`. Alternatifnya (folder `%LOCALAPPDATA%\Halo License` yang uninstaller tak sentuh) hanya diperlukan jika pemilik menjual kepada syarikat.
- **Q8.21 Butang portal "Release install": kosongkan `device_sha256`, kekalkan `redeemed_at` dan `expires_at` (validity berterusan), audit `release`; `redeem` menerima device `null` sebagai boleh diikat semula?** **Cadangan: ya.** Sebab: inilah satu-satunya jalan untuk PC baru / Windows baru. Dengan View (8B) atau New code, pemilik boleh memberi pengguna kod untuk ditebus semula.
- **Q8.22 `/api/status` untuk license yang dikenali tetapi device berbeza menjawab `moved`, dan app menganggapnya seperti `revoked` (jatuh ke Free); `unknown` (Q8.6) kekal untuk id yang tak dikenali?** **Cadangan: ya.** Sebab: tanpa ini, selepas Release install, PC lama kekal Pro selamanya (semakan offline) dan satu license jadi dua. Dengan L2 (semakan harian), PC lama jatuh ke Free dalam masa sehari.
- **Q8.23 Peraturan putaran: baris key dalam `license-keys.txt` tak pernah dibuang selagi ada token yang ditandatangani dengannya masih sah; putaran = tambah baris baru?** **Cadangan: ya.** Sebab: membuang baris menjadikan semua token itu `UnknownKey`, iaitu semua orang jatuh ke Free selepas update. Ini melanggar Q8.9.
- **Q8.24 Uninstall mengekalkan license dan id secara senyap (seperti settings); README dan `PRIVACY.md` menyebutnya; Deactivate kekal sebagai cara membuang token?** **Cadangan: ya.** Sebab: Q8.10 memerlukannya. Pengguna yang mahu buang semua ada Deactivate, dan settings sudah mengikut peraturan yang sama.

### Terbuka (round 3)

- Kalau Q8.4 = ya (diterima): `PRIVACY.md` dan `docs/licenses.md` dikemas kini (`lic` + hash device dihantar sekali sehari).
- `tools/ci-installer.ps1`: semakan "uninstall kekalkan `%APPDATA%\Halo\license*.bin`".
- Teks jawapan app untuk `moved` ("This license moved to another PC").
- Self-service release (tanpa admin)? Belum perlu (YAGNI) sehingga ada pengguna sebenar.

---

## Ringkasan risiko round 2

1. **License tetap mati dalam 0.2.50 tanpa Q8.11.** URL dibaca dari env pengguna, bukan dari build.
2. **"Semua keadaan" tak boleh bermaksud "semasa lock".** Skrin lock ialah desktop lain. Yang boleh hanyalah "selepas unlock" (Q2.12).
3. **Satu headset BT hari ini sudah menghasilkan dua popup** (Volume + Bluetooth). Popup headphone baru akan menjadikannya tiga tanpa Q2.19 + Q2.21.
4. **Volume memadam popup lain** yang sedang dipaparkan (Q2.13).
5. **View kod menambah rahsia baru di server** (`code-key`). Keselamatan bergantung pada key dan data tak pernah berada dalam satu backup (Q8.18), dan folder `keys` hari ini 775 (Q8.13).
6. **Release install tanpa `moved`** menjadikan satu license dua PC Pro selamanya (Q8.22).
7. Ruang header (chip popup Q2.15 + gear item 7) bersaing pada lebar yang sama. Turutan gugur mesti satu senarai (Q7.4).
