# A2Z-08: Option Halo Mascot

**11 Nov 2026.** Option idle baharu "Halo Mascot", ikut spesifikasi `docs/A2Z-07-mascot-ideas.md` (seksyen "Keputusan"). Oyen kekal; Halo hidup di sebelahnya.

## Apa yang ditambah

| Fail | Perubahan |
|---|---|
| `crates/poni-core/src/mascot.rs` | State machine tulen (`Mascot`, `Pose`): masa sahaja, tiada thread atau timer. Juga `last_input()`. |
| `crates/poni-core/src/settings.rs`, `lib.rs` | `Settings::mascot` (bool, default `true`; fail lama tanpa kunci ini pun dapat `true`). |
| `crates/poni/src/state.rs` | `AppState::mascot`. |
| `crates/poni/src/activities.rs` | `mascot_shows()`, `draw_mascot()`, saiz pill idle, `next_wake` untuk maskot. |
| `crates/poni/src/pill/idle.rs` | Tema Pill: pill idle sebesar mata, lukisan ikut Classic. |
| `crates/poni/src/app.rs` | `sync_mascot()` setiap frame, raw input semasa Mengantuk/Tidur, `WM_INPUT`, menu `ToggleMascot`. |
| `crates/poni/src/tray.rs` | Item "Halo Mascot" (bertanda) betul-betul di bawah submenu "Idle look". |
| `crates/poni/src/settings_window.rs` | Suis "Halo Mascot" dalam Appearance › Island at rest (Windows sahaja). |
| `crates/poni/src/selftest.rs` | Scene self-test sedia ada menutup maskot supaya tangkapan jam kekal sama. |

Asas `mascot.rs` dan `Settings::mascot` ditulis oleh sesi `app-009-halo-island-39` dalam working tree yang sama sebelum sesi itu keluar. Sesi ini membuang salinan pendua (`halo.rs`, `Settings::halo_mascot`) dan menyambungnya ke app.

## Cara toggle

- **Tray:** klik kanan ikon › **Halo Mascot** (tanda semak = ON).
- **Settings:** Advanced settings… › Appearance › Island at rest › **Halo Mascot**.
- **Fail:** `"mascot": false` dalam `settings.json`.

Maskot muncul hanya bila island idle (tiada media, timer, notifikasi atau aktiviti lain) dan rupa idle ialah Clock (atau Status di luar tema Pill). Tema Oyen kekal dengan Oyen dan tema Diorama dengan gambarnya. Rupa Eyes, Minimal dan Hidden tidak disentuh. Bila media, timer atau notifikasi datang, mod berubah daripada `Idle` dan pill aktiviti biasa muncul. Bila ia pergi, maskot kembali segar (Jaga).

## State machine

```
            input                       8 s tanpa input        20 s tanpa input
  Jaga ──────────────▶ Jaga   Jaga ─────────────────▶ Mengantuk ─────────────────▶ Tidur
  (kerdip 120 ms                       (600 ms turun,              (frame statik,
   setiap 4.0-5.8 s)                    kemudian statik)            tiada wake)
                                            │                          │
                                            └──── input pertama ───────┴──▶ Melambai 1.2 s ──▶ Jaga
```

| State | Lukisan | Wake seterusnya |
|---|---|---|
| Jaga | Dua kapsul 8×16, radius penuh, jarak 10 | Hujung kerdip atau Mengantuk, mana dulu |
| Kerdip | Kapsul 8×2 | Hujung kerdip (120 ms) |
| Mengantuk | Kapsul 8×16 → 12×4, turun 2 px dalam 600 ms | `last_input + 20 s` |
| Tidur | Dua lengkung ∪, garis 2.5 | Tiada: frame statik sehingga input |
| Melambai | Lengkung ∩ + tangan kapsul 4×9 di kanan, condong ±20° dua kali | Hujung lambai (1.2 s) |
| Ikut pointer | Kedua mata beralih maks 2 px ke arah kursor | Hanya `WM_MOUSEMOVE` ke island |

Input dikesan dengan dua cara:
- **Jaga:** `GetLastInputInfo` dibaca hanya pada frame yang memang dilukis (kerdip, pointer). Masa input dikira semula supaya Mengantuk jatuh 8 s selepas input sebenar.
- **Mengantuk dan Tidur:** raw input papan kekunci dan tetikus didaftarkan (`RIDEV_INPUTSINK`). `WM_INPUT` pertama meminta satu frame yang memulakan lambaian, dan pendaftaran dilepaskan serta-merta.

## Keep it light

- **Tiada poll:** setiap perubahan ialah wake one-shot `TIMER_WAKE` (`activities::next_wake`). Tidur tidak menjadualkan apa-apa.
- **Animasi berhenti:** bila `App::locked`, `App::display_off` atau `st.power.saver`, `Mascot::still = true`. Tiada kerdip, transisi, lambaian atau wake, dan raw input tidak didaftar.
- **Frame penuh** hanya semasa transisi Mengantuk (0.6 s) dan Melambai (1.2 s).
- **Raw input** hidup hanya semasa Mengantuk atau Tidur yang kelihatan. Ia dilepaskan pada input pertama, bila island disorok untuk app skrin penuh, atau bila maskot tiada.
- **Kos:** tiada crate baharu dan tiada aset (vektor sahaja). Saiz download dan memori at rest tidak diukur di sini; lihat bahagian "Belum siap".

## Keputusan test

- `cargo test -p poni`: **0 test dijalankan, 0 gagal.** Di Linux semua modul `poni` ialah `cfg(windows|macos)`, jadi binari test kosong. Sebab itu test state machine diletak dalam `poni-core`.
- `poni-core` (`cargo test -p poni-core --lib`): **582 lulus, 0 gagal, 1 diabaikan**, termasuk 6 test berkaitan maskot:
  - `mascot::tests::awake_blinks_dozes_then_sleeps_and_stays_still`
  - `mascot::tests::blink_gaps_stay_between_four_and_six_seconds`
  - `mascot::tests::input_after_doze_or_sleep_waves_then_wakes`
  - `mascot::tests::input_while_awake_only_holds_off_the_doze`
  - `mascot::tests::still_means_no_motion_and_no_wakes`
  - `settings::tests::halo_mascot_is_on_until_switched_off_and_the_choice_persists` (persist setting)
- Test `poni` baharu (`tray::halo_mascot_is_a_checked_item_under_idle_look`, `settings_window::halo_mascot_switch_flips_and_saves`) **lulus type-check** (`cargo check -p poni --tests --target x86_64-pc-windows-gnu`). Ia **belum dijalankan** kerana mesin ini tiada mingw (`x86_64-w64-mingw32-dlltool`) atau Windows.
- `cargo wcheck`: lulus.
- `cargo mcheck`: gagal di `aws-lc-sys`, iaitu toolchain C cross untuk Mac yang tiada di mesin ini. Ini tiada kaitan dengan perubahan ini.

## Belum siap

- **Mac:** notch Mac belum melukis Halo. `mascot_shows` ialah `cfg!(windows)` dan suis Settings disorok di Mac.
- **Tema terang:** palet light (`#F3F5F9` / `#141821`) belum diuji atas Glass terang. Warna mata mengikut `Look::paint` tema.
- **Ujian sebenar:** belum diuji dalam app di Windows. Tangkapan self-test untuk scene maskot dan ukuran kos frame Jaga/Melambai (cycles) belum dibuat.
- **Kos PR:** saiz installer dan memori at rest belum diukur. Kedua-duanya keluar daripada artifact CI.
