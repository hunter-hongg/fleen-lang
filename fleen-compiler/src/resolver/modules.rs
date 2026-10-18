//! Module table for 0.0.3 V05, generalized to multiple crates.
//!
//! The module table is a sub-pass of name resolution (not an independent
//! phase): before per-module resolution we discover which files participate
//! in the program, which crate each belongs to, what each imports, and what
//! each exports. The actual file-walking logic lives in `session`/driver
//! code; this module only declares the table shape and the helper functions
//! the resolver needs.
//!
//! # Crates
//!
//! A program is a graph of *crates*. The crate being compiled is the **root
//! crate**: its module paths are unprefixed (`net.tcp`), which keeps existing
//! single-file programs and the `main` entry-point convention unchanged. A
//! dependency crate is registered under its package name, so its modules
//! live under `<crate-name>.*` (`jsonlite.util`). Paths are absolute across
//! the whole table — a crate boundary is a namespace boundary, so a
//! dependency can never reach into the depending project.

use crate::lexer::Span;
use crate::parser::ast::{self, Ast};
use crate::resolver::error::{ResolveError, ResolveErrorKind};
use crate::source_map::{FileId, SourceMap};
use std::collections::{HashMap, HashSet};

/// Index into [`ModuleTable::modules`].
pub type ModuleId = usize;

/// Index into [`ModuleTable::crates`].
pub type CrateId = usize;

/// Index of the crate under compilation in every [`ModuleTable`].
pub const ROOT_CRATE: CrateId = 0;

/// The reserved import root for the shipped `std` library.
pub const STD_CRATE_NAME: &str = "std";

/// Top-level item classification for visibility checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    Func,
    Const,
    Global,
    Struct,
}

/// One top-level item in a module, with its visibility.
#[derive(Debug, Clone)]
pub struct ExportDef {
    pub name: String,
    pub is_pub: bool,
    pub span: Span,
    pub kind: ExportKind,
}

/// One crate participating in a compilation.
///
/// The root crate (id [`ROOT_CRATE`]) has an empty `name`: its modules keep
/// unprefixed paths. A dependency crate uses its package name, which becomes
/// the first component of every module path it contributes.
#[derive(Debug, Clone)]
pub struct CrateDef {
    /// Import-root name; empty for the root crate.
    pub name: String,
    /// The crate's root file (`src/main.fln`, or a dependency's own root).
    pub root: FileId,
}

impl CrateDef {
    /// The root crate: unprefixed paths and the `main` entry point.
    pub fn root_crate(root: FileId) -> Self {
        CrateDef {
            name: String::new(),
            root,
        }
    }

    /// A dependency crate addressed as `<name>.*`.
    pub fn dependency(name: impl Into<String>, root: FileId) -> Self {
        CrateDef {
            name: name.into(),
            root,
        }
    }

    /// Directory prefix shared by every file of this crate, derived from the
    /// crate root's file name (`src/main.fln` → `src/`).
    pub fn source_dir(&self, sources: &SourceMap) -> String {
        dir_prefix(&sources.get(self.root).name)
    }
}

/// A single module: one-file/one-module, with imports and the set of
/// top-level items used for `pub` visibility checks.
#[derive(Debug, Clone)]
pub struct ModuleDef {
    pub file: FileId,
    /// Absolute dot-separated module path (e.g. `["net", "tcp"]` in the root
    /// crate, `["jsonlite", "util"]` in a dependency). Always includes the
    /// owning crate's name unless that crate is the root crate.
    pub path: Vec<String>,
    /// Which crate this module belongs to.
    pub crate_id: CrateId,
    /// Whether this file is its crate's root file (`src/main.fln`).
    pub is_crate_root: bool,
    /// Import declarations found in this module.
    pub imports: Vec<ast::ImportDecl>,
    /// All top-level declarations by unqualified name (private and pub).
    pub items: HashMap<String, ExportDef>,
}

impl ModuleDef {
    /// Qualified name prefix for top-level symbols defined in this module.
    ///
    /// Only the root crate's root module is unprefixed — that is what keeps
    /// `main` and single-file top-level names working. A dependency crate's
    /// own root module is prefixed like any other (`jsonlite.helper`).
    pub fn mangled_name(&self, local_name: &str) -> String {
        if self.crate_id == ROOT_CRATE && self.is_crate_root {
            local_name.to_string()
        } else {
            format!("{}.{}", self.path.join("."), local_name)
        }
    }

    /// Module path as a display string (`net.tcp`).
    pub fn path_string(&self) -> String {
        self.path.join(".")
    }
}

/// Complete module graph for one compilation.
#[derive(Debug, Clone, Default)]
pub struct ModuleTable {
    /// Every crate in the compilation; index [`ROOT_CRATE`] is the crate
    /// under compilation and is always present.
    pub crates: Vec<CrateDef>,
    pub modules: Vec<ModuleDef>,
}

impl ModuleTable {
    pub fn root(&self) -> ModuleId {
        0
    }

    /// The crate with the given import-root name, if any. The root crate is
    /// matched by its empty name.
    pub fn crate_by_name(&self, name: &str) -> Option<CrateId> {
        self.crates.iter().position(|c| c.name == name)
    }

    /// Module id whose dotted path exactly matches `path`, if any.
    pub fn find(&self, path: &[String]) -> Option<ModuleId> {
        self.modules.iter().position(|m| m.path == path)
    }

    /// Longest prefix of `path` that names an existing module.
    pub fn longest_module_prefix(&self, path: &[String]) -> Option<(ModuleId, usize)> {
        let mut best: Option<(ModuleId, usize)> = None;
        for (i, m) in self.modules.iter().enumerate() {
            if m.path.len() <= path.len()
                && path[..m.path.len()] == m.path[..]
                && best.is_none_or(|(_, best_len)| m.path.len() > best_len)
            {
                best = Some((i, m.path.len()));
            }
        }
        best
    }
}

/// Collect the top-level item map for one parsed module.
pub(crate) fn exports_of_ast(ast: &Ast) -> HashMap<String, ExportDef> {
    let mut items = HashMap::new();
    for item in &ast.items {
        match item {
            ast::Item::Import(_) => {}
            ast::Item::Expr(_) => {}
            ast::Item::Decl(decl) => {
                let (name, is_pub, span, kind) = match decl {
                    ast::Decl::Func(f) => (f.name.clone(), f.is_pub, f.span, ExportKind::Func),
                    ast::Decl::Const(c) => (c.name.clone(), c.is_pub, c.span, ExportKind::Const),
                    ast::Decl::Var(v) => (v.name.clone(), v.is_pub, v.span, ExportKind::Global),
                    ast::Decl::Struct(s) => (s.name.clone(), s.is_pub, s.span, ExportKind::Struct),
                };
                items.insert(
                    name.clone(),
                    ExportDef {
                        name,
                        is_pub,
                        span,
                        kind,
                    },
                );
            }
        }
    }
    items
}

/// Collect import paths for one parsed module.
pub(crate) fn imports_of_ast(ast: &Ast) -> Vec<ast::ImportDecl> {
    ast.items
        .iter()
        .filter_map(|item| match item {
            ast::Item::Import(import) => Some(import.clone()),
            _ => None,
        })
        .collect()
}

/// Extract the directory prefix from a root file name, normalized to `/`.
///
/// `src/main.fln` → `src/`; `main.fln` → ``.
fn dir_prefix(root_name: &str) -> String {
    let normalized = root_name.replace('\\', "/");
    match normalized.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/"),
        None => String::new(),
    }
}

/// Derive a crate-relative module path for `file_name`, i.e. the path
/// components below the crate's source directory.
///
/// This is the *crate-local* path; [`crate_module_path`] turns it into the
/// absolute path the module table stores.
///
/// Examples (crate root `src/main.fln`, source dir `src/`):
/// - `src/main.fln` → `[]` (crate root)
/// - `src/net/tcp.fln` → `["net", "tcp"]`
/// - `src/net/mod.fln` → `["net"]`
pub(crate) fn module_path_from_name(root_name: &str, file_name: &str) -> Option<Vec<String>> {
    let root = root_name.replace('\\', "/");
    let file = file_name.replace('\\', "/");
    if file == root {
        return Some(Vec::new());
    }
    let root_prefix = dir_prefix(&root);
    let rel = file.strip_prefix(&root_prefix).unwrap_or(&file);
    if root_prefix.is_empty() && rel == root {
        return Some(Vec::new());
    }
    if !rel.ends_with(".fln") {
        return None;
    }
    let stem = &rel[..rel.len() - 4];
    let mut parts: Vec<String> = stem
        .split('/')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    if parts.last().is_some_and(|p| p == "mod") {
        parts.pop();
    }
    if parts.is_empty() {
        return Some(Vec::new());
    }
    Some(parts)
}

/// Prefix a crate-local path with the crate's import-root name.
///
/// The root crate has an empty name, so its modules stay unprefixed; a
/// dependency named `jsonlite` turns `["util"]` into `["jsonlite", "util"]`.
fn crate_module_path(crate_name: &str, local: &[String]) -> Vec<String> {
    let mut path = Vec::with_capacity(local.len() + 1);
    if !crate_name.is_empty() {
        path.push(crate_name.to_string());
    }
    path.extend_from_slice(local);
    path
}

/// Build the module table for a whole-program compilation (0.0.3 V05,
/// generalized to a dependency graph).
///
/// `asts` are the already-parsed files, indexed by `FileId` (same order as
/// `sources`). `crates` must have the root crate at [`ROOT_CRATE`]; every
/// file is assigned to the crate whose source directory contains it, and the
/// resulting paths are absolute (dependency modules carry their crate name).
///
/// The table maps module paths → definitions, then validates import targets,
/// crate boundaries and the import graph (cycles).
pub(crate) fn build_module_table(
    crates: &[CrateDef],
    sources: &SourceMap,
    asts: &[Ast],
) -> Result<ModuleTable, Vec<ResolveError>> {
    let mut table = ModuleTable {
        crates: crates.to_vec(),
        modules: Vec::new(),
    };
    let mut errors = Vec::new();
    debug_assert!(
        table.crate_by_name("").is_some(),
        "the root crate must be registered so its modules stay unprefixed"
    );

    // Module order is part of the compilation contract: it fixes the binding
    // and function ID spaces that the merged HIR is built from, so it must
    // not depend on the order files happened to be registered in. The root
    // crate's root file is always module 0 (see [`ModuleTable::root`]); the
    // rest follow in registration order, which is how every existing project
    // has always been laid out.
    let mut by_path: HashMap<Vec<String>, ModuleId> = HashMap::new();
    let mut ordered: Vec<FileId> = Vec::with_capacity(asts.len());
    ordered.push(crates[ROOT_CRATE].root);
    ordered.extend(
        (0..asts.len() as u32)
            .map(FileId)
            .filter(|f| *f != crates[ROOT_CRATE].root),
    );

    for file in ordered {
        let ast = &asts[file.0 as usize];
        let Some((crate_id, local_path)) = crate_of_file(crates, sources, file) else {
            // A file outside every crate's source directory takes part in no
            // module; nothing to register.
            continue;
        };
        let is_crate_root = file == crates[crate_id].root;
        let path = crate_module_path(&crates[crate_id].name, &local_path);
        let id = table.modules.len();
        if is_crate_root && crate_id == ROOT_CRATE {
            debug_assert!(path.is_empty(), "the root crate root has no prefix");
        }
        if let Some(previous) = by_path.insert(path.clone(), id) {
            // Restore the original owner: the losing module is never pushed,
            // so leaving our id in the map would hand out a dangling
            // `ModuleId` to every later lookup.
            by_path.insert(path.clone(), previous);
            let first = sources.get(table.modules[previous].file).name.clone();
            errors.push(ResolveError::new(
                ResolveErrorKind::DuplicateModule {
                    path: path.join("."),
                    file: sources.get(file).name.clone(),
                    first,
                },
                ast.span,
            ));
            continue;
        }
        table.modules.push(ModuleDef {
            file,
            path: path.clone(),
            crate_id,
            is_crate_root,
            imports: imports_of_ast(ast),
            items: exports_of_ast(ast),
        });
    }
    debug_assert!(
        table
            .modules
            .first()
            .is_some_and(|m| m.is_crate_root && m.crate_id == ROOT_CRATE),
        "module 0 must be the root crate's root file"
    );

    validate_imports(&table, &by_path, sources, &mut errors);
    compute_import_cycles(&table, &by_path, &mut errors);

    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(table)
}

/// Which crate owns `file`, and the crate-local module path within it.
///
/// A file belongs to the crate whose source directory is the longest prefix
/// of its name — so a dependency cached at
/// `/home/u/.flint/cache/jsonlite-abc/src/util.fln` belongs to the crate
/// rooted at `/home/u/.flint/cache/jsonlite-abc/src/main.fln`, not to a
/// project whose root happens to be `src/main.fln`. Ties (two crates with
/// the same source directory) resolve to the first registered crate, and the
/// resulting path collision is reported by the caller.
fn crate_of_file(
    crates: &[CrateDef],
    sources: &SourceMap,
    file: FileId,
) -> Option<(CrateId, Vec<String>)> {
    let file_name = sources.get(file).name.replace('\\', "/");
    // (crate id, local path, length of the matching source-directory prefix)
    let mut best: Option<(CrateId, Vec<String>, usize)> = None;
    for (crate_id, krate) in crates.iter().enumerate() {
        let root_name = sources.get(krate.root).name.replace('\\', "/");
        if root_name == file_name {
            // The crate's own root file: shortest possible path.
            return Some((crate_id, Vec::new()));
        }
        let prefix = dir_prefix(&root_name);
        let Some(rel) = file_name.strip_prefix(&prefix) else {
            continue;
        };
        // Only claim `.fln` files that live below this crate's directory.
        if !rel.ends_with(".fln") {
            continue;
        }
        let Some(local) = module_path_from_name(&root_name, &file_name) else {
            continue;
        };
        // Longest source-directory prefix wins, so a crate nested inside
        // another's tree shadows the outer one.
        if best
            .as_ref()
            .is_none_or(|(_, _, best_len)| prefix.len() > *best_len)
        {
            best = Some((crate_id, local, prefix.len()));
        }
    }
    best.map(|(crate_id, local, _)| (crate_id, local))
}

/// Validate every import in the table against the module namespace.
///
/// Two rules:
/// - an import must name a module that exists (`std.*` is always resolvable,
///   since the shipped library is appended on demand);
/// - a crate may not import a crate that **already depends on it**. That
///   single rule covers both directions the ticket forbids — a dependency
///   importing the depending project, and two dependencies importing each
///   other — while still letting the root crate import its dependencies and
///   a dependency import *its own* dependencies.
///
/// Whether `a` may import `b` at all (i.e. whether `b` is a declared
/// dependency of `a`) is a property of the manifest, so it belongs to the
/// build tool; the compiler only enforces what the source itself says.
fn validate_imports(
    table: &ModuleTable,
    by_path: &HashMap<Vec<String>, ModuleId>,
    sources: &SourceMap,
    errors: &mut Vec<ResolveError>,
) {
    // Crate-level dependency graph, derived from the module imports.
    let crate_deps = crate_dependency_graph(table, by_path);
    // Every cross-crate edge, deduplicated by (importer, owner) so a mutual
    // dependency is reported once rather than once per edge.
    let mut reported: HashSet<(CrateId, CrateId)> = HashSet::new();

    for module in &table.modules {
        for import in &module.imports {
            let Some(first) = import.path.first() else {
                continue;
            };
            if first == STD_CRATE_NAME {
                continue;
            }
            match by_path.get(&import.path) {
                Some(&target) => {
                    let importer_crate = module.crate_id;
                    let owner_crate = table.modules[target].crate_id;
                    if importer_crate == owner_crate {
                        continue;
                    }
                    // `std` is appended on demand and never imports back.
                    if owner_crate == table.crate_by_name(STD_CRATE_NAME).unwrap_or(usize::MAX) {
                        continue;
                    }
                    if crate_reaches(&crate_deps, owner_crate, importer_crate)
                        && reported.insert((importer_crate, owner_crate))
                    {
                        errors.push(ResolveError::new(
                            ResolveErrorKind::CrossCrateImport {
                                path: import.path.join("."),
                                crate_name: table.crates[importer_crate].name.clone(),
                                owner: table.crates[owner_crate].name.clone(),
                            },
                            import.span,
                        ));
                    }
                }
                None => {
                    // The path names a module that does not exist. Suggest
                    // where it would have to live: inside the crate the
                    // *first component* names, which is the dependency for
                    // `jsonlite.nosuch`, and the importing crate when the
                    // path is local to it.
                    let owner = table.crate_by_name(first).unwrap_or(module.crate_id);
                    let tried = tried_paths(sources, table, owner, &import.path);
                    errors.push(ResolveError::new(
                        ResolveErrorKind::ModuleNotFound {
                            path: import.path.join("."),
                            tried,
                        },
                        import.span,
                    ));
                }
            }
        }
    }
}

/// Crate-level dependency edges: `deps[a]` lists the crates `a` imports from.
fn crate_dependency_graph(
    table: &ModuleTable,
    by_path: &HashMap<Vec<String>, ModuleId>,
) -> Vec<Vec<CrateId>> {
    let mut deps: Vec<Vec<CrateId>> = vec![Vec::new(); table.crates.len()];
    for module in &table.modules {
        for import in &module.imports {
            let Some(&target) = by_path.get(&import.path) else {
                continue;
            };
            let owner = table.modules[target].crate_id;
            if owner != module.crate_id && !deps[module.crate_id].contains(&owner) {
                deps[module.crate_id].push(owner);
            }
        }
    }
    deps
}

/// Whether crate `from` can reach crate `to` by following dependency edges.
fn crate_reaches(deps: &[Vec<CrateId>], from: CrateId, to: CrateId) -> bool {
    let mut seen = vec![false; deps.len()];
    let mut stack = vec![from];
    while let Some(id) = stack.pop() {
        if id == to {
            return true;
        }
        if std::mem::replace(&mut seen[id], true) {
            continue;
        }
        stack.extend(deps[id].iter().copied());
    }
    false
}

/// Render the two canonical file candidates in the `ModuleNotFound` help.
fn tried_paths(
    sources: &SourceMap,
    table: &ModuleTable,
    crate_id: CrateId,
    path: &[String],
) -> Vec<String> {
    let prefix = table.crates[crate_id].source_dir(sources);
    // Paths inside a dependency crate are written relative to its root, so
    // strip the crate name back off before suggesting a file location.
    let rel: &[String] = if crate_id == ROOT_CRATE {
        path
    } else {
        path.get(1..).unwrap_or(&[])
    };
    let joined = rel.join("/");
    vec![
        format!("{prefix}{joined}.fln"),
        format!("{prefix}{joined}/mod.fln"),
    ]
}

/// Color-based DFS cycle detection over module imports (`a → b → a`).
///
/// Every module is a DFS root, not just the crate root: with a dependency
/// graph, a crate that is never imported by the project still has to be
/// acyclic on its own. `std` edges are skipped — they belong to the shipped
/// library, not the program's own graph.
fn compute_import_cycles(
    table: &ModuleTable,
    by_path: &HashMap<Vec<String>, ModuleId>,
    errors: &mut Vec<ResolveError>,
) {
    let n = table.modules.len();
    let mut color = vec![0u8; n];
    let mut stack: Vec<ModuleId> = Vec::new();
    for start in 0..n {
        if color[start] == 0 {
            visit_cycle_dfs(table, by_path, start, &mut color, &mut stack, errors);
        }
    }
}

fn visit_cycle_dfs(
    table: &ModuleTable,
    by_path: &HashMap<Vec<String>, ModuleId>,
    id: ModuleId,
    color: &mut [u8],
    stack: &mut Vec<ModuleId>,
    errors: &mut Vec<ResolveError>,
) {
    if color[id] == 1 {
        // Close the cycle from the first repeat of `id`.
        let start = stack.iter().position(|&m| m == id).unwrap_or(0);
        let mut cycle: Vec<String> = stack[start..]
            .iter()
            .map(|&m| table.modules[m].path_string())
            .collect();
        cycle.push(table.modules[id].path_string());
        let span = table.modules[id]
            .imports
            .first()
            .map(|i| i.span)
            .unwrap_or(Span::SYNTHETIC);
        errors.push(ResolveError::new(
            ResolveErrorKind::CircularImport { cycle },
            span,
        ));
        return;
    }
    if color[id] == 2 {
        return;
    }
    color[id] = 1;
    stack.push(id);
    for import in &table.modules[id].imports {
        if import.path.first().is_some_and(|p| p == STD_CRATE_NAME) {
            continue;
        }
        if let Some(&next) = by_path.get(&import.path) {
            visit_cycle_dfs(table, by_path, next, color, stack, errors);
        }
    }
    stack.pop();
    color[id] = 2;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize_in_file;
    use crate::parser;
    use crate::resolver::error::ResolveErrorKind;
    use crate::source_map::SourceMap;

    fn parse(file: crate::source_map::FileId, source: &str) -> Ast {
        let tokens = tokenize_in_file(file, source).unwrap();
        parser::parse(tokens).unwrap()
    }

    #[test]
    fn module_path_from_name_maps_src_tree() {
        assert_eq!(
            module_path_from_name("src/main.fln", "src/main.fln"),
            Some(vec![])
        );
        assert_eq!(
            module_path_from_name("src/main.fln", "src/net/tcp.fln"),
            Some(vec!["net".into(), "tcp".into()])
        );
        assert_eq!(
            module_path_from_name("src/main.fln", "src/net/mod.fln"),
            Some(vec!["net".into()])
        );
        assert_eq!(
            module_path_from_name("main.fln", "net/tcp.fln"),
            Some(vec!["net".into(), "tcp".into()])
        );
    }

    #[test]
    fn build_module_table_rejects_missing_module() {
        let mut sm = SourceMap::new();
        let root = sm.add_file("src/main.fln", "import net.tcp;\n");
        let asts = vec![parse(root, &sm.get(root).source)];
        let crates = [CrateDef::root_crate(root)];
        let res = build_module_table(&crates, &sm, &asts);
        let errs = res.expect_err("missing module must fail");
        assert!(matches!(
            &errs[0].kind,
            ResolveErrorKind::ModuleNotFound { path, tried }
                if path == "net.tcp"
                    && tried
                        == &vec![
                            "src/net/tcp.fln".to_string(),
                            "src/net/tcp/mod.fln".to_string(),
                        ]
        ));
    }

    #[test]
    fn crate_of_file_uses_longest_source_prefix() {
        let mut sm = SourceMap::new();
        // Root project crate + a vendored dependency cached under the home dir.
        let proj_root = sm.add_file("proj/src/main.fln", "");
        let dep_root = sm.add_file("/home/u/.flint/jsonlite-abc/src/main.fln", "");
        let crates = [
            CrateDef::root_crate(proj_root),
            CrateDef::dependency("jsonlite", dep_root),
        ];

        // A file under the dependency's source dir belongs to the dependency,
        // even though its filename also ends in `.fln` under a `src/` dir.
        let dep_util = sm.add_file("/home/u/.flint/jsonlite-abc/src/util.fln", "");
        let (crate_id, local) = crate_of_file(&crates, &sm, dep_util).expect("dep-owned");
        assert_eq!(crate_id, 1, "jsonlite owns its cached util.fln");
        assert_eq!(local, vec!["util".to_string()]);

        // The project's own root maps to the root crate with an empty path.
        let (crate_id, local) = crate_of_file(&crates, &sm, proj_root).expect("root-owned");
        assert_eq!(crate_id, 0);
        assert!(local.is_empty());

        // A file outside every crate's source directory is unclaimed.
        let stray = sm.add_file("unrelated/x.fln", "");
        assert!(
            crate_of_file(&crates, &sm, stray).is_none(),
            "file outside all crate source dirs is unclaimed"
        );
    }

    #[test]
    fn build_module_table_detects_cycle() {
        let mut sm = SourceMap::new();
        let root = sm.add_file("src/main.fln", "import net.a;\nfunc main(): int = 0\n");
        let a = sm.add_file("src/net/a.fln", "import net.b;\npub func af(): int = 0\n");
        let b = sm.add_file("src/net/b.fln", "import net.a;\npub func bf(): int = 0\n");
        let mut asts = Vec::new();
        for id in [root, a, b] {
            asts.push(parse(id, &sm.get(id).source));
        }
        let crates = [CrateDef::root_crate(root)];
        let errs = build_module_table(&crates, &sm, &asts).expect_err("cycle must fail");
        assert!(matches!(
            &errs[0].kind,
            ResolveErrorKind::CircularImport { cycle }
                if cycle == &vec!["net.a".to_string(), "net.b".to_string(), "net.a".to_string()]
        ));
    }
}
