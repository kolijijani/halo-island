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
