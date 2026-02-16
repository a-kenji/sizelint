use miette::Diagnostic;
use std::path::{Path, PathBuf};
use std::process::Command;
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

    /// List all commits in a range, oldest first, skipping merges.
    pub fn list_commits_in_range(&self, range: &str) -> Result<Vec<String>> {
        let expanded = self.expand_git_range(range)?;
        let command = format!("git rev-list --no-merges --reverse {expanded}");

        let output = Command::new("git")
            .args(["rev-list", "--no-merges", "--reverse"])
            .arg(&expanded)
            .current_dir(&self.root)
            .output()
            .map_err(GitError::Exec)?;

        if !output.status.success() {
            return Err(self.command_failed(&command, &output));
        }

        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| line.to_string())
            .collect())
    }

    /// Get blobs added/modified in a single commit via `git diff-tree`.
    /// Skips submodule entries (mode 160000).
    pub fn get_changed_blobs_in_commit(&self, commit: &str) -> Result<Vec<HistoryBlob>> {
        let command = format!("git diff-tree --no-commit-id -r --diff-filter=ACMRT {commit}");

        let output = Command::new("git")
            .args([
                "diff-tree",
                "--no-commit-id",
                "-r",
                "--diff-filter=ACMRT",
                commit,
            ])
            .current_dir(&self.root)
            .output()
            .map_err(GitError::Exec)?;

        if !output.status.success() {
            return Err(self.command_failed(&command, &output));
        }

        let short_commit = &commit[..commit.len().min(12)];
        let mut blobs = Vec::new();

        // Each line: :<old_mode> <new_mode> <old_hash> <new_hash> <status>\t<path>
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Split on tab to separate metadata from path
            let Some((meta, path)) = line.split_once('\t') else {
                continue;
            };

            let parts: Vec<&str> = meta.split_whitespace().collect();
            if parts.len() < 5 {
                continue;
            }

            // parts[1] is the new mode — skip submodules
            let new_mode = parts[1];
            if new_mode == "160000" {
                continue;
            }

            // parts[3] is the new blob hash
            let blob_hash = parts[3];

            let size = self.get_blob_size_by_hash(blob_hash)?;

            blobs.push(HistoryBlob {
                path: self.root.join(path).to_string_lossy().to_string(),
                size,
                commit: short_commit.to_string(),
            });
        }

        Ok(blobs)
    }

    fn get_blob_size_by_hash(&self, blob_hash: &str) -> Result<u64> {
        let output = Command::new("git")
            .args(["cat-file", "-s", blob_hash])
            .current_dir(&self.root)
            .output()
            .map_err(GitError::Exec)?;

        if !output.status.success() {
            return Err(self.command_failed(&format!("git cat-file -s {blob_hash}"), &output));
        }

        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u64>()
            .map_err(|_| GitError::CommandFailed {
                command: format!("git cat-file -s {blob_hash}"),
                exit_code: -1,
                stderr: "Could not parse blob size".to_string(),
            })
    }

    /// Walk every commit in the range and collect all added/modified blobs.
    pub fn walk_history_blobs(&self, range: &str) -> Result<Vec<HistoryBlob>> {
        let commits = self.list_commits_in_range(range)?;
        let mut blobs = Vec::new();
        for commit in &commits {
            blobs.extend(self.get_changed_blobs_in_commit(commit)?);
        }
        Ok(blobs)
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
