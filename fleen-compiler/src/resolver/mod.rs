//! Resolver module: name resolution and scope checking.
//!
//! Transforms AST → HIR (High-level IR) with resolved names and scope information.
//!
//! Key semantics (per `DESIGN.md`):
//! - **Binding vs assignment** (§3.1): first occurrence of `x = expr` in a scope is a
//!   binding; a later occurrence in the *same* scope is an assignment.
//! - **Shadowing** (§4.3): inside a block, `x = expr` is always a *new binding* that
//!   shadows the outer `x` (if mutability matches); it never assigns to the outer one.
//! - **Function scope** (§4.2/§4.3): the function body is independent; declarations
//!   inside it do not shadow-check against enclosing scopes.
//! - **Forward references**: function names are collected before bodies are resolved,
//!   so a function may be called before its declaration. Variables cannot.

pub mod error;
pub mod hir;
pub mod scope;

use crate::parser::ast::*;
use error::{ResolveError, ResolveErrorKind};
use hir::{
    BindingId, BlockHir, ChooseArmHir, ConstDeclHir, DeclHir, ExprChooseHir, ExprHir, ExprIfHir,
    ExprWhileHir, FuncBodyHir, FuncDeclHir, Hir, HirId, HirItem, ImportDeclHir, ParamHir,
    PatternHir, StmtHir, VarBindingHir,
};
use scope::{Binding, BindingKind, ScopeStack};

/// Builtin functions available in every program (registered in the global scope).
const BUILTINS: &[&str] = &["print"];

/// Resolve names in an AST, producing a HIR.
///
/// # Arguments
/// - `ast`: The parsed AST from the parser.
///
/// # Returns
/// - `Ok(Hir)`: Resolution successful, all names bound.
/// - `Err(Vec<ResolveError>)`: Resolution errors (collected, not fail-fast).
pub fn resolve(ast: Ast) -> Result<Hir, Vec<ResolveError>> {
    let resolver = Resolver::new();
    resolver.resolve_program(ast)
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
}

impl Resolver {
    fn new() -> Self {
        Self {
            scopes: ScopeStack::new(),
            errors: Vec::new(),
            hir_id_counter: 0,
            binding_id_counter: 0,
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

    fn add_error(&mut self, kind: ResolveErrorKind, span: crate::lexer::Span) {
        self.errors.push(ResolveError { kind, span });
    }

    /// Main entry point: resolve the entire program.
    fn resolve_program(mut self, ast: Ast) -> Result<Hir, Vec<ResolveError>> {
        // Register builtin functions in the global scope.
        self.register_builtins();

        // First pass: collect top-level function declarations for forward references.
        // The global scope does not shadow-check (Global kind), so duplicates among
        // builtins/functions are simply overwritten by design of `declare` failing
        // silently here; a duplicate user function shadows nothing and is reported
        // by later phases if needed.
        for item in &ast.items {
            if let Item::Decl(Decl::Func(func_decl)) = item {
                let binding_id = self.next_binding_id();
                let _ = self.scopes.declare(
                    func_decl.name.clone(),
                    Binding {
                        id: binding_id,
                        kind: BindingKind::Function,
                        mutable: false, // functions are immutable bindings
                        span: func_decl.span,
                        hir_id: HirId(0), // placeholder, filled when the body resolves
                    },
                );
            }
        }

        // Second pass: resolve all items. Errors are collected; a failed item
        // contributes an error placeholder so positions of later items stay stable.
        let mut hir_items = Vec::new();
        for item in ast.items {
            match self.resolve_item(item) {
                Ok(hir_item) => hir_items.push(hir_item),
                Err(_) => {
                    // Error already recorded by add_error.
                }
            }
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
                    kind: BindingKind::Function,
                    mutable: false,
                    span: crate::lexer::Span::new(0, 0), // builtins have no source span
                    hir_id: HirId(0),
                },
            );
        }
    }

    fn resolve_item(&mut self, item: Item) -> Result<HirItem, ()> {
        let hir_item = match item {
            Item::Import(import_decl) => HirItem::Import(self.resolve_import(import_decl)),
            Item::Decl(decl) => HirItem::Decl(self.resolve_decl(decl)?),
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

    fn resolve_decl(&mut self, decl: Decl) -> Result<DeclHir, ()> {
        match decl {
            Decl::Func(func) => Ok(DeclHir::Func(self.resolve_func_decl(func)?)),
            Decl::Var(var) => Ok(DeclHir::Var(self.resolve_var_binding(var)?)),
            Decl::Const(c) => Ok(DeclHir::Const(self.resolve_const_decl(c)?)),
        }
    }

    /// Resolve a function declaration.
    fn resolve_func_decl(&mut self, func: FuncDecl) -> Result<FuncDeclHir, ()> {
        // Declare the function in the CURRENT scope (before entering function scope).
        // This allows nested functions to be called by later code in the same scope.
        let binding_id = self.next_binding_id();
        let _ = self.scopes.declare(
            func.name.clone(),
            Binding {
                id: binding_id,
                kind: BindingKind::Function,
                mutable: false,
                span: func.span,
                hir_id: HirId(0), // placeholder, filled after body resolves
            },
        );

        // Enter function scope for the function body
        self.scopes.enter_function_scope();

        // Resolve parameters
        let mut hir_params = Vec::new();
        for param in func.params {
            let binding_id = self.next_binding_id();
            let hir_id = self.next_hir_id();
            let _ = self.scopes.declare(
                param.name.clone(),
                Binding {
                    id: binding_id,
                    kind: BindingKind::Parameter,
                    mutable: false, // parameters are immutable in 0.0.1
                    span: param.span,
                    hir_id,
                },
            );
            hir_params.push(ParamHir {
                name: param.name,
                ty: param.ty,
                binding_id,
                hir_id,
                span: param.span,
            });
        }

        // Resolve function body
        let hir_body = match func.body {
            FuncBody::SingleExpr(expr) => {
                let hir_expr = self.resolve_expr(*expr)?;
                FuncBodyHir::SingleExpr(Box::new(hir_expr))
            }
            FuncBody::Block(block) => {
                let hir_block = self.resolve_block(block)?;
                FuncBodyHir::Block(hir_block)
            }
        };

        // Exit function scope
        self.scopes.exit_scope();

        let hir_id = self.next_hir_id();
        let func_hir = FuncDeclHir {
            name: func.name.clone(),
            params: hir_params,
            ret_type: func.ret_type,
            body: hir_body,
            hir_id,
            span: func.span,
        };

        // Update the binding's hir_id in the scope where it was declared
        if let Some(binding) = self.scopes.get_mut(&func.name) {
            binding.hir_id = hir_id;
        }

        Ok(func_hir)
    }

    /// Resolve a variable binding statement `x = expr` / `x: int = expr`.
    ///
    /// Semantics depend on the current scope kind:
    /// - Name exists in the current scope → assignment to that binding
    ///   (must be mutable; error otherwise).
    /// - Name not in current scope → new binding (shadowing an outer name if any;
    ///   mutability must match the shadowed name, per DESIGN.md §4.3).
    ///
    /// The initializer is resolved BEFORE the binding is declared, so
    /// `x = x + 1` correctly fails (x not in scope during init resolution).
    fn resolve_var_binding(&mut self, var: VarBinding) -> Result<VarBindingHir, ()> {
        // First resolve the initializer WITHOUT the new binding in scope.
        // This implements "use before binding is an error" (DESIGN.md §3.1).
        let init = self.resolve_expr(*var.init)?;

        let hir_id = self.next_hir_id();

        // Now check if this is an assignment (name already in current scope) or new binding.
        let existing = self
            .scopes
            .current_get(&var.name)
            .map(|b| (b.id, b.mutable));

        if let Some((existing_id, existing_mutable)) = existing {
            // Assignment to an existing binding in the same scope.
            if !existing_mutable {
                self.add_error(
                    ResolveErrorKind::AssignToImmutable {
                        name: var.name.clone(),
                    },
                    var.span,
                );
                return Err(());
            }
            // Per DESIGN.md §3.4: assignment cannot carry a type annotation.
            if var.ty.is_some() {
                self.add_error(ResolveErrorKind::TypeAnnotationOnAssignment, var.span);
                return Err(());
            }

            if let Some(binding) = self.scopes.get_mut(&var.name) {
                binding.hir_id = hir_id;
            }

            Ok(VarBindingHir {
                name: var.name,
                ty: var.ty,
                init: Box::new(init),
                binding_id: existing_id,
                hir_id,
                span: var.span,
            })
        } else {
            // New binding. If it shadows an outer name, mutability must match
            // (DESIGN.md §4.3). Function boundary acts as a shadowing barrier
            // (DESIGN.md §4.3: "函数不生效").
            let new_mutable = true; // plain bindings are mutable
            if let Some(outer) = self.scopes.exists_until_function(&var.name)
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

            let _ = self.scopes.declare(
                var.name.clone(),
                Binding {
                    id: binding_id,
                    kind: BindingKind::Variable,
                    mutable: true,
                    span: var.span,
                    hir_id,
                },
            );

            Ok(VarBindingHir {
                name: var.name,
                ty: var.ty,
                init: Box::new(init),
                binding_id,
                hir_id,
                span: var.span,
            })
        }
    }

    /// Resolve a const declaration.
    fn resolve_const_decl(&mut self, c: ConstDecl) -> Result<ConstDeclHir, ()> {
        // First resolve the initializer WITHOUT the new binding in scope.
        let init = self.resolve_expr(*c.init)?;

        let hir_id = self.next_hir_id();

        let existing = self.scopes.current_get(&c.name).map(|b| (b.id, b.mutable));

        if let Some((_existing_id, existing_mutable)) = existing {
            // const on an existing name in the same scope:
            // - const → const: re-binding an immutable name is an error (DESIGN.md §3.5)
            // - mutable → const: would change mutability, ambiguous (DESIGN.md §16.3)
            let _ = existing_mutable;
            self.add_error(
                ResolveErrorKind::DuplicateDeclaration {
                    name: c.name.clone(),
                },
                c.span,
            );
            return Err(());
        }

        // New const binding. Shadow-check only against scopes up to function boundary
        // (DESIGN.md §4.3: "函数不生效").
        let new_mutable = false; // const is immutable
        if let Some(outer) = self.scopes.exists_until_function(&c.name)
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

        let _ = self.scopes.declare(
            c.name.clone(),
            Binding {
                id: binding_id,
                kind: BindingKind::Variable,
                mutable: false,
                span: c.span,
                hir_id,
            },
        );

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
            Expr::Int(n) => ExprHir::Int(n),
            Expr::Float(f) => ExprHir::Float(f),
            Expr::Bool(b) => ExprHir::Bool(b),
            Expr::Str(s) => ExprHir::Str(s),
            Expr::Ident(name) => self.resolve_ident(name)?,
            Expr::Block(block) => ExprHir::Block(self.resolve_block(block)?),
        };
        Ok(hir)
    }

    /// Resolve an assignment expression: `lhs = rhs`.
    /// LHS must be an identifier (for now).
    fn resolve_assign(&mut self, lhs: Expr, rhs: Expr) -> Result<ExprHir, ()> {
        let lhs_name = match lhs {
            Expr::Ident(name) => name,
            _ => {
                self.add_error(
                    ResolveErrorKind::InvalidAssignmentTarget,
                    // Use a default span; the parser ensures LHS is ident
                    crate::lexer::Span::new(0, 0),
                );
                return Err(());
            }
        };

        let rhs_hir = self.resolve_expr(rhs)?;

        // Look up the variable in scope
        match self.scopes.get(&lhs_name) {
            Some(binding) => {
                if !binding.mutable {
                    self.add_error(
                        ResolveErrorKind::AssignToImmutable {
                            name: lhs_name.clone(),
                        },
                        rhs_hir.span(),
                    );
                    return Err(());
                }
                Ok(ExprHir::Assign {
                    name: lhs_name,
                    binding_id: binding.id,
                    rhs: Box::new(rhs_hir),
                    hir_id: self.next_hir_id(),
                })
            }
            None => {
                self.add_error(
                    ResolveErrorKind::UndeclaredVariable {
                        name: lhs_name.clone(),
                    },
                    rhs_hir.span(),
                );
                Err(())
            }
        }
    }

    /// Resolve an identifier expression (variable or function reference).
    fn resolve_ident(&mut self, name: String) -> Result<ExprHir, ()> {
        // Get binding info first (immutable borrow)
        let binding_info = match self.scopes.get(&name) {
            Some(binding) => (binding.id, binding.span),
            None => {
                self.add_error(
                    ResolveErrorKind::UndeclaredVariable { name: name.clone() },
                    crate::lexer::Span::new(0, 0), // TODO: need better span from AST
                );
                return Err(());
            }
        };

        // Now we can mutate
        Ok(ExprHir::Ident {
            name,
            binding_id: binding_info.0,
            hir_id: self.next_hir_id(),
            span: binding_info.1, // use definition span for now
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
        // The condition sees the enclosing scope; only the body is a new scope.
        let condition = self.resolve_expr(*while_expr.condition)?;
        let body = self.resolve_block(while_expr.body)?;

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

            let pattern = self.resolve_pattern(arm.pattern)?;
            let guard = if let Some(g) = arm.guard {
                Some(Box::new(self.resolve_expr(*g)?))
            } else {
                None
            };
            let body = self.resolve_block(arm.body)?;

            self.scopes.exit_scope();

            arms.push(ChooseArmHir {
                pattern,
                guard,
                body,
            });
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
            Pattern::Ident(name) => {
                // Pattern binding: declares a new variable in the arm's scope.
                // Shadow-check against enclosing scopes (mutability must match).
                let new_mutable = true; // pattern bindings are mutable
                if let Some(outer) = self.scopes.exists_until_function(&name)
                    && outer.mutable != new_mutable
                {
                    self.add_error(
                        ResolveErrorKind::ShadowingMutabilityMismatch {
                            outer_mutable: outer.mutable,
                            inner_mutable: new_mutable,
                        },
                        crate::lexer::Span::new(0, 0), // TODO: need pattern span from AST
                    );
                    return Err(());
                }

                let binding_id = self.next_binding_id();
                let hir_id = self.next_hir_id();

                let _ = self.scopes.declare(
                    name.clone(),
                    Binding {
                        id: binding_id,
                        kind: BindingKind::Variable,
                        mutable: true,
                        span: crate::lexer::Span::new(0, 0), // TODO: need pattern span from AST
                        hir_id,
                    },
                );

                Ok(PatternHir::Ident {
                    name,
                    binding_id,
                    hir_id,
                })
            }
        }
    }

    fn resolve_block(&mut self, block: Block) -> Result<BlockHir, ()> {
        self.scopes.enter_block_scope();

        let mut hir_stmts = Vec::new();
        for stmt in block.stmts {
            // Error recovery: a failed statement contributes an Error placeholder
            // so later statements still resolve (errors are collected, not fail-fast).
            let hir_stmt = match self.resolve_stmt(stmt) {
                Ok(s) => s,
                Err(()) => StmtHir::Error,
            };
            hir_stmts.push(hir_stmt);
        }

        let tail_expr = if let Some(expr) = block.tail_expr {
            let hir_expr = self.resolve_expr(*expr)?;
            Some(Box::new(hir_expr))
        } else {
            None
        };

        self.scopes.exit_scope();

        Ok(BlockHir {
            stmts: hir_stmts,
            tail_expr,
            hir_id: self.next_hir_id(),
            span: block.span,
        })
    }

    fn resolve_stmt(&mut self, stmt: Stmt) -> Result<StmtHir, ()> {
        match stmt {
            Stmt::Decl(decl) => Ok(StmtHir::Decl(self.resolve_decl(decl)?)),
            Stmt::Expr(expr, has_semi) => {
                let hir_expr = self.resolve_expr(*expr)?;
                Ok(StmtHir::Expr(Box::new(hir_expr), has_semi))
            }
        }
    }
}
