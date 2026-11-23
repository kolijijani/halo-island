//! The Shelf: a temporary stash of file/folder references dropped onto the island.

use serde::{Deserialize, Serialize};

pub const MAX_SHELF_ITEMS: usize = 24;
/// Maximum pinned quick-launch apps/files kept at the front of the Shelf.
pub const MAX_PINNED_SHELF: usize = 6;

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShelfItem {
    pub path: String,
    pub added_unix: i64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub pinned: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shelf {
    pub items: Vec<ShelfItem>,
}

impl Shelf {
    fn sort_pinned_first(&mut self) {
        let mut pinned = Vec::new();
        let mut rest = Vec::new();
        for it in self.items.drain(..) {
            if it.pinned {
                pinned.push(it);
            } else {
                rest.push(it);
            }
        }
        self.items = pinned;
        self.items.extend(rest);
    }

    /// Add paths (newest first after any pinned items), de-duplicated case-insensitively.
    /// Pinned items stay at the front and are never evicted by `MAX_SHELF_ITEMS`.
    pub fn add_paths<S: AsRef<str>>(&mut self, paths: &[S], now_unix: i64) -> usize {
        let mut added = 0;
        for p in paths {
            let p = p.as_ref().trim();
            if p.is_empty() {
                continue;
            }
            let key = p.to_lowercase();
            if let Some(i) = self.items.iter().position(|it| it.path.to_lowercase() == key) {
                let it = self.items.remove(i);
                let pos = if it.pinned { 0 } else { self.pinned_count() };
                self.items.insert(pos, ShelfItem { added_unix: now_unix, ..it });
                continue;
            }
            let pos = self.pinned_count();
            self.items.insert(pos, ShelfItem { path: p.to_string(), added_unix: now_unix, pinned: false });
            added += 1;
        }
        self.sort_pinned_first();
        self.items.truncate(MAX_SHELF_ITEMS);
        added
    }

    pub fn pinned_count(&self) -> usize {
        self.items.iter().filter(|it| it.pinned).count()
    }

    /// Pin or unpin `path`. Returns `Some(true)` if now pinned, `Some(false)` if
    /// unpinned, or `None` if absent or already at `MAX_PINNED_SHELF`.
    pub fn toggle_pin(&mut self, path: &str) -> Option<bool> {
        let key = path.to_lowercase();
        let idx = self.items.iter().position(|it| it.path.to_lowercase() == key)?;
        if !self.items[idx].pinned && self.pinned_count() >= MAX_PINNED_SHELF {
            return None;
        }
        let next = !self.items[idx].pinned;
        self.items[idx].pinned = next;
        self.sort_pinned_first();
        Some(next)
    }

    pub fn remove(&mut self, path: &str) -> bool {
        let before = self.items.len();
        let key = path.to_lowercase();
        self.items.retain(|it| it.path.to_lowercase() != key);
        before != self.items.len()
    }

    /// Clear unpinned items, preserving pinned quick-launchers. Returns removed count.
    pub fn clear_unpinned(&mut self) -> usize {
        let before = self.items.len();
        self.items.retain(|it| it.pinned);
        before - self.items.len()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn paths(&self) -> Vec<String> {
        self.items.iter().map(|i| i.path.clone()).collect()
    }
}

/// True for executables and Windows app shortcuts suitable for Quick Launcher tiles.
pub fn is_app_shortcut(path: &str) -> bool {
    matches!(extension(path).as_str(), "exe" | "lnk" | "appref-ms" | "bat" | "cmd")
}

/// Clean tile label: strips `.lnk`/`.exe` on app shortcuts ("Code.exe" → "Code").
pub fn tile_label(path: &str) -> String {
    let name = display_name(path);
    if is_app_shortcut(path) {
        if let Some((stem, _)) = name.rsplit_once('.') {
            if !stem.is_empty() {
                return stem.to_string();
            }
        }
    }
    name
}

/// Last path component for display ("C:\a\b\report.pdf" → "report.pdf").
pub fn display_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['\\', '/']);
    let name = trimmed.rsplit(['\\', '/']).next().unwrap_or(trimmed);
    if name.is_empty() {
        path.to_string()
    } else {
        name.to_string()
    }
}

/// Lower-case extension without the dot ("" for none / folders).
pub fn extension(path: &str) -> String {
    let name = display_name(path);
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() && ext.len() <= 8 => ext.to_lowercase(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_dedupes_and_orders() {
        let mut s = Shelf::default();
        assert_eq!(s.add_paths(&["C:\\a.txt", "C:\\b.png"], 1), 2);
        assert_eq!(s.items[0].path, "C:\\b.png");
        assert_eq!(s.add_paths(&["c:\\A.TXT"], 2), 0);
        assert_eq!(s.items[0].path, "C:\\a.txt");
        assert_eq!(s.items.len(), 2);
        assert!(s.remove("C:\\B.png"));
        assert_eq!(s.items.len(), 1);
    }

    #[test]
    fn caps_items() {
        let mut s = Shelf::default();
        let paths: Vec<String> = (0..40).map(|i| format!("C:\\f{i}")).collect();
        s.add_paths(&paths, 0);
        assert_eq!(s.items.len(), MAX_SHELF_ITEMS);
        assert_eq!(s.items[0].path, "C:\\f39");
    }

    #[test]
    fn names_and_extensions() {
        assert_eq!(display_name("C:\\Users\\x\\report.final.PDF"), "report.final.PDF");
        assert_eq!(extension("C:\\Users\\x\\report.final.PDF"), "pdf");
        assert_eq!(display_name("C:\\Projects\\"), "Projects");
        assert_eq!(extension("C:\\Projects"), "");
        assert_eq!(extension("C:\\.gitignore"), "");
    }

    #[test]
    fn pin_stays_first_and_survives_clear_and_cap() {
        let mut s = Shelf::default();
        s.add_paths(&["C:\\Apps\\Code.exe", "C:\\tmp\\a.txt"], 1);
        assert_eq!(s.toggle_pin("C:\\Apps\\Code.exe"), Some(true));
        assert_eq!(s.items[0].path, "C:\\Apps\\Code.exe");
        // Adding > MAX_SHELF_ITEMS keeps the pinned launcher at index 0.
        let extra: Vec<String> = (0..30).map(|i| format!("C:\\f{i}.txt")).collect();
        s.add_paths(&extra, 2);
        assert_eq!(s.items.len(), MAX_SHELF_ITEMS);
        assert_eq!(s.items[0].path, "C:\\Apps\\Code.exe");
        assert!(s.items[0].pinned);
        // Clear unpinned leaves pinned launchers intact.
        assert_eq!(s.clear_unpinned(), MAX_SHELF_ITEMS - 1);
        assert_eq!(s.items.len(), 1);
        assert_eq!(s.items[0].path, "C:\\Apps\\Code.exe");
    }

    #[test]
    fn tile_label_cleans_app_shortcuts() {
        assert!(is_app_shortcut("C:\\Tools\\Discord.lnk"));
        assert!(is_app_shortcut("C:\\Tools\\Code.exe"));
        assert!(!is_app_shortcut("C:\\Docs\\notes.pdf"));
        assert_eq!(tile_label("C:\\Tools\\Discord.lnk"), "Discord");
        assert_eq!(tile_label("C:\\Tools\\Code.exe"), "Code");
        assert_eq!(tile_label("C:\\Docs\\notes.pdf"), "notes.pdf");
    }
}
