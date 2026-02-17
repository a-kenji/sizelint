use miette::Diagnostic;
use rayon::prelude::*;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use thiserror::Error;

#[derive(Error, Debug, Diagnostic)]
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

struct BlobEntry {
    blob_hash: String,
    path: String,
    commit: String,
}

pub struct GitRepo {
    root: PathBuf,
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

    /// Count the number of commits in a range.
    pub fn count_commits_in_range(&self, range: &str) -> Result<usize> {
        let expanded = self.expand_git_range(range)?;
        let output = Command::new("git")
            .args(["rev-list", "--count"])
            .arg(&expanded)
            .current_dir(&self.root)
            .output()
            .map_err(GitError::Exec)?;

        if !output.status.success() {
            return Ok(0);
        }

        Ok(String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<usize>()
            .unwrap_or(0))
    }

    /// Expand a git range string for use with `git diff`.
    ///
    /// Bare refs (no `..` or `...`) are expanded to `<merge-base>..HEAD`
    /// so that `--git main` means "files changed since diverging from main".
    /// Two-dot and three-dot ranges are passed through unchanged.
    pub fn expand_git_range(&self, range: &str) -> Result<String> {
        if range.contains("...") || range.contains("..") {
            return Ok(range.to_string());
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
        Ok(format!("{merge_base}..HEAD"))
    }

    pub fn get_diff_files(&self, range: &str) -> Result<Vec<PathBuf>> {
        let expanded = self.expand_git_range(range)?;
        let command = format!("git diff --name-only --diff-filter=ACMRT {expanded}");

        let output = Command::new("git")
            .arg("diff")
            .arg("--name-only")
            .arg("--diff-filter=ACMRT")
            .arg(&expanded)
            .current_dir(&self.root)
            .output()
            .map_err(GitError::Exec)?;

        if !output.status.success() {
            return Err(self.command_failed(&command, &output));
        }

        Ok(self.parse_paths(&output.stdout))
    }

    fn rev_list_commits(&self, expanded_range: &str) -> Result<Vec<String>> {
        let command = format!("git rev-list --no-merges {expanded_range}");
        let output = Command::new("git")
            .args(["rev-list", "--no-merges"])
            .arg(expanded_range)
            .current_dir(&self.root)
            .output()
            .map_err(GitError::Exec)?;

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
            if line.len() == 40 && line.bytes().all(|b| b.is_ascii_hexdigit()) {
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
    fn collect_history_entries(&self, range: &str) -> Result<Vec<BlobEntry>> {
        let expanded = self.expand_git_range(range)?;
        let commits = self.rev_list_commits(&expanded)?;

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
        let hashes: Vec<String> = entries.iter().map(|e| e.blob_hash.clone()).collect();
        let writer_thread = std::thread::spawn(move || -> std::io::Result<()> {
            let mut writer = std::io::BufWriter::new(stdin);
            for hash in &hashes {
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
            return Err(self.command_failed("git cat-file --batch-check", &output));
        }

        // Each output line: "<hash> <type> <size>" or "<hash> missing"
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .lines()
            .zip(entries)
            .map(|(line, entry)| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                match parts.as_slice() {
                    [_, _, size_str] => {
                        size_str
                            .parse::<u64>()
                            .map_err(|_| GitError::CommandFailed {
                                command: format!(
                                    "git cat-file --batch-check ({})",
                                    entry.blob_hash
                                ),
                                exit_code: -1,
                                stderr: format!("Could not parse blob size from: {line}"),
                            })
                    }
                    _ => Err(GitError::CommandFailed {
                        command: format!("git cat-file --batch-check ({})", entry.blob_hash),
                        exit_code: -1,
                        stderr: format!("Unexpected output: {line}"),
                    }),
                }
            })
            .collect()
    }

    /// Walk every commit in the range and collect all added/modified blobs.
    /// Uses `git rev-list` + parallel `git diff-tree --stdin` workers +
    /// single `git cat-file --batch-check`.
    pub fn walk_history_blobs(&self, range: &str) -> Result<Vec<HistoryBlob>> {
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
        assert!(expanded.contains("..HEAD"));
        assert!(!expanded.contains("..."));
    }

    #[test]
    #[ignore = "requires git binary"]
    fn test_expand_git_range_two_dot() {
        let (_tmp, repo) = setup_test_repo();
        let expanded = repo.expand_git_range("HEAD~1..HEAD").unwrap();
        assert_eq!(expanded, "HEAD~1..HEAD");
    }

    #[test]
    #[ignore = "requires git binary"]
    fn test_expand_git_range_three_dot() {
        let (_tmp, repo) = setup_test_repo();
        let expanded = repo.expand_git_range("HEAD~1...HEAD").unwrap();
        assert_eq!(expanded, "HEAD~1...HEAD");
    }
}
