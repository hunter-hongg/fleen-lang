//! `flint.toml` parsing and validation (0.0.3 V08, D9).
//!
//! We deliberately support only a minimal TOML subset — no arrays, no
//! nested tables, no multi-line — because that is exactly what a `flint.toml`
//! needs and it keeps the workspace free of a third-party TOML dependency
//! (V08 ticket: "a small TOML parser crate is acceptable"; we choose the
//! zero-dependency alternative). Unsupported syntax produces an actionable
//! error instead of a corrupt parse.
//!
//! Supported surface (the documented flint.toml):
//! ```toml
//! [package]
//! name = "myproj"
//! version = "0.1.0"
//!
//! [dependencies]
//! jsonlite = { git = "https://github.com/user/jsonlite", version = "0.2" }
//! ```

use std::collections::BTreeMap;
use std::fmt;

/// One dependency declaration from `flint.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub name: String,
    pub git: String,
    /// Advisory in 0.0.3 (D9): the lockfile is the pin.
    pub version: Option<String>,
}

/// A parsed and validated `flint.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    /// Advisory in 0.0.3.
    pub version: Option<String>,
    pub dependencies: Vec<Dependency>,
}

/// A `flint.toml` problem; the message is the returned error text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestError(pub String);

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ManifestError {}

/// Parse a `flint.toml` string into a [`Manifest`].
pub fn parse_manifest(text: &str) -> Result<Manifest, ManifestError> {
    let doc = parse_doc(text)?;
    let name = doc
        .package
        .get("name")
        .ok_or_else(|| ManifestError("missing [package] name".into()))?
        .clone();
    let version = doc.package.get("version").cloned();
    let mut dependencies = Vec::new();
    for (dep_name, fields) in &doc.deps {
        let git = fields
            .get("git")
            .ok_or_else(|| {
                ManifestError(format!(
                    "dependency `{dep_name}` is missing a `git` URL \
                     (only git dependencies are supported in 0.0.3)"
                ))
            })?
            .clone();
        dependencies.push(Dependency {
            name: dep_name.clone(),
            git,
            version: fields.get("version").cloned(),
        });
    }
    let manifest = Manifest {
        name,
        version,
        dependencies,
    };
    validate(&manifest)?;
    Ok(manifest)
}

/// A dependency's pin from `Flint.lock`: the git URL plus the pinned commit
/// (0.0.3 D9 — the lockfile is the pin). Version strings are advisory and are
/// round-tripped in the file but not needed for resolution, so they are not
/// retained here.
pub struct LockEntry {
    pub git: String,
    pub commit: String,
}

/// Parse a `Flint.lock` string: same subset, `[package]` + `[dependencies]`,
/// every entry carrying a `commit`.
pub fn parse_lock(text: &str) -> Result<(String, BTreeMap<String, LockEntry>), ManifestError> {
    let doc = parse_doc(text)?;
    let name = doc.package.get("name").cloned().unwrap_or_default();
    let mut entries = BTreeMap::new();
    for (dep_name, fields) in &doc.deps {
        let git = fields
            .get("git")
            .ok_or_else(|| ManifestError(format!("lock entry `{dep_name}` missing `git`")))?
            .clone();
        let commit = fields
            .get("commit")
            .ok_or_else(|| ManifestError(format!("lock entry `{dep_name}` missing `commit`")))?
            .clone();
        entries.insert(dep_name.clone(), LockEntry { git, commit });
    }
    Ok((name, entries))
}

fn validate(manifest: &Manifest) -> Result<(), ManifestError> {
    if !is_valid_package_name(&manifest.name) {
        return Err(ManifestError(format!(
            "invalid package name `{}`: must be a single identifier \
             (lowercase letters, digits, underscores; not a reserved word)",
            manifest.name
        )));
    }
    let mut seen = std::collections::HashSet::new();
    for dep in &manifest.dependencies {
        if dep.name == "std" {
            return Err(ManifestError(
                "`std` is reserved for the shipped standard library; \
                 rename the dependency"
                    .into(),
            ));
        }
        if !is_valid_package_name(&dep.name) {
            return Err(ManifestError(format!(
                "invalid dependency name `{}`",
                dep.name
            )));
        }
        if !seen.insert(dep.name.clone()) {
            return Err(ManifestError(format!(
                "duplicate dependency `{}`",
                dep.name
            )));
        }
    }
    Ok(())
}

/// Package/import-root name validity: a single identifier matching the
/// compiler's module-root rule, minus reserved words.
pub fn is_valid_package_name(name: &str) -> bool {
    let Some(first) = name.chars().next() else {
        return false;
    };
    if !(first.is_ascii_lowercase() || first == '_') {
        return false;
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return false;
    }
    !RESERVED.contains(&name)
}

/// Fleen reserved words (SPEC §14): `import`/`struct`/`for`/`in`/`pub` and the
/// 0.0.2 ownership words. A crate root is an import target, so its name must
/// not be a reserved word.
const RESERVED: &[&str] = &[
    "import",
    "struct",
    "for",
    "in",
    "pub",
    "func",
    "const",
    "if",
    "elif",
    "else",
    "while",
    "choose",
    "when",
    "otherwise",
    "return",
    "move",
    "clone",
    "deref",
    "box",
    "ref",
    "as",
    "std",
    "Result",
    "Ok",
    "Err",
    "Range",
];

// ---------------------------------------------------------------------------
// The minimal TOML-subset parser
// ---------------------------------------------------------------------------

/// Structural parse of the supported subset. `package` holds scalar string
/// values under `[package]`; `deps` holds name → field map under
/// `[dependencies]`. Comment-stripping respects double-quoted strings.
struct Doc {
    package: BTreeMap<String, String>,
    deps: BTreeMap<String, BTreeMap<String, String>>,
}

enum Section {
    Package,
    Dependencies,
}

fn parse_doc(text: &str) -> Result<Doc, ManifestError> {
    let mut package = BTreeMap::new();
    let mut deps: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut section: Option<Section> = None;

    for (lineno, raw) in text.lines().enumerate() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            let inner = line
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .ok_or_else(|| err(lineno, "malformed table header"))?
                .trim();
            section = Some(match inner {
                "package" => Section::Package,
                "dependencies" => Section::Dependencies,
                other => {
                    return Err(err(
                        lineno,
                        &format!(
                            "unsupported table `[{other}]` (flint.toml supports \
                             `[package]` and `[dependencies]`)"
                        ),
                    ));
                }
            });
            continue;
        }

        let (key, value) =
            split_key_value(line).ok_or_else(|| err(lineno, "expected `key = value`"))?;
        match section {
            Some(Section::Package) => {
                package.insert(key.to_string(), parse_scalar(value, lineno)?);
            }
            Some(Section::Dependencies) => {
                // `name = { git = "...", version = "..." }`
                let fields = parse_inline_table(value, lineno)?;
                if deps.insert(key.to_string(), fields).is_some() {
                    return Err(err(lineno, &format!("duplicate dependency `{key}`")));
                }
            }
            None => {
                return Err(err(
                    lineno,
                    "key/value outside any table (missing `[package]` or `[dependencies]`)",
                ));
            }
        }
    }
    Ok(Doc { package, deps })
}

/// Everything from the first unquoted `#` to end-of-line.
fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' => in_string = !in_string,
            b'#' if !in_string => return &line[..i],
            _ => {}
        }
    }
    line
}

fn split_key_value(line: &str) -> Option<(&str, &str)> {
    let eq = line.find('=')?;
    let key = line[..eq].trim();
    let value = line[eq + 1..].trim();
    if key.is_empty() || value.is_empty() {
        return None;
    }
    Some((key, value))
}

/// Parse `"..."`; rejects unquoted values explicitly so a typo fails loudly.
fn parse_scalar(value: &str, lineno: usize) -> Result<String, ManifestError> {
    match unquote(value) {
        Some(s) => Ok(s.to_string()),
        None => Err(err(lineno, "expected a `\"quoted string\"` value")),
    }
}

/// Parse `{ key = "value", key2 = "value2" }`.
fn parse_inline_table(
    value: &str,
    lineno: usize,
) -> Result<BTreeMap<String, String>, ManifestError> {
    let inner = value
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or_else(|| err(lineno, "expected an inline table `{ ... }`"))?;
    let mut fields = BTreeMap::new();
    for pair in split_top_level(inner) {
        let (key, val) = split_key_value(pair.trim())
            .ok_or_else(|| err(lineno, "malformed inline-table entry"))?;
        fields.insert(key.to_string(), parse_scalar(val.trim(), lineno)?);
    }
    Ok(fields)
}

/// Split on top-level commas (the only separator flint.toml uses).
fn split_top_level(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

/// Strip surrounding double quotes and return the inner text, if well-formed.
fn unquote(s: &str) -> Option<&str> {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        Some(&s[1..s.len() - 1])
    } else {
        None
    }
}

fn err(lineno: usize, msg: &str) -> ManifestError {
    ManifestError(format!("flint.toml:{lineno}: {msg}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_documented_manifest() {
        let m = parse_manifest(
            "[package]\nname = \"myproj\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\njsonlite = { git = \"https://github.com/user/jsonlite\", version = \"0.2\" }\n",
        )
        .unwrap();
        assert_eq!(m.name, "myproj");
        assert_eq!(m.version.as_deref(), Some("0.1.0"));
        assert_eq!(m.dependencies.len(), 1);
        assert_eq!(m.dependencies[0].name, "jsonlite");
        assert_eq!(m.dependencies[0].git, "https://github.com/user/jsonlite");
    }

    #[test]
    fn missing_package_name_is_an_error() {
        let e = parse_manifest("[package]\nversion = \"0.1.0\"\n").unwrap_err();
        assert!(e.0.contains("missing [package] name"));
    }

    #[test]
    fn dependency_without_git_is_an_error() {
        let e =
            parse_manifest("[package]\nname = \"x\"\n[dependencies]\na = { version = \"1\" }\n")
                .unwrap_err();
        assert!(e.0.contains("missing a `git` URL"));
    }

    #[test]
    fn duplicate_dependency_is_an_error() {
        let e = parse_manifest(
            "[package]\nname = \"x\"\n[dependencies]\n\
             a = { git = \"u/a\" }\na = { git = \"u/a2\" }\n",
        )
        .unwrap_err();
        assert!(e.0.contains("duplicate dependency"));
    }

    #[test]
    fn std_dependency_name_is_rejected() {
        let e = parse_manifest(
            "[package]\nname = \"x\"\n[dependencies]\n\
             std = { git = \"u/std\" }\n",
        )
        .unwrap_err();
        assert!(e.0.contains("reserved"));
    }

    #[test]
    fn comments_and_quoted_hashes_are_handled() {
        // A `#` inside a quoted value is data, not a comment; a bare `#`
        // before the value strips the rest of the line.
        let m = parse_manifest(
            "# header\n[package]\nname = \"proj\"  # trailing\nversion = \"0.1.0\"\n[dependencies]\n",
        )
        .unwrap();
        assert_eq!(m.name, "proj");
        assert_eq!(m.version.as_deref(), Some("0.1.0"));
    }

    #[test]
    fn lock_round_trip() {
        let lock = "[package]\nname = \"myproj\"\n\n[dependencies]\n\
                    jsonlite = { git = \"u/jsonlite\", version = \"0.2\", commit = \"abc123\" }\n";
        let (name, entries) = parse_lock(lock).unwrap();
        assert_eq!(name, "myproj");
        let e = &entries["jsonlite"];
        assert_eq!(e.git, "u/jsonlite");
        assert_eq!(e.commit, "abc123");
    }

    #[test]
    fn invalid_package_names() {
        assert!(is_valid_package_name("myproj"));
        assert!(is_valid_package_name("jsonlite"));
        assert!(!is_valid_package_name(""));
        assert!(!is_valid_package_name("MyProj"));
        assert!(!is_valid_package_name("my_proj-x"));
        assert!(!is_valid_package_name("import"));
        assert!(!is_valid_package_name("std"));
    }
}
