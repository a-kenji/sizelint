use crate::error::{Result, SizelintError};
use crate::git::GitRepo;
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use tracing::{Level, debug, span};

const DEFAULT_FILES_CAPACITY: usize = 1024;
const DEFAULT_DIR_CAPACITY: usize = 512;

pub struct FileDiscovery {
    root: PathBuf,
    git_repo: Option<GitRepo>,
    excludes: GlobSet,
}

impl FileDiscovery {
    pub fn new<P: AsRef<Path>>(root: P, exclude_patterns: &[String]) -> Result<Self> {
        let root = root.as_ref().to_path_buf();

        let git_repo = GitRepo::discover(&root).ok();

        let mut builder = GlobSetBuilder::new();
        for pattern in exclude_patterns {
            let glob = Glob::new(pattern)
                .map_err(|e| SizelintError::config_invalid_pattern(pattern.clone(), e))?;
            builder.add(glob);
        }
        let excludes = builder.build().map_err(|e| {
            SizelintError::config_invalid(
                "exclude_patterns".to_string(),
                "globset_builder".to_string(),
                format!("Failed to build exclude patterns: {e}"),
            )
        })?;

        Ok(FileDiscovery {
            root,
            git_repo,
            excludes,
        })
    }

    fn create_walker(&self, root: &Path, respect_gitignore: bool) -> WalkBuilder {
        let mut builder = WalkBuilder::new(root);
        builder
            .hidden(false)
            .git_ignore(respect_gitignore)
            .git_global(respect_gitignore)
            .git_exclude(respect_gitignore)
            .threads(rayon::current_num_threads());
        builder
    }

    fn walk_parallel(&self, walker: ignore::WalkParallel, capacity: usize) -> Result<Vec<PathBuf>> {
        let files = std::sync::Mutex::new(Vec::with_capacity(capacity));

        walker.run(|| {
            let files = &files;
            let excludes = &self.excludes;

            Box::new(move |entry| {
                match entry {
                    Ok(entry) if entry.file_type().is_some_and(|ft| ft.is_file()) => {
                        let path = entry.path();

                        // Skip files inside .git directory
                        // This is needed, because we walk hidden files
                        // hidden(false) by default
                        if path.components().any(|c| c.as_os_str() == ".git") {
                            return ignore::WalkState::Continue;
                        }

                        if !excludes.is_match(path) {
                            files.lock().unwrap().push(path.to_path_buf());
                        }
                    }
                    Err(_) => return ignore::WalkState::Quit,
                    _ => {}
                }
                ignore::WalkState::Continue
            })
        });

        let files = files.into_inner().unwrap();
        Ok(files)
    }

    pub fn discover_files(&self, respect_gitignore: bool) -> Result<Vec<PathBuf>> {
        let _span = span!(
            Level::DEBUG,
            "discover_files",
            respect_gitignore = respect_gitignore
        )
        .entered();

        let builder = self.create_walker(&self.root, respect_gitignore);
        let walker = builder.build_parallel();
        let files = self.walk_parallel(walker, DEFAULT_FILES_CAPACITY)?;

        debug!("Discovered {} files", files.len());
        Ok(files)
    }

    pub fn discover_staged_files(&self) -> Result<Vec<PathBuf>> {
        match &self.git_repo {
            Some(git_repo) => {
                let staged_files = git_repo.get_staged_files()?;
                Ok(self.filter_files(staged_files))
            }
            None => Err(crate::git::GitError::RepoNotFound {
                path: self.root.clone(),
            }
            .into()),
        }
    }

    pub fn discover_working_tree_files(&self) -> Result<Vec<PathBuf>> {
        match &self.git_repo {
            Some(git_repo) => {
                let working_files = git_repo.get_working_tree_files()?;
                Ok(self.filter_files(working_files))
            }
            None => Err(crate::git::GitError::RepoNotFound {
                path: self.root.clone(),
            }
            .into()),
        }
    }

    pub fn discover_git_diff_files(&self, range: &str) -> Result<Vec<PathBuf>> {
        match &self.git_repo {
            Some(git_repo) => {
                let diff_files = git_repo.get_diff_files(range)?;
                Ok(self.filter_files(diff_files))
            }
            None => Err(crate::git::GitError::RepoNotFound {
                path: self.root.clone(),
            }
            .into()),
        }
    }

    pub fn discover_history_blobs(&self, range: &str) -> Result<Vec<crate::git::HistoryBlob>> {
        match &self.git_repo {
            Some(git_repo) => {
                let blobs = git_repo.walk_history_blobs(range)?;
                Ok(blobs
                    .into_iter()
                    .filter(|blob| {
                        let path = Path::new(&blob.path);
                        !self.excludes.is_match(path)
                    })
                    .collect())
            }
            None => Err(crate::git::GitError::RepoNotFound {
                path: self.root.clone(),
            }
            .into()),
        }
    }

    pub fn discover_specific_paths(&self, paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        for path in paths {
            if path.is_file() {
                if !self.excludes.is_match(path) {
                    files.push(path.clone());
                }
            } else if path.is_dir() {
                let dir_files = self.discover_files_in_directory(path)?;
                files.extend(dir_files);
            }
        }

        Ok(files)
    }

    fn discover_files_in_directory(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        let builder = self.create_walker(dir, true);
        let walker = builder.build_parallel();
        self.walk_parallel(walker, DEFAULT_DIR_CAPACITY)
    }

    fn filter_files(&self, files: Vec<PathBuf>) -> Vec<PathBuf> {
        files
            .into_par_iter()
            .filter(|path| !self.excludes.is_match(path))
            .collect()
    }

    pub fn is_in_git_repo(&self) -> bool {
        self.git_repo.is_some()
    }

    pub fn git_repo(&self) -> Option<&GitRepo> {
        self.git_repo.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    struct TestRepo {
        _temp_dir: TempDir,
        root: PathBuf,
    }

    impl TestRepo {
        fn new() -> Result<Self> {
            let temp_dir = tempfile::tempdir().map_err(|e| {
                SizelintError::filesystem(
                    "create temp directory".to_string(),
                    PathBuf::from("/tmp"),
                    e,
                )
            })?;

            let root = temp_dir.path().to_path_buf();

            std::process::Command::new("git")
                .args(["init"])
                .current_dir(&root)
                .output()
                .map_err(|e| {
                    SizelintError::filesystem("execute git init".to_string(), root.clone(), e)
                })?;

            std::process::Command::new("git")
                .args(["config", "user.email", "test@example.com"])
                .current_dir(&root)
                .output()
                .map_err(|e| {
                    SizelintError::filesystem("execute git config".to_string(), root.clone(), e)
                })?;

            std::process::Command::new("git")
                .args(["config", "user.name", "Test User"])
                .current_dir(&root)
                .output()
                .map_err(|e| {
                    SizelintError::filesystem("execute git config".to_string(), root.clone(), e)
                })?;

            Ok(TestRepo {
                _temp_dir: temp_dir,
                root,
            })
        }

        fn create_file<P: AsRef<Path>>(&self, path: P, content: &str) -> Result<PathBuf> {
            let full_path = self.root.join(path);
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    SizelintError::filesystem(
                        "create directory".to_string(),
                        parent.to_path_buf(),
                        e,
                    )
                })?;
            }
            fs::write(&full_path, content).map_err(|e| {
                SizelintError::filesystem("write file".to_string(), full_path.clone(), e)
            })?;
            Ok(full_path)
        }

        fn create_gitignore(&self, content: &str) -> Result<()> {
            self.create_file(".gitignore", content)?;
            Ok(())
        }

        fn path(&self) -> &Path {
            &self.root
        }
    }

    #[test]
    #[ignore = "requires git binary"]
    fn test_discovers_files_without_gitignore() -> Result<()> {
        let repo = TestRepo::new()?;

        repo.create_file("file1.txt", "content1")?;
        repo.create_file("src/file2.rs", "content2")?;
        repo.create_file("docs/file3.md", "content3")?;

        let discovery = FileDiscovery::new(repo.path(), &[])?;
        let files = discovery.discover_files(true)?;

        let file_names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        // Should find the files we created
        // (gitignore functionality working means git metadata is excluded)
        assert!(file_names.contains(&"file1.txt".to_string()));
        assert!(file_names.contains(&"file2.rs".to_string()));
        assert!(file_names.contains(&"file3.md".to_string()));

        // Should NOT find git metadata files
        // (.git directory should be automatically ignored)
        assert!(!file_names.iter().any(|name| name.starts_with("HEAD")));
        assert!(!file_names.iter().any(|name| name.starts_with("config")));

        Ok(())
    }

    #[test]
    #[ignore = "requires git binary"]
    fn test_respects_gitignore() -> Result<()> {
        let repo = TestRepo::new()?;

        repo.create_gitignore("*.log\nsrc/generated/\ndocs/private.md")?;
        repo.create_file("file1.txt", "content1")?;
        repo.create_file("debug.log", "log content")?;
        repo.create_file("src/main.rs", "rust code")?;
        repo.create_file("src/generated/auto.rs", "generated code")?;
        repo.create_file("docs/readme.md", "docs")?;
        repo.create_file("docs/private.md", "private docs")?;

        let discovery = FileDiscovery::new(repo.path(), &[])?;
        let files = discovery.discover_files(true)?;

        let file_names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        // Should find files not in gitignore
        assert!(file_names.contains(&"file1.txt".to_string()));
        assert!(file_names.contains(&"main.rs".to_string()));
        assert!(file_names.contains(&"readme.md".to_string()));
        assert!(file_names.contains(&".gitignore".to_string()));

        // Should NOT find ignored files
        assert!(!file_names.contains(&"debug.log".to_string()));
        assert!(!file_names.contains(&"auto.rs".to_string()));
        assert!(!file_names.contains(&"private.md".to_string()));

        Ok(())
    }

    #[test]
    #[ignore = "requires git binary"]
    fn test_ignores_gitignore_when_disabled() -> Result<()> {
        let repo = TestRepo::new()?;

        repo.create_gitignore("*.log")?;

        repo.create_file("file1.txt", "content1")?;
        repo.create_file("debug.log", "log content")?;

        let discovery = FileDiscovery::new(repo.path(), &[])?;
        let files = discovery.discover_files(false)?; // respect_gitignore = false

        let file_names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        // Should find ALL files when gitignore is disabled
        assert!(file_names.contains(&"file1.txt".to_string()));
        assert!(file_names.contains(&"debug.log".to_string()));
        assert!(file_names.contains(&".gitignore".to_string()));

        Ok(())
    }

    #[test]
    #[ignore = "requires git binary"]
    fn test_config_excludes_override_gitignore() -> Result<()> {
        let repo = TestRepo::new()?;

        repo.create_gitignore("src/generated/")?;

        repo.create_file("file1.txt", "content1")?;
        repo.create_file("debug.log", "log content")?;
        repo.create_file("src/main.rs", "rust code")?;

        let discovery = FileDiscovery::new(repo.path(), &["*.log".to_string()])?;
        let files = discovery.discover_files(true)?;

        let file_names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        // Should find files not excluded by config
        assert!(file_names.contains(&"file1.txt".to_string()));
        assert!(file_names.contains(&"main.rs".to_string()));
        assert!(file_names.contains(&".gitignore".to_string()));

        // Should NOT find files excluded by config
        // (even though gitignore allows them)
        assert!(!file_names.contains(&"debug.log".to_string()));

        Ok(())
    }

    #[test]
    #[ignore = "requires git binary"]
    fn test_specific_files_ignore_gitignore() -> Result<()> {
        let repo = TestRepo::new()?;

        repo.create_gitignore("*.log")?;

        let _file1 = repo.create_file("file1.txt", "content1")?;
        let log_file = repo.create_file("debug.log", "log content")?;

        let discovery = FileDiscovery::new(repo.path(), &[])?;

        let files = discovery.discover_specific_paths(&[log_file])?;
        assert_eq!(files.len(), 1);
        assert!(
            files[0]
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains("debug.log")
        );

        Ok(())
    }
}
