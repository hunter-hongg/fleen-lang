//! Whole-program compilation for a flint project (0.0.3 V08).
//!
//! This is the project-mode entry the V05 ticket promised flint would wrap:
//! it walks the project's source tree and every resolved dependency's source
//! tree, registers them all in one [`fleen_compiler::Session`] under their
//! **real paths**, and calls [`Session::compile_project`] — which runs the
//! module-table sub-pass (V05), merges the modules into one bytecode module,
//! and returns it for verification and execution.
//!
//! Diagnostics render through the session's real `SourceMap`, so errors point
//! at the actual file (a cached dependency path included), never a synthetic
//! name.

use std::fmt;
use std::path::{Path, PathBuf};

use fleen_compiler::codegen::Module;
use fleen_compiler::lexer::Span;
use fleen_compiler::source_map::SourceMap;
use fleen_compiler::{CompileError, Session};

use crate::manifest::{self, Manifest};

use crate::resolve::ResolvedCrate;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildError(pub String);

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for BuildError {}

/// Where the project source lives: `src/` if present, else the project root
/// (mirrors `fleen-vm`'s `--project` convention).
fn source_dir(project: &Path) -> PathBuf {
    if project.join("src").join("main.fln").is_file() {
        project.join("src")
    } else {
        project.to_path_buf()
    }
}

/// Recursively collect `.fln` files under `dir`, sorted for deterministic
/// module/ID ordering (V08: registration order is the ID contract).
fn collect_fln(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(read) => read.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(_) => return, // not a directory (root main.fln project)
    };
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_fln(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("fln") {
            out.push(path);
        }
    }
}

/// Load `flint.toml` from a directory.
pub fn load_manifest(project: &Path) -> Result<Manifest, BuildError> {
    let path = project.join("flint.toml");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| BuildError(format!("reading {}: {e}", path.display())))?;
    manifest::parse_manifest(&text).map_err(|e| BuildError(e.0))
}

/// Compile the project at `project` together with its resolved dependencies.
///
/// `crates` is the resolved dependency set; the root crate (the project) is
/// added first with unprefixed module paths. Returns the compiled module.
pub fn build_project(project: &Path, crates: &[ResolvedCrate]) -> Result<Module, BuildError> {
    let src = source_dir(project);
    let mut session = Session::new();

    // Register the project's own files first (module 0 must be its root).
    let mut project_files = Vec::new();
    collect_fln(&src, &mut project_files);
    project_files.sort();
    let mut root_id = None;
    for abs in &project_files {
        let rel = abs.strip_prefix(project).unwrap_or(abs);
        let rel_name = rel.to_string_lossy().replace('\\', "/");
        let source = std::fs::read_to_string(abs)
            .map_err(|e| BuildError(format!("reading {}: {e}", abs.display())))?;
        let id = session.add_file(rel_name.clone(), source);
        // The crate root is the file that reads as main.fln at the source
        // root (src/main.fln or main.fln).
        let is_main = abs
            .strip_prefix(project)
            .is_ok_and(|r| r == Path::new("src/main.fln") || r == Path::new("main.fln"));
        if is_main {
            root_id = Some(id);
        }
    }
    let root_id = root_id
        .ok_or_else(|| BuildError("project has no `src/main.fln` (or `main.fln`)".to_string()))?;

    // Register every dependency's files under their real cached paths.
    for krate in crates {
        let dep_src = source_dir(&krate.dir);
        let mut dep_files = Vec::new();
        collect_fln(&dep_src, &mut dep_files);
        dep_files.sort();
        let mut dep_root = None;
        for abs in &dep_files {
            let abs_name = abs.to_string_lossy().replace('\\', "/");
            let source = std::fs::read_to_string(abs)
                .map_err(|e| BuildError(format!("reading {}: {e}", abs.display())))?;
            let id = session.add_file(abs_name.clone(), source);
            let is_main = abs
                .strip_prefix(&krate.dir)
                .is_ok_and(|r| r == Path::new("src/main.fln") || r == Path::new("main.fln"));
            if is_main {
                dep_root = Some(id);
            }
        }
        let dep_root = dep_root.ok_or_else(|| {
            BuildError(format!(
                "dependency `{}` has no `src/main.fln` (or `main.fln`)",
                krate.name
            ))
        })?;
        session.add_dependency(&krate.name, dep_root);
    }

    session
        .compile_project(root_id)
        .map_err(|e| render_compile_error(&session, e))
}

/// Render a multi-file compile error against the session's real source map.
/// Each failing phase's locations point at the true file/line/column.
///
/// Entries carry no `error:` prefix — the CLI adds one. Multiple distinct
/// errors are separated by a blank line.
fn render_compile_error(session: &Session, e: CompileError) -> BuildError {
    let sources = session.sources();
    let mut out = String::new();
    stage_errors(&mut out, sources, &e);
    if out.is_empty() {
        // No span-bearing stage (e.g. codegen) — fall back to the Display.
        out = e.to_string();
    }
    BuildError(out)
}

fn stage_errors(out: &mut String, sources: &SourceMap, e: &CompileError) {
    use fleen_compiler::CompileError::*;
    match e {
        Lex(errors) => {
            for err in errors {
                push(out, sources, &err.kind.to_string(), err.span);
            }
        }
        Parse(err) => {
            // ParseErrorKind carries its own Display; ParseError::Display
            // appends the raw span (` at X..Y`), which we re-render as a
            // location block, so use the kind's Display.
            push(out, sources, &err.kind.to_string(), err.span);
        }
        Resolve(errors) => {
            for err in errors {
                push(out, sources, &err.kind.to_string(), err.span);
            }
        }
        Typeck(errors) => {
            for err in errors {
                push(out, sources, &err.kind.to_string(), err.span);
            }
        }
        Lower(err) => {
            push(out, sources, &err.to_string(), err.span);
        }
        Codegen(err) => {
            // Codegen errors carry no span (invariant violations from Lower).
            push_plain(out, &err.to_string());
        }
    }
}

fn push(out: &mut String, sources: &SourceMap, message: &str, span: Span) {
    if !span.is_synthetic() {
        let loc = sources.render_location(span);
        push_entry(out, message, Some(&loc));
    } else {
        push_plain(out, message);
    }
}

fn push_plain(out: &mut String, message: &str) {
    push_entry(out, message, None);
}

fn push_entry(out: &mut String, message: &str, location: Option<&str>) {
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(message);
    if let Some(loc) = location {
        out.push('\n');
        out.push_str(loc);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_project(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("flint-build-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// `source_dir` prefers `src/` when it contains `main.fln`, else the root.
    #[test]
    fn source_dir_prefers_src_with_main() {
        let proj = tmp_project("src");
        fs::create_dir_all(proj.join("src")).unwrap();
        fs::write(proj.join("src/main.fln"), "").unwrap();
        assert_eq!(source_dir(&proj), proj.join("src"));

        let _ = fs::remove_dir_all(&proj);
    }

    /// A single-file project (root `main.fln`, no `src/`) roots at the project.
    #[test]
    fn source_dir_falls_back_to_root_without_src() {
        let proj = tmp_project("root");
        fs::write(proj.join("main.fln"), "").unwrap();
        assert_eq!(source_dir(&proj), proj);

        // A `src/` that lacks `main.fln` also roots at the project.
        fs::create_dir_all(proj.join("src")).unwrap();
        assert_eq!(source_dir(&proj), proj);

        let _ = fs::remove_dir_all(&proj);
    }
}
