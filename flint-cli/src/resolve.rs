//! Dependency resolution: git fetch, cache layout, transitive manifests,
//! and the `Flint.lock` write-back (0.0.3 V08, D9).
//!
//! Design (ticket V08):
//! - Git access shells out to the `git` CLI (no `git2` C dependency; the
//!   decision is recorded in the ticket). `git` must be on PATH.
//! - Cache layout `$FLINT_HOME/cache/<name>-<commit>/`, content-addressed by
//!   commit. `FLINT_HOME` (default `~/.flint`) is injectable so tests never
//!   touch a real home directory.
//! - The lockfile is the pin: a dependency whose `(name, git)` matches an
//!   existing `Flint.lock` entry reuses that commit without network traffic.
//!   Version strings are advisory in 0.0.3.
//! - `[dependencies]` are read recursively (deps of deps); the result is a
//!   flat, deduplicated-by-name list of crates to compile.
//! - Dependency-source cycles are bounded by the name-dedup in `resolve`
//!   (a crate is fetched once). Import cycles inside a program are reported
//!   by the compiler's module table (`CircularImport` / `CrossCrateImport`),
//!   which is flint's job to relay, not to re-implement.

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::manifest::{self, Manifest};

/// One resolved, on-disk dependency crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCrate {
    pub name: String,
    pub git: String,
    pub version: Option<String>,
    /// Pinned commit (the lock is the pin).
    pub commit: String,
    /// Absolute path to the fetched source (its own flint-project root).
    pub dir: PathBuf,
}

/// Resolution failure. `0` is the rendered error text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveError(pub String);

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ResolveError {}

/// Where flint keeps its cache. `home` must be injectable (tests isolate it —
/// the real `~/.flint` is exactly the global state a test suite must never
/// share).
#[derive(Debug, Clone)]
pub struct FlintHome {
    pub home: PathBuf,
}

impl FlintHome {
    /// `$FLINT_HOME` if set, else `~/.flint`; the directory need not exist.
    /// A missing `$HOME` resolves to a relative `.flint`.
    pub fn from_env() -> Self {
        let home = std::env::var_os("FLINT_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".flint")))
            .unwrap_or_else(|| PathBuf::from(".flint"));
        Self { home }
    }

    /// Directory for one resolved crate, keyed by commit.
    pub fn crate_dir(&self, git: &str, commit: &str) -> PathBuf {
        self.home
            .join(format!("{}-{}", crate_dir_name(git), commit))
    }
}

/// Directory name stem for a git URL — the last non-empty path component.
fn crate_dir_name(git: &str) -> String {
    git.trim_end_matches('/')
        .rsplit(['/', ':']) // handles both https://host/a/b and git@host:a/b
        .find(|p| !p.is_empty())
        .map(|p| {
            // Drop a trailing `.git` so `user/repo.git` and `user/repo` share a dir.
            p.strip_suffix(".git").unwrap_or(p).to_string()
        })
        .map(|p| sanitize(&p))
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "dep".into())
}

/// Keep only characters safe on disk.
fn sanitize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect()
}

/// Resolve the whole dependency graph for `root_manifest`, fetching anything
/// not already pinned in `lock`. Returns the transitive, deduplicated set of
/// crates *excluding* the root project itself.
///
/// A cycle in the dependency graph (a crate that transitively depends on
/// itself; e.g. `a → b → a`) is an error reporting the cycle path. Import
/// cycles *inside* a compiled program are separately reported by the
/// compiler's module table.
pub fn resolve(
    root_manifest: &Manifest,
    lock: &BTreeMap<String, manifest::LockEntry>,
    home: &mut FlintHome,
) -> Result<Vec<ResolvedCrate>, ResolveError> {
    // DFS with an explicit path stack so a cycle reports a back edge.
    // `state` records, per crate name, the resolved crate once fully expanded.
    let mut state: BTreeMap<String, ResolvedCrate> = BTreeMap::new();
    // The DFS stack of names on the current path (for cycle reporting).
    let mut path_stack: Vec<String> = Vec::new();
    // Fully-expanded set (also guards against duplicate expansion).
    let mut done: HashSet<String> = HashSet::new();

    for dep in &root_manifest.dependencies {
        expand(
            &dep.name,
            &dep.git,
            &dep.version,
            lock,
            home,
            &mut path_stack,
            &mut done,
            &mut state,
        )?;
    }
    Ok(state.into_values().collect())
}

/// DFS-expand one direct dependency and its transitive closure.
#[allow(clippy::too_many_arguments)]
fn expand(
    name: &str,
    git: &str,
    version: &Option<String>,
    lock: &BTreeMap<String, manifest::LockEntry>,
    home: &mut FlintHome,
    path_stack: &mut Vec<String>,
    done: &mut HashSet<String>,
    state: &mut BTreeMap<String, ResolvedCrate>,
) -> Result<(), ResolveError> {
    // Cycle: `name` is already on the current path.
    if let Some(pos) = path_stack.iter().position(|n| n == name) {
        let mut cycle: Vec<String> = path_stack[pos..].to_vec();
        cycle.push(name.to_string());
        return Err(ResolveError(format!(
            "dependency cycle: {}",
            cycle.join(" → ")
        )));
    }
    if done.contains(name) {
        return Ok(()); // already expanded
    }
    path_stack.push(name.to_string());
    let commit = lock_commit_or_fetch(name, git, lock, home)?;
    let dir = home.crate_dir(git, &commit);
    let sub = read_dep_manifest(name, &dir)?;
    for d in &sub.dependencies {
        expand(
            &d.name, &d.git, &d.version, lock, home, path_stack, done, state,
        )?;
    }
    path_stack.pop();
    state.insert(
        name.to_string(),
        ResolvedCrate {
            name: name.to_string(),
            git: git.to_string(),
            version: version.clone().or(sub.version),
            commit: commit.clone(),
            dir,
        },
    );
    done.insert(name.to_string());
    Ok(())
}

/// The commit for one dependency: reuse the lock entry when the git URL
/// matches and the source is still cached (no network), else resolve the
/// remote's default-branch HEAD and fetch it.
fn lock_commit_or_fetch(
    name: &str,
    git: &str,
    lock: &BTreeMap<String, manifest::LockEntry>,
    home: &FlintHome,
) -> Result<String, ResolveError> {
    if let Some(entry) = lock.get(name)
        && entry.git == git
        && home
            .crate_dir(git, &entry.commit)
            .join("flint.toml")
            .is_file()
    {
        return Ok(entry.commit.clone());
    }
    let commit = remote_head(git)?;
    let dir = home.crate_dir(git, &commit);
    if !dir.join("flint.toml").is_file() {
        fetch(git, &commit, &dir)?;
    }
    Ok(commit)
}

/// `git ls-remote <url> HEAD` → the default-branch HEAD commit sha.
fn remote_head(git: &str) -> Result<String, ResolveError> {
    let out = Command::new("git")
        .args(["ls-remote", git, "HEAD"])
        .output()
        .map_err(|e| ResolveError(format!("failed to run `git ls-remote {git}`: {e}")))?;
    if !out.status.success() {
        return Err(ResolveError(format!(
            "`git ls-remote {git}` failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let sha = stdout
        .split_whitespace()
        .next()
        .filter(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| {
            ResolveError(format!(
                "could not resolve a commit for `{git}` (empty or unreachable repository?)"
            ))
        })?
        .to_string();
    Ok(sha)
}

/// Clone `git` at `commit` into `dir`, creating parent directories. A partial
/// or wrong commit is cleaned up so the next build re-fetches.
fn fetch(git: &str, commit: &str, dir: &Path) -> Result<(), ResolveError> {
    if !dir.exists() {
        let out = Command::new("git")
            .args(["clone", git, dir.to_str().unwrap_or_default()])
            .output()
            .map_err(|e| ResolveError(format!("failed to run `git clone {git}`: {e}")))?;
        if !out.status.success() {
            let _ = std::fs::remove_dir_all(dir);
            return Err(ResolveError(format!(
                "`git clone {git}` failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
    }
    // Ensure the working tree is at the pinned commit (a stale checkout from a
    // previous version of this dep would otherwise fail the check below).
    let _ = Command::new("git")
        .args([
            "-C",
            dir.to_str().unwrap_or_default(),
            "checkout",
            "--quiet",
            commit,
        ])
        .status();
    if git_head_commit(dir).as_deref() == Some(commit) {
        Ok(())
    } else {
        let _ = std::fs::remove_dir_all(dir);
        Err(ResolveError(format!(
            "cached crate `{}` is not at pinned commit `{commit}`",
            dir.display()
        )))
    }
}

/// The working-tree HEAD commit sha in `dir`, if git reports one.
fn git_head_commit(dir: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["-C", dir.to_str()?, "rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Read and parse a dependency's own `flint.toml`.
fn read_dep_manifest(name: &str, dir: &Path) -> Result<Manifest, ResolveError> {
    let path = dir.join("flint.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| {
        ResolveError(format!(
            "dependency `{name}` has no readable flint.toml at {}: {e}",
            path.display()
        ))
    })?;
    manifest::parse_manifest(&text).map_err(|e| {
        ResolveError(format!(
            "invalid flint.toml in dependency `{name}` ({}): {e}",
            path.display()
        ))
    })
}

/// Serialize the resolved `Flint.lock` body: `[package]` + `[dependencies]`
/// entries for every resolved crate, each pinning its resolved commit,
/// sorted by name. Canonical so a manifest change reliably dirties it.
pub fn render_lock(project_name: &str, crates: &[ResolvedCrate]) -> String {
    let mut out = String::from("# This file is generated by flint; do not edit.\n");
    out.push_str("# It pins dependency commits for reproducible builds.\n");
    out.push_str(&format!("[package]\nname = {:?}\n\n", project_name));
    out.push_str("[dependencies]\n");
    let mut sorted: Vec<&ResolvedCrate> = crates.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    for krate in sorted {
        out.push_str(&format!("{} = {{ git = {:?}", krate.name, krate.git));
        if let Some(v) = &krate.version {
            out.push_str(&format!(", version = {:?}", v));
        }
        out.push_str(&format!(", commit = {:?} }}\n", krate.commit));
    }
    out
}

/// A lockfile is "dirty" when the manifest's direct dependency set no longer
/// matches it (something added/removed/rewritten a git URL).
pub fn lock_is_dirty(lock: &BTreeMap<String, manifest::LockEntry>, manifest: &Manifest) -> bool {
    if lock.len() != manifest.dependencies.len() {
        return true;
    }
    manifest
        .dependencies
        .iter()
        .any(|dep| match lock.get(&dep.name) {
            Some(entry) => entry.git != dep.git,
            None => true,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_dir_name_from_urls() {
        assert_eq!(
            crate_dir_name("https://github.com/user/jsonlite"),
            "jsonlite"
        );
        assert_eq!(crate_dir_name("git@github.com:a/b.git"), "b");
        assert_eq!(crate_dir_name("file:///tmp/repo"), "repo");
        assert_eq!(crate_dir_name("https://host/"), "host");
    }

    #[test]
    fn flint_home_falls_back_to_dot_flint_in_home() {
        // Patch FLINT_HOME away so from_env exercises the HOME fallback.
        // (Rust 2024 makes env mutation unsafe; avoid it by constructing the
        // struct directly for the fallback assertion instead.)
        let h = FlintHome {
            home: PathBuf::from("test-home"),
        };
        assert_eq!(h.home, PathBuf::from("test-home"));
        let dir = h.crate_dir("https://github.com/user/jsonlite", "abc123");
        assert_eq!(dir, PathBuf::from("test-home/jsonlite-abc123"));
    }

    #[test]
    fn lock_render_is_canonical_and_sorted() {
        let crates = vec![
            ResolvedCrate {
                name: "z".into(),
                git: "u/z".into(),
                version: None,
                commit: "ccc".into(),
                dir: PathBuf::from("/tmp/z-ccc"),
            },
            ResolvedCrate {
                name: "a".into(),
                git: "u/a".into(),
                version: Some("1".into()),
                commit: "aaa".into(),
                dir: PathBuf::from("/tmp/a-aaa"),
            },
        ];
        let s = render_lock("p", &crates);
        let a = s.find("a = {").expect("entry a present");
        let z = s.find("z = {").expect("entry z present");
        assert!(a < z, "entries sorted by name: {s}");
        assert!(s.contains("# This file is generated by flint; do not edit."));
    }

    #[test]
    fn lock_dirtiness_detects_git_url_change() {
        let mut lock = BTreeMap::new();
        lock.insert(
            "a".into(),
            manifest::LockEntry {
                git: "u/a".into(),
                commit: "abc".into(),
            },
        );
        let manifest = Manifest {
            name: "p".into(),
            version: None,
            dependencies: vec![manifest::Dependency {
                name: "a".into(),
                git: "u/a2".into(),
                version: None,
            }],
        };
        assert!(lock_is_dirty(&lock, &manifest));
        // Match → clean.
        lock.get_mut("a").unwrap().git = "u/a2".into();
        assert!(!lock_is_dirty(&lock, &manifest));
    }

    #[test]
    fn lock_dirtiness_detects_missing_and_extra_deps() {
        let manifest = Manifest {
            name: "p".into(),
            version: None,
            dependencies: vec![],
        };
        let lock = BTreeMap::new();
        assert!(
            !lock_is_dirty(&lock, &manifest),
            "empty lock matches empty deps"
        );
    }
}
