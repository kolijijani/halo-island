# Chosen Direction — Phase 1 Dock Decision

- **Selected direction:** **A — Magnification shelf** (`mockups/dock/a.html`)
- **Ledger item:** `dock-phase-1-investigate-window-layer-an`
- **Branch:** `ag/dock-phase1`
- **Decision date:** 2026-11-26

## Summary of Chosen Interaction Model

1. **Floating bottom shelf (`a.html`):**
   - Centred bottom pill above `MonitorInfo::work.bottom` (or `rect.bottom` when the Windows taskbar auto-hides), pairing cleanly with the top Halo Island notch (Notch = live activities & alerts; Dock = app launcher, running windows, downloads stack, and quick system status capsule).
   - Continuous parabolic cursor-follow icon magnification with spring settle, launch bounce, and running-app indicator dots.
2. **Genie minimize / restore warp (`a.html`):**
   - Smooth 72-slice cosine S-curve (`~420 ms`) perspective mesh warp into/from the dock icon when minimizing or restoring via the dock (`PrintWindow` / DWM thumbnail snapshot deformed on a Direct3D mesh; native foreign titlebar `─` buttons keep standard Windows minimize behavior).
3. **Edge states & overflow rules:**
   - **Fullscreen:** Hides immediately via `SW_HIDE` on `FullscreenWatcher` (`AppEvent::Fullscreen(true)`); bottom-edge wake disabled until fullscreen exits.
   - **30 running apps:** Single-row proportional slot shrink (`48 px` → `36 px`, `32 px` at `125%` DPI) with zero clipping or second-row wrap.
   - **Hide without Settings:** Right-click shelf → *Hide shelf* tucks the dock into a `4 px` bottom peek handle (click handle to restore).

## Architectural Constraints Carried into Phase 2

- **Zero idle cost (`AGENTS.md`):** `FrameClock` parks on `Condvar` at 0% CPU (`0` frames/s, `0` polls/s) when the pointer is away, the session is locked (`App::locked`), the display is off (`App::display_off`), or fullscreen hides the shelf.
- **Shared GPU device, non-blocking second `Present`:** Reuse Halo's existing `ID3D11Device` / `ID2D1Device` across `island` and `dock` windows; pace on vblank once per tick (`Present(0, 0)` on the secondary swapchain) so two `Present(1, 0)` calls never halve refresh rate.
- **Event-driven window & icon pipeline:** Seed running windows once via `EnumWindows` when `dock_enabled` turns on, maintain via out-of-context `SetWinEventHook` (`0` calls/s at rest), and resolve Win32 + `AppsFolder` UWP/MSIX icons off the UI thread.
- **Safety boundaries:** Never hide, kill, or modify `Shell_TrayWnd` / `Shell_SecondaryTrayWnd`, and land the notch placement regression test proposed in `docs/DOCK-INVESTIGATION.md` (§ Test seam) before modifying `window.rs` or `app.rs`.
