//! Games page (Windows only, with the System add-on): Show over games.
//!
//! On the left its settings: the switch, whether the readout stays or
//! peeks, what the readout shows beside the GPU's load (FPS, the GPU's
//! temperature, the CPU's load) and the screen's refresh rate, which tops
//! the frame-rate chart and ring. In the middle the frame rate's last
//! minute and the frame times under it. On the right what's in front: a
//! ring of its frame rate between the GPU's load and temperature, its name,
//! the CPU and video memory, and whether Windows lets Halo read frames
//! (with the one-time OK). It draws with the System page's rings, charts
//! and rows, from the same readers; the page itself starts none.
//!
//! As the page opens (`pages::entrance`) it fills top to bottom as System
//! does: the section rules grow out from their titles, the ring and gauges
//! draw to their readings, the chart's grid and line run out from the left;
//! the numbers and the switches show at once.

use super::entrance::Entrance;
use super::system::{
    chart_box, dashboard, history_column, plot, separator, set_entrance, stack, temperature, Cols,
    Fill, Gauge, Ring, Row,
};
use super::PageKind;
use crate::commands::Command;
use crate::gfx::{TextStyle, Weight};
use crate::state::{AppState, FpsAccess};
use crate::ui::{wid, PillStyle, Ui, PILL_SMALL};
use halo_core::frames::{fmt_fps, fmt_frame_ms};
use halo_core::geom::{palette, Rect};
use halo_core::settings::GAME_HZ;
use halo_core::sysmon::{fmt_bytes, fmt_pct};

/// The System page's height: the window never grows for this page.
const HEIGHT: f32 = 256.0;
const SEP_H: f32 = 20.0;
const ROW_H: f32 = 24.0;
const DASH_H: f32 = 86.0;
const DETAIL_H: f32 = 20.0;
const PLACEHOLDER: &str = "\u{2013}";

/// Frame rate charts top out at the nearest common refresh rate above the peak.
const FPS_TOPS: [f32; 8] = [30.0, 60.0, 75.0, 120.0, 144.0, 165.0, 240.0, 360.0];

/// The readout's two ways over a game, and the `game_peek` each one sets.
const GAME_READOUT: [(bool, &str); 2] = [(false, "Stays"), (true, "Peeks")];

pub fn height(_st: &AppState) -> f32 {
    HEIGHT
}

fn fps_ceiling(peak: f32) -> f32 {
    FPS_TOPS
        .into_iter()
        .find(|t| peak <= *t)
        .unwrap_or_else(|| (peak / 120.0).ceil().max(1.0) * 120.0)
}

/// The chart's and ring's top: the refresh rate picked, else Auto's.
fn fps_top(st: &AppState) -> f32 {
    match st.settings.game_hz {
        0 => fps_ceiling(st.game.history.max()),
        hz => f32::from(hz),
    }
}

fn hz_name(hz: u16) -> String {
    if hz == 0 {
        "Auto".into()
    } else {
        format!("{hz} Hz")
    }
}

/// True once a CPU reading arrived with no temperature while CPU temp is on.
fn cpu_temp_missing(st: &AppState) -> bool {
    st.settings.game_cpu_temp
        && st
            .system
            .latest
            .as_ref()
            .and_then(|r| r.cpu.as_ref())
            .is_some_and(|c| c.temp_c.is_none())
}

fn cpu_row_value(cpu: Option<(f32, Option<f32>)>, want_temp: bool) -> String {
    cpu.map_or_else(
        || PLACEHOLDER.into(),
        |(load, temp_c)| match temp_c {
            Some(t) => format!("{} \u{b7} {t:.0}\u{b0}C", fmt_pct(load)),
            None if want_temp => format!("{} \u{b7} no sensor", fmt_pct(load)),
            None => fmt_pct(load),
        },
    )
}

pub fn draw(ui: &mut Ui, st: &mut AppState, area: Rect) {
    // Fixed layout height: the island's spring may still be opening.
    let area = Rect::new(area.x, area.y, area.w, HEIGHT);
    set_entrance(Entrance::of(ui, st, PageKind::Games), area.y);
    let cols = Cols::new(area);
    draw_settings(ui, st, cols.dash);
    draw_chart(ui, st, cols.mid);
    draw_front(ui, st, cols.top);
}

/// The switch, the readout's way, what it shows and the refresh rate.
fn draw_settings(ui: &mut Ui, st: &mut AppState, r: Rect) {
    let accent = st.accent;
    let (head, rest) = r.split_top(SEP_H);
    separator(ui, head, "OVER GAMES");
    let (row, rest) = rest.split_top(ROW_H);
    let on = st.settings.game_stats;
    let label = if st.settings.pro {
        "Show over games"
    } else {
        "Show over games (Pro)"
    };
    if crate::pages::settings::switch_row(ui, wid("games.switch"), row, label, Some(on), accent)
        && st.settings.pro
    {
        st.settings.game_stats = !on;
        st.cmd(Command::SaveSettings);
    }
    // The readout stays over the game, or peeks for a few seconds at a time
    // (between them nothing covers the game: no added delay).
    let (row, rest) = rest.split_top(ROW_H);
    let names = GAME_READOUT.map(|r| r.1);
    let x = row.right() - crate::pages::settings::picker_width(ui, &names);
    ui.c.text(
        "Readout",
        Rect::new(row.x, row.y, (x - 10.0 - row.x).max(0.0), row.h),
        &TextStyle::body(),
    );
    let sel = GAME_READOUT
        .iter()
        .position(|r| r.0 == st.settings.game_peek)
        .unwrap_or(0);
    if let Some(i) = crate::pages::settings::picker(
        ui,
        "games.readout",
        x,
        row.center().y - 12.0,
        24.0,
        &names,
        sel,
    ) {
        st.settings.game_peek = GAME_READOUT[i].0;
        st.cmd(Command::SaveSettings);
    }
    let (about, rest) = rest.split_top(28.0);
    let about_text = if st.settings.game_peek {
        "A few seconds as a game starts, when FPS drops and on Alt+Shift+F."
    } else {
        "Stays over the game (about a frame of delay). Alt+Shift+F hides it."
    };
    ui.c.text(
        about_text,
        about.inset(0.0, 1.0),
        &TextStyle::caption()
            .color(palette::TEXT_DIM)
            .wrap()
            .top()
            .line_spacing(1.2),
    );

    // What the readout shows beside the GPU's load.
    let (head, rest) = rest.split_top(SEP_H);
    separator(ui, head, "READOUT SHOWS");
    let (row, rest) = rest.split_top(ROW_H);
    if crate::pages::settings::switch_row(
        ui,
        wid("games.fps"),
        row,
        "FPS",
        Some(st.settings.game_fps),
        accent,
    ) {
        st.settings.game_fps = !st.settings.game_fps;
        st.cmd(Command::SaveSettings);
    }
    let (row, rest) = rest.split_top(ROW_H);
    if crate::pages::settings::switch_row(
        ui,
        wid("games.temp"),
        row,
        "GPU temperature",
        Some(st.settings.game_temp),
        accent,
    ) {
        st.settings.game_temp = !st.settings.game_temp;
        st.cmd(Command::SaveSettings);
    }
    let (row, rest) = rest.split_top(ROW_H);
    if crate::pages::settings::switch_row(
        ui,
        wid("games.cpu"),
        row,
        "CPU load",
        Some(st.settings.game_cpu),
        accent,
    ) {
        st.settings.game_cpu = !st.settings.game_cpu;
        st.cmd(Command::SaveSettings);
    }
    let (row, rest) = rest.split_top(ROW_H);
    if crate::pages::settings::switch_row(
        ui,
        wid("games.cpu_temp"),
        row,
        "CPU temperature",
        Some(st.settings.game_cpu_temp),
        accent,
    ) {
        st.settings.game_cpu_temp = !st.settings.game_cpu_temp;
        st.cmd(Command::SaveSettings);
    }
    // The refresh rate: a click steps to the next one, Auto after the last.
    let (row, rest) = rest.split_top(ROW_H + 2.0);
    let name = hz_name(st.settings.game_hz);
    let w = ui.pill_width("360 Hz", false, true);
    let pill = Rect::new(row.right() - w, row.bottom() - PILL_SMALL, w, PILL_SMALL);
    ui.c.text(
        "Refresh rate",
        Rect::new(row.x, pill.y, (pill.x - 10.0 - row.x).max(0.0), pill.h),
        &TextStyle::body(),
    );
    if ui
        .pill(wid("games.hz"), pill, &name, None, PillStyle::Neutral)
        .clicked
    {
        let at = GAME_HZ
            .iter()
            .position(|h| *h == st.settings.game_hz)
            .unwrap_or(0);
        st.settings.game_hz = GAME_HZ[(at + 1) % GAME_HZ.len()];
        st.cmd(Command::SaveSettings);
    }
    if cpu_temp_missing(st) {
        let (note, _) = rest.split_top(16.0);
        ui.c.text(
            "No CPU temp sensor reported by Windows.",
            note,
            &TextStyle::caption().color(palette::TEXT_FAINT),
        );
    }
}

/// The frame rate's last minute (running out from the left as the page
/// opens, as System's charts do), and the frame times under it.
fn draw_chart(ui: &mut Ui, st: &AppState, r: Rect) {
    let (chart, sub) = history_column(ui, r, "FRAMES PER SECOND", "FRAME TIME");
    chart_box(ui, chart);
    let h = &st.game.history;
    if h.values().any(f32::is_finite) {
        let top = fps_top(st);
        plot(ui, h, top, st.accent, chart.inset(1.0, 1.0), Fill::Gradient);
        let scale = TextStyle::caption().tabular().color(palette::TEXT_FAINT);
        ui.c.text(
            &format!("{top:.0}"),
            Rect::new(chart.x + 7.0, chart.y + 3.0, 40.0, 14.0),
            &scale,
        );
    } else {
        let waiting = if !st.settings.game_stats {
            "Off"
        } else if st.game.active {
            "No frames yet"
        } else {
            "Start a game in borderless full screen"
        };
        ui.c.text(
            waiting,
            chart.inset(8.0, 0.0),
            &TextStyle::caption().color(palette::TEXT_FAINT).center(),
        );
    }
    let rate = st.game.rate.filter(|_| st.game.active);
    let times = [
        Row::plain(
            "Average",
            rate.map_or_else(|| PLACEHOLDER.into(), |r| fmt_frame_ms(r.frame_ms)),
        ),
        Row::plain(
            "Slowest in the last second",
            rate.map_or_else(|| PLACEHOLDER.into(), |r| fmt_frame_ms(r.worst_ms)),
        ),
    ];
    for (row, rect) in times.iter().zip(stack(sub, DETAIL_H)) {
        row.draw(ui, rect);
    }
}

/// What's in front: its frame rate ring between the GPU's load and
/// temperature, its name, the CPU and video memory, and the frames' access.
fn draw_front(ui: &mut Ui, st: &mut AppState, r: Rect) {
    let accent = st.accent;
    let (head, rest) = r.split_top(SEP_H);
    separator(ui, head, "IN FRONT");
    let rate = st.game.rate.filter(|_| st.game.active);
    let reading = st.system.latest.as_ref().filter(|_| st.game.active);
    let gpu = reading.and_then(|r| r.gpu.as_ref());
    let left = gpu.map_or_else(
        || Gauge::unknown("GPU"),
        |g| Gauge {
            frac: Some(g.util),
            value: fmt_pct(g.util),
            label: "GPU",
            color: accent,
        },
    );
    let right = gpu
        .and_then(|g| g.temp_c)
        .map_or_else(|| Gauge::unknown("Temperature"), temperature);
    let ring = match rate {
        Some(r) => Ring {
            parts: vec![(r.fps / fps_top(st), accent)],
            value: fmt_fps(r.fps),
        },
        None => Ring {
            parts: Vec::new(),
            value: PLACEHOLDER.into(),
        },
    };
    let (dash, rest) = rest.split_top(DASH_H);
    dashboard(ui, dash, &left, &ring, &right);

    let (name_r, rest) = rest.split_top(DETAIL_H);
    let (name, ink) = match (st.game.active, &st.game.app) {
        (true, Some(app)) => (app.clone(), palette::TEXT),
        (true, None) => ("A full-screen app".to_string(), palette::TEXT),
        (false, _) => ("No game in front".to_string(), palette::TEXT_FAINT),
    };
    ui.c.text(
        &name,
        name_r,
        &TextStyle::footnote().weight(Weight::Semibold).color(ink),
    );
    let rows = [
        Row::plain(
            "CPU",
            cpu_row_value(reading.and_then(|r| r.cpu.as_ref()).map(|c| (c.load, c.temp_c)), st.settings.game_cpu_temp),
        ),
        Row::plain(
            "Video memory",
            gpu.map_or_else(|| PLACEHOLDER.into(), |g| fmt_bytes(g.mem_used)),
        ),
    ];
    let (rows_r, rest) = rest.split_top(2.0 * DETAIL_H);
    for (row, rect) in rows.iter().zip(stack(rows_r, DETAIL_H)) {
        row.draw(ui, rect);
    }

    // Whether Windows lets Halo read frames, with its one button.
    if st.game.access == FpsAccess::Unknown {
        st.cmd(Command::CheckFps);
    }
    let (text, action) = match &st.game.access {
        FpsAccess::Unknown | FpsAccess::Checking => ("Checking with Windows\u{2026}", None),
        FpsAccess::Ready => ("FPS ready: counted from Windows' own log of frames.", None),
        FpsAccess::NeedsOk => (
            "FPS needs a one-time OK from Windows.",
            Some(("Allow FPS", Command::AllowFps)),
        ),
        FpsAccess::Asking => ("Answer Windows' prompt to allow FPS.", None),
        FpsAccess::SignOut => ("Allowed. Sign out and back in, and FPS shows.", None),
        // Why goes to halo.log; the page says it plainly.
        FpsAccess::Failed(_) => (
            "Windows didn't allow FPS this time.",
            Some(("Check again", Command::CheckFps)),
        ),
    };
    let (text_r, rest) = rest.split_top(30.0);
    ui.c.text(
        text,
        text_r.inset(0.0, 2.0),
        &TextStyle::caption()
            .color(palette::TEXT_DIM)
            .wrap()
            .top()
            .line_spacing(1.2),
    );
    if let Some((label, cmd)) = action {
        let w = ui.pill_width(label, false, true);
        if ui
            .pill(
                wid("games.allow"),
                Rect::new(rest.x, rest.y, w, PILL_SMALL),
                label,
                None,
                PillStyle::Primary(accent),
            )
            .clicked
        {
            st.cmd(cmd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fps_scale_follows_the_refresh_rate() {
        use halo_core::settings::Settings;
        use halo_core::store::Store;
        let mut st = AppState::new(Settings::default(), Store::default());
        st.game.history.push(Some(130.0));
        // Auto: the nearest common rate above the minute's peak.
        assert_eq!(fps_top(&st), 144.0);
        st.settings.game_hz = 240;
        assert_eq!(fps_top(&st), 240.0);
        assert_eq!(fps_ceiling(400.0), 480.0);
        assert_eq!(hz_name(0), "Auto");
        assert_eq!(hz_name(165), "165 Hz");
    }

    #[test]
    fn the_settings_column_fits_the_page() {
        // OVER GAMES, two rows and the note; READOUT SHOWS, four switches, rate and optional sensor note.
        let used = SEP_H + 2.0 * ROW_H + 28.0 + SEP_H + 4.0 * ROW_H + (ROW_H + 2.0) + 16.0;
        assert!(used <= HEIGHT, "{used}");
        // IN FRONT: the dashboard, the name, two rows, the note and its button.
        let used = SEP_H + DASH_H + 3.0 * DETAIL_H + 30.0 + PILL_SMALL;
        assert!(used <= HEIGHT, "{used}");
    }

    #[test]
    fn cpu_row_notes_missing_sensor_when_enabled() {
        assert_eq!(cpu_row_value(Some((0.42, None)), false), "42%");
        assert_eq!(cpu_row_value(Some((0.42, None)), true), "42% \u{b7} no sensor");
        assert_eq!(cpu_row_value(Some((0.42, Some(64.4))), true), "42% \u{b7} 64\u{b0}C");
        assert_eq!(cpu_row_value(None, true), PLACEHOLDER);
    }
}
