# Backlog — Halo Island

Recorded list of improvements to make. Items stay here until the maintainer
orders them into a work order (a `BUGFIX-*.md` or `GRILL-*.md`) and a version.

## License

### L1 — Uninstall destroys the license (and the install id)

**What:** uninstalling deletes the whole cache dir, which holds both the
license token and this install's id.

```
RMDir /r "$LOCALAPPDATA\Halo"     ; installer/halo.nsi
%LOCALAPPDATA%\Halo\license.bin         ; the token (DPAPI-sealed)
%LOCALAPPDATA%\Halo\license-device.bin  ; the install's random id
```

**Effect:** reinstall generates a *new* install id, so the old code answers
`409 already_used` on redeem — the user needs a *new* code, not just to
re-enter the old one.

**Fix to decide:** keep `license-device.bin` across uninstall (move it to
`%APPDATA%\Halo`), or add an admin reissue/redeem-again flow. The token itself
should still be removed on uninstall (it is, correctly).

### L2 — Revoke does not reach the installed app

**What:** the app checks the token offline only — `services::license::status(false)`,
the `revoked` flag is hardcoded false. There is no timer asking
`POST /api/status`, so an admin revoke is never seen by a running/installed app.

**Effect:** a revoked license keeps unlocking Pro until its `exp` passes; a
permanent Pro (no `exp`) never drops. (Owner decided "no daily `/api/status`
poll" in `docs/GRILL-0.2.40.md` §10 — revisit this when we build it.)

**Fix to decide:** a light `/api/status` check while Pro is active (e.g. once a
day, per `docs/licenses.md`), or another reach mechanism.

## Permintaan CC CLI (owner's request)

### R1 — View license key in the admin portal

Owner's request (verbatim): "Minta CC CLI buat — di website
`http://100.99.255.105:8790/` wujudkan fungsi untuk saya tengok license key."

**Scope:** the admin portal (`server/license-server.mjs`, the `home`/Keys page,
tailnet-only, port 8790) gains a way for the owner to view a license key after
it was made.

**Current limitation (deliberate):** the server stores only `SHA-256` of each
code (`codeHash`), and shows the code *once* when keys are made — "the server
never logs a code". To view keys later, the server must store the code (or an
owner-decryptable form), which is a security trade-off the maintainer must
approve first.

---

## 0.3.00 — Animation work (new version)

Owner's requests, collected as he sends them (list is open — more comments
incoming). Version target `0.3.00` (a new version, "if possible"). CC CLI is
to use the `grill-me` skill (adversarial plan interview) before implementing.

### R2 — Match the animations in the reference video

- Reference video (owner): a clip he shared
- Question to CC CLI: *can we make animations like this video?*
- Context (pending confirmation against the video):
  mascot eyes that doze off then wave hello when you return, synced lyrics
  that light up line by line, a breathing Live Activity for the Pomodoro, a
  status ring (battery / water) around the clock, Glass / floating-pill styles.

### R3 — Animation suggestions for other parts

- Question to CC CLI: *suggest animations for other parts of Halo Island*
  (not just the one in the video).

### R5 — OSD pop-up family: Caps Lock + Bluetooth + Wi-Fi (owner, 12 Nov 2026)

Owner's request (verbatim): "wujudkan fungsi supaya button caps lock popup,
connect bluetooth connect wifi dan lain lain jika perlu. sama macam bila user
tekan volume down/up notch island kita keluar bagi tahu. buat animasi bagi
cantik."

- Trigger pop-up for Caps Lock (ON/OFF), Bluetooth connect/disconnect,
  Wi-Fi connect/disconnect — same feel as the existing volume up/down pop-up.
- Animate them so they look good (expand/collapse, icon + label, auto-dismiss,
  no keyboard-focus steal, dedup/queue so pop-ups don't spam, respect
  reduced-motion, enable/disable per category).
- Current state (verified in code): `HudKind` already has Volume, Charging,
  LowBattery, Bluetooth, CapsLock, Wifi. `toggle()` draws CapsLock + Wifi;
  `bluetooth()` draws Bluetooth separately. So the pop-ups exist — what's
  missing is the *animation*, applied to the whole family (toggle + bluetooth
  + power), not Caps Lock only.

### Method (owner's instruction)

- CC CLI must run the `grill-me` skill: stress-test the 0.3.00 animation plan
  through structured adversarial rounds before any code, and only implement
  after the owner confirms alignment.

---

## Komen pemilik — 15 Nov 2026

New batch of owner comments (verbatim intent, numbered as given):

### C1 — Remove the "every setting" menu

"menu every setting tak perlu ada, boleh remove" — remove the
every-setting/see-all-settings menu entry (find it and delete).

### C2 — Weather widget does not work

"weather widget masih tak berfungsi" — the weather widget is broken. Fix it.

### C3 — Add more suitable pop-ups; all free (no Halo Pro gating)

"pop up menu. tambahkan fungsi lain lain yang bersesuaian. sebab saya semak,
macam wifi connect tapi tak ada tunjuk di notch. tambahkan fungsi baru dan
bersesuaian. kekalkan popup sedia ada sebagai free, untuk popup baru yang
nampak sebagai limited atau key point buat as halo pro."

Owner update (15 Nov 2026): masukkan semua sebagai free — tak perlu Halo Pro.

- Wi-Fi connect currently does NOT show at the notch (owner observed) — the
  Wi-Fi pop-up exists (`HudKind::Wifi`) but its trigger is missing/broken.
- Add more *suitable* pop-ups beyond the current six, ALL free:
  - Num Lock / Scroll Lock on/off (`GetKeyState`)
  - Headphone / audio device plug/unplug (`IMMDeviceEnumerator`)
  - Secondary monitor connect/disconnect (`WM_DISPLAYCHANGE`)
  - Mic in-use indicator
  - Camera in-use indicator
  - Focus / Do Not Disturb session start/end (Win11)
  - Airplane mode on/off

### C4 — Add an FPS menu under advanced settings

"masukkan menu fps di dalam advance setting" — add an FPS counter/menu inside
the advanced settings.

### C5 — Trim and tidy the hidden-icon taskbar menu

"kurangkan dan kemas kan menu di hidden icon taskbar" — reduce and tidy the
tray (hidden-icon) context menu.

Owner update (15 Nov 2026): kurangkan menu sebab sesetengah menu dah ada dalam
advance setting — kekalkan yang diperlukan sahaja.

### C6 — Settings button on the notch gets hidden when many menus above it

"butang setting di notch tersorok jika banyak menu diatas notch" — the notch's
settings button is hidden/covered when there are many menu items above the
notch. Fix the layout so it stays visible.

---

## Senarai kerja 0.3.00 (FINAL — disahkan pemilik, 15 Nov 2026)

Skop muktamad selepas pemilik semak. Semua item lain dalam senarai lama (Today
news, stock, caffeine, add-on, music lyric, Mireld API key, widget 6 slot)
dah siap / tak perlu — tiada dalam skop.

1. **Baiki** — Weather widget tak berfungsi (`C2`).
2. **Popup OSD — semua free, animasi sekali untuk semua** (`R5` + `C3`):
   - CapsLock + Bluetooth + Wi-Fi popup + animasi (R5)
   - Wi-Fi connect tak tunjuk di notch — trigger missing (C3)
   - Popup baru: Num/Scroll Lock, headphone plug/unplug, monitor connect/disconnect, mic in-use, camera in-use, Focus/DND, airplane mode (C3)
3. **Animasi menyeluruh** — semak semua bahagian, buat animasi untuk tempat yang boleh (gabung `R2`–`R4`).
4. **Buang** — menu "every setting" (`C1`).
5. **Bina** — menu FPS dalam advance setting (`C4`).
6. **Kurangkan** — menu tray hidden-icon; sesetengah menu dah ada dalam advance setting, kekalkan yang perlu sahaja (`C5`).
7. **Baiki** — butang Settings di notch tersorok bila banyak menu atasnya (`C6`).
8. **License** — L1 uninstall padam license + install id; L2 revoke tak sampai ke app; R1 view license key admin portal.

