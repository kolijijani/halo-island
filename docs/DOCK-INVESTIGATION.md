# Dock Investigation — Window, Input, and Rendering Layer

Every claim below cites `file:line` against branch `ag/dock-phase1`. Anything absent from the tree is marked **does not exist**.

---

## 1. Island Window Creation, Placement, Topmost, DWM, Focus, Multi-Monitor, and DPI

### Creation and `WS_EX_*` flags
Three Win32 window classes are registered in `register_classes` (`crates/halo/src/window.rs:57-99`): `Halo.Island` (`window.rs:22`), `Halo.Helper` (`window.rs:23`), and `Halo.Visual` (`window.rs:24`).

- **Hit-test / input window (`Halo.Island`)**: `create_island_window` (`crates/halo/src/window.rs:103-121`) creates a `WS_POPUP` (`window.rs:110`) at `1×1` px with:
  `WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` (`window.rs:107`).
- **GPU visual window (`Halo.Visual`)**: `create_visual_window` (`crates/halo/src/window.rs:131-156`) creates an owned `WS_POPUP` (`owner = island` at `window.rs:142`) with:
  `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` (`window.rs:134`).
  Its window procedure `visual_wndproc` (`window.rs:44-55`) returns `HTTRANSPARENT` on `WM_NCHITTEST` (`window.rs:49`) and `LRESULT(0)` on `WM_DPICHANGED` (`window.rs:52`). On the GPU path (`DirectComposition`), `visual` displays the flip-model swapchain (`crates/halo/src/gfx.rs:10-16, 43, 1320-1332`), while `island` holds a `1/255`-alpha hit-test bitmap pushed via `UpdateLayeredWindowIndirect` (`gfx.rs:13-16, 360, 392`; `crates/halo/src/app.rs:4138-4155`) so touch, pen, and mouse pass through transparent areas (`window.rs:126-130`). Under the software fallback, `island` holds the full premultiplied DIB pushed via `UpdateLayeredWindowIndirect` (`gfx.rs:18-20, 1334-1339`).
- **Hidden helper window (`Halo.Helper`)**: `create_helper_window` (`crates/halo/src/window.rs:254-259`) creates a `0×0` `WS_POPUP` with `WS_EX_TOOLWINDOW` (`window.rs:257`) owning the notification-area icon, `SHAppBarMessage` registration, power/session broadcasts, and conditional hotkeys.

### Placement
`App::layout_surface` (`crates/halo/src/app.rs:917-922`), `App::layout_surface_for` (`app.rs:941-947`), and `App::place_surface` (`app.rs:953-976`) size the window once to fit the widest, tallest expanded page plus spring overshoot (`surface_size` at `app.rs:944`) so morphing never resizes or moves the HWND (`gfx.rs:4-8`, `app.rs:912-915`).
- Horizontal X: centred on `monitor.rect` (`left + (mon_w - w) / 2` at `app.rs:958`), or clamped near `Settings::island_x` (`spot::window_left` at `app.rs:959-962`), with internal sub-window centre `App::place_island` (`app.rs:982-987`).
- Vertical Y: `self.monitor.top()` (`app.rs:966`), where `MonitorInfo::top` (`crates/halo/src/window.rs:310-320`) returns `work.top` if a Windows taskbar (`Shell_TrayWnd` or `Shell_SecondaryTrayWnd`) is docked along the monitor's top edge (`taskbar_at_top` at `window.rs:266-289`), and `rect.top` otherwise.

### Topmost handling
Created with `WS_EX_TOPMOST` (`window.rs:107, 134`). Re-asserted without focus steal via `SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)` in `App::raise_topmost` (`crates/halo/src/app.rs:3377-3381`) and on first show (`app.rs:4086`). When another window comes forward (`AppEvent::Foreground` at `services/fullscreen.rs:347`), fullscreen exits (`app.rs:3062`), or Explorer restarts (`TaskbarCreated` at `app.rs:1550-1560`), `App::start_raises` (`app.rs:3346-3355`) schedules `TIMER_RAISE` (`app.rs:1948-1959`), calling `App::raise_if_covered` (`app.rs:3359-3374`), which checks `GWL_EXSTYLE & WS_EX_TOPMOST` and `services::fullscreen::window_above(self.hwnd)` (`app.rs:3360-3361`). Because `visual` is owned by `island` (`window.rs:142`), raising `island` keeps `visual` directly above it (`window.rs:129-130`).

### DWM calls
- `DwmFlush` (`crates/halo/src/window.rs:12, 462`): vblank wait fallback (`Vblank::Dwm`) when no `IDXGIOutput` matches the monitor (`window.rs:441-446`).
- `DwmGetCompositionTimingInfo` (`crates/halo/src/app.rs:5568-5570`): read in `dwm_counters` (`app.rs:4169`) for motion hitch logging.
- `DwmGetWindowAttribute(..., DWMWA_CLOAKED, ...)` (`crates/halo/src/services/fullscreen.rs:34`): filters cloaked UWP / virtual-desktop windows during fullscreen and top-band checks.
- `DwmExtendFrameIntoClientArea` / `DwmEnableBlurBehindWindow` / `SetWindowCompositionAttribute` (Acrylic/Mica): **does not exist**. Glass refraction is Halo's own Direct3D/Direct2D shader pass (`crates/halo/src/glass.rs`, wired at `gfx.rs:27`).

### Focus-stealing avoidance
- `WS_EX_NOACTIVATE` on `island` and `visual` (`window.rs:107, 134`).
- `WM_MOUSEACTIVATE` returns `MA_NOACTIVATE` unless `self.text_focus` is true (`crates/halo/src/app.rs:1430`).
- Window show and z-order calls pass `SW_SHOWNOACTIVATE` (`app.rs:3036, 4085`; `gfx.rs:1355`) and `SWP_NOACTIVATE` (`app.rs:3379, 4086`).
- When an expanded page's text field needs keyboard input, `App::focus_text` (`app.rs:3394-3405`) records `self.prev_foreground = GetForegroundWindow()` (`app.rs:3400`), clears `WS_EX_NOACTIVATE` (`App::set_noactivate(false)` at `app.rs:3383-3392, 3401`), and calls `SetForegroundWindow` + `SetFocus`. `App::blur_text` (`app.rs:3407-3420`) and `WM_ACTIVATE(WA_INACTIVE)` (`app.rs:1514-1522`) restore `WS_EX_NOACTIVATE` and return foreground to `self.prev_foreground` (`app.rs:3415-3417`). Context menus in `tray::show_menu` (`crates/halo/src/tray.rs:206-216`) likewise restore `prev = GetForegroundWindow()` after `TrackPopupMenu`.

### Multi-monitor and DPI change
- `MonitorChoice` (`crates/halo-core/src/settings.rs:100-114`) supports `Primary`, `Cursor`, `Secondary`, and `All` (one island window at a time, moving to the monitor containing `GetForegroundWindow()` on foreground events — `window::pick_monitor` at `crates/halo/src/window.rs:338-374`). A per-monitor simultaneous multi-window mode **does not exist** (`settings.rs:111-112`).
- `WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE` on `island` (`crates/halo/src/app.rs:1524-1539`) and `WM_DISPLAYCHANGE | WM_SETTINGCHANGE` on `helper` (`app.rs:1650-1653`) call `App::refresh_monitor` (`app.rs:3422-3439`). If `MonitorInfo` (`rect`, `work`, or `dpi`) changed, `App::set_dpi` (`app.rs:907-910`) updates `self.gfx.set_dpi(dpi as f32)` and `self.layout_surface()`, re-targets `self.clock.set_monitor(m.handle)` (`app.rs:3437`, `window.rs:575-578`), and calls `request_frame()`. (`SettingsWindow` handles its own `WM_DPICHANGED` via `w.set_dpi` at `app.rs:1912-1920`.)

---

## 2. Fullscreen, Session Lock, Display-Off, Battery Saver, and Ambient Pause

### Detection functions
- **Fullscreen**: `FullscreenWatcher::start` (`crates/halo/src/services/fullscreen.rs:142-195`) combines three event-driven sources (no polling):
  1. `register_appbar` (`fullscreen.rs:273-280`) calling `SHAppBarMessage(ABM_NEW)` on `helper`, handled by `FullscreenWatcher::on_appbar_message` (`fullscreen.rs:199-212`) on `ABN_FULLSCREENAPP` (`app.rs:1586-1593`).
  2. Global `SetWinEventHook(EVENT_SYSTEM_FOREGROUND, ..., WINEVENT_OUTOFCONTEXT)` (`fullscreen.rs:147-157`) invoking `on_win_event` (`fullscreen.rs:295-350`).
  3. Per-thread `ThreadHooks::install` / `update_thread_hooks` (`fullscreen.rs:360-417`) installing `EVENT_OBJECT_LOCATIONCHANGE` and `EVENT_OBJECT_DESTROY..=EVENT_OBJECT_HIDE` on at most two foreign threads (foreground and kept fullscreen culprit).
  `reevaluate` (`fullscreen.rs:191, 206, 339`) emits `AppEvent::Fullscreen(on)`, stored in `App::fullscreen_on` (`crates/halo/src/app.rs:468, 2614`). `App::fullscreen_hides` (`app.rs:3012-3014`) computes visibility and `App::apply_fullscreen` (`app.rs:3027-3064`) hides/shows `self.hwnd` and sets `App::fullscreen_hidden` (`app.rs:460`).
- **Session locked**: `services::power::register_session` (`crates/halo/src/services/power.rs:77-86`) calls `WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION)` (`power.rs:79`). `App::handle_helper` handles `WM_WTSSESSION_CHANGE_` (`crates/halo/src/app.rs:1611-1624`), setting `App::locked` (`app.rs:554`) on `WTS_SESSION_LOCK_` / `WTS_SESSION_UNLOCK_` (`app.rs:1613-1614`).
- **Display off**: `services::power::register_session` registers `GUID_CONSOLE_DISPLAY_STATE` via `RegisterPowerSettingNotification` (`crates/halo/src/services/power.rs:82`). `services::power::display_state_change` (`power.rs:90-102`) parses `WM_POWERBROADCAST` in `App::handle_helper` (`crates/halo/src/app.rs:1594-1597`), setting `App::display_off = !on` (`app.rs:555, 1596`).
- **Battery saver**: `services::power::register` (`crates/halo/src/services/power.rs:65-73`) registers `GUID_POWER_SAVING_STATUS` (`power.rs:67-68`). `services::power::query` (`power.rs:46-60`) calls `GetSystemPowerStatus` and sets `PowerState::saver = s.SystemStatusFlag == 1` (`power.rs:58`), stored in `self.st.power.saver` (`crates/halo/src/app.rs:1601-1602`).

### Where ambient animation and background work pause
- `App::frame` (`crates/halo/src/app.rs:4103-4127`): `let ambient_ok = !self.locked && !self.display_off;` gates `eq_live` (`activities::is_live(&self.mode, &self.st)`) and `drift_live`, and `self.clock.set_active(moving || eq_live || drift_live)` parks `FrameClock` when false.
- Equalizer, HUD motion, and page entrances on battery saver: `crates/halo/src/activities.rs:383, 932` (`st.power.saver`) and `crates/halo/src/app.rs:3802` (`self.reduce_motion || self.st.power.saver`).
- `App::sync_mascot` (`crates/halo/src/app.rs:3584-3607`): sets `m.still = self.locked || self.display_off || self.st.power.saver` (`app.rs:3588`) and drops raw-input wake listening (`app.rs:3600-3606`).
- `App::schedule_wake` (`crates/halo/src/app.rs:4207-4240`): suppresses second-tick and lyric-line wake timers when `fullscreen_hidden`, `locked`, or `display_off`.
- Background readers: `App::tell_sysmon_rest` (`app.rs:2987-2989`), `App::set_usage_watch` (`app.rs:2057-2067`), `App::set_stocks_watch` (`app.rs:2127-2138`), `App::halo_account_activity` (`app.rs:2113-2122`), `App::on_timer(TIMER_MINUTE)` (`app.rs:2004`), and `App::update_due` (`app.rs:4299-4301`).

---

## 3. Reusable Monitor and Work-Area Helpers

- `window::MonitorInfo` (`crates/halo/src/window.rs:292-300`): provides `handle: isize`, `rect: RECT` (`rcMonitor`), `work: RECT` (`rcWork`, taskbar excluded), `dpi: u32`, `MonitorInfo::scale()` (`window.rs:302-304`), and `MonitorInfo::center_x()` (`window.rs:321-323`).
- `window::monitor_info(hmon)` (`window.rs:326-336`): wraps `GetMonitorInfoW` and `GetDpiForMonitor(..., MDT_EFFECTIVE_DPI)`.
- `window::pick_monitor(choice)` (`window.rs:338-374`): selects monitor for `Primary`, `Cursor`, `Secondary`, or `All`.
- `window::monitors()` (`window.rs:377-390`): `EnumDisplayMonitors` returning physical `Vec<RECT>`.
- `window::taskbar_at_top(mon: &RECT)` (`window.rs:266-289`): checks if `Shell_TrayWnd` or `Shell_SecondaryTrayWnd` sits at the top edge. A bottom-taskbar detector (`taskbar_at_bottom`) **does not exist**, though `MonitorInfo::work.bottom < MonitorInfo::rect.bottom` already indicates a non-auto-hidden bottom bar (`window.rs:296, 334`).

---

## 4. Window and Process Enumeration Today, and Options for a Running-Apps List

### What exists today
- **General running-apps window list**: **does not exist**.
- **Targeted window walks**:
  - `crates/halo/src/services/applemusic_ui.rs:289`: one-shot `EnumWindows` to locate Apple Music's HWND on demand.
  - `crates/halo/src/services/fullscreen.rs:48, 225, 337` (`rescan_top`): walks top-level Z-order (`GetWindow` with `GW_HWNDNEXT`) on `EVENT_SYSTEM_FOREGROUND` only when `Settings::step_aside` is on.
- **Process enumeration**:
  - `crates/halo/src/services/sysmon.rs:731-751` (`process_list`) + `crates/halo-core/src/sysmon.rs:701-741` (`parse_process_list`): calls `NtQuerySystemInformation(SystemProcessInformation, ...)` into a reusable buffer (`~256–512 KB`) and resolves executable descriptions via `QueryFullProcessImageNameW` + `GetFileVersionInfoW` (`services/sysmon.rs:754-766`). Runs only while the System page or game readout is active (0 calls/s at rest). Unsuitable on its own for a dock: lists background processes without top-level windows and lacks `HWND` handles needed to focus/restore windows.
  - `crates/halo/src/services/downloader.rs:310`: `CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)` for `yt-dlp` subprocess cleanup only.

### Options for a dock's running-apps list

| Option | Mechanism | Memory cost | Calls/s at rest | Calls/s during window churn |
| :--- | :--- | :--- | :--- | :--- |
| **1. `EnumWindows` seed + out-of-context `SetWinEventHook` (recommended)** | One `EnumWindows` scan when dock turns on (`IsWindowVisible`, uncloaked via `DWMWA_CLOAKED`, unowned or `WS_EX_APPWINDOW`, `!WS_EX_TOOLWINDOW`), kept live by `EVENT_OBJECT_CREATE/DESTROY/SHOW/HIDE/CLOAKED/UNCLOAKED` + existing `EVENT_SYSTEM_FOREGROUND` (`fullscreen.rs:148`), filtered at entry by `idObject == OBJID_WINDOW && idChild == CHILDID_SELF` (`fullscreen.rs:305`). | **Estimated `< 0.2 MB`** (~64 KB table for 30–60 windows; `user32`/`dwmapi`/`kernel32` already loaded). | **0 calls/s** | ~5–30 filtered WinEvent callbacks/s while apps open, close, or switch focus. |
| **2. `EnumWindows` seed + `RegisterShellHookWindow`** | Seed once via `EnumWindows`, then receive `HSHELL_WINDOWCREATED`, `HSHELL_WINDOWDESTROYED`, `HSHELL_WINDOWACTIVATED`, `HSHELL_REDRAW` messages on a dedicated HWND. | **Estimated `< 0.1 MB`** (`user32` already loaded). | **0 calls/s** | 1 message per top-level taskbar window lifecycle/focus event (requires extra `DWMWA_CLOAKED` verification on UWP suspend). |
| **3. Periodic `EnumWindows` poll (1 Hz)** | Timer-driven `EnumWindows` every 1 s. | **Estimated `< 0.2 MB`** | **1 `EnumWindows` + ~250 HWND queries/s** (violates `AGENTS.md` zero-idle rule). | Same (~250 queries/s). |
| **4. WinRT / UIAutomation (`IUIAutomation` / `AppDiagnosticInfo`)** | COM/WinRT window/app watchers. | **Estimated `4–10 MB`** (`UIAutomationCore.dll` + WinRT heaps). | **0 calls/s** | Event-driven, heavy COM marshaling. |

---

## 5. Icon Acquisition Today and Reuse for a Dock

- **Halo's own icon**: `window::app_icon` (`crates/halo/src/window.rs:32-40`) and `Tray::add` (`crates/halo/src/tray.rs:17-20`) load resource ordinal `1` (`halo.ico`) from `halo.exe` via `LoadImageW(..., IMAGE_ICON, ...)` / `LoadIconW`. Cannot load external app icons.
- **Internal UI icons**: `crates/halo/src/icons.rs:14-496` defines static 24×24 SVG path vectors (`Icon` enum).
- **Shell icon & thumbnail pipeline**: `Thumbnails::extract` (`crates/halo/src/services/imaging.rs:647-685`) calls `SHCreateItemFromParsingName(&HSTRING::from(path), None)` (`imaging.rs:649`) and `IShellItemImageFactory::GetImage(size, SIIGBF_BIGGERSIZEOK | SIIGBF_ICONONLY)` (`imaging.rs:653-659`), converts `HBITMAP` via `IWICImagingFactory::CreateBitmapFromHBITMAP(..., WICBitmapUsePremultipliedAlpha)` (`imaging.rs:664-670`), fixes all-zero alpha legacy DIBs (`imaging.rs:673-677`), and returns premultiplied BGRA `ImageData` (`gfx.rs:54-75`).
- **Can it feed a dock?**
  - **Yes for classic Win32 `.exe` and `.lnk` paths** (run on a worker thread, never the UI thread, because `IShellItemImageFactory` touches disk/shell handlers).
  - **Two gaps**:
    1. Packaged UWP/MSIX apps (`ApplicationFrameHost.exe`, Windows Terminal, Calculator, Windows Settings): `SHCreateItemFromParsingName` on `ApplicationFrameHost.exe` yields a generic frame icon, not the hosted app's icon. Extracting those requires `GetApplicationUserModelId` / `IPropertyStore` (`PKEY_AppUserModel_ID`) + `SHCreateItemInKnownFolder(&FOLDERID_AppsFolder, ..., aumid)` (**does not exist** today).
    2. Bitmap cache cap: `Gfx::end` (`crates/halo/src/gfx.rs:1307-1310`) evicts unreferenced bitmaps whenever `self.bitmaps.len() > 48`. A 30-app dock plus an open island page (album art + shelf items) can exceed 48 bitmaps in a single frame or across two `Gfx` instances; the cap would need raising (e.g., 96) or a dedicated icon atlas/cache on the dock's `Gfx`.

---

## 6. Animation Frame Driver Today and Per-Frame Cost of a Magnifying Dock

### Today's frame driver
`FrameClock` (`crates/halo/src/window.rs:524-717`) spawns one thread `"halo-frame-clock"` (`window.rs:547`) that blocks on `IDXGIOutput::WaitForVBlank()` (`window.rs:460`, falling back to `DwmFlush()` at `window.rs:462` or `thread::sleep` at `window.rs:468`) and posts `WM_APP_FRAME` (`crates/halo/src/events.rs:17`, `window.rs:713`) to `self.hwnd`. At the end of `App::frame` (`crates/halo/src/app.rs:4107-4127`), `self.clock.set_active(moving || eq_live || drift_live)` parks the clock thread on `Condvar::wait` (`window.rs:641-643`) at **0% CPU** whenever nothing animates. `WM_TIMER` (`app.rs:1925-2039`) is used only for one-shot delays and minute/water/save wakeups, never frame pacing.

### What a magnifying dock adds per frame
- **Directions A and B (separate bottom bar)**: **a second independent redraw, swapchain `Present`, and hit-mask `UpdateLayeredWindowIndirect`** on a second HWND pair (`dock_hwnd` + `dock_visual`), even if ticked by the same `WM_APP_FRAME` message on the UI thread. Stretching a single `Gfx` surface across the entire `1920×1080` monitor to hold both top notch and bottom dock is ruled out: a full-screen premultiplied RGBA flip swapchain costs `~16.6–25 MB` of GPU/DWM buffers at 1080p (`~66 MB` at 4K) and forces full-screen DWM composition on every frame.
  - **Direction A (Magnification shelf)**: every `WM_MOUSEMOVE` across the dock wakes `FrameClock` at full display refresh (60–240 Hz) plus ~180–250 ms spring settle after stop. Each frame runs a second `ID2D1DeviceContext::BeginDraw/Clear/EndDraw` (`~10–30` scaled `DrawBitmap` calls + backdrop pill + dots + label), a second `IDXGISwapChain1::Present(1, 0)` (watch out: calling `Present(1, 0)` synchronously on *two* swapchains on one UI thread can serialize on vblank unless the second uses `Present(0, 0)` or `DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT`), and periodic `UpdateLayeredWindowIndirect` updates (`MASK_INTERVAL`, `app.rs:4142`) as the magnified silhouette height changes.
  - **Direction B (Flush dock, no magnification)**: redraws only on icon enter/leave highlight (~8–12 frames) and app launch/focus/close events; moving the pointer within an icon or at rest costs **0 frames/s**.
- **Direction C (Island strip)**: **no second window, no second swapchain, no second `Present`, and no second hit-mask call**. It renders inside the existing `island` / `visual` surface (`app.rs:941-976, 4066`) during `App::frame`, adding only `~10–30` `DrawBitmap` calls while the strip is open/hovered and **0 work** when collapsed.

---

## 7. Settings-Flag Pattern to Follow

The closest plain boolean feature flag is `Settings::step_aside: bool` (alongside `Settings::mascot: bool`):
1. **Data model**: `pub step_aside: bool` on `Settings` (`crates/halo-core/src/settings.rs:153`, `#[serde(default)]` at `settings.rs:127`, default set in `Settings::default()` at `settings.rs:419`). *(Alternative if packaged as a built-in zero-download add-on like Widgets/Stocks/Caffeine: `Settings::addons: BTreeMap<String, bool>` at `settings.rs:200` and `addons::WIDGETS` in `crates/halo-core/src/addons.rs`.)*
2. **Settings UI**: `switch(Key::StepAside, "Mini handle", Some("..."), s.step_aside)` in `crates/halo/src/settings_window.rs:609` (using `Ctl::Switch(Some(on))` at `settings_window.rs:373`), mutated in `apply` at `settings_window.rs:1001` (`Key::StepAside => s.step_aside = !s.step_aside`).
3. **Runtime lifecycle**: `App::settings_changed(&mut self, before: &Settings)` at `crates/halo/src/app.rs:2315-2318` checks `before.step_aside != self.st.settings.step_aside`, starting hooks/windows only when turned `true` and dropping them immediately when turned `false` so disabled features have zero runtime footprint.

---

## 8. Existing Hotkeys, Hooks, and Taskbar APIs (Collision Map)

- **Global hotkeys (`RegisterHotKey`)**: **Yes, two conditional registrations on `self.helper`**:
  - `Alt+Shift+F` (`HOTKEY_PEEK`): `crates/halo/src/app.rs:3109-3124` (`set_peek_hotkey`), registered while `st.game.active` is true; handled at `app.rs:1625-1629`.
  - `Alt+Shift+S` (`HOTKEY_SCRIPT`): `crates/halo/src/app.rs:3127-3142` (`set_script_hotkey`), registered while the Script page is readable (`app.rs:3162-3163`); handled at `app.rs:1630-1635`.
- **Raw input (`RegisterRawInputDevices`)**: **Yes, on `self.hwnd`** (`crates/halo/src/app.rs:3601-3620`) for Caps Lock OSD (`hud_caps`) and mascot wake-from-doze.
- **WinEvent hooks (`SetWinEventHook`)**: **Yes**, in `crates/halo/src/services/fullscreen.rs:148-156` (global `EVENT_SYSTEM_FOREGROUND`) and `fullscreen.rs:362-375` (per-thread `EVENT_OBJECT_LOCATIONCHANGE` and `EVENT_OBJECT_DESTROY..=EVENT_OBJECT_HIDE`).
- **Windows hooks (`SetWindowsHookEx`) & `RegisterShellHookWindow`**: **does not exist**.
- **Taskbar / AppBar APIs**: **Yes, three touchpoints**:
  1. `SHAppBarMessage(ABM_NEW / ABM_REMOVE)` on `self.helper` (`crates/halo/src/services/fullscreen.rs:273-287`, callback `WM_APP_APPBAR` at `app.rs:1586-1593`). **Direct collision if reused**: Windows permits only one `ABM_NEW` per `HWND`, and calling `ABM_SETPOS` from a dock would shrink `MonitorInfo::work` and fire `WM_SETTINGCHANGE`, re-triggering `App::refresh_monitor` (`app.rs:1524`).
  2. `RegisterWindowMessageW(w!("TaskbarCreated"))` (`crates/halo/src/app.rs:699, 1550-1562`), re-adding `Shell_NotifyIconW` (`crates/halo/src/tray.rs:17-37`) and re-registering the AppBar (`fullscreen.rs:216-227`).
  3. `FindWindowExW` reading `Shell_TrayWnd` / `Shell_SecondaryTrayWnd` (`crates/halo/src/window.rs:266-289`; `fullscreen.rs:60-61`).
  4. `ITaskbarList` / `ITaskbarList3`: **does not exist**.

---

## Test Seam (Item 2 — Proposed Regression Check, No Code Written)

`window_check` in `crates/halo/src/selftest.rs:4286-4307` currently verifies `WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` and checks `GetWindowRect(hwnd) == self.surface`, while `pill_checks` (`selftest.rs:4325-4345`) only inspects `NotchType::Pill`. Because `window_check` compares `GetWindowRect` against `self.surface` (which `place_surface` itself wrote) rather than independent monitor coordinates, and never checks the Classic notch's alpha silhouette, a dock change that shifts `place_surface` off `monitor.top()` / `monitor.center_x()` or alters the resting notch outline would pass `halo.exe --selftest` unnoticed. The smallest check lives in `crates/halo/src/selftest.rs` right beside `window_check` and runs once on the existing Classic `IdleClock` scene (`settings.island_x == None`), costing `< 0.5 ms` in CI (one `GetMonitorInfoW` call plus three 1D scanlines on the already-captured `Frame` RGBA buffer; zero extra scenes or processes):

- `GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY))` independent check: `window_rect.top == monitor_top` and `((window_rect.left + window_rect.right) / 2 - (rcMonitor.left + rcMonitor.right) / 2).abs() <= 1`.
- Top-bezel attachment scan (`y = 0` in `Frame`): alpha is `255` across the entire resting notch span `[cx_px - (rest_w / 2.0 + SHOULDER) * scale, cx_px + (rest_w / 2.0 + SHOULDER) * scale]` (±1 px tolerance) and `0` outside that span.
- Resting height & shoulder concavity scan: at `x = cx_px`, alpha is `255` from `y = 0` through `(rest_h * scale).floor() - 1` and `<= 1` (hit-mask only) from `(rest_h * scale).ceil() + 2` to `frame.h - 1`; at `y = (rest_h * 0.5 * scale).round()`, horizontal opaque width equals `rest_w * scale` (±2 px), proving concave shoulders were not flattened into a rectangle.

---

## Cost Budget

| Metric | Direction A (Magnification shelf) | Direction B (Flush dock) | Direction C (Island strip) | Basis |
| :--- | :--- | :--- | :--- | :--- |
| **Download size delta (`Halo-Island-Setup.exe`)** | `~15–35 KB` code (0 new crates, 0 bundled assets) | `~12–25 KB` code (0 new crates, 0 bundled assets) | `~8–18 KB` code (reuses island window & `Gfx`) | **Estimated** |
| **Memory at rest (`dock_enabled = false`)** | `0 MB` (1 `bool` in `Settings`) | `0 MB` (1 `bool` in `Settings`) | `0 MB` (1 `bool` in `Settings`) | **Estimated** (follows `step_aside` pattern, `app.rs:2315`) |
| **Memory at rest (`dock_enabled = true`, 12–30 icons cached)** | `~3.5–6.5 MB` (second DXGI flip swapchain ~`1920×140` + hit-mask DIB + 30×`64×64` BGRA icon bitmaps ~`0.5 MB`) | `~2.5–4.5 MB` (smaller swapchain ~`1920×72` + 30×`48×48` BGRA icon bitmaps) | `~0.5–1.2 MB` (no second window/swapchain; only cached `ImageData` + `ID2D1Bitmap`s inside existing `Gfx`) | **Estimated** (VRAM/DWM attribution per swapchain on Windows 11 is **unknown** until measured in `smoke-out/metrics.txt`) |
| **Per-frame work while idle (cursor away, no window churn)** | `0 frames/s`, `0 CPU cycles` (`FrameClock` parked on `Condvar`, `window.rs:641`) | `0 frames/s`, `0 CPU cycles` (`FrameClock` parked) | `0 frames/s`, `0 CPU cycles` (`FrameClock` parked) | **Estimated** from `window.rs:636-644` |
| **Per-frame work while cursor moves over dock** | **Second independent redraw + `Present` + periodic `UpdateLayeredWindowIndirect`** at full display Hz (60–240 Hz) on every `WM_MOUSEMOVE` (~`0.4–1.2 ms` GPU/UI-thread per frame; exact cycle count **unknown** until measured with `QueryProcessCycleTime`) | `0 frames/s` while moving inside one icon; brief ~8–12 frame fade (`~0.3–0.8 ms`/frame) only when crossing icon boundaries | Single existing `Gfx` redraw + `Present` (`app.rs:4066`); adds ~`10–30` `DrawBitmap` calls (~`0.15–0.4 ms`) to existing island frame | **Estimated** (cycle cost **unknown**) |

---

## Risks (Max 5)

1. **Dual-swapchain vblank serialization stalls UI thread to half refresh rate**
   - *Failure*: Calling `IDXGISwapChain1::Present(1, 0)` (`gfx.rs:1324`) synchronously on both `island` and `dock` swapchains during one `WM_APP_FRAME` blocks twice on vblank, dropping a 60 Hz display to 30 fps whenever both animate (e.g., music equalizer in island + cursor magnifying dock).
   - *How caught*: `MotionStats` hitch counter and `dwm_counters()` in `crates/halo/src/app.rs:4160-4178` (logs `missed` frames > 0); mitigated by `Present(0, 0)` on the secondary surface (since `FrameClock` already phase-locks to `IDXGIOutput::WaitForVBlank`) or Direction C (one swapchain).
2. **Bottom-edge fight with Windows 11 Taskbar (`Shell_TrayWnd`)**
   - *Failure*: If the user has not enabled Windows "Automatically hide the taskbar", a bottom dock either sits directly on top of `Shell_TrayWnd` (visual collision and stolen taskbar clicks) or floats 48 DIPs above the bottom (wasting vertical screen height); if auto-hide *is* enabled, moving the pointer to `y = 1079` un-hides `Shell_TrayWnd` at the top of the `WS_EX_TOPMOST` band right over the dock.
   - *How caught*: Manual & self-test check with `Shell_TrayWnd` both docked (`work.bottom < rect.bottom`) and auto-hidden (`work.bottom == rect.bottom`); Direction C avoids the bottom edge completely.
3. **Packaged UWP / MSIX blank or generic icons and wrong window grouping**
   - *Failure*: `QueryFullProcessImageNameW` on `ApplicationFrameHost.exe` groups Calculator, Settings, and Windows Store under a single `ApplicationFrameHost` icon with a white generic frame bitmap.
   - *How caught*: Running-app test with Calculator + Windows Settings open simultaneously; caught if both collapse into one icon or render `Icon::App` fallback.
4. **`Gfx::bitmaps` 48-entry cache thrashing (`gfx.rs:1307`)**
   - *Failure*: At 30 running apps + expanded Shelf/Media page, total bitmaps exceed 48, causing `Gfx::end` (`gfx.rs:1307-1310`) to evict and re-upload GPU textures on every animation frame.
   - *How caught*: `QueryProcessCycleTime` spike and frame-budget check (`FRAME_BUDGET_MS = 16.0` in `selftest.rs:129`) on a 30-icon scene.
5. **UI-thread shell stall during icon extraction**
   - *Failure*: Calling `IShellItemImageFactory::GetImage` (`services/imaging.rs:653`) on the UI thread when a network-mounted or sleeping-disk `.exe` launches freezes both the dock and the island morph for 100–2,000 ms.
   - *How caught*: `MotionStats` late-frame trace (`app.rs:4177`) and existing network-path guard pattern in `services/imaging.rs:41, 625-640`; icon extraction must stay strictly async on a worker thread.

---

## What Would Make This a No-Go

Drop the standalone bottom dock (Directions A and B) if **any** of the following holds:
1. **Windows 11 bottom-edge collision cannot be made clean without touching `Shell_TrayWnd`**: Halo must never hide, kill, or hook `Shell_TrayWnd`. Because Windows 11's auto-hidden taskbar still triggers at `y = rect.bottom - 1` and raises itself to the top of `WS_EX_TOPMOST`, if the maintainer finds sitting above a visible taskbar redundant and triggering the auto-hidden taskbar underneath unacceptable, a bottom dock on Windows cannot behave like macOS's Dock without an unsupported Explorer hack. (Direction C survives this because it lives at the top notch.)
2. **Rest memory with `dock_enabled = true` exceeds +5 MB private working set**: The System page's performance counters already cost ~6 MB when loaded (`AGENTS.md`); if a second DirectComposition swapchain + shell icon factory keeps > 5 MB resident all day at rest, it violates Halo's core lightweight product rule.
3. **Dual-surface rendering causes any measurable hitch (> 0 missed vblanks) in the island's own morph**: The island is the product; if maintaining a second topmost surface degrades island frame pacing or breaks Windows Independent Flip in borderless games (`fullscreen_hides`), the dock must be dropped or restricted to Direction C.
