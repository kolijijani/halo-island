//! Battery / charging state (`GetSystemPowerStatus`), change notifications,
//! and native Win32 system power actions (Shutdown, Restart, Sleep, Lock).

use crate::commands::PowerAction;
use crate::state::PowerState;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use windows::core::GUID;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, LUID, WPARAM};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED, SE_SHUTDOWN_NAME, TOKEN_ADJUST_PRIVILEGES,
    TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Power::{
    GetSystemPowerStatus, RegisterPowerSettingNotification, SetSuspendState, POWERBROADCAST_SETTING, SYSTEM_POWER_STATUS,
};
use windows::Win32::System::RemoteDesktop::{WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION};
use windows::Win32::System::SystemServices::{GUID_ACDC_POWER_SOURCE, GUID_BATTERY_PERCENTAGE_REMAINING, GUID_CONSOLE_DISPLAY_STATE, GUID_POWER_SAVING_STATUS};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::WindowsAndMessaging::{DEVICE_NOTIFY_WINDOW_HANDLE, PBT_POWERSETTINGCHANGE};

#[link(name = "user32")]
extern "system" {
    fn LockWorkStation() -> i32;
    fn ExitWindowsEx(flags: u32, reason: u32) -> i32;
}

const EWX_SHUTDOWN: u32 = 0x0000_0001;
const EWX_REBOOT: u32 = 0x0000_0002;
const EWX_POWEROFF: u32 = 0x0000_0008;
const SHTDN_REASON_FLAG_PLANNED: u32 = 0x8000_0000;

/// Guard preventing a power action from firing twice (rapid clicks or duplicate commands).
struct ActionGuard {
    terminating: bool,
    last_at: Option<Instant>,
}

const ACTION_COOLDOWN: Duration = Duration::from_secs(3);
static ACTION_GUARD: Mutex<ActionGuard> = Mutex::new(ActionGuard { terminating: false, last_at: None });

/// `GUID_DEVICE_POWER_POLICY_VIDEO_BRIGHTNESS`: Windows notifies `WM_POWERBROADCAST`
/// whenever display brightness changes (0..=100), with zero polling.
const GUID_VIDEO_BRIGHTNESS: GUID = GUID::from_u128(0xaded5e82_b909_4619_9949_f5d71dac0bcb);

pub fn query() -> PowerState {
    let mut s = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut s) }.is_err() {
        return PowerState::default();
    }
    // BatteryFlag: 128 = no system battery, 255 = unknown.
    let has_battery = s.BatteryFlag != 128 && s.BatteryFlag != 255 && s.BatteryLifePercent <= 100;
    PowerState {
        has_battery,
        percent: if s.BatteryLifePercent <= 100 { s.BatteryLifePercent } else { 0 },
        charging: has_battery && (s.BatteryFlag & 8) != 0,
        on_ac: s.ACLineStatus == 1,
        saver: s.SystemStatusFlag == 1,
    }
}

/// Ask Windows to send `WM_POWERBROADCAST` to `hwnd` when the power source,
/// the battery percentage or battery saver changes (battery saver can be
/// switched by hand at any charge).
pub fn register(hwnd: HWND) {
    unsafe {
        for guid in [&GUID_ACDC_POWER_SOURCE, &GUID_BATTERY_PERCENTAGE_REMAINING, &GUID_POWER_SAVING_STATUS, &GUID_VIDEO_BRIGHTNESS] {
            if let Err(e) = RegisterPowerSettingNotification(HANDLE(hwnd.0), guid, DEVICE_NOTIFY_WINDOW_HANDLE) {
                crate::pwarn!("RegisterPowerSettingNotification failed: {e}");
            }
        }
    }
}

/// Ask for session lock/unlock (`WM_WTSSESSION_CHANGE`) and display on/off
/// (`WM_POWERBROADCAST`) notifications so ambient animation can pause.
pub fn register_session(hwnd: HWND) {
    unsafe {
        if let Err(e) = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) {
            crate::pwarn!("WTSRegisterSessionNotification failed: {e}");
        }
        if let Err(e) = RegisterPowerSettingNotification(HANDLE(hwnd.0), &GUID_CONSOLE_DISPLAY_STATE, DEVICE_NOTIFY_WINDOW_HANDLE) {
            crate::pwarn!("display-state notification unavailable: {e}");
        }
    }
}

/// For `WM_POWERBROADCAST`: `Some(display_on)` when the message reports a
/// console display state change (0 = off, 1 = on, 2 = dimmed counts as on).
pub fn display_state_change(wp: WPARAM, lp: LPARAM) -> Option<bool> {
    if wp.0 as u32 != PBT_POWERSETTINGCHANGE || lp.0 == 0 {
        return None;
    }
    // SAFETY: for PBT_POWERSETTINGCHANGE, lParam points to a POWERBROADCAST_SETTING
    // valid for the duration of the message.
    let s = unsafe { &*(lp.0 as *const POWERBROADCAST_SETTING) };
    if s.PowerSetting != GUID_CONSOLE_DISPLAY_STATE || s.DataLength < 4 {
        return None;
    }
    let v = unsafe { std::ptr::read_unaligned(s.Data.as_ptr() as *const u32) };
    Some(v != 0)
}

/// For `WM_POWERBROADCAST`: `Some(level)` (0.0..=1.0) when Windows reports a
/// display brightness change.
pub fn brightness_change(wp: WPARAM, lp: LPARAM) -> Option<f32> {
    if wp.0 as u32 != PBT_POWERSETTINGCHANGE || lp.0 == 0 {
        return None;
    }
    let s = unsafe { &*(lp.0 as *const POWERBROADCAST_SETTING) };
    if s.PowerSetting != GUID_VIDEO_BRIGHTNESS || s.DataLength < 4 {
        return None;
    }
    let v = unsafe { std::ptr::read_unaligned(s.Data.as_ptr() as *const u32) };
    Some((v.min(100) as f32) / 100.0)
}

/// Enable `SE_SHUTDOWN_NAME` on the current process token before `ExitWindowsEx`.
fn enable_shutdown_privilege() -> Result<(), String> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token)
            .map_err(|e| format!("OpenProcessToken: {e}"))?;
        let mut luid = LUID::default();
        let lookup = LookupPrivilegeValueW(None, SE_SHUTDOWN_NAME, &mut luid);
        if let Err(e) = lookup {
            let _ = CloseHandle(token);
            return Err(format!("LookupPrivilegeValueW: {e}"));
        }
        let tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
        };
        let adjust = AdjustTokenPrivileges(token, false, Some(&tp), 0, None, None);
        let _ = CloseHandle(token);
        adjust.map_err(|e| format!("AdjustTokenPrivileges: {e}"))
    }
}

/// Run a system power action via Win32 APIs (no shell commands).
/// Returns `false` if suppressed by the double-fire guard.
pub fn execute(action: PowerAction) -> bool {
    {
        let Ok(mut g) = ACTION_GUARD.lock() else { return false };
        if g.terminating || g.last_at.is_some_and(|t| t.elapsed() < ACTION_COOLDOWN) {
            return false;
        }
        g.last_at = Some(Instant::now());
        if action.needs_confirm() {
            g.terminating = true;
        }
    }
    // One-shot worker thread so `SetSuspendState` never blocks the UI message loop.
    std::thread::spawn(move || {
        let ok = match action {
            PowerAction::Lock => unsafe { LockWorkStation() != 0 },
            PowerAction::Sleep => unsafe { SetSuspendState(false, false, false).as_bool() },
            PowerAction::Shutdown => match enable_shutdown_privilege() {
                Ok(()) => unsafe { ExitWindowsEx(EWX_SHUTDOWN | EWX_POWEROFF, SHTDN_REASON_FLAG_PLANNED) != 0 },
                Err(e) => {
                    crate::pwarn!("power shutdown privilege failed: {e}");
                    false
                }
            },
            PowerAction::Restart => match enable_shutdown_privilege() {
                Ok(()) => unsafe { ExitWindowsEx(EWX_REBOOT, SHTDN_REASON_FLAG_PLANNED) != 0 },
                Err(e) => {
                    crate::pwarn!("power restart privilege failed: {e}");
                    false
                }
            },
        };
        if !ok {
            crate::pwarn!("power action {:?} failed", action);
            if action.needs_confirm() {
                if let Ok(mut g) = ACTION_GUARD.lock() {
                    g.terminating = false;
                }
            }
        }
    });
    true
}
