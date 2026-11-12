# BRIEF PASS 3 — Halo Island (APP-009): VIDEO PENUH FREE → PRO
# Run: claude-opus-5.5 via CC CLI · effort max · jangan commit/tag/push/release.

Repo ini sudah lepas Pass 1 (rebrand + MVP, 817/817 test lulus) dan Pass 2
(video mockup 28 s + maskot). Pass 3 ada SATU deliverable utama: SATU video
mockup penuh yang menunjukkan SEMUA feature Halo Island dari FREE sampai PRO,
dengan perincian setiap feature supaya Encik Koliji jelas apa yang akan dibina
sebelum production build. Maskot rasmi `Halo` MESTI muncul dalam video.

Baca dulu: docs/A2Z-01-dissection.md, docs/A2Z-02-mvp-plan.md,
docs/A2Z-06-video-mockup.md (pipeline render sedia ada),
docs/A2Z-07-mascot-ideas.md (spesifikasi akhir maskot, seksyen "Keputusan"),
mockups/render_mockup.py (cara render lama), mockups/halo-island-mockup.html
(contoh mockup sedia ada), AGENTS.md.

## Deliverable (semua fail BARU — jangan sentuh fail lama)
1. `mockups/halo-island-full.html` — mockup web self-contained (CSS+JS inline,
   tiada CDN/aset luar). Animasi frame-tulen: fungsi `seek(t)` deterministik,
   `window.DURATION` diisytihar; preview live dan render frame-demi-frame sama.
2. `mockups/render_full.py` — render HTML ke MP4 + poster guna Playwright
   Chromium (dah ada) + ffmpeg (dah ada). Ikut corak `mockups/render_mockup.py`:
   viewport 1920×1080, deviceScaleFactor 1, screenshot `seek(i/fps)` frame-by-frame,
   pipe PNG terus ke `ffmpeg -c:v libx264 -preset slow -crf 18 -pix_fmt yuv420p
   -movflags +faststart`. Jangan cipta fail frame sementara. Poster juga 1920×1080.
3. `mockups/halo-island-full.mp4` — output video.
4. `mockups/halo-island-full-poster.png` — poster (scene menarik, bukan hitam).
5. `docs/A2Z-09-full-video.md` — laporan: cara render, senarai scene + masa,
   output ffprobe, sha256, nota teknikal.

JANGAN ubah/overwrite: `mockups/halo-island-mockup.*`, `mockups/render_mockup.py`,
`crates/poni-core/src/mascot.rs`, semua `docs/A2Z-0*.md` yang sedia ada.

## Video: durasi & teknikal
- Satu video sahaja, 1920×1080, 30 fps, H.264 `yuv420p`, `+faststart`.
- Durasi sasaran ~80 saat (julat 75–95 s; jangan lebih 100 s). Dengan 10 feature
  FREE + 7 feature PRO + segmen maskot + intro/outro + kad perbandingan, 80 s
  memadai: setiap feature 3.5–5 s.
- Gaya Windows 11 / Dynamic Island sama seperti mockup lama: desktop wallpaper
  gradient lembut, island/notch hitam rounded tengah atas, window chrome halus,
  easing lembut. Wallpaper boleh bertukar dark→light pada segmen tema.
- Font: `Outfit` (fallback Segoe UI) seperti app sebenar.
- Jenama: hanya `Halo Island` dan `kolijijani`. Tiada Oyenza/poni/Oyen/nama
  syarikat lama dalam kandungan visual.
- Ini mockup konsep, bukan rakaman app sebenar.

## Struktur scene (ikut urutan)
Semua caption feature dalam Bahasa Melayu ringkas + nama feature (istilah app).
Setiap feature: judul + satu baris penerangan + visual bergerak dalam island.

### A. Intro (0–~5 s)
Kad "Halo Island" + "by kolijijani", subtitle "Semua feature · Free → Pro".

### B. Maskot Halo (~5–~15 s)
Tunjukkan maskot rasmi dalam island idle. Spesifikasi MESTI ikut
docs/A2Z-07 "Keputusan": DUA kapsul tegak 8×16 px, radius penuh (4 px), jarak
10 px, berpusat dalam pill. TIADA cincin, TIADA glow, TIADA mulut. Palet:
dark — pill `#0B0D12`, mata `#F2F5FA`; light — pill `#F3F5F9`, mata `#141821`.
Tunjukkan 5 state dengan label:
1. Jaga (awake) — mata terbuka, kerdip sekali.
2. Mengantuk (doze) — kapsul mendatar 12×4, turun 2 px.
3. Tidur (sleep) — dua lengkung ∪ nipis.
4. Melambai (wave) — lengkung ∩ + tangan kapsul kecil di kanan condong ±20°.
5. Ikut pointer — mata beralih maks 2 px ke arah kursor.
Tunjukkan sekurang-kurangnya sekali dalam dark dan sekali dalam light.

### C. FREE (setiap feature dengan label "FREE")
1. Island & Hover — notch kecil, hover 220 ms → expand spring, auto-hide bila tinggalkan.
2. Music — kawal Spotify/Apple Music/YouTube/browser; play/pause/skip; lirik sync + romaji/kana.
3. Focus Timer — countdown dengan progress ring dalam island.
4. To-do + Shelf — senarai tugasan + simpanan pantas.
5. Prayer Times — waktu solat setempat dalam island.
6. Battery & Peek — bateri/charging/headphones peek.
7. Themes — light/dark + 5 tema standard (tukar sekilas).
8. Widgets — 2 slot free.
9. Water Reminder — peringatan minum.
10. Clipboard — history 5 item.

### D. PRO (setiap feature dengan label "PRO" + penanda visual premium)
1. Overlay Games — FPS/GPU/CPU atas game (fullscreen borderless).
2. System Monitor — per-core, history panjang, top process, Power watt.
3. Phone Drop — hantar/terima fail dua hala dari mana-mana network.
4. Clipboard Unlimited — history tanpa had + cari.
5. Widgets Pro — 4–6 slot + widget eksklusif (world clock, moon, countdown).
6. Tema Premium + Custom Theme Builder.
7. Cloud Sync — sync settings antara PC (optional).

### E. Free vs Pro + outro (~8–10 s)
Kad perbandingan dua lajur ringkas (FREE vs PRO), kemudian outro
"Halo Island · A living notch for Windows · by kolijijani" +
mesej freemium: "Free dah lengkap — Pro untuk power user." (konsep ini penting:
JANGAN sampaikan Pro sebagai kunci feature asas).

## QA video (wajib, buktikan dalam docs/A2Z-09-full-video.md)
- `ffprobe -v error -show_entries format=duration,size -show_entries stream=codec_name,width,height,avg_frame_rate mockups/halo-island-full.mp4`
  Mesti: h264, 1920x1080, ~30 fps, durasi dalam julat, saiz munasabah.
- `sha256sum` MP4 dan poster.
- Contact sheet 8–12 frame dari MP4 (sebar rata) disemak: setiap segmen A–E muncul
  pada masa betul, tiada frame hitam/kosong, teks tidak terpotong.
- Tiada `pageerror` semasa render.
- Poster bukan hitam: semak dimensi + saiz fail, dan nyatakan scene poster.

## Guardrails
- Jangan commit/tag/push/release. Jangan sentuh .github, LICENSE, notices.
- Jangan ubah kod app kecuali fail mockups/ baru yang dinyatakan (video ini
  mockup, bukan implementasi feature).
- `crates/poni-core/src/mascot.rs` baharu wujud di repo — JANGAN sentuh.
- Kalau render 80 s terlalu lama untuk satu sesi, pecahkan render kepada
  2–3 segmen MP4 dan concat dengan ffmpeg (concat demuxer, codec sama), tetapi
  hasil akhir tetap SATU fail `halo-island-full.mp4` penuh. JANGAN berhenti
  tanpa MP4 lengkap.
- Pastikan maskot dalam video betul-betul "mata kapsul sahaja" — tanpa cincin/
  gelung/glow/aksesori. Ini keputusan rasmi Encik Koliji.
