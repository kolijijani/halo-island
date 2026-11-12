# A2Z-06 — Video mockup Halo Island

Tarikh: 11 Nov 2026 · Pass 2 · Tujuan: Encik Koliji nampak rupa dan rasa Halo Island sebelum production build.

## Fail

| Fail | Saiz | Kegunaan |
|---|---|---|
| `mockups/halo-island-mockup.html` | 24 KB | Mockup web self-contained (CSS + JS inline, tiada CDN/aset luar). Buka terus dalam browser: animasi main berulang dengan `requestAnimationFrame`. |
| `mockups/render_mockup.py` | 2 KB | Render HTML ke MP4 + poster (Playwright Chromium + ffmpeg). |
| `mockups/halo-island-mockup.mp4` | 2.1 MB | Video 28 s, 1920×1080, 30 fps, H.264 yuv420p, `+faststart`. |
| `mockups/halo-island-poster.png` | 704 KB | Poster 1920×1080 (scene media player, t = 9.5 s). |

## Cara render

```
python3 mockups/render_mockup.py              # --fps 30 --poster-at 9.5 (default)
```

Skrip buka HTML dengan `?capture` (animasi live dimatikan), kemudian untuk setiap frame `i` panggil `seek(i/30)` dalam page dan ambil screenshot PNG 1920×1080 (`deviceScaleFactor` 1). PNG di-pipe terus ke `ffmpeg -c:v libx264 -preset slow -crf 18 -pix_fmt yuv420p -movflags +faststart`. Tiada fail frame sementara.

Masa render di node ini: 17 min 34 s untuk 840 frame (kebanyakan masa ialah screenshot Chromium headless).

## QA

```
$ ffprobe -v error -show_entries format=duration,size -show_entries stream=codec_name,width,height,avg_frame_rate mockups/halo-island-mockup.mp4
[STREAM]
codec_name=h264
width=1920
height=1080
avg_frame_rate=30/1
[/STREAM]
[FORMAT]
duration=28.000000
size=2145007
[/FORMAT]
```

Juga: `pix_fmt=yuv420p`, `nb_frames=840`.

```
$ sha256sum mockups/halo-island-mockup.mp4 mockups/halo-island-poster.png
9ae64da721162a15f6eb04d71f5d28ef575889d2f4a723a51d5630d8836a26cb  mockups/halo-island-mockup.mp4
0d711e23ab847d0320b57e19e39db89c7c90f7e3462ac9170fb527a6e16b39f0  mockups/halo-island-poster.png
```

Poster: PNG 1920×1080, 720 716 bait (bukan hitam/kosong: island media player, wallpaper, window Notepad dan taskbar kelihatan). Contact sheet 10 frame dari MP4 (setiap 2.8 s) disemak secara visual: setiap scene di bawah muncul pada masa yang betul. Tiada `pageerror` semasa render.

## Scene (28 s)

| Masa | Scene | Apa berlaku |
|---|---|---|
| 0.0–3.1 s | a. Intro | Kad "Halo Island · by kolijijani" fade in/out atas latar gelap |
| 3.0–7.4 s | b. Idle | Island turun dengan spring, jam `9:41` + `Wed 11 Nov`; kursor datang, hover: island membesar 6% + ripple |
| 7.4–13.4 s | c. Media | Klik, island expand ke player: cover art, "Golden Hour Drive", progress bar bergerak, waveform; klik pause (waveform reda), klik play semula |
| 13.4–17.5 s | Mini player | Island mengecil ke pill now-playing dengan waveform kecil |
| 14.2–17.5 s | d. Notification | Kad "Team sync in 10 minutes" masuk dari kanan, island bercahaya sekilas, bar masa habis, auto-dismiss |
| 17.5–23.4 s | e. Timer | Countdown 0:05 → 0:01 dengan progress ring, kemudian "Time's up" (ring berdenyut) |
| 23.4–25.8 s | f. Dark/light | Pill "Light mode": wallpaper, taskbar, window dan island tukar ke light, kemudian kembali dark |
| 25.8–28.0 s | Idle + outro | Island kembali ke jam; kad "Halo Island · A living notch for Windows · by kolijijani" |

## Nota teknikal

- Setiap frame ialah fungsi tulen `seek(t)`: preview live dan render frame-demi-frame sama tepat, tiada frame tercicir walaupun mesin perlahan. Durasi dibaca dari `window.DURATION` (28).
- Saiz pentas tetap 1920×1080, di-scale ikut viewport (`fit()`), `overflow: hidden` jadi tiada scrollbar.
- Font: `Outfit` (font app sebenar, `crates/poni/res/fonts`) jika terpasang di sistem, fallback Segoe UI / system-ui. Node ini ada Outfit, jadi video guna font yang sama dengan app.
- Bentuk island (lebar/tinggi/radius) bertukar dengan spring ~9% overshoot; lapisan kandungan cross-fade dengan sedikit scale.
- Jenama: hanya "Halo Island" dan "kolijijani". Tiada jenama upstream/poni/Oyen dalam kandungan visual.
- Ini mockup konsep, bukan rakaman app sebenar (app Windows-only, tiada Windows di node). Ukuran dan rupa mengikut gaya island app, tetapi pixel sebenar akan berbeza sedikit.
- Playwright `record_video` tidak diperlukan; laluan screenshot + ffmpeg berjaya pada cubaan pertama.
