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
use crate::resolver::hir::BindingId;
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
}

impl LowerCtx {
    fn new(hir: &TypedHir) -> Self {
        let mut ctx = LowerCtx {
            funcs: Vec::new(),
            globals: Vec::new(),
            func_ids: HashMap::new(),
            global_of_binding: HashMap::new(),
        };
        // Builtin functions occupy leading FuncIds.
        for &name in BUILTINS {
            let id = FuncId(ctx.funcs.len() as u32);
            ctx.func_ids.insert(name.to_string(), id);
            ctx.funcs.push(MirFunc {
                func_id: id,
                name: name.to_string(),
                params: 1,
                locals: 1,
                entry: BlockId(0),
                blocks: vec![MirBlock {
                    id: BlockId(0),
                    instrs: vec![MirInstr::Unit],
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
        TypedExprHir::Int(v, _) => out.push(MirInstr::ConstInt(*v)),
        TypedExprHir::Float(v, _) => out.push(MirInstr::ConstFloat(*v)),
        TypedExprHir::Str(v, _) => out.push(MirInstr::ConstStr(v.clone())),
        TypedExprHir::Bool(true, _) => out.push(MirInstr::True),
        TypedExprHir::Bool(false, _) => out.push(MirInstr::False),
        TypedExprHir::Ident {
            name, binding_id, ..
        } => {
            if let Some(&gid) = lcx.global_of_binding.get(binding_id) {
                out.push(MirInstr::LoadGlobal(gid.0 as u16));
            } else if let Some(&fid) = lcx.func_ids.get(name) {
                out.push(MirInstr::LoadFunc(fid));
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
                Type::Int => out.push(MirInstr::NegI),
                Type::Float => out.push(MirInstr::NegF),
                _ => {
                    return Err(LowerError::new(
                        LowerErrorKind::InvalidOperand { op: "neg" },
                        expr.span(),
                    ));
                }
            }
        }
        TypedExprHir::Not(e) => {
            lower_global_init(lcx, e, out)?;
            out.push(MirInstr::Not);
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
                        MirInstr::IAdd
                    } else {
                        MirInstr::FAdd
                    }
                }
                TypedExprHir::Sub(..) => {
                    if is_int {
                        MirInstr::ISub
                    } else {
                        MirInstr::FSub
                    }
                }
                TypedExprHir::Mul(..) => {
                    if is_int {
                        MirInstr::IMul
                    } else {
                        MirInstr::FMul
                    }
                }
                TypedExprHir::Div(..) => {
                    if is_int {
                        MirInstr::IDiv
                    } else {
                        MirInstr::FDiv
                    }
                }
                _ => {
                    if matches!(l.ty(), Type::Int) && matches!(r.ty(), Type::Int) {
                        MirInstr::IMod
                    } else {
                        return Err(LowerError::new(
                            LowerErrorKind::InvalidOperand { op: "mod" },
                            expr.span(),
                        ));
                    }
                }
            };
            out.push(op);
        }
        TypedExprHir::Eq(l, r)
        | TypedExprHir::Ne(l, r)
        | TypedExprHir::Lt(l, r)
        | TypedExprHir::Gt(l, r)
        | TypedExprHir::Le(l, r)
        | TypedExprHir::Ge(l, r) => {
            lower_global_init(lcx, l, out)?;
            lower_global_init(lcx, r, out)?;
            out.push(match expr {
                TypedExprHir::Eq(..) => MirInstr::Eq,
                TypedExprHir::Ne(..) => MirInstr::Ne,
                TypedExprHir::Lt(..) => MirInstr::Lt,
                TypedExprHir::Gt(..) => MirInstr::Gt,
                TypedExprHir::Le(..) => MirInstr::Le,
                _ => MirInstr::Ge,
            });
        }
        TypedExprHir::Call(callee, args, _) => match callee.as_ref() {
            TypedExprHir::Ident { name, .. } if lcx.func_ids.contains_key(name) => {
                for a in args {
                    lower_global_init(lcx, a, out)?;
                }
                out.push(MirInstr::Call(lcx.func_ids[name]));
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
            out.push(MirInstr::ToStr);
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
                    fb.emit(MirInstr::StoreLocal(slot));
                }
                TypedStmtHir::Decl(TypedDeclHir::Const(c)) => {
                    self.slots.slot_for(c.binding_id);
                    self.lower_expr_into(fb, &c.init)?;
                    // Invariant: slot_for above just allocated it.
                    let slot = self.slots.get(c.binding_id).expect("slot just allocated");
                    fb.emit(MirInstr::StoreLocal(slot));
                }
                TypedStmtHir::Decl(TypedDeclHir::Func(_)) => {
                    // Functions are hoisted; nothing emitted inline.
                }
                TypedStmtHir::Expr(e, has_semi) => {
                    self.lower_expr_into(fb, e)?;
                    let is_last_and_no_tail = i + 1 == n && block.tail_expr.is_none();
                    if *has_semi || !is_last_and_no_tail {
                        fb.emit(MirInstr::Pop);
                    }
                }
                TypedStmtHir::Error => {}
            }
        }
        match &block.tail_expr {
            Some(tail) => self.lower_expr_into(fb, tail)?,
            None => fb.emit(MirInstr::Unit),
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
            TypedExprHir::Int(v, _) => fb.emit(MirInstr::ConstInt(*v)),
            TypedExprHir::Float(v, _) => fb.emit(MirInstr::ConstFloat(*v)),
            TypedExprHir::Str(v, _) => fb.emit(MirInstr::ConstStr(v.clone())),
            TypedExprHir::Bool(true, _) => fb.emit(MirInstr::True),
            TypedExprHir::Bool(false, _) => fb.emit(MirInstr::False),
            TypedExprHir::Ident {
                binding_id,
                name,
                span,
                ..
            } => {
                if self.is_function_binding(*binding_id) {
                    match self.lcx.func_ids.get(name) {
                        Some(&fid) => fb.emit(MirInstr::LoadFunc(fid)),
                        None => {
                            return Err(LowerError::new(
                                LowerErrorKind::UndeclaredBinding { name: name.clone() },
                                *span,
                            ));
                        }
                    }
                } else {
                    match self.loc(*binding_id, name, *span)? {
                        Location::Local(slot) => fb.emit(MirInstr::LoadLocal(slot)),
                        Location::Global(gid) => fb.emit(MirInstr::LoadGlobal(gid.0 as u16)),
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
                fb.emit(MirInstr::Dup);
                match self.loc(*binding_id, "?", *span)? {
                    Location::Local(slot) => fb.emit(MirInstr::StoreLocal(slot)),
                    Location::Global(gid) => fb.emit(MirInstr::StoreGlobal(gid.0 as u16)),
                }
            }
            TypedExprHir::Not(e) => {
                self.lower_expr_into(fb, e)?;
                fb.emit(MirInstr::Not);
            }
            TypedExprHir::Neg(e) => {
                self.lower_expr_into(fb, e)?;
                match e.ty() {
                    Type::Int => fb.emit(MirInstr::NegI),
                    Type::Float => fb.emit(MirInstr::NegF),
                    _ => {
                        return Err(LowerError::new(
                            LowerErrorKind::InvalidOperand { op: "neg" },
                            e.span(),
                        ));
                    }
                }
            }
            TypedExprHir::Add(l, r) => {
                self.binop(
                    fb,
                    l,
                    r,
                    |int| {
                        if int { MirInstr::IAdd } else { MirInstr::FAdd }
                    },
                )?
            }
            TypedExprHir::Sub(l, r) => {
                self.binop(
                    fb,
                    l,
                    r,
                    |int| {
                        if int { MirInstr::ISub } else { MirInstr::FSub }
                    },
                )?
            }
            TypedExprHir::Mul(l, r) => {
                self.binop(
                    fb,
                    l,
                    r,
                    |int| {
                        if int { MirInstr::IMul } else { MirInstr::FMul }
                    },
                )?
            }
            TypedExprHir::Div(l, r) => {
                self.binop(
                    fb,
                    l,
                    r,
                    |int| {
                        if int { MirInstr::IDiv } else { MirInstr::FDiv }
                    },
                )?
            }
            TypedExprHir::Mod(l, r) => {
                self.lower_expr_into(fb, l)?;
                self.lower_expr_into(fb, r)?;
                if matches!(l.ty(), Type::Int) && matches!(r.ty(), Type::Int) {
                    fb.emit(MirInstr::IMod);
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
                fb.emit(match expr {
                    TypedExprHir::Eq(..) => MirInstr::Eq,
                    TypedExprHir::Ne(..) => MirInstr::Ne,
                    TypedExprHir::Lt(..) => MirInstr::Lt,
                    TypedExprHir::Gt(..) => MirInstr::Gt,
                    TypedExprHir::Le(..) => MirInstr::Le,
                    _ => MirInstr::Ge,
                });
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
                fb.emit(MirInstr::True);
                let jt = fb.jump_later();
                let f_b = fb.new_block();
                fb.start(f_b);
                fb.emit(MirInstr::False);
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
                fb.emit(MirInstr::False);
                let jf = fb.jump_later();
                let t = fb.new_block();
                fb.start(t);
                fb.emit(MirInstr::True);
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
                fb.emit(MirInstr::Pop); // discard body's value
                fb.jump(head);
                let end = fb.new_block();
                fb.start(end);
                fb.emit(MirInstr::Unit);
                fb.resolve(c, end);
            }
            TypedExprHir::Choose(e) => self.lower_choose(fb, e)?,
            TypedExprHir::Call(callee, args, _) => match callee.as_ref() {
                TypedExprHir::Ident {
                    name, binding_id, ..
                } if self.is_function_binding(*binding_id) => {
                    for a in args {
                        self.lower_expr_into(fb, a)?;
                    }
                    fb.emit(MirInstr::Call(self.lcx.func_ids[name]));
                }
                _ => {
                    self.lower_expr_into(fb, callee)?;
                    for a in args {
                        self.lower_expr_into(fb, a)?;
                    }
                    fb.emit(MirInstr::CallValue(args.len() as u8));
                }
            },
            TypedExprHir::Block(b) => self.lower_block(fb, b)?,
            TypedExprHir::Cast(inner, _) => {
                self.lower_expr_into(fb, inner)?;
                fb.emit(MirInstr::ToStr);
            }
            // 0.0.2 U04: typeck accepts `?` and Result constructors; their
            // bytecode lowering (Result instruction group) lands with U06.
            TypedExprHir::Question { .. } => {
                return Err(LowerError::new(
                    LowerErrorKind::UnsupportedFeature {
                        feature: "`?` operator lowering",
                    },
                    expr.span(),
                ));
            }
            TypedExprHir::ResultCtor { .. } => {
                return Err(LowerError::new(
                    LowerErrorKind::UnsupportedFeature {
                        feature: "Ok/Err constructor lowering",
                    },
                    expr.span(),
                ));
            }
            // 0.0.2 U05: typeck accepts the ownership forms; their bytecode
            // lowering (AllocBox / DerefBox / StoreDerefBox instruction
            // group) lands with U06.
            TypedExprHir::Box(_, _) => {
                return Err(LowerError::new(
                    LowerErrorKind::UnsupportedFeature {
                        feature: "`box` expression lowering",
                    },
                    expr.span(),
                ));
            }
            TypedExprHir::Deref(_, _) => {
                return Err(LowerError::new(
                    LowerErrorKind::UnsupportedFeature {
                        feature: "`deref` expression lowering",
                    },
                    expr.span(),
                ));
            }
            TypedExprHir::AssignDeref { .. } => {
                return Err(LowerError::new(
                    LowerErrorKind::UnsupportedFeature {
                        feature: "`deref b = v` lowering",
                    },
                    expr.span(),
                ));
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
        pick: impl Fn(bool) -> MirInstr,
    ) -> Result<(), LowerError> {
        self.lower_expr_into(fb, l)?;
        self.lower_expr_into(fb, r)?;
        fb.emit(pick(matches!(l.ty(), Type::Int)));
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
            None => fb.emit(MirInstr::Unit),
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
        // Scrutinee loaded once; each arm: compare (+bind) (+guard), then body.
        // Layout: scrut; c0; body0; c1; body1; …; fallback-or-catchall; M
        self.lower_expr_into(fb, &e.scrutinee)?;

        let mut body_jumps: Vec<BlockId> = Vec::new();
        // Cond blocks of the *current* arm awaiting their false target.
        let mut pending: Vec<BlockId> = Vec::new();
        // Chain ends permanently at a catch-all (ident w/o guard).
        let mut chain_done = false;

        for arm in &e.arms {
            if chain_done {
                break;
            }
            match &arm.pattern {
                TypedPatternHir::Literal(lit) => {
                    fb.emit(MirInstr::Dup);
                    self.lower_expr_into(fb, lit)?;
                    fb.emit(MirInstr::Eq);
                    pending.push(fb.jump_if_false_later());
                    if let Some(guard) = &arm.guard {
                        let g = fb.new_block();
                        fb.start(g);
                        self.lower_expr_into(fb, guard)?;
                        pending.push(fb.jump_if_false_later());
                    }
                    let ok = fb.new_block();
                    fb.start(ok);
                    fb.emit(MirInstr::Pop); // discard scrutinee value
                    self.lower_block(fb, &arm.body)?;
                    body_jumps.push(fb.jump_later());
                }
                TypedPatternHir::Ident { binding_id, .. } => {
                    self.slots.slot_for(*binding_id);
                    // Invariant: slot_for above just allocated it.
                    let slot = self.slots.get(*binding_id).expect("slot just allocated");
                    fb.emit(MirInstr::BindMatch(slot));
                    match &arm.guard {
                        None => {
                            fb.emit(MirInstr::Pop);
                            self.lower_block(fb, &arm.body)?;
                            body_jumps.push(fb.jump_later());
                            chain_done = true;
                        }
                        Some(guard) => {
                            self.lower_expr_into(fb, guard)?;
                            pending.push(fb.jump_if_false_later());
                            let ok = fb.new_block();
                            fb.start(ok);
                            fb.emit(MirInstr::Pop);
                            self.lower_block(fb, &arm.body)?;
                            body_jumps.push(fb.jump_later());
                        }
                    }
                }
                // 0.0.2 U04: typeck accepts Ok/Err patterns; their lowering
                // (Result match instructions) lands with U06.
                TypedPatternHir::ResultCtor { span, .. } => {
                    return Err(LowerError::new(
                        LowerErrorKind::UnsupportedFeature {
                            feature: "Ok/Err pattern lowering",
                        },
                        *span,
                    ));
                }
                TypedPatternHir::Error => {}
            }

            // Advance to a fresh block for the next arm's preamble and
            // patch this arm's deferred false targets to it.
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
            // No-match fallback (guarded choose without otherwise):
            // pop the scrutinee value and produce Unit.
            fb.emit(MirInstr::Pop);
            fb.emit(MirInstr::Unit);
            body_jumps.push(fb.jump_later());
        }

        let m = fb.new_block();
        fb.start(m);
        for j in body_jumps {
            fb.resolve(j, m);
        }
        Ok(())
    }
}
