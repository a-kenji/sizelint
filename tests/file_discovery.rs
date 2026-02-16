use sizelint::discovery::FileDiscovery;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub mod utils {
    use super::*;

    pub fn tmp_mkdir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    pub fn mkdir<P>(path: P)
    where
        P: AsRef<Path>,
    {
        fs::create_dir_all(path).unwrap();
    }

    pub fn write_file<P>(path: P, content: &str)
    where
        P: AsRef<Path>,
    {
        let mut file = File::create(path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    pub struct Git<'a> {
        path: PathBuf,
        exclude: Option<&'a str>,
        ignore: Option<&'a str>,
        git_ignore: Option<&'a str>,
        /// Whether to write into `.git` directory.
        /// Useful in case testing outside of git is desired.
        write_git: bool,
    }

    impl<'a> Git<'a> {
        pub fn new(root: PathBuf) -> Self {
            Self {
                path: root,
                exclude: None,
                ignore: None,
                git_ignore: None,
                write_git: true,
            }
        }

        pub fn exclude(&mut self, content: &'a str) -> &mut Self {
            self.exclude = Some(content);
            self
        }

        pub fn ignore(&mut self, content: &'a str) -> &mut Self {
            self.ignore = Some(content);
            self
        }

        pub fn git_ignore(&mut self, content: &'a str) -> &mut Self {
            self.git_ignore = Some(content);
            self
        }

        pub fn write_git(&mut self, write_git: bool) -> &mut Self {
            self.write_git = write_git;
            self
        }

        /// Creates all configured directories and files.
        pub fn create(&mut self) {
            let git_dir = self.path.join(".git");
            if self.write_git {
                mkdir(&git_dir);
            }
            if let Some(exclude) = self.exclude {
                if !self.write_git {
                    panic!("Can't write git specific personal excludes without a .git directory.")
                }
                let info_dir = git_dir.join("info");
                mkdir(&info_dir);
                let exclude_file = info_dir.join("exclude");
                write_file(&exclude_file, exclude);
            }
            if let Some(ignore) = self.ignore {
                let ignore_file = self.path.join(".ignore");
                write_file(&ignore_file, ignore);
            }
            if let Some(gitignore) = self.git_ignore {
                let ignore_file = self.path.join(".gitignore");
                write_file(&ignore_file, gitignore);
            }
        }
    }
}

// Helper to count files by extension
fn count_files_by_extension(files: &[PathBuf], ext: &str) -> usize {
    files
        .iter()
        .filter(|f| f.extension().and_then(|e| e.to_str()) == Some(ext))
        .count()
}

// Helper to check if files contain specific names
fn files_contain_name(files: &[PathBuf], name: &str) -> bool {
    files.iter().any(|f| {
        f.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.contains(name))
    })
}

#[test]
fn test_discovery_some_matches() {
    let tmpdir = utils::tmp_mkdir();
    let tree_root = tmpdir.path();

    utils::Git::new(tree_root.to_path_buf()).create();

    let test_files = vec!["test", "test1", "test3", ".test4"];

    for file in &test_files {
        utils::write_file(tree_root.join(format!("{file}.rs")), "rust code");
        utils::write_file(tree_root.join(format!("{file}.py")), "python code");
        utils::write_file(tree_root.join(format!("{file}.js")), "javascript code");
        utils::write_file(tree_root.join(file), "plain file");
    }

    let discovery = FileDiscovery::new(tree_root, &[]).unwrap();
    let files = discovery.discover_files(true).unwrap();

    // Should find all created files (4 files * 4 extensions = 16 files)
    assert_eq!(files.len(), 16);
    assert_eq!(count_files_by_extension(&files, "rs"), 4);
    assert_eq!(count_files_by_extension(&files, "py"), 4);
    assert_eq!(count_files_by_extension(&files, "js"), 4);
}

#[test]
fn test_discovery_respects_gitignore() {
    let tmpdir = utils::tmp_mkdir();
    let tree_root = tmpdir.path();

    utils::Git::new(tree_root.to_path_buf())
        .git_ignore("*.log\ntarget/\nresult")
        .create();

    let test_files = vec!["test", "test1", ".test4"];

    for file in &test_files {
        utils::write_file(tree_root.join(format!("{file}.rs")), "rust code");
        utils::write_file(tree_root.join(format!("{file}.log")), "log content");
        utils::write_file(tree_root.join(file), "plain file");
    }
    utils::write_file(tree_root.join("result"), "result file");

    // Create target directory with files
    utils::mkdir(tree_root.join("target"));
    utils::write_file(tree_root.join("target/debug.rs"), "debug binary");

    let discovery = FileDiscovery::new(tree_root, &[]).unwrap();
    let files = discovery.discover_files(true).unwrap();

    // Should find only non-ignored files
    // 3 .rs files + 3 plain files + 1 .gitignore = 7 files
    assert_eq!(files.len(), 7);
    assert_eq!(count_files_by_extension(&files, "rs"), 3);
    assert_eq!(count_files_by_extension(&files, "log"), 0); // ignored
    assert!(!files_contain_name(&files, "result")); // ignored
    assert!(!files_contain_name(&files, "target")); // ignored
    assert!(files_contain_name(&files, ".gitignore")); // gitignore file itself
}

#[test]
fn test_discovery_ignores_gitignore_when_disabled() {
    let tmpdir = utils::tmp_mkdir();
    let tree_root = tmpdir.path();

    utils::Git::new(tree_root.to_path_buf())
        .git_ignore("*.log\nresult")
        .create();

    utils::write_file(tree_root.join("test.rs"), "rust code");
    utils::write_file(tree_root.join("test.log"), "log content");
    utils::write_file(tree_root.join("result"), "result file");

    let discovery = FileDiscovery::new(tree_root, &[]).unwrap();
    let files = discovery.discover_files(false).unwrap(); // respect_gitignore = false

    // Should find ALL files when gitignore is disabled
    assert_eq!(files.len(), 4); // .rs + .log + result + .gitignore
    assert_eq!(count_files_by_extension(&files, "rs"), 1);
    assert_eq!(count_files_by_extension(&files, "log"), 1);
    assert!(files_contain_name(&files, "result"));
}

#[test]
fn test_discovery_git_exclude() {
    let tmpdir = utils::tmp_mkdir();
    let tree_root = tmpdir.path();

    utils::Git::new(tree_root.to_path_buf())
        .git_ignore("*.log")
        .exclude("result\n.env")
        .create();

    utils::write_file(tree_root.join("test.rs"), "rust code");
    utils::write_file(tree_root.join("test.log"), "log content");
    utils::write_file(tree_root.join("result"), "result file");
    utils::write_file(tree_root.join(".env"), "env file");

    let discovery = FileDiscovery::new(tree_root, &[]).unwrap();
    let files = discovery.discover_files(true).unwrap();

    // Should respect both gitignore and git exclude
    // .rs + .gitignore files (log, result, .env excluded by gitignore/exclude)
    // Note: .git/info/exclude file itself should not be included in discovery
    assert_eq!(files.len(), 2);
    assert_eq!(count_files_by_extension(&files, "rs"), 1);
    assert!(!files_contain_name(&files, "log"));
    assert!(!files_contain_name(&files, "result"));
    assert!(!files_contain_name(&files, ".env"));
}

#[test]
fn test_discovery_ignore_file() {
    let tmpdir = utils::tmp_mkdir();
    let tree_root = tmpdir.path();

    utils::Git::new(tree_root.to_path_buf())
        .git_ignore("*.log")
        .ignore("result\n.cache\nignored*")
        .create();

    utils::write_file(tree_root.join("test.rs"), "rust code");
    utils::write_file(tree_root.join("test.log"), "log content");
    utils::write_file(tree_root.join("result"), "result file");
    utils::write_file(tree_root.join(".cache"), "cache file");
    utils::write_file(tree_root.join("ignored_file.txt"), "ignored content");

    let discovery = FileDiscovery::new(tree_root, &[]).unwrap();
    let files = discovery.discover_files(true).unwrap();

    // Should respect gitignore, .ignore files
    assert_eq!(files.len(), 3); // .rs + .gitignore + .ignore
    assert_eq!(count_files_by_extension(&files, "rs"), 1);
    assert!(!files_contain_name(&files, "log"));
    assert!(!files_contain_name(&files, "result"));
    assert!(!files_contain_name(&files, ".cache"));
    assert!(!files_contain_name(&files, "ignored_file"));
}

#[test]
fn test_discovery_not_git_directory() {
    let tmpdir = utils::tmp_mkdir();
    let tree_root = tmpdir.path();

    // Create .gitignore and .ignore files but NO .git directory
    utils::Git::new(tree_root.to_path_buf())
        .git_ignore("*.log")
        .ignore("result")
        .write_git(false)
        .create();

    utils::write_file(tree_root.join("test.rs"), "rust code");
    utils::write_file(tree_root.join("test.log"), "log content");
    utils::write_file(tree_root.join("result"), "result file");

    let discovery = FileDiscovery::new(tree_root, &[]).unwrap();
    let files = discovery.discover_files(true).unwrap();

    // Should respect .ignore but not .gitignore (no git repo)
    assert_eq!(files.len(), 4); // .rs + .log + .gitignore + .ignore (result ignored by .ignore)
    assert_eq!(count_files_by_extension(&files, "rs"), 1);
    assert_eq!(count_files_by_extension(&files, "log"), 1); // not ignored (no git)
    assert!(!files_contain_name(&files, "result")); // ignored by .ignore
}

#[test]
fn test_discovery_config_excludes() {
    let tmpdir = utils::tmp_mkdir();
    let tree_root = tmpdir.path();

    utils::Git::new(tree_root.to_path_buf())
        .git_ignore("*.log") // gitignore allows .tmp files
        .create();

    utils::write_file(tree_root.join("test.rs"), "rust code");
    utils::write_file(tree_root.join("test.log"), "log content");
    utils::write_file(tree_root.join("test.tmp"), "temp content");
    utils::write_file(tree_root.join("data.json"), "json data");

    let discovery =
        FileDiscovery::new(tree_root, &["*.tmp".to_string(), "*.json".to_string()]).unwrap();
    let files = discovery.discover_files(true).unwrap();

    // Config excludes should override gitignore behavior
    assert_eq!(files.len(), 2); // .rs + .gitignore (.log ignored by git, .tmp/.json by config)
    assert_eq!(count_files_by_extension(&files, "rs"), 1);
    assert!(!files_contain_name(&files, "log")); // ignored by gitignore
    assert!(!files_contain_name(&files, "tmp")); // ignored by config
    assert!(!files_contain_name(&files, "json")); // ignored by config
}

#[test]
fn test_discovery_specific_files_ignore_gitignore() {
    let tmpdir = utils::tmp_mkdir();
    let tree_root = tmpdir.path();

    utils::Git::new(tree_root.to_path_buf())
        .git_ignore("*.log")
        .create();

    utils::write_file(tree_root.join("test.rs"), "rust code");
    let log_file = tree_root.join("test.log");
    utils::write_file(&log_file, "log content");

    let discovery = FileDiscovery::new(tree_root, &[]).unwrap();

    // Directory traversal should ignore gitignored files
    let all_files = discovery.discover_files(true).unwrap();
    assert!(!files_contain_name(&all_files, "log"));

    // But specific file paths should work even if gitignored
    let specific_files = discovery.discover_specific_paths(&[log_file]).unwrap();
    assert_eq!(specific_files.len(), 1);
    assert!(files_contain_name(&specific_files, "log"));
}

#[test]
fn test_discovery_hidden_files() {
    let tmpdir = utils::tmp_mkdir();
    let tree_root = tmpdir.path();

    utils::Git::new(tree_root.to_path_buf()).create();

    // Create regular and hidden files
    utils::write_file(tree_root.join("regular.txt"), "regular content");
    utils::write_file(tree_root.join(".hidden.txt"), "hidden content");
    utils::write_file(tree_root.join(".env"), "env content");

    let discovery = FileDiscovery::new(tree_root, &[]).unwrap();
    let files = discovery.discover_files(true).unwrap();

    // Should find both regular and hidden files
    assert!(files_contain_name(&files, "regular.txt"));
    assert!(files_contain_name(&files, ".hidden.txt"));
    assert!(files_contain_name(&files, ".env"));
}

#[test]
fn test_discovery_nested_directories() {
    let tmpdir = utils::tmp_mkdir();
    let tree_root = tmpdir.path();

    utils::Git::new(tree_root.to_path_buf())
        .git_ignore("build/\n*.tmp")
        .create();

    // Create nested structure
    utils::mkdir(tree_root.join("src"));
    utils::mkdir(tree_root.join("tests"));
    utils::mkdir(tree_root.join("build"));

    utils::write_file(tree_root.join("src/main.rs"), "main code");
    utils::write_file(tree_root.join("src/lib.rs"), "lib code");
    utils::write_file(tree_root.join("tests/test.rs"), "test code");
    utils::write_file(tree_root.join("build/output.txt"), "build output");
    utils::write_file(tree_root.join("temp.tmp"), "temp file");

    let discovery = FileDiscovery::new(tree_root, &[]).unwrap();
    let files = discovery.discover_files(true).unwrap();

    // Should find files respecting gitignore
    assert_eq!(count_files_by_extension(&files, "rs"), 3);
    assert!(!files_contain_name(&files, "output.txt")); // build/ ignored
    assert!(!files_contain_name(&files, "temp.tmp")); // *.tmp ignored
}
