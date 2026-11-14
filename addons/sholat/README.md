# Prayer times add-on

Prayer times is off by default. Switching it on in the Add-ons tab (the "+"
at the end of the tab row) downloads these files; switching it off deletes
them. The code that works out the times (`crates/halo-core/src/prayer/`) is
part of halo.exe and stays asleep until the add-on is on.

The files are published, byte for byte, on the pre-release
[`addon-sholat-3`](https://github.com/kolijijani/halo-island/releases/tag/addon-sholat-3).
Each one's size and SHA-256 are built into the app (`halo_core::addons`),
and `cargo test -p halo-core addons` checks that they match the files here.
Changing a file means a new tag (`addon-sholat-4`), new numbers in
`halo_core::addons`, and an app release. Never delete an older tag:
`addon-sholat-1` holds the files older copies download (the
older adzan), and `addon-sholat-2` a world list that went online instead and
never shipped.

- `places.json`: the places people can pick. All 517 locations in Kemenag's
  prayer schedule (every kab/kota, plus five island points such as "Pulau
  Laut, Kab. Natuna"), named as Kemenag writes them. Each has the reference
  point and the Terbit and Maghrib horizon (`h0`) that reproduce Kemenag's
  own 2026 schedule with this engine, so the computed times land on
  Kemenag's minutes. Six Papua highland regencies carry Kemenag's wider
  twilight angles there (Subuh 21°, Isya 19°). Makkah, Madinah and Jeddah
  use Umm al-Qura. Settings keep the chosen place by its `id`, so never
  rename one.
- `adzan.m4a`: "The Adhan - Muslim Call to Prayer - Aaqib Azeez" (86 s,
  AAC), by Atcovi, from Wikimedia Commons under CC BY-SA 4.0, so the notice
  credits it and this file is shared under the same license. A close, clean
  recording: the whole frequency range, and the pauses between the phrases
  about 38 dB under the voice. Checked word by word: four takbir, each
  shahada twice, each hayya twice, the closing takbir and "la ilaha
  illallah", and no Subuh line, so it plays for the other four prayers.
  Made from the Commons file with `ffmpeg -i
  The_Adhan_-_Muslim_Call_to_Prayer_-_Aaqib_Azeez.mp3 -af
  "pan=mono|c0=0.5*c0+0.5*c1,highpass=f=70:poles=2,atrim=start=0.25:end=86.0,asetpts=PTS-STARTPTS,afade=t=in:d=0.2,afade=t=out:st=84.35:d=1.4,volume=0.25dB,alimiter=limit=0.8414:attack=2:release=40:level=false"
  -ar 44100 -ac 1 -c:a aac -b:a 80k -map_metadata -1 -map_chapters -1
  -fflags +bitexact -flags:a +bitexact -movflags +faststart adzan.m4a`
  (FFmpeg 6.1.1 gives these exact bytes). The adzan before it (on
  `addon-sholat-1`), the Maghrib adzan at Masjidil Haram on 25 Feb 2012 by
  3omar Faruq, came from a phone video: nothing above 4 kHz, and the crowd
  and the hall under every phrase. The first recording, from 21 Jan 2013,
  stopped mid-way through the closing takbir.
- `NOTICE.txt`: where each file comes from, its licence and what was changed.
  `addon-sholat-3` keeps the notice it was published with
  (`sholat-3/NOTICE.txt`); this one, with the voices' credits, goes on
  `addon-sholat-4`.

## Adzan voices (`addon-sholat-4`)

Prayer times' Sound picks the voice. Each is its own file on the pre-release
[`addon-sholat-4`](https://github.com/kolijijani/halo-island/releases/tag/addon-sholat-4),
downloaded only once someone picks it and deleted when they pick another
(`halo_core::addons::VOICES`, `Settings::fetch_on`); nothing streams or is
fetched ahead. A voice whose file isn't on the PC plays the tone.

- `adzan-makkah.m4a`: the Maghrib adzan at Masjid al-Haram, Makkah, on 25 Feb
  2012, by 3omar Faruq, from Wikimedia Commons under CC BY 3.0 (reviewed by
  Commons' YouTubeReviewBot). 178 s, AAC 80 kbit/s, 1.8 MB, -16.4 LUFS. The
  same file `addon-sholat-1` held, made then by keeping 1:54 to 4:52 of the
  video, mono, faded and levelled. The crowd and the hall are in it, and
  nothing above 4 kHz: it is the real place, not a studio.
- `adzan-subuh.m4a`: Subuh's adzan by Besim Azemi at the Islamic Center
  Malmö, on Eid al-Fitr, 19 Aug 2012, from Wikimedia Commons under CC BY 3.0
  (reviewed by YouTubeReviewBot). Checked line by line: four takbir, each
  shahada twice, each hayya twice, "as-salatu khairum min an-naum" twice,
  the closing takbir and "la ilaha illallah". Subuh plays it whichever voice
  the other prayers have, so Subuh no longer falls back to the tone. 233 s,
  AAC 80 kbit/s, 2.4 MB, -16.4 LUFS. Made with `ffmpeg -i
  Eid_al-Fitr_Fajr_azan_at_Malmö_Mosque_-_19_August_2012.webm -vn -af
  "pan=mono|c0=0.5*c0+0.5*c1,highpass=f=70:poles=2,atrim=start=4.6:end=237.2,asetpts=PTS-STARTPTS,afade=t=in:d=0.2,afade=t=out:st=231.4:d=1.2,volume=0.5dB,alimiter=limit=0.8414:attack=2:release=40:level=false"
  -ar 44100 -ac 1 -c:a aac -b:a 80k -map_metadata -1 -map_chapters -1
  -fflags +bitexact -flags:a +bitexact -movflags +faststart
  adzan-subuh.m4a` (FFmpeg 6.1.1 gives these exact bytes).
- `NOTICE.txt`: every recording's credit and licence.

No recording from Masjid an-Nabawi, Madinah, is on Wikimedia Commons (14 Nov
2026), and none was taken from anywhere whose licence isn't stated. A second
Makkah recording there, "Time of Adhan, Great Mosque of Mecca - Jan 6, 2018"
(CC BY 3.0), starts mid-way through its opening takbir, so it was left out.

`tools/publish_addons.py stage` gathers every tag's files from here and
checks them against the pins; `commands` prints what publishes them.

Cities outside Indonesia aren't in the add-on. The place picker looks them
up online as you type, one small file at a time from the website
(`site/places/1/`, built by `tools/make_world_places.py` from GeoNames, CC BY
4.0), and settings keep only the city picked, with its clock, so its times
need nothing online afterwards.

## How the times are checked

`cargo test -p halo-core prayer` replays:

- Kemenag's published 2026 schedule for all 517 locations, 227,367 times:
  98.2% exact, never more than 1 minute off. Every place is at least 95%
  exact, and every miss sits within seconds of a minute boundary.
- Kanwil Kemenag Jawa Tengah's Ramadan 1447 H schedule for Kota Semarang,
  21 times, all exact.
- Halo Umroh's prayer engine (adhan-js with the same rules) on 18,225
  place-days from 2026 to 2075, all identical to the minute.
- adhan-js itself on 47 cities abroad (each country's method, Hanafi Ashar
  where the list says so), every 9 days of 2026 and 2027: identical to the
  minute, except polar winter days where adhan-js puts Ashar out of order.

The schedules came from api.myquran.com, which republishes Kemenag's
(bimasislam.kemenag.go.id). They are test data only: the add-on ships
places, never Kemenag's tables.
