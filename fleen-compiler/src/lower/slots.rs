//! Local slot allocation (no SSA).
//!
//! Every binding inside a function gets a distinct slot. Parameters occupy
//! slots `0..params`; everything else is appended in first-appearance
//! order. Shadowing produces a new slot (the new `BindingId` is distinct
//! from the outer one, so this falls out naturally); the old slot is
//! simply no longer referenced.

use crate::resolver::hir::BindingId;
use crate::typeck::typed_hir::*;
use std::collections::HashMap;

/// Maps binding IDs to slots for one function.
#[derive(Debug, Default)]
pub struct SlotAlloc {
    /// BindingId → slot for *local* bindings (params, vars, consts, patterns).
    pub slot_of: HashMap<BindingId, u16>,
    /// Next slot to assign.
    next: u16,
}

impl SlotAlloc {
    /// Create an allocator preloaded with the parameter slots `0..params`.
    pub fn new(func: &TypedFuncDeclHir) -> Self {
        let mut alloc = SlotAlloc {
            slot_of: HashMap::new(),
            next: 0,
        };
        for param in &func.params {
            alloc.slot_of.insert(param.binding_id, alloc.next);
            alloc.next += 1;
        }
        alloc
    }

    /// Allocate a slot for `binding_id` (idempotent).
    pub fn slot_for(&mut self, binding_id: BindingId) -> u16 {
        if let Some(&slot) = self.slot_of.get(&binding_id) {
            return slot;
        }
        let slot = self.next;
        self.next += 1;
        self.slot_of.insert(binding_id, slot);
        slot
    }

    /// Look up the slot of an existing local binding.
    pub fn get(&self, binding_id: BindingId) -> Option<u16> {
        self.slot_of.get(&binding_id).copied()
    }

    /// Total number of slots (locals count for `MirFunc::locals`).
    pub fn total(&self) -> u16 {
        self.next
    }

    /// Allocate a fresh temporary slot (for global ref arguments, 0.0.2 U06).
    pub fn fresh(&mut self) -> u16 {
        let slot = self.next;
        self.next += 1;
        slot
    }
}

/// Walk a function body collecting every local binding (declarations and
/// pattern identifiers) in first-appearance order, assigning each a slot.
///
/// Callers must lower from the same pass: slot assignment is order
/// dependent only for reproducibility, not correctness (each distinct
/// `BindingId` gets a distinct slot either way).
pub fn allocate_slots(func: &TypedFuncDeclHir) -> SlotAlloc {
    let mut alloc = SlotAlloc::new(func);
    match &func.body {
        TypedFuncBodyHir::SingleExpr(e) => collect_expr(&mut alloc, e),
        TypedFuncBodyHir::Block(b) => collect_block(&mut alloc, b),
    }
    alloc
}

fn collect_block(alloc: &mut SlotAlloc, block: &TypedBlockHir) {
    for stmt in &block.stmts {
        match stmt {
            TypedStmtHir::Decl(decl) => match decl {
                TypedDeclHir::Var(v) => {
                    collect_expr(alloc, &v.init);
                    alloc.slot_for(v.binding_id);
                }
                TypedDeclHir::Const(c) => {
                    collect_expr(alloc, &c.init);
                    alloc.slot_for(c.binding_id);
                }
                TypedDeclHir::Func(f) => {
                    // Nested function declarations create their own frames;
                    // but Fleen 0.0.1 functions are top-level, so nothing to do.
                    let _ = f;
                }
            },
            TypedStmtHir::Expr(e, _) => collect_expr(alloc, e),
            TypedStmtHir::Error => {}
        }
    }
    if let Some(tail) = &block.tail_expr {
        collect_expr(alloc, tail);
    }
}

fn collect_expr(alloc: &mut SlotAlloc, expr: &TypedExprHir) {
    match expr {
        TypedExprHir::Assign { rhs, .. } => collect_expr(alloc, rhs),
        TypedExprHir::If(e) => {
            collect_expr(alloc, &e.condition);
            collect_block(alloc, &e.then_branch);
            for (cond, branch) in &e.elif_branches {
                collect_expr(alloc, cond);
                collect_block(alloc, branch);
            }
            if let Some(else_branch) = &e.else_branch {
                collect_block(alloc, else_branch);
            }
        }
        TypedExprHir::While(e) => {
            collect_expr(alloc, &e.condition);
            collect_block(alloc, &e.body);
        }
        TypedExprHir::Choose(e) => {
            collect_expr(alloc, &e.scrutinee);
            for arm in &e.arms {
                match &arm.pattern {
                    TypedPatternHir::Literal(lit) => collect_expr(alloc, lit),
                    TypedPatternHir::Ident { binding_id, .. } => {
                        alloc.slot_for(*binding_id);
                    }
                    TypedPatternHir::ResultCtor { binding_id, .. } => {
                        alloc.slot_for(*binding_id);
                    }
                    TypedPatternHir::Error => {}
                }
                if let Some(guard) = &arm.guard {
                    collect_expr(alloc, guard);
                }
                collect_block(alloc, &arm.body);
            }
        }
        TypedExprHir::Or(l, r) | TypedExprHir::And(l, r) => {
            collect_expr(alloc, l);
            collect_expr(alloc, r);
        }
        TypedExprHir::Eq(l, r)
        | TypedExprHir::Ne(l, r)
        | TypedExprHir::Lt(l, r)
        | TypedExprHir::Gt(l, r)
        | TypedExprHir::Le(l, r)
        | TypedExprHir::Ge(l, r)
        | TypedExprHir::Add(l, r)
        | TypedExprHir::Sub(l, r)
        | TypedExprHir::Mul(l, r)
        | TypedExprHir::Div(l, r)
        | TypedExprHir::Mod(l, r) => {
            collect_expr(alloc, l);
            collect_expr(alloc, r);
        }
        TypedExprHir::Not(e) | TypedExprHir::Neg(e) => collect_expr(alloc, e),
        TypedExprHir::Cast(e, _) => collect_expr(alloc, e),
        TypedExprHir::Question { operand, .. } => collect_expr(alloc, operand),
        TypedExprHir::ResultCtor { value, .. } => collect_expr(alloc, value),
        TypedExprHir::Box(inner, _) => collect_expr(alloc, inner),
        TypedExprHir::Deref(inner, _) => collect_expr(alloc, inner),
        TypedExprHir::AssignDeref {
            rhs, binding_id, ..
        } => {
            collect_expr(alloc, rhs);
            alloc.slot_for(*binding_id);
        }
        TypedExprHir::Call(f, args, _) => {
            collect_expr(alloc, f);
            for arg in args {
                collect_expr(alloc, arg);
            }
        }
        TypedExprHir::Index(a, i) => {
            collect_expr(alloc, a);
            collect_expr(alloc, i);
        }
        TypedExprHir::Field(o, _) => collect_expr(alloc, o),
        TypedExprHir::Block(b) => collect_block(alloc, b),
        TypedExprHir::Int(..)
        | TypedExprHir::Float(..)
        | TypedExprHir::Bool(..)
        | TypedExprHir::Str(..)
        | TypedExprHir::Ident { .. } => {}
    }
}
