# Redesign 0.3.00, fasa 1: Water + Waktu solat

Status: dibuat (15 Nov 2026). Skop: dua paparan dalam island sahaja. Notch luar (frame, header, tab, expand/collapse) tak disentuh. Rujukan: `prayer.png`, `notch-animation.mp4`.

## 1. Satu trigger, satu spring

- **Trigger** (`.ly.play`): bila mod island bertukar ke `Expanded(page)` (buka dari tertutup, atau tukar tab), app simpan `st.ui.entrance = (page, now)`. Satu masa mula untuk semua bahagian; tiap bahagian ada delay kecil sendiri (stagger).
- **Spring**: `halo_core::spring::bouncy_at(t)`: bentuk tertutup `Spring::bouncy` (420/30, zeta 0.73) dari 0 ke 1. Overshoot ~3.4%, sama dengan `cubic-bezier(0.34, 1.32, 0.5, 1)` (puncak 1.034). Mendarat dalam 0.6 s. Tiada state untuk di-step: kedudukan dikira daripada masa sahaja.
- **Draw** (arc, bar): `ease_out_cubic`, tak pernah lepas 1 (setara `.3,.7,.2,1` / `.2,.85,.25,1`).
- Modul: `pages/entrance.rs` (`Entrance::of`, `pop`, `draw`, `rise`, `scaled`).

## 2. Kos (AGENTS.md "Keep it light")

- `request_anim()` hanya bila `t < END` (1.1 s selepas trigger). Lepas itu tiada frame tambahan. Tiada timer, thread atau poll baharu.
- Tiada entrance bila Windows "Animation effects" off / macOS Reduce Motion, atau battery saver: app tak set trigger, jadi paparan terus di state akhir.
- Teks dan angka **tidak** bergerak, tidak fade dan tidak count-up: dilukis tepat di tempat akhir dari frame pertama. Hanya bentuk (kad, ring, bar, titik, butang ikon) yang beranimasi.
- Self-test: `selftest_settled` tunggu entrance habis, supaya gambar laman tak terambil separuh jalan.

## 3. Water (`pages/widgets.rs::draw_water`)

Susun atur (kad 230×52 sama, supaya Clipboard tak hilang satu baris):

| Bahagian | Dulu | Sekarang | Entrance |
|---|---|---|---|
| Kad | `SURFACE` | sama | `rise` (fade + naik 8 DIP, spring), 0 s |
| Ring | r12 w4 | r14 w4, track + arc cyan | arc draw 0→n/goal, 0.08 s, 0.6 s |
| Teks | "Water", "5/8 glasses" | "Water · 63%" (atau "Goal reached" cyan), nombor besar "5" + "/ 8 glasses" | tiada |
| −, + | label teks | ikon `Minus`/`Plus` | pop scale 0→1, 0.14 s / 0.20 s |

## 4. Waktu solat (`pages/prayer.rs`)

| Bahagian | Perubahan | Entrance |
|---|---|---|
| Bar progres (baharu) | Bawah kiraan detik: masa yang sudah berlalu dari waktu solat sebelumnya ke waktu seterusnya (`prayer::current_prayer` → `next_prayer`), data sebenar. Tiada waktu sebelumnya = tiada bar. | bar draw, 0.10 s, 0.55 s |
| Tempat | turun 4 DIP untuk beri ruang bar | tiada |
| Garis arc bertitik | bahagian hari yang sudah lalu dalam warna accent | titik muncul kiri→kanan (arc draw), 0.05 s, 0.6 s |
| 5 titik solat + cincin "next" | sama | pop, stagger 0.15 + 0.06·i s |
| Matahari / bulan | sama | pop, 0.45 s |
| Nama, waktu, Hijri, info, footer | sama | tiada (teks) |

## 5. Fasa 2: semua paparan lain (15 Nov 2026)

`entrance::PAGES` dibuang: setiap paparan kini dapat trigger. Paparan tanpa bahagian bergerak tidak panggil `Entrance::of`, jadi tiada frame diminta. Peraturan sama: bentuk sahaja, teks dan angka tepat dari frame pertama, tiada entrance bila reduced motion / battery saver.

| Paparan | Entrance | Reka bentuk |
|---|---|---|
| Today (berita + kalendar) | bulatan hari ini pop 0.12 s; garis accent bawah tajuk berita draw 0.05 s / 0.6 s | garis accent nipis bawah "TOP STORIES" |
| Media | art pop 0 s; bar progres draw ke posisi lagu 0.12 s / 0.6 s; butang play pop 0.2 s | sama |
| Focus | ring draw ke fasa 0.05 s / 0.7 s; titik kitaran pop, stagger 0.25 + 0.05·i s | sama |
| Tasks | jalur tugasan selesai rise + checkbox pop, stagger 0.06 + 0.05·i s; ring footer draw | tugasan selesai atas jalur hijau pudar |
| Shelf | tile rise, stagger 0.04·i s; zon kosong rise, ikon tray pop | sama |
| Stocks | garis carta draw kiri→kanan (clip) 0.1 s / 0.7 s | sama |
| Caffeine | medan titik draw kiri→kanan 0.05 s / 0.7 s; titik minuman pop, stagger | sama |
| Widgets (tile) | tile rise stagger 0.06·i s; bulan pop | sama |
| System | ring, gauge, cas bateri draw 0.08 s / 0.7 s | sama |
| Games (0.2.53, tab sendiri) | gelang FPS + gauge GPU/suhu draw 0.08 s / 0.7 s; carta FPS draw kiri→kanan (clip) 0.15 s / 0.7 s | tetapan kiri, carta tengah, game di depan kanan |
| AI usage | kad rise stagger; bar had draw 0.12 s / 0.6 s | sama |
| Add-ons, Downloads, Notifications, Settings, Record | lencana/ikon pop, stagger per baris; trek Record rise; titik merah rakaman pop | sama |
| Keadaan kosong (`pages::empty_state`, 5 paparan) | ikon pop 0.05 s | sama |

Notch luar (frame, header, tab, expand/collapse) tidak disentuh.

## 6. Fasa 3: semua tab lain (0.2.53, 16 Nov 2026)

Semua tab kecuali Music, Widgets dan Today (sudah okay) dapat reka bentuk dan animasi lebih. Peraturan sama: bentuk sahaja, teks dan angka tepat dari frame pertama, tiada entrance bila reduced motion / battery saver. Alat baharu: `Entrance::at(ui, t0)` (main semula entrance dari masa sendiri, contoh System bila modul dipilih). Animasi ambien (`entrance::alive`) hanya di Focus (halo bernafas bila sesi berjalan) dan Record (cincin dari titik merah bila merakam): ~30 fps 10 s selepas tab dibuka, berhenti bila kunci / skrin mati, tiada dalam self-test.

| Paparan | Entrance / interaksi | Reka bentuk |
|---|---|---|
| System | cip strip rise stagger 0.04·i s, sparkline draw kiri→kanan; lajur cascade ikut tinggi (`cascade`: 0.08 + 0.27·y/H s): garis pemisah tumbuh dari tajuk, grid + carta draw kiri→kanan, bar naik kiri→kanan, titik + ikon apl pop; jarum tekanan memori berpusing | pilihan modul meluncur (spring) ke cip baharu dan lajur draw semula; hover cip fade 0.16 s; garis accent di kaki cip terpilih |
| Games | sama dengan System (bahagian dikongsi) | sama |
| Shelf | thumbnail pop selepas tile; pil Clear/Phone pop; tile Phone rise | hover fade, thumbnail naik 4%, butang buang pop, tekan = dip 0.97 |
| Tasks | pil footer pop | hover baris fade, butang padam pop; ripple hijau dari ring bila tugasan terakhir hari ini selesai |
| Focus | picker rise, Start/Reset/Skip pop stagger, titik di hujung arc pop | cakera warna fasa dalam ring; halo bernafas (alive) bila berjalan; hover segmen fade |
| AI | butang atas pop; kad rise + shine, logo pop, bar had draw baris demi baris, bar hari naik kiri→kanan; baris Services rise stagger | sorotan atas kad |
| Script | penanda baris pop, garis kemajuan draw, butang bar pop stagger | penanda accent di sebelah baris dibaca; garis nipis atas bar = sejauh mana dibaca |
| Record | cip input pop stagger, Record pop; Pause/Stop pop | pilihan sasaran meluncur (spring); hover fade; cincin dari titik merah (alive) |
| Prayer | pil footer pop; cahaya di hujung bar menunggu pop | hover baris pemilih bandar fade + tanda accent |
| Stocks | baris rise stagger, garis bawah draw, sparkline draw kiri→kanan, pil perubahan pop | sama |
| Caffeine | baris senarai rise stagger, Add pop; cip jenis minuman pop baris demi baris bila sheet dibuka / langkah bertukar | hover baris fade |
| Downloads | Paste/Download pop, baris rise + garis + bar draw, Clear pop | lencana hijau + tanda bila siap |
| Notifications | loceng pop dan berayun sekali, butang pop, garis draw, suis rise stagger | sama |
| Settings | baris draw in stagger; kad lembaran Update rise + shine; emoji lembaran About pop; tag dan pil halaman Pro pop | latar baris hover fade (`settings::row_wash`); kad Update: garis hover + dip bila tekan |
| Add-ons | lencana, suis dan bar masuk stagger; bar muat turun meluncur | lencana terang bila hidup; latar baris hover fade |
| Semua suis dan picker | suis meluncur dan angkat picker meluncur 0.2 s selepas berubah (`entrance::glide` / `glide_k`) | tiada dengan reduced motion, battery saver atau self-test (`entrance::set_still`) |

Kos: tiada benang, pemasa atau poll baharu. Frame tambahan hanya 1.1 s selepas tab dibuka atau modul System / sasaran Record dipilih, < 0.6 s untuk hover / tekan / spring, dan 10 s ambien di Focus (sesi berjalan) dan Record (sedang merakam). Tiada langsung dengan reduced motion atau battery saver.

