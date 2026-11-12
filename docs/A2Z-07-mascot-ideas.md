# A2Z-07 — Cadangan maskot Halo Island

Maskot lama (Oyen, kucing krim) kekal milik upstream (A2Z-03, K5). Dokumen ini mencadangkan 6 maskot baru yang original untuk Halo Island oleh kolijijani.

**Arah (dibetulkan, 11 Nov 2026):** bukan pixel-art. Konsepnya sama macam Nusa: sepasang mata minimalis yang **licin** (vektor, lengkung halus, tepi anti-alias) yang muncul dalam notch bila idle, mengantuk lalu terlelap selepas beberapa saat, dan melambai bila pengguna kembali. Halo Island menambah satu elemen jenama: **halo**, cincin atau lengkung cahaya nipis.

Mockup: `mockups/halo-island-mascot-nusa-style.png` (3 calon terbaik, dark dan light, state awake / doze / wave).

## Syarat bersama

- **Bentuk:** hanya primitif geometri licin: kapsul (rounded rect dengan radius = separuh lebar), bulatan, elips, lengkung (arc) dengan hujung bulat, bentuk lensa/sabit hasil tolak dua bulatan. Tiada blok piksel, tiada sudut tajam, tiada tekstur.
- **Saiz:** mesti terbaca dalam notch setinggi 24–32 px. Mata 6–10 px tinggi, jarak antara mata ≈ 1.2× lebar mata. Halo garis 1.5–2.5 px. Lukis sebagai vektor (Direct2D geometry / Core Graphics path), bukan bitmap, supaya licin pada setiap skala DPI.
- **Warna:** 1 warna mata + 1 warna halo + warna pill. Glow halo ialah blur lembut warna halo pada 30–40% alpha; matikan glow pada tema light (cukup garis).
- **Bukan Oyen:** tiada telinga, misai, ekor; elak krim/oren Oyen.
- **Tiada mulut.** Emosi datang dari bentuk mata, kedudukan halo dan satu "tangan" kapsul kecil untuk lambai.

## Kitar idle (semua calon)

| State | Bila | Apa berlaku | Kos |
|---|---|---|---|
| **Jaga** (awake) | Island idle, tiada media/notifikasi | Mata terbuka, kerdip sekali setiap 4–6 s | Animasi; berhenti bila masuk Mengantuk |
| **Mengantuk** (doze) | 8 s tanpa input | Kelopak turun separuh, halo condong dan malap | Satu transisi 600 ms, kemudian statik |
| **Tidur** (sleep) | 20 s tanpa input | Mata jadi garis lengkung, halo turun dan padam ke 30% | Frame statik. Tiada redraw sehingga input |
| **Melambai** (wave) | Input pertama selepas Mengantuk/Tidur | Mata gembira (∩), tangan kapsul melambai 2×, halo melantun | One-shot 1.2 s, kemudian Jaga |
| **Ikut pointer** | Kursor atas island | Mata (atau pupil) beralih maks 2 px ke arah kursor | Hanya pada `WM_MOUSEMOVE` yang sampai ke window, bukan poll |

**Kos (AGENTS.md "Keep it light"):** maskot hanya bergerak bila island kelihatan. Tidur ialah frame statik, jadi island yang ditinggalkan tidak melukis apa-apa. Semua animasi berhenti terus bila `App::locked`, `App::display_off` atau `st.power.saver`. Pemasa kerdip dan pemasa 8 s / 20 s ialah satu `SetTimer` one-shot yang dijadual semula pada input, bukan tick berterusan.

## Ringkasan

| # | Nama | Mata | Halo | Mockup |
|---|---|---|---|---|
| 1 | **Halo** | Dua kapsul tegak | Cincin elips terapung di atas mata | Ya |
| 2 | **Gelang** | Dua titik bulat | Cincin elips yang mengelilingi mata | Ya |
| 3 | **Sabit** | Dua lensa (badam) | Lengkung separuh cincin di atas | Ya |
| 4 | **Titis** | Dua bulatan besar + kilau | Glow lembut di bawah, seperti bayang cahaya | — |
| 5 | **Orbit** | Dua kapsul pendek | Satu titik cahaya yang mengorbit pada laluan elips | — |
| 6 | **Redup** | Dua squircle (skrin kecil) | Garis cahaya nipis di bawah mata | — |

---

## 1. Halo

**Personaliti:** Tenang, setia, sedikit malu. Penjaga island. Paling dekat dengan rasa Nusa: sepasang mata dan satu tanda jenama.

**Rupa:** Dua kapsul tegak, 8×16 px pada notch 32 px (radius 4 px, hujung bulat penuh), jarak 10 px. Cincin elips nipis (32×7 px, garis 2 px) terapung 4 px di atas mata dengan glow lembut. Silhouette: `▯ ▯` dengan `◯` pipih di atas.

**Kelakuan idle:**
- **Jaga:** kerdip dengan kapsul memendek ke 2 px tinggi dan kembali (120 ms). Cincin berayun ±1 px sangat perlahan hanya semasa Jaga.
- **Mengantuk:** kapsul jadi kapsul mendatar 12×4 px (kelopak separuh), cincin condong 12° dan malap ke 60%.
- **Tidur:** mata jadi dua lengkung ∪ nipis, cincin turun dan menyentuh kepala, 30% terang. Statik.
- **Melambai:** mata jadi lengkung ∩ (gembira), tangan kapsul kecil muncul di kanan dan condong ±20° dua kali, cincin melantun 3 px ke atas.
- **Ikut pointer:** kedua kapsul beralih maks 2 px ke arah kursor; cincin ikut 1 px (terasa ada kedalaman).

**Palet:**

| Peranan | Dark | Light |
|---|---|---|
| Pill | `#0B0D12` | `#F3F5F9` |
| Mata | `#F2F5FA` | `#141821` |
| Halo | `#7FD4FF` + glow 35% | `#1E88C8` (tiada glow) |
| Tangan | `#F2F5FA` | `#141821` |

**Kenapa "Halo":** halo literal di atas kepala; cincin itu boleh berdiri sendiri sebagai logo dan ikon tray.

---

## 2. Gelang

**Personaliti:** Ingin tahu, cepat teruja. Suka "memeluk" apa saja yang masuk ke island dalam cincinnya.

**Rupa:** Dua titik bulat (diameter 8 px, jarak 10 px) di dalam cincin elips (44×24 px, garis 2 px). Cincin ialah "muka"; mata terapung bebas di dalamnya. Silhouette: `( • • )`.

**Kelakuan idle:**
- **Jaga:** mata kerdip (bulatan jadi elips 8×2), cincin bernafas: skala 100↔103% sekali setiap kerdip.
- **Mengantuk:** mata jadi separuh bulatan (atas dipotong mendatar), cincin mengecil ke 92% dan malap.
- **Tidur:** mata jadi lengkung ∪, cincin rebah jadi elips pipih (44×10) seperti bantal. Statik.
- **Melambai:** cincin condong 15° seperti kepala dimiringkan, mata ∩, tangan kapsul keluar dari tepi cincin.
- **Ikut pointer:** titik mata bergerak dalam cincin (maks 3 px), cincin diam. Paling ekspresif untuk ikut pointer kerana ada "bingkai" sebagai rujukan.

**Palet:**

| Peranan | Dark | Light |
|---|---|---|
| Pill | `#0B0D12` | `#F3F5F9` |
| Mata | `#F2F5FA` | `#141821` |
| Halo (cincin) | `#6EF0C2` + glow 30% | `#0F9E74` |
| Tangan | `#6EF0C2` | `#0F9E74` |

**Kenapa "Halo":** maskot ini duduk di dalam halo; bentuk cincin dan pill hampir sama.

---

## 3. Sabit

**Personaliti:** Lembut, sedikit mengantuk secara semula jadi, suka malam. Inspirasi tempatan: anak bulan.

**Rupa:** Dua mata lensa (badam): persilangan dua bulatan, 12×9 px, hujung runcing tapi licin. Di atas, lengkung separuh cincin (arc 200°, lebar 30 px, garis 2 px, hujung bulat) seperti bulan sabit terlentang.

**Kelakuan idle:**
- **Jaga:** lensa kerdip dengan memipih ke 12×2, lengkung halo kekal.
- **Mengantuk:** lensa jadi 12×4, lengkung halo berputar 15° dan malap. Mata paling "mengantuk" secara semula jadi di antara semua calon.
- **Tidur:** lensa jadi garis lengkung ∪, halo berputar jadi sabit di sisi, 30% terang. Statik.
- **Melambai:** mata jadi sabit gembira (bulatan ditolak bulatan dari bawah), tangan kapsul melambai, halo melantun.
- **Ikut pointer:** lensa beralih 2 px; halo berputar sedikit ke arah kursor.

**Palet:**

| Peranan | Dark | Light |
|---|---|---|
| Pill | `#0B0D12` | `#F3F5F9` |
| Mata | `#F2F5FA` | `#141821` |
| Halo | `#FFE27A` + glow 30% | `#B98A00` |
| Tangan | `#F2F5FA` | `#141821` |

Nota: kuning lemon sejuk, jauh daripada krim/oren Oyen.

**Kenapa "Halo":** halo bulan ialah cincin cahaya di langit malam, dan sabit berkongsi lengkung pill.

---

## 4. Titis

**Personaliti:** Comel, ekspresif, mudah terharu. Paling "watak kartun" dalam senarai ini.

**Rupa:** Dua bulatan besar (diameter 11 px, jarak 6 px) dengan titik kilau kecil (3 px) di kiri atas setiap mata. Halo bukan cincin di atas, tetapi glow elips lembut di bawah mata (36×6 px, blur 4 px) seperti cahaya yang memantul pada permukaan.

**Kelakuan idle:**
- **Jaga:** kerdip; titik kilau kekal di tempat (ikut kelopak), memberi rasa basah dan hidup.
- **Mengantuk:** kelopak turun sebagai potongan mendatar licin, kilau hilang, glow bawah malap.
- **Tidur:** mata jadi lengkung ∪ tebal, glow padam ke 20%. Statik.
- **Melambai:** mata ∩ dengan kilau kekal, glow bawah berdenyut sekali, tangan kapsul.
- **Ikut pointer:** kilau bergerak bertentangan arah kursor (1 px) dan mata ke arah kursor (2 px): ilusi kedalaman yang murah.

**Palet:**

| Peranan | Dark | Light |
|---|---|---|
| Pill | `#0B0D12` | `#F3F5F9` |
| Mata | `#F2F5FA` | `#141821` |
| Kilau | `#0B0D12` 70% | `#FFFFFF` |
| Halo (glow bawah) | `#FF8FA3` 40% | `#D9466A` 25% |

**Kenapa "Halo":** halo sebagai cahaya lembut, bukan cincin. Sesuai jika jenama mahu lebih hangat.

---

## 5. Orbit

**Personaliti:** Sibuk, rajin, sentiasa memerhati. Penjaga notifikasi: titik orbit itu "berita" yang mengelilinginya.

**Rupa:** Dua kapsul pendek (8×12 px, radius 4 px). Satu titik cahaya (4 px) bergerak pada laluan elips (40×14 px) di sekeliling mata; laluan itu sendiri dilukis sangat samar (garis 1 px, 15% alpha). Halo tersirat dari pergerakan titik.

**Kelakuan idle:**
- **Jaga:** titik mengorbit satu pusingan (2 s) bila maskot muncul, kemudian berhenti di atas kepala. Bukan gelung tanpa henti.
- **Mengantuk:** titik meluncur perlahan ke sisi kiri dan berhenti; kapsul memendek.
- **Tidur:** titik hinggap di "bahu" kanan dan malap; mata ∪. Statik.
- **Melambai:** titik membuat satu pusingan pantas (600 ms), mata ∩, tangan kapsul.
- **Ikut pointer:** titik bergerak pada laluan orbit ke sudut terdekat dengan kursor; mata beralih 2 px.

**Palet:**

| Peranan | Dark | Light |
|---|---|---|
| Pill | `#0B0D12` | `#F3F5F9` |
| Mata | `#F2F5FA` | `#141821` |
| Titik orbit | `#CFE3FF` + glow 40% | `#3D6FB8` |
| Laluan | `#CFE3FF` 15% | `#3D6FB8` 20% |

**Kenapa "Halo":** halo ialah laluan, bukan objek; menarik tetapi kos animasi paling tinggi (orbit = banyak frame). Hadkan kepada satu pusingan setiap state.

---

## 6. Redup

**Personaliti:** Tenang, profesional, minimal. Untuk pengguna yang mahu maskot hampir tidak kelihatan.

**Rupa:** Dua squircle (10×10 px, radius 4 px, superelips) seperti dua skrin kecil yang menyala. Di bawah, satu garis cahaya kapsul nipis (28×2 px) seperti pantulan atau "dagu".

**Kelakuan idle:**
- **Jaga:** squircle kerdip dengan skala menegak ke 2 px; garis bawah tetap.
- **Mengantuk:** squircle jadi kapsul mendatar 10×3 px, garis bawah memendek ke 18 px.
- **Tidur:** squircle jadi dua titik 3 px, garis bawah padam. Statik. Hampir tiada piksel menyala.
- **Melambai:** squircle membulat penuh dan melantun, garis bawah melengkung jadi senyum (arc), tangan kapsul.
- **Ikut pointer:** kedua squircle beralih 2 px, garis bawah ikut 1 px.

**Palet:**

| Peranan | Dark | Light |
|---|---|---|
| Pill | `#0B0D12` | `#F3F5F9` |
| Mata | `#F2F5FA` | `#141821` |
| Halo (garis bawah) | `#B8F25A` + glow 25% | `#5E9A00` |

**Kenapa "Halo":** halo sebagai garis cahaya. Paling ringan dan paling neutral.

---

## Cadangan

- **Pilihan utama: Halo (1).** Paling dekat dengan konsep Nusa yang Encik Koliji mahu (sepasang mata licin), dan cincin di atasnya ialah tanda jenama yang sama dengan nama app. Satu aset vektor untuk maskot, logo dan ikon tray.
- **Alternatif ekspresif: Gelang (2).** Ikut pointer paling jelas kerana mata ada bingkai.
- **Alternatif lembut: Sabit (3).** Mengantuk dan tidur paling semula jadi.
- Elak Orbit (5) sebagai pilihan utama kerana kos animasi.

Langkah seterusnya (bila maskot dipilih): lukis path vektor sebenar pada 24/28/32 px notch, uji atas pill dark dan light dalam app, ukur kos frame state Jaga/Melambai, kemudian ganti aset Oyen (K5 dalam A2Z-02).

---

## Keputusan

**11 Nov 2026, Encik Koliji:** maskot rasmi Halo Island ialah **Halo (1), versi mata sahaja**. Cincin elips di atas kepala dibuang. Tiada halo, tiada gelung, tiada aksesori lain: hanya sepasang mata kapsul licin gaya Nusa. Nama "Halo" kekal sebagai nama maskot dan app; ia tidak lagi dilukis sebagai cincin.

Mockup: `mockups/halo-island-mascot-eyes-only.png` (dark dan light, 5 state, saiz 5× dan saiz sebenar 1×). Dilukis oleh `mockups/render_mascot_eyes.py` (Pillow, supersampling 4×, downscale LANCZOS).

### Spesifikasi akhir

**Bentuk (notch 32 px):** dua kapsul tegak **8×16 px**, radius penuh (4 px), jarak **10 px** antara mata, berpusat dalam pill. Lukis sebagai vektor (Direct2D geometry / Core Graphics path), bukan bitmap. Tiada mulut; emosi datang dari bentuk mata dan satu tangan kapsul kecil semasa lambai.

| State | Bentuk mata | Bila / tempoh |
|---|---|---|
| **Jaga** (awake) | Kapsul tegak 8×16 | Island idle; kerdip sekali setiap 4–6 s |
| **Kerdip** (blink) | Kapsul memendek ke 8×2, kembali | 120 ms, sebahagian daripada Jaga |
| **Mengantuk** (doze) | Kapsul mendatar 12×4 (kelopak separuh), turun 2 px | 8 s tanpa input; transisi 600 ms, kemudian statik |
| **Tidur** (sleep) | Dua lengkung ∪ nipis (garis 2.5 px, hujung bulat) | 20 s tanpa input; frame statik, tiada redraw sehingga input |
| **Melambai** (wave) | Lengkung ∩ gembira + tangan kapsul 4×9 di kanan, condong ±20° dua kali | Input pertama selepas Mengantuk/Tidur; one-shot 1.2 s, kemudian Jaga |
| **Ikut pointer** | Kedua kapsul beralih maks 2 px ke arah kursor | Hanya pada `WM_MOUSEMOVE` yang sampai ke window, bukan poll |

**Palet:**

| Peranan | Dark | Light |
|---|---|---|
| Pill | `#0B0D12` | `#F3F5F9` (garis tepi `#D5DAE3`) |
| Mata | `#F2F5FA` | `#141821` |
| Tangan | `#F2F5FA` | `#141821` |

Tiada warna halo dan tiada glow: satu warna mata atas warna pill.

**Kos:** sama seperti "Kitar idle" di atas. Tanpa cincin, Jaga hanya melukis semasa kerdip (tiada ayunan ±1 px), jadi island idle lebih jarang redraw. Tidur statik; semua animasi berhenti bila `App::locked`, `App::display_off` atau `st.power.saver`.

**Langkah seterusnya:** path vektor pada notch 24/28/32 px, uji dalam app atas pill dark dan light, ukur kos frame Jaga/Melambai, kemudian ganti aset Oyen (K5 dalam A2Z-02). Ikon tray dan logo perlu reka semula kerana cincin tidak lagi menjadi tanda jenama.
