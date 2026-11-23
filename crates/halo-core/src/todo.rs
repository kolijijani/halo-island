//! A small personal task list with up to three levels: title > task > subtask.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const MAX_TASK_LEN: usize = 200;
pub const MAX_TASKS: usize = 200;
pub const MAX_DEPTH: usize = 3;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: u64,
    pub text: String,
    pub done: bool,
    pub created_unix: i64,
    #[serde(default)]
    pub done_unix: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "TaskListRaw")]
pub struct TaskList {
    pub tasks: Vec<Task>,
    pub next_id: u64,
}

#[derive(Deserialize)]
#[serde(default)]
struct TaskListRaw {
    tasks: Vec<Task>,
    next_id: u64,
}

impl Default for TaskListRaw {
    fn default() -> Self {
        Self { tasks: Vec::new(), next_id: 1 }
    }
}

impl From<TaskListRaw> for TaskList {
    fn from(raw: TaskListRaw) -> Self {
        let mut list = Self { tasks: raw.tasks, next_id: raw.next_id };
        list.recompute();
        list
    }
}

impl Default for TaskList {
    fn default() -> Self {
        Self { tasks: Vec::new(), next_id: 1 }
    }
}

impl TaskList {
    /// Add a root task; returns its id, or `None` for blank text / a full list.
    pub fn add(&mut self, text: &str, now_unix: i64) -> Option<u64> {
        self.add_under(None, text, now_unix)
    }

    /// Add a task under `parent` (or at root when `None`). Depth clamps at
    /// `MAX_DEPTH`, so adding under a depth-3 subtask attaches to its depth-2
    /// parent. Returns `None` for blank text or a full list.
    pub fn add_under(&mut self, parent: Option<u64>, text: &str, now_unix: i64) -> Option<u64> {
        let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() || self.tasks.len() >= MAX_TASKS {
            return None;
        }
        let text: String = text.chars().take(MAX_TASK_LEN).collect();
        let resolved_parent = parent.and_then(|pid| self.clamp_parent(pid));
        let max_id = self.tasks.iter().map(|t| t.id).max().unwrap_or(0);
        let id = self.next_id.max(max_id + 1).max(1);
        self.next_id = id + 1;
        self.tasks.push(Task {
            id,
            text,
            done: false,
            created_unix: now_unix,
            done_unix: None,
            parent: resolved_parent,
        });
        self.recompute();
        Some(id)
    }

    /// 1-based depth (1 = title, 2 = task, 3 = subtask), or 0 if `id` is absent.
    pub fn depth(&self, id: u64) -> usize {
        let parents: HashMap<u64, Option<u64>> = self.tasks.iter().map(|t| (t.id, t.parent)).collect();
        if !parents.contains_key(&id) {
            return 0;
        }
        let mut depth = 1;
        let mut cur = parents[&id];
        let mut seen = HashSet::new();
        seen.insert(id);
        while let Some(pid) = cur {
            if !seen.insert(pid) || !parents.contains_key(&pid) {
                break;
            }
            depth += 1;
            cur = parents[&pid];
        }
        depth
    }

    pub fn has_children(&self, id: u64) -> bool {
        self.tasks.iter().any(|t| t.parent == Some(id))
    }

    /// Rename `id` in place (trimmed, single-spaced, clamped to `MAX_TASK_LEN`).
    /// Returns `true` when the task's text changed.
    pub fn rename(&mut self, id: u64, text: &str) -> bool {
        let clean: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if clean.is_empty() {
            return false;
        }
        let clean: String = clean.chars().take(MAX_TASK_LEN).collect();
        let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) else { return false };
        if t.text == clean {
            return false;
        }
        t.text = clean;
        true
    }

    /// `(done_leaves, total_leaves)` among descendants of `id` (`(0, 0)` for a leaf).
    pub fn branch_progress(&self, id: u64) -> (usize, usize) {
        let desc = self.descendants(id);
        if desc.is_empty() {
            return (0, 0);
        }
        let parents = self.parent_ids();
        let mut done = 0;
        let mut total = 0;
        for t in &self.tasks {
            if desc.contains(&t.id) && !parents.contains(&t.id) {
                total += 1;
                if t.done {
                    done += 1;
                }
            }
        }
        (done, total)
    }

    /// Clamp a requested parent so a new child never exceeds `MAX_DEPTH`.
    fn clamp_parent(&self, pid: u64) -> Option<u64> {
        let parents: HashMap<u64, Option<u64>> = self.tasks.iter().map(|t| (t.id, t.parent)).collect();
        if !parents.contains_key(&pid) {
            return None;
        }
        let mut chain = vec![pid];
        let mut cur = parents[&pid];
        let mut seen = HashSet::new();
        seen.insert(pid);
        while let Some(p) = cur {
            if !seen.insert(p) || !parents.contains_key(&p) {
                break;
            }
            chain.push(p);
            cur = parents[&p];
        }
        chain.reverse(); // [root, ..., pid]
        let idx = (MAX_DEPTH - 2).min(chain.len() - 1);
        Some(chain[idx])
    }

    fn parent_ids(&self) -> HashSet<u64> {
        self.tasks.iter().filter_map(|t| t.parent).collect()
    }

    /// All descendants of `id` (excluding `id` itself).
    fn descendants(&self, id: u64) -> HashSet<u64> {
        let mut out = HashSet::new();
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            for t in &self.tasks {
                if t.parent == Some(cur) && out.insert(t.id) {
                    stack.push(t.id);
                }
            }
        }
        out
    }

    /// Normalize parent links (orphan/cycle/depth clamp) and derive every
    /// parent's `done` and `done_unix` strictly from its descendants.
    pub fn recompute(&mut self) {
        let ids: HashSet<u64> = self.tasks.iter().map(|t| t.id).collect();
        for t in &mut self.tasks {
            if let Some(pid) = t.parent {
                if pid == t.id || !ids.contains(&pid) {
                    t.parent = None;
                }
            }
        }
        // Break any cycles in parent pointers.
        let mut parent_of: HashMap<u64, Option<u64>> = self.tasks.iter().map(|t| (t.id, t.parent)).collect();
        for t in &mut self.tasks {
            let mut seen = HashSet::new();
            seen.insert(t.id);
            let mut cur = t.parent;
            while let Some(pid) = cur {
                if !seen.insert(pid) {
                    t.parent = None;
                    parent_of.insert(t.id, None);
                    break;
                }
                cur = parent_of.get(&pid).copied().flatten();
            }
        }
        // Clamp any node deeper than MAX_DEPTH up to the depth-(MAX_DEPTH-1) ancestor.
        let parent_of: HashMap<u64, Option<u64>> = self.tasks.iter().map(|t| (t.id, t.parent)).collect();
        for t in &mut self.tasks {
            let mut chain = Vec::new();
            let mut cur = t.parent;
            let mut seen = HashSet::new();
            seen.insert(t.id);
            while let Some(pid) = cur {
                if !seen.insert(pid) {
                    break;
                }
                chain.push(pid);
                cur = parent_of.get(&pid).copied().flatten();
            }
            if chain.len() >= MAX_DEPTH {
                chain.reverse(); // [root, depth2, ...]
                t.parent = Some(chain[MAX_DEPTH - 2]);
            }
        }

        // Derive parent `done` / `done_unix` bottom-up: depth 2, then depth 1.
        let depths: HashMap<u64, usize> = self.tasks.iter().map(|t| (t.id, self.depth(t.id))).collect();
        for level in (1..MAX_DEPTH).rev() {
            let updates: Vec<(u64, bool, Option<i64>)> = self
                .tasks
                .iter()
                .filter(|t| depths.get(&t.id).copied() == Some(level))
                .filter_map(|p| {
                    let mut has_child = false;
                    let mut all_done = true;
                    let mut latest: Option<i64> = None;
                    for c in &self.tasks {
                        if c.parent == Some(p.id) {
                            has_child = true;
                            if !c.done {
                                all_done = false;
                            }
                            if let Some(ts) = c.done_unix {
                                latest = Some(latest.map_or(ts, |m| m.max(ts)));
                            }
                        }
                    }
                    has_child.then_some((p.id, all_done, if all_done { latest } else { None }))
                })
                .collect();
            for (pid, done, done_unix) in updates {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == pid) {
                    t.done = done;
                    t.done_unix = done_unix;
                }
            }
        }
    }

    pub fn toggle(&mut self, id: u64, now_unix: i64) -> bool {
        if !self.tasks.iter().any(|t| t.id == id) {
            return false;
        }
        let desc = self.descendants(id);
        if desc.is_empty() {
            if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
                t.done = !t.done;
                t.done_unix = if t.done { Some(now_unix) } else { None };
            }
        } else {
            let parents = self.parent_ids();
            let want_done = self.tasks.iter().any(|t| desc.contains(&t.id) && !parents.contains(&t.id) && !t.done);
            for t in &mut self.tasks {
                if desc.contains(&t.id) && !parents.contains(&t.id) {
                    if want_done {
                        if !t.done {
                            t.done = true;
                            t.done_unix = Some(now_unix);
                        }
                    } else {
                        t.done = false;
                        t.done_unix = None;
                    }
                }
            }
        }
        self.recompute();
        true
    }

    /// Remove `id` and its entire subtree, returning the removed tasks (for undo).
    pub fn remove_subtree(&mut self, id: u64) -> Vec<Task> {
        if !self.tasks.iter().any(|t| t.id == id) {
            return Vec::new();
        }
        let mut to_remove = self.descendants(id);
        to_remove.insert(id);
        let mut removed = Vec::new();
        let mut kept = Vec::with_capacity(self.tasks.len());
        for t in self.tasks.drain(..) {
            if to_remove.contains(&t.id) {
                removed.push(t);
            } else {
                kept.push(t);
            }
        }
        self.tasks = kept;
        self.recompute();
        removed
    }

    pub fn remove(&mut self, id: u64) -> bool {
        !self.remove_subtree(id).is_empty()
    }

    /// Remove done leaves and any parent left with no children; returns the
    /// removed tasks (for undo).
    pub fn clear_done_taking(&mut self) -> Vec<Task> {
        self.recompute();
        let was_parent = self.parent_ids();
        let mut removed = Vec::new();
        let mut kept = Vec::with_capacity(self.tasks.len());
        for t in self.tasks.drain(..) {
            if !was_parent.contains(&t.id) && t.done {
                removed.push(t);
            } else {
                kept.push(t);
            }
        }
        self.tasks = kept;
        loop {
            let still_parent = self.parent_ids();
            let mut next_kept = Vec::with_capacity(self.tasks.len());
            let mut dropped = false;
            for t in self.tasks.drain(..) {
                if was_parent.contains(&t.id) && !still_parent.contains(&t.id) {
                    removed.push(t);
                    dropped = true;
                } else {
                    next_kept.push(t);
                }
            }
            self.tasks = next_kept;
            if !dropped {
                break;
            }
        }
        self.recompute();
        removed
    }

    pub fn clear_done(&mut self) -> usize {
        self.clear_done_taking().len()
    }

    /// Drop hidden done leaves from earlier days and any parent left empty.
    pub fn prune_done_before(&mut self, day_start_unix: i64) -> usize {
        self.recompute();
        let before = self.tasks.len();
        let was_parent = self.parent_ids();
        self.tasks.retain(|t| {
            was_parent.contains(&t.id) || !t.done || t.done_unix.is_some_and(|d| d >= day_start_unix)
        });
        loop {
            let still_parent = self.parent_ids();
            let n = self.tasks.len();
            self.tasks.retain(|t| !was_parent.contains(&t.id) || still_parent.contains(&t.id));
            if self.tasks.len() == n {
                break;
            }
        }
        self.recompute();
        before - self.tasks.len()
    }

    /// Put a snapshot of removed tasks back (parents before children so
    /// subtrees survive even if the list approaches `MAX_TASKS`). Returns the
    /// restored ids.
    pub fn restore(&mut self, mut tasks: Vec<Task>) -> Vec<u64> {
        let batch_parent: HashMap<u64, Option<u64>> = tasks.iter().map(|t| (t.id, t.parent)).collect();
        let batch_depth = |id: u64| -> usize {
            let mut d = 0usize;
            let mut cur = batch_parent.get(&id).copied().flatten();
            let mut seen = HashSet::new();
            seen.insert(id);
            while let Some(pid) = cur {
                if !seen.insert(pid) || !batch_parent.contains_key(&pid) {
                    break;
                }
                d += 1;
                cur = batch_parent.get(&pid).copied().flatten();
            }
            d
        };
        tasks.sort_by_key(|t| (batch_depth(t.id), t.done, std::cmp::Reverse(t.done_unix)));
        let mut restored = Vec::new();
        for t in tasks {
            if self.tasks.len() >= MAX_TASKS {
                break;
            }
            if self.tasks.iter().any(|x| x.id == t.id) {
                continue;
            }
            self.next_id = self.next_id.max(t.id + 1);
            restored.push(t.id);
            self.tasks.push(t);
        }
        if !restored.is_empty() {
            self.recompute();
        }
        restored
    }

    pub fn open_count(&self) -> usize {
        let parents = self.parent_ids();
        self.tasks.iter().filter(|t| !parents.contains(&t.id) && !t.done).count()
    }

    /// Leaf tasks completed at or after `day_start_unix`.
    pub fn done_since(&self, day_start_unix: i64) -> usize {
        let parents = self.parent_ids();
        self.tasks
            .iter()
            .filter(|t| !parents.contains(&t.id) && t.done && t.done_unix.is_some_and(|d| d >= day_start_unix))
            .count()
    }

    /// "done today / (open + done today)" — e.g. 1/3. Leaves only.
    pub fn today_progress(&self, day_start_unix: i64) -> (usize, usize) {
        let done = self.done_since(day_start_unix);
        (done, done + self.open_count())
    }

    /// Pre-order DFS display order: at each level, open tasks first (oldest
    /// first), then today's done tasks (newest first).
    pub fn display_order(&self, day_start_unix: i64) -> Vec<&Task> {
        let mut out = Vec::with_capacity(self.tasks.len());
        self.collect_display(None, day_start_unix, &mut out);
        out
    }

    fn collect_display<'a>(&'a self, parent: Option<u64>, day_start_unix: i64, out: &mut Vec<&'a Task>) {
        let mut open: Vec<&Task> = self.tasks.iter().filter(|t| t.parent == parent && !t.done).collect();
        let mut done: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|t| t.parent == parent && t.done && t.done_unix.is_some_and(|d| d >= day_start_unix))
            .collect();
        open.sort_by_key(|t| (t.created_unix, t.id));
        done.sort_by_key(|t| (std::cmp::Reverse(t.done_unix), t.id));
        for t in open.into_iter().chain(done) {
            out.push(t);
            self.collect_display(Some(t.id), day_start_unix, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(list: &TaskList, id: u64) -> &Task {
        list.tasks.iter().find(|t| t.id == id).expect("task exists")
    }

    #[test]
    fn add_toggle_remove() {
        let mut l = TaskList::default();
        assert_eq!(l.add("   ", 0), None);
        let a = l.add("  buy   milk ", 10).unwrap();
        let b = l.add("ship halo", 20).unwrap();
        assert_eq!(l.tasks[0].text, "buy milk");
        assert!(l.toggle(a, 100));
        assert_eq!(l.today_progress(50), (1, 2));
        assert_eq!(l.today_progress(200), (0, 1));
        let order: Vec<u64> = l.display_order(50).iter().map(|t| t.id).collect();
        assert_eq!(order, vec![b, a]);
        assert_eq!(l.clear_done(), 1);
        assert!(l.remove(b));
        assert!(!l.remove(b));
    }

    #[test]
    fn truncates_long_text() {
        let mut l = TaskList::default();
        let id = l.add(&"x".repeat(500), 0).unwrap();
        assert_eq!(find(&l, id).text.len(), MAX_TASK_LEN);
    }

    #[test]
    fn ids_survive_default_zero() {
        let mut l = TaskList { tasks: vec![], next_id: 0 };
        assert_eq!(l.add("a", 0), Some(1));
        assert_eq!(l.add("b", 0), Some(2));
    }

    #[test]
    fn old_flat_json_loads_as_root() {
        let json = r#"{
            "tasks": [
                { "id": 1, "text": "buy milk", "done": false, "created_unix": 10 },
                { "id": 2, "text": "ship halo", "done": true, "created_unix": 20, "done_unix": 100 }
            ],
            "next_id": 3
        }"#;
        let list: TaskList = serde_json::from_str(json).expect("deserialize flat json");
        assert_eq!(list.tasks.len(), 2);
        for t in &list.tasks {
            assert_eq!(t.parent, None);
            assert_eq!(list.depth(t.id), 1);
        }
        assert!(!find(&list, 1).done);
        assert!(find(&list, 2).done);
    }

    #[test]
    fn depth_clamps_at_3() {
        let mut l = TaskList::default();
        let title = l.add("Title", 1).unwrap();
        let task = l.add_under(Some(title), "Task", 2).unwrap();
        let sub = l.add_under(Some(task), "Subtask", 3).unwrap();
        let clamped = l.add_under(Some(sub), "Would be 4th", 4).unwrap();
        assert_eq!(l.depth(title), 1);
        assert_eq!(l.depth(task), 2);
        assert_eq!(l.depth(sub), 3);
        assert_eq!(l.depth(clamped), 3);
        assert_eq!(find(&l, clamped).parent, Some(task));

        // Loaded JSON with depth 4 also clamps to depth 3.
        let json = r#"{
            "tasks": [
                { "id": 1, "text": "L1", "done": false, "created_unix": 1 },
                { "id": 2, "text": "L2", "done": false, "created_unix": 2, "parent": 1 },
                { "id": 3, "text": "L3", "done": false, "created_unix": 3, "parent": 2 },
                { "id": 4, "text": "L4", "done": false, "created_unix": 4, "parent": 3 }
            ],
            "next_id": 5
        }"#;
        let loaded: TaskList = serde_json::from_str(json).expect("deserialize");
        assert_eq!(loaded.depth(4), 3);
        assert_eq!(find(&loaded, 4).parent, Some(2));
    }

    #[test]
    fn parent_cannot_be_hand_ticked() {
        // Hand-set parent done:true with an open child is cleared on load.
        let json = r#"{
            "tasks": [
                { "id": 1, "text": "Title", "done": true, "created_unix": 1, "done_unix": 99 },
                { "id": 2, "text": "Open child", "done": false, "created_unix": 2, "parent": 1 }
            ],
            "next_id": 3
        }"#;
        let mut l: TaskList = serde_json::from_str(json).expect("deserialize");
        assert!(!find(&l, 1).done);
        assert_eq!(find(&l, 1).done_unix, None);

        // Mutating parent.done directly and recomputing restores derived state.
        l.tasks.iter_mut().find(|t| t.id == 1).unwrap().done = true;
        l.recompute();
        assert!(!find(&l, 1).done);

        // Hand-set parent done:false with all children done is ticked on load.
        let json_all_done = r#"{
            "tasks": [
                { "id": 1, "text": "Title", "done": false, "created_unix": 1 },
                { "id": 2, "text": "Done child", "done": true, "created_unix": 2, "done_unix": 150, "parent": 1 }
            ],
            "next_id": 3
        }"#;
        let loaded: TaskList = serde_json::from_str(json_all_done).expect("deserialize");
        assert!(find(&loaded, 1).done);
        assert_eq!(find(&loaded, 1).done_unix, Some(150));

        // Adding an open child under an already-done leaf reopens it.
        let mut l2 = TaskList::default();
        let a = l2.add("Done leaf", 1).unwrap();
        l2.toggle(a, 50);
        assert!(find(&l2, a).done);
        let _child = l2.add_under(Some(a), "New child", 60).unwrap();
        assert!(!find(&l2, a).done);
        assert_eq!(find(&l2, a).done_unix, None);
    }

    #[test]
    fn ticking_parent_ticks_subtree() {
        let mut l = TaskList::default();
        let title = l.add("Title", 1).unwrap();
        let t1 = l.add_under(Some(title), "Task 1", 2).unwrap();
        let t2 = l.add_under(Some(title), "Task 2", 3).unwrap();
        let s1 = l.add_under(Some(t1), "Sub 1", 4).unwrap();
        let s2 = l.add_under(Some(t1), "Sub 2", 5).unwrap();

        assert!(l.toggle(title, 100));
        for id in [title, t1, t2, s1, s2] {
            assert!(find(&l, id).done, "node {id} should be done");
            assert_eq!(find(&l, id).done_unix, Some(100));
        }

        // Unticking parent clears every descendant.
        assert!(l.toggle(title, 200));
        for id in [title, t1, t2, s1, s2] {
            assert!(!find(&l, id).done, "node {id} should be open");
            assert_eq!(find(&l, id).done_unix, None);
        }
    }

    #[test]
    fn unticking_one_child_reopens_two_ancestors() {
        let mut l = TaskList::default();
        let title = l.add("Title", 1).unwrap();
        let task = l.add_under(Some(title), "Task", 2).unwrap();
        let s1 = l.add_under(Some(task), "Sub 1", 3).unwrap();
        let s2 = l.add_under(Some(task), "Sub 2", 4).unwrap();

        l.toggle(s1, 100);
        assert!(!find(&l, task).done);
        assert!(!find(&l, title).done);

        // Ticking the last child ticks both ancestors.
        l.toggle(s2, 120);
        assert!(find(&l, task).done);
        assert!(find(&l, title).done);
        assert_eq!(find(&l, title).done_unix, Some(120));

        // Unticking one child reopens both ancestors.
        l.toggle(s1, 130);
        assert!(!find(&l, s1).done);
        assert!(find(&l, s2).done);
        assert!(!find(&l, task).done);
        assert!(!find(&l, title).done);
        assert_eq!(find(&l, task).done_unix, None);
        assert_eq!(find(&l, title).done_unix, None);
    }

    #[test]
    fn counters_ignore_parents() {
        let mut l = TaskList::default();
        let title = l.add("Title", 1).unwrap();
        let task = l.add_under(Some(title), "Task", 2).unwrap();
        let s1 = l.add_under(Some(task), "Sub 1", 3).unwrap();
        let s2 = l.add_under(Some(task), "Sub 2", 4).unwrap();
        let flat = l.add("Flat leaf", 5).unwrap();

        // 3 leaves total: s1, s2, flat. title & task are parents.
        assert_eq!(l.open_count(), 3);
        assert_eq!(l.done_since(50), 0);
        assert_eq!(l.today_progress(50), (0, 3));

        // Complete s1 and s2 -> task & title become done too, yet only 2 leaves count.
        l.toggle(s1, 100);
        l.toggle(s2, 110);
        assert!(find(&l, task).done);
        assert!(find(&l, title).done);
        assert_eq!(l.open_count(), 1);
        assert_eq!(l.done_since(50), 2);
        assert_eq!(l.today_progress(50), (2, 3));

        // Complete flat too.
        l.toggle(flat, 120);
        assert_eq!(l.open_count(), 0);
        assert_eq!(l.done_since(50), 3);
        assert_eq!(l.today_progress(50), (3, 3));
    }

    #[test]
    fn clear_done_keeps_unfinished_branch_and_drops_done_children() {
        let mut l = TaskList::default();
        // Unfinished branch: title -> (done leaf task, open task -> (done sub, open sub))
        let keep_title = l.add("Launch", 1).unwrap();
        let done_task = l.add_under(Some(keep_title), "Done task", 2).unwrap();
        let open_task = l.add_under(Some(keep_title), "Open task", 3).unwrap();
        let done_sub = l.add_under(Some(open_task), "Done sub", 4).unwrap();
        let open_sub = l.add_under(Some(open_task), "Open sub", 5).unwrap();
        // Finished branch: should disappear completely.
        let done_title = l.add("Old project", 6).unwrap();
        let done_child = l.add_under(Some(done_title), "Done child", 7).unwrap();

        l.toggle(done_task, 100);
        l.toggle(done_sub, 100);
        l.toggle(done_child, 100);
        assert!(find(&l, done_title).done);
        assert!(!find(&l, keep_title).done);

        let removed = l.clear_done();
        assert_eq!(removed, 4); // done_task, done_sub, done_child, done_title

        let remaining: Vec<u64> = l.tasks.iter().map(|t| t.id).collect();
        assert_eq!(remaining, vec![keep_title, open_task, open_sub]);
        assert!(!find(&l, keep_title).done);
        assert!(!find(&l, open_task).done);
    }

    #[test]
    fn undo_restores_subtree() {
        let mut l = TaskList::default();
        let title = l.add("Title", 1).unwrap();
        let task = l.add_under(Some(title), "Task", 2).unwrap();
        let sub_done = l.add_under(Some(task), "Sub done", 3).unwrap();
        let sub_open = l.add_under(Some(task), "Sub open", 4).unwrap();
        let other = l.add("Other", 5).unwrap();
        l.toggle(sub_done, 100);

        let snap = l.remove_subtree(title);
        assert_eq!(snap.len(), 4);
        assert_eq!(l.tasks.len(), 1);
        assert_eq!(l.tasks[0].id, other);

        let restored = l.restore(snap);
        assert_eq!(restored.len(), 4);
        assert_eq!(l.tasks.len(), 5);
        assert_eq!(find(&l, task).parent, Some(title));
        assert_eq!(find(&l, sub_done).parent, Some(task));
        assert_eq!(find(&l, sub_open).parent, Some(task));
        assert!(find(&l, sub_done).done);
        assert!(!find(&l, sub_open).done);
        assert!(!find(&l, task).done);
        assert!(!find(&l, title).done);
    }

    #[test]
    fn max_tasks_counts_total_nodes() {
        let mut l = TaskList::default();
        let root = l.add("Root", 1).unwrap();
        let child = l.add_under(Some(root), "Child", 2).unwrap();
        for i in 2..MAX_TASKS {
            assert!(l.add_under(Some(child), &format!("s{i}"), i as i64).is_some());
        }
        assert_eq!(l.tasks.len(), MAX_TASKS);
        assert_eq!(l.add("Overflow root", 999), None);
        assert_eq!(l.add_under(Some(root), "Overflow child", 999), None);
    }

    #[test]
    fn rename_updates_and_clamps_text() {
        let mut l = TaskList::default();
        let id = l.add("Old   name", 1).unwrap();
        assert!(!l.rename(id, "   "));
        assert!(!l.rename(id, "Old name"));
        assert!(!l.rename(999, "Missing"));
        assert!(l.rename(id, "  New   title  "));
        assert_eq!(find(&l, id).text, "New title");

        let long = "a".repeat(MAX_TASK_LEN + 20);
        assert!(l.rename(id, &long));
        assert_eq!(find(&l, id).text.chars().count(), MAX_TASK_LEN);
    }

    #[test]
    fn branch_progress_counts_descendant_leaves_only() {
        let mut l = TaskList::default();
        let title = l.add("Trip", 1).unwrap();
        let task = l.add_under(Some(title), "Ferry", 2).unwrap();
        let sub1 = l.add_under(Some(task), "Check times", 3).unwrap();
        let sub2 = l.add_under(Some(task), "Buy ticket", 4).unwrap();
        let leaf = l.add_under(Some(title), "Pack bag", 5).unwrap();

        assert_eq!(l.branch_progress(sub1), (0, 0));
        assert_eq!(l.branch_progress(task), (0, 2));
        assert_eq!(l.branch_progress(title), (0, 3));

        l.toggle(sub1, 10);
        assert_eq!(l.branch_progress(task), (1, 2));
        assert_eq!(l.branch_progress(title), (1, 3));

        l.toggle(sub2, 11);
        assert_eq!(l.branch_progress(task), (2, 2));
        assert_eq!(l.branch_progress(title), (2, 3));

        l.toggle(leaf, 12);
        assert_eq!(l.branch_progress(title), (3, 3));
    }
}
