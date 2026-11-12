# A2Z-09 · Video penuh Halo Island: Free → Pro

Pass 3 (BRIEF-VIDEO-FULL.md): satu video mockup konsep yang menunjukkan maskot Halo dan semua feature Halo Island, FREE dahulu kemudian PRO, sebelum production build. Ini mockup, bukan rakaman app sebenar. Tiada kod app diubah.

## Fail

| Fail | Saiz | Nota |
|---|---|---|
| `mockups/halo-island-full.html` | 74,784 B | Self-contained (CSS+JS inline, tiada CDN/aset luar). `seek(t)` deterministik, `window.DURATION = 89.4`, `window.SCENES` (jadual scene). Buka terus dalam browser untuk preview live. |
| `mockups/render_full.py` | 4,863 B | Render HTML → MP4 + poster (Playwright Chromium + ffmpeg). |
| `mockups/halo-island-full.mp4` | 7,770,958 B (7.4 MiB) | Video akhir. |
| `mockups/halo-island-full-poster.png` | 813,462 B | Poster 1920×1080, frame t = 21.4 s. |

## Cara render

```sh
python3 mockups/render_full.py                 # MP4 + poster, 6 browser selari
python3 mockups/render_full.py --jobs 1        # satu browser, satu laluan (~60 min)
python3 mockups/render_full.py --stills 5,22,60 --stills-dir /tmp/halo   # semak frame sahaja
```

- Viewport 1920×1080, `device_scale_factor` 1. Setiap frame: `seek(i/30)` kemudian screenshot PNG, pipe terus ke `ffmpeg -f image2pipe -c:v png -i - -c:v libx264 -preset slow -crf 18 -pix_fmt yuv420p -movflags +faststart -r 30`. Tiada fail frame sementara.
- Satu browser pada mesin ini hanya ~0.75 frame/s (2,682 frame ≈ 60 min), jadi julat frame dibahagi kepada `--jobs` bahagian (lalai 6) yang dirender serentak, setiap satu ke MP4 sendiri dalam direktori sementara, kemudian disambung dengan concat demuxer `ffmpeg -f concat -c copy` (codec dan tetapan sama, tiada encode semula) menjadi satu `halo-island-full.mp4`. Bahagian sementara dipadam sendiri.
- `pageerror` dari mana-mana browser menghentikan render dengan ralat. Render akhir: **0 pageerror**.
- Masa render akhir: 19 min 02 s (6 bahagian, 8 core).

## Scene dan masa

Masa diterbitkan daripada jadual `SC` dalam HTML (tukar durasi satu baris, semua masa selepasnya ikut).

| Seg | Masa (s) | Scene | Isi |
|---|---|---|---|
| A | 0.00–4.00 | Intro | "Halo Island" · "by kolijijani" · "Semua feature · Free → Pro", maskot di atas tajuk (kerdip) |
| B | 4.00–14.00 | Maskot Halo | 1 Jaga + kerdip 120 ms (4.0–6.6) · 2 Mengantuk 12×4 turun 2 px (6.6–8.2) · 3 Tidur ∪ (8.2–10.2) · 4 Melambai ∩ + tangan ±20° dua kali, 1.2 s (10.2–11.6) · 5 Ikut pointer maks 2 px (11.6–14.0). Dark sehingga 9.6 s, kemudian light. Island atas cermin state yang sama. |
| C | 14.00–15.20 | Kad FREE | "10 feature asas · lengkap untuk kegunaan harian" |
| C | 15.20–18.80 | FREE 01 Island & Hover | Pill idle → hover 220 ms (bar kiraan) → kembang spring (jam, kalendar, cuaca) → kursor pergi → sorok ke pill |
| C | 18.80–23.20 | FREE 02 Music | Chip Spotify/Apple Music/YouTube/Browser, lirik sync karaoke, klik Romaji → Kana, klik skip → lagu YouTube |
| C | 23.20–26.70 | FREE 03 Focus Timer | 25:00 → 0:00 (dipercepat) dengan cincin progres → "Rehat 5 min" |
| C | 26.70–30.60 | FREE 04 To-do + Shelf | Dua tugasan ditanda siap; `laporan.pdf` diseret dari desktop ke shelf |
| C | 30.60–34.10 | FREE 05 Prayer Times | Asar 4:27 PM, "lagi 12/11 min", lima waktu, Kuala Lumpur |
| C | 34.10–37.70 | FREE 06 Battery & Peek | "Mengecas 82→83%" kemudian "Buds Pro disambung · L/R/Kes" |
| C | 37.70–41.60 | FREE 07 Themes | Dark → light ikut Windows; aksen bertukar melalui 5 tema (Halo, Lautan, Hutan, Senja, Mono); kembali dark |
| C | 41.60–45.10 | FREE 08 Widgets | 2 slot: Cuaca, Kalendar |
| C | 45.10–48.60 | FREE 09 Water Reminder | "Masa minum air", klik "Dah minum" → 5/8 gelas, titisan terisi |
| C | 48.60–52.20 | FREE 10 Clipboard | Ctrl+C → item baru masuk, item ke-6 keluar (had 5) |
| D | 52.20–53.40 | Kad PRO | "7 feature untuk power user · semua FREE kekal percuma" |
| D | 53.40–57.20 | PRO 01 Overlay Games | Game fullscreen borderless; island jadi overlay FPS + sparkline, GPU %/°C, CPU, RAM |
| D | 57.20–61.20 | PRO 02 System Monitor | 8 core, sejarah CPU 60 min, proses teratas, Power dalam W |
| D | 61.20–65.40 | PRO 03 Phone Drop | Telefon → PC (IMG_2041.jpg), kemudian PC → telefon (laporan.pdf); "mana-mana network" |
| D | 65.40–69.00 | PRO 04 Clipboard Unlimited | "2,184 item · tanpa had", taip "inv" → 3 hasil diserlahkan |
| D | 69.00–72.80 | PRO 05 Widgets Pro | 4 slot → 6 slot: Kalendar, World Clock (KL/Tokyo/London), Bulan, Countdown, Cuaca, Saham |
| D | 72.80–77.00 | PRO 06 Tema Premium + Theme Builder | Zamrud → Pasir → Bata (premium), kemudian builder: warna, bucu, kaca → "Teh Tarik" |
| D | 77.00–80.60 | PRO 07 Cloud Sync | Suis "pilihan" dihidupkan; tetapan bergerak PC Rumah → awan → PC Pejabat (Tema, Widget, Hotkey ✓) |
| E | 80.60–85.40 | Free vs Pro | Dua lajur: 10 FREE; PRO = "Semua dalam FREE, ditambah:" + 7. Kaki: "Free dah lengkap — Pro untuk power user." |
| E | 85.40–89.40 | Outro | "Halo Island" · "A living notch for Windows · by kolijijani" · "Free dah lengkap — Pro untuk power user.", maskot melambai |

Setiap feature ada tag FREE (hijau, garis) atau PRO (emas), nombor "NN / 10" atau "NN / 7", judul dan satu baris BM. Sepanjang segmen D island bertepi emas sebagai penanda premium. Rel A–E di kiri atas menunjukkan segmen semasa.

## Maskot (ikut A2Z-07 "Keputusan")

Dilukis sebagai SVG vektor pada grid notch 104×32: dua kapsul 8×16, `rx` = min(w,h)/2 (4 px), pusat mata ±9 px (jarak 10 px), berpusat. Mengantuk 12×4 turun 2 px (peralihan 600 ms); Tidur dua lengkung ∪ garis 2.5 px hujung bulat; Melambai lengkung ∩ + tangan kapsul 4×9 di kanan, ±20° dua kali dalam 1.2 s; Ikut pointer beralih maks 2 px ke arah kursor. Palet dark pill `#0B0D12` / mata `#F2F5FA`, light pill `#F3F5F9` (tepi `#D5DAE3`) / mata `#141821`. **Tiada cincin, glow, mulut atau aksesori** di mana-mana scene (intro, segmen B, island, tema premium, outro). Geometri sama seperti `mockups/render_mascot_eyes.py`.

## QA

### ffprobe

```
$ ffprobe -v error -show_entries format=duration,size -show_entries stream=codec_name,width,height,avg_frame_rate mockups/halo-island-full.mp4
[STREAM]
codec_name=h264
width=1920
height=1080
avg_frame_rate=30/1
[/STREAM]
[FORMAT]
duration=89.400000
size=7770958
[/FORMAT]
```

Tambahan: `profile=High`, `pix_fmt=yuv420p`, `nb_frames=2682`, dan `-count_frames` membaca 2,682 frame (= 89.4 × 30, tiada frame hilang di sambungan concat). Atom `moov` sebelum `mdat` (faststart berkesan).

### sha256

```
849a8d4e197b2b882223297ca098a7ff8d68d307bfe6506ca4edd23a8bf04dce  mockups/halo-island-full.mp4
7ca1e9806965a7ed5fa373d206bc7e9dc49c3b8822c8df17f9c369e499ac4154  mockups/halo-island-full-poster.png
```

### Contact sheet (12 frame, sebar rata)

Frame n = (i + 0.5) × 2682 / 12, diambil dari MP4 akhir (`select=eq(n,…)`, `tile=4x3`):

| # | t (s) | Seg | Dilihat |
|---|---|---|---|
| 1 | 3.73 | A→B | Kad intro sedang pudar ke desktop, island masuk |
| 2 | 11.17 | B | "4 · Melambai · wave", light, mata ∩ + tangan |
| 3 | 18.63 | C | Island & Hover: island kembali ke pill, caption pudar keluar |
| 4 | 26.07 | C | Focus Timer "Rehat 5 min" |
| 5 | 33.53 | C | Prayer Times, "lagi 11 min" |
| 6 | 40.97 | C | Themes, light, tema Mono dipilih |
| 7 | 48.43 | C | Water Reminder pudar keluar |
| 8 | 55.87 | D | Overlay Games atas game |
| 9 | 63.33 | D | Phone Drop PC → telefon |
| 10 | 70.77 | D | Widgets Pro 6 slot |
| 11 | 78.23 | D | Cloud Sync sedang menyegerak |
| 12 | 85.67 | E | Outro sedang muncul |

Kesemua segmen A–E muncul pada masa jadual. Tiada teks terpotong (disemak juga pada 31 still penuh, satu atau lebih bagi setiap scene, sebelum render). `blackdetect` (d=0.1, pix_th=0.05) hanya mengesan 0–0.73 s: fade-in dari hitam di awal video, disengajakan. Celah hitam 3.3–3.7 s antara intro dan desktop dalam render pertama dibetulkan (kad intro kini crossfade terus ke desktop) dan video dirender semula.

### Poster

`halo-island-full-poster.png`: 1920×1080, 813,462 B, purata RGB (30.4, 33.2, 34.2), bukan hitam. Scene: **FREE 02 Music** pada 21.4 s: island kembang dengan kulit album, "Senja di Pantai Timur · Lofi Sessions · Spotify", chip sumber, lirik Jepun sync karaoke dengan baris kana di bawah, suis Romaji/Kana, kawalan main, dan caption FREE "Music".

## Nota teknikal

- **Deterministik:** setiap frame fungsi tulen `t` (tiada `Date`, `Math.random` atau animasi CSS). Hingar meter (FPS, core, watt) ialah jumlah sinus. Preview live guna `requestAnimationFrame` memanggil `seek` yang sama; `?capture` mematikan gelung untuk render.
- **Font:** Outfit (dipasang pada sistem; fallback Segoe UI). Lirik Jepun jatuh ke Noto Sans CJK JP. Pada mesin tanpa Outfit, rupa teks akan berbeza; HTML tidak membenam font supaya kekal kecil.
- **Jenama:** hanya "Halo Island" dan "kolijijani" dalam kandungan visual. Tiada jenama upstream/Oyen/poni.
- **Freemium:** kad PRO dan lajur PRO menyebut "semua FREE kekal percuma" / "Semua dalam FREE, ditambah:"; tiada feature FREE dikunci dalam video.
- Fail lama (`mockups/halo-island-mockup.*`, `mockups/render_mockup.py`, `crates/poni-core/src/mascot.rs`, `docs/A2Z-01..08`, BRIEF lama) tidak disentuh. Tiada commit, tag, push atau release.
- Angka feature dalam video (12 tema premium, 2,184 item clipboard, 8 core, watt) ialah contoh mockup, bukan spesifikasi.
