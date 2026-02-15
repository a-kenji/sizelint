use sizelint::discovery::FileDiscovery;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

struct TestGitRepo {
    _tmp: TempDir,
    root: PathBuf,
}

impl TestGitRepo {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();

        Self::git(&root, &["init"]);
        Self::git(&root, &["config", "user.email", "test@test.com"]);
        Self::git(&root, &["config", "user.name", "Test"]);

        // Initial commit on default branch
        std::fs::write(root.join("init.txt"), "init").unwrap();
        Self::git(&root, &["add", "."]);
        Self::git(&root, &["commit", "-m", "init"]);

        TestGitRepo { _tmp: tmp, root }
    }

    fn git(cwd: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn default_branch(&self) -> String {
        Self::git(&self.root, &["branch", "--show-current"])
    }

    fn write_file(&self, name: &str, content: &str) {
        let path = self.root.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }
}

fn file_names(files: &[PathBuf]) -> Vec<String> {
    let mut names: Vec<String> = files
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .collect();
    names.sort();
    names
}

#[test]
fn test_git_diff_discovers_changed_files_via_merge_base() {
    let repo = TestGitRepo::new();
    let base = repo.default_branch();

    // Create a feature branch with new files
    TestGitRepo::git(&repo.root, &["checkout", "-b", "feature"]);
    repo.write_file("feature.rs", "fn feature() {}");
    repo.write_file("src/helper.rs", "fn helper() {}");
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "add feature files"]);

    let discovery = FileDiscovery::new(&repo.root, &[]).unwrap();
    let files = discovery.discover_git_diff_files(&base).unwrap();

    let names = file_names(&files);
    assert!(names.contains(&"feature.rs".to_string()));
    assert!(names.contains(&"helper.rs".to_string()));
    assert!(!names.contains(&"init.txt".to_string()));
}

#[test]
fn test_git_diff_two_dot_range() {
    let repo = TestGitRepo::new();
    let base = repo.default_branch();

    TestGitRepo::git(&repo.root, &["checkout", "-b", "feature"]);
    repo.write_file("new.rs", "fn new() {}");
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "new file"]);

    let range = format!("{base}..feature");
    let discovery = FileDiscovery::new(&repo.root, &[]).unwrap();
    let files = discovery.discover_git_diff_files(&range).unwrap();

    let names = file_names(&files);
    assert_eq!(names, vec!["new.rs"]);
}

#[test]
fn test_git_diff_empty_range_returns_zero_files() {
    let repo = TestGitRepo::new();

    // Diff HEAD against itself: no changes
    let discovery = FileDiscovery::new(&repo.root, &[]).unwrap();
    let files = discovery.discover_git_diff_files("HEAD..HEAD").unwrap();

    assert!(files.is_empty());
}

#[test]
fn test_git_diff_config_excludes_filter_results() {
    let repo = TestGitRepo::new();
    let base = repo.default_branch();

    TestGitRepo::git(&repo.root, &["checkout", "-b", "feature"]);
    repo.write_file("src/main.rs", "fn main() {}");
    repo.write_file("build/output.bin", "binary data");
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "mixed files"]);

    let discovery = FileDiscovery::new(&repo.root, &["*.bin".to_string()]).unwrap();
    let files = discovery.discover_git_diff_files(&base).unwrap();

    let names = file_names(&files);
    assert!(names.contains(&"main.rs".to_string()));
    assert!(!names.contains(&"output.bin".to_string()));
}

#[test]
fn test_git_diff_error_when_not_in_git_repo() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    // No git init — not a repo
    let discovery = FileDiscovery::new(root, &[]).unwrap();
    let result = discovery.discover_git_diff_files("main");

    assert!(result.is_err());
}

// History blob tests

fn write_large_file(repo: &TestGitRepo, name: &str, size: usize) {
    let content = "x".repeat(size);
    repo.write_file(name, &content);
}

#[test]
fn test_history_blob_large_file_added_then_deleted() {
    let repo = TestGitRepo::new();
    let base = repo.default_branch();

    TestGitRepo::git(&repo.root, &["checkout", "-b", "feature"]);

    // Add a large file
    write_large_file(&repo, "data/dump.sql", 1024);
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "add dump"]);

    // Delete it
    std::fs::remove_file(repo.root.join("data/dump.sql")).unwrap();
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "remove dump"]);

    let discovery = FileDiscovery::new(&repo.root, &[]).unwrap();
    let blobs = discovery.discover_history_blobs(&base).unwrap();

    // The add commit introduced a 1024-byte blob
    let dump_blobs: Vec<_> = blobs
        .iter()
        .filter(|b| b.path.ends_with("data/dump.sql"))
        .collect();
    assert_eq!(dump_blobs.len(), 1);
    assert_eq!(dump_blobs[0].size, 1024);
    assert!(!dump_blobs[0].commit.is_empty());
}

#[test]
fn test_history_blob_file_still_at_head() {
    let repo = TestGitRepo::new();
    let base = repo.default_branch();

    TestGitRepo::git(&repo.root, &["checkout", "-b", "feature"]);

    // Add file and keep it
    repo.write_file("src/keep.rs", "fn keep() {}");
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "add keep"]);

    let discovery = FileDiscovery::new(&repo.root, &[]).unwrap();
    let blobs = discovery.discover_history_blobs(&base).unwrap();

    // Per-commit walk reports all blobs regardless of HEAD state
    let keep_blobs: Vec<_> = blobs
        .iter()
        .filter(|b| b.path.ends_with("src/keep.rs"))
        .collect();
    assert_eq!(keep_blobs.len(), 1);
}

#[test]
fn test_history_blob_small_file_no_violation() {
    use sizelint::config::RuleDefinition;
    use sizelint::rules::ConfigurableRule;
    use sizelint::rules::RuleEngine;

    let repo = TestGitRepo::new();
    let base = repo.default_branch();

    TestGitRepo::git(&repo.root, &["checkout", "-b", "feature"]);

    // Add a small file then delete it
    repo.write_file("tmp/notes.txt", "small content");
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "add notes"]);

    std::fs::remove_file(repo.root.join("tmp/notes.txt")).unwrap();
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "remove notes"]);

    let discovery = FileDiscovery::new(&repo.root, &[]).unwrap();
    let blobs = discovery.discover_history_blobs(&base).unwrap();

    let notes_blobs: Vec<_> = blobs
        .iter()
        .filter(|b| b.path.ends_with("tmp/notes.txt"))
        .collect();
    assert_eq!(notes_blobs.len(), 1);

    // Check against rules with a high threshold — no violation expected
    let mut engine = RuleEngine::new();
    let rule = ConfigurableRule::new(
        "default".to_string(),
        RuleDefinition {
            enabled: true,
            description: "test".to_string(),
            priority: 100,
            max_size: Some("1MB".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    engine.add_rule(rule);

    let violations = engine.check_history_blobs(&blobs).unwrap();
    assert!(violations.is_empty());
}

#[test]
fn test_history_blob_config_excludes_filter() {
    let repo = TestGitRepo::new();
    let base = repo.default_branch();

    TestGitRepo::git(&repo.root, &["checkout", "-b", "feature"]);

    write_large_file(&repo, "src/code.rs", 512);
    write_large_file(&repo, "vendor/big.dat", 2048);
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "add files"]);

    // Delete both
    std::fs::remove_file(repo.root.join("src/code.rs")).unwrap();
    std::fs::remove_file(repo.root.join("vendor/big.dat")).unwrap();
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "remove files"]);

    let discovery = FileDiscovery::new(&repo.root, &["*.dat".to_string()]).unwrap();
    let blobs = discovery.discover_history_blobs(&base).unwrap();

    // Only code.rs should appear, not big.dat (excluded)
    let has_code = blobs.iter().any(|b| b.path.ends_with("src/code.rs"));
    let has_dat = blobs.iter().any(|b| b.path.ends_with("vendor/big.dat"));
    assert!(has_code);
    assert!(!has_dat);
}

#[test]
fn test_history_blob_add_delete_readd_delete() {
    let repo = TestGitRepo::new();
    let base = repo.default_branch();

    TestGitRepo::git(&repo.root, &["checkout", "-b", "feature"]);

    // Add file, commit
    write_large_file(&repo, "data.csv", 100);
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "add v1"]);

    // Delete, commit
    std::fs::remove_file(repo.root.join("data.csv")).unwrap();
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "delete v1"]);

    // Re-add bigger, commit
    write_large_file(&repo, "data.csv", 500);
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "add v2"]);

    // Delete again, commit
    std::fs::remove_file(repo.root.join("data.csv")).unwrap();
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "delete v2"]);

    let discovery = FileDiscovery::new(&repo.root, &[]).unwrap();
    let blobs = discovery.discover_history_blobs(&base).unwrap();

    // Per-commit walk finds both add commits
    let csv_blobs: Vec<_> = blobs
        .iter()
        .filter(|b| b.path.ends_with("data.csv"))
        .collect();
    assert_eq!(csv_blobs.len(), 2);
}

#[test]
fn test_history_blob_temporarily_too_large_then_shrunk() {
    use sizelint::config::RuleDefinition;
    use sizelint::rules::ConfigurableRule;
    use sizelint::rules::RuleEngine;

    let repo = TestGitRepo::new();
    let base = repo.default_branch();

    TestGitRepo::git(&repo.root, &["checkout", "-b", "feature"]);

    // Add a large file (over 500B threshold)
    write_large_file(&repo, "data.bin", 1024);
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "add large"]);

    // Shrink it below threshold
    write_large_file(&repo, "data.bin", 100);
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "shrink"]);

    let discovery = FileDiscovery::new(&repo.root, &[]).unwrap();
    let blobs = discovery.discover_history_blobs(&base).unwrap();

    // Both the large and small versions appear
    let data_blobs: Vec<_> = blobs
        .iter()
        .filter(|b| b.path.ends_with("data.bin"))
        .collect();
    assert_eq!(data_blobs.len(), 2);

    let mut engine = RuleEngine::new();
    let rule = ConfigurableRule::new(
        "default".to_string(),
        RuleDefinition {
            enabled: true,
            description: "test".to_string(),
            priority: 100,
            max_size: Some("500B".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    engine.add_rule(rule);

    let violations = engine.check_history_blobs(&blobs).unwrap();
    // The 1024-byte intermediate blob should trigger a violation
    assert_eq!(violations.len(), 1);
    assert!(violations[0].path.ends_with("data.bin"));
}

#[test]
fn test_history_blob_grew_then_deleted() {
    use sizelint::config::RuleDefinition;
    use sizelint::rules::ConfigurableRule;
    use sizelint::rules::RuleEngine;

    let repo = TestGitRepo::new();
    let base = repo.default_branch();

    TestGitRepo::git(&repo.root, &["checkout", "-b", "feature"]);

    // Add a large file
    write_large_file(&repo, "big.log", 2048);
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "add big log"]);

    // Delete it
    std::fs::remove_file(repo.root.join("big.log")).unwrap();
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "remove big log"]);

    let discovery = FileDiscovery::new(&repo.root, &[]).unwrap();
    let blobs = discovery.discover_history_blobs(&base).unwrap();

    let mut engine = RuleEngine::new();
    let rule = ConfigurableRule::new(
        "default".to_string(),
        RuleDefinition {
            enabled: true,
            description: "test".to_string(),
            priority: 100,
            max_size: Some("1KB".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    engine.add_rule(rule);

    let violations = engine.check_history_blobs(&blobs).unwrap();
    assert_eq!(violations.len(), 1);
    assert!(violations[0].path.ends_with("big.log"));
}

#[test]
fn test_history_blob_dedup_keeps_largest() {
    use sizelint::config::RuleDefinition;
    use sizelint::rules::ConfigurableRule;
    use sizelint::rules::RuleEngine;

    let repo = TestGitRepo::new();
    let base = repo.default_branch();

    TestGitRepo::git(&repo.root, &["checkout", "-b", "feature"]);

    // First commit: 600B (over 500B threshold)
    write_large_file(&repo, "grow.dat", 600);
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "add 600"]);

    // Second commit: grow to 900B
    write_large_file(&repo, "grow.dat", 900);
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "grow to 900"]);

    // Third commit: grow to 1200B
    write_large_file(&repo, "grow.dat", 1200);
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "grow to 1200"]);

    let discovery = FileDiscovery::new(&repo.root, &[]).unwrap();
    let blobs = discovery.discover_history_blobs(&base).unwrap();

    let grow_blobs: Vec<_> = blobs
        .iter()
        .filter(|b| b.path.ends_with("grow.dat"))
        .collect();
    assert_eq!(grow_blobs.len(), 3);

    let mut engine = RuleEngine::new();
    let rule = ConfigurableRule::new(
        "default".to_string(),
        RuleDefinition {
            enabled: true,
            description: "test".to_string(),
            priority: 100,
            max_size: Some("500B".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    engine.add_rule(rule);

    let violations = engine.check_history_blobs(&blobs).unwrap();
    // Dedup keeps only the largest violation per path
    assert_eq!(violations.len(), 1);
    assert!(violations[0].path.ends_with("grow.dat"));
    assert_eq!(violations[0].sort_key, 1200);
}

#[test]
fn test_cross_phase_dedup_keeps_larger_history_blob() {
    use sizelint::config::RuleDefinition;
    use sizelint::rules::ConfigurableRule;
    use sizelint::rules::RuleEngine;

    let repo = TestGitRepo::new();
    let base = repo.default_branch();

    TestGitRepo::git(&repo.root, &["checkout", "-b", "feature"]);

    // Commit 1: large file (over 500B threshold)
    write_large_file(&repo, "bloat.bin", 2000);
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "add large"]);

    // Commit 2: shrink but still over threshold
    write_large_file(&repo, "bloat.bin", 600);
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "shrink"]);

    let discovery = FileDiscovery::new(&repo.root, &[]).unwrap();
    let live_files = discovery.discover_git_diff_files(&base).unwrap();

    let mut engine = RuleEngine::new();
    let rule = ConfigurableRule::new(
        "default".to_string(),
        RuleDefinition {
            enabled: true,
            description: "test".to_string(),
            priority: 100,
            max_size: Some("500B".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    engine.add_rule(rule);

    // Phase 1: check live files (600B at HEAD)
    let mut violations = engine.check_files(&live_files).unwrap();
    let phase1_count = violations.len();
    assert_eq!(phase1_count, 1);

    // Phase 2: history walk finds the 2000B intermediate blob
    let blobs = discovery.discover_history_blobs(&base).unwrap();
    let blob_violations = engine.check_history_blobs(&blobs).unwrap();
    violations.extend(blob_violations);

    // Before dedup: Phase 1 (600B) + Phase 2 (2000B) = 2 violations for same path
    assert!(violations.len() > 1);

    // Cross-phase dedup: keep largest per path
    let mut best: std::collections::HashMap<std::path::PathBuf, sizelint::rules::Violation> =
        std::collections::HashMap::new();
    for v in violations {
        best.entry(v.path.clone())
            .and_modify(|existing| {
                if v.sort_key > existing.sort_key {
                    *existing = v.clone();
                }
            })
            .or_insert(v);
    }
    let violations: Vec<_> = best.into_values().collect();

    assert_eq!(violations.len(), 1);
    // The 2000B history blob wins over the 600B HEAD version
    assert_eq!(violations[0].sort_key, 2000);
}

#[test]
fn test_cross_phase_dedup_no_duplicate_when_unchanged() {
    use sizelint::config::RuleDefinition;
    use sizelint::rules::ConfigurableRule;
    use sizelint::rules::RuleEngine;

    let repo = TestGitRepo::new();
    let base = repo.default_branch();

    TestGitRepo::git(&repo.root, &["checkout", "-b", "feature"]);

    // Single commit: file stays at HEAD
    write_large_file(&repo, "big.dat", 800);
    TestGitRepo::git(&repo.root, &["add", "."]);
    TestGitRepo::git(&repo.root, &["commit", "-m", "add big"]);

    let discovery = FileDiscovery::new(&repo.root, &[]).unwrap();
    let live_files = discovery.discover_git_diff_files(&base).unwrap();

    let mut engine = RuleEngine::new();
    let rule = ConfigurableRule::new(
        "default".to_string(),
        RuleDefinition {
            enabled: true,
            description: "test".to_string(),
            priority: 100,
            max_size: Some("500B".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    engine.add_rule(rule);

    // Phase 1
    let mut violations = engine.check_files(&live_files).unwrap();
    assert_eq!(violations.len(), 1);

    // Phase 2
    let blobs = discovery.discover_history_blobs(&base).unwrap();
    let blob_violations = engine.check_history_blobs(&blobs).unwrap();
    violations.extend(blob_violations);

    // Both phases fire for the same file at the same size
    assert_eq!(violations.len(), 2);

    // After dedup: only 1 violation
    let mut best: std::collections::HashMap<std::path::PathBuf, sizelint::rules::Violation> =
        std::collections::HashMap::new();
    for v in violations {
        best.entry(v.path.clone())
            .and_modify(|existing| {
                if v.sort_key > existing.sort_key {
                    *existing = v.clone();
                }
            })
            .or_insert(v);
    }
    let violations: Vec<_> = best.into_values().collect();

    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].sort_key, 800);
}
