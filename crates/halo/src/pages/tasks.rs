//! Tasks page: a quick to-do list.
//!
//! The input is the list's first row (a plus in the checkbox column; Enter
//! adds and keeps focus for rapid entry, Esc leaves the field), the tasks
//! below with circle checkboxes, hover-only delete and smooth wheel scrolling
//! with fading edges, and a footer with today's progress and "Clear done".
//! Delete and "Clear done" can be undone for a few seconds.
//!
//! Rows glide to their new place when a task is checked off (done tasks sink
//! below the open ones), but only once the pointer leaves the list (or after
//! a moment), so the next row never slides under the cursor. New rows fade
//! in and the checkbox pops. All of it is event-driven: nothing animates
//! while idle. Done tasks sit on a faint green band. As the
//! page opens (`pages::entrance`) the bands rise in and the checkboxes pop,
//! row by row, the footer's ring draws and its pill pops; the words stay put.
//! A row's hover fades in and its delete button pops in; checking off the
//! day's last task sends a green ripple out of the footer's ring.

use super::entrance::{hover, rise, scaled, Entrance};
use super::PageKind;
use crate::gfx::TextStyle;
use crate::icons::Icon;
use crate::state::AppState;
use crate::ui::{wid, wid_n, KeyInput, PillStyle, Ui, WidgetId, PILL_SMALL};
use halo_core::geom::{palette, Color, Point, Rect};
use halo_core::todo::{Task, MAX_DEPTH, MAX_TASKS, MAX_TASK_LEN};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};

const HEIGHT: f32 = 200.0;
const INPUT_H: f32 = 34.0;
const ROW_H: f32 = 31.0;
/// Indentation step per tree level (title = 0, task = 12, subtask = 24).
const INDENT: f32 = 12.0;
/// List viewport, relative to the page top: exactly four rows.
const LIST_TOP: f32 = 44.0;
const LIST_H: f32 = 4.0 * ROW_H;
/// The footer holds a Small capsule, so it is exactly that tall.
const FOOTER_H: f32 = PILL_SMALL;
/// Checkbox / ring / plus column centre and the text column, from the row's left edge.
const MARK_X: f32 = 18.0;
const TEXT_X: f32 = 36.0;
/// The neutral track under every progress ring.
const RING_TRACK: Color = Color::white(0.14);
/// Height of the fade at a scrollable edge.
const FADE: f32 = 14.0;
const FADE_STEPS: usize = 7;
/// Scroll distance per wheel notch.
const WHEEL_STEP: f32 = ROW_H * 1.5;
/// Exponential smoothing rate (1/s) for scrolling and row movement.
const GLIDE_RATE: f32 = 16.0;
const POP_S: f64 = 0.32;
const APPEAR_S: f64 = 0.24;
/// After a toggle the rows keep their places while the pointer stays over the list, at most this long.
const FREEZE_S: f64 = 2.0;
/// How long "Undo" is offered after a delete or "Clear done".
const UNDO_SECS: f64 = 8.0;
/// Maximum interval between two clicks on a task's text to open inline edit.
const DBLCLICK_S: f64 = 0.40;
/// When the page hasn't been drawn for this long, jump to the current state.
const STALE_S: f64 = 1.0;

#[derive(Default)]
struct ListAnim {
    last_draw: f64,
    /// Displayed scroll offset (eases toward `st.ui.task_scroll`).
    scroll: f32,
    /// Displayed y of each row inside the list content, by task id.
    rows: HashMap<u64, f32>,
    /// Tasks added (or restored) from this page → when (for a fade-in).
    appeared: HashMap<u64, f64>,
    /// Last checkbox toggled from this page (pop + strike-through sweep).
    toggled: Option<(u64, f64)>,
    /// Display order (task ids) frozen at the last toggle, and when.
    frozen: Option<(Vec<u64>, f64)>,
    /// Tasks removed by delete / "Clear done" and when, for "Undo".
    undo: Option<(Vec<Task>, f64)>,
    /// Last click on a row's text area, for double-click inline rename.
    last_click: Option<(u64, f64)>,
}

thread_local! {
    static ANIM: Cell<ListAnim> = Cell::new(ListAnim::default());
}

fn ease_out_cubic(t: f32) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

/// Move `from` toward `to` with frame-rate independent smoothing; snaps when
/// close (also the Downloads page's scroll).
pub(crate) fn glide(from: f32, to: f32, dt: f32) -> (f32, bool) {
    let v = from + (to - from) * (1.0 - (-GLIDE_RATE * dt).exp());
    if (to - v).abs() < 0.3 {
        (to, false)
    } else {
        (v, true)
    }
}

fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = a.right().min(b.right());
    let y1 = a.bottom().min(b.bottom());
    (x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
}

/// Reorder `items` to follow a frozen display order; ids it doesn't know
/// (just added) keep their relative order after the known ones.
fn apply_frozen<T>(items: &mut [(u64, T)], order: &[u64]) {
    let slot: HashMap<u64, usize> = order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    items.sort_by_key(|(id, _)| slot.get(id).copied().unwrap_or(usize::MAX));
}

pub fn height(_st: &AppState) -> f32 {
    HEIGHT
}

struct RowMeta {
    done: bool,
    text: String,
    depth: usize,
    has_children: bool,
    collapsed: bool,
    progress: (usize, usize),
}

/// Visible rows in DFS display order, omitting descendants of collapsed rows.
fn visible_items(st: &AppState, day_start: i64) -> Vec<(u64, RowMeta)> {
    let ordered = st.store.tasks.display_order(day_start);
    let has_child: HashSet<u64> = ordered.iter().filter_map(|t| t.parent).collect();
    let parent_of: HashMap<u64, Option<u64>> = st.store.tasks.tasks.iter().map(|t| (t.id, t.parent)).collect();
    let mut out = Vec::with_capacity(ordered.len());
    for t in ordered {
        let mut hidden = false;
        let mut cur = t.parent;
        let mut seen = HashSet::new();
        while let Some(pid) = cur {
            if !seen.insert(pid) {
                break;
            }
            if st.ui.task_collapsed.contains(&pid) {
                hidden = true;
                break;
            }
            cur = parent_of.get(&pid).copied().flatten();
        }
        if hidden {
            continue;
        }
        let has_children = has_child.contains(&t.id);
        let progress = if has_children { st.store.tasks.branch_progress(t.id) } else { (0, 0) };
        out.push((
            t.id,
            RowMeta {
                done: t.done,
                text: t.text.clone(),
                depth: st.store.tasks.depth(t.id).max(1),
                has_children,
                collapsed: st.ui.task_collapsed.contains(&t.id),
                progress,
            },
        ));
    }
    out
}

pub fn draw(ui: &mut Ui, st: &mut AppState, area: Rect) {
    let now = st.now;
    let mut anim = ANIM.with(Cell::take);
    let stale = now - anim.last_draw > STALE_S;
    if stale {
        anim.rows.clear();
        anim.appeared.clear();
        anim.toggled = None;
        anim.frozen = None;
        anim.last_click = None;
    }
    // "Undo" lapses after UNDO_SECS, but never under a press that is about to click it.
    let undo_id = wid("tasks.undo");
    let undo_held = ui.input.pressed_id == Some(undo_id) || ui.input.clicked.is_some_and(|(id, _)| id == undo_id);
    if anim.undo.as_ref().is_some_and(|(_, t0)| now - t0 >= UNDO_SECS) && !undo_held {
        anim.undo = None;
    }
    let dt = if stale { 0.0 } else { (now - anim.last_draw).clamp(0.0, 0.05) as f32 };
    let mut moving = false;
    let e = Entrance::of(ui, st, PageKind::Tasks);
    let day_start = st.day_start_unix();
    if st.store.tasks.tasks.len() >= MAX_TASKS {
        make_room(st, day_start);
    }
    let full = st.store.tasks.tasks.len() >= MAX_TASKS;

    // Clear target parent if it was removed or can no longer accept children.
    if let Some(pid) = st.ui.task_parent {
        let d = st.store.tasks.depth(pid);
        if d == 0 || d >= MAX_DEPTH {
            st.ui.task_parent = None;
        }
    }
    // Cancel inline edit if the task was removed.
    if let Some(eid) = st.ui.task_editing {
        if !st.store.tasks.tasks.iter().any(|t| t.id == eid) {
            st.ui.task_editing = None;
            st.ui.task_edit_buf.clear();
        }
    }

    // ---- Input: the list's first row, sharing its checkbox and text columns.
    let input_id = wid("tasks.input");
    let input_r = Rect::new(area.x, area.y, area.w, INPUT_H);
    let placeholder = if full {
        "Task list is full".to_string()
    } else if let Some(pid) = st.ui.task_parent {
        let name = st.store.tasks.tasks.iter().find(|t| t.id == pid).map(|t| t.text.as_str()).unwrap_or("");
        format!("Add under {name}")
    } else {
        "Add a task, Enter to save".to_string()
    };
    let accent = st.accent;
    let resp = ui.text_input_row(input_id, input_r, &mut st.ui.task_input, &placeholder, MAX_TASK_LEN, accent, Some(Icon::Plus));
    if resp.cancelled && st.ui.task_parent.is_some() {
        st.ui.task_parent = None;
        ui.focus(input_id, st.ui.task_input.chars().count());
    } else if !resp.focused && st.ui.task_parent.is_some() && ui.input.keys.contains(&KeyInput::Escape) {
        st.ui.task_parent = None;
        ui.request_repaint();
    }
    if resp.submitted {
        submit(ui, st, &mut anim, day_start);
    }

    // ---- List.
    let view = Rect::new(area.x, area.y + LIST_TOP, area.w, LIST_H);
    let footer_r = Rect::new(area.x, area.y + HEIGHT - FOOTER_H, area.w, FOOTER_H);
    let mut items = visible_items(st, day_start);
    // Keep the order of the last toggle while the pointer is still over the list.
    let over_list = ui.hovered(view);
    if !anim.frozen.as_ref().is_some_and(|(_, t0)| over_list && now - t0 < FREEZE_S) {
        anim.frozen = None;
    }
    if let Some((order, _)) = &anim.frozen {
        apply_frozen(&mut items, order);
    }

    if items.is_empty() {
        let hint_area = Rect::new(area.x, view.y, area.w, HEIGHT - LIST_TOP);
        super::empty_state(ui, hint_area, Icon::ListChecks, "Nothing on your list", "Done tasks clear at the end of the day", e);
        st.ui.task_scroll = 0.0;
        anim.scroll = 0.0;
        anim.rows.clear();
        if anim.undo.is_some() {
            undo_pill(ui, st, &mut anim, undo_id, footer_r);
        }
    } else {
        let max_scroll = (items.len() as f32 * ROW_H - LIST_H).max(0.0);
        if max_scroll > 0.0 {
            let notches = ui.wheel_in(view);
            st.ui.task_scroll -= notches * WHEEL_STEP;
        }
        st.ui.task_scroll = st.ui.task_scroll.clamp(0.0, max_scroll);
        let (scroll, m) = if stale { (st.ui.task_scroll, false) } else { glide(anim.scroll, st.ui.task_scroll, dt) };
        anim.scroll = scroll.clamp(0.0, max_scroll);
        moving |= m;
        let scrolling = m;

        let mut toggle = None;
        let mut text_click = None;
        let mut commit_edit = false;
        let mut cancel_edit = false;
        let mut delete = None;
        let mut fold = None;
        let mut add_under = None;
        let active_parent = st.ui.task_parent;
        let editing_id = st.ui.task_editing;
        let chev_gutter = if items.iter().any(|(_, m)| m.has_children || m.depth > 1) { 10.0 } else { 0.0 };
        let mut rows = HashMap::with_capacity(items.len());
        ui.c.clip(view);
        for (i, (id, meta)) in items.iter().enumerate() {
            let target = i as f32 * ROW_H;
            let (y, m) = match anim.rows.get(id) {
                Some(&y0) => glide(y0, target, dt),
                None => (target, false),
            };
            moving |= m;
            rows.insert(*id, y);
            let appear = anim.appeared.get(id).map(|&t0| ((now - t0) / APPEAR_S).clamp(0.0, 1.0) as f32).unwrap_or(1.0);
            if appear < 1.0 {
                moving = true;
            }
            let row = Rect::new(area.x, view.y + y - anim.scroll + 6.0 * (1.0 - ease_out_cubic(appear)), area.w, ROW_H);
            let Some(hit) = intersect(row, view) else { continue };
            let pop = match anim.toggled {
                Some((tid, t0)) if tid == *id => {
                    let t = ((now - t0) / POP_S).clamp(0.0, 1.0) as f32;
                    if t < 1.0 {
                        moving = true;
                    }
                    Some(t)
                }
                _ => None,
            };
            let prev_alpha = ui.c.alpha();
            ui.c.set_alpha(prev_alpha * appear);
            // A row on the move glides between pixels; at rest it sits on whole pixels.
            let snap_was = ui.c.set_text_snap(!(scrolling || m || appear < 1.0));
            let k = e.pop(0.06 + 0.05 * i.min(6) as f32);
            let action = draw_row(ui, st, row, hit, view, *id, meta, chev_gutter, active_parent == Some(*id), editing_id == Some(*id), pop, k);
            ui.c.set_text_snap(snap_was);
            ui.c.set_alpha(prev_alpha);
            match action {
                RowAction::Toggle => toggle = Some(*id),
                RowAction::TextClick => text_click = Some(*id),
                RowAction::CommitEdit => commit_edit = true,
                RowAction::CancelEdit => cancel_edit = true,
                RowAction::Delete => delete = Some(*id),
                RowAction::ToggleFold => fold = Some(*id),
                RowAction::AddUnder => add_under = Some(*id),
                RowAction::None => {}
            }
        }
        // Fade content out at edges that can scroll further.
        if anim.scroll > 0.5 {
            fade(ui, Rect::new(view.x, view.y, view.w, FADE), true);
        }
        if anim.scroll < max_scroll - 0.5 {
            fade(ui, Rect::new(view.x, view.bottom() - FADE, view.w, FADE), false);
        }
        ui.c.unclip();
        anim.rows = rows;

        if commit_edit {
            if let Some(id) = st.ui.task_editing.take() {
                if st.store.tasks.rename(id, &st.ui.task_edit_buf) {
                    st.save();
                    st.toast("Task renamed");
                }
            }
            st.ui.task_edit_buf.clear();
            ui.blur();
        } else if cancel_edit {
            st.ui.task_editing = None;
            st.ui.task_edit_buf.clear();
        }
        if let Some(id) = fold {
            if !st.ui.task_collapsed.remove(&id) {
                st.ui.task_collapsed.insert(id);
            }
            anim.frozen = None;
            ui.request_repaint();
        }
        if let Some(id) = add_under {
            if st.ui.task_parent == Some(id) {
                st.ui.task_parent = None;
            } else {
                st.ui.task_parent = Some(id);
                st.ui.task_collapsed.remove(&id);
                ui.focus(input_id, st.ui.task_input.chars().count());
            }
        }
        if let Some(id) = text_click {
            if anim.last_click.is_some_and(|(lid, t0)| lid == id && now - t0 <= DBLCLICK_S) {
                // Second click within DBLCLICK_S: undo the first click's toggle and open inline rename.
                if st.store.tasks.toggle(id, st.unix) {
                    st.save();
                }
                anim.toggled = None;
                anim.frozen = None;
                anim.last_click = None;
                if let Some(t) = st.store.tasks.tasks.iter().find(|t| t.id == id) {
                    st.ui.task_editing = Some(id);
                    st.ui.task_edit_buf = t.text.clone();
                    ui.focus(wid_n("tasks.edit", id), st.ui.task_edit_buf.chars().count());
                }
            } else {
                anim.last_click = Some((id, now));
                toggle = Some(id);
            }
        } else if toggle.is_some() {
            anim.last_click = None;
        }
        if let Some(id) = toggle {
            if st.store.tasks.toggle(id, st.unix) {
                st.save();
                anim.toggled = Some((id, now));
                // Freeze what is on screen now; the rows re-sort once the pointer leaves.
                anim.frozen = Some((items.iter().map(|(id, _)| *id).collect(), now));
            }
        }
        if let Some(id) = delete {
            let removed = st.store.tasks.remove_subtree(id);
            if !removed.is_empty() {
                for t in &removed {
                    anim.rows.remove(&t.id);
                    anim.appeared.remove(&t.id);
                    st.ui.task_collapsed.remove(&t.id);
                    if st.ui.task_parent == Some(t.id) {
                        st.ui.task_parent = None;
                    }
                    if st.ui.task_editing == Some(t.id) {
                        st.ui.task_editing = None;
                        st.ui.task_edit_buf.clear();
                    }
                }
                st.save();
                offer_undo(&mut anim, removed, now);
            }
        }

        draw_footer(ui, st, footer_r, day_start, full, &mut anim, undo_id, e);
    }

    anim.appeared.retain(|_, t0| now - *t0 < APPEAR_S);
    if anim.toggled.is_some_and(|(_, t0)| now - t0 >= POP_S) {
        anim.toggled = None;
    }
    if moving {
        ui.request_anim();
    }
    anim.last_draw = now;
    ANIM.with(|c| c.set(anim));
}

/// Add the typed task (at root or under `st.ui.task_parent`). Keeps keyboard
/// focus (rapid entry) and scrolls the new row into view. A full list keeps
/// the text so nothing typed is lost.
fn submit(ui: &mut Ui, st: &mut AppState, anim: &mut ListAnim, day_start: i64) {
    if st.ui.task_input.trim().is_empty() {
        st.ui.task_input.clear();
        ui.input.caret = 0;
        return;
    }
    let parent = st.ui.task_parent;
    let Some(id) = st.store.tasks.add_under(parent, &st.ui.task_input, st.unix) else { return };
    st.ui.task_input.clear();
    ui.input.caret = 0;
    // Expand the parent chain so the newly added row is visible.
    let mut cur = st.store.tasks.tasks.iter().find(|t| t.id == id).and_then(|t| t.parent);
    while let Some(pid) = cur {
        st.ui.task_collapsed.remove(&pid);
        cur = st.store.tasks.tasks.iter().find(|t| t.id == pid).and_then(|t| t.parent);
    }
    st.save();
    anim.appeared.insert(id, st.now);
    // The new row goes to its sorted place, so stop holding the old order.
    anim.frozen = None;
    if let Some(idx) = visible_items(st, day_start).iter().position(|(tid, _)| *tid == id) {
        let y = idx as f32 * ROW_H;
        if y + ROW_H > st.ui.task_scroll + LIST_H {
            st.ui.task_scroll = y + ROW_H - LIST_H;
        } else if y < st.ui.task_scroll {
            st.ui.task_scroll = y;
        }
    }
}

/// Remember removed tasks for "Undo". Removals made while "Undo" is still
/// showing join the same snapshot, so one click brings all of them back.
fn offer_undo(anim: &mut ListAnim, removed: Vec<Task>, now: f64) {
    if removed.is_empty() {
        return;
    }
    match anim.undo.as_mut() {
        Some((tasks, t0)) => {
            tasks.extend(removed);
            *t0 = now;
        }
        None => anim.undo = Some((removed, now)),
    }
}

/// Put the snapshot back (parents before children so subtrees survive).
fn restore(st: &mut AppState, anim: &mut ListAnim) {
    let Some((tasks, _)) = anim.undo.take() else { return };
    let restored = st.store.tasks.restore(tasks);
    for id in &restored {
        anim.appeared.insert(*id, st.now);
    }
    if !restored.is_empty() {
        st.save();
    }
    anim.frozen = None;
}

/// Done tasks from earlier days are never shown (`display_order` lists only
/// today's), yet they count toward `MAX_TASKS`. Once the list is full, drop
/// those hidden ones so the user isn't blocked by tasks they cannot see.
fn make_room(st: &mut AppState, day_start: i64) {
    let removed = st.store.tasks.prune_done_before(day_start);
    if removed > 0 {
        crate::pinfo!("tasks: list was full, removed {removed} done tasks from earlier days");
        st.save();
    }
}

enum RowAction {
    None,
    Toggle,
    TextClick,
    CommitEdit,
    CancelEdit,
    Delete,
    ToggleFold,
    AddUnder,
}

/// One task row. `hit` is the part of `row` inside the list viewport `view`.
/// `k` is the page's entrance for this row: its band rises in and its
/// checkbox pops (1 at rest). Its hover fades in (`entrance::hover`).
#[allow(clippy::too_many_arguments)]
fn draw_row(
    ui: &mut Ui,
    st: &mut AppState,
    row: Rect,
    hit: Rect,
    view: Rect,
    id: u64,
    meta: &RowMeta,
    chev_gutter: f32,
    is_add_target: bool,
    editing: bool,
    pop: Option<f32>,
    k: f32,
) -> RowAction {
    let mut action = RowAction::None;
    let cy = row.center().y;
    let indent = (meta.depth.saturating_sub(1)) as f32 * INDENT;
    let can_add = meta.depth < MAX_DEPTH;
    let tx = row.x + indent + TEXT_X + chev_gutter;
    let right_pad = if can_add { 60.0 } else { 36.0 };

    let row_id = wid_n("tasks.row", id);
    let (hovered, pressed, clicked) = if editing {
        (false, false, false)
    } else {
        let resp = ui.button(row_id, hit);
        (resp.hovered, resp.pressed, resp.clicked)
    };
    let h = hover(ui, st, row_id, hovered);
    let band = row.inset(2.0, 2.0);
    if pressed {
        ui.c.fill_rrect(band, 9.0, palette::SURFACE_HOVER);
    } else {
        let rest = if meta.done { palette::GREEN.with_alpha(0.10) } else { Color::white(0.0) };
        let fill = Color::lerp(rest, palette::SURFACE, h);
        if fill.a > 0.0 {
            rise(ui, k, |ui| ui.c.fill_rrect(band, 9.0, fill));
        }
    }
    if clicked {
        action = if ui.input.mouse.is_some_and(|p| p.x >= tx - 4.0 && p.x < row.right() - right_pad) {
            RowAction::TextClick
        } else {
            RowAction::Toggle
        };
    }

    // Subtle vertical tree guide lines aligned under each ancestor's chevron.
    for lvl in 1..meta.depth {
        let gx = row.x + (lvl - 1) as f32 * INDENT + 10.0;
        ui.c.line(Point::new(gx, row.y + 3.0), Point::new(gx, row.bottom() - 3.0), Color::white(0.10), 1.0);
    }

    // Disclosure chevron for rows with children (UI-only collapse state).
    if meta.has_children {
        let chev_r = Rect::new(row.x + indent + 1.0, cy - 12.0, 18.0, 24.0);
        if let Some(chev_hit) = intersect(chev_r, view) {
            let icon = if meta.collapsed { Icon::ChevronRight } else { Icon::ChevronDown };
            let col = if ui.hovered(chev_hit) { palette::TEXT } else { palette::TEXT_DIM };
            if ui.icon_button(wid_n("tasks.fold", id), chev_hit, icon, col, 11.0).clicked {
                action = RowAction::ToggleFold;
            }
        }
    }

    // Checkbox (pops when toggled).
    let scale = pop.map(|t| 1.0 + 0.22 * (t * std::f32::consts::PI).sin()).unwrap_or(1.0);
    let c = Point::new(row.x + indent + MARK_X + chev_gutter, cy);
    let size = 18.0 * scale * k.max(0.01);
    let icon_r = Rect::from_center(c, size, size);
    if meta.done {
        ui.c.fill_circle(c, size * 9.0 / 24.0, palette::GREEN.with_alpha(0.16));
        ui.c.icon(Icon::CheckCircle, icon_r, palette::GREEN);
    } else {
        ui.c.icon(Icon::Circle, icon_r, if hovered { palette::TEXT_DIM } else { palette::TEXT_FAINT });
    }

    if editing {
        let edit_r = Rect::new(tx - 4.0, row.y + 3.0, (row.right() - 8.0 - (tx - 4.0)).max(24.0), row.h - 6.0);
        let eresp = ui.text_input(wid_n("tasks.edit", id), edit_r, &mut st.ui.task_edit_buf, "Task name", MAX_TASK_LEN);
        if eresp.submitted {
            return RowAction::CommitEdit;
        }
        if eresp.cancelled || !eresp.focused {
            return RowAction::CancelEdit;
        }
        return action;
    }

    // Progress badge (`done/total`) on parent rows, right before the "+" button.
    let mut text_right = row.right() - right_pad;
    if meta.has_children && meta.progress.1 > 0 {
        let badge = format!("{}/{}", meta.progress.0, meta.progress.1);
        let bstyle = TextStyle::footnote().color(if meta.done { palette::GREEN } else { palette::TEXT_DIM });
        let text_w = ui.c.measure(&badge, &bstyle, 48.0).0;
        let bw = (text_w + 10.0).ceil();
        let bx = text_right - bw;
        if bx > tx + 16.0 {
            let badge_r = Rect::new(bx, cy - 8.0, bw, 16.0);
            let bfill = if meta.done { palette::GREEN.with_alpha(0.16) } else { Color::white(0.07) };
            ui.c.fill_rrect(badge_r, 8.0, bfill);
            ui.c.text(&badge, Rect::new(badge_r.x + (bw - text_w) * 0.5, badge_r.y, text_w + 2.0, badge_r.h), &bstyle);
            text_right = bx - 6.0;
        }
    }

    // Text, struck through when done (the line sweeps in right after checking).
    let tw = (text_right - tx).max(0.0);
    let style = TextStyle::body().color(if meta.done { palette::TEXT_FAINT } else { palette::TEXT });
    ui.c.text(&meta.text, Rect::new(tx, row.y, tw, row.h), &style);
    if meta.done && tw > 0.0 {
        let w = ui.c.measure(&meta.text, &style, tw).0.min(tw);
        let sweep = pop.map(ease_out_cubic).unwrap_or(1.0);
        if sweep > 0.0 {
            let y = cy + 1.0;
            ui.c.line(Point::new(tx - 1.0, y), Point::new(tx - 1.0 + (w + 2.0) * sweep, y), palette::TEXT_FAINT, 1.2);
        }
    }

    // Small "+" on rows that can accept children (depth 1 and 2).
    let row_visible = cy - 12.0 >= view.y && cy + 12.0 <= view.bottom();
    if can_add && row_visible {
        let add_r = Rect::new(row.right() - 56.0, cy - 12.0, 24.0, 24.0);
        if is_add_target {
            ui.c.fill_rrect(add_r, 12.0, st.accent.with_alpha(0.18));
        }
        let add_col = if is_add_target {
            st.accent
        } else if ui.hovered(add_r) {
            palette::TEXT
        } else if hovered {
            palette::TEXT_DIM
        } else {
            palette::TEXT_FAINT.with_alpha(0.55)
        };
        if ui.icon_button(wid_n("tasks.add", id), add_r, Icon::Plus, add_col, 12.0).clicked {
            action = RowAction::AddUnder;
        }
    }

    // Delete, only while hovering a fully visible row.
    let del = Rect::new(row.right() - 30.0, cy - 12.0, 24.0, 24.0);
    if hovered && row_visible {
        let color = if ui.hovered(del) { palette::RED } else { palette::TEXT_DIM };
        if scaled(ui, del.center(), h, |ui| ui.icon_button(wid_n("tasks.del", id), del, Icon::Trash, color, 14.0)).clicked {
            action = RowAction::Delete;
        }
    }
    action
}

/// Darken `r` toward the island colour; `top` = strongest at the top edge.
/// Glass has no one island colour to fade into, so the list just ends there.
/// Also the Downloads page's list.
pub(crate) fn fade(ui: &mut Ui, r: Rect, top: bool) {
    if ui.c.look().material == halo_core::look::Material::Glass {
        return;
    }
    let step = r.h / FADE_STEPS as f32;
    for k in 0..FADE_STEPS {
        // k = 0 is the strip at the edge.
        let a = 0.92 * (1.0 - (k as f32 + 0.5) / FADE_STEPS as f32).powf(1.4);
        let y = if top { r.y + k as f32 * step } else { r.bottom() - (k as f32 + 1.0) * step };
        ui.c.fill_rect(Rect::new(r.x, y, r.w, step), palette::ISLAND.with_alpha(a));
    }
}

/// A Small Utility capsule with an accent "Undo" label, right-aligned in
/// `row`. Returns its left edge.
fn undo_pill(ui: &mut Ui, st: &mut AppState, anim: &mut ListAnim, id: WidgetId, row: Rect) -> f32 {
    let accent = st.accent;
    let w = ui.pill_width("Undo", true, true);
    let r = Rect::new(row.right() - w, row.y, w, row.h);
    let clicked = ui.pill(id, r, "Undo", Some(Icon::Undo), PillStyle::UtilityAccent(accent)).clicked;
    if clicked {
        restore(st, anim);
    }
    r.x
}

/// "1/3 done today" with a small ring, then "Clear done" when anything is
/// done, or "Undo" for a few seconds after a removal.
#[allow(clippy::too_many_arguments)]
fn draw_footer(ui: &mut Ui, st: &mut AppState, r: Rect, day_start: i64, full: bool, anim: &mut ListAnim, undo_id: WidgetId, e: Entrance) {
    let (done, total) = st.store.tasks.today_progress(day_start);
    let cy = r.center().y;
    let frac = if total > 0 { done as f32 / total as f32 } else { 0.0 } * e.draw(0.15, 0.6);
    let ring_c = Point::new(r.x + MARK_X, cy);
    ui.c.ring(ring_c, 6.0, 2.2, frac, RING_TRACK, palette::GREEN);
    // The day's last task just checked off: a ripple out of the ring (its
    // frames are the checkbox pop's, asked for by `draw`).
    if let Some((_, t0)) = anim.toggled.filter(|_| total > 0 && done == total) {
        let t = ((st.now - t0) / POP_S).clamp(0.0, 1.0) as f32;
        if t < 1.0 {
            ui.c.stroke_circle(ring_c, 7.0 + 10.0 * ease_out_cubic(t), palette::GREEN.with_alpha(0.5 * (1.0 - t)), 1.5);
        }
    }

    let show_clear = done > 0;
    let mut text_right = r.right();
    if anim.undo.is_some() {
        text_right = undo_pill(ui, st, anim, undo_id, r) - 12.0;
    } else if show_clear {
        let w = ui.pill_width("Clear done", false, true);
        let pr = Rect::new(r.right() - w, r.y, w, r.h);
        text_right = pr.x - 12.0;
        if scaled(ui, pr.center(), e.pop(0.2), |ui| ui.pill(wid("tasks.clear"), pr, "Clear done", None, PillStyle::Utility)).clicked {
            let removed = st.store.tasks.clear_done_taking();
            if !removed.is_empty() {
                for t in &removed {
                    st.ui.task_collapsed.remove(&t.id);
                    if st.ui.task_parent == Some(t.id) {
                        st.ui.task_parent = None;
                    }
                    if st.ui.task_editing == Some(t.id) {
                        st.ui.task_editing = None;
                        st.ui.task_edit_buf.clear();
                    }
                }
                // The scroll offset is clamped to the shorter list on the next frame.
                st.save();
                let n = removed.len();
                offer_undo(anim, removed, st.now);
                st.toast(if n == 1 { "Cleared 1 task".to_string() } else { format!("Cleared {n} tasks") });
                anim.toggled = None;
                anim.frozen = None;
            }
        }
    }

    let tx = r.x + TEXT_X;
    let text_r = Rect::new(tx, r.y, (text_right - tx).max(0.0), r.h);
    let (text, color) = if full && show_clear {
        ("List is full — clear done tasks to add more".to_string(), palette::ORANGE)
    } else if full {
        (format!("List is full ({MAX_TASKS} tasks)"), palette::ORANGE)
    } else if total > 0 && done == total {
        ("All done for today".to_string(), palette::GREEN)
    } else {
        (format!("{done}/{total} done today"), palette::TEXT_DIM)
    };
    ui.c.text(&text, text_r, &TextStyle::footnote().color(color));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_intersection() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert_eq!(intersect(a, Rect::new(5.0, 5.0, 10.0, 10.0)), Some(Rect::new(5.0, 5.0, 5.0, 5.0)));
        assert_eq!(intersect(a, Rect::new(10.0, 0.0, 5.0, 5.0)), None);
    }

    #[test]
    fn glide_converges_and_snaps() {
        let (mut v, mut moving) = (0.0f32, true);
        let mut frames = 0;
        while moving && frames < 200 {
            (v, moving) = glide(v, 100.0, 1.0 / 60.0);
            frames += 1;
        }
        assert_eq!(v, 100.0);
        assert!(frames < 40, "took {frames} frames");
        assert_eq!(glide(5.0, 5.0, 0.0), (5.0, false));
    }

    #[test]
    fn frozen_order_holds_and_appends_newcomers() {
        // After toggling 2 (which would sink below 3), the frozen order keeps it in place.
        let mut items = vec![(1u64, ()), (3, ()), (4, ()), (2, ())];
        apply_frozen(&mut items, &[1, 2, 3]);
        let ids: Vec<u64> = items.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![1, 2, 3, 4]);
    }

    #[test]
    fn undo_snapshots_accumulate() {
        let task = |id: u64| Task { id, text: format!("t{id}"), done: false, created_unix: 0, done_unix: None, parent: None };
        let mut anim = ListAnim::default();
        offer_undo(&mut anim, vec![task(1)], 1.0);
        offer_undo(&mut anim, vec![task(2)], 3.0);
        offer_undo(&mut anim, Vec::new(), 4.0);
        let (tasks, t0) = anim.undo.expect("snapshot");
        assert_eq!(tasks.iter().map(|t| t.id).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(t0, 3.0);
    }
}

/// When the page next changes on its own (Undo lapsing, frozen order ending).
pub fn next_wake(_st: &AppState) -> Option<f64> {
    ANIM.with(|c| {
        let a = c.take();
        let undo = a.undo.as_ref().map(|(_, t0)| t0 + UNDO_SECS);
        let frozen = a.frozen.as_ref().map(|(_, t0)| t0 + FREEZE_S);
        c.set(a);
        match (undo, frozen) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (x, y) => x.or(y),
        }
    })
}
