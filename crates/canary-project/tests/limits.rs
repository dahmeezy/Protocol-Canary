//! Tests for scan bounds during project detection (issue #414).

use canary_project::{
    detect_details, DiagnosticCode, MAX_DIRECTORIES_VISITED, MAX_WORKSPACE_DEPTH,
    MAX_WORKSPACE_MEMBERS,
};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    pub path: PathBuf,
}

fn temp_dir(prefix: &str) -> TempDir {
    let mut path = std::env::temp_dir();
    let unique = format!(
        "{prefix}-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    path.push(unique);
    fs::create_dir_all(&path).unwrap();
    TempDir { path }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn max_depth_bound_stops_traversal_and_emits_warning() {
    let scratch = temp_dir("limits-depth");
    let root = &scratch.path;

    // Create a 10-level deep Cargo workspace pattern e.g. "a/b/c/d/e/f/g/h/i/j"
    let deep_pattern = "a/b/c/d/e/f/g/h/i/j";
    fs::write(
        root.join("Cargo.toml"),
        format!("[workspace]\nmembers = [\"{deep_pattern}\"]\n"),
    )
    .unwrap();

    let deep_dir = root.join("a/b/c/d/e/f/g/h/i/j");
    fs::create_dir_all(&deep_dir).unwrap();
    fs::write(
        deep_dir.join("Cargo.toml"),
        "[package]\nname = \"deep\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    let details = detect_details(root);
    let bound_hits: Vec<_> = details
        .diagnostics
        .iter()
        .filter(|d| d.code == DiagnosticCode::ScanBoundHit)
        .collect();

    assert_eq!(bound_hits.len(), 1);
    assert!(bound_hits[0]
        .detail
        .contains(&MAX_WORKSPACE_DEPTH.to_string()));
    assert!(details.workspace_members.is_empty());
}

#[test]
fn max_workspace_members_bound_stops_traversal_and_emits_warning() {
    let scratch = temp_dir("limits-members");
    let root = &scratch.path;

    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"m/*\"]\n",
    )
    .unwrap();

    for i in 0..MAX_WORKSPACE_MEMBERS + 5 {
        let m_dir = root.join(format!("m/member_{i:03}"));
        fs::create_dir_all(&m_dir).unwrap();
        fs::write(
            m_dir.join("Cargo.toml"),
            "[package]\nname = \"member\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
    }

    let details = detect_details(root);
    let bound_hits: Vec<_> = details
        .diagnostics
        .iter()
        .filter(|d| d.code == DiagnosticCode::ScanBoundHit)
        .collect();

    assert_eq!(bound_hits.len(), 1);
    assert!(bound_hits[0]
        .detail
        .contains(&MAX_WORKSPACE_MEMBERS.to_string()));
    assert_eq!(details.workspace_members.len(), MAX_WORKSPACE_MEMBERS);
}

#[test]
fn max_directories_visited_bound_stops_traversal_and_emits_warning() {
    let scratch = temp_dir("limits-dirs");
    let root = &scratch.path;

    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"dirs/*/*\"]\n",
    )
    .unwrap();

    let dirs_parent = root.join("dirs");
    fs::create_dir_all(&dirs_parent).unwrap();

    // Create more directory visits than MAX_DIRECTORIES_VISITED
    for i in 0..MAX_DIRECTORIES_VISITED + 10 {
        let d = dirs_parent.join(format!("d_{i:04}"));
        fs::create_dir_all(&d).unwrap();
    }

    let details = detect_details(root);
    let bound_hits: Vec<_> = details
        .diagnostics
        .iter()
        .filter(|d| d.code == DiagnosticCode::ScanBoundHit)
        .collect();

    assert_eq!(bound_hits.len(), 1);
    assert!(bound_hits[0].detail.contains("2000"));
}
