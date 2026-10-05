//! Type inference for the type checker.
//!
//! This module contains the core type inference logic, transforming
//! HIR expressions into Typed HIR expressions with type information.

use crate::lexer::Span;
use crate::parser::ast::{self, *};
use crate::resolver::hir::*;
use crate::resolver::scope::{Binding, BindingKind, ScopeStack};
use crate::typeck::error::{TypeckError, TypeckErrorKind};
use crate::typeck::typed_hir::{
    Type, TypedBlockHir, TypedChooseArmHir, TypedConstDeclHir, TypedDeclHir, TypedExprChooseHir,
    TypedExprHir, TypedExprIfHir, TypedExprWhileHir, TypedFuncBodyHir, TypedFuncDeclHir, TypedHir,
    TypedHirItem, TypedParamHir, TypedPatternHir, TypedStmtHir, TypedVarBindingHir,
};
use crate::typeck::unify::{as_func_type, as_result_type, unify, unify_assign};
use std::collections::HashMap;

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
    hir_id_counter: u32,
    /// Counter for generating unique binding IDs.
    binding_id_counter: u32,
    /// Function signatures collected in forward pass.
    func_signatures: HashMap<String, (Vec<Type>, Type)>,
    /// Binding ID to type mapping.
    binding_types: HashMap<BindingId, Type>,
}

impl TypeChecker {
    /// Create a new type checker.
    pub fn new() -> Self {
        Self {
            scopes: ScopeStack::new(),
            errors: Vec::new(),
            hir_id_counter: 0,
            binding_id_counter: 0,
            func_signatures: HashMap::new(),
            binding_types: HashMap::new(),
        }
    }

    /// Generate the next HIR ID.
    fn next_hir_id(&mut self) -> HirId {
        let id = HirId(self.hir_id_counter);
        self.hir_id_counter += 1;
        id
    }

    /// Generate the next binding ID.
    fn next_binding_id(&mut self) -> BindingId {
        let id = BindingId(self.binding_id_counter);
        self.binding_id_counter += 1;
        id
    }

    /// Add an error to the collection.
    fn add_error(&mut self, kind: TypeckErrorKind, span: Span) {
        self.errors.push(TypeckError::new(kind, span));
    }

    /// Register builtin functions.
    fn register_builtins(&mut self) {
        // print: (string) -> unit
        let print_sig = (vec![Type::String], Type::Unit);
        self.func_signatures.insert("print".to_string(), print_sig);
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
            }
        }
    }

    /// Type check the entire program.
    pub fn typeck(mut self, hir: Hir) -> Result<TypedHir, Vec<TypeckError>> {
        self.register_builtins();
        self.collect_func_signatures(&hir);

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

    /// Type check a top-level item.
    fn typeck_item(&mut self, item: HirItem) -> Result<TypedHirItem, ()> {
        match item {
            HirItem::Import(import) => Ok(TypedHirItem::Import(import)),
            HirItem::Decl(decl) => Ok(TypedHirItem::Decl(self.typeck_decl(decl)?)),
            HirItem::Expr(expr) => Ok(TypedHirItem::Expr(self.typeck_expr(expr)?)),
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
            let binding_id = self.next_binding_id();
            let hir_id = self.next_hir_id();
            let param_type = convert_type(&param.ty);
            let _ = self.scopes.declare(
                param.name.clone(),
                Binding {
                    id: binding_id,
                    kind: BindingKind::Parameter,
                    mutable: false,
                    builtin: false,
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

        // Type check body
        let typed_body = match &func.body {
            FuncBodyHir::SingleExpr(expr) => {
                let typed_expr = self.typeck_expr((**expr).clone())?;
                // Check return type
                if let Some(ref ret_type) = func.ret_type {
                    let typed_ret_type = convert_type(ret_type);
                    if let Err(e) = unify(&typed_ret_type, &typed_expr.ty(), typed_expr.span()) {
                        self.add_error(e.kind, e.span);
                        return Err(());
                    }
                }
                TypedFuncBodyHir::SingleExpr(Box::new(typed_expr))
            }
            FuncBodyHir::Block(block) => {
                let typed_block = self.typeck_block(block)?;
                // Check return type
                if let Some(ref ret_type) = func.ret_type {
                    let typed_ret_type = convert_type(ret_type);
                    if let Err(e) = unify(&typed_ret_type, &typed_block.ty, typed_block.span) {
                        self.add_error(e.kind, e.span);
                        return Err(());
                    }
                }
                TypedFuncBodyHir::Block(typed_block)
            }
        };

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

    /// Type check a variable binding.
    fn typeck_var_binding(&mut self, var: VarBindingHir) -> Result<TypedVarBindingHir, ()> {
        let typed_init = self.typeck_expr(*var.init)?;

        // Check type annotation if present
        let typed_ty = var.ty.as_ref().map(convert_type);
        if let Some(ref annotated_ty) = typed_ty
            && let Err(e) = unify(annotated_ty, &typed_init.ty(), typed_init.span())
        {
            self.add_error(e.kind, e.span);
            return Err(());
        }

        // Check if this is an assignment (binding_id already in binding_types)
        if let Some(existing_type) = self.binding_types.get(&var.binding_id) {
            // This is an assignment, not a new binding
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
                span: var.span,
                hir_id,
            },
        );

        // Store the type in our binding_types map
        let var_type = typed_ty.clone().unwrap_or_else(|| typed_init.ty());
        self.binding_types.insert(binding_id, var_type);

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
        let typed_init = self.typeck_expr(*c.init)?;

        // Check type annotation if present
        let typed_ty = c.ty.as_ref().map(convert_type);
        if let Some(ref annotated_ty) = typed_ty
            && let Err(e) = unify(annotated_ty, &typed_init.ty(), typed_init.span())
        {
            self.add_error(e.kind, e.span);
            return Err(());
        }

        // Add binding to scope
        let binding_id = self.next_binding_id();
        let hir_id = self.next_hir_id();
        let _ = self.scopes.declare(
            c.name.clone(),
            Binding {
                id: binding_id,
                kind: BindingKind::Variable,
                mutable: false,
                builtin: false,
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
    fn typeck_block(&mut self, block: &BlockHir) -> Result<TypedBlockHir, ()> {
        self.scopes.enter_block_scope();

        let mut typed_stmts = Vec::new();
        for stmt in &block.stmts {
            match self.typeck_stmt(stmt) {
                Ok(s) => typed_stmts.push(s),
                Err(()) => typed_stmts.push(TypedStmtHir::Error),
            }
        }

        let typed_tail = match &block.tail_expr {
            Some(expr) => Some(Box::new(self.typeck_expr((**expr).clone())?)),
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
                let typed_expr = self.typeck_expr((**expr).clone())?;
                Ok(TypedStmtHir::Expr(Box::new(typed_expr), *has_semi))
            }
            StmtHir::Error => Ok(TypedStmtHir::Error),
        }
    }

    /// Type check an expression.
    fn typeck_expr(&mut self, expr: ExprHir) -> Result<TypedExprHir, ()> {
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
                Ok(TypedExprHir::Ident {
                    name,
                    binding_id,
                    ty,
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
                let typed_rhs = self.typeck_expr(*rhs)?;
                let var_type = self.lookup_type(&name, span)?;

                if let Err(e) = unify_assign(&var_type, &typed_rhs.ty(), typed_rhs.span()) {
                    self.add_error(e.kind, e.span);
                    return Err(());
                }

                Ok(TypedExprHir::Assign {
                    name,
                    binding_id,
                    rhs: Box::new(typed_rhs),
                    ty: Type::Unit,
                    hir_id,
                    span,
                })
            }

            ExprHir::If(if_expr) => self.typeck_if_expr(if_expr),
            ExprHir::While(while_expr) => self.typeck_while_expr(while_expr),
            ExprHir::Choose(choose_expr) => self.typeck_choose_expr(choose_expr),

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
                let typed_expr = self.typeck_expr(*expr)?;
                if let Err(e) = unify(&Type::Bool, &typed_expr.ty(), typed_expr.span()) {
                    self.add_error(e.kind, e.span);
                    return Err(());
                }
                Ok(TypedExprHir::Not(Box::new(typed_expr)))
            }

            ExprHir::Neg(expr) => {
                let typed_expr = self.typeck_expr(*expr)?;
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

            ExprHir::Call(func, args) => self.typeck_call(*func, args),
            ExprHir::Index(arr, idx) => self.typeck_index(*arr, *idx),
            ExprHir::Field(obj, field) => self.typeck_field(*obj, field),
            ExprHir::Block(block) => {
                let typed_block = self.typeck_block(&block)?;
                Ok(TypedExprHir::Block(typed_block))
            }
        }
    }

    /// Type check an if expression.
    fn typeck_if_expr(&mut self, if_expr: ExprIfHir) -> Result<TypedExprHir, ()> {
        let typed_cond = self.typeck_expr(*if_expr.condition)?;

        // Condition must be Bool
        if let Err(e) = unify(&Type::Bool, &typed_cond.ty(), typed_cond.span()) {
            self.add_error(e.kind, e.span);
            return Err(());
        }

        let typed_then = self.typeck_block(&if_expr.then_branch)?;

        let mut typed_elifs = Vec::new();
        for (cond, block) in &if_expr.elif_branches {
            let typed_cond = self.typeck_expr((**cond).clone())?;
            if let Err(e) = unify(&Type::Bool, &typed_cond.ty(), typed_cond.span()) {
                self.add_error(e.kind, e.span);
                return Err(());
            }
            let typed_block = self.typeck_block(block)?;
            typed_elifs.push((Box::new(typed_cond), typed_block));
        }

        let typed_else = match &if_expr.else_branch {
            Some(block) => Some(self.typeck_block(block)?),
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
        let typed_cond = self.typeck_expr(*while_expr.condition)?;

        // Condition must be Bool
        if let Err(e) = unify(&Type::Bool, &typed_cond.ty(), typed_cond.span()) {
            self.add_error(e.kind, e.span);
            return Err(());
        }

        let typed_body = self.typeck_block(&while_expr.body)?;

        Ok(TypedExprHir::While(TypedExprWhileHir {
            condition: Box::new(typed_cond),
            body: typed_body,
            ty: Type::Unit,
            hir_id: while_expr.hir_id,
            span: while_expr.span,
        }))
    }

    /// Type check a choose expression.
    fn typeck_choose_expr(&mut self, choose_expr: ExprChooseHir) -> Result<TypedExprHir, ()> {
        let typed_scrutinee = self.typeck_expr(*choose_expr.scrutinee)?;
        let scrutinee_type = typed_scrutinee.ty().clone();

        let mut typed_arms = Vec::new();
        for arm in &choose_expr.arms {
            self.scopes.enter_block_scope();

            let typed_pattern = self.typeck_pattern(&arm.pattern, &scrutinee_type)?;

            let typed_guard = match &arm.guard {
                Some(guard) => {
                    let typed_guard = self.typeck_expr((**guard).clone())?;
                    if let Err(e) = unify(&Type::Bool, &typed_guard.ty(), typed_guard.span()) {
                        self.add_error(e.kind, e.span);
                        return Err(());
                    }
                    Some(Box::new(typed_guard))
                }
                None => None,
            };

            let typed_body = self.typeck_block(&arm.body)?;

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
            // Empty choose (should not happen in valid code)
            Ok(TypedExprHir::Choose(TypedExprChooseHir {
                scrutinee: Box::new(typed_scrutinee),
                arms: typed_arms,
                ty: Type::Unit,
                hir_id: choose_expr.hir_id,
                span: choose_expr.span,
            }))
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
                let typed_expr = self.typeck_expr((**expr).clone())?;
                if let Err(e) = unify(scrutinee_type, &typed_expr.ty(), typed_expr.span()) {
                    self.add_error(e.kind, e.span);
                    return Err(());
                }
                Ok(TypedPatternHir::Literal(Box::new(typed_expr)))
            }
            PatternHir::Ident { name, span, .. } => {
                // Pattern binding: new variable with scrutinee type
                let new_binding_id = self.next_binding_id();
                let new_hir_id = self.next_hir_id();
                let _ = self.scopes.declare(
                    name.clone(),
                    Binding {
                        id: new_binding_id,
                        kind: BindingKind::Variable,
                        mutable: true,
                        builtin: false,
                        span: *span,
                        hir_id: new_hir_id,
                    },
                );
                Ok(TypedPatternHir::Ident {
                    name: name.clone(),
                    binding_id: new_binding_id,
                    ty: scrutinee_type.clone(),
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
        // Check for otherwise (parsed as wildcard pattern `_`)
        let has_otherwise = arms.iter().any(|arm| {
            matches!(&arm.pattern, TypedPatternHir::Ident { name, .. } if name == "_" || name == "otherwise")
        });

        if has_otherwise {
            return Ok(());
        }

        // Check for Bool exhaustiveness
        if matches!(scrutinee_type, Type::Bool) {
            let has_true = arms.iter().any(|arm| {
                matches!(&arm.pattern, TypedPatternHir::Literal(expr) if matches!(**expr, TypedExprHir::Bool(true, _)))
            });
            let has_false = arms.iter().any(|arm| {
                matches!(&arm.pattern, TypedPatternHir::Literal(expr) if matches!(**expr, TypedExprHir::Bool(false, _)))
            });

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
        }

        // Check for Result exhaustiveness
        if as_result_type(scrutinee_type).is_some() {
            let has_ok = arms.iter().any(|arm| {
                matches!(&arm.pattern, TypedPatternHir::Literal(expr) if matches!(**expr, TypedExprHir::Call(..)))
            });
            let has_err = arms.iter().any(|arm| {
                matches!(&arm.pattern, TypedPatternHir::Literal(expr) if matches!(**expr, TypedExprHir::Call(..)))
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
        }

        // For other types (Int, Float, String), require otherwise
        // since we can't enumerate all possible values
        if !matches!(scrutinee_type, Type::Bool | Type::Result(..)) {
            self.add_error(
                TypeckErrorKind::ChooseNotExhaustive {
                    scrutinee_type: scrutinee_type.clone(),
                    missing_patterns: vec!["otherwise".to_string()],
                },
                span,
            );
            return Err(());
        }

        Ok(())
    }

    /// Type check a binary operator.
    fn typeck_binary_op(
        &mut self,
        lhs: ExprHir,
        rhs: ExprHir,
        op: &str,
        _result_type: Type,
    ) -> Result<TypedExprHir, ()> {
        let typed_lhs = self.typeck_expr(lhs)?;
        let typed_rhs = self.typeck_expr(rhs)?;

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
        let typed_lhs = self.typeck_expr(lhs)?;
        let typed_rhs = self.typeck_expr(rhs)?;

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
    fn typeck_call(&mut self, func: ExprHir, args: Vec<ExprHir>) -> Result<TypedExprHir, ()> {
        let typed_func = self.typeck_expr(func)?;
        let func_type = typed_func.ty().clone();

        // Check if it's a function type
        let (param_types, ret_type) = match as_func_type(&func_type) {
            Some((params, ret)) => (params.clone(), ret.clone()),
            None => {
                self.add_error(
                    TypeckErrorKind::TypeMismatch {
                        expected: Type::Func(vec![], Box::new(Type::Unit)),
                        found: func_type,
                    },
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
            let typed_arg = self.typeck_expr(arg)?;
            if let Err(e) = unify(&param_types[i], &typed_arg.ty(), typed_arg.span()) {
                self.add_error(e.kind, e.span);
                return Err(());
            }
            typed_args.push(typed_arg);
        }

        Ok(TypedExprHir::Call(
            Box::new(typed_func),
            typed_args,
            ret_type,
        ))
    }

    /// Type check an index expression.
    fn typeck_index(&mut self, arr: ExprHir, idx: ExprHir) -> Result<TypedExprHir, ()> {
        let typed_arr = self.typeck_expr(arr)?;
        let typed_idx = self.typeck_expr(idx)?;

        // Index must be Int
        if let Err(e) = unify(&Type::Int, &typed_idx.ty(), typed_idx.span()) {
            self.add_error(e.kind, e.span);
            return Err(());
        }

        // Array type (unsupported in 0.0.1)
        Ok(TypedExprHir::Index(
            Box::new(typed_arr),
            Box::new(typed_idx),
        ))
    }

    /// Type check a field access.
    fn typeck_field(&mut self, obj: ExprHir, field: String) -> Result<TypedExprHir, ()> {
        let typed_obj = self.typeck_expr(obj)?;

        // Field access (unsupported in 0.0.1)
        Ok(TypedExprHir::Field(Box::new(typed_obj), field))
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
                    // Fallback: if we don't have a type, assume Unit
                    // This shouldn't happen in well-formed code
                    Ok(Type::Unit)
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
