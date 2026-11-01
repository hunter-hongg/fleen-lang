//! Standard library resolution (0.0.5 T08, D8).
//!
//! Provides a three-tier lookup order for std sources:
//! 1. FLINT_STD_PATH environment variable (explicit override)
//! 2. $FLINT_HOME/std/<compiler-version>/ (release copy)
//! 3. Embedded copy (fleen-compiler/src/std_src.rs)
//!
//! External std copies must match the compiler version exactly; a mismatch
//! is a hard error naming both versions.

use std::env;
use std::path::{Path, PathBuf};

/// The compiler version (from workspace Cargo.toml).
pub const COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Environment variable for explicit std override.
pub const FLINT_STD_PATH: &str = "FLINT_STD_PATH";

/// Environment variable for the Flint home directory.
pub const FLINT_HOME: &str = "FLINT_HOME";

/// Name of the version file inside an external std tree.
pub const VERSION_FILE: &str = "VERSION";

/// A source of std files, with its origin and path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StdSource {
    /// Explicit override via FLINT_STD_PATH.
    EnvVar(PathBuf),
    /// Release copy at $FLINT_HOME/std/<version>/.
    FlintHome(PathBuf),
    /// Embedded copy (std_src.rs).
    Embedded,
}

impl StdSource {
    /// The directory path for this source (if applicable).
    pub fn path(&self) -> Option<&Path> {
        match self {
            StdSource::EnvVar(p) => Some(p),
            StdSource::FlintHome(p) => Some(p),
            StdSource::Embedded => None,
        }
    }

    /// Human-readable description of the source.
    pub fn description(&self) -> &'static str {
        match self {
            StdSource::EnvVar(_) => "FLINT_STD_PATH",
            StdSource::FlintHome(_) => "$FLINT_HOME",
            StdSource::Embedded => "embedded",
        }
    }
}

/// Diagnostic information about which std source is in effect.
#[derive(Debug, Clone)]
pub struct StdDiagnosis {
    /// The selected source.
    pub source: StdSource,
    /// The directory path (if applicable).
    pub path: Option<PathBuf>,
    /// The std version.
    pub version: String,
}

/// Error type for std resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    /// Version mismatch between external std and compiler.
    VersionMismatch {
        external: String,
        compiler: String,
        source: StdSource,
    },
    /// External std lacks a VERSION file.
    NoVersionFile(PathBuf),
    /// Failed to read VERSION file.
    VersionReadError(PathBuf, String),
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::VersionMismatch {
                external,
                compiler,
                source,
            } => {
                write!(
                    f,
                    "External std version {} does not match compiler version {}.                      Source: {}. Set FLINT_STD_PATH to a matching std or remove the {} copy.",
                    external,
                    compiler,
                    source.description(),
                    source.description()
                )
            }
            ResolveError::NoVersionFile(path) => {
                write!(f, "External std lacks a VERSION file at {}", path.display())
            }
            ResolveError::VersionReadError(path, err) => {
                write!(
                    f,
                    "Failed to read VERSION file at {}: {}",
                    path.display(),
                    err
                )
            }
        }
    }
}

impl std::error::Error for ResolveError {}

/// Resolver for std sources.
pub struct StdResolver {
    compiler_version: String,
}

impl StdResolver {
    /// Create a new resolver with the given compiler version.
    pub fn new(compiler_version: String) -> Self {
        Self { compiler_version }
    }

    /// Create a resolver using the compiler's built-in version.
    pub fn from_compiler_version() -> Self {
        Self::new(COMPILER_VERSION.to_string())
    }

    /// Resolve the std source using the three-tier lookup order.
    ///
    /// Tier 1: FLINT_STD_PATH (explicit override)
    /// Tier 2: $FLINT_HOME/std/<version>/
    /// Tier 3: Embedded copy (always available)
    pub fn resolve(&self) -> Result<StdSource, ResolveError> {
        // Tier 1: FLINT_STD_PATH (explicit override)
        if let Some(source) = self.try_env_var()? {
            return Ok(source);
        }

        // Tier 2: $FLINT_HOME/std/<version>/
        if let Some(source) = self.try_flint_home()? {
            return Ok(source);
        }

        // Tier 3: Embedded copy (always available)
        Ok(StdSource::Embedded)
    }

    /// Try to resolve from FLINT_STD_PATH.
    ///
    /// Returns:
    /// - Ok(Some(source)) — env var present and version matches
    /// - Ok(None) — env var absent, path missing, or not a directory (fall through)
    /// - Err — env var present but invalid (version mismatch etc.)
    fn try_env_var(&self) -> Result<Option<StdSource>, ResolveError> {
        let Ok(raw) = env::var(FLINT_STD_PATH) else {
            return Ok(None);
        };
        let path = PathBuf::from(raw);

        if !path.exists() {
            return Ok(None); // Missing path: fall through to next tier
        }
        if !path.is_dir() {
            return Ok(None); // Not a directory: fall through to next tier
        }

        // Version mismatch is a hard error, never a silent fallback.
        self.validate_version(&path, &StdSource::EnvVar(path.clone()))?;
        Ok(Some(StdSource::EnvVar(path)))
    }

    /// Try to resolve from $FLINT_HOME/std/<version>/.
    fn try_flint_home(&self) -> Result<Option<StdSource>, ResolveError> {
        let Ok(home) = env::var(FLINT_HOME) else {
            return Ok(None);
        };
        let std_path = PathBuf::from(home).join("std").join(&self.compiler_version);

        if !std_path.exists() {
            return Ok(None); // Missing path: fall through to embedded
        }
        if !std_path.is_dir() {
            return Ok(None); // Not a directory: fall through to embedded
        }

        // Version mismatch is a hard error, never a silent fallback.
        self.validate_version(&std_path, &StdSource::FlintHome(std_path.clone()))?;
        Ok(Some(StdSource::FlintHome(std_path)))
    }

    /// Validate that an external std's version matches the compiler's.
    fn validate_version(&self, path: &Path, source: &StdSource) -> Result<(), ResolveError> {
        let version = self.read_version_at_path(path)?;
        if version != self.compiler_version {
            return Err(ResolveError::VersionMismatch {
                external: version,
                compiler: self.compiler_version.clone(),
                source: source.clone(),
            });
        }
        Ok(())
    }

    /// Diagnostic-only version: reports which source is in effect without loading.
    pub fn diagnose(&self) -> Result<StdDiagnosis, ResolveError> {
        let source = self.resolve()?;
        let path = source.path().map(|p| p.to_path_buf());
        let version = self.read_version(&source)?;
        Ok(StdDiagnosis {
            source,
            path,
            version,
        })
    }

    /// Read the version from a StdSource.
    fn read_version(&self, source: &StdSource) -> Result<String, ResolveError> {
        match source {
            StdSource::EnvVar(path) | StdSource::FlintHome(path) => self.read_version_at_path(path),
            StdSource::Embedded => Ok(self.compiler_version.clone()),
        }
    }

    /// Read the version from a directory path.
    fn read_version_at_path(&self, path: &Path) -> Result<String, ResolveError> {
        let version_file = path.join(VERSION_FILE);
        if !version_file.exists() {
            return Err(ResolveError::NoVersionFile(version_file));
        }
        std::fs::read_to_string(&version_file)
            .map(|s| s.trim().to_string())
            .map_err(|e| ResolveError::VersionReadError(version_file, e.to_string()))
    }
}

impl Default for StdResolver {
    fn default() -> Self {
        Self::from_compiler_version()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TEST_SEQ: AtomicU32 = AtomicU32::new(0);

    /// Serializes env-var-mutating tests: set_var/remove_var are process-wide,
    /// so parallel test execution would race on the shared environment.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn tmp_dir(tag: &str) -> PathBuf {
        let n = TEST_SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "fleen-std-resolver-{tag}-{}-{}",
            std::process::id(),
            n
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolver_defaults_to_compiler_version() {
        let resolver = StdResolver::default();
        assert_eq!(resolver.compiler_version, COMPILER_VERSION);
    }

    #[test]
    fn embedded_always_available() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard_home = EnvGuard::remove(FLINT_HOME);
        let _guard_path = EnvGuard::remove(FLINT_STD_PATH);
        let resolver = StdResolver::from_compiler_version();
        let source = resolver.resolve().unwrap();
        assert_eq!(source, StdSource::Embedded);
    }

    #[test]
    fn diagnose_works() {
        let resolver = StdResolver::from_compiler_version();
        let diag = resolver.diagnose().unwrap();
        assert_eq!(diag.source, StdSource::Embedded);
        assert_eq!(diag.version, COMPILER_VERSION);
    }

    /// D8 tier 1: FLINT_STD_PATH wins when set and valid.
    #[test]
    fn flint_std_path_env_var_wins() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tmp_dir("env-wins");
        fs::write(dir.join(VERSION_FILE), COMPILER_VERSION).unwrap();
        fs::write(dir.join("mod.fln"), "").unwrap();

        // SAFETY: scoped env var; restored by drop.
        let _guard = EnvGuard::set(FLINT_STD_PATH, dir.to_str().unwrap().to_string());

        let resolver = StdResolver::from_compiler_version();
        let source = resolver.resolve().unwrap();
        assert_eq!(source, StdSource::EnvVar(dir.clone()));
        assert!(source.path().unwrap().is_dir());

        let _ = fs::remove_dir_all(&dir);
    }

    /// D8 tier 2: FLINT_HOME/std/<version> used when FLINT_STD_PATH absent.
    #[test]
    fn flint_home_std_used_when_env_var_absent() {
        let _lock = ENV_LOCK.lock().unwrap();
        let home = tmp_dir("home");
        let std_ver = home.join("std").join(COMPILER_VERSION);
        fs::create_dir_all(&std_ver).unwrap();
        fs::write(std_ver.join(VERSION_FILE), COMPILER_VERSION).unwrap();
        fs::write(std_ver.join("mod.fln"), "").unwrap();

        let _guard_env = EnvGuard::set(FLINT_HOME, home.to_str().unwrap().to_string());
        let _guard_path = EnvGuard::remove(FLINT_STD_PATH);

        let resolver = StdResolver::from_compiler_version();
        let source = resolver.resolve().unwrap();
        assert_eq!(source, StdSource::FlintHome(std_ver));

        let _ = fs::remove_dir_all(&home);
    }

    /// D8 tier 3: embedded used when neither override present.
    #[test]
    fn embedded_used_when_no_overrides() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard_home = EnvGuard::remove(FLINT_HOME);
        let _guard_path = EnvGuard::remove(FLINT_STD_PATH);

        let resolver = StdResolver::from_compiler_version();
        let source = resolver.resolve().unwrap();
        assert_eq!(source, StdSource::Embedded);
    }

    /// D8: version mismatch is a hard error, never a silent fallback.
    #[test]
    fn version_mismatch_env_var_is_hard_error() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tmp_dir("mismatch");
        fs::write(dir.join(VERSION_FILE), "0.0.99").unwrap();

        let _guard = EnvGuard::set(FLINT_STD_PATH, dir.to_str().unwrap().to_string());

        let resolver = StdResolver::from_compiler_version();
        let err = resolver.resolve().unwrap_err();
        match err {
            ResolveError::VersionMismatch {
                external,
                compiler,
                source,
            } => {
                assert_eq!(external, "0.0.99");
                assert_eq!(compiler, COMPILER_VERSION);
                assert!(matches!(source, StdSource::EnvVar(_)));
            }
            other => panic!("expected VersionMismatch, got {other:?}"),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    /// D8: version mismatch in FLINT_HOME is also a hard error.
    #[test]
    fn version_mismatch_flint_home_is_hard_error() {
        let _lock = ENV_LOCK.lock().unwrap();
        let home = tmp_dir("home-mismatch");
        let std_ver = home.join("std").join(COMPILER_VERSION);
        fs::create_dir_all(&std_ver).unwrap();
        fs::write(std_ver.join(VERSION_FILE), "0.0.98").unwrap();

        let _guard_home = EnvGuard::set(FLINT_HOME, home.to_str().unwrap().to_string());
        let _guard_path = EnvGuard::remove(FLINT_STD_PATH);

        let resolver = StdResolver::from_compiler_version();
        let err = resolver.resolve().unwrap_err();
        match err {
            ResolveError::VersionMismatch {
                external,
                compiler,
                source,
            } => {
                assert_eq!(external, "0.0.98");
                assert_eq!(compiler, COMPILER_VERSION);
                assert!(matches!(source, StdSource::FlintHome(_)));
            }
            other => panic!("expected VersionMismatch, got {other:?}"),
        }

        let _ = fs::remove_dir_all(&home);
    }

    /// D8: missing FLINT_STD_PATH silently falls through to embedded.
    #[test]
    fn missing_env_var_path_falls_through() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard_path = EnvGuard::set(FLINT_STD_PATH, "/nonexistent/path/xyz".to_string());
        let _guard_home = EnvGuard::remove(FLINT_HOME);

        let resolver = StdResolver::from_compiler_version();
        let source = resolver.resolve().unwrap();
        assert_eq!(source, StdSource::Embedded);
    }

    /// D8: missing FLINT_HOME/std/<version> silently falls through to embedded.
    #[test]
    fn missing_flint_home_falls_through() {
        let _lock = ENV_LOCK.lock().unwrap();
        let home = tmp_dir("missing-home");
        let _guard_home = EnvGuard::set(FLINT_HOME, home.to_str().unwrap().to_string());
        let _guard_path = EnvGuard::remove(FLINT_STD_PATH);

        let resolver = StdResolver::from_compiler_version();
        let source = resolver.resolve().unwrap();
        assert_eq!(source, StdSource::Embedded);

        let _ = fs::remove_dir_all(&home);
    }

    /// D8: an external std without a VERSION file is a hard error, not a fallback.
    #[test]
    fn missing_version_file_is_hard_error() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tmp_dir("no-version");
        fs::write(dir.join("mod.fln"), "").unwrap();

        let _guard = EnvGuard::set(FLINT_STD_PATH, dir.to_str().unwrap().to_string());

        let resolver = StdResolver::from_compiler_version();
        let err = resolver.resolve().unwrap_err();
        assert!(matches!(err, ResolveError::NoVersionFile(_)));

        let _ = fs::remove_dir_all(&dir);
    }

    /// Environment guard: set/remove env var for the duration of a test.
    struct EnvGuard {
        var: &'static str,
        old: Option<Option<String>>,
    }

    impl EnvGuard {
        fn set(var: &'static str, value: String) -> Self {
            let old = std::env::var_os(var).map(|v| v.to_string_lossy().into_owned());
            unsafe {
                std::env::set_var(var, value);
            }
            Self {
                var,
                old: Some(old),
            }
        }

        fn remove(var: &'static str) -> Self {
            let old = std::env::var_os(var).map(|v| v.to_string_lossy().into_owned());
            unsafe {
                std::env::remove_var(var);
            }
            Self {
                var,
                old: Some(old),
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(old) = &self.old {
                match old {
                    Some(v) => unsafe { std::env::set_var(self.var, v) },
                    None => unsafe { std::env::remove_var(self.var) },
                }
            }
        }
    }
}
