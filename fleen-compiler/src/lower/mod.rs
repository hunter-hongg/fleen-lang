//! Lower stage: TypedHir → MIR.
//!
//! Control flow (`if`/`elif`/`else`, `while`, `choose`, `and`/`or`) is
//! flattened into basic blocks connected by conditional jumps and jumps.
//! Locals live in numbered slots (no SSA); bindings are looked up via
//! `BindingId`. Shadowing is resolved at compile time into distinct slots.

pub mod cfg;
pub mod mir;
pub mod slots;

#[cfg(test)]
mod tests;

use self::cfg::FnBuilder;
use self::mir::*;
use self::slots::{SlotAlloc, allocate_slots};
use crate::lexer::Span;
use crate::parser::ast::ResultCtor;
use crate::resolver::hir::BindingId;
use crate::typeck::typed_hir::Access;
use crate::typeck::typed_hir::*;
use std::collections::HashMap;

/// Lower a Typed HIR program into MIR.
///
/// # Arguments
/// - `typed_hir`: Type-checked HIR (output of `typeck`).
///
/// # Returns
/// The MIR module: functions (incl. builtin placeholders) + globals.
///
/// # Errors
/// Errors with `LowerError` on constructs that typeck already flags as
/// unsupported (`Index`, `Field`, `Box`/`Ref`/`Array` types), which
/// cannot be lowered in 0.0.1, or on defensively-checked invariant
/// violations.
pub fn lower(typed_hir: TypedHir) -> Result<Mir, LowerError> {
    let mut lcx = LowerCtx::new(&typed_hir);
    lcx.lower_globals(&typed_hir.items)?;
    lcx.lower_funcs(&typed_hir.items)?;
    Ok(Mir {
        funcs: lcx.funcs,
        globals: lcx.globals,
    })
}

/// An error during lowering.
#[derive(Debug, Clone, PartialEq)]
pub struct LowerError {
    pub kind: LowerErrorKind,
    pub span: crate::lexer::Span,
}

/// Kinds of lowering errors.
#[derive(Debug, Clone, PartialEq)]
pub enum LowerErrorKind {
    /// A feature that the parser/typeck accept but the current lower stage
    /// cannot lower yet (e.g. pending U06 bytecode support).
    UnsupportedFeature { feature: &'static str },
    /// A global initializer with control flow (if/while/choose/block).
    ComplexGlobalInit,
    /// An indirect (function-value) call in a global initializer.
    IndirectCallInGlobalInit,
    /// An identifier resolved to no known slot/global/function.
    UndeclaredBinding { name: String },
    /// Negation or arithmetic on an incompatible type (typeck invariant broken).
    InvalidOperand { op: &'static str },
    /// 0.0.2 U06: a `ref` argument passed to a global initializer call.
    /// Global init has no frame to host a temp slot for global ref args.
    RefArgInGlobalInit { name: String },
    /// 0.0.2 U06: a `choose` on a Result scrutinee must have both
    /// `when Ok(..)` and `when Err(..)` arms (no catchall/wildcard).
    ChooseResultNeedsOkErrArms,
    /// 0.0.2 U06: defensive: typeck rejects deref-assign of global boxes.
    /// This ensures lower stays total for malformed trees.
    GlobalBoxDerefAssign { name: String },
}

impl LowerError {
    fn new(kind: LowerErrorKind, span: crate::lexer::Span) -> Self {
        LowerError { kind, span }
    }
}

impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            LowerErrorKind::UnsupportedFeature { feature } => {
                write!(f, "unsupported feature: {feature}")
            }
            LowerErrorKind::ComplexGlobalInit => {
                write!(f, "global initializer must be a simple expression")
            }
            LowerErrorKind::IndirectCallInGlobalInit => {
                write!(f, "indirect calls are not supported in global initializers")
            }
            LowerErrorKind::UndeclaredBinding { name } => {
                write!(f, "undeclared binding: {name}")
            }
            LowerErrorKind::InvalidOperand { op } => {
                write!(f, "invalid operand for operator {op}")
            }
            LowerErrorKind::RefArgInGlobalInit { name } => {
                write!(
                    f,
                    "global initializer cannot take a `ref` argument ({name})"
                )
            }
            LowerErrorKind::ChooseResultNeedsOkErrArms => {
                write!(
                    f,
                    "a `choose` on a Result requires both `when Ok(..)` and `when Err(..)` arms"
                )
            }
            LowerErrorKind::GlobalBoxDerefAssign { name } => {
                write!(f, "cannot deref-assign a global box ({name})")
            }
        }
    }
}

impl std::error::Error for LowerError {}

/// Builtin function names known to 0.0.1.
const BUILTINS: &[&str] = &["print"];

/// Where a binding lives at runtime.
enum Location {
    Local(u16),
    Global(GlobalId),
}

struct LowerCtx {
    funcs: Vec<MirFunc>,
    globals: Vec<MirGlobal>,
    /// Function name → FuncId (function names are scope-unique in 0.0.1).
    func_ids: HashMap<String, FuncId>,
    /// BindingId → location for global variables.
    global_of_binding: HashMap<BindingId, GlobalId>,
    /// FuncId → parameter types (for `ref` argument detection in calls).
    /// Builtins have an empty vec (no Ref parameters).
    param_types: HashMap<FuncId, Vec<Type>>,
}

impl LowerCtx {
    fn new(hir: &TypedHir) -> Self {
        let mut ctx = LowerCtx {
            funcs: Vec::new(),
            globals: Vec::new(),
            func_ids: HashMap::new(),
            global_of_binding: HashMap::new(),
            param_types: HashMap::new(),
        };
        // Builtin functions occupy leading FuncIds.
        for &name in BUILTINS {
            let id = FuncId(ctx.funcs.len() as u32);
            ctx.func_ids.insert(name.to_string(), id);
            ctx.param_types.insert(id, Vec::new()); // builtins have no Ref params
            ctx.funcs.push(MirFunc {
                func_id: id,
                name: name.to_string(),
                params: 1,
                locals: 1,
                entry: BlockId(0),
                blocks: vec![MirBlock {
                    id: BlockId(0),
                    instrs: vec![MirInstr::new(MirInstrKind::Unit, Span::new(0, 0))],
                    terminator: Terminator::Return,
                }],
                is_builtin: true,
                ret_type: Type::Unit,
            });
        }
        // Assign FuncIds to user functions in declaration order.
        for item in &hir.items {
            if let TypedHirItem::Decl(TypedDeclHir::Func(f)) = item {
                let id = FuncId(ctx.funcs.len() as u32);
                ctx.func_ids.insert(f.name.clone(), id);
                let param_tys: Vec<Type> = f.params.iter().map(|p| p.ty.clone()).collect();
                ctx.param_types.insert(id, param_tys);
                ctx.funcs.push(MirFunc {
                    func_id: id,
                    name: f.name.clone(),
                    params: f.params.len() as u16,
                    locals: 0, // filled after slot allocation
                    entry: BlockId(0),
                    blocks: Vec::new(),
                    is_builtin: false,
                    ret_type: f.ret_type.clone().unwrap_or(Type::Unit),
                });
            }
        }
        ctx
    }

    fn lower_globals(&mut self, items: &[TypedHirItem]) -> Result<(), LowerError> {
        for item in items {
            let (name, mutable, binding_id, init_expr) = match item {
                TypedHirItem::Decl(TypedDeclHir::Var(v)) => {
                    (v.name.clone(), true, v.binding_id, &v.init)
                }
                TypedHirItem::Decl(TypedDeclHir::Const(c)) => {
                    (c.name.clone(), false, c.binding_id, &c.init)
                }
                _ => continue,
            };
            let gid = GlobalId(self.globals.len() as u32);
            self.global_of_binding.insert(binding_id, gid);
            let mut init = Vec::new();
            lower_global_init(self, init_expr, &mut init)?;
            self.globals.push(MirGlobal {
                global_id: gid,
                name,
                mutable,
                ty: init_expr.ty(),
                init,
            });
        }
        Ok(())
    }

    fn lower_funcs(&mut self, items: &[TypedHirItem]) -> Result<(), LowerError> {
        for item in items {
            let TypedHirItem::Decl(TypedDeclHir::Func(f)) = item else {
                continue;
            };
            let fid = self.func_ids[&f.name].0 as usize;
            let slots = allocate_slots(f);
            let mut fb = FnBuilder::new();
            let mut bcx = FnBodyCx {
                lcx: &mut *self,
                slots,
            };
            match &f.body {
                TypedFuncBodyHir::SingleExpr(e) => bcx.lower_expr_into(&mut fb, e)?,
                TypedFuncBodyHir::Block(b) => bcx.lower_block(&mut fb, b)?,
            }
            fb.end(Terminator::Return);
            let locals = bcx.slots.total();
            self.funcs[fid].locals = locals;
            let (entry, blocks) = fb.finish();
            self.funcs[fid].entry = entry;
            self.funcs[fid].blocks = blocks;
        }
        Ok(())
    }
}

/// Lower a global initializer to a flat instruction list.
///
/// Control-flow-bearing initializers are rejected: 0.0.1 requires global
/// initializers to be "simple" (literals, arithmetic, calls, identifiers).
fn lower_global_init(
    lcx: &LowerCtx,
    expr: &TypedExprHir,
    out: &mut Vec<MirInstr>,
) -> Result<(), LowerError> {
    match expr {
        TypedExprHir::Int(v, _) => out.push(MirInstr::new(MirInstrKind::ConstInt(*v), expr.span())),
        TypedExprHir::Float(v, _) => {
            out.push(MirInstr::new(MirInstrKind::ConstFloat(*v), expr.span()))
        }
        TypedExprHir::Str(v, _) => out.push(MirInstr::new(
            MirInstrKind::ConstStr(v.clone()),
            expr.span(),
        )),
        TypedExprHir::Bool(true, _) => out.push(MirInstr::new(MirInstrKind::True, expr.span())),
        TypedExprHir::Bool(false, _) => out.push(MirInstr::new(MirInstrKind::False, expr.span())),
        TypedExprHir::Ident {
            name, binding_id, ..
        } => {
            if let Some(&gid) = lcx.global_of_binding.get(binding_id) {
                out.push(MirInstr::new(
                    MirInstrKind::LoadGlobal(gid.0 as u16),
                    expr.span(),
                ));
            } else if let Some(&fid) = lcx.func_ids.get(name) {
                out.push(MirInstr::new(MirInstrKind::LoadFunc(fid), expr.span()));
            } else {
                return Err(LowerError::new(
                    LowerErrorKind::UndeclaredBinding { name: name.clone() },
                    expr.span(),
                ));
            }
        }
        TypedExprHir::Neg(e) => {
            lower_global_init(lcx, e, out)?;
            match e.ty() {
                Type::Int => out.push(MirInstr::new(MirInstrKind::NegI, expr.span())),
                Type::Float => out.push(MirInstr::new(MirInstrKind::NegF, expr.span())),
                _ => {
                    return Err(LowerError::new(
                        LowerErrorKind::InvalidOperand { op: "neg" },
                        e.span(),
                    ));
                }
            }
        }
        TypedExprHir::Not(e) => {
            lower_global_init(lcx, e, out)?;
            out.push(MirInstr::new(MirInstrKind::Not, expr.span()));
        }
        TypedExprHir::Add(l, r)
        | TypedExprHir::Sub(l, r)
        | TypedExprHir::Mul(l, r)
        | TypedExprHir::Div(l, r)
        | TypedExprHir::Mod(l, r) => {
            lower_global_init(lcx, l, out)?;
            lower_global_init(lcx, r, out)?;
            let is_int = matches!(l.ty(), Type::Int);
            let op = match expr {
                TypedExprHir::Add(..) => {
                    if is_int {
                        MirInstrKind::IAdd
                    } else {
                        MirInstrKind::FAdd
                    }
                }
                TypedExprHir::Sub(..) => {
                    if is_int {
                        MirInstrKind::ISub
                    } else {
                        MirInstrKind::FSub
                    }
                }
                TypedExprHir::Mul(..) => {
                    if is_int {
                        MirInstrKind::IMul
                    } else {
                        MirInstrKind::FMul
                    }
                }
                TypedExprHir::Div(..) => {
                    if is_int {
                        MirInstrKind::IDiv
                    } else {
                        MirInstrKind::FDiv
                    }
                }
                _ => {
                    if matches!(l.ty(), Type::Int) && matches!(r.ty(), Type::Int) {
                        MirInstrKind::IMod
                    } else {
                        return Err(LowerError::new(
                            LowerErrorKind::InvalidOperand { op: "mod" },
                            expr.span(),
                        ));
                    }
                }
            };
            out.push(MirInstr::new(op, expr.span()));
        }
        TypedExprHir::Eq(l, r)
        | TypedExprHir::Ne(l, r)
        | TypedExprHir::Lt(l, r)
        | TypedExprHir::Gt(l, r)
        | TypedExprHir::Le(l, r)
        | TypedExprHir::Ge(l, r) => {
            lower_global_init(lcx, l, out)?;
            lower_global_init(lcx, r, out)?;
            out.push(MirInstr::new(
                match expr {
                    TypedExprHir::Eq(..) => MirInstrKind::Eq,
                    TypedExprHir::Ne(..) => MirInstrKind::Ne,
                    TypedExprHir::Lt(..) => MirInstrKind::Lt,
                    TypedExprHir::Gt(..) => MirInstrKind::Gt,
                    TypedExprHir::Le(..) => MirInstrKind::Le,
                    _ => MirInstrKind::Ge,
                },
                expr.span(),
            ));
        }
        TypedExprHir::Call(callee, args, _) => match callee.as_ref() {
            TypedExprHir::Ident { name, .. } if lcx.func_ids.contains_key(name) => {
                // 0.0.2 U06: check for Ref arguments in global init calls.
                let fid = lcx.func_ids[name];
                if let Some(param_tys) = lcx.param_types.get(&fid) {
                    for (i, arg) in args.iter().enumerate() {
                        if param_tys.get(i).is_some_and(|t| matches!(t, Type::Ref(_))) {
                            return Err(LowerError::new(
                                LowerErrorKind::RefArgInGlobalInit { name: name.clone() },
                                arg.span(),
                            ));
                        }
                    }
                }
                for a in args {
                    lower_global_init(lcx, a, out)?;
                }
                out.push(MirInstr::new(MirInstrKind::Call(fid), expr.span()));
            }
            _ => {
                return Err(LowerError::new(
                    LowerErrorKind::IndirectCallInGlobalInit,
                    expr.span(),
                ));
            }
        },
        // 0.0.2 U13: scalar → string in a global initializer.
        TypedExprHir::Cast(inner, _) => {
            lower_global_init(lcx, inner, out)?;
            out.push(MirInstr::new(MirInstrKind::ToStr, expr.span()));
        }
        // 0.0.2 U06: `box e` in a global initializer.
        TypedExprHir::Box(inner, span) => {
            lower_global_init(lcx, inner, out)?;
            out.push(MirInstr::new(MirInstrKind::AllocBox, *span));
        }
        _ => {
            return Err(LowerError::new(
                LowerErrorKind::ComplexGlobalInit,
                expr.span(),
            ));
        }
    }
    Ok(())
}

/// Source span of a `choose` arm: the pattern's span when it has one,
/// otherwise the arm body's (only `TypedPatternHir::Error` carries none,
/// and that only appears in error-recovery trees).
fn choose_arm_span(arm: &TypedChooseArmHir) -> Span {
    match &arm.pattern {
        TypedPatternHir::Literal(lit) => lit.span(),
        TypedPatternHir::Ident { span, .. } | TypedPatternHir::ResultCtor { span, .. } => *span,
        TypedPatternHir::Error => arm.body.span,
    }
}

/// Per-function lowering context.
struct FnBodyCx<'a> {
    lcx: &'a mut LowerCtx,
    slots: SlotAlloc,
}

impl<'a> FnBodyCx<'a> {
    /// Resolve where a binding lives. Function bindings are never dereferenced
    /// directly — they are treated specially (LoadFunc / direct Call).
    fn loc(
        &self,
        binding_id: BindingId,
        name: &str,
        span: crate::lexer::Span,
    ) -> Result<Location, LowerError> {
        if let Some(slot) = self.slots.get(binding_id) {
            Ok(Location::Local(slot))
        } else if let Some(&gid) = self.lcx.global_of_binding.get(&binding_id) {
            Ok(Location::Global(gid))
        } else {
            Err(LowerError::new(
                LowerErrorKind::UndeclaredBinding {
                    name: name.to_string(),
                },
                span,
            ))
        }
    }

    /// A binding is a *function* binding iff it owns neither a local slot
    /// nor a global entry (params/vars/consts/patterns have slots; only
    /// top-level functions live elsewhere).
    fn is_function_binding(&self, binding_id: BindingId) -> bool {
        self.slots.get(binding_id).is_none()
            && !self.lcx.global_of_binding.contains_key(&binding_id)
    }

    /// Allocate a fresh temporary slot for global ref arguments.
    fn fresh_temp(&mut self) -> u16 {
        self.slots.fresh()
    }

    /// Lower a block (statements then tail expression).
    ///
    /// On exit the block's value (tail expr, or `Unit`) is on the stack.
    fn lower_block(&mut self, fb: &mut FnBuilder, block: &TypedBlockHir) -> Result<(), LowerError> {
        let n = block.stmts.len();
        for (i, stmt) in block.stmts.iter().enumerate() {
            match stmt {
                TypedStmtHir::Decl(TypedDeclHir::Var(v)) => {
                    self.slots.slot_for(v.binding_id);
                    self.lower_expr_into(fb, &v.init)?;
                    // Invariant: slot_for above just allocated it.
                    let slot = self.slots.get(v.binding_id).expect("slot just allocated");
                    fb.emit(MirInstr::new(MirInstrKind::StoreLocal(slot), v.span));
                }
                TypedStmtHir::Decl(TypedDeclHir::Const(c)) => {
                    self.slots.slot_for(c.binding_id);
                    self.lower_expr_into(fb, &c.init)?;
                    // Invariant: slot_for above just allocated it.
                    let slot = self.slots.get(c.binding_id).expect("slot just allocated");
                    fb.emit(MirInstr::new(MirInstrKind::StoreLocal(slot), c.span));
                }
                TypedStmtHir::Decl(TypedDeclHir::Func(_)) => {
                    // Functions are hoisted; nothing emitted inline.
                }
                TypedStmtHir::Expr(e, has_semi) => {
                    self.lower_expr_into(fb, e)?;
                    let is_last_and_no_tail = i + 1 == n && block.tail_expr.is_none();
                    if *has_semi || !is_last_and_no_tail {
                        fb.emit(MirInstr::new(MirInstrKind::Pop, e.span()));
                    }
                }
                TypedStmtHir::Error => {}
            }
        }
        match &block.tail_expr {
            Some(tail) => self.lower_expr_into(fb, tail)?,
            None => fb.emit(MirInstr::new(MirInstrKind::Unit, block.span)),
        }
        Ok(())
    }

    /// Lower an expression; its value is left on the stack.
    fn lower_expr_into(
        &mut self,
        fb: &mut FnBuilder,
        expr: &TypedExprHir,
    ) -> Result<(), LowerError> {
        match expr {
            TypedExprHir::Int(v, _) => {
                fb.emit(MirInstr::new(MirInstrKind::ConstInt(*v), expr.span()))
            }
            TypedExprHir::Float(v, _) => {
                fb.emit(MirInstr::new(MirInstrKind::ConstFloat(*v), expr.span()))
            }
            TypedExprHir::Str(v, _) => fb.emit(MirInstr::new(
                MirInstrKind::ConstStr(v.clone()),
                expr.span(),
            )),
            TypedExprHir::Bool(true, _) => fb.emit(MirInstr::new(MirInstrKind::True, expr.span())),
            TypedExprHir::Bool(false, _) => {
                fb.emit(MirInstr::new(MirInstrKind::False, expr.span()))
            }
            TypedExprHir::Ident {
                binding_id,
                name,
                access,
                span,
                ..
            } => {
                if self.is_function_binding(*binding_id) {
                    match self.lcx.func_ids.get(name) {
                        Some(&fid) => fb.emit(MirInstr::new(MirInstrKind::LoadFunc(fid), *span)),
                        None => {
                            return Err(LowerError::new(
                                LowerErrorKind::UndeclaredBinding { name: name.clone() },
                                *span,
                            ));
                        }
                    }
                } else {
                    match self.loc(*binding_id, name, *span)? {
                        Location::Local(slot) => match access {
                            Access::Copy => {
                                fb.emit(MirInstr::new(MirInstrKind::LoadLocal(slot), *span));
                            }
                            Access::Move => {
                                fb.emit(MirInstr::new(MirInstrKind::MoveLocal(slot), *span));
                            }
                            Access::Clone => {
                                fb.emit(MirInstr::new(MirInstrKind::CloneLocal(slot), *span));
                            }
                        },
                        Location::Global(gid) => {
                            match access {
                                Access::Copy => {
                                    fb.emit(MirInstr::new(
                                        MirInstrKind::LoadGlobal(gid.0 as u16),
                                        *span,
                                    ));
                                }
                                Access::Clone => {
                                    fb.emit(MirInstr::new(
                                        MirInstrKind::CloneGlobal(gid.0 as u16),
                                        *span,
                                    ));
                                }
                                Access::Move => {
                                    // Defensive: typeck rejects moving owned globals.
                                    return Err(LowerError::new(
                                        LowerErrorKind::UnsupportedFeature {
                                            feature: "move out of owned global",
                                        },
                                        *span,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            TypedExprHir::Assign {
                binding_id,
                rhs,
                span,
                ..
            } => {
                self.lower_expr_into(fb, rhs)?;
                // Dup/DupDeep for owned types (B2 decision).
                let dup_kind = if rhs.ty().is_owned() {
                    MirInstrKind::DupDeep
                } else {
                    MirInstrKind::Dup
                };
                fb.emit(MirInstr::new(dup_kind, *span));
                match self.loc(*binding_id, "?", *span)? {
                    Location::Local(slot) => {
                        fb.emit(MirInstr::new(MirInstrKind::StoreLocal(slot), *span))
                    }
                    Location::Global(gid) => fb.emit(MirInstr::new(
                        MirInstrKind::StoreGlobal(gid.0 as u16),
                        *span,
                    )),
                }
            }
            TypedExprHir::Not(e) => {
                self.lower_expr_into(fb, e)?;
                fb.emit(MirInstr::new(MirInstrKind::Not, expr.span()));
            }
            TypedExprHir::Neg(e) => {
                self.lower_expr_into(fb, e)?;
                match e.ty() {
                    Type::Int => fb.emit(MirInstr::new(MirInstrKind::NegI, expr.span())),
                    Type::Float => fb.emit(MirInstr::new(MirInstrKind::NegF, expr.span())),
                    _ => {
                        return Err(LowerError::new(
                            LowerErrorKind::InvalidOperand { op: "neg" },
                            e.span(),
                        ));
                    }
                }
            }
            TypedExprHir::Add(l, r) => self.binop(fb, l, r, |int| {
                if int {
                    MirInstrKind::IAdd
                } else {
                    MirInstrKind::FAdd
                }
            })?,
            TypedExprHir::Sub(l, r) => self.binop(fb, l, r, |int| {
                if int {
                    MirInstrKind::ISub
                } else {
                    MirInstrKind::FSub
                }
            })?,
            TypedExprHir::Mul(l, r) => self.binop(fb, l, r, |int| {
                if int {
                    MirInstrKind::IMul
                } else {
                    MirInstrKind::FMul
                }
            })?,
            TypedExprHir::Div(l, r) => self.binop(fb, l, r, |int| {
                if int {
                    MirInstrKind::IDiv
                } else {
                    MirInstrKind::FDiv
                }
            })?,
            TypedExprHir::Mod(l, r) => {
                self.lower_expr_into(fb, l)?;
                self.lower_expr_into(fb, r)?;
                if matches!(l.ty(), Type::Int) && matches!(r.ty(), Type::Int) {
                    fb.emit(MirInstr::new(MirInstrKind::IMod, expr.span()));
                } else {
                    return Err(LowerError::new(
                        LowerErrorKind::InvalidOperand { op: "mod" },
                        expr.span(),
                    ));
                }
            }
            TypedExprHir::Eq(l, r)
            | TypedExprHir::Ne(l, r)
            | TypedExprHir::Lt(l, r)
            | TypedExprHir::Gt(l, r)
            | TypedExprHir::Le(l, r)
            | TypedExprHir::Ge(l, r) => {
                self.lower_expr_into(fb, l)?;
                self.lower_expr_into(fb, r)?;
                fb.emit(MirInstr::new(
                    match expr {
                        TypedExprHir::Eq(..) => MirInstrKind::Eq,
                        TypedExprHir::Ne(..) => MirInstrKind::Ne,
                        TypedExprHir::Lt(..) => MirInstrKind::Lt,
                        TypedExprHir::Gt(..) => MirInstrKind::Gt,
                        TypedExprHir::Le(..) => MirInstrKind::Le,
                        _ => MirInstrKind::Ge,
                    },
                    expr.span(),
                ));
            }
            TypedExprHir::And(l, r) => {
                // c0: l; JumpIfFalse F   c1: r; JumpIfFalse F   t: True   F: False
                // Layout: c0, c1, t, F, M
                self.lower_expr_into(fb, l)?;
                let c0 = fb.jump_if_false_later();
                let c1_b = fb.new_block();
                fb.start(c1_b);
                self.lower_expr_into(fb, r)?;
                let c1 = fb.jump_if_false_later();
                let t = fb.new_block();
                fb.start(t);
                fb.emit(MirInstr::new(MirInstrKind::True, expr.span()));
                let jt = fb.jump_later();
                let f_b = fb.new_block();
                fb.start(f_b);
                fb.emit(MirInstr::new(MirInstrKind::False, expr.span()));
                let jf = fb.jump_later();
                let m = fb.new_block();
                fb.start(m);
                fb.resolve(c0, f_b);
                fb.resolve(c1, f_b);
                fb.resolve(jt, m);
                fb.resolve(jf, m);
            }
            TypedExprHir::Or(l, r) => {
                // c0: l; JumpIfTrue T   c1: r; JumpIfTrue T   F: False   T: True
                // Layout: c0, c1, F, T, M
                self.lower_expr_into(fb, l)?;
                let c0 = fb.jump_if_true_later();
                let c1_b = fb.new_block();
                fb.start(c1_b);
                self.lower_expr_into(fb, r)?;
                let c1 = fb.jump_if_true_later();
                let f_b = fb.new_block();
                fb.start(f_b);
                fb.emit(MirInstr::new(MirInstrKind::False, expr.span()));
                let jf = fb.jump_later();
                let t = fb.new_block();
                fb.start(t);
                fb.emit(MirInstr::new(MirInstrKind::True, expr.span()));
                let jt = fb.jump_later();
                let m = fb.new_block();
                fb.start(m);
                fb.resolve(c0, t);
                fb.resolve(c1, t);
                fb.resolve(jf, m);
                fb.resolve(jt, m);
            }
            TypedExprHir::If(e) => self.lower_if(fb, e)?,
            TypedExprHir::While(e) => {
                // 独立头块：回边必须只重入"重新求条件"，而不是当前合并块。
                // 否则首次进入（栈上有上文的汇合值）与回边进入（循环携带深度）
                // 栈深不一致，verify 会（理应）报 StackDepthMismatch。
                let head = fb.new_block();
                fb.jump(head);
                fb.start(head);
                self.lower_expr_into(fb, &e.condition)?;
                let c = fb.jump_if_false_later();
                let body = fb.new_block();
                fb.start(body);
                self.lower_block(fb, &e.body)?;
                fb.emit(MirInstr::new(MirInstrKind::Pop, e.span));
                fb.jump(head);
                let end = fb.new_block();
                fb.start(end);
                fb.emit(MirInstr::new(MirInstrKind::Unit, e.span));
                fb.resolve(c, end);
            }
            TypedExprHir::Choose(e) => self.lower_choose(fb, e)?,
            TypedExprHir::Call(callee, args, _) => {
                self.lower_call(fb, callee, args, expr.span())?;
            }
            TypedExprHir::Block(b) => self.lower_block(fb, b)?,
            TypedExprHir::Cast(inner, _) => {
                self.lower_expr_into(fb, inner)?;
                fb.emit(MirInstr::new(MirInstrKind::ToStr, expr.span()));
            }
            // 0.0.2 U06: `?` operator (Result propagation).
            TypedExprHir::Question { operand, span, .. } => {
                self.lower_question(fb, operand, *span)?;
            }
            // 0.0.2 U06: Result constructors.
            TypedExprHir::ResultCtor {
                ctor, value, span, ..
            } => {
                self.lower_expr_into(fb, value)?;
                let kind = match ctor {
                    ResultCtor::Ok => MirInstrKind::PackOk,
                    ResultCtor::Err => MirInstrKind::PackErr,
                };
                fb.emit(MirInstr::new(kind, *span));
            }
            // 0.0.2 U06: box expression.
            TypedExprHir::Box(inner, span) => {
                self.lower_expr_into(fb, inner)?;
                fb.emit(MirInstr::new(MirInstrKind::AllocBox, *span));
            }
            // 0.0.2 U06: deref expression (read).
            TypedExprHir::Deref(inner, span) => {
                self.lower_expr_into(fb, inner)?;
                fb.emit(MirInstr::new(MirInstrKind::DerefBox, *span));
                // `DerefBox` is `b → b v` (BYTECODE.md §5.9): the box stays on
                // the stack and the pointee copy is pushed on top, so a bare
                // read leaves TWO values where an expression must leave one.
                // Park the copy in a temp slot, drop the box, then reload the
                // copy. This keeps `DerefBox`'s frozen stack effect (Δ+1) and
                // still yields exactly one value.
                let tmp = self.fresh_temp();
                fb.emit(MirInstr::new(MirInstrKind::StoreLocal(tmp), *span));
                fb.emit(MirInstr::new(MirInstrKind::Pop, *span));
                fb.emit(MirInstr::new(MirInstrKind::LoadLocal(tmp), *span));
            }
            // 0.0.2 U06: deref assignment.
            TypedExprHir::AssignDeref {
                binding_id,
                rhs,
                span,
                ..
            } => {
                let box_loc = self.loc(*binding_id, "?", *span)?;
                match box_loc {
                    Location::Local(slot) => {
                        // [b]; [v]; StoreDerefBox; Unit — one value on the stack.
                        fb.emit(MirInstr::new(MirInstrKind::LoadLocal(slot), *span));
                        self.lower_expr_into(fb, rhs)?;
                        fb.emit(MirInstr::new(MirInstrKind::StoreDerefBox, *span));
                        fb.emit(MirInstr::new(MirInstrKind::Unit, *span));
                    }
                    Location::Global(gid) => {
                        return Err(LowerError::new(
                            LowerErrorKind::GlobalBoxDerefAssign {
                                name: format!("global#{}", gid.0),
                            },
                            *span,
                        ));
                    }
                }
            }
            TypedExprHir::Index(..) => {
                return Err(LowerError::new(
                    LowerErrorKind::UnsupportedFeature { feature: "index" },
                    expr.span(),
                ));
            }
            TypedExprHir::Field(..) => {
                return Err(LowerError::new(
                    LowerErrorKind::UnsupportedFeature { feature: "field" },
                    expr.span(),
                ));
            }
        }
        Ok(())
    }

    /// Lower `l op r`; `pick(is_int)` selects the instruction.
    fn binop(
        &mut self,
        fb: &mut FnBuilder,
        l: &TypedExprHir,
        r: &TypedExprHir,
        pick: impl Fn(bool) -> MirInstrKind,
    ) -> Result<(), LowerError> {
        self.lower_expr_into(fb, l)?;
        self.lower_expr_into(fb, r)?;
        fb.emit(MirInstr::new(pick(matches!(l.ty(), Type::Int)), l.span()));
        Ok(())
    }

    /// Lower a function call with ref argument handling.
    fn lower_call(
        &mut self,
        fb: &mut FnBuilder,
        callee: &TypedExprHir,
        args: &[TypedExprHir],
        span: Span,
    ) -> Result<(), LowerError> {
        match callee {
            TypedExprHir::Ident {
                name, binding_id, ..
            } if self.is_function_binding(*binding_id) => {
                let fid = self.lcx.func_ids[name];
                let param_tys = self.lcx.param_types.get(&fid).cloned().unwrap_or_default();
                self.prepare_args(fb, args, &param_tys)?;
                fb.emit(MirInstr::new(MirInstrKind::Call(fid), span));
            }
            _ => {
                // Indirect call: get param types from callee's function type.
                self.lower_expr_into(fb, callee)?;
                let param_tys = match callee.ty() {
                    Type::Func(params, _) => params,
                    _ => vec![],
                };
                self.prepare_args(fb, args, &param_tys)?;
                fb.emit(MirInstr::new(
                    MirInstrKind::CallValue(args.len() as u8),
                    span,
                ));
            }
        }
        Ok(())
    }

    /// Prepare call arguments, handling ref parameter places.
    fn prepare_args(
        &mut self,
        fb: &mut FnBuilder,
        args: &[TypedExprHir],
        param_tys: &[Type],
    ) -> Result<(), LowerError> {
        for (i, arg) in args.iter().enumerate() {
            let param_ty = param_tys.get(i);
            if param_ty.is_some_and(|t| matches!(t, Type::Ref(_))) {
                // Ref parameter: argument must be a bare Ident place (typeck guarantee).
                match arg {
                    TypedExprHir::Ident {
                        binding_id,
                        name,
                        span,
                        ..
                    } => {
                        match self.loc(*binding_id, name, *span)? {
                            Location::Local(slot) => {
                                fb.emit(MirInstr::new(MirInstrKind::MakeRefLocal(slot), *span));
                            }
                            Location::Global(gid) => {
                                // Global ref arg: copy to temp slot, then borrow the temp.
                                let tmp = self.fresh_temp();
                                fb.emit(MirInstr::new(
                                    MirInstrKind::CloneGlobal(gid.0 as u16),
                                    *span,
                                ));
                                fb.emit(MirInstr::new(MirInstrKind::StoreLocal(tmp), *span));
                                fb.emit(MirInstr::new(MirInstrKind::MakeRefLocal(tmp), *span));
                            }
                        }
                    }
                    _ => {
                        // Defensive: typeck should reject non-place ref args.
                        return Err(LowerError::new(
                            LowerErrorKind::UnsupportedFeature {
                                feature: "ref argument must be a variable (typeck should reject)",
                            },
                            arg.span(),
                        ));
                    }
                }
            } else {
                // Normal argument: lower and push.
                self.lower_expr_into(fb, arg)?;
            }
        }
        Ok(())
    }

    /// Lower `?` operator (Result propagation).
    ///
    /// Layout (BYTECODE.md §6.7):
    /// ```text
    /// <eval operand> Dup/DupDeep  IsErr  JumpIfTrue L_err
    /// L_ok: UnwrapOk  Jump L_cont
    /// L_err: UnwrapErr  PackErr  Return
    /// L_cont: ...
    /// ```
    /// The ok-path is a distinct block from the operand block so the
    /// unconditional `Jump L_cont` does not overwrite the conditional
    /// `JumpIfTrue` terminator.
    fn lower_question(
        &mut self,
        fb: &mut FnBuilder,
        operand: &TypedExprHir,
        span: Span,
    ) -> Result<(), LowerError> {
        self.lower_expr_into(fb, operand)?;
        // Dup/DupDeep based on operand type (owned → DupDeep).
        let dup_kind = if operand.ty().is_owned() {
            MirInstrKind::DupDeep
        } else {
            MirInstrKind::Dup
        };
        fb.emit(MirInstr::new(dup_kind, span));
        fb.emit(MirInstr::new(MirInstrKind::IsErr, span));
        let c = fb.jump_if_true_later();
        // Ok path: unwrap and fall through to the continuation.
        let ok = fb.new_block();
        fb.start(ok);
        fb.emit(MirInstr::new(MirInstrKind::UnwrapOk, span));
        let j_cont = fb.jump_later();
        // Err path: re-pack the error (must nest it, not return it bare) and
        // return from the enclosing function.
        let l_err = fb.new_block();
        fb.start(l_err);
        fb.emit(MirInstr::new(MirInstrKind::UnwrapErr, span));
        fb.emit(MirInstr::new(MirInstrKind::PackErr, span));
        fb.end(Terminator::Return);
        let l_cont = fb.new_block();
        fb.start(l_cont);
        fb.resolve(c, l_err);
        fb.resolve(j_cont, l_cont);
        Ok(())
    }

    fn lower_if(&mut self, fb: &mut FnBuilder, e: &TypedExprIfHir) -> Result<(), LowerError> {
        // Layout: c0; then0; c1; then1; …; else-or-unit; M
        let mut cond_blocks: Vec<BlockId> = Vec::new();
        let mut branch_jumps: Vec<BlockId> = Vec::new();

        self.lower_expr_into(fb, &e.condition)?;
        cond_blocks.push(fb.jump_if_false_later());

        let t0 = fb.new_block();
        fb.start(t0);
        self.lower_block(fb, &e.then_branch)?;
        branch_jumps.push(fb.jump_later());

        for (cond, body) in &e.elif_branches {
            let ci = fb.new_block();
            fb.start(ci);
            self.lower_expr_into(fb, cond)?;
            cond_blocks.push(fb.jump_if_false_later());
            let ti = fb.new_block();
            fb.start(ti);
            self.lower_block(fb, body)?;
            branch_jumps.push(fb.jump_later());
        }

        let eb = fb.new_block();
        fb.start(eb);
        match &e.else_branch {
            Some(b) => self.lower_block(fb, b)?,
            None => fb.emit(MirInstr::new(MirInstrKind::Unit, e.span)),
        }
        branch_jumps.push(fb.jump_later());

        let m = fb.new_block();
        fb.start(m);

        // Patch cond false-targets: c_i → next cond, last → else block.
        let mut targets: Vec<BlockId> = cond_blocks.iter().skip(1).copied().collect();
        targets.push(eb);
        for (c, t) in cond_blocks.iter().zip(targets) {
            fb.resolve(*c, t);
        }
        for j in branch_jumps {
            fb.resolve(j, m);
        }
        Ok(())
    }

    fn lower_choose(
        &mut self,
        fb: &mut FnBuilder,
        e: &TypedExprChooseHir,
    ) -> Result<(), LowerError> {
        // Special case: Result scrutinee (0.0.2 U06).
        if matches!(e.scrutinee.ty(), Type::Result(_, _)) {
            return self.lower_choose_result(fb, e);
        }

        // Generic choose (0.0.1 algorithm, updated with MirInstr struct).
        self.lower_expr_into(fb, &e.scrutinee)?;

        let mut body_jumps: Vec<BlockId> = Vec::new();
        let mut pending: Vec<BlockId> = Vec::new();
        let mut chain_done = false;

        for arm in &e.arms {
            if chain_done {
                break;
            }
            match &arm.pattern {
                TypedPatternHir::Literal(lit) => {
                    fb.emit(MirInstr::new(MirInstrKind::Dup, e.scrutinee.span()));
                    self.lower_expr_into(fb, lit)?;
                    fb.emit(MirInstr::new(MirInstrKind::Eq, e.scrutinee.span()));
                    pending.push(fb.jump_if_false_later());
                    if let Some(guard) = &arm.guard {
                        let g = fb.new_block();
                        fb.start(g);
                        self.lower_expr_into(fb, guard)?;
                        pending.push(fb.jump_if_false_later());
                    }
                    let ok = fb.new_block();
                    fb.start(ok);
                    fb.emit(MirInstr::new(MirInstrKind::Pop, e.scrutinee.span()));
                    self.lower_block(fb, &arm.body)?;
                    body_jumps.push(fb.jump_later());
                }
                TypedPatternHir::Ident { binding_id, .. } => {
                    self.slots.slot_for(*binding_id);
                    let slot = self.slots.get(*binding_id).expect("slot just allocated");
                    fb.emit(MirInstr::new(
                        MirInstrKind::BindMatch(slot),
                        e.scrutinee.span(),
                    ));
                    match &arm.guard {
                        None => {
                            fb.emit(MirInstr::new(MirInstrKind::Pop, e.scrutinee.span()));
                            self.lower_block(fb, &arm.body)?;
                            body_jumps.push(fb.jump_later());
                            chain_done = true;
                        }
                        Some(guard) => {
                            self.lower_expr_into(fb, guard)?;
                            pending.push(fb.jump_if_false_later());
                            let ok = fb.new_block();
                            fb.start(ok);
                            fb.emit(MirInstr::new(MirInstrKind::Pop, e.scrutinee.span()));
                            self.lower_block(fb, &arm.body)?;
                            body_jumps.push(fb.jump_later());
                        }
                    }
                }
                TypedPatternHir::ResultCtor { span, .. } => {
                    return Err(LowerError::new(
                        LowerErrorKind::ChooseResultNeedsOkErrArms,
                        *span,
                    ));
                }
                TypedPatternHir::Error => {}
            }

            if !chain_done {
                let this = std::mem::take(&mut pending);
                let next = fb.new_block();
                fb.start(next);
                for cond_block in this {
                    fb.resolve(cond_block, next);
                }
            }
        }

        if !chain_done {
            fb.emit(MirInstr::new(MirInstrKind::Pop, e.scrutinee.span()));
            fb.emit(MirInstr::new(MirInstrKind::Unit, e.scrutinee.span()));
            body_jumps.push(fb.jump_later());
        }

        let m = fb.new_block();
        fb.start(m);
        for j in body_jumps {
            fb.resolve(j, m);
        }
        Ok(())
    }

    /// Lower a `choose` on a Result scrutinee (0.0.2 U06).
    ///
    /// Layout follows BYTECODE.md §6.8.
    /// Only `when Ok(..)` and `when Err(..)` arms are supported: the lowering is
    /// the `IsErr`-then-branch shape with no fallback block, so `otherwise`
    /// (parsed as a wildcard `Ident` pattern) or any literal pattern cannot be
    /// represented and is rejected up front rather than silently dropped.
    /// Guards on Ok/Err arms are deferred to U07/U09.
    fn lower_choose_result(
        &mut self,
        fb: &mut FnBuilder,
        e: &TypedExprChooseHir,
    ) -> Result<(), LowerError> {
        // Reject arms the branch shape cannot express. typeck lets `otherwise`
        // through (it short-circuits the exhaustivity check), so without this
        // guard the arm's body would be dropped on the floor.
        for arm in &e.arms {
            if !matches!(&arm.pattern, TypedPatternHir::ResultCtor { .. }) {
                return Err(LowerError::new(
                    LowerErrorKind::UnsupportedFeature {
                        feature: "non-`Ok`/`Err` arm in a Result `choose` (deferred to U07)",
                    },
                    choose_arm_span(arm),
                ));
            }
        }

        self.lower_expr_into(fb, &e.scrutinee)?;
        // Dup/DupDeep based on scrutinee type (owned → DupDeep).
        let dup_kind = if e.scrutinee.ty().is_owned() {
            MirInstrKind::DupDeep
        } else {
            MirInstrKind::Dup
        };
        fb.emit(MirInstr::new(dup_kind, e.scrutinee.span()));
        fb.emit(MirInstr::new(MirInstrKind::IsErr, e.scrutinee.span()));

        // Branch: Err side (all Err arms in source order).
        let jump_err = fb.jump_if_true_later();

        // Ok side: walk Ok arms in source order (no guards in U06).
        let mut ok_jumps: Vec<BlockId> = Vec::new();
        let mut has_ok_arm = false;

        for arm in &e.arms {
            if let TypedPatternHir::ResultCtor {
                ctor: ResultCtor::Ok,
                binding_id,
                span,
                ..
            } = &arm.pattern
            {
                has_ok_arm = true;
                if arm.guard.is_some() {
                    return Err(LowerError::new(
                        LowerErrorKind::UnsupportedFeature {
                            feature: "Result choose with guards (deferred to U07)",
                        },
                        *span,
                    ));
                }
                self.slots.slot_for(*binding_id);
                let slot = self.slots.get(*binding_id).expect("slot just allocated");
                let arm_ok = fb.new_block();
                fb.start(arm_ok);
                fb.emit(MirInstr::new(MirInstrKind::UnwrapOk, *span));
                fb.emit(MirInstr::new(MirInstrKind::StoreLocal(slot), *span));
                self.lower_block(fb, &arm.body)?;
                ok_jumps.push(fb.jump_later());
            }
        }

        // Err side: walk Err arms in source order (no guards in U06).
        // The IsErr `jump_err` is patched to the *first* Err arm block once we
        // know it, so there is no empty forwarder block (which would otherwise
        // hit a placeholder `Return`).
        let mut err_jumps: Vec<BlockId> = Vec::new();
        let mut first_err: Option<BlockId> = None;
        let mut has_err_arm = false;

        for arm in &e.arms {
            if let TypedPatternHir::ResultCtor {
                ctor: ResultCtor::Err,
                binding_id,
                span,
                ..
            } = &arm.pattern
            {
                has_err_arm = true;
                if arm.guard.is_some() {
                    return Err(LowerError::new(
                        LowerErrorKind::UnsupportedFeature {
                            feature: "Result choose with guards (deferred to U07)",
                        },
                        *span,
                    ));
                }
                self.slots.slot_for(*binding_id);
                let slot = self.slots.get(*binding_id).expect("slot just allocated");
                let arm_err = fb.new_block();
                if first_err.is_none() {
                    first_err = Some(arm_err);
                }
                fb.start(arm_err);
                fb.emit(MirInstr::new(MirInstrKind::UnwrapErr, *span));
                fb.emit(MirInstr::new(MirInstrKind::StoreLocal(slot), *span));
                self.lower_block(fb, &arm.body)?;
                err_jumps.push(fb.jump_later());
            }
        }

        // Defensive: typeck guarantees an unguarded Ok and an unguarded Err arm.
        if !has_ok_arm || !has_err_arm {
            return Err(LowerError::new(
                LowerErrorKind::ChooseResultNeedsOkErrArms,
                e.span,
            ));
        }

        // Merge Ok and Err branches.
        let merge = fb.new_block();
        fb.start(merge);
        for j in ok_jumps {
            fb.resolve(j, merge);
        }
        for j in err_jumps {
            fb.resolve(j, merge);
        }
        // The IsErr (taken) branch targets the first Err arm. The ok
        // fall-through lands on the first Ok arm (emitted right after the
        // scrutinee block).
        fb.resolve(jump_err, first_err.expect("has Err arm (checked above)"));

        Ok(())
    }
}
