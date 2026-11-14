# Advanced settings: every item

The Settings window (Advanced settings, the island's last Settings row) has a section, **Every setting**, that lists each item the window can draw: its name, the section it lives in, and how it stands on this computer (on, off, its value, needs Halo Pro, needs an add-on, shows with another setting, or not on this build). It reads the states already in `AppState`; nothing runs for it.

This page is the same list. It comes from `settings_window::INVENTORY`, and the test `the_inventory_is_every_row_the_window_draws` checks it against the rows the window really draws, both ways: a row added to the window and not to the list, or listed and never drawn, fails it.

Besides the rows below, sections hold parts that aren't rows: the version and Check now (General, About), the theme cards (Appearance), the Add-ons page itself (Add-ons), the Halo Pro key and license details (Halo Pro, see [licenses.md](licenses.md#settings--halo-pro)), and the inventory (Every setting).


## General

| Item | Shows |
|---|---|
| Start with Windows | Always. On a Mac it reads "Start at login". |
| Sounds | Always. |
| 24-hour clock | Always. |
| News on Today | Always. |
| Keep System page history | With Halo Pro and the System add-on on. |
| Drink water | Always. |
| Glasses a day | Always. |
| Clipboard | Always. |
| Widget slots | One row per widget slot (2 free, 5 with Pro). |
| Add a widget | While a slot is free. |

## Appearance

| Item | Shows |
|---|---|
| Idle look | Always. |
| Halo Mascot | Windows only. |

## Island

| Item | Shows |
|---|---|
| Opens after | Always. |
| Closes | Always. |
| Width when open | Always. |
| Display | Windows only. |
| In full-screen apps | Always. |
| Only for a few seconds | Windows only, with Halo Pro. |
| Mini handle | Always. |

## Music and lyrics

| Item | Shows |
|---|---|
| Synced lyrics | Always. |
| Lyrics pronunciation | Always. |
| Timing | Always. |
| Show what's playing | Always. |

## Pop-ups

| Item | Shows |
|---|---|
| Volume | Always. |
| Charger and battery | Always. |
| Bluetooth | Always. |
| Caps Lock | Always. |
| Wi-Fi | Always. |

## Tabs

| Item | Shows |
|---|---|
| Open where you left off | Always. |
| Opens on | With "Open where you left off" off. |
| Music | Always. |
| Today | Always. |
| Shelf | Always. |
| Tasks | Always. |
| Focus | Always. |
| Notifications | Only in a copy that can read notifications. |

## Focus

| Item | Shows |
|---|---|
| Focus | Always. |
| Short break | Always. |
| Long break | Always. |
| Long break after | Always. |
| Breaks | Always. |
| Focus sessions | Always. |

## Prayer times

The section shows when: The Prayer times add-on is on.

| Item | Shows |
|---|---|
| Follow | Always. |
| Sound | Always. |
| Pause music for the adzan | Always. |
| Countdown before each prayer | Always. |
| Follow my location | Always. |

## Caffeine

The section shows when: The Caffeine add-on is on.

| Item | Shows |
|---|---|
| Bedtime | Always. |
| Half-life | Always. |
| Left at bedtime | Always. |
| Remind me before last call | Always. |
| Day limit | Always. |

## AI usage

The section shows when: Halo Pro, and AI usage not switched off.

| Item | Shows |
|---|---|
| Heads-up notices | Always. |
| Close to the limit at | With Heads-up notices on. |

## Halo Pro

| Item | Shows |
|---|---|
| Theme builder | With Halo Pro. |
| Colours | With Halo Pro and Theme builder on. |
| Corners | With Halo Pro and Theme builder on. |
| Glass | With Halo Pro and Theme builder on. |
| Cloud sync | With Halo Pro. |

## About

| Item | Shows |
|---|---|
| What's new | Always. |
| Lyrics | Always. |
| Data folder | Always. |
