# Halo Island brand files

Halo Island belongs to the Halo family. Halo is the orange cat of the Halo Island logo.

| File | What |
|---|---|
| `halo-mark.svg`, `halo-mark-white.svg` | Halo's head in colour and in white, copied unchanged from the Halo Island brand kit |
| `halo-logo.svg`, `halo-logo-512.png` | The logo: an orange tile, the island hanging from its top edge playing music (album cover and equalizer, as on the real island), the white Halo below |
| `art/halo-sit.webp` | Halo sitting on the island while it plays music (the installer's welcome and finish picture) |
| `art/halo-peek.webp` | Halo peeking over the island while it plays music (README, link preview) |
| `art/halo-asleep.webp` | Halo asleep on the island while it plays music (the website's Halo theme section) |
| `art/poses/halo-*.webp` | Halo with each page's prop: music, focus, health, shelf, tasks, clipboard, today, system (a processor chip), games (a controller), prayer (a lantern) (README pictures and website) |
| `social-preview.png` | The card GitHub shows when the repo link is shared (Settings → General → Social preview) |

`python3 tools/make_brand.py` redraws the logo, `crates/halo/res/halo.ico` and everything in `installer/art` from these files.

Drawing Halo: the head is exactly the shape of the mark, the left ear (viewer's left) is folded forward with a darker underside, and the face has only the two eyes: no mouth, nose, whiskers or blush. Don't redraw Halo in another style.

These files are not covered by the MIT license that covers the code.
