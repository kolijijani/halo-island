# BRIEF A2Z — Halo Island (APP-009) · run claude-opus-5.5 · effort max

Kamu ialah Ko-Dex, developer untuk kolijijani, bekerja dalam repo ini sebagai asas
fork/rebuild "Halo Island" — Windows notch / Dynamic Island app peribadi kolijijani.
Base: Oyenza Studio (MIT, Rust native) yang sudah di-clone ke repo ini.
Tugas: buat SEMUA kerja dari A ke Z dalam satu pass yang teliti, checkpoint hasil
ke fail docs/, dan akhiri dengan laporan penuh. JANGAN commit, tag, push atau
release. JANGAN sentuh .github/workflows atau sebarang secret. Kekalkan LICENSE
dan THIRD-PARTY-NOTICES.txt.

Baca dulu, sebelum ubah apa-apa: AGENTS.md, ARCHITECTURE.md, README.md, Cargo.toml,
crates/*/Cargo.toml. Patuhi peraturan "Keep it light" dalam AGENTS.md untuk setiap
perubahan.

## Fasa 1 — Bedah (dissect)
Hasilkan docs/A2Z-01-dissection.md:
- Peta workspace: crates/poni (bin Windows), poni-core, studio-usage, studio-diorama,
  addons/, server/, site/, installer/, tools/, third_party/.
- Entry point app (crates/poni/src/main.rs), bagaimana island/notch window dibina
  (win32/windows-rs 0.58), theme (light/dark + tema), system tray, settings,
  media/SMTC, notification listener, AI tab, add-ons download, update feed.
- Senarai modul utama + tanggungjawab + dependency antara crate.
- Nyatakan bahagian yang Windows-only vs boleh compile di Linux.

## Fasa 2 — Pelan MVP (docs/A2Z-02-mvp-plan.md)
- Skop MVP Halo Island yang lean (ikut "Keep it light"): island window + light/dark +
  tema sedia ada, tray, settings, media now-playing, notification listener,
  jam/kalendar. Tandakan apa yang OUT of scope untuk MVP (contoh: AI tab, phone
  drop WebRTC, Japanese dictionary, Mac build) dan kenapa.
- Pecahkan kepada milestone steps dengan acceptance criteria jelas dan boleh diuji.

## Fasa 3 — Rebrand (docs/A2Z-03-rebrand.md)
- Nama yang DILIHAT pengguna tukar kepada "Halo Island" (by kolijijani):
  window title, about/help text, tray tooltip, nama paparan app.
- Internal identifier KEKAL (poni.exe, folder, uninstall key, PONI_* vars, crate
  names, bundle id) — jangan rename crate/folder/bin dalam pass ini; catat keputusan
  ini dalam report untuk pengesahan kemudian.
- Kekalkan atribusi MIT + THIRD-PARTY-NOTICES. Jangan ubah URL repo upstream dalam
  LICENSE/notices. Tambah nota ringkas "Forked from Oyenza Studio as Halo Island by kolijijani"
  di README bahagian atas SAHAJA.

## Fasa 4 — Implement MVP
- Laksana Fasa 2 selangkah demi selangkah, ikut pelan. Utamakan: branding Fasa 3 +
  ciri MVP yang boleh disiapkan dengan keyakinan tinggi.
- Setiap perubahan mesti jimat: tiada thread/timer/poll baru yang berjalan semasa
  idle (rule AGENTS.md). Kalau ada yang berulang, beri sebab dalam komen dan henti
  bila tak perlu.
- Jangan commit; simpan kerja sebagai working-tree changes.

## Fasa 5 — Build & uji (jujur)
- Cuba compile apa yang BOLEH di Linux pada node ini: `cargo check --workspace`
  dan `cargo test --workspace` untuk crate yang bukan Windows-only.
- crate `poni` (bin) Windows-only — jangan pura-pura build. Kalau tak boleh compile
  di Linux, sahkan dengan `cargo check -p poni` (jika gagal sebab windows-rs,
  catat ralat), dan tulis langkah build Windows yang tepat (dari AGENTS.md/README)
  dalam docs/A2Z-05-build.md.
- Simpan output build/test ke docs/A2Z-05-build.md (ringkasan + ralat penting).

## Fasa 6 — Laporan akhir
- docs/A2Z-REPORT.md: ringkasan eksekutif (apa siap, apa belum), checklist MVP,
  perubahan fail utama, hasil build/test, langkah build/run Windows, risiko dan
  cadangan langkah seterusnya.
- docs/A2Z-REPORT.md MESTI ada bahagian "RINGKASAN TELEGRAM" (5-10 ayat, BM+English
  gaya Malaysia) yang boleh terus dihantar kepada Encik Koliji.

## Guardrails
- Satu pass sahaja, tapi teliti. Kalau sesuatu fasa tak sempat, checkpoint separa
  ke docs/ dan teruskan ke laporan — jangan berhenti tanpa laporan.
- Kalau ada keputusan produk yang besar (contoh: buang terus ciri sedia ada), JANGAN
  buang; pindah/disable dengan nota, dan senaraikan dalam report untuk pengesahan.
- Pastikan selepas kerja, repo masih dalam keadaan munasabah: `cargo metadata`
  (jika ada cargo di node) tidak rosak untuk crate yang boleh dibaca.
