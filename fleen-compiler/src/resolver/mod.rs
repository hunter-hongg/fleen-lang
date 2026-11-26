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
    BindingId, BlockHir, ChooseArmHir, ConstDeclHir, DeclHir, EnumDeclHir, EnumVariantHir,
    ExprChooseHir, ExprForHir, ExprHir, ExprIfHir, ExprWhileHir, FuncBodyHir, FuncDeclHir, Hir,
    HirId, HirItem, ImportDeclHir, InterpPartHir, ParamHir, PatternHir, StmtHir, StructDeclHir,
    StructFieldHir, VarBindingHir,
};
pub use modules::{
    CrateDef, CrateId, ExportKind, ModuleDef, ModuleId, ModuleTable, ROOT_CRATE, STD_CRATE_NAME,
    STD_RESULT_MODULE_NAME,
};
use scope::{Binding, BindingKind, ScopeKind, ScopeStack};
use std::collections::HashMap;

/// Builtin functions available in every program (registered in the global scope).
///
/// `print` is the only builtin *function*. `Ok`/`Err` are the prelude
/// variant-constructor names of the std-declared `Result` enum (0.0.10 D2):
/// they resolve like functions here, while typeck gives them their
/// context-dependent constructor semantics (0.0.2 U04, PLAN §3.4.1). From
/// 0.0.10 T02 (D2) the trio `Ok` / `Err` / `Result` cannot be shadowed —
/// see [`Resolver::check_reserved_binding_name`].
const BUILTINS: &[&str] = &["print", "Ok", "Err", "range", "range_from"];

/// The canonical module path of the std-declared `Result` enum (0.0.10 D1):
/// the compiler-recognized builtin path every `Result<T, E>` type use and
/// `Ok` / `Err` constructor reference maps onto. A user module can never
/// contribute a module of this path (the `std` crate name is reserved), so
/// comparing an enum's mangled name against this constant identifies the
/// builtin declaration unambiguously.
const BUILTIN_RESULT_PATH: &str = "std.result.Result";

/// The prelude constructor names of the builtin `Result` (0.0.10 D2). These
/// are reserved binding names: declaring a binding of any of them is a
/// `ReservedBuiltinName` rejection. (`Result` itself is covered at parse
/// level — it stays a lexer keyword — and at declaration level for `enum
/// Result` via `collect_enums`, T01.)
const RESERVED_CTOR_NAMES: &[&str] = &["Ok", "Err"];

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
        Type::Future(inner) => type_contains_ref(inner),
        Type::Channel(inner) => type_contains_ref(inner),
        Type::Ptr(inner) => type_contains_ref(inner),
        Type::Map { key, value } => type_contains_ref(key) || type_contains_ref(value),
        Type::Named { args, .. } => args.iter().any(type_contains_ref),
        Type::Range => false,
        Type::Base(_) => false,
    }
}

/// Assignment target shape, split out of the AST before binding lookup.
enum AssignTarget {
    Ident(String),
    /// `deref b` — write the pointee of the box held by `b`.
    Deref(String),
    /// `deref_ptr p` — write through the pointer held by `p` (0.0.5 T02).
    DerefPtr(String),
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
    table
        .crates
        .push(CrateDef::root_crate(crate::source_map::FileId(0)));
    table.modules.push(ModuleDef {
        file: crate::source_map::FileId(0),
        path: Vec::new(),
        crate_id: ROOT_CRATE,
        is_crate_root: true,
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
    /// 0.0.6 T03: enums declared in the current module, keyed by bare name.
    /// Populated by a pre-pass before expression resolution so a constructor
    /// path resolves regardless of declaration order (like global functions).
    enum_table: HashMap<String, EnumDeclHir>,
    /// 0.0.6 T04 (D3): variant name → declaring enum name, built from the
    /// same enum pre-pass. Backs bare-variant construction (`Circle`,
    /// `Point`): a bare identifier that names a variant and is not shadowed
    /// by a local binding, function, const or var resolves to an enum
    /// constructor, exactly as the dotted `Shape.Circle` spelling does.
    /// Per-enum variant names are already unique (checked at registration),
    /// but two enums may reuse a variant name, so this map is first-wins by
    /// declaration order.
    variant_to_enum: HashMap<String, String>,
    /// 0.0.7 T03 (D1): declaration-local type-parameter scope for the enum
    /// body currently being registered. Set by `collect_enums` around one
    /// parameterized enum's payload-type resolution and cleared right after:
    /// the names are `Option<T>`'s `T` — not values, not bindings, not
    /// visible outside the declaration body.
    type_param_scope: Vec<String>,
    /// 0.0.8 T04 (D3): curried signature halves awaiting their body, keyed by
    /// name, with the item index the signature line appeared at. Registration
    /// is global (every signature is predeclared before any body resolves —
    /// recursion and forward reference unchanged), but *matching* enforces
    /// the source order: a body may only match a signature that precedes it.
    pending_sigs: HashMap<String, (CurriedSignature, usize)>,
    /// 0.0.8 T04: names whose signature was already consumed by a matched
    /// body, with the first body's span — a second body for the same name is
    /// a `DuplicateBinding`, not a fresh `BodyWithoutSignature`.
    matched_sigs: HashMap<String, Span>,
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
            enum_table: HashMap::new(),
            variant_to_enum: HashMap::new(),
            type_param_scope: Vec::new(),
            pending_sigs: HashMap::new(),
            matched_sigs: HashMap::new(),
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

    /// Report a name collision: the same `DuplicateBinding` shape for every
    /// registration site (module items, scope bindings, enum type parameters,
    /// enum variants), with the first declaration's span for diagnostics.
    fn push_duplicate(&mut self, name: &str, first_span: Span, span: Span) {
        self.add_error(
            ResolveErrorKind::DuplicateBinding {
                name: name.to_string(),
                first_span,
            },
            span,
        );
    }

    /// 0.0.10 T02 (D2): register the builtin `Result` declaration as a
    /// prelude name in every module's resolution-time tables, so `Ok` / `Err`
    /// classify as its variant constructors (patterns) whether or not the
    /// module spells `import std.result;` — the D2 prelude rule ("print +
    /// the Result trio").
    ///
    /// The synthetic declaration is exactly what the std `result` module's
    /// own pre-pass registers (same mangled name [`BUILTIN_RESULT_PATH`],
    /// same two variants), so `collect_enums` re-registering it there is a
    /// no-op overwrite. `variant_to_enum` is first-wins by registration
    /// order: seeding `Ok` / `Err` here — before any module declaration —
    /// keeps a user enum's `Ok` / `Err` variants unreachable through the
    /// bare spelling, which is precisely the pre-0.0.10 behavior (the
    /// parser's `Ok(`/`Err(` special case used to hijack the pattern first).
    /// The entry never reaches the output HIR: `collect_enums` excludes the
    /// std `result` module's declaration (T01), and this registration feeds
    /// only the classification lookup below.
    fn register_builtin_result_prelude(&mut self) {
        let param = |name: &str| Type::Named {
            name: name.to_string(),
            args: Vec::new(),
            span: Span::SYNTHETIC,
        };
        let variant = |name: &str, payload: Type| EnumVariantHir {
            name: name.to_string(),
            payload_types: vec![payload],
            span: Span::SYNTHETIC,
        };
        let decl = EnumDeclHir {
            name: BUILTIN_RESULT_PATH.to_string(),
            type_params: vec!["T".to_string(), "E".to_string()],
            variants: vec![variant("Ok", param("T")), variant("Err", param("E"))],
            span: Span::SYNTHETIC,
            // The prelude registration is a resolution-time seed, never an
            // export surface: the std.result module's own `pub` declaration
            // (T01) is the importable one.
            is_pub: false,
        };
        self.enum_table.insert("Result".to_string(), decl);
        for ctor in RESERVED_CTOR_NAMES {
            self.variant_to_enum
                .entry((*ctor).to_string())
                .or_insert_with(|| "Result".to_string());
        }
    }

    /// 0.0.10 T02 (D1): whether this resolved enum declaration is the builtin
    /// `std.result.Result` — the compiler-mapped std declaration whose
    /// variants are the prelude `Ok` / `Err` constructors.
    fn is_builtin_result_enum(decl: &EnumDeclHir) -> bool {
        decl.name == BUILTIN_RESULT_PATH
    }

    /// 0.0.10 T02 (D2): reject a binding declaration named `Ok` / `Err` —
    /// the prelude constructor names are not shadowable. The type name
    /// `Result` is covered elsewhere: it stays a lexer keyword (every other
    /// declaration position rejects it at parse level, as before 0.0.10)
    /// and `collect_enums` rejects `enum Result` outside `std.result` (T01).
    ///
    /// **Recorded behavior change (T02 as-built)**: before this milestone
    /// these names were shadowable (the §4.3 Python-style builtin overwrite;
    /// `Scope::declare` lets a user binding overwrite a builtin one). D2's
    /// normative rule is non-shadowable, so a program that shadowed `Ok` /
    /// `Err` — legal at the 0.0.9 merge-base — is now rejected. Every
    /// program *rejected* before is still rejected (the parity sweep's
    /// one-directional constraint); the flip is recorded in
    /// `docs/0.0.10/TICKETS.md` T02, never silent.
    ///
    /// This doc comment is the **canonical rationale** for the shadow flip:
    /// the call sites and tests point here instead of restating it (review
    /// note: one copy, not six).
    fn check_reserved_binding_name(&mut self, name: &str, span: Span) {
        if RESERVED_CTOR_NAMES.contains(&name) {
            self.add_error(
                ResolveErrorKind::ReservedBuiltinName {
                    name: name.to_string(),
                },
                span,
            );
        }
    }

    /// Resolve the payload types of one enum's variants (0.0.6 T04 /
    /// 0.0.7 T03). This is the ONLY place that installs
    /// `type_param_scope`: it is active exactly for payload-type resolution
    /// and is cleared before this returns, so no early path can leak the
    /// declaration-local parameters into other resolution.
    fn resolve_enum_variants(
        &mut self,
        params: &[String],
        variants: &[EnumVariant],
    ) -> Vec<EnumVariantHir> {
        debug_assert!(
            self.type_param_scope.is_empty(),
            "type-parameter scope must be empty outside payload resolution"
        );
        self.type_param_scope = params.to_vec();
        let resolved = variants
            .iter()
            .map(|v| EnumVariantHir {
                name: v.name.clone(),
                payload_types: v
                    .payload_types
                    .iter()
                    .map(|ty| self.resolve_type(ty, v.span))
                    .collect(),
                span: v.span,
            })
            .collect();
        self.type_param_scope.clear();
        resolved
    }

    /// Resolve type references for 0.0.3 V05: qualify bare struct names
    /// declared in the current module and, for already-qualified names,
    /// validate the target exists and check `pub` visibility. Composite
    /// types recurse so nested `Named` references are rewritten too.
    fn resolve_type(&mut self, ty: &Type, _span: Span) -> Type {
        // `_span` is only ever threaded through the recursive calls — every
        // `Type::Named` arm carries its own `ty_span`, and the base cases
        // clone. The parameter exists so callers can pass a location for the
        // future case where resolution itself needs to report one (clippy
        // `only_used_in_recursion` notes this; the signature is deliberate).
        match ty {
            Type::Array(elem) => Type::Array(Box::new(self.resolve_type(elem, _span))),
            Type::Box(inner) => Type::Box(Box::new(self.resolve_type(inner, _span))),
            Type::Ref(inner) => Type::Ref(Box::new(self.resolve_type(inner, _span))),
            Type::Result(ok, err) => Type::Result(
                Box::new(self.resolve_type(ok, _span)),
                Box::new(self.resolve_type(err, _span)),
            ),
            Type::Func(params, ret) => Type::Func(
                params.iter().map(|p| self.resolve_type(p, _span)).collect(),
                Box::new(self.resolve_type(ret, _span)),
            ),
            Type::Named {
                name,
                args,
                span: ty_span,
            } => {
                // 0.0.10 T02 (D1): a parameterized type use of the prelude
                // `Result` — the bare spelling `Result<T, E>` / `Result[T, E]`
                // (the parser produces exactly two arguments for the keyword
                // spelling; the check is defensive) — maps back onto the
                // internal `Type::Result`, so typeck / lower / codegen see
                // the same shape as before the migration (D1: the std
                // declaration is the language-level source of truth, the
                // backend keeps the 0.0.2 encoding).
                if name == "Result" && args.len() == 2 {
                    return Type::Result(
                        Box::new(self.resolve_type(&args[0], *ty_span)),
                        Box::new(self.resolve_type(&args[1], *ty_span)),
                    );
                }
                let resolved_name = self.resolve_type_name_str(name, *ty_span);
                let resolved_args = args
                    .iter()
                    .map(|a| self.resolve_type(a, *ty_span))
                    .collect::<Vec<_>>();
                Type::Named {
                    name: resolved_name,
                    args: resolved_args,
                    span: *ty_span,
                }
            }
            // 0.0.4 F3: `Future<T>` recurses so a nested named type resolves.
            Type::Future(inner) => Type::Future(Box::new(self.resolve_type(inner, _span))),
            // 0.0.4 F9: `channel<T>` recurses so the payload type resolves.
            Type::Channel(inner) => Type::Channel(Box::new(self.resolve_type(inner, _span))),
            // 0.0.5 T02: `ptr<T>` recurses so the pointee type resolves.
            Type::Ptr(inner) => Type::Ptr(Box::new(self.resolve_type(inner, _span))),
            // 0.0.11 T02: `map[K, V]` recurses so the key/value types resolve.
            Type::Map { key, value } => Type::Map {
                key: Box::new(self.resolve_type(key, _span)),
                value: Box::new(self.resolve_type(value, _span)),
            },
            Type::Base(_) | Type::Range => ty.clone(),
        }
    }

    fn resolve_type_name_str(&mut self, name: &str, span: Span) -> String {
        let module = self.table.modules[self.cur_module].clone();
        if !name.contains('.') {
            // 0.0.7 T03 (D1): a declaration-local type parameter shadows
            // module items — inside `enum Option<T> { Some(T) }` the payload
            // `T` is the parameter, not a module item named `T`. The name
            // stays unmangled (it is declaration-local); typeck (T04)
            // resolves it to `Type::Param(i)` by declaration order.
            if self.type_param_scope.iter().any(|p| p == name) {
                return name.to_string();
            }
            // A bare name is a struct declared in the current module; leave
            // unknown names for typeck to report with its full error text.
            if module.items.contains_key(name) {
                return module.mangled_name(name);
            }
            return name.to_string();
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
                        return name.to_string();
                    }
                    return target.mangled_name(item_name);
                }
            }
        }
        name.to_string()
    }

    /// Main entry point: resolve the entire program.
    fn resolve_program(self, ast: Ast) -> Result<Hir, Vec<ResolveError>> {
        self.resolve_program_inner(ast).map(|(hir, _, _)| hir)
    }

    /// Main entry point, also returning the next free binding/hir ID so a
    /// multi-module compiler can keep ID spaces disjoint (0.0.3 V05).
    fn resolve_program_inner(mut self, ast: Ast) -> Result<(Hir, u32, u32), Vec<ResolveError>> {
        self.register_builtins();
        // 0.0.10 T02 (D2): the prelude is `print` + the Result trio — seed
        // the builtin Result declaration before any module enum is
        // collected, so `Ok` / `Err` classify through it in every module.
        self.register_builtin_result_prelude();
        self.register_imported_items();
        self.predeclare_global_functions(&ast);
        let global_funcs = std::mem::take(&mut self.global_funcs);
        // 0.0.6 T03: collect enum declarations before expression resolution
        // so a constructor path resolves regardless of declaration order
        // (the same forward-reference rule global functions follow).
        let enums = self.collect_enums(&ast);

        let mut hir_items = Vec::new();
        let mut structs: Vec<StructDeclHir> = Vec::new();
        for (index, item) in ast.items.into_iter().enumerate() {
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
            // Enum declarations were collected by the pre-pass (`collect_enums`);
            // they are a type-table input for typeck (T04), not executable code.
            if let Item::Decl(Decl::Enum(_)) = &item {
                continue;
            }
            // 0.0.8 T04 (D3): a signature half was registered by the
            // predeclare pass; it contributes no HIR item of its own — the
            // matched body below becomes the ordinary function declaration.
            if let Item::Decl(Decl::CurriedSignature(_)) = &item {
                continue;
            }
            // A body half is matched to its preceding signature (source order
            // and arrow-count arity enforced, D3) and then resolved as an
            // ordinary `func` declaration: the signature carried the types
            // and modifiers, the body the parameter names and implementation.
            if let Item::Decl(Decl::CurriedBody(body)) = &item {
                if let Ok(func_decl) = self.match_curried_body(body, index)
                    && let Ok(hir_item) =
                        self.resolve_item(Item::Decl(Decl::Func(func_decl)), &global_funcs)
                {
                    hir_items.push(hir_item);
                }
                // On Err the error was already recorded by `add_error`; the
                // body contributes no placeholder because the HIR is
                // discarded anyway. Either way the half is consumed here —
                // it never falls through to the generic item resolution.
                continue;
            }
            if let Ok(hir_item) = self.resolve_item(item, &global_funcs) {
                hir_items.push(hir_item);
            }
            // On Err the error was already recorded by `add_error`; the item
            // contributes no placeholder because the HIR is discarded anyway.
        }

        // 0.0.8 T04 (D3): signatures still pending after the item pass never
        // met a body line (or their body was out of order). Sorted by span so
        // the collected error order is deterministic despite the HashMap.
        let mut orphans: Vec<(String, CurriedSignature)> = self
            .pending_sigs
            .drain()
            .map(|(name, (sig, _))| (name, sig))
            .collect();
        orphans.sort_by_key(|(_, sig)| (sig.span.start, sig.span.end));
        for (name, sig) in orphans {
            self.add_error(ResolveErrorKind::SignatureWithoutBody { name }, sig.span);
        }

        if !self.errors.is_empty() {
            return Err(self.errors);
        }

        Ok((
            Hir {
                items: hir_items,
                structs,
                enums,
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
                // Structs and enums have no runtime binding; their names are
                // resolved through the typeck struct/enum table.
                if matches!(item.kind, ExportKind::Struct | ExportKind::Enum) {
                    continue;
                }
                let binding_id = self.next_binding_id();
                let kind = match item.kind {
                    ExportKind::Func => BindingKind::Function,
                    ExportKind::Const => BindingKind::Const,
                    ExportKind::Global => BindingKind::Variable,
                    ExportKind::Struct | ExportKind::Enum => continue,
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
    ///
    /// 0.0.8 T04 (D3): a curried *signature* half registers here exactly as
    /// a `func` declaration does — name, binding, mangled global name — and
    /// additionally waits in `pending_sigs` (with its item index) for its
    /// body half to match against. Registration happens before any body
    /// resolves, so recursion and forward reference work unchanged.
    fn predeclare_global_functions(&mut self, ast: &Ast) {
        for (index, item) in ast.items.iter().enumerate() {
            let (name, span) = match item {
                Item::Decl(Decl::Func(func_decl)) => (&func_decl.name, func_decl.span),
                Item::Decl(Decl::CurriedSignature(sig)) => {
                    // Order data for the body match; a duplicate signature is
                    // reported below and the FIRST one stays pending.
                    self.pending_sigs
                        .entry(sig.name.clone())
                        .or_insert((sig.clone(), index));
                    (&sig.name, sig.span)
                }
                _ => continue,
            };
            // 0.0.10 T02 (D2): the prelude ctor names are not shadowable —
            // a `func Ok` (or curried signature `Ok: ...`) is rejected (see
            // `check_reserved_binding_name`).
            self.check_reserved_binding_name(name, span);
            let binding_id = self.next_binding_id();
            let module = self.table.modules[self.cur_module].clone();
            let global_name = module.mangled_name(name);
            let binding = Binding {
                id: binding_id,
                kind: BindingKind::Function,
                mutable: false, // function names are immutable bindings
                builtin: false,
                ref_param: false,
                span,
                hir_id: HirId(0), // placeholder, filled when the body resolves
                global_name: Some(global_name),
            };
            match self.scopes.declare(name.clone(), binding) {
                Ok(()) => {
                    self.global_funcs.insert(name.clone(), binding_id);
                }
                Err(first) => {
                    self.add_error(
                        ResolveErrorKind::DuplicateBinding {
                            name: name.clone(),
                            first_span: first.span,
                        },
                        span,
                    );
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
            // 0.0.9 T03 (D4): the keyword is a token kind — nothing to
            // resolve; typeck owns the position check.
            Item::Break(span) => HirItem::Break(span),
            Item::Continue(span) => HirItem::Continue(span),
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
            // 0.0.6 T03: enum declarations are intercepted by `resolve_program`
            // (they feed the typeck enum table, not `items`), and the parser
            // rejects enum declarations inside blocks, so this arm cannot run.
            Decl::Enum(_) => unreachable!("enum declarations never reach resolve_decl"),
            // 0.0.8 T04: curried halves are intercepted by `resolve_program`
            // (signature registration + body matching happen there), and the
            // parser accepts the two-part form at top level only, so these
            // arms cannot run.
            Decl::CurriedSignature(_) => {
                unreachable!("curried signature halves never reach resolve_decl")
            }
            Decl::CurriedBody(_) => {
                unreachable!("curried body halves never reach resolve_decl")
            }
        }
    }

    /// Match a two-part curried *body* half to its signature (0.0.8 T04, D3)
    /// and synthesize the ordinary [`FuncDecl`] the rest of the pipeline
    /// resolves: the signature carries the types, the modifiers and `pub`
    /// (the interface half); the body carries the parameter names and the
    /// implementation.
    ///
    /// Errors (all recorded, `Err(())` returned):
    /// - the signature line has not been seen at the body's position — either
    ///   never (no signature at all) or out of order (body before its
    ///   signature; source order is enforced, D3) → `BodyWithoutSignature`;
    /// - the signature was already consumed by an earlier body →
    ///   `DuplicateBinding` (the same flat-namespace rule as every
    ///   declaration);
    /// - the signature's parameter count disagrees with the body's parameter
    ///   count → `SignatureArityMismatch`.
    fn match_curried_body(&mut self, body: &CurriedBody, index: usize) -> Result<FuncDecl, ()> {
        // Inspect the pending signature with a short immutable borrow so the
        // (expensive) parameter-type clone happens only on the success path —
        // error paths record their diagnostic from scalar data alone.
        enum SigMatch {
            /// No signature pending under this name: never declared, or an
            /// earlier body already consumed it.
            Missing,
            /// A signature is pending but sits after this body in the source
            /// (order is enforced, D3).
            OutOfOrder,
            /// The signature is a function type whose parameter count differs
            /// from the body's.
            Arity { sig_params: usize },
            /// The pair matches; the types travel to the synthesis below.
            Ready {
                param_tys: Vec<Type>,
                ret: Box<Type>,
            },
        }
        let outcome = match self.pending_sigs.get(&body.name) {
            None => SigMatch::Missing,
            Some((sig, sig_index)) => {
                if *sig_index >= index {
                    SigMatch::OutOfOrder
                } else {
                    // The parser guarantees a signature's type is a function
                    // type (`SignatureNeedsFunctionType` at parse time), so
                    // this match cannot fail on parser-produced ASTs.
                    match &sig.ty {
                        Type::Func(params, ret) => {
                            if params.len() != body.params.len() {
                                SigMatch::Arity {
                                    sig_params: params.len(),
                                }
                            } else {
                                SigMatch::Ready {
                                    param_tys: params.clone(),
                                    ret: ret.clone(),
                                }
                            }
                        }
                        _ => unreachable!(
                            "parser guarantees a curried signature's type is a function type (`SignatureNeedsFunctionType`)"
                        ),
                    }
                }
            }
        };

        match outcome {
            SigMatch::Missing => {
                if let Some(first_body) = self.matched_sigs.get(&body.name) {
                    self.add_error(
                        ResolveErrorKind::DuplicateBinding {
                            name: body.name.clone(),
                            first_span: *first_body,
                        },
                        body.span,
                    );
                } else {
                    self.add_error(
                        ResolveErrorKind::BodyWithoutSignature {
                            name: body.name.clone(),
                        },
                        body.span,
                    );
                }
                Err(())
            }
            SigMatch::OutOfOrder => {
                self.add_error(
                    ResolveErrorKind::BodyWithoutSignature {
                        name: body.name.clone(),
                    },
                    body.span,
                );
                // The pair is already reported through its specific error —
                // consume the signature so it is not *also* surfaced as
                // `SignatureWithoutBody` by the end-of-pass orphan sweep
                // (the same no-double-report rule as the arity path).
                self.consume_curried_sig(&body.name, body.span);
                Err(())
            }
            SigMatch::Arity { sig_params } => {
                self.add_error(
                    ResolveErrorKind::SignatureArityMismatch {
                        name: body.name.clone(),
                        sig_params,
                        body_params: body.params.len(),
                    },
                    body.span,
                );
                // The body did find its signature — the pair is matched, just
                // not arity-consistent. Consume the signature so the pair is
                // not *also* reported as orphan halves.
                self.consume_curried_sig(&body.name, body.span);
                Err(())
            }
            SigMatch::Ready { param_tys, ret } => {
                let (sig, _) = self.pending_sigs.remove(&body.name).unwrap_or_else(|| {
                    unreachable!("signature was just inspected in `pending_sigs`")
                });
                self.matched_sigs.insert(body.name.clone(), body.span);
                Ok(FuncDecl {
                    name: sig.name,
                    params: body
                        .params
                        .iter()
                        .zip(param_tys)
                        .map(|(p, ty)| Param {
                            name: p.name.clone(),
                            ty,
                            span: p.span,
                        })
                        .collect(),
                    ret_type: Some(*ret),
                    body: body.body.clone(),
                    span: body.span,
                    is_pub: sig.is_pub,
                    is_async: sig.mods.is_async,
                    is_unsafe: sig.mods.is_unsafe,
                    is_trusted: sig.mods.is_trusted,
                })
            }
        }
    }

    /// Consume a matched-but-rejected signature half (out-of-order body or
    /// arity mismatch): the pair was reported through its specific error, so
    /// the signature must not also surface as `SignatureWithoutBody` in the
    /// end-of-pass orphan sweep. Recording the span in `matched_sigs` also
    /// makes a *later* duplicate body a `DuplicateBinding` instead of a
    /// second match against a half the source already rejected.
    fn consume_curried_sig(&mut self, name: &str, span: Span) {
        self.pending_sigs.remove(name);
        self.matched_sigs.insert(name.to_string(), span);
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
            is_async: func.is_async,
            is_unsafe: func.is_unsafe,
            is_trusted: func.is_trusted,
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
            // 0.0.10 T02 (D2): a parameter named `Ok` / `Err` is rejected
            // (see `check_reserved_binding_name`).
            self.check_reserved_binding_name(&param.name, param.span);
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
        // 0.0.10 T02 (D2): a binding named `Ok` / `Err` is rejected (see
        // `check_reserved_binding_name`).
        self.check_reserved_binding_name(&var.name, var.span);
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
        // 0.0.10 T02 (D2): a `const` named `Ok` / `Err` is rejected (see
        // `check_reserved_binding_name`).
        self.check_reserved_binding_name(&c.name, c.span);
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
            // 0.0.5 T02 (D2): `addr <place>` — the place shape and the base
            // binding's mutability/borrowedness are validated here (they are
            // binding-table properties); the type-level rules are typeck's.
            Expr::Addr(operand, span) => self.resolve_addr(*operand, span)?,
            Expr::DerefPtr(expr, span) => {
                ExprHir::DerefPtr(Box::new(self.resolve_expr(*expr)?), span)
            }
            // 0.0.2 U13: type cast; operand resolved, target type passed through as-is.
            Expr::Cast(expr, ty, span) => {
                let resolved_ty = self.resolve_type(&ty, span);
                ExprHir::Cast(Box::new(self.resolve_expr(*expr)?), resolved_ty, span)
            }
            Expr::Question(expr, span) => {
                ExprHir::Question(Box::new(self.resolve_expr(*expr)?), span)
            }
            Expr::Await(expr, span) => ExprHir::Await(Box::new(self.resolve_expr(*expr)?), span),
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
            Expr::MapLit(lit) => {
                let pairs = lit
                    .pairs
                    .into_iter()
                    .map(|(k, v)| {
                        let k = self.resolve_expr(k)?;
                        let v = self.resolve_expr(v)?;
                        Ok((k, v))
                    })
                    .collect::<Result<Vec<_>, ()>>()?;
                ExprHir::MapLit(pairs, lit.span)
            }
            Expr::Field(obj, field) => {
                let span = obj.span();
                if let Some(hir) = self.try_resolve_enum_variant(&obj, &field, span)? {
                    hir
                } else if let Some(hir) = self.try_resolve_module_path(&obj, &field, span)? {
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
            // 0.0.10 T05 (D4): each embedded expression resolves in the
            // enclosing scope — no new resolution rule; the stringly-typed
            // conversion rule is typeck's (F2, D5).
            Expr::Interp(interp) => {
                let parts = interp
                    .parts
                    .into_iter()
                    .map(|p| match p {
                        crate::parser::ast::InterpPart::Lit(s) => Ok(InterpPartHir::Lit(s)),
                        crate::parser::ast::InterpPart::Expr(e, span) => {
                            let hir = self.resolve_expr(e)?;
                            Ok(InterpPartHir::Expr(Box::new(hir), span))
                        }
                    })
                    .collect::<Result<Vec<_>, ()>>()?;
                ExprHir::Interp(parts, interp.span)
            }
            Expr::Ident(name, span) => self.resolve_ident(name, span)?,
            Expr::ChannelNew { ty, capacity, span } => {
                let capacity_hir = self.resolve_expr(*capacity)?;
                let ty_hir = self.resolve_type(&ty, span);
                ExprHir::ChannelNew {
                    ty: ty_hir,
                    capacity: Box::new(capacity_hir),
                    hir_id: self.next_hir_id(),
                    span,
                }
            }
            Expr::Block(block) => ExprHir::Block(self.resolve_block(block)?),
        };
        Ok(hir)
    }

    /// Resolve `addr <place>` (0.0.5 T02, D2).
    ///
    /// The place must be a one-level storage slot, the same shape family as
    /// the write targets (V02/V03): a local/global binding, a field or
    /// element of one, or a box interior (`deref b`). Deeper paths
    /// (`p.f.g`, `a[i][j]`) fall into the invalid-place case.
    ///
    /// Writing through the resulting pointer would mutate the referent, so
    /// the binding-level write protections the ptr write cannot see are
    /// enforced here at the source: an immutable (const) base binding is
    /// rejected (`AddrOfImmutable`), and a `ref` parameter is read-only in
    /// all forms (`AddrOfRefParam`, DESIGN §10.4). A box interior stays
    /// writable regardless of the box binding's mutability (same rule as
    /// `deref b = v`), but never through a borrowed box.
    ///
    /// Whether the referent is later moved away is the unsafe author's
    /// contract (D2) — no aliasing analysis happens here or in typeck.
    fn resolve_addr(&mut self, operand: Expr, span: Span) -> Result<ExprHir, ()> {
        // The base binding behind the place, if the shape is addressable.
        // Only these four shapes are places; everything else — a call
        // result, a literal, a deeper path — is rejected here.
        let base: Option<(&str, bool)> = match &operand {
            Expr::Ident(name, _) => Some((name, false)),
            Expr::Field(obj, _) => match &**obj {
                Expr::Ident(name, _) => Some((name, false)),
                _ => None,
            },
            Expr::Index(obj, _, _) => match &**obj {
                Expr::Ident(name, _) => Some((name, false)),
                _ => None,
            },
            // Box interior (`deref b`): writable regardless of `b`'s
            // mutability (same rule as `deref b = v`), so `immutable` is
            // irrelevant here — but never through a borrowed box.
            Expr::Deref(inner, _) => match &**inner {
                Expr::Ident(name, _) => Some((name, true)),
                _ => None,
            },
            _ => None,
        };
        let Some((name, box_interior)) = base else {
            self.add_error(ResolveErrorKind::AddrOfInvalidPlace, operand.span());
            return Err(());
        };

        let Some(binding) = self.scopes.get(name) else {
            self.add_error(
                ResolveErrorKind::UndeclaredVariable {
                    name: name.to_string(),
                },
                span,
            );
            return Err(());
        };
        if !box_interior && !binding.mutable {
            self.add_error(
                ResolveErrorKind::AddrOfImmutable {
                    name: name.to_string(),
                },
                span,
            );
            return Err(());
        }
        if binding.ref_param {
            self.add_error(
                ResolveErrorKind::AddrOfRefParam {
                    name: name.to_string(),
                },
                span,
            );
            return Err(());
        }

        // Resolve the place as an ordinary expression: typeck types it (the
        // place type becomes the pointee type `T` of `ptr<T>`).
        let place_hir = self.resolve_expr(operand)?;
        Ok(ExprHir::Addr {
            place: Box::new(place_hir),
            hir_id: self.next_hir_id(),
            span,
        })
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
            // 0.0.5 T02: one-level write through a pointer `deref_ptr p = v`.
            // The one-level rule is enforced here at the cheapest stage: the
            // receiver must be a bare binding, so `deref_ptr p.f` falls into
            // the invalid-target case below (same shape rule as `deref b`).
            Expr::DerefPtr(inner, span) => match *inner {
                Expr::Ident(name, _) => (AssignTarget::DerefPtr(name), span),
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
            | AssignTarget::DerefPtr(name)
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
            AssignTarget::DerefPtr(_) => Ok(ExprHir::AssignDerefPtr {
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

    /// The scope-first bare-name lookup shared by the value side
    /// (`resolve_ident`) and the pattern side (`is_zero_payload_variant`,
    /// 0.0.9 T01): when no binding of `name` is visible on the scope chain,
    /// the name may still denote a declared enum variant (0.0.6 T04, D3).
    /// Returns the owning enum's declaration and the variant so the value
    /// side can build a constructor and the pattern side can ask for zero
    /// payloads — pattern and value positions agree on what a bare name can
    /// refer to because both walk this one lookup. The registry and the enum
    /// table are populated together, so a registry hit always finds its
    /// declaration; the `?` chain keeps the lookup total regardless.
    fn lookup_unshadowed_variant(&self, name: &str) -> Option<(&EnumDeclHir, &EnumVariantHir)> {
        if self.scopes.get(name).is_some() {
            return None;
        }
        let enum_name = self.variant_to_enum.get(name)?;
        let decl = self.enum_table.get(enum_name)?;
        let variant = decl.variants.iter().find(|v| v.name == name)?;
        Some((decl, variant))
    }

    /// Resolve an identifier expression (variable or function reference).
    /// The recorded span is the *use* site, not the definition site.
    fn resolve_ident(&mut self, name: String, span: Span) -> Result<ExprHir, ()> {
        // 0.0.6 T04 (D3): a bare identifier that names an enum variant and is
        // not shadowed by a local binding, function, const or var resolves to
        // that variant's constructor, exactly as the dotted `Shape.Circle`
        // spelling does. The scope-first order lives in
        // `lookup_unshadowed_variant`, shared with the pattern side (0.0.9
        // T01) so both positions classify a bare name identically.
        if let Some((decl, variant)) = self.lookup_unshadowed_variant(&name) {
            let enum_name = decl.name.clone();
            let payload_types = variant.payload_types.clone();
            let arity = payload_types.len();
            return Ok(ExprHir::EnumCtor {
                enum_name,
                variant_name: name,
                payload_types,
                arity,
                hir_id: self.next_hir_id(),
                span,
            });
        }
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

    /// 0.0.10 T01 (D1): whether this module is the shipped std `result`
    /// module — the only place `enum Result` may be declared. The
    /// declaration's mangled name is exactly `std.result.Result`, the
    /// canonical builtin path the compiler maps onto its internal
    /// `Type::Result`; user crates cannot contribute a module of that path
    /// (the `std` crate name is reserved), so the crate check keeps a
    /// project's own `src/std/result.fln` from ever claiming the name.
    fn is_std_result_module(&self, module: &ModuleDef) -> bool {
        module.path.len() == 2
            && module.path[0] == STD_CRATE_NAME
            && module.path[1] == STD_RESULT_MODULE_NAME
            && self
                .table
                .crates
                .get(module.crate_id)
                .is_some_and(|c| c.name == STD_CRATE_NAME)
    }

    /// 0.0.6 T03 pre-pass: collect every top-level `enum` declaration in
    /// declaration order, mangling names for the module table and checking
    /// collisions (duplicate enum, enum vs function/var, variant names).
    /// Returns the HIR enum table for typeck (T04).
    fn collect_enums(&mut self, ast: &Ast) -> Vec<EnumDeclHir> {
        let module = self.table.modules[self.cur_module].clone();
        let declaring_std_result = self.is_std_result_module(&module);
        let mut enums: Vec<EnumDeclHir> = Vec::new();
        for item in &ast.items {
            let Item::Decl(Decl::Enum(e)) = item else {
                continue;
            };
            // 0.0.10 T01 (D2 groundwork): `Result` names the std.result
            // builtin type and cannot be shadowed or re-declared. The
            // declaration inside `std/result.fln` is the only legal one;
            // everywhere else it is rejected. (Before 0.0.10 the `Result`
            // keyword made this a parse error — the rejection verdict is
            // unchanged, only its stage moved here.)
            if e.name == "Result" && !declaring_std_result {
                self.errors.push(ResolveError {
                    kind: ResolveErrorKind::ReservedBuiltinName {
                        name: e.name.clone(),
                    },
                    span: e.span,
                });
                continue;
            }
            let mangled = module.mangled_name(&e.name);
            if let Some(first) = enums.iter().find(|d| d.name == mangled) {
                self.errors.push(ResolveError {
                    kind: ResolveErrorKind::DuplicateBinding {
                        name: mangled.clone(),
                        first_span: first.span,
                    },
                    span: e.span,
                });
                continue;
            }
            // An enum name shares the flat identifier namespace with
            // functions, structs and other enums (D3 locks). The module
            // table's item map is the first source checked, but it is built
            // last-wins (one entry per name), so an enum that shares a name
            // with a *later* struct would otherwise slip through: the map
            // would hold the enum, the struct pre-pass never consults enums,
            // and the clash is silently accepted. To catch enum-vs-struct in
            // both declaration orders we check the raw AST directly. (Struct
            // and enum names never enter the binding scope, so func/const/var
            // clashes are still visible through the item map below.)
            let struct_clash = ast
                .items
                .iter()
                .any(|item| matches!(item, Item::Decl(Decl::Struct(s)) if s.name == e.name));
            let collides = struct_clash
                || module
                    .items
                    .get(&e.name)
                    .is_some_and(|item| item.kind != crate::resolver::modules::ExportKind::Enum);
            if collides {
                let first_span = if struct_clash {
                    ast.items
                        .iter()
                        .find_map(|item| match item {
                            Item::Decl(Decl::Struct(s)) if s.name == e.name => Some(s.span),
                            _ => None,
                        })
                        .unwrap_or(e.span)
                } else {
                    module
                        .items
                        .get(&e.name)
                        .map(|item| item.span)
                        .unwrap_or(e.span)
                };
                self.push_duplicate(&e.name, first_span, e.span);
                continue;
            }
            // A top-level function or variable already bound in the global
            // scope is the same collision, seen from the binding side.
            if let Some(first) = self.scopes.get(&e.name) {
                self.push_duplicate(&e.name, first.span, e.span);
                continue;
            }
            // Variant payload types obey the same `ref` rule as struct
            // fields (D1: payloads obey struct-field storage rules).
            for variant in &e.variants {
                for ty in &variant.payload_types {
                    if type_contains_ref(ty) {
                        self.errors.push(ResolveError {
                            kind: ResolveErrorKind::RefNotAllowedHere {
                                context: "enum payload type",
                            },
                            span: variant.span,
                        });
                    }
                }
            }
            // 0.0.7 T03 (D1): register the declaration-local type parameters.
            // Duplicate parameter names are a registration error (the same
            // DuplicateBinding rule as every other name collision); the
            // names do NOT enter the binding scope — they exist only so the
            // payload types below resolve their bare references to the
            // parameter, shadowing any module item of the same name. The
            // diagnostic names the first occurrence's span (the parameter's
            // own identifier), and the error points at the duplicate.
            let mut params: Vec<String> = Vec::with_capacity(e.type_params.len());
            let mut duplicate_param: Option<(String, Span, Span)> = None;
            for p in &e.type_params {
                if params.contains(&p.name) {
                    let first_span = e
                        .type_params
                        .iter()
                        .find(|q| q.name == p.name)
                        .map(|q| q.span)
                        .unwrap_or(e.span);
                    duplicate_param = Some((p.name.clone(), first_span, p.span));
                    break;
                }
                params.push(p.name.clone());
            }
            if let Some((name, first_span, span)) = duplicate_param {
                self.push_duplicate(&name, first_span, span);
                continue;
            }
            // Duplicate variant names within one enum are a registration
            // error (D3: the variant namespace is per-enum, checked here).
            let variants = self.resolve_enum_variants(&params, &e.variants);
            for (i, v) in variants.iter().enumerate() {
                if let Some(first) = variants[..i].iter().find(|p| p.name == v.name) {
                    self.push_duplicate(&v.name, first.span, v.span);
                }
            }
            let decl = EnumDeclHir {
                name: mangled,
                type_params: params.clone(),
                variants,
                span: e.span,
                is_pub: e.is_pub,
            };
            self.enum_table.insert(e.name.clone(), decl.clone());
            // Seed the variant → enum map for bare-variant construction
            // (0.0.6 T04, D3). A later declaration with the same variant name
            // does not overwrite an earlier one: the map is first-wins by
            // declaration order, matching the flat-namespace rule (the
            // per-enum duplicate check above already rejects repeats within a
            // single enum; only cross-enum reuse reaches here).
            for v in &decl.variants {
                self.variant_to_enum
                    .entry(v.name.clone())
                    .or_insert_with(|| e.name.clone());
            }
            // 0.0.10 T01 (D1): the std `Result` declaration is
            // declaration-only. It stays in the resolution-time tables (the
            // 0.0.9 D1 variant-reference machinery classifies `Ok` / `Err`
            // through it from T02 on), but it never enters the output HIR —
            // typeck/lower/codegen see no entry, so no `enum_defs` row is
            // emitted and the 0.0.2 Result encoding stays the runtime form.
            if !declaring_std_result {
                enums.push(decl);
            }
        }
        enums
    }

    /// Try to resolve a dotted path whose final segment is an enum variant
    /// constructor (`Color.Red`, `shapes.Color.Red`). Returns None when the
    /// path is not an enum reference (the caller falls through to module
    /// path / field access).
    fn try_resolve_enum_variant(
        &mut self,
        obj: &Expr,
        field: &str,
        span: Span,
    ) -> Result<Option<ExprHir>, ()> {
        // The path is `Enum.Variant` or `module.Enum.Variant`: the base
        // expression holds everything left of the variant segment.
        let base_segments: Vec<String> = match obj {
            Expr::Ident(name, _) => vec![name.clone()],
            Expr::Field(inner, f) => {
                let mut segs = vec![f.clone()];
                let mut cur: &Expr = inner;
                loop {
                    match cur {
                        Expr::Ident(name, _) => {
                            segs.push(name.clone());
                            break;
                        }
                        Expr::Field(i2, f2) => {
                            segs.push(f2.clone());
                            cur = i2;
                        }
                        _ => return Ok(None),
                    }
                }
                segs.reverse();
                segs
            }
            _ => return Ok(None),
        };
        // A local binding shadows an enum name.
        if base_segments.iter().any(|s| self.scopes.get(s).is_some()) {
            return Ok(None);
        }
        // Same-module enum: the base is exactly the enum name.
        if base_segments.len() == 1 {
            let enum_name = &base_segments[0];
            let Some(decl) = self.enum_table.get(enum_name) else {
                return Ok(None);
            };
            let Some(variant) = decl.variants.iter().find(|v| v.name == *field) else {
                self.add_error(
                    ResolveErrorKind::UndeclaredVariable {
                        name: format!("{}.{}", enum_name, field),
                    },
                    span,
                );
                return Err(());
            };
            return Ok(Some(ExprHir::EnumCtor {
                enum_name: decl.name.clone(),
                variant_name: field.to_string(),
                payload_types: variant.payload_types.clone(),
                arity: variant.payload_types.len(),
                hir_id: self.next_hir_id(),
                span,
            }));
        }
        // Cross-module enum: `module.Enum.Variant` — the module prefix must
        // be imported and the enum `pub` (checked in the module table).
        let (mid, prefix_len) = match self.table.longest_module_prefix(&base_segments) {
            Some(found) => found,
            None => return Ok(None),
        };
        // The base must be exactly `<module prefix>.<Enum>`.
        if prefix_len + 1 != base_segments.len() {
            return Ok(None);
        }
        let enum_name = &base_segments[prefix_len];
        let target = &self.table.modules[mid];
        let module = self.table.modules[self.cur_module].clone();
        let is_imported = mid == self.cur_module
            || module
                .imports
                .iter()
                .any(|imp| imp.path.iter().eq(target.path.iter()));
        if !is_imported {
            return Ok(None);
        }
        // The enum must be exported by the module's item table and pub.
        let Some(item) = target.items.get(enum_name) else {
            return Ok(None);
        };
        if !matches!(item.kind, crate::resolver::modules::ExportKind::Enum) {
            return Ok(None);
        }
        if !item.is_pub && mid != self.cur_module {
            self.add_error(
                ResolveErrorKind::PrivateAccess {
                    item: enum_name.clone(),
                    module: target.path_string(),
                },
                span,
            );
            return Err(());
        }
        // The variant list travels with the export (the module table is
        // built from the raw AST), so a cross-module constructor path
        // resolves against the *importing* module's view of the enum.
        if !item.enum_variants.iter().any(|v| v == field) {
            self.add_error(
                ResolveErrorKind::UndeclaredVariable {
                    name: format!("{}.{}", enum_name, field),
                },
                span,
            );
            return Err(());
        }
        Ok(Some(ExprHir::EnumCtor {
            enum_name: target.mangled_name(enum_name),
            variant_name: field.to_string(),
            // Cross-module payload types are converted by typeck (T04) from
            // the enum's own declaration; the resolver does not duplicate
            // the payload list across the module boundary.
            payload_types: Vec::new(),
            arity: 0,
            hir_id: self.next_hir_id(),
            span,
        }))
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
        // 0.0.10 T02 (D2): the loop variable is a fresh binding, so `for
        // Ok in …` is rejected (see `check_reserved_binding_name`); the
        // diagnostic points at the loop variable, not the whole `for`.
        self.check_reserved_binding_name(&for_expr.var, for_expr.var_span);
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
                // 0.0.9 T01 (BUG-2, PLAN D1): a bare identifier pattern is a
                // *variant reference* — not a binding — iff no binding of the
                // name is visible (same-pattern earlier bindings and enclosing
                // locals all live on the scope chain) and the name is a
                // zero-payload variant of a reachable enum. The scope-first
                // precedence lives in `lookup_unshadowed_variant`, the same
                // lookup `resolve_ident` applies on the value side, so the two
                // positions always agree. The name then never enters the
                // pattern's binding namespace, so `when P(None, None)` no
                // longer trips `DuplicateBinding`. The HIR shape is unchanged:
                // typeck reclassifies the unshadowed bare identifier into
                // `TypedPatternHir::EnumVariant` via the scrutinee's type (the
                // 0.0.6 T05 path), so coverage and lowering are unchanged.
                if self.is_zero_payload_variant(&name) {
                    // The ids are still allocated (ids stay contiguous) so a
                    // typeck fall-back to the binding path (a scrutinee of a
                    // non-enum type) keeps a coherent `binding_id`.
                    let binding_id = self.next_binding_id();
                    let hir_id = self.next_hir_id();
                    return Ok(PatternHir::Ident {
                        name,
                        binding_id,
                        hir_id,
                        span,
                    });
                }
                let (binding_id, hir_id) = self.declare_pattern_binding(&name, span)?;
                Ok(PatternHir::Ident {
                    name,
                    binding_id,
                    hir_id,
                    span,
                })
            }
            Pattern::Enum {
                name,
                payloads,
                span,
            } => {
                // 0.0.10 T02 (D1): the prelude `Ok(p)` / `Err(p)` patterns
                // arrive as ordinary constructor patterns and classify back
                // onto the ResultCtor-tagged HIR nodes through the same
                // resolution-time tables every user variant goes through
                // (the 0.0.9 D1 variant-reference machinery; the prelude
                // registration is `register_builtin_result_prelude`). A user
                // binding of the name shadows the classification (it cannot
                // exist — D2 rejects the declaration — so the check is
                // defensive), and a payload count other than one falls
                // through to the generic enum-pattern path: typeck then
                // reports the mismatch against the scrutinee type, which
                // replaces the pre-0.0.10 parse-level rejection for
                // `Ok(a, b)` / `Ok()` (recorded in the T02 card).
                let is_prelude_ctor = RESERVED_CTOR_NAMES.contains(&name.as_str())
                    && self.scopes.get(&name).is_none_or(|b| b.builtin)
                    && self
                        .variant_to_enum
                        .get(&name)
                        .and_then(|enum_name| self.enum_table.get(enum_name))
                        .is_some_and(Self::is_builtin_result_enum);
                if is_prelude_ctor && payloads.len() == 1 {
                    let hir_id = self.next_hir_id();
                    let ctor = if name == "Ok" {
                        ResultCtor::Ok
                    } else {
                        ResultCtor::Err
                    };
                    // `payloads.len() == 1` holds (checked above), so the
                    // iterator yields exactly one payload — the `else` is an
                    // invariant violation (SPEC §4), not a user error. The
                    // move replaces the old `payload.clone()` (review note,
                    // SPEC §9).
                    let Some(payload) = payloads.into_iter().next() else {
                        unreachable!("single payload checked above");
                    };
                    let payload_hir = self.resolve_pattern(payload)?;
                    return Ok(PatternHir::ResultCtor {
                        ctor,
                        payload: Box::new(payload_hir),
                        hir_id,
                        span,
                    });
                }
                let hir_id = self.next_hir_id();
                let mut payloads_hir = Vec::with_capacity(payloads.len());
                for p in payloads {
                    payloads_hir.push(self.resolve_pattern(p)?);
                }
                Ok(PatternHir::Enum {
                    name,
                    payloads: payloads_hir,
                    hir_id,
                    span,
                })
            }
            Pattern::Or(alts, span) => {
                // 0.0.7 T05 (D6): alternatives may share binding names
                // (`Some(v) | Other(v)`), so each alternative is resolved in
                // its own child scope and its bindings are then merged into
                // the arm scope — the first alternative to bind a name wins
                // the `BindingId`, later same-name declarations are skipped
                // (cross-alternative duplicates are expected, not an error;
                // the same-names/same-types rule is typeck's
                // `OrPatternBindingMismatch`). Duplicates *within* one
                // alternative (`Pair(a, a)`) still surface as
                // `DuplicateBinding` inside the child scope.
                let mut alts_hir = Vec::with_capacity(alts.len());
                for alt in alts {
                    self.scopes.enter_block_scope();
                    let alt_hir = self.resolve_pattern(alt)?;
                    let alt_bindings = self.scopes.current_bindings();
                    self.scopes.exit_scope();
                    for (name, binding) in alt_bindings {
                        // Ignore the re-declaration error: a name bound by an
                        // earlier alternative is already in the arm scope.
                        let _ = self.scopes.declare(name, binding);
                    }
                    alts_hir.push(alt_hir);
                }
                Ok(PatternHir::Or(alts_hir, span))
            }
            Pattern::TypeGuard { pattern, ty, span } => {
                let inner = self.resolve_pattern(*pattern)?;
                Ok(PatternHir::TypeGuard {
                    pattern: Box::new(inner),
                    ty,
                    span,
                })
            }
        }
    }

    /// Whether `name` is a zero-payload variant of an enum reachable in
    /// scope — the shared [`Self::lookup_unshadowed_variant`] lookup (the
    /// same `variant_to_enum` / `enum_table` pair the value-side
    /// bare-variant constructor resolution uses, 0.0.6 T04, D3), so pattern
    /// and value positions always agree on what a bare name can refer to.
    /// 0.0.9 T01 (BUG-2, PLAN D1).
    fn is_zero_payload_variant(&self, name: &str) -> bool {
        self.lookup_unshadowed_variant(name)
            .is_some_and(|(_, variant)| variant.payload_types.is_empty())
    }

    /// Register a pattern binding (identifier or Result-payload binding) in
    /// the arm's scope. Shadow-check up to the function boundary; parameters,
    /// functions and builtins are exempt (DESIGN.md §4.3).
    ///
    /// 0.0.10 T02 (D2): a pattern binding named `Ok` / `Err` is a
    /// `ReservedBuiltinName` rejection — the prelude constructor names are
    /// constructors, not bindable values (see `check_reserved_binding_name`).
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
        self.check_reserved_binding_name(name, span);
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
            // 0.0.9 T03 (D4): `break` / `continue` name nothing — the keyword
            // is already a token kind, so no identifier resolution applies.
            // The position check (inside a loop body) is typeck's.
            Stmt::Break(span) => Ok(StmtHir::Break(span)),
            Stmt::Continue(span) => Ok(StmtHir::Continue(span)),
        }
    }
}
