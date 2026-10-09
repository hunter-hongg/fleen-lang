//! Type inference for the type checker.
//!
//! This module contains the core type inference logic, transforming
//! HIR expressions into Typed HIR expressions with type information.

use crate::lexer::Span;
use crate::parser::ast::{self, *};
use crate::resolver::hir::*;
use crate::resolver::scope::{Binding, BindingKind, ScopeStack};
use crate::typeck::builtins::{BuiltinParam, find_builtin};
use crate::typeck::error::{TypeckError, TypeckErrorKind};
use crate::typeck::typed_hir::{
    Access, Type, TypedBlockHir, TypedChooseArmHir, TypedConstDeclHir, TypedDeclHir,
    TypedExprChooseHir, TypedExprHir, TypedExprIfHir, TypedExprWhileHir, TypedFuncBodyHir,
    TypedFuncDeclHir, TypedHir, TypedHirItem, TypedParamHir, TypedPatternHir, TypedStmtHir,
    TypedVarBindingHir,
};
use crate::typeck::unify::{as_func_type, as_result_type, unify, unify_arg, unify_assign};
use std::collections::{HashMap, HashSet};

/// Convert `ast::Type` to `typed_hir::Type`.
fn convert_type(ty: &ast::Type) -> Type {
    match ty {
        ast::Type::Base(BaseType::Int) => Type::Int,
        ast::Type::Base(BaseType::Float) => Type::Float,
        ast::Type::Base(BaseType::Bool) => Type::Bool,
        ast::Type::Base(BaseType::String) => Type::String,
        ast::Type::Base(BaseType::Unit) => Type::Unit,
        ast::Type::Func(params, ret) => {
            let param_types: Vec<Type> = params.iter().map(convert_type).collect();
            Type::Func(param_types, Box::new(convert_type(ret)))
        }
        ast::Type::Result(ok, err) => {
            Type::Result(Box::new(convert_type(ok)), Box::new(convert_type(err)))
        }
        ast::Type::Array(elem) => Type::Array(Box::new(convert_type(elem))),
        ast::Type::Box(inner) => Type::Box(Box::new(convert_type(inner))),
        ast::Type::Ref(inner) => Type::Ref(Box::new(convert_type(inner))),
    }
}

/// Type inference context.
pub struct TypeChecker {
    /// Scope stack for type lookup.
    scopes: ScopeStack,
    /// Collected errors.
    errors: Vec<TypeckError>,
    /// Counter for generating unique HIR node IDs.
    /// Counter for generating unique binding IDs.
    /// Function signatures collected in forward pass.
    func_signatures: HashMap<String, (Vec<Type>, Type)>,
    /// Names of user-declared functions, used to distinguish shadowed
    /// builtins (`print`, `Ok`, `Err`) from the real builtins.
    user_func_names: HashSet<String>,
    /// Declared return type of each function currently being checked
    /// (innermost last); `None` = function has no return annotation.
    /// Needed by `?`, whose legality depends on the enclosing function's
    /// return type — information that the `expected` thread cannot carry.
    fn_ret_stack: Vec<Option<Type>>,
    /// Binding ID to type mapping.
    binding_types: HashMap<BindingId, Type>,
    /// Binding IDs of top-level (global) variables/consts. 0.0.2 U05: owned
    /// globals are read as deep copies and may never be moved — ownership
    /// checks key off this set.
    global_ids: HashSet<BindingId>,
}

impl TypeChecker {
    /// Create a new type checker.
    pub fn new() -> Self {
        Self {
            scopes: ScopeStack::new(),
            errors: Vec::new(),
            func_signatures: HashMap::new(),
            user_func_names: HashSet::new(),
            fn_ret_stack: Vec::new(),
            binding_types: HashMap::new(),
            global_ids: HashSet::new(),
        }
    }

    /// Add an error to the collection.
    fn add_error(&mut self, kind: TypeckErrorKind, span: Span) {
        self.errors.push(TypeckError::new(kind, span));
    }

    /// Register builtin functions.
    ///
    /// Seeds `func_signatures` so that a bare builtin identifier (e.g. `print`
    /// passed as a value) has a function type. `Ok`/`Err` are not registered
    /// here: their type depends on context, so calls are recognized by name in
    /// `typeck_call` (unless shadowed by a user function, §4.3).
    fn register_builtins(&mut self) {
        for sig in crate::typeck::builtins::BUILTINS {
            // Variadic builtins get their element type as the single
            // parameter for value-position lookup; call checking uses the
            // table's real (variadic) shape.
            let params: Vec<Type> = match sig.param {
                BuiltinParam::Fixed(params) => params.to_vec(),
                BuiltinParam::Variadic(elem) => elem.to_vec(),
            };
            self.func_signatures
                .insert(sig.name.to_string(), (params, sig.ret.clone()));
        }
    }

    /// Collect function signatures in a forward pass.
    fn collect_func_signatures(&mut self, hir: &Hir) {
        for item in &hir.items {
            if let HirItem::Decl(DeclHir::Func(func)) = item {
                let param_types: Vec<Type> =
                    func.params.iter().map(|p| convert_type(&p.ty)).collect();
                let ret_type = func
                    .ret_type
                    .as_ref()
                    .map(convert_type)
                    .unwrap_or(Type::Unit);
                self.func_signatures
                    .insert(func.name.clone(), (param_types, ret_type));
                self.user_func_names.insert(func.name.clone());
            }
        }
    }

    /// Type check the entire program.
    pub fn typeck(mut self, hir: Hir) -> Result<TypedHir, Vec<TypeckError>> {
        self.register_builtins();
        self.collect_func_signatures(&hir);
        self.collect_global_ids(&hir);

        let mut typed_items = Vec::new();
        for item in hir.items {
            if let Ok(typed_item) = self.typeck_item(item) {
                typed_items.push(typed_item);
            }
        }

        if !self.errors.is_empty() {
            return Err(self.errors);
        }

        Ok(TypedHir {
            items: typed_items,
            span: hir.span,
        })
    }

    /// 0.0.2 U05: record the binding IDs of top-level variables and consts.
    /// Owned globals are read as deep copies and may never be moved; the
    /// ownership checks key off this set.
    fn collect_global_ids(&mut self, hir: &Hir) {
        for item in &hir.items {
            match item {
                HirItem::Decl(DeclHir::Var(v)) => {
                    self.global_ids.insert(v.binding_id);
                }
                HirItem::Decl(DeclHir::Const(c)) => {
                    self.global_ids.insert(c.binding_id);
                }
                _ => {}
            }
        }
    }

    /// Type check a top-level item.
    fn typeck_item(&mut self, item: HirItem) -> Result<TypedHirItem, ()> {
        match item {
            HirItem::Import(import) => Ok(TypedHirItem::Import(import)),
            HirItem::Decl(decl) => Ok(TypedHirItem::Decl(self.typeck_decl(decl)?)),
            HirItem::Expr(expr) => Ok(TypedHirItem::Expr(self.typeck_expr(expr, None)?)),
        }
    }

    /// Type check a declaration.
    fn typeck_decl(&mut self, decl: DeclHir) -> Result<TypedDeclHir, ()> {
        match decl {
            DeclHir::Func(func) => Ok(TypedDeclHir::Func(self.typeck_func_decl(func)?)),
            DeclHir::Var(var) => Ok(TypedDeclHir::Var(self.typeck_var_binding(var)?)),
            DeclHir::Const(c) => Ok(TypedDeclHir::Const(self.typeck_const_decl(c)?)),
        }
    }

    /// Type check a function declaration.
    fn typeck_func_decl(&mut self, func: FuncDeclHir) -> Result<TypedFuncDeclHir, ()> {
        // Enter function scope
        self.scopes.enter_function_scope();

        // Add parameters to scope
        let mut typed_params = Vec::new();
        for param in &func.params {
            // Keep the resolver's binding_id so that TypedParamHir, body
            // identifier uses, and slot allocation in `lower` all refer to
            // the same binding.
            let binding_id = param.binding_id;
            let hir_id = param.hir_id;
            let param_type = convert_type(&param.ty);
            let _ = self.scopes.declare(
                param.name.clone(),
                Binding {
                    id: binding_id,
                    kind: BindingKind::Parameter,
                    mutable: false,
                    builtin: false,
                    ref_param: matches!(param_type, Type::Ref(_)),
                    span: param.span,
                    hir_id,
                },
            );
            // Store the parameter type
            self.binding_types.insert(binding_id, param_type.clone());
            typed_params.push(TypedParamHir {
                name: param.name.clone(),
                ty: param_type,
                binding_id,
                hir_id,
                span: param.span,
            });
        }

        // Return type: `None` means the function has no annotation (unit).
        let ret_ty: Option<Type> = func.ret_type.as_ref().map(convert_type);

        // The declared return type drives both the `expected` thread into the
        // body (Ok/Err constructors) and the `?` legality check, which needs
        // it even in positions `expected` cannot reach.
        self.fn_ret_stack.push(ret_ty.clone());
        let body_result = self.typeck_func_body(&func.body, ret_ty.as_ref());
        self.fn_ret_stack.pop();
        let typed_body = body_result?;

        // Exit function scope
        self.scopes.exit_scope();

        Ok(TypedFuncDeclHir {
            name: func.name,
            params: typed_params,
            ret_type: func.ret_type.as_ref().map(convert_type),
            body: typed_body,
            hir_id: func.hir_id,
            span: func.span,
        })
    }

    /// Type check a function body against the declared return type.
    fn typeck_func_body(
        &mut self,
        body: &FuncBodyHir,
        ret_ty: Option<&Type>,
    ) -> Result<TypedFuncBodyHir, ()> {
        match body {
            FuncBodyHir::SingleExpr(expr) => {
                let typed_expr = self.typeck_expr((**expr).clone(), ret_ty)?;
                // Check return type: an omitted annotation means `unit`.
                let expected_ret = ret_ty.cloned().unwrap_or(Type::Unit);
                // `ref T` is a distinct type (PLAN §3.3.3): a borrow does not
                // flow out as `T` — the value must be `clone`d explicitly.
                if let Err(e) = unify(&expected_ret, &typed_expr.ty(), typed_expr.span()) {
                    self.add_error(e.kind, e.span);
                    return Err(());
                }
                Ok(TypedFuncBodyHir::SingleExpr(Box::new(typed_expr)))
            }
            FuncBodyHir::Block(block) => {
                let typed_block = self.typeck_block(block, ret_ty)?;
                // Check return type: an omitted annotation means `unit`.
                let expected_ret = ret_ty.cloned().unwrap_or(Type::Unit);
                if let Err(e) = unify(&expected_ret, &typed_block.ty, typed_block.span) {
                    self.add_error(e.kind, e.span);
                    return Err(());
                }
                Ok(TypedFuncBodyHir::Block(typed_block))
            }
        }
    }

    /// Type check a variable binding.
    fn typeck_var_binding(&mut self, var: VarBindingHir) -> Result<TypedVarBindingHir, ()> {
        let typed_ty = var.ty.as_ref().map(convert_type);
        // Annotated bindings pass their type down so `Ok`/`Err` in the
        // initializer can infer (D6, path 2 of the expected thread).
        let typed_init = self.typeck_expr(*var.init, typed_ty.as_ref())?;
        if let Some(ref annotated_ty) = typed_ty
            && let Err(e) = unify(annotated_ty, &typed_init.ty(), typed_init.span())
        {
            self.add_error(e.kind, e.span);
            return Err(());
        }

        // Check if this is an assignment (binding_id already in binding_types)
        if let Some(existing_type) = self.binding_types.get(&var.binding_id) {
            // This is an assignment, not a new binding. `ref T` is a
            // distinct type (PLAN §3.3.3): a borrow does not assign to a
            // value binding — write `x = clone s` instead.
            if let Err(e) = unify_assign(existing_type, &typed_init.ty(), typed_init.span()) {
                self.add_error(e.kind, e.span);
                return Err(());
            }
            // Keep the existing type
            return Ok(TypedVarBindingHir {
                name: var.name,
                ty: Some(existing_type.clone()),
                init: Box::new(typed_init),
                binding_id: var.binding_id,
                hir_id: var.hir_id,
                span: var.span,
            });
        }

        // New binding: add to scope
        let binding_id = var.binding_id;
        let hir_id = var.hir_id;
        let _ = self.scopes.declare(
            var.name.clone(),
            Binding {
                id: binding_id,
                kind: BindingKind::Variable,
                mutable: true,
                builtin: false,
                ref_param: false,
                span: var.span,
                hir_id,
            },
        );

        // Store the type in our binding_types map
        let var_type = typed_ty.clone().unwrap_or_else(|| typed_init.ty());
        self.binding_types.insert(binding_id, var_type);

        // 0.0.2 U05: a bare owned local on a new binding's RHS must
        // transfer explicitly — `y = move x` / `y = clone x` (PLAN §3.1.3
        // rule 2). Fresh values (literals, call results, ...) are fine.
        self.reject_bare_owned_arg(&typed_init, typed_init.span())?;

        Ok(TypedVarBindingHir {
            name: var.name,
            ty: typed_ty,
            init: Box::new(typed_init),
            binding_id,
            hir_id,
            span: var.span,
        })
    }

    /// Type check a const declaration.
    fn typeck_const_decl(&mut self, c: ConstDeclHir) -> Result<TypedConstDeclHir, ()> {
        let typed_ty = c.ty.as_ref().map(convert_type);
        // Same expected thread as `typeck_var_binding` (D6, path 2).
        let typed_init = self.typeck_expr(*c.init, typed_ty.as_ref())?;
        if let Some(ref annotated_ty) = typed_ty
            && let Err(e) = unify(annotated_ty, &typed_init.ty(), typed_init.span())
        {
            self.add_error(e.kind, e.span);
            return Err(());
        }

        // Add binding to scope. Keep the resolver's binding_id so
        // `TypedConstDeclHir`, body uses, and `lower` slot allocation
        // agree on the same binding.
        let binding_id = c.binding_id;
        let hir_id = c.hir_id;
        let _ = self.scopes.declare(
            c.name.clone(),
            Binding {
                id: binding_id,
                kind: BindingKind::Const,
                mutable: false,
                builtin: false,
                ref_param: false,
                span: c.span,
                hir_id,
            },
        );

        // Store the type in our binding_types map
        let const_type = typed_ty.clone().unwrap_or_else(|| typed_init.ty());
        self.binding_types.insert(binding_id, const_type);

        Ok(TypedConstDeclHir {
            name: c.name,
            ty: typed_ty,
            init: Box::new(typed_init),
            binding_id,
            hir_id,
            span: c.span,
        })
    }

    /// Type check a block.
    ///
    /// `expected` is the context type for the block's tail expression only
    /// (function return type or binding annotation); statements never
    /// inherit it.
    fn typeck_block(
        &mut self,
        block: &BlockHir,
        expected: Option<&Type>,
    ) -> Result<TypedBlockHir, ()> {
        self.scopes.enter_block_scope();

        let mut typed_stmts = Vec::new();
        for stmt in &block.stmts {
            match self.typeck_stmt(stmt) {
                Ok(s) => typed_stmts.push(s),
                Err(()) => typed_stmts.push(TypedStmtHir::Error),
            }
        }

        let typed_tail = match &block.tail_expr {
            Some(expr) => Some(Box::new(self.typeck_expr((**expr).clone(), expected)?)),
            None => None,
        };

        self.scopes.exit_scope();

        // Block type is the type of the tail expression, or Unit if no tail
        let block_ty = typed_tail
            .as_ref()
            .map(|e| e.ty().clone())
            .unwrap_or(Type::Unit);

        Ok(TypedBlockHir {
            stmts: typed_stmts,
            tail_expr: typed_tail,
            ty: block_ty,
            hir_id: block.hir_id,
            span: block.span,
        })
    }

    /// Type check a statement.
    fn typeck_stmt(&mut self, stmt: &StmtHir) -> Result<TypedStmtHir, ()> {
        match stmt {
            StmtHir::Decl(decl) => Ok(TypedStmtHir::Decl(self.typeck_decl(decl.clone())?)),
            StmtHir::Expr(expr, has_semi) => {
                let typed_expr = self.typeck_expr((**expr).clone(), None)?;
                // A statement's value is discarded; a `Result` may not be
                // silently dropped (0.0.2's only must-handle rule).
                if let Type::Result(_, _) = typed_expr.ty() {
                    self.add_error(
                        TypeckErrorKind::UnhandledResult {
                            ty: typed_expr.ty(),
                        },
                        typed_expr.span(),
                    );
                    return Err(());
                }
                Ok(TypedStmtHir::Expr(Box::new(typed_expr), *has_semi))
            }
            StmtHir::Error => Ok(TypedStmtHir::Error),
        }
    }

    /// Type check an expression.
    ///
    /// `expected` is a *hint*, not a constraint: it flows only into value
    /// positions where a `Result` constructor can infer from it (function
    /// bodies / branch tails / block tails, annotated binding initializers).
    /// It never replaces the "branches must agree" and operand checks.
    fn typeck_expr(&mut self, expr: ExprHir, expected: Option<&Type>) -> Result<TypedExprHir, ()> {
        match expr {
            ExprHir::Int(n, span) => Ok(TypedExprHir::Int(n, span)),
            ExprHir::Float(f, span) => Ok(TypedExprHir::Float(f, span)),
            ExprHir::Bool(b, span) => Ok(TypedExprHir::Bool(b, span)),
            ExprHir::Str(s, span) => Ok(TypedExprHir::Str(s, span)),

            ExprHir::Ident {
                name,
                binding_id,
                hir_id,
                span,
            } => {
                let ty = self.lookup_type(&name, span)?;
                // `Access::Copy` is the default; the ownership pass (U05)
                // upgrades bare owned reads to `Clone` and implicit
                // tail-position transfers to `Move`.
                Ok(TypedExprHir::Ident {
                    name,
                    binding_id,
                    ty,
                    access: Access::Copy,
                    hir_id,
                    span,
                })
            }

            ExprHir::Assign {
                name,
                binding_id,
                rhs,
                hir_id,
                span,
            } => {
                let typed_rhs = self.typeck_expr(*rhs, None)?;
                let var_type = self.lookup_type(&name, span)?;

                if let Err(e) = unify_assign(&var_type, &typed_rhs.ty(), typed_rhs.span()) {
                    self.add_error(e.kind, e.span);
                    return Err(());
                }

                // 0.0.2 U05: a bare owned local value in binding/assignment
                // RHS position must transfer ownership explicitly.
                self.reject_bare_owned_arg(&typed_rhs, typed_rhs.span())?;

                Ok(TypedExprHir::Assign {
                    name,
                    binding_id,
                    rhs: Box::new(typed_rhs),
                    ty: Type::Unit,
                    hir_id,
                    span,
                })
            }

            ExprHir::If(if_expr) => self.typeck_if_expr(if_expr, expected),
            ExprHir::While(while_expr) => self.typeck_while_expr(while_expr),
            ExprHir::Choose(choose_expr) => self.typeck_choose_expr(choose_expr, expected),

            ExprHir::Or(lhs, rhs) => self.typeck_binary_op(*lhs, *rhs, "or", Type::Bool),
            ExprHir::And(lhs, rhs) => self.typeck_binary_op(*lhs, *rhs, "and", Type::Bool),
            ExprHir::Eq(lhs, rhs) => self.typeck_binary_op(*lhs, *rhs, "==", Type::Bool),
            ExprHir::Ne(lhs, rhs) => self.typeck_binary_op(*lhs, *rhs, "!=", Type::Bool),
            ExprHir::Lt(lhs, rhs) => self.typeck_binary_op(*lhs, *rhs, "<", Type::Bool),
            ExprHir::Gt(lhs, rhs) => self.typeck_binary_op(*lhs, *rhs, ">", Type::Bool),
            ExprHir::Le(lhs, rhs) => self.typeck_binary_op(*lhs, *rhs, "<=", Type::Bool),
            ExprHir::Ge(lhs, rhs) => self.typeck_binary_op(*lhs, *rhs, ">=", Type::Bool),
            ExprHir::Add(lhs, rhs) => self.typeck_arith_op(*lhs, *rhs, "+"),
            ExprHir::Sub(lhs, rhs) => self.typeck_arith_op(*lhs, *rhs, "-"),
            ExprHir::Mul(lhs, rhs) => self.typeck_arith_op(*lhs, *rhs, "*"),
            ExprHir::Div(lhs, rhs) => self.typeck_arith_op(*lhs, *rhs, "/"),
            ExprHir::Mod(lhs, rhs) => self.typeck_arith_op(*lhs, *rhs, "%"),

            ExprHir::Not(expr) => {
                let typed_expr = self.typeck_expr(*expr, None)?;
                if !matches!(typed_expr.ty(), Type::Bool) {
                    self.add_error(
                        TypeckErrorKind::InvalidOperand {
                            op: "!".to_string(),
                            ty: typed_expr.ty().clone(),
                        },
                        typed_expr.span(),
                    );
                    return Err(());
                }
                Ok(TypedExprHir::Not(Box::new(typed_expr)))
            }

            ExprHir::Neg(expr) => {
                let typed_expr = self.typeck_expr(*expr, None)?;
                match typed_expr.ty() {
                    Type::Int | Type::Float => Ok(TypedExprHir::Neg(Box::new(typed_expr))),
                    _ => {
                        self.add_error(
                            TypeckErrorKind::InvalidOperand {
                                op: "-".to_string(),
                                ty: typed_expr.ty().clone(),
                            },
                            typed_expr.span(),
                        );
                        Err(())
                    }
                }
            }

            ExprHir::Call(func, args) => self.typeck_call(*func, args, expected),
            ExprHir::Index(arr, idx) => self.typeck_index(*arr, *idx),
            // 0.0.2 U05: ownership expressions — validated here; the
            // flow-sensitive move analysis runs afterwards in
            // `ownership.rs` over the produced TypedHir.
            ExprHir::Move(inner, span) => self.typeck_move(*inner, span),
            ExprHir::Clone(inner, span) => self.typeck_clone(*inner, span),
            ExprHir::Box(inner, span) => self.typeck_box(*inner, span),
            ExprHir::Deref(inner, span) => self.typeck_deref(*inner, span),
            // 0.0.2 U13: `as` cast — whitelist: only scalar (int/float/bool) → string.
            ExprHir::Cast(inner, ty, span) => self.typeck_cast(*inner, ty, span),
            // 0.0.2 U04: `?` Result propagation.
            ExprHir::Question(operand, span) => self.typeck_question(*operand, span),
            ExprHir::AssignDeref {
                name,
                binding_id,
                rhs,
                hir_id,
                span,
            } => self.typeck_assign_deref(name, binding_id, *rhs, hir_id, span),
            ExprHir::Field(obj, field) => self.typeck_field(*obj, field),
            ExprHir::Block(block) => {
                let typed_block = self.typeck_block(&block, expected)?;
                Ok(TypedExprHir::Block(typed_block))
            }
        }
    }

    /// Type check an if expression.
    ///
    /// `expected` flows into every branch's tail so `Ok`/`Err` constructors
    /// can infer there (function body context); the condition never sees it.
    fn typeck_if_expr(
        &mut self,
        if_expr: ExprIfHir,
        expected: Option<&Type>,
    ) -> Result<TypedExprHir, ()> {
        let typed_cond = self.typeck_expr(*if_expr.condition, None)?;

        // Condition must be Bool
        if !matches!(typed_cond.ty(), Type::Bool) {
            self.add_error(
                TypeckErrorKind::ConditionNotBool {
                    found: typed_cond.ty().clone(),
                },
                typed_cond.span(),
            );
            return Err(());
        }

        let typed_then = self.typeck_block(&if_expr.then_branch, expected)?;

        let mut typed_elifs = Vec::new();
        for (cond, block) in &if_expr.elif_branches {
            let typed_cond = self.typeck_expr((**cond).clone(), None)?;
            if !matches!(typed_cond.ty(), Type::Bool) {
                self.add_error(
                    TypeckErrorKind::ConditionNotBool {
                        found: typed_cond.ty().clone(),
                    },
                    typed_cond.span(),
                );
                return Err(());
            }
            let typed_block = self.typeck_block(block, expected)?;
            typed_elifs.push((Box::new(typed_cond), typed_block));
        }

        let typed_else = match &if_expr.else_branch {
            Some(block) => Some(self.typeck_block(block, expected)?),
            None => None,
        };

        // All branches must have the same type
        let branch_type = typed_then.ty.clone();
        for (_, block) in &typed_elifs {
            if let Err(e) = unify(&branch_type, &block.ty, block.span) {
                self.add_error(e.kind, e.span);
                return Err(());
            }
        }
        if let Some(ref else_block) = typed_else
            && let Err(e) = unify(&branch_type, &else_block.ty, else_block.span)
        {
            self.add_error(e.kind, e.span);
            return Err(());
        }

        Ok(TypedExprHir::If(TypedExprIfHir {
            condition: Box::new(typed_cond),
            then_branch: typed_then,
            elif_branches: typed_elifs,
            else_branch: typed_else,
            ty: branch_type,
            hir_id: if_expr.hir_id,
            span: if_expr.span,
        }))
    }

    /// Type check a while expression.
    fn typeck_while_expr(&mut self, while_expr: ExprWhileHir) -> Result<TypedExprHir, ()> {
        let typed_cond = self.typeck_expr(*while_expr.condition, None)?;

        // Condition must be Bool
        if !matches!(typed_cond.ty(), Type::Bool) {
            self.add_error(
                TypeckErrorKind::ConditionNotBool {
                    found: typed_cond.ty().clone(),
                },
                typed_cond.span(),
            );
            return Err(());
        }

        // The body's value is discarded (while evaluates to unit), so the
        // body never inherits the outer expected type.
        let typed_body = self.typeck_block(&while_expr.body, None)?;

        Ok(TypedExprHir::While(TypedExprWhileHir {
            condition: Box::new(typed_cond),
            body: typed_body,
            ty: Type::Unit,
            hir_id: while_expr.hir_id,
            span: while_expr.span,
        }))
    }

    /// Type check a choose expression.
    ///
    /// `expected` flows into each arm body's tail (function body context);
    /// the scrutinee never sees it.
    fn typeck_choose_expr(
        &mut self,
        choose_expr: ExprChooseHir,
        expected: Option<&Type>,
    ) -> Result<TypedExprHir, ()> {
        let typed_scrutinee = self.typeck_expr(*choose_expr.scrutinee, None)?;
        let scrutinee_type = typed_scrutinee.ty().clone();
        // 0.0.2 U05 (D7): a `choose` scrutinee is a consuming position —
        // a bare owned local must be written `move x` / `clone x` (the
        // `Err(e)` / `Ok(v)` pattern bindings then receive the value).
        self.reject_bare_owned_arg(&typed_scrutinee, typed_scrutinee.span())?;

        let mut typed_arms = Vec::new();
        for arm in &choose_expr.arms {
            self.scopes.enter_block_scope();

            let typed_pattern = self.typeck_pattern(&arm.pattern, &scrutinee_type)?;

            let typed_guard = match &arm.guard {
                Some(guard) => {
                    let typed_guard = self.typeck_expr((**guard).clone(), None)?;
                    if !matches!(typed_guard.ty(), Type::Bool) {
                        self.add_error(
                            TypeckErrorKind::ConditionNotBool {
                                found: typed_guard.ty().clone(),
                            },
                            typed_guard.span(),
                        );
                        return Err(());
                    }
                    Some(Box::new(typed_guard))
                }
                None => None,
            };

            let typed_body = self.typeck_block(&arm.body, expected)?;

            self.scopes.exit_scope();

            typed_arms.push(TypedChooseArmHir {
                pattern: typed_pattern,
                guard: typed_guard,
                body: typed_body,
            });
        }

        // Check exhaustiveness
        self.check_exhaustiveness(&scrutinee_type, &typed_arms, choose_expr.span)?;

        // All arms must have the same type
        if let Some(first_arm) = typed_arms.first() {
            let arm_type = first_arm.body.ty.clone();
            for arm in &typed_arms[1..] {
                if let Err(e) = unify(&arm_type, &arm.body.ty, arm.body.span) {
                    self.add_error(e.kind, e.span);
                    return Err(());
                }
            }

            Ok(TypedExprHir::Choose(TypedExprChooseHir {
                scrutinee: Box::new(typed_scrutinee),
                arms: typed_arms,
                ty: arm_type,
                hir_id: choose_expr.hir_id,
                span: choose_expr.span,
            }))
        } else {
            // Empty `choose` always fails the exhaustiveness check above,
            // so this branch is unreachable in the current flow.
            unreachable!("exhaustiveness check rejects an empty choose")
        }
    }

    /// Type check a pattern.
    fn typeck_pattern(
        &mut self,
        pattern: &PatternHir,
        scrutinee_type: &Type,
    ) -> Result<TypedPatternHir, ()> {
        match pattern {
            PatternHir::Literal(expr) => {
                let typed_expr = self.typeck_expr((**expr).clone(), None)?;
                if let Err(e) = unify(scrutinee_type, &typed_expr.ty(), typed_expr.span()) {
                    self.add_error(e.kind, e.span);
                    return Err(());
                }
                Ok(TypedPatternHir::Literal(Box::new(typed_expr)))
            }
            PatternHir::Ident {
                name,
                binding_id: pat_binding_id,
                hir_id: pat_hir_id,
                span,
            } => {
                // Pattern binding: new variable with scrutinee type.
                // Keep the resolver's binding_id for consistency with
                // guard/body identifier uses and `lower`.
                let new_binding_id = *pat_binding_id;
                let new_hir_id = *pat_hir_id;
                let _ = self.scopes.declare(
                    name.clone(),
                    Binding {
                        id: new_binding_id,
                        kind: BindingKind::Variable,
                        mutable: true,
                        builtin: false,
                        ref_param: false,
                        span: *span,
                        hir_id: new_hir_id,
                    },
                );
                // Record the pattern binding's type so `lookup_type` finds it.
                self.binding_types
                    .insert(new_binding_id, scrutinee_type.clone());
                Ok(TypedPatternHir::Ident {
                    name: name.clone(),
                    binding_id: new_binding_id,
                    ty: scrutinee_type.clone(),
                    hir_id: new_hir_id,
                    span: *span,
                })
            }
            PatternHir::ResultCtor {
                ctor,
                name,
                binding_id: pat_binding_id,
                hir_id: pat_hir_id,
                span,
            } => {
                let ctor_name = match ctor {
                    ast::ResultCtor::Ok => "Ok",
                    ast::ResultCtor::Err => "Err",
                };
                let payload_ty = match (ctor, as_result_type(scrutinee_type)) {
                    (ast::ResultCtor::Ok, Some((t, _))) => t.clone(),
                    (ast::ResultCtor::Err, Some((_, e))) => e.clone(),
                    (_, None) => {
                        // Ok/Err patterns only match a Result scrutinee.
                        self.add_error(
                            TypeckErrorKind::PatternTypeMismatch {
                                pattern: ctor_name,
                                found: scrutinee_type.clone(),
                            },
                            *span,
                        );
                        return Err(());
                    }
                };
                // The payload binding registers like an identifier pattern
                // (its type is T or E). The binding is a *move* (D7); the
                // transfer is registered by the ownership checker (U05).
                let new_binding_id = *pat_binding_id;
                let new_hir_id = *pat_hir_id;
                let _ = self.scopes.declare(
                    name.clone(),
                    Binding {
                        id: new_binding_id,
                        kind: BindingKind::Variable,
                        mutable: true,
                        builtin: false,
                        ref_param: false,
                        span: *span,
                        hir_id: new_hir_id,
                    },
                );
                self.binding_types
                    .insert(new_binding_id, payload_ty.clone());
                Ok(TypedPatternHir::ResultCtor {
                    ctor: *ctor,
                    name: name.clone(),
                    binding_id: new_binding_id,
                    ty: payload_ty,
                    hir_id: new_hir_id,
                    span: *span,
                })
            }
            PatternHir::Error => Ok(TypedPatternHir::Error),
        }
    }

    /// Check choose exhaustiveness.
    fn check_exhaustiveness(
        &mut self,
        scrutinee_type: &Type,
        arms: &[TypedChooseArmHir],
        span: Span,
    ) -> Result<(), ()> {
        // Only guard-free arms count toward exhaustiveness: an arm with a
        // guard may not actually fire, so it cannot cover its pattern.
        let count = |pred: &dyn Fn(&TypedChooseArmHir) -> bool| {
            arms.iter().any(|arm| arm.guard.is_none() && pred(arm))
        };

        // Check for otherwise (parsed as wildcard pattern `_`)
        let has_otherwise = arms.iter().any(|arm| {
            arm.guard.is_none()
                && matches!(&arm.pattern, TypedPatternHir::Ident { name, .. } if name == "_" || name == "otherwise")
        });

        if has_otherwise {
            return Ok(());
        }

        // Bool is the only scrutinee type whose values we can enumerate,
        // so it needs explicit `true` and `false` arms.
        if matches!(scrutinee_type, Type::Bool) {
            let has_true = count(
                &|arm| matches!(&arm.pattern, TypedPatternHir::Literal(expr) if matches!(**expr, TypedExprHir::Bool(true, _))),
            );
            let has_false = count(
                &|arm| matches!(&arm.pattern, TypedPatternHir::Literal(expr) if matches!(**expr, TypedExprHir::Bool(false, _))),
            );

            if !has_true || !has_false {
                let mut missing = Vec::new();
                if !has_true {
                    missing.push("true".to_string());
                }
                if !has_false {
                    missing.push("false".to_string());
                }
                self.add_error(
                    TypeckErrorKind::ChooseNotExhaustive {
                        scrutinee_type: scrutinee_type.clone(),
                        missing_patterns: missing,
                    },
                    span,
                );
                return Err(());
            }
            return Ok(());
        }

        // `Result<T, E>`: `Ok` + `Err` arms together are exhaustive; each
        // alone is not (guarded arms never count, per the Bool case above).
        if matches!(scrutinee_type, Type::Result(_, _)) {
            let has_ok = count(&|arm| {
                matches!(
                    &arm.pattern,
                    TypedPatternHir::ResultCtor {
                        ctor: ast::ResultCtor::Ok,
                        ..
                    }
                )
            });
            let has_err = count(&|arm| {
                matches!(
                    &arm.pattern,
                    TypedPatternHir::ResultCtor {
                        ctor: ast::ResultCtor::Err,
                        ..
                    }
                )
            });
            if !has_ok || !has_err {
                let mut missing = Vec::new();
                if !has_ok {
                    missing.push("Ok".to_string());
                }
                if !has_err {
                    missing.push("Err".to_string());
                }
                self.add_error(
                    TypeckErrorKind::ChooseNotExhaustive {
                        scrutinee_type: scrutinee_type.clone(),
                        missing_patterns: missing,
                    },
                    span,
                );
                return Err(());
            }
            return Ok(());
        }

        // All other scrutinee types (Int, Float, String, ...) are
        // not enumerable, so `otherwise` is required.
        self.add_error(
            TypeckErrorKind::ChooseNotExhaustive {
                scrutinee_type: scrutinee_type.clone(),
                missing_patterns: vec!["otherwise".to_string()],
            },
            span,
        );
        Err(())
    }

    /// Type check a binary operator.
    fn typeck_binary_op(
        &mut self,
        lhs: ExprHir,
        rhs: ExprHir,
        op: &str,
        _result_type: Type,
    ) -> Result<TypedExprHir, ()> {
        let typed_lhs = self.typeck_expr(lhs, None)?;
        let typed_rhs = self.typeck_expr(rhs, None)?;

        // Both operands must have the same type
        if let Err(e) = unify(&typed_lhs.ty(), &typed_rhs.ty(), typed_rhs.span()) {
            self.add_error(e.kind, e.span);
            return Err(());
        }

        // For logical operators, operands must be Bool
        if matches!(op, "or" | "and")
            && let Err(e) = unify(&Type::Bool, &typed_lhs.ty(), typed_lhs.span())
        {
            self.add_error(e.kind, e.span);
            return Err(());
        }

        Ok(match op {
            "or" => TypedExprHir::Or(Box::new(typed_lhs), Box::new(typed_rhs)),
            "and" => TypedExprHir::And(Box::new(typed_lhs), Box::new(typed_rhs)),
            "==" => TypedExprHir::Eq(Box::new(typed_lhs), Box::new(typed_rhs)),
            "!=" => TypedExprHir::Ne(Box::new(typed_lhs), Box::new(typed_rhs)),
            "<" => TypedExprHir::Lt(Box::new(typed_lhs), Box::new(typed_rhs)),
            ">" => TypedExprHir::Gt(Box::new(typed_lhs), Box::new(typed_rhs)),
            "<=" => TypedExprHir::Le(Box::new(typed_lhs), Box::new(typed_rhs)),
            ">=" => TypedExprHir::Ge(Box::new(typed_lhs), Box::new(typed_rhs)),
            _ => unreachable!(),
        })
    }

    /// Type check an arithmetic operator.
    fn typeck_arith_op(
        &mut self,
        lhs: ExprHir,
        rhs: ExprHir,
        op: &str,
    ) -> Result<TypedExprHir, ()> {
        let typed_lhs = self.typeck_expr(lhs, None)?;
        let typed_rhs = self.typeck_expr(rhs, None)?;

        // Both operands must have the same type
        if let Err(e) = unify(&typed_lhs.ty(), &typed_rhs.ty(), typed_rhs.span()) {
            self.add_error(e.kind, e.span);
            return Err(());
        }

        // Operands must be numeric
        if !typed_lhs.ty().is_numeric() {
            self.add_error(
                TypeckErrorKind::InvalidOperand {
                    op: op.to_string(),
                    ty: typed_lhs.ty().clone(),
                },
                typed_lhs.span(),
            );
            return Err(());
        }

        // `%` is only defined for Int in 0.0.2 (BYTECODE.md defines IMod,
        // no FMod). Reject float operands here instead of failing later
        // in `lower`.
        if op == "%" && matches!(typed_lhs.ty(), Type::Float) {
            self.add_error(
                TypeckErrorKind::InvalidOperand {
                    op: op.to_string(),
                    ty: typed_lhs.ty().clone(),
                },
                typed_lhs.span(),
            );
            return Err(());
        }

        Ok(match op {
            "+" => TypedExprHir::Add(Box::new(typed_lhs), Box::new(typed_rhs)),
            "-" => TypedExprHir::Sub(Box::new(typed_lhs), Box::new(typed_rhs)),
            "*" => TypedExprHir::Mul(Box::new(typed_lhs), Box::new(typed_rhs)),
            "/" => TypedExprHir::Div(Box::new(typed_lhs), Box::new(typed_rhs)),
            "%" => TypedExprHir::Mod(Box::new(typed_lhs), Box::new(typed_rhs)),
            _ => unreachable!(),
        })
    }

    /// Type check a function call.
    ///
    /// `expected` applies only when the call is a `Result` constructor
    /// (`Ok(...)` / `Err(...)` inferring from context, D6); ordinary calls
    /// take their type from the callee's signature.
    fn typeck_call(
        &mut self,
        func: ExprHir,
        args: Vec<ExprHir>,
        expected: Option<&Type>,
    ) -> Result<TypedExprHir, ()> {
        // `Ok(...)` / `Err(...)`: builtin Result constructors (PLAN §3.4.1),
        // recognized by name unless the user shadowed them with their own
        // function or binding (§4.3 — same mechanism as `print`).
        if let ExprHir::Ident { ref name, .. } = func
            && (name == "Ok" || name == "Err")
            && !self.user_func_names.contains(name)
            && self.scopes.get(name).is_none()
        {
            let ctor = if name == "Ok" {
                ast::ResultCtor::Ok
            } else {
                ast::ResultCtor::Err
            };
            return self.typeck_result_ctor(ctor, args, func.span(), expected);
        }

        // Builtin functions, checked against the signature table (PLAN §6).
        // Skipped when a user declaration shadows the builtin name.
        if let ExprHir::Ident { ref name, .. } = func
            && !self.user_func_names.contains(name)
            && self.scopes.get(name).is_none()
            && let Some(sig) = find_builtin(name)
        {
            return self.typeck_builtin_call(sig, func, args);
        }

        let typed_func = self.typeck_expr(func, None)?;
        let func_type = typed_func.ty().clone();

        // Check if it's a function type
        let (param_types, ret_type) = match as_func_type(&func_type) {
            Some((params, ret)) => (params.clone(), ret.clone()),
            None => {
                self.add_error(
                    TypeckErrorKind::NotCallable { ty: func_type },
                    typed_func.span(),
                );
                return Err(());
            }
        };

        // Check argument count
        if args.len() != param_types.len() {
            self.add_error(
                TypeckErrorKind::ArityMismatch {
                    expected: param_types.len(),
                    found: args.len(),
                },
                typed_func.span(),
            );
            return Err(());
        }

        // Check argument types
        let mut typed_args = Vec::new();
        for (i, arg) in args.into_iter().enumerate() {
            let typed_arg = self.typeck_expr(arg, None)?;
            if let Err(e) =
                Self::unify_arg_ref_aware(&param_types[i], &typed_arg.ty(), i, typed_arg.span())
            {
                self.add_error(e.kind, e.span);
                return Err(());
            }
            // 0.0.2 U05: a `ref` parameter borrows the argument — it must not
            // receive a `move` (DESIGN §10.4). A borrow handle points at a
            // slot, so the argument must be a *variable* (local, global, or
            // another `ref` parameter): no box pointee, no temporary.
            match &param_types[i] {
                Type::Ref(_) => {
                    match &typed_arg {
                        TypedExprHir::Deref(..) => {
                            self.add_error(TypeckErrorKind::BorrowOfBoxInterior, typed_arg.span());
                            return Err(());
                        }
                        TypedExprHir::Ident { name, access, .. } => {
                            if *access == Access::Move {
                                self.add_error(
                                    TypeckErrorKind::BorrowArgWithMove { name: name.clone() },
                                    typed_arg.span(),
                                );
                                return Err(());
                            }
                            // `clone s` yields a fresh value, not a place: a
                            // borrow has no slot to point at.
                            if *access == Access::Clone {
                                self.add_error(
                                    TypeckErrorKind::RefArgNotAVariable,
                                    typed_arg.span(),
                                );
                                return Err(());
                            }
                        }
                        _ => {
                            self.add_error(TypeckErrorKind::RefArgNotAVariable, typed_arg.span());
                            return Err(());
                        }
                    }
                }
                param if param.is_owned() => {
                    self.reject_bare_owned_arg(&typed_arg, typed_arg.span())?;
                }
                _ => {}
            }
            typed_args.push(typed_arg);
        }

        Ok(TypedExprHir::Call(
            Box::new(typed_func),
            typed_args,
            ret_type,
        ))
    }

    /// Check a builtin call against its table signature (`builtins.rs`).
    fn typeck_builtin_call(
        &mut self,
        sig: &crate::typeck::builtins::BuiltinSig,
        func: ExprHir,
        args: Vec<ExprHir>,
    ) -> Result<TypedExprHir, ()> {
        let typed_func = self.typeck_expr(func, None)?;

        let allowed: &[Type] = match sig.param {
            BuiltinParam::Fixed(params) => {
                if args.len() != params.len() {
                    self.add_error(
                        TypeckErrorKind::ArityMismatch {
                            expected: params.len(),
                            found: args.len(),
                        },
                        typed_func.span(),
                    );
                    return Err(());
                }
                params
            }
            BuiltinParam::Variadic(elem) => elem,
        };

        let mut typed_args = Vec::new();
        for (i, arg) in args.into_iter().enumerate() {
            let typed_arg = self.typeck_expr(arg, None)?;
            // Fixed: the expected type is the i-th parameter. Variadic: any
            // of the element types — report the mismatch against the first
            // (single-element sets cover the only builtin today).
            let expected_ty = match sig.param {
                BuiltinParam::Fixed(params) => params[i].clone(),
                BuiltinParam::Variadic(elem) => elem.first().cloned().unwrap_or(Type::Unit),
            };
            // A `ref T` parameter value reads as `T` (PLAN §3.3).
            let found_ty = typed_arg.ty();
            let found_val = Self::effective_value(&found_ty);
            if !allowed.contains(found_val) {
                self.add_error(
                    TypeckErrorKind::ArgTypeMismatch {
                        index: i,
                        expected: expected_ty,
                        found: found_val.clone(),
                    },
                    typed_arg.span(),
                );
                return Err(());
            }
            typed_args.push(typed_arg);
        }

        Ok(TypedExprHir::Call(
            Box::new(typed_func),
            typed_args,
            sig.ret.clone(),
        ))
    }

    /// Check a `Ok(...)` / `Err(...)` constructor call (PLAN §3.4.1, D6).
    ///
    /// The constructor's type comes from context: the expected `Result<T, E>`
    /// determines which payload slot (`T` for `Ok`, `E` for `Err`) the
    /// argument must satisfy. Without a `Result` context the constructor
    /// cannot be typed at all.
    fn typeck_result_ctor(
        &mut self,
        ctor: ast::ResultCtor,
        args: Vec<ExprHir>,
        span: Span,
        expected: Option<&Type>,
    ) -> Result<TypedExprHir, ()> {
        let ctor_name = if ctor == ast::ResultCtor::Ok {
            "Ok"
        } else {
            "Err"
        };

        // Exactly one payload argument.
        if args.len() != 1 {
            self.add_error(
                TypeckErrorKind::ArityMismatch {
                    expected: 1,
                    found: args.len(),
                },
                span,
            );
            return Err(());
        }

        // Payload slot is selected by the constructor and the context type.
        let Some(ctx) = expected.and_then(|ty| as_result_type(ty)) else {
            // No context (or a non-Result context): the constructor cannot
            // be typed — require an annotation (D6).
            self.add_error(
                TypeckErrorKind::CannotInferResultType { ctor: ctor_name },
                span,
            );
            return Err(());
        };
        let payload_ty = if ctor == ast::ResultCtor::Ok {
            ctx.0.clone()
        } else {
            ctx.1.clone()
        };

        let mut args = args;
        let arg = args.swap_remove(0);
        let arg_span = arg.span();
        let typed_value = self.typeck_expr(arg, Some(&payload_ty))?;
        if let Err(e) = unify_arg(&payload_ty, &typed_value.ty(), 0, arg_span) {
            self.add_error(e.kind, e.span);
            return Err(());
        }
        // The payload is *moved into* the Result value (D7).
        self.reject_bare_owned_arg(&typed_value, arg_span)?;

        let full_ty = Type::Result(Box::new(ctx.0.clone()), Box::new(ctx.1.clone()));
        Ok(TypedExprHir::ResultCtor {
            ctor,
            value: Box::new(typed_value),
            ty: full_ty,
            span,
        })
    }

    /// Check a `expr?` Result propagation expression (PLAN §3.4.3, D6).
    fn typeck_question(&mut self, operand: ExprHir, span: Span) -> Result<TypedExprHir, ()> {
        let typed_operand = self.typeck_expr(operand, None)?;

        // The operand itself must be a Result.
        let operand_ty = typed_operand.ty();
        let Some((t, e)) = as_result_type(&operand_ty) else {
            self.add_error(
                TypeckErrorKind::QuestionOnNonResult {
                    found: typed_operand.ty(),
                },
                span,
            );
            return Err(());
        };

        // `?` may only appear where the enclosing function's declared return
        // type is `Result<T2, E>` with the *same* error type (D6; 0.0.2 has
        // no error-type conversion). Outside any function the check fails
        // the same way — there is no return type to propagate to.
        match self.fn_ret_stack.last().and_then(|rt| rt.as_ref()) {
            Some(Type::Result(_, fn_e)) => {
                if **fn_e != *e {
                    self.add_error(
                        TypeckErrorKind::QuestionTypeMismatch {
                            expected: (**fn_e).clone(),
                            found: e.clone(),
                        },
                        span,
                    );
                    return Err(());
                }
            }
            _ => {
                self.add_error(TypeckErrorKind::QuestionOutsideResultFn, span);
                return Err(());
            }
        }

        Ok(TypedExprHir::Question {
            operand: Box::new(typed_operand),
            ty: t.clone(),
            span,
        })
    }

    /// 0.0.2 U05 (PLAN §3.1.3 rule 2): a *bare* owned local value in a
    /// consuming position (binding/assignment RHS, argument, `choose`
    /// scrutinee, `box` inner, `Ok`/`Err` payload) must transfer ownership
    /// explicitly via `move` / `clone`. Reports `OwnedArgRequiresMove`.
    ///
    /// A bare `ref T` parameter value is caught too: it borrows, so a
    /// consuming position must copy it out (`clone s`).
    ///
    /// Passes silently for: Copy values, `move`/`clone` wrappers, owned
    /// *globals* (reads are deep copies, D5), fresh values (literals, call
    /// results, ...).
    fn reject_bare_owned_arg(&mut self, expr: &TypedExprHir, span: Span) -> Result<(), ()> {
        if let TypedExprHir::Ident {
            name,
            binding_id,
            ty,
            access,
            ..
        } = expr
            && *access == Access::Copy
            && (ty.is_owned() || matches!(ty, Type::Ref(_)))
            && !self.global_ids.contains(binding_id)
        {
            self.add_error(
                TypeckErrorKind::OwnedArgRequiresMove {
                    name: name.clone(),
                    ty: Self::effective_value(ty).clone(),
                },
                span,
            );
            return Err(());
        }
        Ok(())
    }

    /// 0.0.2 U05: type check `move <place>` (PLAN §3.1.2).
    ///
    /// The operand must be a local variable (or a `deref` place, which is
    /// rejected by name): Copy types make `move` meaningless, owned globals
    /// and `ref` parameters cannot be moved. `move x` folds into
    /// `Ident { access: Move }` — the transfer itself is registered by the
    /// flow-sensitive ownership pass.
    fn typeck_move(&mut self, inner: ExprHir, span: Span) -> Result<TypedExprHir, ()> {
        match inner {
            ExprHir::Ident {
                name,
                binding_id,
                hir_id,
                span: ident_span,
            } => {
                let ty = self.lookup_type(&name, ident_span)?;
                if ty.is_copy() {
                    self.add_error(
                        TypeckErrorKind::MoveOfCopyType {
                            name: name.clone(),
                            ty: ty.clone(),
                        },
                        ident_span,
                    );
                    return Err(());
                }
                if self.global_ids.contains(&binding_id) {
                    self.add_error(TypeckErrorKind::MoveOutOfGlobal { name }, ident_span);
                    return Err(());
                }
                if matches!(ty, Type::Ref(_)) {
                    self.add_error(TypeckErrorKind::MoveOfBorrowed { name }, ident_span);
                    return Err(());
                }
                Ok(TypedExprHir::Ident {
                    name,
                    binding_id,
                    ty,
                    access: Access::Move,
                    hir_id,
                    span: ident_span,
                })
            }
            ExprHir::Deref(_, _) => {
                // The box keeps its unique ownership; the pointee can only
                // be copied out (`clone deref b`).
                self.add_error(TypeckErrorKind::MoveOutOfBox, span);
                Err(())
            }
            _ => {
                self.add_error(TypeckErrorKind::InvalidClonePlace, span);
                Err(())
            }
        }
    }

    /// 0.0.2 U05: type check `clone <place>` (PLAN §3.1.2, §3.3.1).
    ///
    /// The operand is a local variable or a box pointee (`clone deref b`).
    /// A `clone` never empties a slot: for owned values and `ref`
    /// parameters the result carries `Access::Clone`. `clone s` of a
    /// `ref T` parameter yields the *value type* `T` — an owned copy that
    /// leaves the borrow (PLAN §3.3.1). Copy values are plain reads.
    fn typeck_clone(&mut self, inner: ExprHir, span: Span) -> Result<TypedExprHir, ()> {
        match inner {
            ExprHir::Ident {
                name,
                binding_id,
                hir_id,
                span: ident_span,
            } => {
                let ty = self.lookup_type(&name, ident_span)?;
                // `ref T` clones to an owned `T` (the value leaves the
                // borrow); every other type keeps its declared type.
                let value_ty = match ty {
                    Type::Ref(inner_ty) => *inner_ty,
                    other => other,
                };
                let access = if value_ty.is_copy() {
                    Access::Copy
                } else {
                    Access::Clone
                };
                Ok(TypedExprHir::Ident {
                    name,
                    binding_id,
                    ty: value_ty,
                    access,
                    hir_id,
                    span: ident_span,
                })
            }
            ExprHir::Deref(box_expr, _) => self.typeck_deref(*box_expr, span),
            _ => {
                self.add_error(TypeckErrorKind::InvalidClonePlace, span);
                Err(())
            }
        }
    }

    /// 0.0.2 U05: type check `box <expr>` (PLAN §3.2.1).
    ///
    /// The inner value is *consumed* into the fresh allocation, so a bare
    /// owned local must be written `move` / `clone` there as well.
    fn typeck_box(&mut self, inner: ExprHir, span: Span) -> Result<TypedExprHir, ()> {
        let typed_inner = self.typeck_expr(inner, None)?;
        self.reject_bare_owned_arg(&typed_inner, typed_inner.span())?;
        Ok(TypedExprHir::Box(Box::new(typed_inner), span))
    }

    /// 0.0.2 U05: type check `deref <box>` (PLAN §3.2.1).
    ///
    /// Reads the pointee as a copy; the box itself stays in its slot. The
    /// operand must be a variable holding a `box<T>` value.
    fn typeck_deref(&mut self, inner: ExprHir, span: Span) -> Result<TypedExprHir, ()> {
        let typed_inner = self.typeck_expr(inner, None)?;
        let TypedExprHir::Ident { ty, .. } = &typed_inner else {
            self.add_error(
                TypeckErrorKind::InvalidOperand {
                    op: "deref".to_string(),
                    ty: typed_inner.ty().clone(),
                },
                span,
            );
            return Err(());
        };
        match ty {
            Type::Box(_) => Ok(TypedExprHir::Deref(Box::new(typed_inner), span)),
            other => {
                self.add_error(
                    TypeckErrorKind::InvalidOperand {
                        op: "deref".to_string(),
                        ty: other.clone(),
                    },
                    typed_inner.span(),
                );
                Err(())
            }
        }
    }

    /// 0.0.2 U05: type check `deref <b> = <expr>` (PLAN §3.2.1).
    ///
    /// The target must be a *local* `box<T>` binding: through a global the
    /// write would only reach a deep copy of the global, so it is rejected
    /// as `DerefAssignOfGlobalBox` (0.0.2 limit, DESIGN §10.3 / U12).
    /// The pointee is replaced — a bare owned RHS must transfer explicitly.
    fn typeck_assign_deref(
        &mut self,
        name: String,
        binding_id: BindingId,
        rhs: ExprHir,
        hir_id: HirId,
        span: Span,
    ) -> Result<TypedExprHir, ()> {
        let target_ty = self.lookup_type(&name, span)?;
        let pointee_ty = match target_ty {
            Type::Box(pointee) => *pointee,
            other => {
                self.add_error(
                    TypeckErrorKind::InvalidOperand {
                        op: "deref =".to_string(),
                        ty: other,
                    },
                    span,
                );
                return Err(());
            }
        };

        if self.global_ids.contains(&binding_id) {
            self.add_error(TypeckErrorKind::DerefAssignOfGlobalBox, span);
            return Err(());
        }

        let typed_rhs = self.typeck_expr(rhs, None)?;
        if let Err(e) = unify_assign(&pointee_ty, &typed_rhs.ty(), typed_rhs.span()) {
            self.add_error(e.kind, e.span);
            return Err(());
        }
        self.reject_bare_owned_arg(&typed_rhs, typed_rhs.span())?;

        Ok(TypedExprHir::AssignDeref {
            name,
            binding_id,
            rhs: Box::new(typed_rhs),
            ty: Type::Unit,
            hir_id,
            span,
        })
    }

    /// 0.0.2 U05: argument unification that understands `ref` parameters —
    /// a `ref T` parameter accepts a value of type `T` (a local variable
    /// passed by borrow; lowering emits `MakeRefLocal`, U06).
    fn unify_arg_ref_aware(
        expected: &Type,
        found: &Type,
        index: usize,
        span: Span,
    ) -> Result<(), TypeckError> {
        let found_value = Self::effective_value(found);
        match expected {
            Type::Ref(inner) if inner.as_ref() == found_value => Ok(()),
            _ => unify_arg(expected, found_value, index, span),
        }
    }

    /// 0.0.2 U05 (PLAN §3.3.1): the value type of a `ref T` place. A
    /// borrow forwards as a `ref T` argument (handle flow, zero copy), and
    /// read-only builtin arguments accept the borrowed value — but the
    /// coercion is *not* applied to assignments, returns, or owned
    /// parameters: `ref T ≠ T` is strict there (write `clone s`).
    fn effective_value(ty: &Type) -> &Type {
        match ty {
            Type::Ref(inner) => inner,
            other => other,
        }
    }

    /// Type check an index expression.
    fn typeck_index(&mut self, arr: ExprHir, idx: ExprHir) -> Result<TypedExprHir, ()> {
        let typed_arr = self.typeck_expr(arr, None)?;
        let typed_idx = self.typeck_expr(idx, None)?;

        // Index must be Int
        if let Err(e) = unify(&Type::Int, &typed_idx.ty(), typed_idx.span()) {
            self.add_error(e.kind, e.span);
            return Err(());
        }

        // Arrays are parsed but not supported in 0.0.2.
        match typed_arr.ty() {
            Type::Array(_) => {
                self.add_error(
                    TypeckErrorKind::UnsupportedFeature {
                        feature: "array indexing".to_string(),
                    },
                    typed_arr.span(),
                );
                Err(())
            }
            other => {
                self.add_error(
                    TypeckErrorKind::TypeMismatch {
                        expected: Type::Array(Box::new(Type::Unsupported("elem".to_string()))),
                        found: other,
                    },
                    typed_arr.span(),
                );
                Err(())
            }
        }
    }

    /// Type check a field access.
    fn typeck_field(&mut self, obj: ExprHir, _field: String) -> Result<TypedExprHir, ()> {
        let typed_obj = self.typeck_expr(obj, None)?;

        // Structs/fields are parsed but not supported in 0.0.2.
        self.add_error(
            TypeckErrorKind::UnsupportedFeature {
                feature: "field access".to_string(),
            },
            typed_obj.span(),
        );
        Err(())
    }

    /// Type check an `as` cast expression (0.0.2 U13).
    ///
    /// 0.0.2 whitelist: only scalar (int/float/bool) → string is allowed.
    /// All other casts are rejected with `UnsupportedCast`.
    fn typeck_cast(
        &mut self,
        inner: ExprHir,
        target: ast::Type,
        span: Span,
    ) -> Result<TypedExprHir, ()> {
        let typed_inner = self.typeck_expr(inner, None)?;
        let target_ty = convert_type(&target);

        // 0.0.2 whitelist: only int/float/bool → string.
        let valid = matches!(typed_inner.ty(), Type::Int | Type::Float | Type::Bool)
            && matches!(target_ty, Type::String);
        if !valid {
            self.add_error(
                TypeckErrorKind::UnsupportedCast {
                    from: typed_inner.ty(),
                    to: target_ty,
                },
                span,
            );
            return Err(());
        }

        Ok(TypedExprHir::Cast(Box::new(typed_inner), span))
    }

    /// Look up the type of a variable.
    fn lookup_type(&mut self, name: &str, span: Span) -> Result<Type, ()> {
        // First check function signatures
        if let Some((params, ret)) = self.func_signatures.get(name) {
            return Ok(Type::Func(params.clone(), Box::new(ret.clone())));
        }

        // Then check scope
        match self.scopes.get(name) {
            Some(binding) => {
                // Look up the type from our binding_types map
                if let Some(ty) = self.binding_types.get(&binding.id) {
                    Ok(ty.clone())
                } else {
                    // The resolver should have registered every binding with
                    // a type; reaching this means the resolver and typeck
                    // disagree — treat as a compiler bug, never silently
                    // invent a type.
                    self.add_error(
                        TypeckErrorKind::InternalError {
                            message: format!("no recorded type for binding `{name}`"),
                        },
                        span,
                    );
                    Err(())
                }
            }
            None => {
                self.add_error(
                    TypeckErrorKind::UndefinedVariable {
                        name: name.to_string(),
                    },
                    span,
                );
                Err(())
            }
        }
    }
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}
