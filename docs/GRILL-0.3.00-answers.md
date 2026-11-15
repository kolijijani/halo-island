# Grill 0.3.00 — Jawapan Pemilik (Round 1)

Tarikh: 15 Nov 2026. Pemilik: Encik Koliji. Status: jawapan round 1, untuk round 2.

Format: `ID — jawapan pemilik (dengan nota)`.

## 0. Release dan versi

- **Q0.1** — **Tidak guna 0.3.0. Naik ke `0.2.50`.**
- **Q0.2** — Pemilik tanya: boleh ke release terus `0.2.50` tanpa release `0.2.41` dulu? (Ko-Dex dah jawab: BOLEH — pengguna 0.2.40 kena install manual 0.2.50 sekali, selepas itu auto-update wujud. Cadang terus 0.2.50.)
- **Q0.3** — Ya. Windows sahaja, Mac tunggu.

## 1. Weather

- **Q1.1** — Ya. Pemilik laporkan tile tunjuk **"Can't reach it"** (error). Akan hantar screenshot + baris `weather:`/`location:` dari `halo.log`.
- **Q1.2** — Ya. Log HRESULT sebenar.
- **Q1.3** — Ya. Baiki self-test bocor ke network.
- **Q1.4** — Ya. Kekal baca bila tab Widgets dibuka.

## 2. Popup

- **Q2.1** — **SEMUA situasi** (bukan satu). Popup Wi-Fi mesti keluar dalam semua senario.
- **Q2.2** — **TIDAK abaikan. Popup mesti keluar di SEMUA keadaan, termasuk island terbuka.** (Terbalikkan reka bentuk asal.)
- **Q2.3** — Ya. Mic/camera popup sekali.
- **Q2.4** — Ya. API rasmi.
- **Q2.5** — Ya. Windows Focus sahaja, DND digugurkan.
- **Q2.6** — Ya. Airplane tekan cascade.
- **Q2.7** — **Ya, jack berwayar + headphone Bluetooth** (bukan wired sahaja).
- **Q2.8** — Ya. Monitor bila bilangan berubah.
- **Q2.9** — Ya. Default ON/OFF ikut cadangan.
- **Q2.10** — Ya. Satu suis per jenis.

## 3. Animasi

- **Q3.1** — Ya. Senarai tetap.
- **Q3.2** — Ya. Pemilik akan terangkan momen video / hantar screenshot.
- **Q3.3** — Ya. Tolak breathing, pulse masa fasa.
- **Q3.4** — Ya. Status ring ditolak.
- **Q3.5** — Ya. Battery saver langkau.
- **Q3.6** — Ya. Update ANIMATION-PLAN.

## 4. Every setting

- **Q4.1** — Ya. Section sidebar dalam Advanced settings.
- **Q4.2** — Ya. Buang UI, simpan INVENTORY+test.
- **Q4.3** — Ya. Baris sejarah kekal, buang keyword.

## 5. FPS

- **Q5.1** — Ya. Overlay game sedia ada.
- **Q5.2** — Ya. Kekal Pro, baris sentiasa nampak berkunci.

## 6. Tray

- **Q6.1** — Ya. Menu dipotong ikut senarai.
- **Q6.2** — Ya. Apple Music lyrics pindah Advanced › Music.
- **Q6.3** — Ya. Klik kanan island ikut tray.

## 7. Butang Settings

- **Q7.1** — Ya. Gear+pin sentiasa ada ruang.
- **Q7.2** — Ya. Overflow "..." jalan terakhir.
- **Q7.3** — Ya. Banyak menu = banyak tab add-on.

## 8. License

- **Q8.1** — Ya. Hidupkan license-server.
- **Q8.2** — Ya. L1 pindah license-device.bin ke %APPDATA%.
- **Q8.3** — Ya. L1 license.bin (token) juga pindah.
- **Q8.4** — Ya. L2 tanya /api/status sekali sehari.
- **Q8.5** — Ya. Gagal rangkaian = Pro kekal.
- **Q8.6** — Ya. Server jawab "unknown".
- **Q8.7** — Ya. Butang "New code" portal.
- **Q8.8** — **Ya (BARU). License portal admin boleh VIEW kod license yang dijana** (lihat kod penuh, bukan sekadar 8 aksara id). Ini bercanggah dengan prinsip "kod hanya SHA-256, tak boleh dipapar" — perlukan simpanan encrypted.
- **Q8.9** — **Ya (BARU). Bila user UPDATE app, license masih boleh guna.**
- **Q8.10** — **Ya (BARU). Bila user UNINSTALL + reinstall, license lama masih boleh pakai ikut validity.**

## Perubahan vs cadangan CC CLI (untuk perhatian round 2)

1. **0.1** versi `0.2.50` (bukan 0.3.0) — kemas kini semua rujukan versi.
2. **2.1 + 2.2** popup dalam SEMUA keadaan termasuk island terbuka — perlu reka cara papar tanpa pecah layout.
3. **2.7** headphone termasuk Bluetooth — perlu settle dedup dengan popup Bluetooth sedia ada.
4. **8.8** admin boleh view kod penuh — perlu reka simpanan encrypted kod (AES-256-GCM + key file) atau mekanisme lain; bercanggah dengan Q8.7.
5. **8.9 / 8.10** pengesahan lifecycle license: update kekal, reinstall kekal ikut validity.
