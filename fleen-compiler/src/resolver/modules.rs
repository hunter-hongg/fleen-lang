//! Module table for 0.0.3 V05.
//!
//! The module table is a sub-pass of name resolution (not an independent
//! phase): before per-module resolution we discover which files participate
//! in the program, what each imports, and what each exports. The actual
//! file-walking logic lives in `session`/driver code; this module only
//! declares the table shape and the helper functions the resolver needs.

use crate::lexer::Span;
use crate::parser::ast::{self, Ast};
use crate::resolver::error::{ResolveError, ResolveErrorKind};
use crate::source_map::{FileId, SourceMap};
use std::collections::HashMap;

/// Index into [`ModuleTable::modules`].
pub type ModuleId = usize;

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

/// A single module: one-file/one-module, with imports and the set of
/// top-level items used for `pub` visibility checks.
#[derive(Debug, Clone)]
pub struct ModuleDef {
    pub file: FileId,
    /// Dot-separated module path components (e.g. `["net", "tcp"]`).
    pub path: Vec<String>,
    /// Whether this is the crate root (`src/main.fln`). The root keeps
    /// unprefixed names so existing single-file programs and the `main`
    /// entry-point convention are unchanged.
    pub is_root: bool,
    /// Import declarations found in this module.
    pub imports: Vec<ast::ImportDecl>,
    /// All top-level declarations by unqualified name (private and pub).
    pub items: HashMap<String, ExportDef>,
}

impl ModuleDef {
    /// Qualified name prefix for top-level symbols defined in this module.
    /// The crate root has no prefix.
    pub fn mangled_name(&self, local_name: &str) -> String {
        if self.is_root {
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
    pub modules: Vec<ModuleDef>,
}

impl ModuleTable {
    pub fn root(&self) -> ModuleId {
        0
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

/// Derive the module path for `file_name` relative to the crate-root file.
///
/// Examples (root name `src/main.fln`):
/// - `src/main.fln` → `[]` (crate root)
/// - `src/net/tcp.fln` → `["net", "tcp"]`
/// - `src/net/mod.fln` → `["net"]`
pub(crate) fn module_path_from_name(root_name: &str, file_name: &str) -> Option<Vec<String>> {
    fn norm(s: &str) -> String {
        s.replace('\\', "/")
    }
    let root = norm(root_name);
    let file = norm(file_name);
    if file == root {
        return Some(Vec::new());
    }
    let root_basedir = root.rsplit_once('/').map_or("", |(d, _)| d);
    let root_prefix = if root_basedir.is_empty() {
        String::new()
    } else {
        format!("{root_basedir}/")
    };
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

/// Build the module table for a whole-program compilation (0.0.3 V05).
///
/// `asts` are the already-parsed files, indexed by `FileId` (same order as
/// `sources`). The table maps module paths → definitions, validates import
/// targets and reports missing modules / circular imports.
pub(crate) fn build_module_table(
    root: FileId,
    sources: &SourceMap,
    asts: &[Ast],
) -> Result<ModuleTable, Vec<ResolveError>> {
    let mut table = ModuleTable::default();
    let mut errors = Vec::new();
    let root_name = sources.get(root).name.clone();

    // The crate root is always module 0 with the empty path.
    table.modules.push(ModuleDef {
        file: root,
        path: Vec::new(),
        is_root: true,
        imports: imports_of_ast(&asts[root.0 as usize]),
        items: exports_of_ast(&asts[root.0 as usize]),
    });

    let mut by_path: HashMap<Vec<String>, ModuleId> = HashMap::new();
    by_path.insert(Vec::new(), 0);

    for (file_idx, ast) in asts.iter().enumerate() {
        let file = FileId(file_idx as u32);
        if file == root {
            continue;
        }
        let name = &sources.get(file).name;
        let Some(path) = module_path_from_name(&root_name, name) else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        let id = table.modules.len();
        by_path.insert(path.clone(), id);
        table.modules.push(ModuleDef {
            file,
            path: path.clone(),
            is_root: false,
            imports: imports_of_ast(ast),
            items: exports_of_ast(ast),
        });
    }

    // Validate import targets: every non-`std` import must name a module in
    // the table. `std.*` is the shipped prelude/std root (empty until V06).
    for module_id in 0..table.modules.len() {
        for import in &table.modules[module_id].imports {
            if import.path.first().is_some_and(|p| p == "std") {
                continue;
            }
            if !by_path.contains_key(&import.path) {
                let tried = tried_paths(&root_name, &import.path);
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

    // Cycle detection: root BFS/DFS over the import graph. `std` edges are
    // skipped (they belong to the shipped prelude, not the user graph).
    compute_import_cycles(&table, &by_path, &mut errors);

    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(table)
}

/// Render the two canonical file candidates in the `ModuleNotFound` help.
fn tried_paths(root_name: &str, path: &[String]) -> Vec<String> {
    let joined = path.join("/");
    let prefix = root_dir_prefix(root_name);
    vec![
        format!("{prefix}{joined}.fln"),
        format!("{prefix}{joined}/mod.fln"),
    ]
}

/// Extract the directory prefix from a root file name as an owned `String`.
///
/// For `src/main.fln` → `src/`. For `main.fln` → `` (empty).
/// This mirrors the prefix logic in `module_path_from_name` so that
/// `ModuleNotFound` suggestions match the project layout.
fn root_dir_prefix(root_name: &str) -> String {
    let normalized = root_name.replace('\\', "/");
    match normalized.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/"),
        None => String::new(),
    }
}

/// Color-based DFS cycle detection over module imports (`a → b → a`).
fn compute_import_cycles(
    table: &ModuleTable,
    by_path: &HashMap<Vec<String>, ModuleId>,
    errors: &mut Vec<ResolveError>,
) {
    let n = table.modules.len();
    let mut color = vec![0u8; n];
    let mut stack: Vec<ModuleId> = Vec::new();
    visit_cycle_dfs(table, by_path, 0, &mut color, &mut stack, errors);
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
        if import.path.first().is_some_and(|p| p == "std") {
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
        let res = build_module_table(root, &sm, &asts);
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
    fn build_module_table_detects_cycle() {
        let mut sm = SourceMap::new();
        let root = sm.add_file("src/main.fln", "import net.a;\nfunc main(): int = 0\n");
        let a = sm.add_file("src/net/a.fln", "import net.b;\npub func af(): int = 0\n");
        let b = sm.add_file("src/net/b.fln", "import net.a;\npub func bf(): int = 0\n");
        let mut asts = Vec::new();
        for id in [root, a, b] {
            asts.push(parse(id, &sm.get(id).source));
        }
        let errs = build_module_table(root, &sm, &asts).expect_err("cycle must fail");
        assert!(matches!(
            &errs[0].kind,
            ResolveErrorKind::CircularImport { cycle }
                if cycle == &vec!["net.a".to_string(), "net.b".to_string(), "net.a".to_string()]
        ));
    }
}
