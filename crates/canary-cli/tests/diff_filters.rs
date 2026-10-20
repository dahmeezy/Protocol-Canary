//! Tests for `stellar-canary diff` filtering flags (issue #435).

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use canary_core::{
    CompatibilityResult, GitContext, PolicyDecision, ProjectType, ProtocolVersion, ResultSource,
    Status, Surface,
};
use canary_report::{JsonReporter, ProjectSummary, ReportInput};

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

fn mock_report(results: Vec<CompatibilityResult>) -> ReportInput {
    ReportInput {
        tool_version: "0.2.0".into(),
        target_protocol: ProtocolVersion(28),
        project: ProjectSummary {
            name: "test-proj".into(),
            project_type: ProjectType::Soroban,
        },
        network: None,
        results,
        skipped: Vec::new(),
        verification: Vec::new(),
        decision: PolicyDecision::Pass,
        git: GitContext::default(),
        verbose: false,
        run: None,
    }
}

fn mock_res(id: &str, surface: Surface, status: Status) -> CompatibilityResult {
    CompatibilityResult {
        test_id: id.to_string(),
        protocol: ProtocolVersion(28),
        surface,
        status,
        summary: String::new(),
        details: None,
        duration_ms: 10,
        fixture_id: Some(id.to_string()),
        source: ResultSource::Live,
    }
}

fn binary_path() -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join(format!("stellar-canary{}", std::env::consts::EXE_SUFFIX))
}

#[test]
fn diff_filter_by_surface_and_status() {
    let scratch = temp_dir("diff-filters");
    let baseline_path = scratch.path.join("baseline.json");
    let current_path = scratch.path.join("current.json");

    let baseline_report = mock_report(vec![
        mock_res("rpc-1", Surface::Rpc, Status::Pass),
        mock_res("xdr-1", Surface::Xdr, Status::Pass),
        mock_res("soroban-1", Surface::Soroban, Status::Pass),
    ]);

    let current_report = mock_report(vec![
        mock_res("rpc-1", Surface::Rpc, Status::Fail), // Regressed
        mock_res("xdr-1", Surface::Xdr, Status::Pass), // Unchanged
        mock_res("soroban-1", Surface::Soroban, Status::Fail), // Regressed
    ]);

    fs::write(&baseline_path, JsonReporter::render(&baseline_report)).unwrap();
    fs::write(&current_path, JsonReporter::render(&current_report)).unwrap();

    let exe = binary_path();

    // Filter --surface rpc with --format json
    let output = std::process::Command::new(&exe)
        .arg("diff")
        .arg(&baseline_path)
        .arg(&current_path)
        .arg("--surface")
        .arg("rpc")
        .arg("--format")
        .arg("json")
        .output()
        .expect("failed to run binary");

    assert_eq!(output.status.code(), Some(1)); // Verdict is regression
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let entries = json["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["id"], "rpc-1");
    assert_eq!(entries[0]["surface"], "rpc");

    // Filter --status regressed with default terminal format
    let output = std::process::Command::new(&exe)
        .arg("diff")
        .arg(&baseline_path)
        .arg(&current_path)
        .arg("--status")
        .arg("regressed")
        .output()
        .expect("failed to run binary");

    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("Report Comparison:"));
    assert!(text.contains("rpc-1"));
    assert!(text.contains("soroban-1"));
    assert!(!text.contains("xdr-1"));
}

#[test]
fn diff_invalid_status_flag_rejected() {
    let scratch = temp_dir("diff-invalid-status");
    let baseline_path = scratch.path.join("b.json");
    let current_path = scratch.path.join("c.json");

    fs::write(&baseline_path, JsonReporter::render(&mock_report(vec![]))).unwrap();
    fs::write(&current_path, JsonReporter::render(&mock_report(vec![]))).unwrap();

    let exe = binary_path();
    let output = std::process::Command::new(&exe)
        .arg("diff")
        .arg(&baseline_path)
        .arg(&current_path)
        .arg("--status")
        .arg("invalid-status-name")
        .output()
        .expect("failed to run binary");

    assert_eq!(output.status.code(), Some(2)); // clap argument error
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid value 'invalid-status-name'"));
}
