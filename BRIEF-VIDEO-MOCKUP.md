# BRIEF PASS 2 — Halo Island (APP-009): branding fix + VIDEO MOCKUP
# Run: claude-opus-5.5 via CC CLI · effort max · jangan commit/tag/push/release.

Repo ini sudah lepas Pass 1 (dissect, MVP plan, rebrand asas, implement, test 817/817).
Pass 2 ada DUA tugasan: (A) betulkan branding ikut keputusan akhir Encik Koliji,
(B) hasilkan VIDEO MOCKUP Halo Island untuk dia review SEBELUM production build.

Baca dulu: docs/A2Z-01-dissection.md, docs/A2Z-02-mvp-plan.md, docs/A2Z-03-rebrand.md,
docs/A2Z-REPORT.md, AGENTS.md (rule "Keep it light" kekal dipatuhi untuk perubahan kod).

## A. BRANDING FINAL (buat dulu, cepat)
Keputusan Encik Koliji: app personal, BUKAN produk syarikat. Nama app: `Halo Island`.
Kredit/author guna nama GitHub beliau: `kolijijani`. JANGAN ada sebarang
nama syarikat lama atau © syarikat lama dalam repo (kod, metadata, installer, docs).

1. Buang SEMUA nama syarikat lama dari seluruh repo — termasuk docs/A2Z-*.md
   yang Pass 1 tulis. Ganti konteks:
   - CompanyName / publisher dalam `crates/poni/res/poni.rc`, `installer/poni.iss`,
     `installer/poni.nsi`, `tools/ci-installer.ps1` → `kolijijani`.
   - About / version card Settings (`settings_window.rs`, `version.rs`): tambah
     satu baris kecil "by kolijijani" (GitHub) supaya nama beliau nampak.
   - README nota fork → "Forked from Oyenza Studio as Halo Island by kolijijani".
   - docs/A2Z-03-rebrand.md + A2Z-REPORT.md: kemas kini supaya konsisten
     (CompanyName/publisher kolijijani, tiada nama syarikat lama).
2. Kekal: `LICENSE`, `THIRD-PARTY-NOTICES.txt`, copyright asal `Jatevo Labs`
   (atribusi MIT), internal identifiers (`poni.exe`, folder `Poni`, mutex, dsb),
   `.github/workflows` — JANGAN sentuh.
3. Selepas edit: grep nama syarikat lama (tidak sensitif huruf) MESTI kosong (kecuali mungkin dalam fail
   `BRIEF-*.md` lama — padam/neutralkan juga). Pastikan `cargo check --workspace`
   masih OK kalau ada perubahan .rs.

## B. VIDEO MOCKUP (deliverable utama)
Hasilkan mockup video Halo Island supaya Encik Koliji boleh nampak rupa & rasa
app sebelum production. App sebenar Windows-only, jadi bina mockup animasi web
yang realistik, kemudian render ke MP4.

1. Buat SATU fail `mockups/halo-island-mockup.html` self-contained (inline CSS+JS,
   tiada aset luar, tiada CDN). Gaya Windows 11 / Dynamic Island:
   - Latar desktop wallpaper (gradient lembut) yang bertukar dark→light→dark.
   - Island/notch hitam rounded di tengah atas, dengan window chrome halus.
   - Urutan scene animasi lancar (requestAnimationFrame, easing lembut):
     a. Intro card "Halo Island · by kolijijani" (fade in/out).
     b. Idle: island kecil dengan jam + tarikh, hover sikit (ripple/scale).
     c. Media: island expand ke player — cover art, tajuk lagu, progress bar
        bergerak, butang play/pause, waveform halus.
     d. Notification: masuk dari sisi, ikon + tajuk + isi, auto-dismiss.
     e. Timer: countdown 5 saat dalam island, progress ring.
     f. Dark/light toggle sekilas, kemudian idle semula + outro "Halo Island".
   - Teks guna jenama: Halo Island (bukan Oyenza/poni). Tiada logo syarikat lama.
   - Pastikan responsive untuk viewport 1920×1080, tiada scrollbar.

2. Render video di render-node:
   - Guna python3 + Playwright Chromium (dah ada) + ffmpeg (dah ada).
   - Viewport 1920×1080, deviceScaleFactor 1, total ~25-30 saat, fps 30.
   - Cara cadang: buka `mockups/halo-island-mockup.html`, jalankan animasi,
     screenshot frame-by-frame (atau guna video recording Playwright), kemudian
     encode H.264 + yuv420p dengan ffmpeg.
   - Output: `mockups/halo-island-mockup.mp4` dan poster `mockups/halo-island-poster.png`.

3. QA video (wajib, buktikan dalam laporan):
   - `ffprobe -v error -show_entries format=duration,size -show_entries stream=codec_name,width,height,avg_frame_rate mockups/halo-island-mockup.mp4`
   - Mesti: codec h264, 1920x1080, duration ~25-30s, size munasabah.
   - `sha256sum` MP4 dan poster.
   - Screenshot poster: pastikan bukan hitam/kosong (semak saiz fail + dimensi).

4. Tulis `docs/A2Z-06-video-mockup.md`: cara render, fail paths, ffprobe output,
   sha256, scene list, dan nota teknikal.

## C. KEMAS KINI LAPORAN
- `docs/A2Z-REPORT.md`: tambah bahagian Pass 2 (branding kolijijani, video mockup
  paths + sha256), dan ganti RINGKASAN TELEGRAM dengan versi terkini yang
  menyebut: branding `Halo Island · by kolijijani` (tiada nama syarikat lama), video mockup siap
  di `mockups/halo-island-mockup.mp4`, dan production masih ditahan untuk review.

## Guardrails
- Jangan commit/tag/push/release. Jangan sentuh .github, LICENSE, notices.
- Jangan ubah internal identifiers atau logik app selain teks branding dalam A.
- Kalau ffmpeg/Playwright ada masalah, cuba alternatif (record_video Playwright
  webm → ffmpeg transcode) dan catat dalam laporan. JANGAN berhenti tanpa hasil:
  sekurang-kurangnya MP4 mesti wujud.
