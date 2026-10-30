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
    Access, StructId, StructInfo, Type, TypedBlockHir, TypedChooseArmHir, TypedConstDeclHir,
    TypedDeclHir, TypedExprChooseHir, TypedExprForHir, TypedExprHir, TypedExprIfHir,
    TypedExprWhileHir, TypedFuncBodyHir, TypedFuncDeclHir, TypedHir, TypedHirItem, TypedParamHir,
    TypedPatternHir, TypedStmtHir, TypedStructDef, TypedVarBindingHir,
};
use crate::typeck::unify::{as_func_type, as_result_type, unify, unify_arg, unify_assign};
use std::collections::{HashMap, HashSet};

/// One struct-table entry (0.0.3 V02). Built in three phases by
/// [`TypeChecker::collect_structs`]: names registered, copy-ness computed
/// in dependency order (against the raw AST field types), then converted
/// field types filled in.
struct StructEntry {
    name: String,
    /// Declared fields in declaration order, with converted types.
    fields: Vec<(String, Type, Span)>,
    /// D2 cached verdict: a struct is Copy iff every field is Copy.
    copy: bool,
    decl_span: Span,
}

impl StructEntry {
    /// Declaration-order index of a field, if declared.
    fn field_index(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|(n, _, _)| n == name)
    }
}

/// Collect the names of every named (struct) type mentioned anywhere in
/// `ty` (recursing through composites) and feed them to `f`.
fn collect_named_type_refs(ty: &ast::Type, f: &mut impl FnMut(&str)) {
    match ty {
        ast::Type::Named(name) => f(name),
        ast::Type::Array(el) | ast::Type::Box(el) | ast::Type::Ref(el) => {
            collect_named_type_refs(el, f)
        }
        ast::Type::Result(ok, err) => {
            collect_named_type_refs(ok, f);
            collect_named_type_refs(err, f);
        }
        ast::Type::Func(params, ret) => {
            for p in params {
                collect_named_type_refs(p, f);
            }
            collect_named_type_refs(ret, f);
        }
        ast::Type::Future(inner) => collect_named_type_refs(inner, f),
        ast::Type::Channel(inner) => collect_named_type_refs(inner, f),
        ast::Type::Ptr(inner) => collect_named_type_refs(inner, f),
        ast::Type::Base(_) => {}
        ast::Type::Range => {}
    }
}

/// The assignment target of a field assignment `p.f = expr` (0.0.3 V02, D2):
/// the object binding, plus the field being written.
struct FieldPlace {
    obj_name: String,
    binding_id: BindingId,
    field: String,
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
    /// Whether each enclosing function is `async` (innermost last). 0.0.4
    /// T03 (D2): a borrow may not appear anywhere lexically inside an
    /// `async` fn, nested sync declarations included.
    fn_async_stack: Vec<bool>,
    /// Whether each enclosing function is `unsafe` (innermost last). 0.0.5
    /// T03 (D3): unsafe ops and unsafe calls require an unsafe/trusted context.
    fn_unsafe_stack: Vec<bool>,
    /// Whether each enclosing function is `trusted` (innermost last). 0.0.5
    /// T03 (D3): trusted functions may contain unsafe ops and are callable
    /// from anywhere.
    fn_trusted_stack: Vec<bool>,
    /// Ref bindings already diagnosed by `RefInAsync`; a ref parameter is
    /// reported once at its declaration, so later uses do not repeat it.
    reported_ref_ids: HashSet<BindingId>,
    /// Binding ID to type mapping.
    binding_types: HashMap<BindingId, Type>,
    /// Fully qualified name → type for top-level var/const declarations,
    /// used by imported bindings whose local `BindingId` differs from the
    /// declaration site (0.0.3 V05).
    global_types: HashMap<String, Type>,
    /// Binding IDs of top-level (global) variables/consts. 0.0.2 U05: owned
    /// globals are read as deep copies and may never be moved — ownership
    /// checks key off this set.
    global_ids: HashSet<BindingId>,
    /// Struct table, indexed by [`StructId`] (0.0.3 V02).
    structs: Vec<StructEntry>,
    /// Struct name → id (declaration order; duplicates are rejected).
    struct_by_name: HashMap<String, StructId>,
    /// Function name → (is_unsafe, is_trusted) mapping. Built during typeck.
    func_flags: HashMap<String, (bool, bool)>,
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
            fn_async_stack: Vec::new(),
            fn_unsafe_stack: Vec::new(),
            fn_trusted_stack: Vec::new(),
            reported_ref_ids: HashSet::new(),
            binding_types: HashMap::new(),
            global_types: HashMap::new(),
            global_ids: HashSet::new(),
            structs: Vec::new(),
            struct_by_name: HashMap::new(),
            func_flags: HashMap::new(),
        }
    }

    /// Add an error to the collection.
    fn add_error(&mut self, kind: TypeckErrorKind, span: Span) {
        self.errors.push(TypeckError::new(kind, span));
    }

    /// Whether any lexically enclosing function is `async` (0.0.4 T03/D2).
    fn in_async_context(&self) -> bool {
        self.fn_async_stack.iter().any(|&is_async| is_async)
    }

    /// Whether the innermost enclosing function is `unsafe` (0.0.5 T03/D3).
    ///
    /// Nested `func` declarations carry their own flags, not the parent's, so
    /// only the innermost stack entry decides the context (contrast with
    /// `in_async_context`, where the async rule *does* extend to nested sync
    /// declarations).
    fn in_unsafe_context(&self) -> bool {
        self.fn_unsafe_stack.last().copied().unwrap_or(false)
    }

    /// Whether the innermost enclosing function is `trusted` (0.0.5 T03/D3).
    fn in_trusted_context(&self) -> bool {
        self.fn_trusted_stack.last().copied().unwrap_or(false)
    }

    /// Whether the current context permits unsafe operations (unsafe or trusted).
    fn can_use_unsafe(&self) -> bool {
        self.in_unsafe_context() || self.in_trusted_context()
    }

    /// Report a `ref` use inside async code (0.0.4 T03/D2). A binding is
    /// reported at most once: an async fn's own ref parameter is already
    /// diagnosed at its declaration.
    fn check_ref_in_async(&mut self, ty: &Type, binding_id: BindingId, span: Span) {
        if self.in_async_context()
            && matches!(ty, Type::Ref(_))
            && self.reported_ref_ids.insert(binding_id)
        {
            self.add_error(TypeckErrorKind::RefInAsync, span);
        }
    }

    /// Convert `ast::Type` to `typed_hir::Type` (0.0.3 V02: struct-aware).
    ///
    /// A named type resolves through the struct table; an unknown name
    /// records `UnknownType` and degrades to `Type::Unsupported` — the
    /// overall compilation fails with the collected errors anyway, and the
    /// `Unsupported` placeholder keeps the caller total.
    fn convert_type(&mut self, ty: &ast::Type, span: Span) -> Type {
        match ty {
            ast::Type::Base(BaseType::Int) => Type::Int,
            ast::Type::Base(BaseType::Float) => Type::Float,
            ast::Type::Base(BaseType::Bool) => Type::Bool,
            ast::Type::Base(BaseType::String) => Type::String,
            ast::Type::Base(BaseType::Unit) => Type::Unit,
            ast::Type::Func(params, ret) => {
                let param_types: Vec<Type> =
                    params.iter().map(|p| self.convert_type(p, span)).collect();
                Type::Func(param_types, Box::new(self.convert_type(ret, span)))
            }
            ast::Type::Result(ok, err) => Type::Result(
                Box::new(self.convert_type(ok, span)),
                Box::new(self.convert_type(err, span)),
            ),
            ast::Type::Array(elem) => Type::Array(Box::new(self.convert_type(elem, span))),
            ast::Type::Box(inner) => Type::Box(Box::new(self.convert_type(inner, span))),
            ast::Type::Ref(inner) => Type::Ref(Box::new(self.convert_type(inner, span))),
            ast::Type::Range => Type::Range,
            ast::Type::Future(inner) => Type::Future(Box::new(self.convert_type(inner, span))),
            ast::Type::Channel(inner) => Type::Channel(Box::new(self.convert_type(inner, span))),
            // 0.0.5 T02 D1: a raw pointer type, non-Copy and move-only.
            ast::Type::Ptr(inner) => Type::Ptr(Box::new(self.convert_type(inner, span))),
            ast::Type::Named(name) => match self.struct_by_name.get(name) {
                Some(&id) => {
                    let entry = &self.structs[id.0 as usize];
                    Type::Struct(Box::new(StructInfo {
                        id,
                        name: entry.name.clone().into_boxed_str(),
                        copy: entry.copy,
                    }))
                }
                None => {
                    self.add_error(TypeckErrorKind::UnknownType { name: name.clone() }, span);
                    Type::Unsupported(name.clone())
                }
            },
        }
    }

    /// Build the struct table from the HIR's struct declarations (0.0.3 V02).
    ///
    /// Three phases, in this order:
    /// 1. register every declaration's name (duplicates reported);
    /// 2. cycle detection over the struct-reference graph, then copy-ness
    ///    (`is_copy`) in dependency order — a struct is Copy iff every
    ///    field is Copy (D2); acyclicity (phase 2) is what makes the
    ///    recursion terminate;
    /// 3. field-type conversion, so every `Type::Struct` value carries the
    ///    final name/copy verdict.
    fn collect_structs(&mut self, hir: &Hir) {
        // Phase 1: names. Raw field types are kept in a parallel vec (index
        // = StructId) until every verdict is final.
        let mut raw: Vec<Vec<(String, ast::Type, Span)>> = Vec::new();
        for decl in &hir.structs {
            if self.struct_by_name.contains_key(&decl.name) {
                self.add_error(
                    TypeckErrorKind::DuplicateStructName {
                        name: decl.name.clone(),
                    },
                    decl.span,
                );
                continue;
            }
            let id = StructId(self.structs.len() as u32);
            self.structs.push(StructEntry {
                name: decl.name.clone(),
                fields: Vec::new(),
                copy: false,
                decl_span: decl.span,
            });
            raw.push(
                decl.fields
                    .iter()
                    .map(|f| (f.name.clone(), f.ty.clone(), f.span))
                    .collect(),
            );
            self.struct_by_name.insert(decl.name.clone(), id);
        }

        // Phase 2a: cycle detection (D1: direct or indirect, including via
        // `box` — a box field is still an edge to the pointee struct).
        let mut color: Vec<u8> = vec![0; self.structs.len()]; // 0 white, 1 gray, 2 black
        for start in 0..self.structs.len() {
            if color[start] == 0 {
                self.detect_struct_cycle(start, &raw, &mut color, &mut Vec::new());
            }
        }

        // Phase 2b: copy-ness in dependency order (memoized DFS).
        let ids: Vec<StructId> = (0..self.structs.len() as u32).map(StructId).collect();
        for id in ids {
            let copy = self.compute_struct_copy(id, &raw, &mut HashSet::new());
            self.structs[id.0 as usize].copy = copy;
        }

        // Phase 3: convert field types now that every verdict is final.
        for (i, fields) in raw.into_iter().enumerate() {
            let converted: Vec<(String, Type, Span)> = fields
                .into_iter()
                .map(|(name, ty, span)| {
                    let conv = self.convert_type(&ty, span);
                    // 0.0.4 T07d: channels are non-storable — a field read
                    // deep-copies owned values, which would fork the shared
                    // buffer silently. 0.0.4 T08: a Future is non-storable
                    // for the same reason — the read would fork the suspended
                    // coroutine frame (this also retires the codegen-side
                    // debug_assert that used to be the only guard). The check
                    // sees through array nesting: a `[Future]` field is
                    // copied wholesale by a field read, and a
                    // `Result<Future, _>` / `Result<channel<_>, _>` field
                    // through the payload the derived `Clone` deep-copies.
                    if conv.contains_channel() {
                        self.add_error(TypeckErrorKind::ChannelNotStorable, span);
                    } else if conv.contains_future() {
                        self.add_error(TypeckErrorKind::FutureNotStorable, span);
                    } else if conv.contains_ptr() {
                        // 0.0.5 T02 D1/D2: a pointer field read is a
                        // safe-code deep copy that would hand out a pointer
                        // without `unsafe` — pointers are non-storable.
                        self.add_error(TypeckErrorKind::PtrNotStorable, span);
                    }
                    (name, conv, span)
                })
                .collect();
            self.structs[i].fields = converted;
        }
    }

    /// DFS cycle detection over struct field references. `path` carries the
    /// current stack of struct names; a gray hit closes a cycle and reports
    /// `RecursiveStruct` with the full path.
    fn detect_struct_cycle(
        &mut self,
        id: usize,
        raw: &[Vec<(String, ast::Type, Span)>],
        color: &mut [u8],
        path: &mut Vec<String>,
    ) {
        if color[id] == 1 {
            // Cycle closes here: render from the first occurrence of `id`.
            let start = path
                .iter()
                .position(|n| *n == self.structs[id].name)
                .unwrap_or(0);
            let mut cycle: Vec<String> = path[start..].to_vec();
            cycle.push(self.structs[id].name.clone());
            let span = self.structs[id].decl_span;
            self.add_error(
                TypeckErrorKind::RecursiveStruct {
                    path: cycle.join(" → "),
                },
                span,
            );
            return;
        }
        if color[id] == 2 {
            return;
        }
        color[id] = 1;
        path.push(self.structs[id].name.clone());
        let refs = self.struct_field_struct_refs(&raw[id]);
        for r in refs {
            self.detect_struct_cycle(r, raw, color, path);
        }
        path.pop();
        color[id] = 2;
    }

    /// Struct ids directly referenced by `raw` field types (via `Named`
    /// names resolved through the table; unknown names are skipped here —
    /// they get their own error at conversion time).
    fn struct_field_struct_refs(&self, fields: &[(String, ast::Type, Span)]) -> Vec<usize> {
        let mut refs = Vec::new();
        for (_, ty, _) in fields {
            collect_named_type_refs(ty, &mut |name| {
                if let Some(&r) = self.struct_by_name.get(name) {
                    refs.push(r.0 as usize);
                }
            });
        }
        refs
    }

    /// D2 copy verdict for struct `id`: Copy iff every field type is Copy.
    /// `in_progress` guards the recursion (cycles are reported separately;
    /// a cycle participant degrades to not-Copy so this stays total).
    fn compute_struct_copy(
        &mut self,
        id: StructId,
        raw: &[Vec<(String, ast::Type, Span)>],
        in_progress: &mut HashSet<usize>,
    ) -> bool {
        let idx = id.0 as usize;
        if !in_progress.insert(idx) {
            return false; // cycle (already reported): treat as owned
        }
        let mut all_copy = true;
        for (_, ty, _) in &raw[idx] {
            if !self.ast_type_is_copy(ty, raw, in_progress) {
                all_copy = false;
                break;
            }
        }
        in_progress.remove(&idx);
        all_copy
    }

    /// Copy-ness of a raw (AST) field type. Struct references recurse into
    /// the table verdict; `box`/array/`ref` are never Copy.
    fn ast_type_is_copy(
        &mut self,
        ty: &ast::Type,
        raw: &[Vec<(String, ast::Type, Span)>],
        in_progress: &mut HashSet<usize>,
    ) -> bool {
        match ty {
            ast::Type::Base(BaseType::Int)
            | ast::Type::Base(BaseType::Float)
            | ast::Type::Base(BaseType::Bool)
            | ast::Type::Base(BaseType::Unit) => true,
            ast::Type::Base(BaseType::String) => false,
            ast::Type::Named(name) => match self.struct_by_name.get(name) {
                Some(&id) => {
                    if self.structs[id.0 as usize].copy {
                        true
                    } else {
                        self.compute_struct_copy(id, raw, in_progress)
                    }
                }
                None => false,
            },
            ast::Type::Array(_) | ast::Type::Box(_) | ast::Type::Ref(_) => false,
            ast::Type::Range => true, // Range is Copy: two ints (D5)
            // F3: a Future owns its suspend state — never a Copy struct field
            // (and cannot be a struct field at all; see codegen tags).
            ast::Type::Future(_) => false,
            ast::Type::Result(ok, err) => {
                self.ast_type_is_copy(ok, raw, in_progress)
                    && self.ast_type_is_copy(err, raw, in_progress)
            }
            ast::Type::Func(_, _) => true,
            // F9: a channel owns its buffer — never Copy (move-only).
            ast::Type::Channel(_) => false,
            // 0.0.5 T02 D1: a ptr is a move-only handle — never Copy (and
            // cannot be a struct field at all; `PtrNotStorable`).
            ast::Type::Ptr(_) => false,
        }
    }

    /// Register builtin functions.
    ///
    /// Seeds `func_signatures` so that a bare builtin identifier (e.g. `print`
    /// passed as a value) has a function type. `Ok`/`Err` are not registered
    /// here: their type depends on context, so calls are recognized by name in
    /// `typeck_call` (unless shadowed by a user function, §4.3).
    /// 0.0.3 V06: `range`/`range_from` register with concrete signatures.
    ///
    /// Schematic builtins (`std.arr.*`) are deliberately **not** registered:
    /// their signatures contain table-only type variables, and `Type::Var`
    /// must never reach TypedHir (PLAN §10). They are call-only — a value
    /// use (`f = std.arr.len`) reports `UndefinedVariable` instead.
    fn register_builtins(&mut self) {
        for sig in crate::typeck::builtins::builtins() {
            // Variadic builtins get their element type as the single
            // parameter for value-position lookup; call checking uses the
            // table's real (variadic) shape.
            let params: Vec<Type> = match &sig.param {
                BuiltinParam::Fixed(params) => params.clone(),
                BuiltinParam::Variadic(elem) => elem.clone(),
                BuiltinParam::Schematic { .. } => continue,
            };
            self.func_signatures
                .insert(sig.name.to_string(), (params, sig.ret.clone()));
        }
    }

    /// Collect function signatures in a forward pass.
    fn collect_func_signatures(&mut self, hir: &Hir) {
        for item in &hir.items {
            if let HirItem::Decl(DeclHir::Func(func)) = item {
                let param_types: Vec<Type> = func
                    .params
                    .iter()
                    .map(|p| self.convert_type(&p.ty, p.span))
                    .collect();
                let ret_type = func
                    .ret_type
                    .as_ref()
                    .map(|t| {
                        let span = func.span;
                        self.convert_type(t, span)
                    })
                    .unwrap_or(Type::Unit);
                // F1: an async fn's *caller* sees `Future<T>` (the body still
                // returns `T`), so its call-site signature is wrapped here.
                let ret_type = if func.is_async {
                    Type::Future(Box::new(ret_type))
                } else {
                    ret_type
                };
                self.func_signatures
                    .insert(func.name.clone(), (param_types, ret_type));
                self.user_func_names.insert(func.name.clone());
                // 0.0.5 T03: collect unsafe/trusted flags in the same forward
                // pass so a call to a later-defined unsafe func is still gated.
                self.func_flags
                    .insert(func.name.clone(), (func.is_unsafe, func.is_trusted));
            }
        }
    }

    /// Type check the entire program.
    pub fn typeck(mut self, hir: Hir) -> Result<TypedHir, Vec<TypeckError>> {
        self.register_builtins();
        self.collect_structs(&hir);
        self.collect_func_signatures(&hir);
        self.collect_global_ids(&hir);

        let typed_globals = self.typeck_globals_first(&hir);
        let mut typed_items = Vec::new();
        for (idx, item) in hir.items.into_iter().enumerate() {
            if let Some(typed_global) = typed_globals.get(&idx) {
                typed_items.push(typed_global.clone());
                continue;
            }
            if let Ok(typed_item) = self.typeck_item(item) {
                typed_items.push(typed_item);
            }
        }

        if !self.errors.is_empty() {
            return Err(self.errors);
        }

        let structs = self
            .structs
            .iter()
            .map(|entry| TypedStructDef {
                field_types: entry.fields.iter().map(|(_, ty, _)| ty.clone()).collect(),
            })
            .collect();

        Ok(TypedHir {
            items: typed_items,
            structs,
            span: hir.span,
        })
    }

    /// Typecheck top-level var/const declarations first so imported globals
    /// (`net.answer`, `net.g`) have a type by the time any module's items
    /// reference them (0.0.3 V05). Returns a map from HIR index to the typed
    /// item so the main loop preserves original declaration order.
    fn typeck_globals_first(&mut self, hir: &Hir) -> HashMap<usize, TypedHirItem> {
        let mut typed = HashMap::new();
        for (idx, item) in hir.items.iter().enumerate() {
            let typed_decl = match item {
                HirItem::Decl(DeclHir::Var(v)) => {
                    if let Ok(typed) = self.typeck_var_binding(v.clone()) {
                        let ty = typed.ty.clone().unwrap_or_else(|| typed.init.ty());
                        // 0.0.4 T08: globals are module-wide readable storage,
                        // and every read is a deep copy (`CloneGlobal`) — a
                        // channel global would hand every reader an alias
                        // without `split` (T07's accepted-for-0.0.4 alias is
                        // withdrawn here), and a Future global would fork the
                        // suspended frame on every read.
                        if self.reject_async_global_type(&ty, v.span) {
                            None
                        } else {
                            self.global_types.insert(v.name.clone(), ty);
                            Some(TypedHirItem::Decl(TypedDeclHir::Var(typed)))
                        }
                    } else {
                        None
                    }
                }
                HirItem::Decl(DeclHir::Const(c)) => {
                    if let Ok(typed) = self.typeck_const_decl(c.clone()) {
                        let ty = typed.ty.clone().unwrap_or_else(|| typed.init.ty());
                        if self.reject_async_global_type(&ty, c.span) {
                            None
                        } else {
                            self.global_types.insert(c.name.clone(), ty);
                            Some(TypedHirItem::Decl(TypedDeclHir::Const(typed)))
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(typed_item) = typed_decl {
                typed.insert(idx, typed_item);
            }
        }
        typed
    }

    /// 0.0.4 T08 (D4 wiring): a global may not hold a `channel<T>`, a
    /// `Future<T>`, or a value-carrying container of either (`[T]`,
    /// `Result<T, E>`) — every global read is a deep copy, which would alias
    /// the shared buffer without `split` or duplicate the suspended frames.
    /// Reports the error and returns `true` when rejected.
    fn reject_async_global_type(&mut self, ty: &Type, span: Span) -> bool {
        if ty.contains_channel() {
            self.add_error(TypeckErrorKind::ChannelNotStorable, span);
            true
        } else if ty.contains_future() {
            self.add_error(TypeckErrorKind::FutureNotStorable, span);
            true
        } else if ty.contains_ptr() {
            // 0.0.5 T02 D1/D2: every global read is a deep copy, which would
            // hand out a pointer in safe code — pointers are non-storable.
            self.add_error(TypeckErrorKind::PtrNotStorable, span);
            true
        } else {
            false
        }
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
        self.fn_async_stack.push(func.is_async);
        self.fn_unsafe_stack.push(func.is_unsafe);
        self.fn_trusted_stack.push(func.is_trusted);

        // Add parameters to scope
        let mut typed_params = Vec::new();
        for param in &func.params {
            // Keep the resolver's binding_id so that TypedParamHir, body
            // identifier uses, and slot allocation in `lower` all refer to
            // the same binding.
            let binding_id = param.binding_id;
            let hir_id = param.hir_id;
            let param_type = self.convert_type(&param.ty, param.span);
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
                    global_name: None,
                },
            );
            // Store the parameter type
            self.binding_types.insert(binding_id, param_type.clone());
            // 0.0.4 T03 (D2): no borrow may exist inside async code,
            // including an async fn's own `ref` parameters. The resolver
            // rejects nested/embedded refs before typeck, so a parameter
            // can only carry a top-level `ref T`.
            if self.in_async_context() && matches!(&param.ty, ast::Type::Ref(_)) {
                self.reported_ref_ids.insert(binding_id);
                self.add_error(TypeckErrorKind::RefInAsync, param.span);
            }
            // 0.0.4 T07: a channel handle cannot sit behind a `ref` — its
            // `send`/`recv` park the running task and resume it later, which
            // would carry the borrowed handle across a suspension. Without
            // this check the handle would flow into the T07a member rewrite
            // only to fail there with a misleading `ChannelMemberNotFound`.
            if let ast::Type::Ref(inner) = &param.ty
                && matches!(inner.as_ref(), ast::Type::Channel(_))
            {
                self.add_error(TypeckErrorKind::ChannelNotBorrowable, param.span);
            }
            typed_params.push(TypedParamHir {
                name: param.name.clone(),
                ty: param_type,
                binding_id,
                hir_id,
                span: param.span,
            });
        }

        // Return type: `None` means the function has no annotation (unit).
        let ret_ty: Option<Type> = func
            .ret_type
            .as_ref()
            .map(|t| self.convert_type(t, func.span));

        // The declared return type drives both the `expected` thread into the
        // body (Ok/Err constructors) and the `?` legality check, which needs
        // it even in positions `expected` cannot reach.
        self.fn_ret_stack.push(ret_ty.clone());
        let body_result = self.typeck_func_body(&func.body, ret_ty.as_ref());
        self.fn_ret_stack.pop();
        // Pop before `?`: errors are collected, so a leaked flag would
        // misclassify every later function.
        self.fn_async_stack.pop();
        self.fn_unsafe_stack.pop();
        self.fn_trusted_stack.pop();
        let typed_body = body_result?;

        // Exit function scope
        self.scopes.exit_scope();

        Ok(TypedFuncDeclHir {
            name: func.name,
            params: typed_params,
            ret_type: func.ret_type.clone().map(|t| {
                let span = func.span;
                self.convert_type(&t, span)
            }),
            body: typed_body,
            hir_id: func.hir_id,
            span: func.span,
            is_async: func.is_async,
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
        let typed_ty = var.ty.as_ref().map(|t| self.convert_type(t, var.span));
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
                global_name: None,
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
        let typed_ty = c.ty.as_ref().map(|t| self.convert_type(t, c.span));
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
                global_name: None,
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
                self.check_ref_in_async(&ty, binding_id, span);
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
                // 0.0.4 T03 (D5): async code may read globals but not write
                // them. Inside a function, a bare `g = v` is a local binding
                // (never this node); the reachable global write here is the
                // module-qualified form `m.g = v`, whose name is the mangled
                // dotted path.
                if self.in_async_context()
                    && (self.global_ids.contains(&binding_id)
                        || (name.contains('.') && self.global_types.contains_key(&name)))
                {
                    self.add_error(
                        TypeckErrorKind::AsyncGlobalWrite { name: name.clone() },
                        span,
                    );
                    return Err(());
                }
                let typed_rhs = self.typeck_expr(*rhs, None)?;
                let var_type = match self.lookup_type(&name, span) {
                    Ok(ty) => ty,
                    Err(()) => match self.global_types.get(&name) {
                        Some(ty) => ty.clone(),
                        None => return Err(()),
                    },
                };

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
            ExprHir::For(for_expr) => self.typeck_for_expr(for_expr),
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
            ExprHir::Index(arr, idx, span) => self.typeck_index(*arr, *idx, span),
            ExprHir::ArrayLit(elems, span) => self.typeck_array_lit(elems, expected, span),
            // 0.0.2 U05: ownership expressions — validated here; the
            // flow-sensitive move analysis runs afterwards in
            // `ownership.rs` over the produced TypedHir.
            ExprHir::Move(inner, span) => self.typeck_move(*inner, span),
            ExprHir::Clone(inner, span) => self.typeck_clone(*inner, span),
            ExprHir::Box(inner, span) => self.typeck_box(*inner, span),
            ExprHir::Deref(inner, span) => self.typeck_deref(*inner, span),
            // 0.0.5 T02 (D2): `addr <place>` — unsafe op; the place shape
            // and the referent's mutability were validated by the resolver.
            ExprHir::Addr {
                place,
                hir_id,
                span,
            } => self.typeck_addr(*place, hir_id, span),
            // 0.0.5 T02: `deref_ptr p` — read through a `ptr<T>`.
            ExprHir::DerefPtr(inner, span) => self.typeck_deref_ptr(*inner, span),
            // 0.0.2 U13: `as` cast — whitelist: only scalar (int/float/bool) → string.
            ExprHir::Cast(inner, ty, span) => self.typeck_cast(*inner, ty, span),
            // 0.0.2 U04: `?` Result propagation.
            ExprHir::Question(operand, span) => self.typeck_question(*operand, span),
            // 0.0.4 F2/F3: `e await` — drive a `Future<T>` to completion.
            ExprHir::Await(operand, span) => self.typeck_await(*operand, span),
            ExprHir::AssignDeref {
                name,
                binding_id,
                rhs,
                hir_id,
                span,
            } => self.typeck_assign_deref(name, binding_id, *rhs, hir_id, span),
            // 0.0.5 T02: `deref_ptr p = v` — write through a `ptr<T>`.
            ExprHir::AssignDerefPtr {
                name,
                binding_id,
                rhs,
                hir_id,
                span,
            } => self.typeck_assign_deref_ptr(name, binding_id, *rhs, hir_id, span),
            ExprHir::AssignField {
                obj_name,
                obj_binding_id,
                field,
                rhs,
                hir_id,
                span,
            } => self.typeck_assign_field(
                FieldPlace {
                    obj_name,
                    binding_id: obj_binding_id,
                    field,
                },
                *rhs,
                hir_id,
                span,
            ),
            ExprHir::AssignIndex {
                name,
                binding_id,
                index,
                rhs,
                hir_id,
                span,
            } => self.typeck_assign_index(name, binding_id, *index, *rhs, hir_id, span),
            ExprHir::StructLit {
                name,
                inits,
                hir_id,
                span,
            } => self.typeck_struct_lit(name, inits, hir_id, span),
            ExprHir::ChannelNew {
                ty,
                capacity,
                hir_id,
                span,
            } => {
                let ty_hir = self.convert_type(&ty, span);
                self.typeck_channel_new(ty_hir, *capacity, hir_id, span)
            }
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

    /// Type check a `for` expression (0.0.3 V04): `for var in iterable { body }`.
    ///
    /// The iterable is type-checked once in the *enclosing* scope (it must not
    /// see the loop variable). The loop variable is then bound immutable with
    /// the element type (the array's element type, or `int` for a `Range`) in a
    /// fresh loop scope, and the body is type-checked within that scope. The
    /// closed iterable set is `{[T], Range}` (D4); anything else is
    /// `NotIterable`.
    fn typeck_for_expr(&mut self, for_expr: ExprForHir) -> Result<TypedExprHir, ()> {
        let typed_iterable = self.typeck_expr(*for_expr.iterable, None)?;
        let iterable_ty = typed_iterable.ty().clone();

        // Determine the loop-variable (element) type and validate the iterable
        // is in the closed set.
        let elem_ty = match &iterable_ty {
            Type::Array(elem) => {
                // 0.0.4 T08: the desugar's element read is a deep copy, which
                // would fork a suspended Future frame — same rule as `ts[i]`.
                // The check sees through nesting: iterating `[[Future]]`
                // copies whole arrays of frames.
                if elem.contains_future() {
                    self.add_error(TypeckErrorKind::FutureIndexRead, for_expr.span);
                    return Err(());
                }
                (**elem).clone()
            }
            Type::Range => Type::Int,
            other => {
                self.add_error(
                    TypeckErrorKind::NotIterable { got: other.clone() },
                    for_expr.span,
                );
                return Err(());
            }
        };

        // Enter a loop scope and bind the loop variable with its element type.
        // `typeck_block` opens its own nested block scope for the body, so the
        // loop variable (declared here) is visible to body references through
        // the scope chain. The loop variable is immutable (D4); the
        // per-iteration write is performed by the lowering desugar.
        self.scopes.enter_block_scope();
        let _ = self.scopes.declare(
            for_expr.var.clone(),
            Binding {
                id: for_expr.var_binding_id,
                kind: BindingKind::Variable,
                mutable: false,
                builtin: false,
                ref_param: false,
                span: for_expr.span,
                hir_id: HirId(0),
                global_name: None,
            },
        );
        self.binding_types
            .insert(for_expr.var_binding_id, elem_ty.clone());

        let typed_body = self.typeck_block(&for_expr.body, None)?;
        self.scopes.exit_scope();

        Ok(TypedExprHir::For(TypedExprForHir {
            var: for_expr.var,
            var_binding_id: for_expr.var_binding_id,
            iterable: Box::new(typed_iterable),
            elem_ty,
            body: typed_body,
            ty: Type::Unit,
            hir_id: for_expr.hir_id,
            span: for_expr.span,
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
                        global_name: None,
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
                        global_name: None,
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

        // 0.0.3 (§1.4): struct/array values have no `==` — element-wise
        // equality needs a recursion rule and traits; the VM's defensive
        // TypeMismatch stays a backstop only. 0.0.4 (T07d): channels have no
        // `==` either — the derived `PartialEq` would compare *buffer
        // contents*, so two distinct channels that happen to hold equal
        // values would compare equal, which silently aliases what the
        // handle discipline (split-only forking) keeps distinct. 0.0.4 (T08):
        // a Future has no `==` — the derived comparison would inspect two
        // suspended frames, and either operand's frame read is a fork.
        if matches!(op, "==" | "!=")
            && (matches!(typed_lhs.ty(), Type::Struct(_) | Type::Array(_))
                || typed_lhs.ty().contains_channel()
                || typed_lhs.ty().contains_future()
                || typed_rhs.ty().contains_channel()
                || typed_rhs.ty().contains_future())
        {
            self.add_error(
                TypeckErrorKind::InvalidOperand {
                    op: op.to_string(),
                    ty: typed_lhs.ty(),
                },
                typed_lhs.span(),
            );
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
        // 0.0.4 T07a: `c.send(v)` / `c.recv()` — recognized in typeck, not grammar.
        // Parser produces Call(Field(c, "send"), [v]). If receiver type is channel<T>
        // and member is send/recv, rewrite to ChannelSend/ChannelRecv.
        if let ExprHir::Field(obj_expr, member) = &func
            && (member == "send" || member == "recv")
        {
            let typed_obj = self.typeck_expr(*obj_expr.clone(), None)?;
            let obj_span = typed_obj.span();
            if let Type::Channel(payload_ty) = typed_obj.ty() {
                // Rewrite to dedicated channel op.
                return match member.as_str() {
                    "send" => {
                        if args.len() != 1 {
                            self.add_error(
                                TypeckErrorKind::ArityMismatch {
                                    expected: 1,
                                    found: args.len(),
                                },
                                obj_span,
                            );
                            return Err(());
                        }
                        // The arity check above leaves exactly one argument;
                        // the empty arm is impossible (SPEC §4 unreachable).
                        let Some(val) = args.into_iter().next() else {
                            unreachable!("send arity checked to be exactly 1 above")
                        };
                        let typed_val = self.typeck_expr(val, None)?;
                        // D4 (0.0.4 review fix): a borrow handle never crosses
                        // a channel — the payload travels into another task's
                        // frame, and a `Value::Ref` stores an absolute
                        // operand-stack index that cannot survive a suspension
                        // snapshot. Note the borrow coercion below
                        // (`unify_arg_ref_aware`) deliberately does *not*
                        // apply here: a `ref string` payload is a D4 error,
                        // not a `string` (send `clone s` for a copy).
                        if matches!(typed_val.ty(), Type::Ref(_)) {
                            self.add_error(
                                TypeckErrorKind::ChannelPayloadBorrowed,
                                typed_val.span(),
                            );
                            return Err(());
                        }
                        // D6: the payload is consumed by the send — a bare
                        // owned local read would silently deep-copy the
                        // value; the transfer must be explicit (`move`/`clone`
                        // — Copy payloads and owned *globals* (deep-copy
                        // reads, D5) pass, same predicate as
                        // `reject_bare_owned_arg`).
                        if let TypedExprHir::Ident {
                            name,
                            binding_id,
                            ty,
                            access,
                            ..
                        } = &typed_val
                            && *access == Access::Copy
                            && ty.is_owned()
                            && !self.global_ids.contains(binding_id)
                            && !self.global_types.contains_key(name)
                        {
                            self.add_error(
                                TypeckErrorKind::OwnedChannelArgRequiresMove { name: name.clone() },
                                typed_val.span(),
                            );
                            return Err(());
                        }
                        // The payload_ty must match.
                        if !Self::unify_arg_ref_aware(
                            &payload_ty,
                            &typed_val.ty(),
                            0,
                            typed_val.span(),
                        )
                        .is_ok()
                        {
                            self.add_error(
                                TypeckErrorKind::TypeMismatch {
                                    expected: payload_ty.as_ref().clone(),
                                    found: typed_val.ty().clone(),
                                },
                                typed_val.span(),
                            );
                            return Err(());
                        }
                        Ok(TypedExprHir::ChannelSend {
                            channel: Box::new(typed_obj),
                            value: Box::new(typed_val),
                            payload_ty: payload_ty.as_ref().clone(),
                            span: obj_span,
                        })
                    }
                    "recv" => {
                        if !args.is_empty() {
                            self.add_error(
                                TypeckErrorKind::ArityMismatch {
                                    expected: 0,
                                    found: args.len(),
                                },
                                obj_span,
                            );
                            return Err(());
                        }
                        Ok(TypedExprHir::ChannelRecv {
                            channel: Box::new(typed_obj),
                            payload_ty: payload_ty.as_ref().clone(),
                            span: obj_span,
                        })
                    }
                    _ => unreachable!(),
                };
            }
            // A `send`/`recv` on a non-channel receiver (T07a): a struct may
            // legitimately have a field of that name — fall through to the
            // ordinary call path — but no other type has such a member.
            let obj_ty = match typed_obj.ty() {
                Type::Ref(inner) => *inner,
                ty => ty,
            };
            if !matches!(obj_ty, Type::Struct(_)) {
                self.add_error(
                    TypeckErrorKind::ChannelMemberNotFound {
                        member: member.clone(),
                        found: obj_ty.clone(),
                    },
                    obj_span,
                );
                return Err(());
            }
        }

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
            return self.typeck_builtin_call(&sig, func, args);
        }

        // 0.0.5 T03: Check for unsafe function call (direct identifier).
        // This is lexical: if the callee is `unsafe func`, it must be called from an unsafe/trusted context.
        // First-class functions (function values) are not covered by this static check — documented limitation.
        if let ExprHir::Ident { name, .. } = &func
            && let Some(&(is_unsafe, _)) = self.func_flags.get(name)
            && is_unsafe
            && !self.can_use_unsafe()
        {
            self.add_error(
                TypeckErrorKind::UnsafeCallOutsideUnsafe {
                    func_name: name.clone(),
                },
                func.span(),
            );
            return Err(());
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
        // Schematic builtins bind their table type variable from the
        // arguments; they take their own path (see below).
        if matches!(sig.param, BuiltinParam::Schematic { .. }) {
            return self.typeck_schematic_builtin_call(sig, func, args);
        }

        let typed_func = self.typeck_expr(func, None)?;

        let allowed: &[Type] = match &sig.param {
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
            // Handled above.
            BuiltinParam::Schematic { .. } => return Err(()),
        };

        let mut typed_args = Vec::new();
        for (i, arg) in args.into_iter().enumerate() {
            let typed_arg = self.typeck_expr(arg, None)?;
            // Fixed: the expected type is the i-th parameter. Variadic: any
            // of the element types — report the mismatch against the first
            // (single-element sets cover the only builtin today).
            let expected_ty = match &sig.param {
                BuiltinParam::Fixed(params) => params[i].clone(),
                BuiltinParam::Variadic(elem) => elem.first().cloned().unwrap_or(Type::Unit),
                // Handled above.
                BuiltinParam::Schematic { .. } => return Err(()),
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
            // 0.0.4 T08: a builtin that consumes a Future-typed argument
            // (`std.async.run`) must not receive a bare read — the read would
            // clone the array and its suspended frames, leaving the original
            // binding alive to hand the same task out again (`run(ts);
            // run(ts)` would drive the task twice). `move` is required,
            // exactly as for a user-function owned parameter. Scalar/string
            // arguments are unaffected (D3 auto-copy), and channel handles
            // read as plain loads by design (T07d).
            if typed_arg.ty().contains_future() {
                self.reject_bare_owned_arg(&typed_arg, typed_arg.span())?;
            }
            typed_args.push(typed_arg);
        }

        Ok(TypedExprHir::Call(
            Box::new(typed_func),
            typed_args,
            sig.ret.clone(),
        ))
    }

    /// Check a schematic builtin call (`std.arr.*`, 0.0.3 V06).
    ///
    /// The table signature carries the type variable `T0`; it is bound from
    /// the argument types at this call site (one-call unification — the
    /// 0.0.2 `Ok`/`Err` context-inference mechanism extended to the table;
    /// anything beyond it is 0.1.0 generics) and substituted into the
    /// parameter and return types. `T0` never reaches TypedHir (PLAN §10).
    fn typeck_schematic_builtin_call(
        &mut self,
        sig: &crate::typeck::builtins::BuiltinSig,
        func: ExprHir,
        args: Vec<ExprHir>,
    ) -> Result<TypedExprHir, ()> {
        // `typeck_call` only reaches here for the bare builtin identifier
        // (a schematic builtin is recognized by name, unshadowed).
        let (name, binding_id, hir_id, func_span) = match func {
            ExprHir::Ident {
                name,
                binding_id,
                hir_id,
                span,
            } => (name, binding_id, hir_id, span),
            other => {
                // Not reachable through `typeck_call`: the schematic path
                // requires a name-recognized callee.
                self.add_error(
                    TypeckErrorKind::InternalError {
                        message: "schematic builtin callee is not an identifier".to_string(),
                    },
                    other.span(),
                );
                return Err(());
            }
        };

        let mut typed_args = Vec::with_capacity(args.len());
        let mut arg_types = Vec::with_capacity(args.len());
        for arg in args {
            let typed_arg = self.typeck_expr(arg, None)?;
            arg_types.push(typed_arg.ty().clone());
            typed_args.push(typed_arg);
        }

        // 0.0.4 T08: same bare-read rule as the fixed builtin path — a
        // schematic call over an array of futures (`std.arr.get(ts, 0)`)
        // would copy frames through the element read. No schematic builtin
        // legitimately takes a future-typed argument today: element access
        // is `FutureIndexRead`'s job to reject, and the whole-array consumer
        // is `std.async.run`.
        for typed_arg in &typed_args {
            if typed_arg.ty().contains_future() {
                self.reject_bare_owned_arg(typed_arg, typed_arg.span())?;
            }
        }

        // 0.0.4 T07d: `std.concurrent.split(ch)` — rewrite to ChannelSplit.
        // The schematic signature has T0 as the payload type; we bind it and
        // emit the dedicated node.
        if name == "std.concurrent.split" {
            let expected_arity = 1;
            if arg_types.len() != expected_arity {
                self.add_error(
                    TypeckErrorKind::ArityMismatch {
                        expected: expected_arity,
                        found: arg_types.len(),
                    },
                    func_span,
                );
                return Err(());
            }
            let arg_ty = &arg_types[0];
            let Type::Channel(_payload_ty) = arg_ty else {
                self.add_error(
                    TypeckErrorKind::InvalidOperand {
                        op: "std.concurrent.split".to_string(),
                        ty: arg_ty.clone(),
                    },
                    typed_args[0].span(),
                );
                return Err(());
            };
            // The arity check above leaves exactly one argument; the empty
            // arm is impossible (SPEC §4 unreachable).
            let Some(channel) = typed_args.into_iter().next() else {
                unreachable!("split arity checked to be exactly 1 above")
            };
            return Ok(TypedExprHir::ChannelSplit {
                channel: Box::new(channel),
                span: func_span,
            });
        }

        // Bind `T0` from the argument types and substitute it into the
        // parameter and return types.
        let expected_arity = match &sig.param {
            BuiltinParam::Schematic { params, .. } => params.len(),
            _ => 0,
        };
        let Some(resolved) = crate::typeck::builtins::resolve_schematic_call(sig, &arg_types)
        else {
            self.add_error(
                TypeckErrorKind::ArityMismatch {
                    expected: expected_arity,
                    found: arg_types.len(),
                },
                func_span,
            );
            return Err(());
        };

        // A type variable bound to two different types by two arguments
        // (`std.arr.push(["a"], 1)` binds `T0` to `string` and then to
        // `int`) is an element-type mismatch.
        if let Some(conflict) = resolved.conflict {
            let span = typed_args[conflict.arg_index].span();
            self.add_error(
                TypeckErrorKind::ArrayTypeMismatch {
                    expected: conflict.expected,
                    found: conflict.found,
                },
                span,
            );
            return Err(());
        }

        // Every argument must satisfy its *substituted* parameter type.
        for (i, (typed_arg, expected_ty)) in typed_args
            .iter()
            .zip(resolved.param_types.iter())
            .enumerate()
        {
            if let Err(e) =
                Self::unify_arg_ref_aware(expected_ty, &typed_arg.ty(), i, typed_arg.span())
            {
                self.add_error(e.kind, e.span);
                return Err(());
            }
        }

        // The callee node carries the *substituted* function type: a
        // `Type::Var` here would leak into TypedHir.
        let ret_type = resolved.ret_type;
        let typed_func = TypedExprHir::Ident {
            name,
            binding_id,
            ty: Type::Func(resolved.param_types, Box::new(ret_type.clone())),
            access: Access::Copy,
            hir_id,
            span: func_span,
        };

        Ok(TypedExprHir::Call(
            Box::new(typed_func),
            typed_args,
            ret_type,
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

    /// Check an `e await` expression (0.0.4 F2/F3): a `Future<T>` is driven
    /// to completion and yields `T`. `await` on a non-Future is
    /// `AwaitNonFuture`. Unlike `?`, `await` has no enclosing-return-type
    /// legality condition — a synchronous caller may drive a future too
    /// (PLAN §3.2).
    fn typeck_await(&mut self, operand: ExprHir, span: Span) -> Result<TypedExprHir, ()> {
        let typed_operand = self.typeck_expr(operand, None)?;
        match typed_operand.ty() {
            Type::Future(inner) => Ok(TypedExprHir::Await {
                operand: Box::new(typed_operand),
                ty: *inner,
                span,
            }),
            other => {
                self.add_error(TypeckErrorKind::AwaitNonFuture { found: other }, span);
                Err(())
            }
        }
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
            && !self.global_types.contains_key(name)
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
        let move_path = hir_field_path(&inner);
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
                if self.global_ids.contains(&binding_id) || self.global_types.contains_key(&name) {
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
            ExprHir::Field(_, _) => {
                // D2: structs own their fields exclusively — no partial
                // moves out of a field, not even one level deep.
                self.add_error(TypeckErrorKind::MoveOutOfStruct { path: move_path }, span);
                Err(())
            }
            ExprHir::Index(_, _, _) => {
                // D3: arrays own their elements exclusively — no moves out
                // of an element, not even one level deep.
                self.add_error(TypeckErrorKind::MoveOutOfArray { path: move_path }, span);
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
                self.check_ref_in_async(&ty, binding_id, ident_span);
                // `ref T` clones to an owned `T` (the value leaves the
                // borrow); every other type keeps its declared type.
                let value_ty = match ty {
                    Type::Ref(inner_ty) => *inner_ty,
                    other => other,
                };
                // 0.0.4 T07d: channels are move-only — `clone ch` is rejected.
                // A deep copy of a shared buffer is meaningless, and a shallow
                // one would alias while typeck believed it copied;
                // `std.concurrent.split(ch)` is the one explicit fork point.
                // The check sees through value-carrying containers: `clone r`
                // on a `Result<channel<_>, _>` would unwrap two handles to
                // one buffer.
                if value_ty.contains_channel() {
                    self.add_error(TypeckErrorKind::ChannelNotClonable, ident_span);
                    return Err(());
                }
                // 0.0.4 T08: a Future is move-only too — a Value-level copy
                // deep-copies the whole suspended coroutine frame, forking the
                // task (`clone f` would run it twice). There is no fork point
                // for futures: `await` drives it, `std.async.run` takes it.
                // The rule sees through nesting: `clone ts` on a `[Future]`
                // copies every frame in the array, and `clone r` on a
                // `Result<Future, _>` (or `Result<[Future], _>`) copies the
                // frames inside the payload the derived `Clone` deep-copies.
                if value_ty.contains_future() {
                    self.add_error(TypeckErrorKind::FutureNotClonable, ident_span);
                    return Err(());
                }
                // 0.0.5 T02 D1/D2: a pointer is move-only — any Value-level
                // copy of a pointer hands out aliasing access in safe code,
                // which would break D2's "the pointer-exists state sits
                // behind `unsafe`". The rule sees through value-carrying
                // containers: `clone r` on a `Result<ptr<_>, _>` would copy
                // the pointer inside the payload the derived `Clone`
                // deep-copies.
                if value_ty.contains_ptr() {
                    self.add_error(TypeckErrorKind::CloneOfPointer, ident_span);
                    return Err(());
                }
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
            // 0.0.3 V02 (D2): `clone p.f` copies the field out — and a field
            // *read* is already a copy (GetField deep-copies), so the clone
            // form lowers to the plain field read.
            ExprHir::Field(obj, field) => self.typeck_field(*obj, field),
            // 0.0.3 V03 (D3): `clone a[i]` copies the element out — and an
            // element *read* is already a copy (ArrGet deep-copies), so the
            // clone form lowers to the plain element read.
            ExprHir::Index(arr, idx, span) => self.typeck_index(*arr, *idx, span),
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
        // 0.0.4 T08: a box cell is a storage position read by deep copy
        // (`deref`), which would fork a suspended Future frame or alias a
        // channel buffer without `split` — the same rule as struct fields.
        // The check sees through value-carrying containers: `deref` on a
        // `box<[Future]>` copies every frame in the array, and on a
        // `box<Result<Future, _>>` the frames inside the payload.
        if typed_inner.ty().contains_channel() {
            self.add_error(TypeckErrorKind::ChannelNotStorable, typed_inner.span());
            return Err(());
        }
        if typed_inner.ty().contains_future() {
            self.add_error(TypeckErrorKind::FutureNotStorable, typed_inner.span());
            return Err(());
        }
        // 0.0.5 T02 D1/D2: a box cell is read by deep copy (`deref`), which
        // would hand out the pointer in safe code — pointers are
        // non-storable (same rule as struct fields).
        if typed_inner.ty().contains_ptr() {
            self.add_error(TypeckErrorKind::PtrNotStorable, typed_inner.span());
            return Err(());
        }
        Ok(TypedExprHir::Box(Box::new(typed_inner), span))
    }

    /// 0.0.2 U05: type check `deref <box>` (PLAN §3.2.1).
    ///
    /// Reads the pointee as a copy; the box itself is untouched. The operand
    /// must be a `box<T>` *expression*: a bare binding (the box stays in its
    /// slot, so the load is a plain read) or any box-typed chain such as
    /// `w.payload` (0.0.3 V02), whose value is already a fresh copy.
    fn typeck_deref(&mut self, inner: ExprHir, span: Span) -> Result<TypedExprHir, ()> {
        let typed_inner = self.typeck_expr(inner, None)?;
        if !matches!(typed_inner.ty(), Type::Box(_)) {
            self.add_error(
                TypeckErrorKind::InvalidOperand {
                    op: "deref".to_string(),
                    ty: typed_inner.ty().clone(),
                },
                typed_inner.span(),
            );
            return Err(());
        }
        Ok(TypedExprHir::Deref(Box::new(typed_inner), span))
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

        if self.global_ids.contains(&binding_id) || self.global_types.contains_key(&name) {
            // 0.0.4 T03: this subsumes `AsyncGlobalWrite` — the write is
            // illegal in every context, so one specific error is enough.
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

    /// 0.0.5 T02 (D2): type check `addr <place>`.
    ///
    /// The place shape (one-level binding/field/element/box interior) and
    /// the referent's mutability/borrowedness were validated by the
    /// resolver; here the place is typed and wrapped in `ptr<T>`. The
    /// unsafe-context gating (D3, "only unsafe/trusted bodies may run this")
    /// is T03's job — this pass only establishes the type rules.
    ///
    /// `addr` is a non-consuming read: the referent's slot is untouched and
    /// stays usable afterwards (the ownership pass records a plain read).
    fn typeck_addr(
        &mut self,
        place: ExprHir,
        hir_id: HirId,
        span: Span,
    ) -> Result<TypedExprHir, ()> {
        // 0.0.5 T03: unsafe-context check; addr is an unsafe operation.
        if !self.can_use_unsafe() {
            self.add_error(TypeckErrorKind::UnsafeOpOutsideUnsafe, span);
            return Err(());
        }
        let typed_place = self.typeck_expr(place, None)?;
        // A global place would be read by deep copy everywhere else, but
        // `addr` captures the slot itself, so globals are legal (PLAN §3.3).
        let pointee = typed_place.ty();
        Ok(TypedExprHir::Addr {
            place: Box::new(typed_place),
            ty: Type::Ptr(Box::new(pointee)),
            hir_id,
            span,
        })
    }

    /// 0.0.5 T02: type check `deref_ptr <p>` — read through a `ptr<T>`.
    ///
    /// Reads the pointee as a copy; the pointer itself is untouched (the
    /// operand access is non-consuming, like `deref` on a box). The operand
    /// must be a `ptr<T>` expression. The unsafe-context gating is T03's.
    fn typeck_deref_ptr(&mut self, inner: ExprHir, span: Span) -> Result<TypedExprHir, ()> {
        // 0.0.5 T03: unsafe-context check; deref_ptr read is an unsafe operation.
        if !self.can_use_unsafe() {
            self.add_error(TypeckErrorKind::UnsafeOpOutsideUnsafe, span);
            return Err(());
        }
        let typed_inner = self.typeck_expr(inner, None)?;
        if !matches!(typed_inner.ty(), Type::Ptr(_)) {
            self.add_error(
                TypeckErrorKind::InvalidOperand {
                    op: "deref_ptr".to_string(),
                    ty: typed_inner.ty().clone(),
                },
                typed_inner.span(),
            );
            return Err(());
        }
        Ok(TypedExprHir::DerefPtr(Box::new(typed_inner), span))
    }

    /// 0.0.5 T02: type check `deref_ptr <p> = <expr>` — write through a
    /// `ptr<T>`.
    ///
    /// Writing through the pointer does not rebind `p`, so immutability of
    /// `p` is not required (same rule as `deref b = v`). A global pointer
    /// binding cannot exist (`PtrNotStorable` seals the declaration), so no
    /// global-write guard is reachable here — unlike the box form, whose
    /// `DerefAssignOfGlobalBox` covers storable global boxes. The
    /// unsafe-context gating is T03's.
    fn typeck_assign_deref_ptr(
        &mut self,
        name: String,
        binding_id: BindingId,
        rhs: ExprHir,
        hir_id: HirId,
        span: Span,
    ) -> Result<TypedExprHir, ()> {
        // 0.0.5 T03: unsafe-context check; deref_ptr write is an unsafe operation.
        if !self.can_use_unsafe() {
            self.add_error(TypeckErrorKind::UnsafeOpOutsideUnsafe, span);
            return Err(());
        }
        let target_ty = self.lookup_type(&name, span)?;
        let pointee_ty = match target_ty {
            Type::Ptr(pointee) => *pointee,
            other => {
                self.add_error(
                    TypeckErrorKind::InvalidOperand {
                        op: "deref_ptr =".to_string(),
                        ty: other,
                    },
                    span,
                );
                return Err(());
            }
        };

        let typed_rhs = self.typeck_expr(rhs, None)?;
        if let Err(e) = unify_assign(&pointee_ty, &typed_rhs.ty(), typed_rhs.span()) {
            self.add_error(e.kind, e.span);
            return Err(());
        }
        self.reject_bare_owned_arg(&typed_rhs, typed_rhs.span())?;

        Ok(TypedExprHir::AssignDerefPtr {
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

    /// Type check an index expression `a[i]` (0.0.3 V03).
    ///
    /// The subscript must be `int` (`IndexNotInt`); the indexed value must
    /// be an array (a `ref [T]` parameter reads as `[T]`; a `box<[T]>` must
    /// be read with `deref` first). The result is a bounds-checked **copy**
    /// of the element — deep for owned element types (D3) — so the read
    /// never disturbs the array.
    fn typeck_index(&mut self, arr: ExprHir, idx: ExprHir, span: Span) -> Result<TypedExprHir, ()> {
        let typed_arr = self.typeck_expr(arr, None)?;
        let typed_idx = self.typeck_expr(idx, None)?;

        if !matches!(typed_idx.ty(), Type::Int) {
            self.add_error(
                TypeckErrorKind::IndexNotInt {
                    found: typed_idx.ty(),
                },
                typed_idx.span(),
            );
            return Err(());
        }

        let arr_ty = typed_arr.ty();
        let target: &Type = match &arr_ty {
            Type::Ref(inner) => inner,
            ty => ty,
        };
        let elem_ty = match target {
            Type::Array(el) => (**el).clone(),
            Type::Box(_) => {
                self.add_error(TypeckErrorKind::IndexOnBox, typed_arr.span());
                return Err(());
            }
            other => {
                self.add_error(
                    TypeckErrorKind::InvalidOperand {
                        op: "index".to_string(),
                        ty: other.clone(),
                    },
                    typed_arr.span(),
                );
                return Err(());
            }
        };

        // 0.0.4 T08: an element read is a deep copy, which would fork a
        // suspended Future frame. Arrays of futures exist for one consumer —
        // `std.async.run` moves the frames out; elements are never read.
        // (`clone a[i]` delegates here, so the clone spelling is covered too.)
        // The check sees through nesting: `tss[0]` on `[[Future]]` copies a
        // whole array of frames.
        if elem_ty.contains_future() {
            self.add_error(TypeckErrorKind::FutureIndexRead, span);
            return Err(());
        }

        // Span covers the whole `a[i]` chain — it is what the runtime
        // `IndexOutOfRange` error points at. The span is threaded from the
        // AST (`arr[idx]`, including the closing `]`) rather than derived
        // here, so it stays accurate even when the index expression has
        // surrounding whitespace.
        Ok(TypedExprHir::Index {
            arr: Box::new(typed_arr),
            idx: Box::new(typed_idx),
            ty: elem_ty,
            span,
        })
    }

    /// Type check an array literal `[e1, e2, ...]` (0.0.3 V03).
    ///
    /// Element types are uniform (0.0.1 equality rule): the element type is
    /// the annotation's element type when one flows in (so `x: [int] = [];`
    /// types the empty literal, and `[Ok(1), Err("e")]` infers payloads from
    /// a `Result` element annotation), otherwise the first element's type.
    /// Later elements must match it exactly → `ArrayTypeMismatch`. Each
    /// element is a consuming position.
    fn typeck_array_lit(
        &mut self,
        elems: Vec<ExprHir>,
        expected: Option<&Type>,
        span: Span,
    ) -> Result<TypedExprHir, ()> {
        let hint_elem: Option<Type> = match expected {
            Some(Type::Array(el)) => Some((**el).clone()),
            _ => None,
        };

        if elems.is_empty() {
            let Some(elem) = hint_elem else {
                self.add_error(TypeckErrorKind::CannotInferArrayType, span);
                return Err(());
            };
            // 0.0.4 T07d: channels are non-storable — a channel (or a
            // value-carrying container of one, whose copied payload would
            // alias the shared buffer) has no wholesale array consumer, so
            // the element type is rejected here; this is the construction
            // seal that makes the element-read routes unreachable.
            if elem.contains_channel() {
                self.add_error(TypeckErrorKind::ChannelNotStorable, span);
                return Err(());
            }
            // 0.0.5 T02 D1/D2: pointers are non-storable — an element read
            // deep-copies, which would hand out the pointer in safe code.
            if elem.contains_ptr() {
                self.add_error(TypeckErrorKind::PtrNotStorable, span);
                return Err(());
            }
            return Ok(TypedExprHir::ArrayLit {
                elems: Vec::new(),
                ty: Type::Array(Box::new(elem)),
                span,
            });
        }

        let mut elem_ty = hint_elem;
        let mut typed_elems = Vec::with_capacity(elems.len());
        for e in elems {
            let typed_e = self.typeck_expr(e, elem_ty.as_ref())?;
            match &elem_ty {
                Some(exp) => {
                    if *exp != typed_e.ty() {
                        self.add_error(
                            TypeckErrorKind::ArrayTypeMismatch {
                                expected: exp.clone(),
                                found: typed_e.ty(),
                            },
                            typed_e.span(),
                        );
                        return Err(());
                    }
                }
                None => elem_ty = Some(typed_e.ty()),
            }
            self.reject_bare_owned_arg(&typed_e, typed_e.span())?;
            typed_elems.push(typed_e);
        }

        // SAFETY: non-empty elems either took the hint or set `elem_ty`
        // from the first element above, so it cannot be `None` here.
        let elem = elem_ty.expect("element type was established");
        // 0.0.4 T07d: channels are non-storable — an element read deep-copies,
        // which would fork the shared buffer silently. The same holds for a
        // container of channels (the copied payload aliases the buffer), so
        // the element type is rejected here — the construction seal that
        // makes the element-read routes unreachable.
        if elem.contains_channel() {
            self.add_error(TypeckErrorKind::ChannelNotStorable, span);
            return Err(());
        }
        // 0.0.5 T02 D1/D2: pointers are non-storable — an element read
        // deep-copies, which would hand out the pointer in safe code.
        if elem.contains_ptr() {
            self.add_error(TypeckErrorKind::PtrNotStorable, span);
            return Err(());
        }
        let ty = Type::Array(Box::new(elem));
        Ok(TypedExprHir::ArrayLit {
            elems: typed_elems,
            ty,
            span,
        })
    }

    /// Type check an index assignment `a[i] = expr` (0.0.3 V03, D3).
    ///
    /// The target must be an array binding (local, global, or by-value
    /// parameter — the resolver rejects `ref` parameters and non-bare
    /// receivers). Writing an element does not rebind `a`, so immutability
    /// of `a` is not required (same rule as `p.f = e`); bounds are checked
    /// at runtime. The RHS is a consuming position.
    fn typeck_assign_index(
        &mut self,
        name: String,
        binding_id: BindingId,
        index: ExprHir,
        rhs: ExprHir,
        hir_id: HirId,
        span: Span,
    ) -> Result<TypedExprHir, ()> {
        let obj_ty = self.lookup_type(&name, span)?;
        let elem_ty = match &obj_ty {
            Type::Array(el) => (**el).clone(),
            Type::Box(_) => {
                self.add_error(TypeckErrorKind::IndexOnBox, span);
                return Err(());
            }
            other => {
                self.add_error(
                    TypeckErrorKind::InvalidOperand {
                        op: "index assignment".to_string(),
                        ty: other.clone(),
                    },
                    span,
                );
                return Err(());
            }
        };

        let typed_idx = self.typeck_expr(index, None)?;
        if !matches!(typed_idx.ty(), Type::Int) {
            self.add_error(
                TypeckErrorKind::IndexNotInt {
                    found: typed_idx.ty(),
                },
                typed_idx.span(),
            );
            return Err(());
        }

        // 0.0.4 T03 (D5): same rule for element writes on a global array.
        if self.in_async_context() && self.global_ids.contains(&binding_id) {
            self.add_error(
                TypeckErrorKind::AsyncGlobalWrite { name: name.clone() },
                span,
            );
            return Err(());
        }

        let typed_rhs = self.typeck_expr(rhs, Some(&elem_ty))?;
        if let Err(e) = unify_assign(&elem_ty, &typed_rhs.ty(), typed_rhs.span()) {
            self.add_error(e.kind, e.span);
            return Err(());
        }
        self.reject_bare_owned_arg(&typed_rhs, typed_rhs.span())?;

        Ok(TypedExprHir::AssignIndex {
            name,
            binding_id,
            index: Box::new(typed_idx),
            rhs: Box::new(typed_rhs),
            ty: Type::Unit,
            hir_id,
            span,
        })
    }

    /// Type check a field access `obj.field` (0.0.3 V02).
    ///
    /// The object must be a struct value (a `ref` parameter forwards to its
    /// pointee struct: a read through a borrow is a copy, D2). A `box` must
    /// be read with `deref` first. The result is a copy of the field — deep
    /// for owned fields — which is why reads never disturb the object.
    fn typeck_field(&mut self, obj: ExprHir, field: String) -> Result<TypedExprHir, ()> {
        let typed_obj = self.typeck_expr(obj, None)?;
        let obj_ty = typed_obj.ty();

        // Resolve the struct whose field is being read: the object's own
        // type, or (through a borrow) the pointee type. Normalizing `ref T`
        // to `T` up front keeps the box/other rejections in one place.
        let target: &Type = match &obj_ty {
            Type::Ref(inner) => inner,
            ty => ty,
        };
        // 0.0.4 T07a: `c.send` / `c.recv` used as a *value* (not called) on a
        // channel — the members exist only in call position.
        if let Type::Channel(_) = target
            && (field == "send" || field == "recv")
        {
            self.add_error(
                TypeckErrorKind::ChannelMemberNotCalled { member: field },
                typed_obj.span(),
            );
            return Err(());
        }
        let info = match target {
            Type::Struct(info) => (**info).clone(),
            Type::Box(_) => {
                // A box must be read with `deref` first (also covers
                // `ref box<T>` parameters, which the resolver rejects).
                self.add_error(TypeckErrorKind::FieldOnBox, typed_obj.span());
                return Err(());
            }
            other => {
                self.add_error(
                    TypeckErrorKind::InvalidOperand {
                        op: "field access".to_string(),
                        ty: other.clone(),
                    },
                    typed_obj.span(),
                );
                return Err(());
            }
        };
        let entry = &self.structs[info.id.0 as usize];
        let Some(idx) = entry.field_index(&field) else {
            let struct_name = info.name.to_string();
            let obj_span = typed_obj.span();
            self.add_error(
                TypeckErrorKind::UnknownField { struct_name, field },
                obj_span,
            );
            return Err(());
        };
        let field_ty = entry.fields[idx].1.clone();
        let obj_span = typed_obj.span();
        Ok(TypedExprHir::Field {
            obj: Box::new(typed_obj),
            field_idx: idx as u16,
            ty: field_ty,
            span: obj_span,
        })
    }

    /// Type check a struct literal `Name { field: expr, ... }` (0.0.3 V02).
    ///
    /// The field set must match the declaration exactly (missing →
    /// `MissingField`, unknown → `UnknownField`; duplicates are already
    /// rejected by the parser). Field types unify exactly (0.0.1 rule) and
    /// each initializer is a consuming position (bare owned values need
    /// `move` / `clone`). Initializers keep source order, paired with their
    /// declaration-order index for lowering's reorder.
    fn typeck_struct_lit(
        &mut self,
        name: String,
        inits: Vec<(String, ExprHir)>,
        hir_id: HirId,
        span: Span,
    ) -> Result<TypedExprHir, ()> {
        let Some(&sid) = self.struct_by_name.get(&name) else {
            self.add_error(TypeckErrorKind::UnknownStruct { name }, span);
            return Err(());
        };

        let declared: Vec<(String, Type)> = self.structs[sid.0 as usize]
            .fields
            .iter()
            .map(|(n, ty, _)| (n.clone(), ty.clone()))
            .collect();
        let struct_ty = Type::Struct(Box::new(StructInfo {
            id: sid,
            name: name.clone().into_boxed_str(),
            copy: self.structs[sid.0 as usize].copy,
        }));

        // No unknown fields.
        for (fname, _) in &inits {
            if !declared.iter().any(|(n, _)| n == fname) {
                self.add_error(
                    TypeckErrorKind::UnknownField {
                        struct_name: name.clone(),
                        field: fname.clone(),
                    },
                    span,
                );
                return Err(());
            }
        }
        // Every declared field present exactly once (the parser rejects
        // duplicates; a >1 count here would be an invariant break).
        for (dn, _) in &declared {
            let count = inits.iter().filter(|(n, _)| n == dn).count();
            if count == 0 {
                self.add_error(
                    TypeckErrorKind::MissingField {
                        struct_name: name.clone(),
                        field: dn.clone(),
                    },
                    span,
                );
                return Err(());
            }
            debug_assert_eq!(count, 1, "parser rejects duplicate fields in literals");
        }

        // Type each initializer against its declared field type.
        let mut typed_inits = Vec::with_capacity(inits.len());
        for (fname, fexpr) in inits {
            let idx = declared.iter().position(|(n, _)| *n == fname);
            let Some(idx) = idx else {
                // Unreachable: unknown fields were rejected above.
                unreachable!("field `{fname}` was validated against the declaration")
            };
            let fty = declared[idx].1.clone();
            let typed = self.typeck_expr(fexpr, Some(&fty))?;
            if let Err(e) = unify(&fty, &typed.ty(), typed.span()) {
                self.add_error(e.kind, e.span);
                return Err(());
            }
            self.reject_bare_owned_arg(&typed, typed.span())?;
            typed_inits.push((idx as u16, typed));
        }

        Ok(TypedExprHir::StructLit {
            struct_id: sid,
            inits: typed_inits,
            ty: struct_ty,
            hir_id,
            span,
        })
    }

    /// Type check `channel<T>(capacity)` constructor (0.0.4 T07).
    ///
    /// The capacity expression must be an `int`. The element type `T` is
    /// taken from the type annotation in the constructor and must be a valid
    /// type (not a type variable, not a `ref` type). The result is a
    /// non-Copy, move-only `channel<T>` value.
    fn typeck_channel_new(
        &mut self,
        ty: Type,
        capacity: ExprHir,
        _hir_id: HirId,
        span: Span,
    ) -> Result<TypedExprHir, ()> {
        // Validate the payload type: must be a concrete type, no type vars.
        // Reject `ref T` inside channel payloads (would violate D4).
        if let Type::Ref(_) = &ty {
            self.add_error(
                TypeckErrorKind::InvalidOperand {
                    op: "channel payload".to_string(),
                    ty: ty.clone(),
                },
                span,
            );
            return Err(());
        }
        if matches!(&ty, Type::Var(_)) {
            self.add_error(
                TypeckErrorKind::UnsupportedFeature {
                    feature: "type variable in channel payload".to_string(),
                },
                span,
            );
            return Err(());
        }

        // Type-check capacity expression (must be int).
        let typed_cap = self.typeck_expr(capacity, Some(&Type::Int))?;
        if !matches!(typed_cap.ty(), Type::Int) {
            self.add_error(
                TypeckErrorKind::TypeMismatch {
                    expected: Type::Int,
                    found: typed_cap.ty(),
                },
                typed_cap.span(),
            );
            return Err(());
        }

        Ok(TypedExprHir::ChannelNew {
            capacity: Box::new(typed_cap),
            payload_ty: ty,
            span,
        })
    }

    /// Type check a field assignment `p.f = expr` (0.0.3 V02, D2).
    ///
    /// The target must be a struct binding (local, global, or by-value
    /// parameter — the resolver rejects `ref` parameters). Writing a field
    /// does not rebind `p`, so immutability of `p` is not required (same
    /// rule as `deref b = v`). The RHS is a consuming position.
    fn typeck_assign_field(
        &mut self,
        place: FieldPlace,
        rhs: ExprHir,
        hir_id: HirId,
        span: Span,
    ) -> Result<TypedExprHir, ()> {
        let FieldPlace {
            obj_name,
            binding_id: obj_binding_id,
            field,
        } = place;
        let obj_ty = self.lookup_type(&obj_name, span)?;
        let info = match obj_ty {
            Type::Struct(info) => info,
            other => {
                self.add_error(
                    TypeckErrorKind::InvalidOperand {
                        op: "field assignment".to_string(),
                        ty: other,
                    },
                    span,
                );
                return Err(());
            }
        };

        let entry = &self.structs[info.id.0 as usize];
        let Some(idx) = entry.field_index(&field) else {
            let struct_name = info.name.to_string();
            self.add_error(TypeckErrorKind::UnknownField { struct_name, field }, span);
            return Err(());
        };
        let field_ty = entry.fields[idx].1.clone();

        // 0.0.4 T03 (D5): a field write mutates the global's storage in
        // place. `global_ids` is exact — a local shadow has its own id.
        if self.in_async_context() && self.global_ids.contains(&obj_binding_id) {
            self.add_error(
                TypeckErrorKind::AsyncGlobalWrite {
                    name: obj_name.clone(),
                },
                span,
            );
            return Err(());
        }

        let typed_rhs = self.typeck_expr(rhs, Some(&field_ty))?;
        if let Err(e) = unify_assign(&field_ty, &typed_rhs.ty(), typed_rhs.span()) {
            self.add_error(e.kind, e.span);
            return Err(());
        }
        self.reject_bare_owned_arg(&typed_rhs, typed_rhs.span())?;

        Ok(TypedExprHir::AssignField {
            obj_name,
            obj_binding_id,
            field_idx: idx as u16,
            rhs: Box::new(typed_rhs),
            ty: Type::Unit,
            hir_id,
            span,
        })
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
        let target_ty = self.convert_type(&target, span);

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
    ///
    /// Order matters: the **innermost scope wins**, then function signatures
    /// (a bare function/builtin identifier used as a value), then imported
    /// globals. The resolver resolved this ident against its own scopes — a
    /// parameter or local shadowing a function name must keep the local's
    /// type here too. Checking `func_signatures` first instead made any
    /// local/param that shares a name with a *user function* resolve to the
    /// function's type — including across modules, where the program-wide
    /// signature map sees the root crate's bare names while the std module's
    /// `min(a, b)`-style parameters are perfectly legal locals
    /// (`std.num.min`'s `a < b` broke under a user `async func b`).
    fn lookup_type(&mut self, name: &str, span: Span) -> Result<Type, ()> {
        // First check the scope chain (innermost binding shadows everything).
        if let Some(binding) = self.scopes.get(name) {
            // Look up the type from our binding_types map
            if let Some(ty) = self.binding_types.get(&binding.id) {
                return Ok(ty.clone());
            }
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
            return Err(());
        }

        // Then check function signatures
        if let Some((params, ret)) = self.func_signatures.get(name) {
            return Ok(Type::Func(params.clone(), Box::new(ret.clone())));
        }

        // Fall back to globals collected in a first pass so that
        // imported globals (e.g. `net.answer`) resolve by their
        // mangled name. 0.0.3 V05.
        if let Some(ty) = self.global_types.get(name) {
            return Ok(ty.clone());
        }
        self.add_error(
            TypeckErrorKind::UndefinedVariable {
                name: name.to_string(),
            },
            span,
        );
        Err(())
    }
}

/// Render a field/index-access chain as a source-shaped path (`o.name`,
/// `w.payload`, `a[i]`) for error messages; other shapes degrade to
/// `<expr>`.
fn hir_field_path(expr: &ExprHir) -> String {
    match expr {
        ExprHir::Field(obj, field) => format!("{}.{}", hir_field_path(obj), field),
        ExprHir::Index(arr, idx, _) => format!("{}[{}]", hir_field_path(arr), hir_field_path(idx)),
        ExprHir::Ident { name, .. } => name.clone(),
        // Literal subscripts render as themselves so `move s[0]` reads as
        // `s[0]`, not `s[<expr>]`.
        ExprHir::Int(n, _) => n.to_string(),
        ExprHir::Float(f, _) => f.to_string(),
        ExprHir::Bool(b, _) => b.to_string(),
        ExprHir::Str(s, _) => format!("\"{s}\""),
        _ => "<expr>".to_string(),
    }
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}
