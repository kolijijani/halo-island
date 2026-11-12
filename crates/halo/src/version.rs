//! Which build this is, for the menu's About line and the tray tooltip:
//! the version, plus where the build came from (build.rs fills these in).

/// The name people see (Halo Island, kolijijani's fork).
/// Internal names stay: `halo.exe`, its folders, `HALO_*` (AGENTS.md, Names that stay).
pub const APP_NAME: &str = "Halo Island";
/// The author's GitHub name, shown under the version on the Settings version card.
pub const AUTHOR: &str = "kolijijani";
/// The taglines (Settings › About).
pub const TAGLINE: &str = "A living notch for Windows: by kolijijani";
pub const TAGLINE_PRO: &str = "Free is whole — Pro for power users.";

const REPO: &str = "https://github.com/kolijijani/halo-island";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// "release" (a release tag's CI build), "test" (any other CI build) or "local".
const KIND: &str = env!("HALO_BUILD_KIND");
/// The commit a CI build was made from, 7 characters; empty for a local build.
const COMMIT: &str = env!("HALO_BUILD_COMMIT");
/// The UTC day a CI build was made, "25 Oct 2026"; empty for a local build.
const DAY: &str = env!("HALO_BUILD_DAY");

/// "About Halo Island 0.2.3 (25 Oct 2026)"; a test build adds its commit.
pub fn about_label() -> String {
    label(VERSION, KIND, COMMIT, DAY)
}

/// The page the About line opens: this release's notes, a test build's commit.
pub fn notes_url() -> String {
    url(VERSION, KIND, COMMIT)
}

fn label(version: &str, kind: &str, commit: &str, day: &str) -> String {
    let detail = match kind {
        "release" => day.to_string(),
        "test" if day.is_empty() => format!("test build {commit}"),
        "test" => format!("test build {commit}, {day}"),
        _ => "local build".to_string(),
    };
    if detail.is_empty() { format!("About {APP_NAME} {version}") } else { format!("About {APP_NAME} {version} ({detail})") }
}

fn url(version: &str, kind: &str, commit: &str) -> String {
    match kind {
        "release" => format!("{REPO}/releases/tag/v{version}"),
        "test" if !commit.is_empty() => format!("{REPO}/commit/{commit}"),
        _ => format!("{REPO}/releases"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_about_line_says_which_build_this_is() {
        assert_eq!(label("0.2.3", "release", "c7d77c8", "25 Oct 2026"), "About Halo Island 0.2.3 (25 Oct 2026)");
        assert_eq!(label("0.2.3", "test", "c7d77c8", "25 Oct 2026"), "About Halo Island 0.2.3 (test build c7d77c8, 25 Oct 2026)");
        assert_eq!(label("0.2.3", "local", "", ""), "About Halo Island 0.2.3 (local build)");
        assert!(about_label().starts_with(&format!("About Halo Island {VERSION}")));
    }

    #[test]
    fn a_release_opens_its_notes_and_a_test_build_its_commit() {
        assert_eq!(url("0.2.3", "release", "c7d77c8"), "https://github.com/kolijijani/halo-island/releases/tag/v0.2.3");
        assert_eq!(url("0.2.3", "test", "c7d77c8"), "https://github.com/kolijijani/halo-island/commit/c7d77c8");
        assert_eq!(url("0.2.3", "local", ""), "https://github.com/kolijijani/halo-island/releases");
    }
}
