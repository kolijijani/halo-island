# Grill 0.2.50 — Jawapan Pemilik (Round 2)

Tarikh: 15 Nov 2026. Pemilik: Encik Koliji. Status: jawapan round 2.

Format: `ID — jawapan pemilik`.

## 0. Versi

- **Q0.4** — Ya. Tag `v0.2.50`, semua fail versi tukar serentak (manifest `0.2.50.0`).
- **Q0.5** — **TIDAK gabung. GUGURKAN `v0.2.41.md` (buang). Fokus Windows sahaja, Mac digugurkan.**
- **Q0.6** — Ya. Nama fail `GRILL-0.3.00*.md` + `ANIMATION-PLAN-0.3.00.md` kekal + baris "Versi sebenar: 0.2.50".
- **Q0.7** — Ya. Band "New in 0.2.50": auto-update dapat kad, ikon bukan kad.

## 1. Weather

- **Q1.5** — **Pemilik DAH buka kedua-dua URL di Edge — kedua-duanya keluar JSON BETUL:**
  - Open-Meteo: OK, `temperature_2m: 26.2`, Kuala Lumpur.
  - ipwho.is: OK, IP `219.92.31.33` (TMNET, Malaysia).
  - **Kesimpulan: rangkaian + DNS + antivirus PC pemilik SEMUA OK. Bug ada dalam laluan `HttpClient` WinRT Halo, bukan rangkaian.**
- **Q1.6** — Ya. Butiran gagal masuk log, tile kekal "Can't reach it".

## 2. Popup

- **Q2.11** — Ya. Timer kira bila kelihatan, queue MAX 3.
- **Q2.12** — Ya. Locked tahan + papar selepas unlock, kecuali Volume/toggle dibuang.
- **Q2.13** — Ya. Volume tanpa queue, popup ditimpa kembali depan queue.
- **Q2.14** — Ya. Full-screen peek, kecuali presentation.
- **Q2.15** — Ya. Popup island terbuka = chip dalam header (ganti date+jam).
- **Q2.16** — **TIDAK. Popup Wi-Fi masa Halo mula MESTI keluar** (Connected · <nama> dikira berita).
- **Q2.17** — **Ya, TETAPI DIPERLUAS: popup untuk SEMUA jenis connection — Wi-Fi DAN Ethernet — dengan nama, dan sama untuk Bluetooth.**
- **Q2.18** — **TIDAK. Hanya bila CONFIRM connected** (jangan kira "Identifying…" sebagai joined).
- **Q2.19** — **TIDAK skip. Keluarkan SEKALI sahaja dengan device name** (dedup = satu popup, bukan dua; popup mesti ada nama device).
- **Q2.20** — Ya, popup bila connected to device.
- **Q2.21** — Ya. Port "switched" Mac ke Windows.
- **Q2.22** — Ya. Headphone wired ikut OnDeviceStateChanged.
- **Q2.23** — Ya. Popup mic/camera namakan app, sesi Halo diabaikan.

## 3. Animasi

- **Q3.7** — Ya. Gerak popup baru ikut jadual (mic=oren, camera=hijau).
- **Q3.8** — Ya. Spring smooth, tile fade 200ms, berhenti bila settle.
- **Q3.9** — **BARU. Semak SEMUA tempat yang boleh dibuat animasi, jadikan animasi — utamakan yang memberi effect besar.** (Melebarkan skop animasi dari senarai tetap kepada audit menyeluruh.)

## 4. Every setting

Selesai (Q4.1–Q4.3). Tiada soalan.

## 5. FPS

- **Q5.3** — Ya. Free: baris berkunci lencana Pro, klik buka Halo Pro.

## 6. Tray

- **Q6.4** — Ya. README tulis semula laluan menu.

## 7. Butang Settings

- **Q7.4** — Ya. Gugur: muka Halo > date > label tab > mampat tab.
- **Q7.5** — Ya. Overflow "…" = menu native Windows (TrackPopupMenu).

## 8. License

- **Q8.11** — Ya. Alamat redeem jadi pemalar code, env override ujian.
- **Q8.12** — Ya. Host redeem = Tailscale Funnel.
- **Q8.13** — Ya. Commit public key + folder keys 700.
- **Q8.14** — Ya. View = AES-256-GCM + code-key berasingan.
- **Q8.15** — Ya. View perlukan admin token ditaip semula + rate limit + audit.
- **Q8.16** — Ya. serve enggan mula tanpa --code-key.
- **Q8.17** — Ya. Ciphertext kekal selepas ditebus.
- **Q8.18** — Ya. Backup data & key berasingan.
- **Q8.19** — Ya. Gugur migrasi %LOCALAPPDATA%.
- **Q8.20** — Ya. Terima risiko roaming profile.
- **Q8.21** — Ya. Butang "Release install" portal.
- **Q8.22** — Ya. /api/status jawab "moved".
- **Q8.23** — Ya. Baris key lama tak dibuang.
- **Q8.24** — Ya. Uninstall kekal license senyap + sebut README/PRIVACY.
- **Q8.25** — **BARU. JANGAN bind license pada PC. User boleh GUNA BALIK license bila pindah PC.** (Konsep: license floating, bukan terikat pada satu device.)
- **Q8.26** — **BARU. License hanya VALID 1 PC pada satu masa. Bila PC kedua guna, PC pertama KICK jadi Free.**

## Perubahan yang perlu CC CLI settle (round 3)

1. **0.5** Mac digugurkan — buang `v0.2.41.md` + sebarang rujukan Mac release.
2. **1.5** Bug HttpClient WinRT Halo (rangkaian PC OK). Diagnosis fix tanpa tunggu log.
3. **2.16** Popup Wi-Fi masa mula = keluar (bukan skip).
4. **2.17** Popup semua connection (Wi-Fi + Ethernet + Bluetooth) dengan nama.
5. **2.18** Hanya confirm connected (bukan "Identifying").
6. **2.19** Dedup satu popup dengan device name.
7. **3.9** Audit animasi menyeluruh — buat yang berkesan.
8. **8.25 + 8.26** Floating license: tak bind PC, 1 PC aktif, PC kedua kick PC pertama Free. Perlu reka bentuk jelas (interaksi dengan Q8.21 Release install + Q8.22 moved).
