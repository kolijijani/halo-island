#!/bin/bash
# Re-shoot website screenshots from the current (rebranded) app.
# Runs the self-test under Wine + Xvfb, segmented so the 90 s scene deadline
# doesn't stop early scenes from being captured.
set -uo pipefail
cd "$HOME/work/3-development/apps/APP-009-Halo-Island" || exit 2
EXE=target/x86_64-pc-windows-gnu/release/halo.exe

run() {
  local out="$1"; shift
  rm -rf "$out"; mkdir -p "$out"
  echo "== $(date +%T) $out :: $*"
  env "$@" timeout 180 tools/run-selftest-wine.sh "$EXE" "$out" >"$out.log" 2>&1
  echo "   exit=$? pngs=$(ls "$out"/*.png 2>/dev/null | wc -l)"
}

merge() { # $1 target, then source dirs
  local dst="$1"; shift
  rm -rf "$dst"; mkdir -p "$dst"
  for d in "$@"; do cp "$d"/*.png "$dst"/ 2>/dev/null; done
  echo "   merged -> $dst ($(ls "$dst"/*.png 2>/dev/null | wc -l) png)"
}

# ---- Classic (default theme), 200 % ----
run out-c1 HALO_SELFTEST_DPI=192
run out-c2 HALO_SELFTEST_DPI=192 HALO_SELFTEST_FROM=halo-money-out
run out-c3 HALO_SELFTEST_DPI=192 HALO_SELFTEST_FROM=page-whats-new
merge out-classic out-c1 out-c2 out-c3

# Classic with Korean lyrics (page-media-ko only)
run out-classic-ko HALO_SELFTEST_DPI=192 HALO_SELFTEST_LYRICS=ko HALO_SELFTEST_FROM=page-media

# ---- Halo theme, 200 % ----
run out-h1 HALO_SELFTEST_DPI=192 HALO_SELFTEST_THEME=halo
run out-h2 HALO_SELFTEST_DPI=192 HALO_SELFTEST_THEME=halo HALO_SELFTEST_FROM=halo-money-out
run out-h3 HALO_SELFTEST_DPI=192 HALO_SELFTEST_THEME=halo HALO_SELFTEST_FROM=page-whats-new
merge out-halo out-h1 out-h2 out-h3

# Halo loop (media-activity on a held note)
run out-halo-loop HALO_SELFTEST_DPI=192 HALO_SELFTEST_THEME=halo HALO_SELFTEST_LOOP=1 HALO_SELFTEST_FROM=page-media

# ---- Pill theme, 200 % ----
run out-p1 HALO_SELFTEST_DPI=192 HALO_SELFTEST_THEME=pill
run out-p2 HALO_SELFTEST_DPI=192 HALO_SELFTEST_THEME=pill HALO_SELFTEST_FROM=halo-money-out
run out-p3 HALO_SELFTEST_DPI=192 HALO_SELFTEST_THEME=pill HALO_SELFTEST_FROM=page-whats-new
merge out-pill out-p1 out-p2 out-p3

# ---- Diorama ----
run out-diorama HALO_SELFTEST_DPI=192 HALO_SELFTEST_THEME=diorama HALO_SELFTEST_DIORAMA=only
run out-diorama-day HALO_SELFTEST_DPI=192 HALO_SELFTEST_THEME=diorama HALO_SELFTEST_FROM=page-media
run out-diorama-night HALO_SELFTEST_DPI=192 HALO_SELFTEST_THEME=diorama HALO_SELFTEST_DIORAMA=night HALO_SELFTEST_FROM=page-media

echo "== all done $(date +%T)"
