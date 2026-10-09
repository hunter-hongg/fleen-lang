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
//!   inside it do not shadow-check against enclosing scopes ("函数不生效").
//! - **Forward references**: function names are collected before bodies are
//!   resolved, so a function may be called before its declaration. Variables cannot.

pub mod error;
pub mod hir;
pub mod scope;

use crate::lexer::Span;
use crate::parser::ast::*;
use error::{ResolveError, ResolveErrorKind};
use hir::{
    BindingId, BlockHir, ChooseArmHir, ConstDeclHir, DeclHir, ExprChooseHir, ExprHir, ExprIfHir,
    ExprWhileHir, FuncBodyHir, FuncDeclHir, Hir, HirId, HirItem, ImportDeclHir, ParamHir,
    PatternHir, StmtHir, VarBindingHir,
};
use scope::{Binding, BindingKind, ScopeKind, ScopeStack};
use std::collections::HashMap;

/// Builtin functions available in every program (registered in the global scope).
///
/// `print` is the only builtin *function*. `Ok`/`Err` are builtin Result
/// constructors: they resolve like functions here (shadowable, §4.3), while
/// typeck gives them their context-dependent semantics (0.0.2 U04, PLAN §3.4.1).
const BUILTINS: &[&str] = &["print", "Ok", "Err"];

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
        Type::Base(_) => false,
    }
}

/// Assignment target shape, split out of the AST before binding lookup.
enum AssignTarget {
    Ident(String),
    /// `deref b` — write the pointee of the box held by `b`.
    Deref(String),
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
    Resolver::new().resolve_program(ast)
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
}

impl Resolver {
    fn new() -> Self {
        Self {
            scopes: ScopeStack::new(),
            errors: Vec::new(),
            hir_id_counter: 0,
            binding_id_counter: 0,
            global_funcs: HashMap::new(),
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

    /// Main entry point: resolve the entire program.
    fn resolve_program(mut self, ast: Ast) -> Result<Hir, Vec<ResolveError>> {
        self.register_builtins();
        self.predeclare_global_functions(&ast);
        let global_funcs = std::mem::take(&mut self.global_funcs);

        let mut hir_items = Vec::new();
        for item in ast.items {
            if let Ok(hir_item) = self.resolve_item(item, &global_funcs) {
                hir_items.push(hir_item);
            }
            // On Err the error was already recorded by `add_error`; the item
            // contributes no placeholder because the HIR is discarded anyway.
        }

        if !self.errors.is_empty() {
            return Err(self.errors);
        }

        Ok(Hir {
            items: hir_items,
            span: ast.span,
        })
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
                    span: Span::new(0, 0), // builtins have no source span
                    hir_id: HirId(0),
                },
            );
        }
    }

    /// First pass: declare all top-level functions so bodies can call
    /// functions declared later in the file. Duplicate top-level function
    /// names are reported here.
    fn predeclare_global_functions(&mut self, ast: &Ast) {
        for item in &ast.items {
            if let Item::Decl(Decl::Func(func_decl)) = item {
                let binding_id = self.next_binding_id();
                let binding = Binding {
                    id: binding_id,
                    kind: BindingKind::Function,
                    mutable: false, // function names are immutable bindings
                    builtin: false,
                    span: func_decl.span,
                    hir_id: HirId(0), // placeholder, filled when the body resolves
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
                    span: func.span,
                    hir_id: HirId(0),
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

        Ok(FuncDeclHir {
            name: func.name,
            params,
            ret_type: func.ret_type,
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
                span: param.span,
                hir_id,
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
                ty: param.ty,
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
                    span: Span::new(0, 0),
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
                span: Span::new(0, 0),
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
        if let Err(first) = self.scopes.declare(
            var.name.clone(),
            Binding {
                id: binding_id,
                kind: BindingKind::Variable,
                mutable: true,
                builtin: false,
                span: var.span,
                hir_id,
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
            name: var.name,
            ty: var.ty,
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
        // "函数不生效"); parameters/functions/builtins are exempt.
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
        if let Err(first) = self.scopes.declare(
            c.name.clone(),
            Binding {
                id: binding_id,
                kind: BindingKind::Const,
                mutable: false,
                builtin: false,
                span: c.span,
                hir_id,
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
            name: c.name,
            ty: c.ty,
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
                ExprHir::Cast(Box::new(self.resolve_expr(*expr)?), ty, span)
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
            Expr::Index(arr, idx) => {
                let a = self.resolve_expr(*arr)?;
                let i = self.resolve_expr(*idx)?;
                ExprHir::Index(Box::new(a), Box::new(i))
            }
            Expr::Field(obj, field) => {
                let o = self.resolve_expr(*obj)?;
                ExprHir::Field(Box::new(o), field)
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
        let (target, span) = match lhs {
            Expr::Ident(name, span) => (AssignTarget::Ident(name), span),
            Expr::Deref(inner, span) => match *inner {
                Expr::Ident(name, _) => (AssignTarget::Deref(name), span),
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

        // Both targets resolve the box/variable by name; they differ only in
        // lookup (plain assignment must find an assignable binding) and in
        // mutability (writing a pointee does not rebind `b`, so `b` need not
        // be mutable).
        let (name, binding) = match &target {
            AssignTarget::Ident(name) => (name, self.scopes.find_assign_target(name)),
            AssignTarget::Deref(name) => (name, self.scopes.get(name)),
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
                span: Span::new(span.start, rhs_span.end),
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

        Ok(ExprHir::Ident {
            name,
            binding_id,
            hir_id: self.next_hir_id(),
            span,
        })
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
                            span: Span::new(0, 0),
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
    fn declare_pattern_binding(
        &mut self,
        name: &str,
        span: Span,
    ) -> Result<(BindingId, HirId), ()> {
        let new_mutable = true; // pattern bindings are mutable
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
                span,
                hir_id,
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
