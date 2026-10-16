use miette::Diagnostic;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use thiserror::Error;

#[derive(Error, Debug, Diagnostic)]
#[non_exhaustive]
pub enum GitError {
    #[error("Git repository not found at {path}")]
    #[diagnostic(
        code(sizelint::git::repo_not_found),
        help("Make sure you're running sizelint from within a git repository")
    )]
    RepoNotFound { path: PathBuf },

    #[error("Git ref '{git_ref}' not found in {repo}")]
    #[diagnostic(
        code(sizelint::git::ref_not_found),
        help("Check that the branch or ref exists in the target repository")
    )]
    RefNotFound { git_ref: String, repo: PathBuf },

    #[error("Paths span multiple git repositories")]
    #[diagnostic(
        code(sizelint::git::multiple_repos),
        help("All paths must be in the same git repository when using --git")
    )]
    MultipleRepos { roots: Vec<PathBuf> },

    #[error("Git command failed: {command} (exit code {exit_code})\n{stderr}")]
    #[diagnostic(code(sizelint::git::command_failed))]
    CommandFailed {
        command: String,
        exit_code: i32,
        stderr: String,
    },

    #[error("Failed to execute git")]
    #[diagnostic(
        code(sizelint::git::exec),
        help("Check that git is installed and on your PATH")
    )]
    Exec(#[source] std::io::Error),
}

type Result<T> = std::result::Result<T, GitError>;

#[derive(Debug, Clone)]
pub struct HistoryBlob {
    pub path: String,
    pub size: u64,
    pub commit: String,
}

/// A revision range produced by [`GitRange::resolve`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRange(String);

impl GitRange {
    pub fn resolve(path: &Path, raw: &str) -> Result<Self> {
        GitRepo::discover(path)?.expand_git_range(raw)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for GitRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

struct BlobEntry {
    blob_hash: String,
    path: String,
    commit: String,
}

pub struct GitRepo {
    root: PathBuf,
}

fn distinct_blob_hashes(entries: &[BlobEntry]) -> Vec<&str> {
    let mut seen = HashSet::new();
    entries
        .iter()
        .map(|e| e.blob_hash.as_str())
        .filter(|hash| seen.insert(*hash))
        .collect()
}

impl GitRepo {
    pub fn discover<P: AsRef<Path>>(start_path: P) -> Result<Self> {
        let path = start_path.as_ref();

        let output = Command::new("git")
            .arg("rev-parse")
            .arg("--show-toplevel")
            .current_dir(path)
            .output()
            .map_err(GitError::Exec)?;

        if !output.status.success() {
            return Err(GitError::RepoNotFound {
                path: path.to_path_buf(),
            });
        }

        let root = String::from_utf8_lossy(&output.stdout).trim().to_string();

        Ok(GitRepo {
            root: PathBuf::from(root),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn get_staged_files(&self) -> Result<Vec<PathBuf>> {
        let command = "git diff --staged --name-only --diff-filter=ACMRT";
        let output = self.exec(&["diff", "--staged", "--name-only", "--diff-filter=ACMRT"])?;

        if !output.status.success() {
            return Err(self.command_failed(command, &output));
        }

        Ok(self.parse_paths(&output.stdout))
    }

    pub fn get_working_tree_files(&self) -> Result<Vec<PathBuf>> {
        let command = "git diff --name-only --diff-filter=ACMRT";
        let output = self.exec(&["diff", "--name-only", "--diff-filter=ACMRT"])?;

        if !output.status.success() {
            return Err(self.command_failed(command, &output));
        }

        Ok(self.parse_paths(&output.stdout))
    }

    pub fn count_commits(&self, range: &GitRange) -> Result<usize> {
        let command = format!("git rev-list --count {range}");
        let output = self.exec(&["rev-list", "--count", range.as_str()])?;

        if !output.status.success() {
            return Err(self.command_failed(&command, &output));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .trim()
            .parse::<usize>()
            .map_err(|_| GitError::CommandFailed {
                command,
                exit_code: -1,
                stderr: format!("Could not parse commit count from: {stdout}"),
            })
    }

    /// Expand a git range string for use with `git diff`.
    ///
    /// Bare refs (no `..` or `...`) are expanded to `<merge-base>..HEAD`
    /// so that `--git main` means "files changed since diverging from main".
    /// Two-dot and three-dot ranges are passed through unchanged.
    pub fn expand_git_range(&self, range: &str) -> Result<GitRange> {
        if range.contains("..") {
            return Ok(GitRange(range.to_string()));
        }

        // Verify the ref exists before trying merge-base
        let verify = Command::new("git")
            .args(["rev-parse", "--verify", &format!("{range}^{{commit}}")])
            .current_dir(&self.root)
            .output()
            .map_err(GitError::Exec)?;

        if !verify.status.success() {
            return Err(GitError::RefNotFound {
                git_ref: range.to_string(),
                repo: self.root.clone(),
            });
        }

        let command = format!("git merge-base {range} HEAD");
        let output = Command::new("git")
            .args(["merge-base", range, "HEAD"])
            .current_dir(&self.root)
            .output()
            .map_err(GitError::Exec)?;

        if !output.status.success() {
            return Err(self.command_failed(&command, &output));
        }

        let merge_base = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(GitRange(format!("{merge_base}..HEAD")))
    }

    pub fn get_diff_files(&self, range: &GitRange) -> Result<Vec<PathBuf>> {
        let command = format!("git diff --name-only --diff-filter=ACMRT {range}");
        let output = self.exec(&["diff", "--name-only", "--diff-filter=ACMRT", range.as_str()])?;

        if !output.status.success() {
            return Err(self.command_failed(&command, &output));
        }

        Ok(self.parse_paths(&output.stdout))
    }

    fn rev_list_commits(&self, range: &GitRange) -> Result<Vec<String>> {
        let command = format!("git rev-list --no-merges {range}");
        let output = self.exec(&["rev-list", "--no-merges", range.as_str()])?;

        if !output.status.success() {
            return Err(self.command_failed(&command, &output));
        }

        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect())
    }

    /// Spawn a single `git diff-tree -r --stdin` process fed with commit hashes,
    /// parse the raw diff output into `BlobEntry` values.
    /// Skips submodule entries (mode 160000).
    fn diff_tree_entries(&self, commits: &[String]) -> Result<Vec<BlobEntry>> {
        let mut child = Command::new("git")
            .args([
                "diff-tree",
                "-r",
                "--root",
                "--stdin",
                "--diff-filter=ACMRT",
            ])
            .current_dir(&self.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(GitError::Exec)?;

        let stdin = child.stdin.take().unwrap();
        let commits_owned: Vec<String> = commits.to_vec();
        let writer_thread = std::thread::spawn(move || -> std::io::Result<()> {
            let mut writer = std::io::BufWriter::new(stdin);
            for hash in &commits_owned {
                writeln!(writer, "{hash}")?;
            }
            Ok(())
        });

        let output = child.wait_with_output().map_err(GitError::Exec)?;
        writer_thread
            .join()
            .expect("stdin writer thread panicked")
            .map_err(GitError::Exec)?;

        if !output.status.success() {
            return Err(self.command_failed("git diff-tree -r --root --stdin", &output));
        }

        let mut entries = Vec::new();
        let mut current_commit = String::new();

        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if matches!(line.len(), 40 | 64) && line.bytes().all(|b| b.is_ascii_hexdigit()) {
                current_commit = line[..12].to_string();
                continue;
            }

            // diff-tree raw lines: :<old_mode> <new_mode> <old_hash> <new_hash> <status>\t<path>
            if !line.starts_with(':') {
                continue;
            }

            let Some((meta, path)) = line.split_once('\t') else {
                continue;
            };

            let parts: Vec<&str> = meta.split_whitespace().collect();
            if parts.len() < 5 {
                continue;
            }

            // parts[1] is the new mode — skip submodules
            if parts[1] == "160000" {
                continue;
            }

            entries.push(BlobEntry {
                blob_hash: parts[3].to_string(),
                path: self.root.join(path).to_string_lossy().to_string(),
                commit: current_commit.clone(),
            });
        }

        Ok(entries)
    }

    /// Skips merges and submodule entries (mode 160000).
    /// Parallelizes tree-diffing across available CPU cores.
    fn collect_history_entries(&self, range: &GitRange) -> Result<Vec<BlobEntry>> {
        let commits = self.rev_list_commits(range)?;

        if commits.is_empty() {
            return Ok(Vec::new());
        }

        let num_cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let chunk_size = commits.len().div_ceil(num_cpus).max(1);

        let chunks: Vec<&[String]> = commits.chunks(chunk_size).collect();
        let results: Result<Vec<Vec<BlobEntry>>> = chunks
            .into_par_iter()
            .map(|chunk| self.diff_tree_entries(chunk))
            .collect();

        Ok(results?.into_iter().flatten().collect())
    }

    /// Resolve blob sizes in batch via a single `git cat-file --batch-check`
    /// process instead of spawning one process per blob.
    fn batch_blob_sizes(&self, entries: &[BlobEntry]) -> Result<Vec<u64>> {
        let hashes = distinct_blob_hashes(entries);
        let sizes: HashMap<&str, u64> = hashes
            .iter()
            .copied()
            .zip(self.cat_file_sizes(&hashes)?)
            .collect();
        Ok(entries
            .iter()
            .map(|e| sizes[e.blob_hash.as_str()])
            .collect())
    }

    /// Look up blob sizes via a single `git cat-file --batch-check` process.
    /// Returns one size per hash, in input order.
    fn cat_file_sizes(&self, hashes: &[&str]) -> Result<Vec<u64>> {
        const COMMAND: &str = "git cat-file --batch-check";
        let mut child = Command::new("git")
            .args(["cat-file", "--batch-check"])
            .current_dir(&self.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(GitError::Exec)?;

        // Write hashes on a separate thread to avoid deadlock: with many
        // blobs the stdout pipe buffer fills while we're still writing to
        // stdin, blocking both sides.
        let stdin = child.stdin.take().unwrap();
        let input: Vec<String> = hashes.iter().map(|h| h.to_string()).collect();
        let writer_thread = std::thread::spawn(move || -> std::io::Result<()> {
            let mut writer = std::io::BufWriter::new(stdin);
            for hash in &input {
                writeln!(writer, "{hash}")?;
            }
            Ok(())
        });

        let output = child.wait_with_output().map_err(GitError::Exec)?;
        writer_thread
            .join()
            .expect("stdin writer thread panicked")
            .map_err(GitError::Exec)?;

        if !output.status.success() {
            return Err(self.command_failed(COMMAND, &output));
        }

        // Each output line: "<hash> <type> <size>" or "<hash> missing"
        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = stdout.lines().collect();
        if lines.len() != hashes.len() {
            return Err(GitError::CommandFailed {
                command: COMMAND.to_string(),
                exit_code: -1,
                stderr: format!(
                    "Expected {} output lines, got {}",
                    hashes.len(),
                    lines.len()
                ),
            });
        }

        lines
            .into_iter()
            .zip(hashes)
            .map(|(line, hash)| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                let stderr = match parts.as_slice() {
                    [_, _, size_str] => match size_str.parse::<u64>() {
                        Ok(size) => return Ok(size),
                        Err(_) => format!("Could not parse blob size from: {line}"),
                    },
                    _ => format!("Unexpected output: {line}"),
                };
                Err(GitError::CommandFailed {
                    command: format!("{COMMAND} ({hash})"),
                    exit_code: -1,
                    stderr,
                })
            })
            .collect()
    }

    /// Walk every commit in the range and collect all added/modified blobs.
    /// Uses `git rev-list` + parallel `git diff-tree --stdin` workers +
    /// single `git cat-file --batch-check`.
    pub fn walk_history_blobs(&self, range: &GitRange) -> Result<Vec<HistoryBlob>> {
        let entries = self.collect_history_entries(range)?;

        if entries.is_empty() {
            return Ok(vec![]);
        }

        let sizes = self.batch_blob_sizes(&entries)?;

        Ok(entries
            .into_iter()
            .zip(sizes)
            .map(|(entry, size)| HistoryBlob {
                path: entry.path,
                size,
                commit: entry.commit,
            })
            .collect())
    }

    fn exec(&self, args: &[&str]) -> Result<std::process::Output> {
        Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()
            .map_err(GitError::Exec)
    }

    fn command_failed(&self, command: &str, output: &std::process::Output) -> GitError {
        GitError::CommandFailed {
            command: command.to_string(),
            exit_code: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        }
    }

    fn parse_paths(&self, stdout: &[u8]) -> Vec<PathBuf> {
        String::from_utf8_lossy(stdout)
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| self.root.join(line))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_test_repo() -> (tempfile::TempDir, GitRepo) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(root)
            .output()
            .unwrap();

        // Initial commit on default branch
        fs::write(root.join("init.txt"), "init").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(root)
            .output()
            .unwrap();

        let repo = GitRepo::discover(root).unwrap();
        (tmp, repo)
    }

    fn entry(blob_hash: &str, path: &str) -> BlobEntry {
        BlobEntry {
            blob_hash: blob_hash.to_string(),
            path: path.to_string(),
            commit: "c0ffee".to_string(),
        }
    }

    #[test]
    fn test_distinct_blob_hashes_lists_each_hash_once_in_first_seen_order() {
        let entries = [
            entry("aaaa", "asset.bin"),
            entry("bbbb", "other.bin"),
            entry("aaaa", "moved.bin"),
            entry("cccc", "third.bin"),
            entry("bbbb", "other-copy.bin"),
        ];

        assert_eq!(distinct_blob_hashes(&entries), vec!["aaaa", "bbbb", "cccc"]);
    }

    #[test]
    #[ignore = "requires git binary"]
    fn test_batch_blob_sizes_maps_shared_hash_onto_every_entry() {
        let (_tmp, repo) = setup_test_repo();
        let root = repo.root().to_path_buf();
        fs::write(root.join("big.bin"), "x".repeat(700)).unwrap();
        fs::write(root.join("small.bin"), "x".repeat(30)).unwrap();
        let hash = |name: &str| {
            let output = Command::new("git")
                .args(["hash-object", "-w", name])
                .current_dir(&root)
                .output()
                .unwrap();
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        };
        let (big, small) = (hash("big.bin"), hash("small.bin"));

        let entries = [
            entry(&big, "big.bin"),
            entry(&small, "small.bin"),
            entry(&big, "moved.bin"),
        ];

        assert_eq!(repo.batch_blob_sizes(&entries).unwrap(), vec![700, 30, 700]);
    }

    #[test]
    #[ignore = "requires git binary"]
    fn test_expand_git_range_bare_ref() {
        let (_tmp, repo) = setup_test_repo();
        let root = repo.root().to_path_buf();

        // Create a branch, add a commit on it
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(&root)
            .output()
            .unwrap();
        fs::write(root.join("feature.txt"), "feature").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "feature"])
            .current_dir(&root)
            .output()
            .unwrap();

        // Bare ref should expand to merge-base..HEAD
        let expanded = repo.expand_git_range("master").unwrap_or_else(|_| {
            // Try "main" if "master" doesn't exist
            repo.expand_git_range("HEAD~1").unwrap()
        });
        assert!(expanded.as_str().contains("..HEAD"));
        assert!(!expanded.as_str().contains("..."));
    }

    #[test]
    #[ignore = "requires git binary"]
    fn test_expand_git_range_two_dot() {
        let (_tmp, repo) = setup_test_repo();
        let expanded = repo.expand_git_range("HEAD~1..HEAD").unwrap();
        assert_eq!(expanded.as_str(), "HEAD~1..HEAD");
    }

    #[test]
    #[ignore = "requires git binary"]
    fn test_expand_git_range_three_dot() {
        let (_tmp, repo) = setup_test_repo();
        let expanded = repo.expand_git_range("HEAD~1...HEAD").unwrap();
        assert_eq!(expanded.as_str(), "HEAD~1...HEAD");
    }
}
