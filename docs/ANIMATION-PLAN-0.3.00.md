# Animation plan 0.3.00 (cadangan sahaja)

Status: proposal. Belum ada kod. Semua item ikut AGENTS.md "Keep it light": tak ada frame, timer atau thread baru semasa island rehat.

Permintaan pemilik (12 Nov): "button caps lock popup, connect bluetooth connect wifi dan lain lain jika perlu. sama macam bila user tekan volume down/up notch island kita keluar bagi tahu. buat animasi bagi cantik." Jadi plan ini meliputi **seluruh keluarga OSD pop-up**, bukan Caps Lock sahaja.

## 1. Yang dah ada

- **Spring engine** (`halo-core/src/spring.rs`): `bouncy` (420/30, zeta ~0.73), `smooth` (critical, k=380), dan `retune` simpan velocity. Semua morph island guna ini.
- **Island morph** (`pill/motion.rs`, `pill/shape.rs`): grow/shrink ikut paksi, corner critical, hover lift/sink, shadow ramp, capsule <-> card, content fade + `CONTENT_DROP` 2 DIP.
- **Mascot** (`halo-core/src/mascot.rs`, `activities.rs::draw_mascot`): blink, doze, sleep, wave, mata ikut pointer. Satu `SetTimer` one-shot, tiada poll. Page switch fade + slide dan glow bawah media (`pill/media.rs::glow`) pun sudah ada.
- **Fungsi pop-up yang pemilik minta sudah wujud**: `HudKind` Volume, Charging (termasuk "On battery"), LowBattery, Bluetooth, CapsLock dan Wi-Fi. Semua dilukis dalam `pill/hud.rs`: `volume()`, `power()`, `bluetooth()` dan `toggle()` (Caps + Wi-Fi), dengan versi Classic di `activities.rs::draw_hud`. Semua capsule H32, 1.6 s untuk Caps dan 2.6 s untuk yang lain.
- **Yang kurang hanya animasi di dalam capsule.** Island sudah morph masuk dan keluar, tapi disc, glyph dan warna muncul serta-merta. Volume bar glide guna `*0.32` per frame.

## 2. Perlu dianimasikan (shortlist)

| # | Bahagian | Ringkas |
|---|---|---|
| 1 | **Keluarga OSD pop-up** (6 jenis, satu bahasa motion) | Detail di §2.1 |
| 2 | **Notice tiba** (`pill/notice.rs`) | Satu glow pulse accent di tepi bawah semasa card mendarat, 600 ms, sekali sahaja. Guna semula `media::glow`. Ini signature "glow pulse" tanpa loop. |
| 3 | **Volume bar** (`hud.rs::volume`) | `cur + (target-cur)*0.32` per frame ditukar kepada `Spring::smooth`. Sekarang glide lebih laju di skrin 144 Hz berbanding 60 Hz. Spring guna `dt`, jadi sama di semua skrin. |

Tiga item cukup. Tak perlu cari item ke-4 atau ke-5.

### 2.1 Keluarga OSD: satu bahasa motion

**Tiga gerak sahaja, dikongsi semua pop-up:**

- **Pop**: sesuatu jadi ON atau sampai. Glyph/disc scale 0.6 -> 1.0, `Spring::new(0.6, 520.0, 30.0)` (zeta ~0.66, overshoot ~6%, settle ~300 ms), glyph naik dari +3 DIP ke 0.
- **Settle**: sesuatu jadi OFF atau pergi. Scale 1.0 -> 0.92 -> 1.0 dan glyph turun dari -3 DIP ke 0, dengan `Spring::smooth`. Tiada overshoot.
- **Glow**: satu pulse di tepi bawah, guna `media::glow` dengan alpha 0 -> 0.45 -> 0, 600 ms (`sin`, bukan spring). **Hanya untuk ON/sampai**, dan warnanya ikut warna disc atau glyph.

Warna disc (white(0.16) <-> accent) sentiasa dengan `Spring::smooth`, sebab warna tak boleh overshoot. Label dan nilai ikut content fade sedia ada; teks dan angka tak dianimasikan.

| Pop-up | Fn | ON / sampai | OFF / pergi | Glow | Kenapa |
|---|---|---|---|---|---|
| **CapsLock** | `toggle()` | Pop disc + glyph naik | Settle disc + glyph turun | Accent, ON sahaja | Feedback tekanan kekunci, dan orang memang tengok. Detail di §4. |
| **Wi-Fi** | `toggle()` | Pop bila joined | Settle bila dropped | Accent, joined sahaja | Fungsi sama dengan Caps, jadi animasi dapat percuma. |
| **Bluetooth** | `bluetooth()` | Pop disc (headphones/BT glyph) | Settle | Accent, connected sahaja | Disc 20 DIP sama dengan toggle, jadi gerak pun sama. Battery cluster dan nama tak bergerak. |
| **Charging** (plugged) | `power()` | **Bolt pop** sahaja, tanpa disc kerana `power()` tak ada disc. Scale 0.6 -> 1.0, spring sama. | — | **Hijau** (`palette::GREEN`), bukan accent | Plug-in ialah "tenaga masuk", jadi bolt dan glow hijau ikut warna glyph sedia ada. Satu-satunya glow bukan accent. |
| **On battery** (unplug) | `power()` | — | Glyph bateri settle (turun 3 DIP) | Tiada | Cabut kabel bukan berita baik, jadi tak perlu disambut. |
| **LowBattery** | `power()` | **Tak perlu animasi.** Hanya morph sedia ada. | — | Tiada | Amaran mesti tegas dan terus dibaca. Pop yang melantun nampak main-main. Pada bateri rendah, Windows selalunya sudah buka battery saver (default 20%), dan peraturan saver kita akan langkau animasi juga, jadi statik lebih konsisten. Glyph merah sedia ada sudah cukup tegas. |
| **Volume** | `volume()` | **Glyph tak perlu animasi.** Bar sahaja guna spring (item 3). | — | Tiada | Kekunci volume repeat bila ditekan lama. Pop pada setiap step akan bising, dan bar sudah menjadi feedback. |

**Keselarasan:**
- Semua pop-up kongsi spring dan durasi yang sama, jadi rasanya satu keluarga.
- Bezanya hanya pada **glow** dan **subjek pop**. CapsLock, Wi-Fi dan Bluetooth pop disc dengan glow accent. Charging pop bolt dengan glow hijau. LowBattery dan Volume tidak pop.
- Bluetooth sengaja **tak** diberi gesture sendiri. Ia sudah ada disc yang sama dengan toggle, dan gesture lain akan pecahkan keluarga.
- **Peraturan ringkas:** pop + glow bila sesuatu datang, settle tanpa glow bila sesuatu pergi, statik bila ia amaran atau berulang.

**Kos:** state animasi diset semula hanya bila `st.hud` bertukar kind/state. `request_anim()` dipanggil hanya semasa spring bergerak atau glow < 600 ms (~720 ms selepas pop-up muncul). Selepas itu, pop-up yang masih terpapar tak minta frame. Classic (`activities.rs::draw_hud`) pakai tuning yang sama.

## 3. Tak perlu dianimasikan

- **LowBattery dan glyph Volume**: lihat §2.1.
- **Pop-up jenis baru** (Num Lock, mic mute, brightness): pemilik kata "jika perlu". Enam jenis sedia ada cover kes biasa. Tambah bila ada permintaan khusus.
- **Island "breathing" atau glow loop semasa rehat**: frame setiap refresh sepanjang hari, jadi langgar AGENTS.md.
- **Pose mascot baru**: mascot hanya ada di idle clock. Lima pose sedia ada sudah cukup.
- **Page transitions**: fade + slide sudah ada. Jangan tambah stagger per elemen.
- **Nombor berdetik** (peratus bateri dalam pop-up, System, Stocks, Usage): angka mesti tepat serta-merta.
- **Graf live System/Stocks**: data sudah bergerak sendiri.
- **Hover pada setiap butang/row dalam pages**: hover lift island sudah cukup.
- **Settings window**: jarang dibuka. Statik ok.
- **Equalizer atau visual muzik tambahan**: equalizer sudah redraw setiap frame semasa main.
- **Diorama, lyric**: sudah ada motion sendiri.
- **Tray icon atau `halo.ico` animasi**: perlu timer. Langkau.
- **Mac notch**: CI Mac sedang paused. Tunggu.
- **Spinner/skeleton loading**: addon download sudah ada progress.

## 4. Caps Lock -> notch

Caps Lock ialah contoh penuh keluarga OSD (§2.1). Pop-up lain ikut timing dan spring yang sama, cuma subjek dan glow berbeza.

### Detect

**Kekalkan yang sedia ada.** Raw input keyboard (`RegisterRawInputDevices`, usage page 1 / usage 6, `RIDEV_INPUTSINK`), dan `CapsLock::key` flip state bila kekunci dilepaskan. State awal dibaca sekali melalui `GetKeyState(VK_CAPITAL)`.

- **Jangan** guna `WH_KEYBOARD_LL`. Hook itu nampak setiap keystroke seluruh sistem, tambah latency pada semua input dan selalu kena flag antivirus sebagai keylogger.
- **Jangan** poll `GetKeyState`. Itu kos semasa rehat.
- Raw input hanya didaftar bila `hud_caps` on, dan diabaikan semasa `locked` / `display_off` (sudah ada di `raw_key`). Kos baru: **tiada**.

### Apa jadi pada island

Morph sedia ada kekal: pill rehat -> capsule toggle (`GROW_W/H` floating, Classic tuning bila tergantung). Tambahan hanya di dalam capsule:

**ON**: disc pop (0.6 -> 1.0, ~1 DIP overshoot pada disc 20 DIP), warna white(0.16) -> accent, glyph naik +3 -> 0 sambil fade in ("terangkat", sesuai dengan huruf besar), dan satu glow accent.

**OFF**: disc settle (1.0 -> 0.92 -> 1.0), accent -> white(0.16), glyph turun -3 -> 0, tanpa glow.

### Timing (dari kekunci dilepaskan, t = 0)

| t (ms) | Apa |
|---|---|
| 0 | `offer_popup` + `request_frame` (sedia ada) |
| 0-350 | Island morph pill -> capsule (spring sedia ada) |
| 60 | Content fade in bermula (sedia ada) |
| 80 | Pop/settle disc + glyph mula, selesai ~380 |
| 120-720 | Glow (ON sahaja), puncak pada 300 |
| ~720 | Animasi dalam capsule tamat. **Tiada frame** sementara capsule terpapar. |
| 1600 | `popup_secs` tamat (Caps 1.6 s; yang lain 2.6 s), shrink sedia ada ~300 ms |
| ~1900 | Island rehat |

Tekan berulang (ON/OFF/ON cepat): `Offer::Show` sedia ada ganti pop-up di tempat yang sama dan reset `until`. Spring hanya `set_target` baru dan simpan velocity, jadi tak ada lompatan. Glow mula semula hanya bila jadi ON.

### Spring

- **Pop scale**: `Spring::new(0.6, 520.0, 30.0)`, zeta ~0.66. Lebih snappy dari `bouncy`, tapi tak berlantun dua kali.
- **Warna disc + offset glyph**: `Spring::smooth`.
- **Glow**: lengkung masa tetap 600 ms, jadi ia pasti berhenti sendiri.
- **Settle**: `is_settled_eps(0.005)` untuk nilai 0..1. Default 0.05 terlalu kasar.
- **Battery saver** (`st.power.saver`): langkau pop dan glow untuk semua pop-up, terus ke state akhir. Morph biasa masih jalan.

### Kenapa reka bentuk ini

- Tiada detect baru, morph baru atau timer baru. Ia cuma dua spring dan satu lengkung, dikongsi `toggle()`, `bluetooth()` dan `power()`.
- ON dan OFF dibezakan oleh gerak (terangkat vs kempis, glow vs tiada), bukan warna sahaja. Ini membantu orang yang buta warna.
- Frame hanya berjalan ~0.7 s selepas pop-up muncul, ditambah morph sedia ada. Kos semasa rehat kekal sifar.

## 5. Skop terkecil munasabah

Satu item utama untuk 0.3.00: **keluarga OSD pop-up**. Pop/settle/glow dikongsi CapsLock, Wi-Fi, Bluetooth dan Charging/On battery. LowBattery dan glyph Volume sengaja statik. Bersama item itu, volume bar ditukar ke spring. Semua kerja dalam `pill/hud.rs` dan laluan Classic `activities.rs::draw_hud`. Glow notice (item 2) hanya kalau masa masih ada, kerana ia guna helper glow yang sama. Selain itu: tak perlu.
