//! Resolver module: name resolution and scope checking.
//!
//! Transforms AST → HIR (High-level IR) with resolved names and scope information.
//!
//! Key semantics (per `DESIGN.md`):
//! - **Binding vs assignment** (§3.1): in statement position, `x = expr` binds a
//!   new name if the name is unbound at this position, and assigns to the
//!   existing binding if the name is already bound here.
//! - **Shadowing** (§4.3): inside a block, `x = expr` is a *new binding* that
//!   shadows the outer `x` (if mutability matches); it never assigns to the outer one.
//! - **Loop bodies** (§4.5): inside a `while` body (including nested blocks, up
//!   to the function boundary), `x = expr` *assigns* to an existing outer
//!   mutable binding instead of shadowing it — the counter idiom
//!   `while c { x = x + 1; }` updates the outer variable.
//! - **Expression position**: `x = expr` is *assignment only*; it never binds.
//!   Targets follow the same rules, so it never writes through a function boundary.
//! - **Function scope** (§4.2/§4.3): the function body is independent; declarations
//!   inside it do not shadow-check against enclosing scopes ("functions have no effect").
//! - **Forward references**: function names are collected before bodies are
//!   resolved, so a function may be called before its declaration. Variables cannot.

pub mod error;
pub mod hir;
pub mod modules;
pub mod scope;

use crate::lexer::Span;
use crate::parser::ast::*;
use error::{ResolveError, ResolveErrorKind};
use hir::{
    BindingId, BlockHir, ChooseArmHir, ConstDeclHir, DeclHir, ExprChooseHir, ExprForHir, ExprHir,
    ExprIfHir, ExprWhileHir, FuncBodyHir, FuncDeclHir, Hir, HirId, HirItem, ImportDeclHir,
    ParamHir, PatternHir, StmtHir, StructDeclHir, StructFieldHir, VarBindingHir,
};
pub use modules::{ExportKind, ModuleDef, ModuleId, ModuleTable};
use scope::{Binding, BindingKind, ScopeKind, ScopeStack};
use std::collections::HashMap;

/// Builtin functions available in every program (registered in the global scope).
///
/// `print` is the only builtin *function*. `Ok`/`Err` are builtin Result
/// constructors: they resolve like functions here (shadowable, §4.3), while
/// typeck gives them their context-dependent semantics (0.0.2 U04, PLAN §3.4.1).
const BUILTINS: &[&str] = &["print", "Ok", "Err", "range", "range_from"];

/// Whether a type mentions `ref` anywhere in its structure (0.0.2 U05):
/// `ref T` is legal only as the *outer* type of a function parameter
/// (DESIGN §10.4), and never nested (`ref ref T`, `ref` inside composite
/// types).
fn type_contains_ref(ty: &Type) -> bool {
    match ty {
        Type::Ref(_) => true,
        Type::Array(el) | Type::Box(el) => type_contains_ref(el),
        Type::Result(ok, err) => type_contains_ref(ok) || type_contains_ref(err),
        Type::Func(params, ret) => params.iter().any(type_contains_ref) || type_contains_ref(ret),
        Type::Range => false,
        Type::Base(_) => false,
        Type::Named(_) => false,
    }
}

/// Assignment target shape, split out of the AST before binding lookup.
enum AssignTarget {
    Ident(String),
    /// `deref b` — write the pointee of the box held by `b`.
    Deref(String),
    /// `p.f` — write field `f` of the struct held by `p` (0.0.3 V02, one
    /// level; deeper paths fall into the generic invalid-target case).
    Field {
        obj: String,
        field: String,
    },
    /// `a[i]` — write element `i` of the array held by `a` (0.0.3 V03, one
    /// level: `a` must be a bare binding; `m[i][j]`/`p.f[i]` fall into the
    /// generic invalid-target case). Bounds are a runtime concern.
    Index {
        obj: String,
        index: Box<ExprHir>,
    },
}

/// Resolve names in an AST, producing a HIR.
///
/// # Arguments
/// - `ast`: The parsed AST from the parser.
///
/// # Returns
/// - `Ok(Hir)`: Resolution successful, all names bound.
/// - `Err(Vec<ResolveError>)`: Resolution errors (collected, not fail-fast).
pub fn resolve(ast: Ast) -> Result<Hir, Vec<ResolveError>> {
    let mut table = ModuleTable::default();
    // Single-file `resolve` is still a whole program: the crate root module
    // exists even though no import graph was discovered.
    table.modules.push(ModuleDef {
        file: crate::source_map::FileId(0),
        path: Vec::new(),
        is_root: true,
        imports: Vec::new(),
        items: std::collections::HashMap::new(),
    });
    Resolver::new_for_module(&table, 0, 0, 0).resolve_program(ast)
}

/// Resolve one module of a whole-program compilation (0.0.3 V05).
///
/// `binding_offset`/`hir_offset` make IDs unique across modules so a later
/// merged HIR (typeck/lower/codegen) never confuses two modules' bindings.
pub fn resolve_module_with_table(
    ast: Ast,
    table: &ModuleTable,
    module_id: ModuleId,
    binding_offset: u32,
    hir_offset: u32,
) -> Result<(Hir, u32, u32), Vec<ResolveError>> {
    Resolver::new_for_module(table, module_id, binding_offset, hir_offset)
        .resolve_program_inner(ast)
}

/// Main resolver struct holding state during resolution.
struct Resolver {
    /// Scope stack for variable/function bindings.
    scopes: ScopeStack,
    /// Collected errors (non-fatal, continue resolving).
    errors: Vec<ResolveError>,
    /// Counter for generating unique HIR node IDs.
    hir_id_counter: u32,
    /// Counter for generating unique binding IDs.
    binding_id_counter: u32,
    /// Top-level functions collected in the forward-reference pass, mapped to
    /// their `BindingId`. Used to avoid double-declaring them in the main pass.
    global_funcs: HashMap<String, BindingId>,
    /// Module table and the module currently being resolved (0.0.3 V05).
    table: ModuleTable,
    cur_module: usize,
}

impl Resolver {
    fn new_for_module(
        table: &ModuleTable,
        module_id: usize,
        binding_offset: u32,
        hir_offset: u32,
    ) -> Self {
        Self {
            scopes: ScopeStack::new(),
            errors: Vec::new(),
            hir_id_counter: hir_offset,
            binding_id_counter: binding_offset,
            global_funcs: HashMap::new(),
            table: table.clone(),
            cur_module: module_id,
        }
    }

    fn next_hir_id(&mut self) -> HirId {
        let id = HirId(self.hir_id_counter);
        self.hir_id_counter += 1;
        id
    }

    fn next_binding_id(&mut self) -> BindingId {
        let id = BindingId(self.binding_id_counter);
        self.binding_id_counter += 1;
        id
    }

    fn add_error(&mut self, kind: ResolveErrorKind, span: Span) {
        self.errors.push(ResolveError { kind, span });
    }

    /// Resolve type references for 0.0.3 V05: qualify bare struct names
    /// declared in the current module and, for already-qualified names,
    /// validate the target exists and check `pub` visibility. Composite
    /// types recurse so nested `Named` references are rewritten too.
    fn resolve_type(&mut self, ty: &Type, span: Span) -> Type {
        match ty {
            Type::Array(elem) => Type::Array(Box::new(self.resolve_type(elem, span))),
            Type::Box(inner) => Type::Box(Box::new(self.resolve_type(inner, span))),
            Type::Ref(inner) => Type::Ref(Box::new(self.resolve_type(inner, span))),
            Type::Result(ok, err) => Type::Result(
                Box::new(self.resolve_type(ok, span)),
                Box::new(self.resolve_type(err, span)),
            ),
            Type::Func(params, ret) => Type::Func(
                params.iter().map(|p| self.resolve_type(p, span)).collect(),
                Box::new(self.resolve_type(ret, span)),
            ),
            Type::Named(name) => self.resolve_type_name(name, span),
            Type::Base(_) | Type::Range => ty.clone(),
        }
    }

    fn resolve_type_name(&mut self, name: &str, span: Span) -> Type {
        let module = self.table.modules[self.cur_module].clone();
        if !name.contains('.') {
            // A bare name is a struct declared in the current module; leave
            // unknown names for typeck to report with its full error text.
            if module.items.contains_key(name) {
                return Type::Named(module.mangled_name(name));
            }
            return Type::Named(name.to_string());
        }

        // Qualified type path: `a.b.T` must name a module in the table and a
        // `pub` item on that module.
        let segments: Vec<String> = name.split('.').map(str::to_string).collect();
        if let Some((mid, prefix_len)) = self.table.longest_module_prefix(&segments)
            && prefix_len == segments.len() - 1
        {
            let target = &self.table.modules[mid];
            let is_imported = mid == self.cur_module
                || module
                    .imports
                    .iter()
                    .any(|imp| imp.path.iter().eq(target.path.iter()));
            if is_imported {
                let item_name = &segments[prefix_len];
                if let Some(item) = target.items.get(item_name) {
                    if !item.is_pub && mid != self.cur_module {
                        let private_item = item_name.clone();
                        let private_module = target.path_string();
                        self.add_error(
                            ResolveErrorKind::PrivateAccess {
                                item: private_item,
                                module: private_module,
                            },
                            span,
                        );
                        return Type::Named(name.to_string());
                    }
                    return Type::Named(target.mangled_name(item_name));
                }
            }
        }
        Type::Named(name.to_string())
    }

    /// Main entry point: resolve the entire program.
    fn resolve_program(self, ast: Ast) -> Result<Hir, Vec<ResolveError>> {
        self.resolve_program_inner(ast).map(|(hir, _, _)| hir)
    }

    /// Main entry point, also returning the next free binding/hir ID so a
    /// multi-module compiler can keep ID spaces disjoint (0.0.3 V05).
    fn resolve_program_inner(mut self, ast: Ast) -> Result<(Hir, u32, u32), Vec<ResolveError>> {
        self.register_builtins();
        self.register_imported_items();
        self.predeclare_global_functions(&ast);
        let global_funcs = std::mem::take(&mut self.global_funcs);

        let mut hir_items = Vec::new();
        let mut structs: Vec<StructDeclHir> = Vec::new();
        for item in ast.items {
            // Struct declarations bypass per-item resolution (0.0.3 V02):
            // they are a type-table input for typeck, not executable code.
            // Only `ref`-in-type legality is checked here.
            if let Item::Decl(Decl::Struct(s)) = &item {
                let module = self.table.modules[self.cur_module].clone();
                let mangled = module.mangled_name(&s.name);
                if let Some(first) = structs.iter().find(|e| e.name == mangled) {
                    self.errors.push(ResolveError {
                        kind: ResolveErrorKind::DuplicateBinding {
                            name: mangled.clone(),
                            first_span: first.span,
                        },
                        span: s.span,
                    });
                    continue;
                }
                for field in &s.fields {
                    if type_contains_ref(&field.ty) {
                        self.errors.push(ResolveError {
                            kind: ResolveErrorKind::RefNotAllowedHere {
                                context: "struct field type",
                            },
                            span: field.span,
                        });
                    }
                }
                structs.push(StructDeclHir {
                    name: mangled,
                    fields: s
                        .fields
                        .iter()
                        .map(|f| StructFieldHir {
                            name: f.name.clone(),
                            ty: self.resolve_type(&f.ty, f.span),
                            span: f.span,
                        })
                        .collect(),
                    span: s.span,
                });
                continue;
            }
            if let Ok(hir_item) = self.resolve_item(item, &global_funcs) {
                hir_items.push(hir_item);
            }
            // On Err the error was already recorded by `add_error`; the item
            // contributes no placeholder because the HIR is discarded anyway.
        }

        if !self.errors.is_empty() {
            return Err(self.errors);
        }

        Ok((
            Hir {
                items: hir_items,
                structs,
                span: ast.span,
            },
            self.binding_id_counter,
            self.hir_id_counter,
        ))
    }

    /// Register builtin functions in the global scope.
    fn register_builtins(&mut self) {
        for name in BUILTINS {
            let binding_id = self.next_binding_id();
            let _ = self.scopes.declare(
                (*name).to_string(),
                Binding {
                    id: binding_id,
                    kind: BindingKind::Builtin,
                    mutable: false,
                    builtin: true,
                    ref_param: false,
                    span: Span::SYNTHETIC, // builtins have no source span
                    hir_id: HirId(0),
                    global_name: None,
                },
            );
        }
    }

    /// Put every `pub` item from modules imported by the current module
    /// into the global scope under its fully-qualified name. The module
    /// table was built before resolution, so `import` declarations do not
    /// need repeated header parsing.
    fn register_imported_items(&mut self) {
        let current = self.table.modules[self.cur_module].clone();
        for import in &current.imports {
            let Some(mid) = self.table.find(&import.path) else {
                continue;
            };
            let target = self.table.modules[mid].clone();
            let mut entries: Vec<(String, crate::resolver::modules::ExportDef)> = target
                .items
                .iter()
                .filter(|(_, item)| item.is_pub)
                .map(|(name, item)| (name.clone(), item.clone()))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (local_name, item) in entries {
                let qualified = target.mangled_name(&local_name);
                if self.scopes.current_get(&qualified).is_some() {
                    continue;
                }
                // Structs have no runtime binding; their names are resolved
                // through the typeck struct table.
                if matches!(item.kind, ExportKind::Struct) {
                    continue;
                }
                let binding_id = self.next_binding_id();
                let kind = match item.kind {
                    ExportKind::Func => BindingKind::Function,
                    ExportKind::Const => BindingKind::Const,
                    ExportKind::Global => BindingKind::Variable,
                    ExportKind::Struct => continue,
                };
                let binding = Binding {
                    id: binding_id,
                    kind,
                    mutable: matches!(kind, BindingKind::Variable),
                    builtin: false,
                    ref_param: false,
                    span: item.span,
                    hir_id: self.next_hir_id(),
                    global_name: Some(qualified.clone()),
                };
                let _ = self.scopes.declare(qualified.clone(), binding);
            }

            // 0.0.3 V06: a std module's VM intrinsics are never declared in
            // Fleen source (D8) — they only exist as builtin table entries —
            // so materialize them as Builtin bindings here, exactly like the
            // prelude `print`, keyed by their qualified names. The name
            // prefix (`target.path_string() + "."`) is the module path, and
            // the leaf carries the single-segment builtin (e.g.
            // `std.io.println` for `import std.io;`). Qualified bare
            // constructors (`std.collection.range.range`) match the same
            // shape; bare `print`/`range`/`range_from` are seeded globally.
            let prefix = format!("{}.", target.path_string());
            for sig in crate::typeck::builtins::builtins() {
                let Some(leaf) = sig.name.strip_prefix(&prefix) else {
                    continue;
                };
                if leaf.is_empty() || leaf.contains('.') {
                    continue;
                }
                if self.scopes.current_get(sig.name).is_some() {
                    continue;
                }
                let binding_id = self.next_binding_id();
                let binding = Binding {
                    id: binding_id,
                    kind: BindingKind::Builtin,
                    mutable: false,
                    builtin: true,
                    ref_param: false,
                    span: Span::SYNTHETIC, // intrinsics have no source span
                    hir_id: self.next_hir_id(),
                    global_name: Some(sig.name.to_string()),
                };
                let _ = self.scopes.declare(sig.name.to_string(), binding);
            }
        }
    }

    /// First pass: declare all top-level functions so bodies can call
    /// functions declared later in the file. Duplicate top-level function
    /// names are reported here.
    fn predeclare_global_functions(&mut self, ast: &Ast) {
        for item in &ast.items {
            if let Item::Decl(Decl::Func(func_decl)) = item {
                let binding_id = self.next_binding_id();
                let module = self.table.modules[self.cur_module].clone();
                let global_name = module.mangled_name(&func_decl.name);
                let binding = Binding {
                    id: binding_id,
                    kind: BindingKind::Function,
                    mutable: false, // function names are immutable bindings
                    builtin: false,
                    ref_param: false,
                    span: func_decl.span,
                    hir_id: HirId(0), // placeholder, filled when the body resolves
                    global_name: Some(global_name),
                };
                match self.scopes.declare(func_decl.name.clone(), binding) {
                    Ok(()) => {
                        self.global_funcs.insert(func_decl.name.clone(), binding_id);
                    }
                    Err(first) => {
                        self.add_error(
                            ResolveErrorKind::DuplicateBinding {
                                name: func_decl.name.clone(),
                                first_span: first.span,
                            },
                            func_decl.span,
                        );
                    }
                }
            }
        }
    }

    fn resolve_item(
        &mut self,
        item: Item,
        global_funcs: &HashMap<String, BindingId>,
    ) -> Result<HirItem, ()> {
        let hir_item = match item {
            Item::Import(import_decl) => HirItem::Import(self.resolve_import(import_decl)),
            Item::Decl(decl) => HirItem::Decl(self.resolve_decl(decl, global_funcs)?),
            Item::Expr(expr) => HirItem::Expr(self.resolve_expr(expr)?),
        };
        Ok(hir_item)
    }

    fn resolve_import(&mut self, import: ImportDecl) -> ImportDeclHir {
        ImportDeclHir {
            path: import.path,
            span: import.span,
        }
    }

    fn resolve_decl(
        &mut self,
        decl: Decl,
        global_funcs: &HashMap<String, BindingId>,
    ) -> Result<DeclHir, ()> {
        match decl {
            Decl::Func(func) => Ok(DeclHir::Func(self.resolve_func_decl(func, global_funcs)?)),
            Decl::Var(var) => Ok(DeclHir::Var(self.resolve_var_binding(var)?)),
            Decl::Const(c) => Ok(DeclHir::Const(self.resolve_const_decl(c)?)),
            // Top-level struct declarations are intercepted by `resolve_program`
            // (they feed the typeck struct table, not `items`), and the parser
            // rejects struct declarations inside blocks, so this arm cannot run.
            Decl::Struct(_) => unreachable!("struct declarations never reach resolve_decl"),
        }
    }

    /// Resolve a function declaration.
    ///
    /// The name is declared in the *current* scope before entering the function
    /// scope, so nested functions are visible to later code in the same scope
    /// and recursive calls resolve. Top-level functions were already declared
    /// by the forward-reference pass, so they are not declared again here.
    fn resolve_func_decl(
        &mut self,
        func: FuncDecl,
        global_funcs: &HashMap<String, BindingId>,
    ) -> Result<FuncDeclHir, ()> {
        // 0.0.2 U05 (DESIGN §10.4): `ref` is legal only as the *outer* type
        // of a function parameter. Return types, and any nested or embedded
        // `ref`, are rejected here so no `ref` type ever reaches typeck.
        let mut ref_error = false;
        if let Some(ret) = &func.ret_type
            && type_contains_ref(ret)
        {
            self.add_error(
                ResolveErrorKind::RefNotAllowedHere {
                    context: "return type",
                },
                func.span,
            );
            ref_error = true;
        }
        for param in &func.params {
            // `ref T` is fine when T itself is not a ref; any other
            // occurrence of `ref` (nested `ref ref T`, `ref` inside
            // composites, non-ref positions) is rejected.
            let allowed = match &param.ty {
                Type::Ref(inner) => !type_contains_ref(inner),
                _ => !type_contains_ref(&param.ty),
            };
            if !allowed {
                self.add_error(
                    ResolveErrorKind::RefNotAllowedHere {
                        context: "function parameter",
                    },
                    param.span,
                );
                ref_error = true;
            }
        }
        if ref_error {
            return Err(());
        }

        let predeclared = global_funcs.get(&func.name).copied();
        let binding_id = match predeclared {
            Some(id) => id,
            None => {
                let id = self.next_binding_id();
                let binding = Binding {
                    id,
                    kind: BindingKind::Function,
                    mutable: false,
                    builtin: false,
                    ref_param: false,
                    span: func.span,
                    hir_id: HirId(0),
                    global_name: None,
                };
                match self.scopes.declare(func.name.clone(), binding) {
                    Ok(()) => id,
                    Err(first) => {
                        self.add_error(
                            ResolveErrorKind::DuplicateBinding {
                                name: func.name.clone(),
                                first_span: first.span,
                            },
                            func.span,
                        );
                        // Still resolve the body so later errors are collected.
                        id
                    }
                }
            }
        };

        let (params, body) = self.resolve_function_body(func.params, func.body);

        let hir_id = self.next_hir_id();
        if let Some(binding) = self.scopes.get_mut_by_id(binding_id) {
            binding.hir_id = hir_id;
        }

        // 0.0.3 V05: use the mangled (fully-qualified) name so multi-module
        // HIRs merge into one flat bytecode namespace without collisions.
        let hir_name = self
            .scopes
            .get_by_id(binding_id)
            .and_then(|b| b.global_name.clone())
            .unwrap_or(func.name);

        Ok(FuncDeclHir {
            name: hir_name,
            params,
            ret_type: func.ret_type.map(|ty| self.resolve_type(&ty, func.span)),
            body,
            hir_id,
            span: func.span,
        })
    }

    /// Enter the function scope, resolve parameters and body, and *always*
    /// leave the scope — even when body resolution fails. Without this,
    /// a failure deep in the body leaks the function scope and corrupts
    /// shadow/loop checks for the rest of the program.
    fn resolve_function_body(
        &mut self,
        params: Vec<Param>,
        body: FuncBody,
    ) -> (Vec<ParamHir>, FuncBodyHir) {
        self.scopes.enter_function_scope();

        let mut hir_params = Vec::new();
        for param in params {
            let binding_id = self.next_binding_id();
            let hir_id = self.next_hir_id();
            let binding = Binding {
                id: binding_id,
                kind: BindingKind::Parameter,
                mutable: false, // parameters are immutable in 0.0.1
                builtin: false,
                // 0.0.2 U05 (DESIGN §10.4): `ref T` parameters are
                // read-only borrows; re-declaring the name is flagged as
                // `AssignToRefParam` in `resolve_var_binding`.
                ref_param: matches!(param.ty, Type::Ref(_)),
                span: param.span,
                hir_id,
                global_name: None,
            };
            match self.scopes.declare(param.name.clone(), binding) {
                Ok(()) => {}
                Err(first) => {
                    self.add_error(
                        ResolveErrorKind::DuplicateBinding {
                            name: param.name.clone(),
                            first_span: first.span,
                        },
                        param.span,
                    );
                }
            }
            hir_params.push(ParamHir {
                name: param.name,
                ty: self.resolve_type(&param.ty, param.span),
                binding_id,
                hir_id,
                span: param.span,
            });
        }

        let body_result = match body {
            FuncBody::SingleExpr(expr) => match self.resolve_expr(*expr) {
                Ok(hir_expr) => Ok(FuncBodyHir::SingleExpr(Box::new(hir_expr))),
                Err(()) => Ok(FuncBodyHir::Block(BlockHir {
                    stmts: vec![StmtHir::Error],
                    tail_expr: None,
                    hir_id: self.next_hir_id(),
                    span: Span::SYNTHETIC,
                })),
            },
            FuncBody::Block(block) => self
                .resolve_block_inner(block, ScopeKind::Block)
                .map(FuncBodyHir::Block),
        };

        self.scopes.exit_scope();

        let hir_body = match body_result {
            Ok(b) => b,
            Err(()) => FuncBodyHir::Block(BlockHir {
                stmts: vec![StmtHir::Error],
                tail_expr: None,
                hir_id: HirId(0),
                span: Span::SYNTHETIC,
            }),
        };
        (hir_params, hir_body)
    }

    /// Resolve a statement-position `x = expr` / `x: int = expr`.
    ///
    /// Unified `=` semantics (DESIGN.md §3.1/§3.4/§4.5):
    /// - Name bound at this position (current scope, or loop body with an
    ///   outer mutable binding up to the function boundary) → **assignment**:
    ///   the binding must be mutable and cannot carry a type annotation.
    /// - Otherwise → **new binding**; an outer name is shadowed (mutability
    ///   must match, per §4.3).
    ///
    /// The initializer is resolved BEFORE the binding is declared, so
    /// `x = x + 1` on a first occurrence fails (§3.1) while
    /// `x = x + 1` on an assignment sees the existing binding.
    fn resolve_var_binding(&mut self, var: VarBinding) -> Result<VarBindingHir, ()> {
        let init = self.resolve_expr(*var.init)?;
        let hir_id = self.next_hir_id();

        // 0.0.2 U05: `ref` annotations are forbidden on variables (DESIGN
        // §10.4); only function parameters may be borrows.
        if let Some(ty) = &var.ty
            && type_contains_ref(ty)
        {
            self.add_error(
                ResolveErrorKind::RefNotAllowedHere {
                    context: if self.scopes.in_function() {
                        "local variable type"
                    } else {
                        "global variable type"
                    },
                },
                var.span,
            );
            return Err(());
        }

        // Assignment to an existing binding visible at this position.
        if let Some(target) = self.scopes.find_assign_target(&var.name) {
            let (existing_id, mutable, kind) = (target.id, target.mutable, target.kind);
            // A function name already bound *in this scope* is a name clash,
            // not an assignment target (DESIGN.md §3.5: one binding per name).
            if kind == BindingKind::Function {
                self.add_error(
                    ResolveErrorKind::DuplicateBinding {
                        name: var.name.clone(),
                        first_span: target.span,
                    },
                    var.span,
                );
                return Err(());
            }
            if !mutable {
                self.add_error(
                    ResolveErrorKind::AssignToImmutable {
                        name: var.name.clone(),
                    },
                    var.span,
                );
                return Err(());
            }
            if var.ty.is_some() {
                self.add_error(ResolveErrorKind::TypeAnnotationOnAssignment, var.span);
                return Err(());
            }

            if let Some(binding) = self.scopes.get_mut_by_id(existing_id) {
                binding.hir_id = hir_id;
            }
            return Ok(VarBindingHir {
                name: var.name,
                ty: var.ty,
                init: Box::new(init),
                binding_id: existing_id,
                hir_id,
                span: var.span,
            });
        }

        // 0.0.2 U05 (DESIGN §10.4): a `ref` parameter is a read-only
        // borrow. Because parameters are immutable, `s = …` at a ref
        // parameter `s` would otherwise become a *new binding* that
        // silently shadows the borrow — the user most likely meant to
        // write through it. Flag it instead of shadowing.
        if let Some(outer) = self.scopes.find_shadow_domain(&var.name)
            && outer.ref_param
        {
            self.add_error(
                ResolveErrorKind::AssignToRefParam {
                    name: var.name.clone(),
                },
                var.span,
            );
            return Err(());
        }

        // New binding. If it shadows an outer name, mutability must match
        // (DESIGN.md §4.3); parameters, functions and builtins are exempt.
        let new_mutable = true; // plain bindings are mutable
        if let Some(outer) = self.scopes.find_shadow_domain(&var.name)
            && outer.kind.shadow_checks()
            && outer.mutable != new_mutable
        {
            self.add_error(
                ResolveErrorKind::ShadowingMutabilityMismatch {
                    outer_mutable: outer.mutable,
                    inner_mutable: new_mutable,
                },
                var.span,
            );
            return Err(());
        }

        let binding_id = self.next_binding_id();
        let module = self.table.modules[self.cur_module].clone();
        let global_name = if self.scopes.in_function() {
            None
        } else {
            Some(module.mangled_name(&var.name))
        };
        if let Err(first) = self.scopes.declare(
            var.name.clone(),
            Binding {
                id: binding_id,
                kind: BindingKind::Variable,
                mutable: true,
                builtin: false,
                ref_param: false,
                span: var.span,
                hir_id,
                global_name: global_name.clone(),
            },
        ) {
            self.add_error(
                ResolveErrorKind::DuplicateBinding {
                    name: var.name.clone(),
                    first_span: first.span,
                },
                var.span,
            );
            return Err(());
        }

        Ok(VarBindingHir {
            name: global_name.unwrap_or(var.name),
            ty: var.ty.map(|ty| self.resolve_type(&ty, var.span)),
            init: Box::new(init),
            binding_id,
            hir_id,
            span: var.span,
        })
    }

    /// Resolve a const declaration.
    fn resolve_const_decl(&mut self, c: ConstDecl) -> Result<ConstDeclHir, ()> {
        // Resolve the initializer WITHOUT the new binding in scope.
        let init = self.resolve_expr(*c.init)?;
        let hir_id = self.next_hir_id();

        // 0.0.2 U05: same `ref` restriction as variable annotations.
        if let Some(ty) = &c.ty
            && type_contains_ref(ty)
        {
            self.add_error(
                ResolveErrorKind::RefNotAllowedHere {
                    context: if self.scopes.in_function() {
                        "local const type"
                    } else {
                        "global const type"
                    },
                },
                c.span,
            );
            return Err(());
        }

        // `const` on a name already bound in the same scope is always an error
        // (DESIGN.md §3.5: mutable → const and const → const are both ❌).
        if let Some(existing) = self.scopes.current_get(&c.name)
            && !existing.builtin
        {
            self.add_error(
                ResolveErrorKind::DuplicateBinding {
                    name: c.name.clone(),
                    first_span: existing.span,
                },
                c.span,
            );
            return Err(());
        }

        // Shadow-check up to the function boundary only (DESIGN.md §4.3:
        // "functions have no effect"); parameters/functions/builtins are exempt.
        let new_mutable = false; // const is immutable
        if let Some(outer) = self.scopes.find_shadow_domain(&c.name)
            && outer.kind.shadow_checks()
            && outer.mutable != new_mutable
        {
            self.add_error(
                ResolveErrorKind::ShadowingMutabilityMismatch {
                    outer_mutable: outer.mutable,
                    inner_mutable: new_mutable,
                },
                c.span,
            );
            return Err(());
        }

        let binding_id = self.next_binding_id();
        let module = self.table.modules[self.cur_module].clone();
        let global_name = if self.scopes.in_function() {
            None
        } else {
            Some(module.mangled_name(&c.name))
        };
        if let Err(first) = self.scopes.declare(
            c.name.clone(),
            Binding {
                id: binding_id,
                kind: BindingKind::Const,
                mutable: false,
                builtin: false,
                ref_param: false,
                span: c.span,
                hir_id,
                global_name: global_name.clone(),
            },
        ) {
            self.add_error(
                ResolveErrorKind::DuplicateBinding {
                    name: c.name.clone(),
                    first_span: first.span,
                },
                c.span,
            );
            return Err(());
        }

        Ok(ConstDeclHir {
            name: global_name.unwrap_or(c.name),
            ty: c.ty.map(|ty| self.resolve_type(&ty, c.span)),
            init: Box::new(init),
            binding_id,
            hir_id,
            span: c.span,
        })
    }

    /// Resolve an expression.
    fn resolve_expr(&mut self, expr: Expr) -> Result<ExprHir, ()> {
        let hir = match expr {
            Expr::Assign(lhs, rhs) => self.resolve_assign(*lhs, *rhs)?,
            Expr::If(if_expr) => ExprHir::If(self.resolve_if_expr(if_expr)?),
            Expr::While(while_expr) => ExprHir::While(self.resolve_while_expr(while_expr)?),
            Expr::For(for_expr) => ExprHir::For(self.resolve_for_expr(for_expr)?),
            Expr::Choose(choose_expr) => ExprHir::Choose(self.resolve_choose_expr(choose_expr)?),
            Expr::Or(lhs, rhs) => {
                let l = self.resolve_expr(*lhs)?;
                let r = self.resolve_expr(*rhs)?;
                ExprHir::Or(Box::new(l), Box::new(r))
            }
            Expr::And(lhs, rhs) => {
                let l = self.resolve_expr(*lhs)?;
                let r = self.resolve_expr(*rhs)?;
                ExprHir::And(Box::new(l), Box::new(r))
            }
            Expr::Eq(lhs, rhs) => {
                let l = self.resolve_expr(*lhs)?;
                let r = self.resolve_expr(*rhs)?;
                ExprHir::Eq(Box::new(l), Box::new(r))
            }
            Expr::Ne(lhs, rhs) => {
                let l = self.resolve_expr(*lhs)?;
                let r = self.resolve_expr(*rhs)?;
                ExprHir::Ne(Box::new(l), Box::new(r))
            }
            Expr::Lt(lhs, rhs) => {
                let l = self.resolve_expr(*lhs)?;
                let r = self.resolve_expr(*rhs)?;
                ExprHir::Lt(Box::new(l), Box::new(r))
            }
            Expr::Gt(lhs, rhs) => {
                let l = self.resolve_expr(*lhs)?;
                let r = self.resolve_expr(*rhs)?;
                ExprHir::Gt(Box::new(l), Box::new(r))
            }
            Expr::Le(lhs, rhs) => {
                let l = self.resolve_expr(*lhs)?;
                let r = self.resolve_expr(*rhs)?;
                ExprHir::Le(Box::new(l), Box::new(r))
            }
            Expr::Ge(lhs, rhs) => {
                let l = self.resolve_expr(*lhs)?;
                let r = self.resolve_expr(*rhs)?;
                ExprHir::Ge(Box::new(l), Box::new(r))
            }
            Expr::Add(lhs, rhs) => {
                let l = self.resolve_expr(*lhs)?;
                let r = self.resolve_expr(*rhs)?;
                ExprHir::Add(Box::new(l), Box::new(r))
            }
            Expr::Sub(lhs, rhs) => {
                let l = self.resolve_expr(*lhs)?;
                let r = self.resolve_expr(*rhs)?;
                ExprHir::Sub(Box::new(l), Box::new(r))
            }
            Expr::Mul(lhs, rhs) => {
                let l = self.resolve_expr(*lhs)?;
                let r = self.resolve_expr(*rhs)?;
                ExprHir::Mul(Box::new(l), Box::new(r))
            }
            Expr::Div(lhs, rhs) => {
                let l = self.resolve_expr(*lhs)?;
                let r = self.resolve_expr(*rhs)?;
                ExprHir::Div(Box::new(l), Box::new(r))
            }
            Expr::Mod(lhs, rhs) => {
                let l = self.resolve_expr(*lhs)?;
                let r = self.resolve_expr(*rhs)?;
                ExprHir::Mod(Box::new(l), Box::new(r))
            }
            Expr::Not(expr) => ExprHir::Not(Box::new(self.resolve_expr(*expr)?)),
            Expr::Neg(expr) => ExprHir::Neg(Box::new(self.resolve_expr(*expr)?)),
            // 0.0.2 prefix keyword expressions: resolved faithfully here;
            // operand legality and semantics are typeck's job (U04/U05).
            Expr::Move(expr, span) => ExprHir::Move(Box::new(self.resolve_expr(*expr)?), span),
            Expr::Clone(expr, span) => ExprHir::Clone(Box::new(self.resolve_expr(*expr)?), span),
            Expr::Box(expr, span) => ExprHir::Box(Box::new(self.resolve_expr(*expr)?), span),
            Expr::Deref(expr, span) => ExprHir::Deref(Box::new(self.resolve_expr(*expr)?), span),
            // 0.0.2 U13: type cast; operand resolved, target type passed through as-is.
            Expr::Cast(expr, ty, span) => {
                let resolved_ty = self.resolve_type(&ty, span);
                ExprHir::Cast(Box::new(self.resolve_expr(*expr)?), resolved_ty, span)
            }
            Expr::Question(expr, span) => {
                ExprHir::Question(Box::new(self.resolve_expr(*expr)?), span)
            }
            Expr::Call(func, args) => {
                let hir_func = self.resolve_expr(*func)?;
                let hir_args: Result<Vec<_>, _> =
                    args.into_iter().map(|a| self.resolve_expr(a)).collect();
                ExprHir::Call(Box::new(hir_func), hir_args?)
            }
            Expr::Index(arr, idx, span) => {
                let a = self.resolve_expr(*arr)?;
                let i = self.resolve_expr(*idx)?;
                ExprHir::Index(Box::new(a), Box::new(i), span)
            }
            Expr::ArrayLit(lit) => {
                let elems = lit
                    .elems
                    .into_iter()
                    .map(|e| self.resolve_expr(e))
                    .collect::<Result<Vec<_>, ()>>()?;
                ExprHir::ArrayLit(elems, lit.span)
            }
            Expr::Field(obj, field) => {
                let span = obj.span();
                if let Some(hir) = self.try_resolve_module_path(&obj, &field, span)? {
                    hir
                } else {
                    let o = self.resolve_expr(*obj)?;
                    ExprHir::Field(Box::new(o), field)
                }
            }
            Expr::StructLit(lit) => {
                let inits = lit
                    .inits
                    .into_iter()
                    .map(|(name, expr)| Ok((name, self.resolve_expr(expr)?)))
                    .collect::<Result<Vec<_>, ()>>()?;
                // 0.0.3 V05: qualify a bare current-module struct so typeck's
                // merged struct table recognizes it. Cross-module values are
                // normally constructed by pub constructors in 0.0.3.
                let module = self.table.modules[self.cur_module].clone();
                let name = if lit.name.contains('.') {
                    lit.name
                } else if module.items.contains_key(&lit.name) {
                    module.mangled_name(&lit.name)
                } else {
                    lit.name
                };
                ExprHir::StructLit {
                    name,
                    inits,
                    hir_id: self.next_hir_id(),
                    span: lit.span,
                }
            }
            Expr::Int(n, span) => ExprHir::Int(n, span),
            Expr::Float(f, span) => ExprHir::Float(f, span),
            Expr::Bool(b, span) => ExprHir::Bool(b, span),
            Expr::Str(s, span) => ExprHir::Str(s, span),
            Expr::Ident(name, span) => self.resolve_ident(name, span)?,
            Expr::Block(block) => ExprHir::Block(self.resolve_block(block)?),
        };
        Ok(hir)
    }

    /// Resolve an expression-position assignment: `x = rhs` or `deref b = rhs`.
    ///
    /// Expression-position `=` is assignment only — it never binds and never
    /// shadows. The target follows the same lookup as statement-position `=`
    /// (current scope, or loop body), so it can never write through a
    /// function boundary (DESIGN.md §4.2).
    ///
    /// 0.0.2 adds `deref b = rhs` (write a box's pointee): the target is the
    /// box variable `b`, resolved by name. Writing the pointee does not
    /// rebind `b`, so mutability of `b` is not required here; whether `b`
    /// actually holds a box is typeck's business (U04/U05).
    fn resolve_assign(&mut self, lhs: Expr, rhs: Expr) -> Result<ExprHir, ()> {
        let rhs_span = rhs.span();

        // 0.0.3 V05: m.items = [...] — a module-qualified global assignment.
        // When the Field base name is a module path (not a local binding),
        // resolve the full qualified path through the module table and
        // treat the result as a plain variable assignment to the mangled
        // global name.
        let module_assign = match &lhs {
            Expr::Field(obj, field) => {
                if let Expr::Ident(name, _) = &**obj {
                    if self.scopes.get(name).is_none()
                        && self.table.find(std::slice::from_ref(name)).is_some()
                    {
                        let span = lhs.span();
                        let resolved = self.try_resolve_module_path(obj, field, span)?;
                        match resolved {
                            Some(ExprHir::Ident {
                                name: qualified,
                                binding_id,
                                ..
                            }) => Some((qualified, binding_id, span)),
                            _ => None,
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some((name, binding_id, span)) = module_assign {
            let rhs_hir = self.resolve_expr(rhs)?;
            let binding = self.scopes.get_by_id(binding_id);
            if let Some(b) = &binding
                && !b.mutable
            {
                self.add_error(
                    ResolveErrorKind::AssignToImmutable { name: name.clone() },
                    span,
                );
                return Err(());
            }
            return Ok(ExprHir::Assign {
                name,
                binding_id,
                rhs: Box::new(rhs_hir),
                hir_id: self.next_hir_id(),
                span,
            });
        }

        let (target, span) = match lhs {
            Expr::Ident(name, span) => (AssignTarget::Ident(name), span),
            Expr::Deref(inner, span) => match *inner {
                Expr::Ident(name, _) => (AssignTarget::Deref(name), span),
                other => {
                    self.add_error(ResolveErrorKind::InvalidAssignmentTarget, other.span());
                    return Err(());
                }
            },
            // 0.0.3 V02: one-level field write `p.f = e`. The one-level rule
            // is enforced here at the cheapest stage: `p.a.b` (Field of a
            // Field) and any other receiver shape fall through to the
            // invalid-target case below.
            Expr::Field(obj, field) => {
                let span = obj.span();
                match *obj {
                    Expr::Ident(name, _) => (AssignTarget::Field { obj: name, field }, span),
                    other => {
                        self.add_error(ResolveErrorKind::InvalidAssignmentTarget, other.span());
                        return Err(());
                    }
                }
            }
            // 0.0.3 V03: one-level index write `a[i] = e`. The one-level
            // rule is enforced here at the cheapest stage: the receiver
            // must be a bare binding, so `m[i][j]` and `p.f[i]` fall into
            // the invalid-target case below. Bounds are a runtime concern;
            // the parser only checked the shape.
            Expr::Index(obj, idx, span) => match *obj {
                Expr::Ident(name, _) => {
                    let index = Box::new(self.resolve_expr(*idx)?);
                    (AssignTarget::Index { obj: name, index }, span)
                }
                other => {
                    self.add_error(ResolveErrorKind::InvalidAssignmentTarget, other.span());
                    return Err(());
                }
            },
            other => {
                self.add_error(ResolveErrorKind::InvalidAssignmentTarget, other.span());
                return Err(());
            }
        };

        let rhs_hir = self.resolve_expr(rhs)?;

        // Both targets resolve the box/struct/array/variable by name; they
        // differ only in lookup (plain assignment must find an assignable
        // binding) and in mutability (writing a pointee/field/element does
        // not rebind the variable, so it need not be mutable). A `ref`
        // parameter is read-only in all composite forms.
        let (name, binding) = match &target {
            AssignTarget::Ident(name) => (name, self.scopes.find_assign_target(name)),
            AssignTarget::Deref(name)
            | AssignTarget::Field { obj: name, .. }
            | AssignTarget::Index { obj: name, .. } => (name, self.scopes.get(name)),
        };
        let Some(binding) = binding else {
            self.add_error(
                ResolveErrorKind::UndeclaredVariable { name: name.clone() },
                span,
            );
            return Err(());
        };
        if matches!(target, AssignTarget::Ident(_)) && !binding.mutable {
            self.add_error(
                ResolveErrorKind::AssignToImmutable { name: name.clone() },
                span,
            );
            return Err(());
        }
        // Writing a field/element of a borrowed struct/array would mutate
        // the caller's value: `ref` parameters are read-only (DESIGN §10.4),
        // same as the shadowing rule for `deref`-assign through a borrowed
        // box.
        if matches!(
            target,
            AssignTarget::Field { .. } | AssignTarget::Index { .. }
        ) && binding.ref_param
        {
            self.add_error(
                ResolveErrorKind::AssignToRefParam { name: name.clone() },
                span,
            );
            return Err(());
        }

        let binding_id = binding.id;
        let name = name.clone();
        match target {
            AssignTarget::Ident(_) => Ok(ExprHir::Assign {
                name,
                binding_id,
                rhs: Box::new(rhs_hir),
                hir_id: self.next_hir_id(),
                span,
            }),
            AssignTarget::Deref(_) => Ok(ExprHir::AssignDeref {
                name,
                binding_id,
                rhs: Box::new(rhs_hir),
                hir_id: self.next_hir_id(),
                span: Span::new(span.file, span.start, rhs_span.end),
            }),
            AssignTarget::Field { field, .. } => Ok(ExprHir::AssignField {
                obj_name: name,
                obj_binding_id: binding_id,
                field,
                rhs: Box::new(rhs_hir),
                hir_id: self.next_hir_id(),
                span: Span::new(span.file, span.start, rhs_span.end),
            }),
            AssignTarget::Index { index, .. } => Ok(ExprHir::AssignIndex {
                name,
                binding_id,
                index,
                rhs: Box::new(rhs_hir),
                hir_id: self.next_hir_id(),
                span: Span::new(span.file, span.start, rhs_span.end),
            }),
        }
    }

    /// Resolve an identifier expression (variable or function reference).
    /// The recorded span is the *use* site, not the definition site.
    fn resolve_ident(&mut self, name: String, span: Span) -> Result<ExprHir, ()> {
        let binding_id = match self.scopes.get(&name) {
            Some(binding) => binding.id,
            None => {
                self.add_error(
                    ResolveErrorKind::UndeclaredVariable { name: name.clone() },
                    span,
                );
                return Err(());
            }
        };

        // 0.0.3 V05: use the fully-qualified global name if available so
        // codegen resolves to the correct global, not the local alias.
        let hir_name = self
            .scopes
            .get_by_id(binding_id)
            .and_then(|b| b.global_name.clone())
            .unwrap_or(name);

        Ok(ExprHir::Ident {
            name: hir_name,
            binding_id,
            hir_id: self.next_hir_id(),
            span,
        })
    }

    /// Try to interpret a dotted expression as a module-qualified item
    /// (m.items). Returns None when the expression is not a pure identifier
    /// chain or when the first segment is shadowed by a local binding.
    fn try_resolve_module_path(
        &mut self,
        obj: &Expr,
        field: &str,
        span: Span,
    ) -> Result<Option<ExprHir>, ()> {
        let mut segments = vec![field.to_string()];
        let mut cur = obj;
        loop {
            match cur {
                Expr::Ident(name, _) => {
                    segments.push(name.clone());
                    break;
                }
                Expr::Field(inner, f) => {
                    segments.push(f.clone());
                    cur = inner;
                }
                _ => return Ok(None),
            }
        }
        segments.reverse();

        // A local binding shadows a module path.
        if self.scopes.get(&segments[0]).is_some() {
            return Ok(None);
        }

        // std.io.print is the prelude spelling; route to the builtin.
        if segments == ["std", "io", "print"] {
            return self.resolve_ident("print".to_string(), span).map(Some);
        }

        let module = self.table.modules[self.cur_module].clone();
        if let Some((mid, prefix_len)) = self.table.longest_module_prefix(&segments)
            && prefix_len == segments.len() - 1
        {
            let target = &self.table.modules[mid];
            let is_imported = mid == self.cur_module
                || module
                    .imports
                    .iter()
                    .any(|imp| imp.path.iter().eq(target.path.iter()));
            if !is_imported {
                return Ok(None);
            }
            let item_name = &segments[prefix_len];
            if let Some(item) = target.items.get(item_name) {
                if !item.is_pub && mid != self.cur_module {
                    self.add_error(
                        ResolveErrorKind::PrivateAccess {
                            item: item_name.clone(),
                            module: target.path_string(),
                        },
                        span,
                    );
                    return Err(());
                }
                let qualified = target.mangled_name(item_name);
                return self.resolve_ident(qualified, span).map(Some);
            }
            // 0.0.3 V06: the item may be a VM intrinsic that lives only in
            // the builtin table (`std.str.len`, `std.os.exit`, ...), in
            // which case it was materialized as a Builtin binding during
            // import registration; route to it under its qualified name.
            let qualified = target.mangled_name(item_name);
            if crate::typeck::builtins::find_builtin(&qualified).is_some() {
                return self.resolve_ident(qualified, span).map(Some);
            }
        }

        Ok(None)
    }

    fn resolve_if_expr(&mut self, if_expr: ExprIf) -> Result<ExprIfHir, ()> {
        let condition = self.resolve_expr(*if_expr.condition)?;
        let then_branch = self.resolve_block(if_expr.then_branch)?;

        let mut elif_branches = Vec::new();
        for (cond, block) in if_expr.elif_branches {
            let hir_cond = self.resolve_expr(*cond)?;
            let hir_block = self.resolve_block(block)?;
            elif_branches.push((Box::new(hir_cond), hir_block));
        }

        let else_branch = if let Some(block) = if_expr.else_branch {
            Some(self.resolve_block(block)?)
        } else {
            None
        };

        Ok(ExprIfHir {
            condition: Box::new(condition),
            then_branch,
            elif_branches,
            else_branch,
            hir_id: self.next_hir_id(),
            span: if_expr.span,
        })
    }

    fn resolve_while_expr(&mut self, while_expr: ExprWhile) -> Result<ExprWhileHir, ()> {
        // The condition sees the enclosing scope; the body is a *loop* scope,
        // where `x = expr` assigns an existing outer mutable binding instead of
        // shadowing it (DESIGN.md §4.5).
        let condition = self.resolve_expr(*while_expr.condition)?;
        let body = self.resolve_block_inner(while_expr.body, ScopeKind::Loop)?;

        Ok(ExprWhileHir {
            condition: Box::new(condition),
            body,
            hir_id: self.next_hir_id(),
            span: while_expr.span,
        })
    }

    /// Resolve a `for` expression (0.0.3 V04): `for var in iterable { body }`.
    ///
    /// The iterable is resolved in the *enclosing* scope (it must not see the
    /// loop variable). The loop variable is then declared as an immutable
    /// binding in a fresh loop scope, and the body is resolved within that
    /// scope so references to `var` resolve to it (DESIGN.md §4.5 loop-scope
    /// rules apply to assignments inside the body).
    fn resolve_for_expr(&mut self, for_expr: ExprFor) -> Result<ExprForHir, ()> {
        let iterable = self.resolve_expr(*for_expr.iterable)?;

        // Enter the loop scope and declare the loop variable before resolving
        // the body. We resolve the body statements directly here (rather than
        // via `resolve_block_inner`, which would open its own nested loop
        // scope) so the single loop scope holds the loop variable.
        self.scopes.enter_loop_scope();
        let var_binding_id = self.next_binding_id();
        let var_hir_id = self.next_hir_id();
        if let Err(existing) = self.scopes.declare(
            for_expr.var.clone(),
            Binding {
                id: var_binding_id,
                kind: BindingKind::Variable,
                mutable: false, // loop variable is immutable (D4)
                builtin: false,
                ref_param: false,
                span: for_expr.span,
                hir_id: var_hir_id,
                global_name: None,
            },
        ) {
            self.add_error(
                ResolveErrorKind::DuplicateBinding {
                    name: for_expr.var.clone(),
                    first_span: existing.span,
                },
                for_expr.span,
            );
        }

        let mut hir_stmts = Vec::new();
        for stmt in for_expr.body.stmts {
            hir_stmts.push(match self.resolve_stmt(stmt) {
                Ok(s) => s,
                Err(()) => StmtHir::Error,
            });
        }
        let tail_result: Result<Option<Box<ExprHir>>, ()> = match for_expr.body.tail_expr {
            Some(expr) => self.resolve_expr(*expr).map(|e| Some(Box::new(e))),
            None => Ok(None),
        };
        self.scopes.exit_scope();
        let body = BlockHir {
            stmts: hir_stmts,
            tail_expr: tail_result?,
            hir_id: self.next_hir_id(),
            span: for_expr.body.span,
        };

        Ok(ExprForHir {
            var: for_expr.var,
            var_binding_id,
            iterable: Box::new(iterable),
            body,
            hir_id: self.next_hir_id(),
            span: for_expr.span,
        })
    }

    fn resolve_choose_expr(&mut self, choose_expr: ExprChoose) -> Result<ExprChooseHir, ()> {
        let scrutinee = self.resolve_expr(*choose_expr.scrutinee)?;

        let mut arms = Vec::new();
        for arm in choose_expr.arms {
            // Each when/otherwise clause gets its own scope for pattern bindings;
            // the pattern binding must not leak into the enclosing scope.
            self.scopes.enter_block_scope();

            let pattern = self.resolve_pattern(arm.pattern);
            let guard = match (pattern.as_ref(), arm.guard) {
                (Ok(_), Some(g)) => match self.resolve_expr(*g) {
                    Ok(hir_guard) => Some(Box::new(hir_guard)),
                    Err(()) => None,
                },
                _ => None,
            };
            let body = self.resolve_block_inner(arm.body, ScopeKind::Block);

            // Leave the arm scope even when the arm failed to resolve.
            self.scopes.exit_scope();

            match (pattern, body) {
                (Ok(pattern), Ok(body)) => {
                    arms.push(ChooseArmHir {
                        pattern,
                        guard,
                        body,
                    });
                }
                (pattern, body) => {
                    // Placeholder arm keeps later arms' positions stable.
                    arms.push(ChooseArmHir {
                        pattern: pattern.unwrap_or(PatternHir::Error),
                        guard,
                        body: body.unwrap_or(BlockHir {
                            stmts: vec![StmtHir::Error],
                            tail_expr: None,
                            hir_id: HirId(0),
                            span: Span::SYNTHETIC,
                        }),
                    });
                }
            }
        }

        Ok(ExprChooseHir {
            scrutinee: Box::new(scrutinee),
            arms,
            hir_id: self.next_hir_id(),
            span: choose_expr.span,
        })
    }

    fn resolve_pattern(&mut self, pattern: Pattern) -> Result<PatternHir, ()> {
        match pattern {
            Pattern::Literal(expr) => {
                let hir_expr = self.resolve_expr(expr)?;
                Ok(PatternHir::Literal(Box::new(hir_expr)))
            }
            Pattern::Ident(name, span) => {
                let (binding_id, hir_id) = self.declare_pattern_binding(&name, span)?;
                Ok(PatternHir::Ident {
                    name,
                    binding_id,
                    hir_id,
                    span,
                })
            }
            Pattern::ResultCtor {
                ctor,
                binding,
                span,
            } => {
                // 0.0.2: `Ok(v)` / `Err(e)` — the parser only saw the form;
                // the binding registers like an identifier pattern and its
                // type (the payload T or E) is determined by typeck (U04).
                let (binding_id, hir_id) = self.declare_pattern_binding(&binding, span)?;
                Ok(PatternHir::ResultCtor {
                    ctor,
                    name: binding,
                    binding_id,
                    hir_id,
                    span,
                })
            }
        }
    }

    /// Register a pattern binding (identifier or Result-payload binding) in
    /// the arm's scope. Shadow-check up to the function boundary; parameters,
    /// functions and builtins are exempt (DESIGN.md §4.3).
    ///
    /// A `ref` parameter is not merely exempt — re-declaring its name is an
    /// `AssignToRefParam` error (0.0.2 U05, DESIGN §10.4), mirroring the
    /// `s = …` shadow case in `resolve_var_binding`.
    fn declare_pattern_binding(
        &mut self,
        name: &str,
        span: Span,
    ) -> Result<(BindingId, HirId), ()> {
        let new_mutable = true; // pattern bindings are mutable
        if let Some(outer) = self.scopes.find_shadow_domain(name)
            && outer.ref_param
        {
            self.add_error(
                ResolveErrorKind::AssignToRefParam {
                    name: name.to_string(),
                },
                span,
            );
            return Err(());
        }
        if let Some(outer) = self.scopes.find_shadow_domain(name)
            && outer.kind.shadow_checks()
            && outer.mutable != new_mutable
        {
            self.add_error(
                ResolveErrorKind::ShadowingMutabilityMismatch {
                    outer_mutable: outer.mutable,
                    inner_mutable: new_mutable,
                },
                span,
            );
            return Err(());
        }

        let binding_id = self.next_binding_id();
        let hir_id = self.next_hir_id();
        if let Err(first) = self.scopes.declare(
            name.to_string(),
            Binding {
                id: binding_id,
                kind: BindingKind::Variable,
                mutable: true,
                builtin: false,
                ref_param: false,
                span,
                hir_id,
                global_name: None,
            },
        ) {
            self.add_error(
                ResolveErrorKind::DuplicateBinding {
                    name: name.to_string(),
                    first_span: first.span,
                },
                span,
            );
            return Err(());
        }

        Ok((binding_id, hir_id))
    }

    /// Resolve a block in a fresh `Block` scope.
    fn resolve_block(&mut self, block: Block) -> Result<BlockHir, ()> {
        self.resolve_block_inner(block, ScopeKind::Block)
    }

    /// Resolve a block in a fresh scope of the given kind, always leaving it.
    fn resolve_block_inner(&mut self, block: Block, kind: ScopeKind) -> Result<BlockHir, ()> {
        match kind {
            ScopeKind::Loop => self.scopes.enter_loop_scope(),
            _ => self.scopes.enter_block_scope(),
        }

        // Error recovery: a failed statement contributes an Error placeholder
        // so later statements still resolve (errors are collected, not
        // fail-fast).
        let mut hir_stmts = Vec::new();
        for stmt in block.stmts {
            let hir_stmt = match self.resolve_stmt(stmt) {
                Ok(s) => s,
                Err(()) => StmtHir::Error,
            };
            hir_stmts.push(hir_stmt);
        }

        let tail_result: Result<Option<Box<ExprHir>>, ()> = match block.tail_expr {
            Some(expr) => self.resolve_expr(*expr).map(|e| Some(Box::new(e))),
            None => Ok(None),
        };

        // Leave the scope *before* propagating a tail-expression failure.
        self.scopes.exit_scope();

        let tail_expr = tail_result?;
        Ok(BlockHir {
            stmts: hir_stmts,
            tail_expr,
            hir_id: self.next_hir_id(),
            span: block.span,
        })
    }

    fn resolve_stmt(&mut self, stmt: Stmt) -> Result<StmtHir, ()> {
        match stmt {
            Stmt::Decl(decl) => {
                // Nested declarations are never pre-declared top-level functions.
                let empty = HashMap::new();
                Ok(StmtHir::Decl(self.resolve_decl(decl, &empty)?))
            }
            Stmt::Expr(expr, has_semi) => {
                let hir_expr = self.resolve_expr(*expr)?;
                Ok(StmtHir::Expr(Box::new(hir_expr), has_semi))
            }
        }
    }
}
