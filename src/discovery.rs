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
