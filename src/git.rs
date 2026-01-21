use crate::error::{Result, SizelintError};
use std::path::{Path, PathBuf};
use std::process::Command;

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
            .map_err(|e| {
                SizelintError::filesystem("execute git command".to_string(), path.to_path_buf(), e)
            })?;

        if !output.status.success() {
            return Err(SizelintError::GitRepoNotFound {
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

    pub fn is_in_git_repo<P: AsRef<Path>>(path: P) -> bool {
        Command::new("git")
            .arg("rev-parse")
            .arg("--is-inside-work-tree")
            .current_dir(path)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub fn get_staged_files(&self) -> Result<Vec<PathBuf>> {
        let command = "git diff --staged --name-only --diff-filter=ACMRT";
        let output = Command::new("git")
            .arg("diff")
            .arg("--staged")
            .arg("--name-only")
            .arg("--diff-filter=ACMRT")
            .current_dir(&self.root)
            .output()
            .map_err(|e| {
                SizelintError::filesystem("execute git command".to_string(), self.root.clone(), e)
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(SizelintError::git_command_failed(
                command.to_string(),
                output.status.code().unwrap_or(-1),
                stderr,
            ));
        }

        let files = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| self.root.join(line))
            .collect();

        Ok(files)
    }

    pub fn get_working_tree_files(&self) -> Result<Vec<PathBuf>> {
        let command = "git diff --name-only --diff-filter=ACMRT";
        let output = Command::new("git")
            .arg("diff")
            .arg("--name-only")
            .arg("--diff-filter=ACMRT")
            .current_dir(&self.root)
            .output()
            .map_err(|e| {
                SizelintError::filesystem("execute git command".to_string(), self.root.clone(), e)
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(SizelintError::git_command_failed(
                command.to_string(),
                output.status.code().unwrap_or(-1),
                stderr,
            ));
        }

        let files = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| self.root.join(line))
            .collect();

        Ok(files)
    }

    pub fn get_all_files(&self) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
        let command = "git status --porcelain=v1 --untracked-files=no";
        let output = Command::new("git")
            .arg("status")
            .arg("--porcelain=v1")
            .arg("--untracked-files=no")
            .current_dir(&self.root)
            .output()
            .map_err(|e| {
                SizelintError::filesystem("execute git command".to_string(), self.root.clone(), e)
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(SizelintError::git_command_failed(
                command.to_string(),
                output.status.code().unwrap_or(-1),
                stderr,
            ));
        }

        let mut staged = Vec::new();
        let mut working_tree = Vec::new();

        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if line.len() >= 3 {
                let path = self.root.join(&line[3..]);
                let status = &line[..2];

                if !status.starts_with(' ') {
                    staged.push(path.clone());
                }

                if status.chars().nth(1).unwrap() != ' ' {
                    working_tree.push(path);
                }
            }
        }

        Ok((staged, working_tree))
    }

    pub fn is_file_tracked<P: AsRef<Path>>(&self, path: P) -> bool {
        let relative_path = match path.as_ref().strip_prefix(&self.root) {
            Ok(p) => p,
            Err(_) => return false,
        };

        Command::new("git")
            .arg("ls-files")
            .arg("--error-unmatch")
            .arg(relative_path)
            .current_dir(&self.root)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub fn is_file_lfs<P: AsRef<Path>>(&self, path: P) -> bool {
        let relative_path = match path.as_ref().strip_prefix(&self.root) {
            Ok(p) => p,
            Err(_) => return false,
        };

        let output = Command::new("git")
            .arg("check-attr")
            .arg("filter")
            .arg(relative_path)
            .current_dir(&self.root)
            .output();

        if let Ok(output) = output
            && output.status.success()
        {
            let output_str = String::from_utf8_lossy(&output.stdout);
            return output_str.contains("filter: lfs");
        }

        false
    }
}
