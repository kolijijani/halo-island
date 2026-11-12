# A2Z-05 — Build & uji (jujur)

## Run 4 (11 Nov 2026): selepas Fasa 4 penuh (lesen, 3-mode, monitor, FREE + PRO)

| Arahan | Hasil |
|---|---|
| `cargo metadata --format-version 1 --no-deps` | OK |
| `cargo check --workspace` | OK, 0 warning |
| `cargo test --workspace` | **OK: 833 lulus, 0 gagal, 1 diabaikan** — `poni-core` 592 (+16 ujian baru: `license`, `halo`, `look::halo_*`, settings Pro/monitor/handle), `studio-usage` 220, `studio-diorama` 21 |
| `cargo wcheck` (Windows type-check, windows-rs 0.58) | **OK, 9 warning dead-code — sama dengan baseline sebelum Run 4** (diukur awal run) |
| `cargo check -p poni --target x86_64-pc-windows-gnu --tests` | OK (ujian Windows `poni` type-check; tidak boleh dijalankan di Linux) |
| `cargo mcheck` (Mac) | GAGAL, sama seperti sebelum: `aws-lc-sys` perlu C compiler Apple. Isu toolchain, bukan kod. Satu perubahan Mac (`Command::ClipBack` → no-op) ditambah supaya match kekal lengkap; tidak dapat disahkan |
| `cargo wbuild` / exe / installer | **Tidak dicuba**: tiada `x86_64-w64-mingw32-gcc`, `windres`, `makensis` atau Wine di node |

Ralat penting yang ditemui dan dibetulkan semasa run (semua dari kod baru, bukan upstream):
- `E0004 non-exhaustive patterns: MonitorChoice::Secondary/All` di `window.rs:324` → `pick_monitor` kini kendalikan Secondary (monitor bukan-utama pertama, kiri ke kanan, via `EnumDisplayMonitors`) dan All (monitor tetingkap aktif).
- Kunci serde `water` bertembung dengan medan lama `Store::water` (sejarah Water page upstream) → medan baru disimpan sebagai `"glasses"`.
- Ujian upstream `game_stats_and_full_screen_choice_follow_each_other`: overlay game kini Pro, jadi ujian beri `pro_key`.

Ujian Windows-only yang dikemas kini (type-check sahaja, **belum dijalankan**): `activities::stepped_aside_*` (handle 34×8), `pages::tests` (tab Widgets selepas Music), `tray::tests` (overlay "(Halo Pro)", sejarah System perlu Pro), self-test `aside-handle` (bukan lagi strip halimunan).

Kos (Keep it light): tiada crate baru. Tiada apa berjalan semasa rehat kecuali yang dihidupkan pengguna: peringatan air (satu `SetTimer`, 45–90 min, OFF default), clipboard (`WM_CLIPBOARDUPDATE` Windows, tiada poll, OFF default), cloud sync (salin 2 fail setiap save, Pro + OFF default), cuaca (satu bacaan bila tab Widgets dibuka dan bacaan > 30 min). Handle mini statik (tiada frame semasa rehat). Slide fullscreen 0.6 s sahaja. Saiz exe belum diukur (perlu mingw).

---

## Run sebelumnya (Pass 1–2)

Node: Linux x86_64 (kernel 7.0.0). Tiada `cargo` asal di node, jadi toolchain dipasang **user-local** dengan rustup (`~/.rustup`, `~/.cargo`, profil minimal, tanpa sudo, tanpa ubah PATH shell): `rustc`/`cargo 1.99.1`, target tambahan `x86_64-pc-windows-gnu` dan `aarch64-apple-darwin`. Tiada `mingw-w64`, `makensis`, `iscc` atau Wine di node.

Semua arahan dijalankan dari root repo dengan `PATH=$HOME/.cargo/bin:$PATH`.

## Ringkasan

| Arahan | Sebelum perubahan | Selepas perubahan | Nota |
|---|---|---|---|
| `cargo metadata --format-version 1 --no-deps` | OK | OK | Repo munasabah |
| `cargo check --workspace` | OK | OK, 0 warning | Bin `poni` di Linux = stub `main()` |
| `cargo check -p poni` (host Linux) | OK | OK | Tidak gagal: modul Windows di belakang `cfg(windows)`, jadi tiada ralat windows-rs. Bukan bukti Windows build |
| `cargo test --workspace` | — | **OK: 817 lulus, 0 gagal, 1 diabaikan** | lihat di bawah |
| `cargo wcheck` (= `check -p poni --target x86_64-pc-windows-gnu`) | OK, 10 warning | **OK, 10 warning** (sama) | Type-check sebenar kod Windows termasuk windows-rs 0.58 |
| `cargo check -p poni --target x86_64-pc-windows-gnu --tests` | — | OK | Ujian unit Windows (tray, version, updates) type-check; tidak boleh **dijalankan** di Linux |
| `cargo mcheck` (Mac) | — | GAGAL (toolchain) | `aws-lc-sys` (via `str0m`→`dimpl`) perlu C compiler Apple: `cc: error: unrecognized command-line option '-arch'`. Bukan dari perubahan ini |
| `cargo wbuild` (exe Windows) | tidak dicuba | tidak dicuba | Perlu `x86_64-w64-mingw32-gcc` + `windres` (tiada di node) |

### Ujian per crate (`cargo test -p <crate>`)

| Crate | Lulus | Gagal | Diabaikan |
|---|---|---|---|
| `poni-core` | 576 | 0 | 1 (`prayer::tests::fit_muhammadiyah`, `#[ignore]` upstream) |
| `studio-usage` | 220 | 0 | 0 |
| `studio-diorama` | 21 | 0 | 0 |
| `poni` (host Linux) | 0 | 0 | 0 (semua ujian di belakang `cfg(windows/macos)`) |

### 10 warning `cargo wcheck` (sedia ada, tidak berubah)

Semua `dead_code` dalam build debug: `commands.rs:23` (`Tick`), `commands.rs:178` (`FocusText`, `WantStats`), `events.rs:57`, `events.rs:159` (`Stats`, `Log`), `gfx.rs:2069` (`reset_transform`), `island.rs:201`, `ui.rs:49`, `ui.rs:258`, `window.rs:286`. Dibandingkan pada tree bersih (`git stash`) dan tree berubah: kiraan sama, tiada warning baru (`FEED_ON`, `APP_NAME` digunakan).

## Apa yang TIDAK disahkan di node ini

- Tiada `poni.exe` dibina, tiada app dijalankan, tiada screenshot. Ujian unit Windows (`tray::tests`, `version::tests`, `updates::tests`, yang telah dikemas kini untuk "Halo Island") hanya type-check, belum dijalankan.
- Installer NSIS/Inno tidak dikompil.
- `poni.rc` (version info) hanya dikompil oleh `windres`/`rc.exe` semasa build Windows.

## Langkah build Windows (tepat, dari README/ARCHITECTURE/AGENTS)

### A. Di Windows 10/11 (MSVC) — disyorkan

1. Pasang Rust stable (rustup, toolchain `x86_64-pc-windows-msvc`) dan Visual Studio Build Tools (C++ + Windows SDK, untuk `rc.exe` dan linker).
2. Dari root repo:
   ```
   cargo test -p poni-core -p studio-usage -p studio-diorama
   cargo test -p poni
   cargo build -p poni --release        # target\release\poni.exe
   ```
3. Smoke tanpa klik:
   ```
   target\release\poni.exe --selftest out
   ```
   Kod keluar 0 dan `out\report.json` `"ok": true`. `PONI_SELFTEST_DPI=144` untuk 150%, `PONI_RENDERER=software` untuk renderer software.
4. Jalankan: `target\release\poni.exe`. Island muncul di atas tengah; ikon tray "Halo Island 0.2.30".

### B. Cross dari Linux/macOS (mingw-w64)

```
sudo apt install mingw-w64                       # x86_64-w64-mingw32-gcc + windres
rustup target add x86_64-pc-windows-gnu
cargo wcheck                                     # type-check
cargo wbuild                                     # target/x86_64-pc-windows-gnu/release/poni.exe
NSIS_HOST=user@linux-box tools/package.sh        # exe + zip portable + installer ke dist/ (atau makensis di PATH)
```

`tools/run-selftest-wine.sh` boleh jalankan self-test di bawah Wine.

### C. Installer

- NSIS (installer CI): `makensis -V2 -DEXE=<abs>/poni.exe -DOUTDIR=<abs>/dist installer/poni.nsi` → `dist/Halo-Studio-Setup-0.2.30.exe` (nama fail kekal; paparan "Halo Island").
- Inno Setup 6.5.2+: `ISCC.exe /DAppVersion=0.2.30 installer\poni.iss`.

### D. Saiz (AGENTS.md, Keep it light)

Bandingkan sebelum/selepas dengan `CARGO_PROFILE_RELEASE_STRIP=false cargo wbuild` dan `x86_64-w64-mingw32-nm --size-sort -S -C`. Pass ini tiada crate baru. Perubahan ialah rentetan dan satu pemalar boolean, jadi saiz dijangka sama (tidak diukur di node ini).

## Pembersihan

Toolchain user-local (`~/.rustup` ≈ 877 MB, `~/.cargo` ≈ 510 MB) dan `target/` (≈ 3.1 GB, di-ignore oleh git) kekal untuk pass seterusnya. Buang dengan `rustup self uninstall` dan `cargo clean` jika tidak perlu.
