//! Core interpreter loop.

use fleen_compiler::codegen::{Const, FuncId, Module, Opcode};

use crate::error::{RuntimeError, RuntimeErrorKind};
use crate::fmt::fmt_scalar;
use crate::frame::CallFrame;
use crate::value::Value;

const MAX_CALL_DEPTH: usize = 1024;

/// The virtual machine.
///
/// Executes the bytecode produced by `fleen-compiler::codegen`. Builtin
/// functions run host-side. A global whose descriptor has `mutable ==
/// false` may only be written while the currently executing function is
/// named `__init__` (the module initializer); writes elsewhere raise
/// [`RuntimeErrorKind::ImmutableGlobal`].
pub struct Vm {
    module: Module,
    stack: Vec<Value>,
    frames: Vec<CallFrame>,
    globals: Vec<Value>,
    /// The host program's command-line arguments (0.0.3 V06), served to
    /// Fleen by `std.os.args` — without argv[0].
    program_args: Vec<String>,
}

impl Vm {
    /// Create a VM ready to execute `module`.
    ///
    /// All globals are initialized to [`Value::Unit`]; the module's
    /// `__init__` function is expected to overwrite them.
    pub fn new(module: Module) -> Self {
        Self::with_args(module, Vec::new())
    }

    /// Create a VM with the program's command-line arguments (0.0.3 V06),
    /// exposed to Fleen through `std.os.args` (without argv[0]).
    pub fn with_args(module: Module, program_args: Vec<String>) -> Self {
        let n_globals = module.globals.len();
        Vm {
            module,
            stack: Vec::new(),
            frames: Vec::new(),
            globals: vec![Value::Unit; n_globals],
            program_args,
        }
    }

    /// Run the module's `entry` function to completion.
    ///
    /// Accepts bytecode versions 1 (0.0.1), 2 (0.0.2) and 3 (0.0.3). On
    /// failure the [`RuntimeError`] carries the source location of the
    /// failing instruction when one can be determined (0.0.2 U10).
    pub fn run(&mut self) -> Result<Value, RuntimeError> {
        self.run_inner().map_err(|kind| self.locate_error(kind))
    }

    fn run_inner(&mut self) -> Result<Value, RuntimeErrorKind> {
        if !(1..=3).contains(&self.module.version) {
            return Err(RuntimeErrorKind::UnsupportedVersion(self.module.version));
        }
        let entry = self.module.entry;
        self.call(entry, 0)?;
        // No globals/`__init__` path pushes extra frames; run until empty.
        // A top-level `Return` leaves its value on the stack (the `Return`
        // dispatch pushes it back), so the pop below yields the module
        // result.
        loop {
            if self.frames.is_empty() {
                // The entry frame starts at stack base 0 and `Return`
                // truncates to the frame's base before pushing its value
                // back, so after unwinding nothing but the module result
                // (if any) can remain.
                debug_assert!(
                    self.stack.len() <= 1,
                    "stray operands left on the stack after unwinding"
                );
                return Ok(self.stack.pop().unwrap_or(Value::Unit));
            }
            self.step()?;
        }
    }

    /// Attach source location to a runtime error (0.0.2 U10).
    ///
    /// The top frame is always where the error originated: `step()` only
    /// writes `frame.ip` back on the success path, so an in-flight `ip`
    /// still names the failing instruction. `Call`/`CallValue` advance the
    /// caller's ip before invoking `call` (the callee runs from the main
    /// loop once the frame is pushed) but rewind it on failure — `call`
    /// errors happen before the new frame is pushed — so the ip names the
    /// `Call` itself. The span is the closest `span_map` entry at or before
    /// that ip — the failing instruction may itself be synthetic and carry
    /// no entry. Every lookup is defensive (`None` on miss): location is
    /// best-effort, never a second failure.
    fn locate_error(&self, kind: RuntimeErrorKind) -> RuntimeError {
        let mut err = RuntimeError::new(kind);
        let Some(frame) = self.frames.last() else {
            return err;
        };
        err.pc = Some(frame.ip as u32);
        if let Some(f) = self.module.functions.get(frame.func.0 as usize) {
            let n = f.span_map.partition_point(|e| e.offset <= frame.ip as u32);
            if let Some(entry) = n.checked_sub(1).and_then(|i| f.span_map.get(i)) {
                err.span = Some((entry.start, entry.end));
            }
            if let Ok(name) = self.func_name(frame.func) {
                err.func = Some(Box::from(name));
            }
        }
        err
    }

    /// Name of a function (for builtin dispatch and `__init__` detection).
    fn func_name(&self, fid: FuncId) -> Result<&str, RuntimeErrorKind> {
        let f = self
            .module
            .functions
            .get(fid.0 as usize)
            .ok_or(RuntimeErrorKind::FuncOutOfRange(fid))?;
        match &self.module.constants[f.name.0 as usize] {
            Const::Str(s) => Ok(s),
            _ => Err(RuntimeErrorKind::StackShape),
        }
    }

    fn current_frame_is_init(&self) -> bool {
        match self.frames.last() {
            Some(f) => self
                .func_name(f.func)
                .map(|n| n == "__init__")
                .unwrap_or(false),
            None => false,
        }
    }

    /// Invoke `fid` with `argc` arguments already on the operand stack.
    /// Builtins execute host-side; normal functions push a new frame.
    fn call(&mut self, fid: FuncId, argc: u16) -> Result<(), RuntimeErrorKind> {
        let func = self
            .module
            .functions
            .get(fid.0 as usize)
            .ok_or(RuntimeErrorKind::FuncOutOfRange(fid))?
            .clone();
        if func.params != argc {
            return Err(RuntimeErrorKind::StackShape);
        }
        if func.is_builtin {
            let name = self.func_name(fid)?.to_string();
            let args: Vec<Value> = (0..argc)
                .map(|_| self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .rev()
                .collect();
            let ret = self.exec_builtin(&name, &args)?;
            self.stack.push(ret);
            return Ok(());
        }
        if self.frames.len() >= MAX_CALL_DEPTH {
            return Err(RuntimeErrorKind::CallDepthExceeded);
        }
        let base = self.stack.len() - argc as usize;
        // Reserve slots for locals beyond the parameters.
        if (func.locals as usize) < argc as usize {
            return Err(RuntimeErrorKind::StackShape);
        }
        self.stack.resize(base + func.locals as usize, Value::Unit);
        self.frames.push(CallFrame {
            func: fid,
            ip: 0,
            stack_base: base,
        });
        Ok(())
    }

    fn exec_builtin(&self, name: &str, args: &[Value]) -> Result<Value, RuntimeErrorKind> {
        match name {
            "print" => {
                // 0.0.2 U13: scalar rendering shares `fmt_scalar` with `ToStr`
                // so print and `as string` can never drift apart. Non-scalar,
                // non-string values still fall back to `Display` until U04
                // tightens the typeck side to `string`-only.
                let line: String = args
                    .iter()
                    .map(|a| {
                        // A borrowed argument (`ref T` parameter read, U09)
                        // arrives as a handle — dereference before formatting
                        // or it would print the opaque `<ref>`.
                        let v = self.read_ref(a)?;
                        Ok(match v {
                            Value::Str(s) => s.to_string(),
                            other => fmt_scalar(other).unwrap_or_else(|| other.to_string()),
                        })
                    })
                    .collect::<Result<Vec<_>, RuntimeErrorKind>>()?
                    .join("");
                println!("{line}");
                Ok(Value::Unit)
            }
            // 0.0.3 V06 intrinsics (D8): only what Fleen cannot express is
            // host-side — standard input/output, the OS process interface,
            // the clock, and the math/string primitives with no source
            // syntax. Signatures come from `typeck/builtins.rs`.
            "std.io.println" => {
                let line: String = args
                    .iter()
                    .map(|a| {
                        let v = self.read_ref(a)?;
                        Ok(match v {
                            Value::Str(s) => s.to_string(),
                            other => fmt_scalar(other).unwrap_or_else(|| other.to_string()),
                        })
                    })
                    .collect::<Result<Vec<_>, RuntimeErrorKind>>()?
                    .join("");
                println!("{line}");
                Ok(Value::Unit)
            }
            "std.io.read_line" => {
                let mut buf = String::new();
                match std::io::stdin().read_line(&mut buf) {
                    Ok(_) => {
                        // Strip the trailing newline (and a carriage return
                        // from CRLF input); EOF yields the empty string.
                        let line = buf.trim_end_matches('\n').trim_end_matches('\r');
                        Ok(Value::Str(line.into()))
                    }
                    Err(_) => Ok(Value::Str("".into())),
                }
            }
            "std.str.len" => match self.read_ref(&args[0])? {
                Value::Str(s) => Ok(Value::Int(s.chars().count() as i64)),
                _ => Err(RuntimeErrorKind::StackShape),
            },
            "std.str.concat" => match (self.read_ref(&args[0])?, self.read_ref(&args[1])?) {
                (Value::Str(a), Value::Str(b)) => {
                    let mut joined = String::with_capacity(a.len() + b.len());
                    joined.push_str(a);
                    joined.push_str(b);
                    Ok(Value::Str(joined.into()))
                }
                _ => Err(RuntimeErrorKind::StackShape),
            },
            "std.str.split" => match (self.read_ref(&args[0])?, self.read_ref(&args[1])?) {
                (Value::Str(s), Value::Str(sep)) => {
                    let parts: Vec<Value> = s
                        .split(sep.as_ref())
                        .map(|p| Value::Str(p.into()))
                        .collect();
                    Ok(Value::Arr(parts.into_boxed_slice()))
                }
                _ => Err(RuntimeErrorKind::StackShape),
            },
            "std.num.sqrt" => match self.read_ref(&args[0])? {
                Value::Float(x) => Ok(Value::Float(x.sqrt())),
                _ => Err(RuntimeErrorKind::StackShape),
            },
            "std.num.floor" => match self.read_ref(&args[0])? {
                Value::Float(x) => Ok(Value::Int(x.floor() as i64)),
                _ => Err(RuntimeErrorKind::StackShape),
            },
            "std.os.args" => {
                let args: Vec<Value> = self
                    .program_args
                    .iter()
                    .map(|a| Value::Str(a.as_str().into()))
                    .collect();
                Ok(Value::Arr(args.into_boxed_slice()))
            }
            "std.os.exit" => match &args[0] {
                Value::Int(code) => Err(RuntimeErrorKind::Exit(*code)),
                _ => Err(RuntimeErrorKind::StackShape),
            },
            "std.time.now_millis" => {
                match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                    Ok(d) => Ok(Value::Int(d.as_millis() as i64)),
                    Err(_) => Ok(Value::Int(0)),
                }
            }
            "std.arr.len" => match self.read_ref(&args[0])? {
                Value::Arr(elems) => Ok(Value::Int(elems.len() as i64)),
                _ => Err(RuntimeErrorKind::StackShape),
            },
            "std.arr.get" => match (self.read_ref(&args[0])?, &args[1]) {
                (Value::Arr(elems), Value::Int(i)) => {
                    // Out of bounds is a *value* (`Err`), not a crash (D3:
                    // the raw `a[i]` syntax keeps the runtime-error path).
                    match elems.get(*i as usize).filter(|_| *i >= 0) {
                        Some(v) => Ok(Value::Ok(Box::new(v.clone()))),
                        None => Ok(Value::Err(Box::new(Value::Str(
                            format!("index {i} out of range (array length {})", elems.len()).into(),
                        )))),
                    }
                }
                _ => Err(RuntimeErrorKind::StackShape),
            },
            "std.arr.set" => {
                let elems: Vec<Value> = match self.read_ref(&args[0])? {
                    Value::Arr(elems) => elems.to_vec(),
                    _ => return Err(RuntimeErrorKind::StackShape),
                };
                let (&Value::Int(i), v) = (&args[1], &args[2]) else {
                    return Err(RuntimeErrorKind::StackShape);
                };
                // Arrays are owned (D3): `set` returns a fresh array; the
                // negative/overflow case fails the index conversion the
                // same way as a too-large index.
                let idx = usize::try_from(i).map_err(|_| self.arr_index_error(i, elems.len()))?;
                let mut elems = elems;
                if idx >= elems.len() {
                    return Err(self.arr_index_error(i, elems.len()));
                }
                elems[idx] = v.clone();
                Ok(Value::Arr(elems.into_boxed_slice()))
            }
            "std.arr.push" => {
                let mut elems: Vec<Value> = match self.read_ref(&args[0])? {
                    Value::Arr(elems) => elems.to_vec(),
                    _ => return Err(RuntimeErrorKind::StackShape),
                };
                elems.push(args[1].clone());
                Ok(Value::Arr(elems.into_boxed_slice()))
            }
            _ => Err(RuntimeErrorKind::UnknownBuiltin(name.to_string())),
        }
    }

    /// The out-of-bounds runtime error for `std.arr.set`, matching the raw
    /// `a[i]` message of the `ArrSet*` instructions (0.0.3 V03).
    fn arr_index_error(&self, index: i64, len: usize) -> RuntimeErrorKind {
        RuntimeErrorKind::IndexOutOfRange { index, len }
    }

    fn step(&mut self) -> Result<(), RuntimeErrorKind> {
        let frame = *self.frames.last().ok_or(RuntimeErrorKind::StackUnderflow)?;
        let func = &self.module.functions[frame.func.0 as usize];
        let code = &func.code;
        if frame.ip >= code.len() {
            return Err(RuntimeErrorKind::BadJumpTarget);
        }
        let op = code[frame.ip];
        let opcode = Opcode::from_byte(op).ok_or(RuntimeErrorKind::InvalidOpcode(op))?;
        let mut ip = frame.ip + 1;

        // Read an operand of `n` bytes; defensive length checks.
        macro_rules! take {
            ($n:expr) => {{
                let n: usize = $n;
                if ip + n > code.len() {
                    return Err(RuntimeErrorKind::BadJumpTarget);
                }
                let s = &code[ip..ip + n];
                ip += n;
                s
            }};
        }
        // Like `take!` but does not advance `ip` (jumps overwrite it).
        macro_rules! take_untracked {
            ($n:expr) => {{
                let n: usize = $n;
                if ip + n > code.len() {
                    return Err(RuntimeErrorKind::BadJumpTarget);
                }
                &code[ip..ip + n]
            }};
        }
        match opcode {
            Opcode::Const => {
                let s = take!(4);
                let id = u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize;
                let v = match self.module.constants.get(id) {
                    Some(Const::Int(v)) => Value::Int(*v),
                    Some(Const::Float(v)) => Value::Float(*v),
                    Some(Const::Str(s)) => Value::Str(Box::from(s.as_ref())),
                    None => {
                        return Err(RuntimeErrorKind::ConstOutOfRange(
                            fleen_compiler::codegen::ConstId(id as u32),
                        ));
                    }
                };
                self.stack.push(v);
            }
            Opcode::True => self.stack.push(Value::Bool(true)),
            Opcode::False => self.stack.push(Value::Bool(false)),
            Opcode::Unit => self.stack.push(Value::Unit),
            Opcode::Pop => {
                self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
            }
            Opcode::Dup => {
                let v = self
                    .stack
                    .last()
                    .cloned()
                    .ok_or(RuntimeErrorKind::StackUnderflow)?;
                self.stack.push(v);
            }
            Opcode::LoadLocal => {
                let s = take!(2);
                let slot = u16::from_le_bytes([s[0], s[1]]) as usize;
                let idx = frame.stack_base + slot;
                let v = self
                    .stack
                    .get(idx)
                    .cloned()
                    .ok_or(RuntimeErrorKind::StackShape)?;
                self.stack.push(v);
            }
            Opcode::StoreLocal => {
                let s = take!(2);
                let slot = u16::from_le_bytes([s[0], s[1]]) as usize;
                let v = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                let idx = frame.stack_base + slot;
                if idx >= self.stack.len() {
                    return Err(RuntimeErrorKind::StackShape);
                }
                self.stack[idx] = v;
            }
            Opcode::LoadGlobal => {
                let s = take!(2);
                let id = u16::from_le_bytes([s[0], s[1]]) as usize;
                let v = self
                    .globals
                    .get(id)
                    .cloned()
                    .ok_or(RuntimeErrorKind::GlobalOutOfRange(
                        fleen_compiler::codegen::GlobalId(id as u32),
                    ))?;
                self.stack.push(v);
            }
            Opcode::StoreGlobal => {
                let s = take!(2);
                let id = u16::from_le_bytes([s[0], s[1]]) as usize;
                let g = self
                    .module
                    .globals
                    .get(id)
                    .ok_or(RuntimeErrorKind::GlobalOutOfRange(
                        fleen_compiler::codegen::GlobalId(id as u32),
                    ))?;
                if !g.mutable && !self.current_frame_is_init() {
                    return Err(RuntimeErrorKind::ImmutableGlobal(
                        fleen_compiler::codegen::GlobalId(id as u32),
                    ));
                }
                let v = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                self.globals[id] = v;
            }
            Opcode::IAdd => self.int_binop(|a, b| a.checked_add(*b))?,
            Opcode::ISub => self.int_binop(|a, b| a.checked_sub(*b))?,
            Opcode::IMul => self.int_binop(|a, b| a.checked_mul(*b))?,
            Opcode::IDiv => {
                let (a, b) = self.pop_int_pair()?;
                if b == 0 {
                    return Err(RuntimeErrorKind::DivisionByZero);
                }
                let v = a
                    .checked_div(b)
                    .ok_or(RuntimeErrorKind::ArithmeticOverflow)?;
                self.stack.push(Value::Int(v));
            }
            Opcode::IMod => {
                let (a, b) = self.pop_int_pair()?;
                if b == 0 {
                    return Err(RuntimeErrorKind::DivisionByZero);
                }
                let v = a
                    .checked_rem(b)
                    .ok_or(RuntimeErrorKind::ArithmeticOverflow)?;
                self.stack.push(Value::Int(v));
            }
            Opcode::FAdd => self.float_binop(|a, b| a + b)?,
            Opcode::FSub => self.float_binop(|a, b| a - b)?,
            Opcode::FMul => self.float_binop(|a, b| a * b)?,
            Opcode::FDiv => self.float_binop(|a, b| a / b)?,
            Opcode::Eq => self.eq_op(|eq| eq)?,
            Opcode::Ne => self.eq_op(|eq| !eq)?,
            Opcode::Lt => self.ord_op(|o| o == std::cmp::Ordering::Less)?,
            Opcode::Gt => self.ord_op(|o| o == std::cmp::Ordering::Greater)?,
            Opcode::Le => self.ord_op(|o| o != std::cmp::Ordering::Greater)?,
            Opcode::Ge => self.ord_op(|o| o != std::cmp::Ordering::Less)?,
            Opcode::Not => {
                let v = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                match v {
                    Value::Bool(b) => self.stack.push(Value::Bool(!b)),
                    _ => return Err(RuntimeErrorKind::ArithTypeMismatch),
                }
            }
            Opcode::NegI => {
                let v = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                match v {
                    Value::Int(i) => self.stack.push(Value::Int(
                        i.checked_neg()
                            .ok_or(RuntimeErrorKind::ArithmeticOverflow)?,
                    )),
                    _ => return Err(RuntimeErrorKind::ArithTypeMismatch),
                }
            }
            Opcode::NegF => {
                let v = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                match v {
                    Value::Float(x) => self.stack.push(Value::Float(-x)),
                    _ => return Err(RuntimeErrorKind::ArithTypeMismatch),
                }
            }
            Opcode::Jump => {
                let s = take_untracked!(2);
                let target = u16::from_le_bytes([s[0], s[1]]) as usize;
                ip = self.checked_target(frame.func, target)?;
            }
            Opcode::JumpIfFalse => {
                let s = take_untracked!(2);
                let target = u16::from_le_bytes([s[0], s[1]]) as usize;
                match self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)? {
                    Value::Bool(false) => ip = self.checked_target(frame.func, target)?,
                    Value::Bool(true) => ip += 2,
                    _ => return Err(RuntimeErrorKind::ArithTypeMismatch),
                }
            }
            Opcode::JumpIfTrue => {
                let s = take_untracked!(2);
                let target = u16::from_le_bytes([s[0], s[1]]) as usize;
                match self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)? {
                    Value::Bool(true) => ip = self.checked_target(frame.func, target)?,
                    Value::Bool(false) => ip += 2,
                    _ => return Err(RuntimeErrorKind::ArithTypeMismatch),
                }
            }
            Opcode::Call => {
                let s = take!(2);
                let id = u16::from_le_bytes([s[0], s[1]]) as u32;
                let fid = FuncId(id);
                // Peek arg count from the callee.
                let params = self
                    .module
                    .functions
                    .get(id as usize)
                    .ok_or(RuntimeErrorKind::FuncOutOfRange(fid))?
                    .params;
                let start = frame.ip;
                self.frames
                    .last_mut()
                    .ok_or(RuntimeErrorKind::StackUnderflow)?
                    .ip = ip;
                if let Err(e) = self.call(fid, params) {
                    // `call` fails before the callee's frame is pushed, so
                    // the top frame is still the caller: rewind its ip to
                    // the `Call` itself or `locate_error` would report the
                    // next instruction's span.
                    self.frames
                        .last_mut()
                        .ok_or(RuntimeErrorKind::StackUnderflow)?
                        .ip = start;
                    return Err(e);
                }
                return Ok(());
            }
            Opcode::LoadFunc => {
                let s = take!(2);
                let id = u16::from_le_bytes([s[0], s[1]]) as u32;
                if id as usize >= self.module.functions.len() {
                    return Err(RuntimeErrorKind::FuncOutOfRange(FuncId(id)));
                }
                self.stack.push(Value::Func(FuncId(id)));
            }
            Opcode::CallValue => {
                let s = take!(1);
                let argc = s[0] as u16;
                if self.stack.len() < argc as usize + 1 {
                    return Err(RuntimeErrorKind::StackUnderflow);
                }
                let idx = self.stack.len() - argc as usize - 1;
                let f = self.stack.remove(idx);
                match f {
                    Value::Func(fid) => {
                        let start = frame.ip;
                        self.frames
                            .last_mut()
                            .ok_or(RuntimeErrorKind::StackUnderflow)?
                            .ip = ip;
                        if let Err(e) = self.call(fid, argc) {
                            // Same rewind as the `Call` arm: `call` errors
                            // surface before the callee's frame is pushed.
                            self.frames
                                .last_mut()
                                .ok_or(RuntimeErrorKind::StackUnderflow)?
                                .ip = start;
                            return Err(e);
                        }
                        return Ok(());
                    }
                    _ => return Err(RuntimeErrorKind::ArithTypeMismatch),
                }
            }
            Opcode::Return => {
                let ret = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                let done = self.frames.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                self.stack.truncate(done.stack_base);
                // Move the return value back onto the caller's stack. Under
                // exclusive ownership (`Str`/`Boxed`/`Ok`/`Err` are not
                // reference counted) cloning here would deep-copy every
                // returned value; the module result is read by `run` from
                // the stack after the last frame unwinds.
                self.stack.push(ret);
                return Ok(());
            }
            Opcode::BindMatch => {
                let s = take!(2);
                let slot = u16::from_le_bytes([s[0], s[1]]) as usize;
                let v = self
                    .stack
                    .last()
                    .cloned()
                    .ok_or(RuntimeErrorKind::StackUnderflow)?;
                let idx = frame.stack_base + slot;
                if idx >= self.stack.len() {
                    return Err(RuntimeErrorKind::StackShape);
                }
                self.stack[idx] = v;
            }
            // 0.0.2 U13: scalar → string. Typeck guarantees the operand is a
            // scalar; a non-scalar reaching here is a compiler bug, so this is
            // a hard error rather than a silent fallback.
            Opcode::ToStr => {
                let v = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                let s = fmt_scalar(&v).ok_or(RuntimeErrorKind::CastOperandNotScalar)?;
                self.stack.push(Value::Str(s.as_str().into()));
            }
            // ----------------------------------------------------------------
            // 0.0.2 U09: ownership & Result instructions (BYTECODE.md §5.9–5.11).
            // Type/shape violations below are defensive — `fleen-verify`
            // rejects malformed modules before execution — and fail loudly
            // rather than falling through as a no-op, which would corrupt
            // every depth invariant the verifier reasons about.
            // ----------------------------------------------------------------
            Opcode::AllocBox => {
                let v = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                // Runtime backstop for hand-written bytecode (typeck forbids
                // boxing a borrow handle — Ref never inside Boxed, so this
                // is unreachable from compiled code; `fleen-verify` has no
                // value-type view of the stack, so release builds need the
                // check too, not just a debug_assert).
                if matches!(v, Value::Ref { .. }) {
                    return Err(RuntimeErrorKind::BorrowOutOfRange);
                }
                self.stack.push(Value::Boxed(Box::new(v)));
            }
            Opcode::DerefBox => {
                // `b → b v`: the box stays on the stack (peek, not pop); a
                // copy of the pointee is pushed. The operand may be a borrow
                // handle (`ref box<T>` read path) — resolve it first.
                let copy = {
                    let b = self.stack.last().ok_or(RuntimeErrorKind::StackUnderflow)?;
                    match self.read_ref(b)? {
                        Value::Boxed(inner) => inner.as_ref().clone(),
                        _ => return Err(RuntimeErrorKind::StackShape),
                    }
                };
                self.stack.push(copy);
            }
            Opcode::StoreDerefBox => {
                // `b v →`: replace the pointee, old value released by the
                // assignment's Drop. Through a borrow handle the write lands
                // in the handle's target slot (the only form with persistent
                // effect; the compiler's own lowering no longer emits this
                // opcode — see `fleen-compiler/src/lower` AssignDeref).
                let v = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                let b = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                match b {
                    Value::Boxed(mut inner) => {
                        *inner = v;
                    }
                    Value::Ref { .. } => {
                        let idx = self
                            .ref_index(&b)
                            .ok_or(RuntimeErrorKind::BorrowOutOfRange)?;
                        let slot = self
                            .stack
                            .get_mut(idx)
                            .ok_or(RuntimeErrorKind::BorrowOutOfRange)?;
                        match slot {
                            Value::Boxed(inner) => **inner = v,
                            _ => return Err(RuntimeErrorKind::StackShape),
                        }
                    }
                    _ => return Err(RuntimeErrorKind::StackShape),
                }
            }
            Opcode::MakeRefLocal => {
                let s = take!(2);
                let slot = u16::from_le_bytes([s[0], s[1]]);
                let idx = frame.stack_base + slot as usize;
                let existing = self.stack.get(idx).ok_or(RuntimeErrorKind::StackShape)?;
                let handle = match existing {
                    // Forwarding: the slot already holds a handle (a `ref`
                    // parameter re-passed to another `ref` parameter). Push
                    // the original handle as-is — re-wrapping would create a
                    // handle-to-handle, which dereference cannot see through.
                    // The check is on the *slot contents*, not the stack top:
                    // with several ref arguments in one call the top of the
                    // stack is the previously prepared argument.
                    Value::Ref { .. } => existing.clone(),
                    _ => Value::Ref {
                        base: frame.stack_base as u32,
                        slot,
                    },
                };
                self.stack.push(handle);
            }
            Opcode::DupDeep => {
                // `clone`'s stack form: deep copy the top. `Box<Value>`'s
                // derived Clone copies recursively; a handle on top copies
                // the value it points at, never the handle.
                let copy = {
                    let top = self.stack.last().ok_or(RuntimeErrorKind::StackUnderflow)?;
                    self.read_ref(top)?.clone()
                };
                self.stack.push(copy);
            }
            Opcode::MoveLocal => {
                let s = take!(2);
                let slot = u16::from_le_bytes([s[0], s[1]]) as usize;
                let idx = frame.stack_base + slot;
                // Consumptive read: the slot is cleared so the moved value's
                // Drop cannot be deferred to frame destruction.
                let v = {
                    let slot_v = self
                        .stack
                        .get_mut(idx)
                        .ok_or(RuntimeErrorKind::StackShape)?;
                    std::mem::replace(slot_v, Value::Unit)
                };
                self.stack.push(v);
            }
            Opcode::CloneLocal => {
                let s = take!(2);
                let slot = u16::from_le_bytes([s[0], s[1]]) as usize;
                let idx = frame.stack_base + slot;
                let copy = match self.stack.get(idx).ok_or(RuntimeErrorKind::StackShape)? {
                    // Cloning a borrowed parameter deep-copies its target.
                    v @ Value::Ref { .. } => self.read_ref(v)?.clone(),
                    other => other.clone(),
                };
                self.stack.push(copy);
            }
            Opcode::CloneGlobal => {
                let s = take!(2);
                let id = u16::from_le_bytes([s[0], s[1]]) as usize;
                let g = self
                    .globals
                    .get(id)
                    .cloned()
                    .ok_or(RuntimeErrorKind::GlobalOutOfRange(
                        fleen_compiler::codegen::GlobalId(id as u32),
                    ))?;
                // Globals never hold borrow handles in compiled code (typeck
                // forbids storing a `Ref`); the defensive backstop for
                // hand-written modules is the dereference at the consumption
                // point, not the copy here.
                self.stack.push(g);
            }
            Opcode::PackOk => {
                let v = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                // Same runtime backstop as `AllocBox` (Ref never inside Ok).
                if matches!(v, Value::Ref { .. }) {
                    return Err(RuntimeErrorKind::BorrowOutOfRange);
                }
                self.stack.push(Value::Ok(Box::new(v)));
            }
            Opcode::PackErr => {
                let v = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                // Same runtime backstop as `AllocBox` (Ref never inside Err).
                if matches!(v, Value::Ref { .. }) {
                    return Err(RuntimeErrorKind::BorrowOutOfRange);
                }
                self.stack.push(Value::Err(Box::new(v)));
            }
            Opcode::IsErr => {
                // Pops the *duplicated* copy the `?`/choose-Result lowering
                // puts on top; the original Result stays for the matching
                // Unwrap on either branch.
                let r = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                let is_err = match r {
                    Value::Ok(_) => false,
                    Value::Err(_) => true,
                    _ => return Err(RuntimeErrorKind::StackShape),
                };
                self.stack.push(Value::Bool(is_err));
            }
            Opcode::UnwrapOk => {
                let r = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                match r {
                    Value::Ok(v) => self.stack.push(*v),
                    Value::Err(_) => return Err(RuntimeErrorKind::ResultMismatch),
                    _ => return Err(RuntimeErrorKind::StackShape),
                }
            }
            Opcode::UnwrapErr => {
                let r = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                match r {
                    Value::Err(e) => self.stack.push(*e),
                    Value::Ok(_) => return Err(RuntimeErrorKind::ResultMismatch),
                    _ => return Err(RuntimeErrorKind::StackShape),
                }
            }
            // ----------------------------------------------------------------
            // 0.0.3 V02: struct instructions (bytecode v3, 0xB0–0xB3).
            // Shape violations are defensive — `fleen-verify` rejects
            // malformed modules before execution.
            // ----------------------------------------------------------------
            Opcode::StructNew => {
                let s = take!(2);
                let id = u16::from_le_bytes([s[0], s[1]]) as usize;
                let n = self
                    .module
                    .struct_defs
                    .get(id)
                    .ok_or(RuntimeErrorKind::StructOutOfRange(id as u16))?
                    .field_types
                    .len();
                if self.stack.len() < n {
                    return Err(RuntimeErrorKind::StackUnderflow);
                }
                let fields: Vec<Value> = self.stack.split_off(self.stack.len() - n);
                self.stack.push(Value::Struct(fields.into_boxed_slice()));
            }
            Opcode::GetField => {
                let s = take!(2);
                let offset = u16::from_le_bytes([s[0], s[1]]) as usize;
                let v = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                // The object may be a borrow handle (`ref S` parameter read
                // path) — dereference first; the copy-out semantics make the
                // read trivially safe (D2).
                let copy = {
                    let obj = self.read_ref(&v)?;
                    match obj {
                        Value::Struct(fields) => {
                            if offset >= fields.len() {
                                return Err(RuntimeErrorKind::FieldOutOfRange {
                                    index: offset as u16,
                                    len: fields.len(),
                                });
                            }
                            fields[offset].clone()
                        }
                        _ => return Err(RuntimeErrorKind::StackShape),
                    }
                };
                self.stack.push(copy);
            }
            Opcode::SetFieldLocal => {
                let s = take!(4);
                let slot = u16::from_le_bytes([s[0], s[1]]) as usize;
                let offset = u16::from_le_bytes([s[2], s[3]]) as usize;
                let v = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                let idx = frame.stack_base + slot;
                let slot_v = self
                    .stack
                    .get_mut(idx)
                    .ok_or(RuntimeErrorKind::StackShape)?;
                // Field writes go through local/global struct bindings only
                // (typeck rejects writes through borrow handles), so a `Ref`
                // in the slot is malformed bytecode.
                match slot_v {
                    Value::Struct(fields) => {
                        if offset >= fields.len() {
                            return Err(RuntimeErrorKind::FieldOutOfRange {
                                index: offset as u16,
                                len: fields.len(),
                            });
                        }
                        // The overwrite drops the old field value.
                        fields[offset] = v;
                    }
                    _ => return Err(RuntimeErrorKind::StackShape),
                }
            }
            Opcode::SetFieldGlobal => {
                let s = take!(4);
                let gid = u16::from_le_bytes([s[0], s[1]]) as usize;
                let offset = u16::from_le_bytes([s[2], s[3]]) as usize;
                let v = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                if gid >= self.globals.len() {
                    return Err(RuntimeErrorKind::GlobalOutOfRange(
                        fleen_compiler::codegen::GlobalId(gid as u32),
                    ));
                }
                // Writing a field does not rebind the global, so the
                // `mutable` gate of `StoreGlobal` does not apply here (same
                // rule as `deref b = v` on a const box binding).
                match &mut self.globals[gid] {
                    Value::Struct(fields) => {
                        if offset >= fields.len() {
                            return Err(RuntimeErrorKind::FieldOutOfRange {
                                index: offset as u16,
                                len: fields.len(),
                            });
                        }
                        fields[offset] = v;
                    }
                    _ => return Err(RuntimeErrorKind::StackShape),
                }
            }
            // ----------------------------------------------------------------
            // 0.0.3 V03: array instructions (bytecode v3, 0xB4–0xB8).
            // `ArrGet` bounds failures are NOT defensive — the `a[i]` syntax
            // is bounds-checked at runtime by design (D3). Other shape
            // violations are defensive (fleen-verify rejects malformed
            // modules before execution).
            // ----------------------------------------------------------------
            Opcode::ArrNew => {
                let s = take!(2);
                let n = u16::from_le_bytes([s[0], s[1]]) as usize;
                if self.stack.len() < n {
                    return Err(RuntimeErrorKind::StackUnderflow);
                }
                let elems: Vec<Value> = self.stack.split_off(self.stack.len() - n);
                self.stack.push(Value::Arr(elems.into_boxed_slice()));
            }
            Opcode::ArrLen => {
                // `a → a int`: peek — the array stays on the stack. Through
                // a borrow handle (`ref [T]` read path) the target's length
                // is read.
                let len = {
                    let a = self.stack.last().ok_or(RuntimeErrorKind::StackUnderflow)?;
                    match self.read_ref(a)? {
                        Value::Arr(elems) => elems.len(),
                        _ => return Err(RuntimeErrorKind::StackShape),
                    }
                };
                self.stack.push(Value::Int(len as i64));
            }
            Opcode::ArrGet => {
                // `a i → v` (pop form, the V02 `GetField` erratum shape):
                // the element copy is a deep copy for owned element types,
                // so the read never disturbs the original. The sign check
                // comes before the range check — `a[-1]` must be an
                // out-of-range error, never a huge usize index.
                let i = match self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)? {
                    Value::Int(i) => i,
                    _ => return Err(RuntimeErrorKind::ArithTypeMismatch),
                };
                let a = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                let copy = {
                    let arr = self.read_ref(&a)?;
                    match arr {
                        Value::Arr(elems) => {
                            if i < 0 || i as usize >= elems.len() {
                                return Err(RuntimeErrorKind::IndexOutOfRange {
                                    index: i,
                                    len: elems.len(),
                                });
                            }
                            elems[i as usize].clone()
                        }
                        _ => return Err(RuntimeErrorKind::StackShape),
                    }
                };
                self.stack.push(copy);
            }
            Opcode::ArrSetLocal => {
                let s = take!(2);
                let slot = u16::from_le_bytes([s[0], s[1]]) as usize;
                let i = match self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)? {
                    Value::Int(i) => i,
                    _ => return Err(RuntimeErrorKind::ArithTypeMismatch),
                };
                let v = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                let idx = frame.stack_base + slot;
                let slot_v = self
                    .stack
                    .get_mut(idx)
                    .ok_or(RuntimeErrorKind::StackShape)?;
                // Element writes go through local/global array bindings
                // only (typeck rejects writes through borrow handles), so a
                // `Ref` in the slot is malformed bytecode.
                match slot_v {
                    Value::Arr(elems) => {
                        if i < 0 || i as usize >= elems.len() {
                            return Err(RuntimeErrorKind::IndexOutOfRange {
                                index: i,
                                len: elems.len(),
                            });
                        }
                        // The overwrite drops the old element value.
                        elems[i as usize] = v;
                    }
                    _ => return Err(RuntimeErrorKind::StackShape),
                }
            }
            Opcode::ArrSetGlobal => {
                let s = take!(2);
                let gid = u16::from_le_bytes([s[0], s[1]]) as usize;
                let i = match self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)? {
                    Value::Int(i) => i,
                    _ => return Err(RuntimeErrorKind::ArithTypeMismatch),
                };
                let v = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                if gid >= self.globals.len() {
                    return Err(RuntimeErrorKind::GlobalOutOfRange(
                        fleen_compiler::codegen::GlobalId(gid as u32),
                    ));
                }
                // Writing an element does not rebind the global, so the
                // `mutable` gate of `StoreGlobal` does not apply here (same
                // rule as `SetFieldGlobal` on a const struct binding).
                match &mut self.globals[gid] {
                    Value::Arr(elems) => {
                        if i < 0 || i as usize >= elems.len() {
                            return Err(RuntimeErrorKind::IndexOutOfRange {
                                index: i,
                                len: elems.len(),
                            });
                        }
                        elems[i as usize] = v;
                    }
                    _ => return Err(RuntimeErrorKind::StackShape),
                }
            }
            // ----------------------------------------------------------------
            // 0.0.3 V04: range instructions (bytecode v3, 0xB9–0xBB).
            // A Range is a plain pair of ints; `RangeStart`/`RangeEnd` read a
            // bound once, before the `for` loop, and leave the Range on the
            // stack for the `Pop` the desugar emits. Not reachable from
            // source until V06 (`std.collection.range`), so a non-Range
            // operand here is malformed bytecode.
            // ----------------------------------------------------------------
            Opcode::MakeRange => {
                // `start end → r`.
                let end = match self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)? {
                    Value::Int(v) => v,
                    _ => return Err(RuntimeErrorKind::ArithTypeMismatch),
                };
                let start = match self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)? {
                    Value::Int(v) => v,
                    _ => return Err(RuntimeErrorKind::ArithTypeMismatch),
                };
                self.stack.push(Value::Range { start, end });
            }
            Opcode::RangeStart => {
                // `r → r int`: peek, the Range stays on the stack.
                let r = match self.stack.last().ok_or(RuntimeErrorKind::StackUnderflow)? {
                    Value::Range { start, .. } => *start,
                    _ => return Err(RuntimeErrorKind::StackShape),
                };
                self.stack.push(Value::Int(r));
            }
            Opcode::RangeEnd => {
                let r = match self.stack.last().ok_or(RuntimeErrorKind::StackUnderflow)? {
                    Value::Range { end, .. } => *end,
                    _ => return Err(RuntimeErrorKind::StackShape),
                };
                self.stack.push(Value::Int(r));
            }
        }
        self.frames
            .last_mut()
            .ok_or(RuntimeErrorKind::StackUnderflow)?
            .ip = ip;
        Ok(())
    }

    fn checked_target(&self, fid: FuncId, target: usize) -> Result<usize, RuntimeErrorKind> {
        let code = &self.module.functions[fid.0 as usize].code;
        if target > code.len() {
            return Err(RuntimeErrorKind::BadJumpTarget);
        }
        Ok(target)
    }

    fn pop_int_pair(&mut self) -> Result<(i64, i64), RuntimeErrorKind> {
        let b = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
        let a = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
        match (a, b) {
            (Value::Int(x), Value::Int(y)) => Ok((x, y)),
            _ => Err(RuntimeErrorKind::ArithTypeMismatch),
        }
    }

    fn int_binop(&mut self, f: impl Fn(&i64, &i64) -> Option<i64>) -> Result<(), RuntimeErrorKind> {
        let (a, b) = self.pop_int_pair()?;
        let v = f(&a, &b).ok_or(RuntimeErrorKind::ArithmeticOverflow)?;
        self.stack.push(Value::Int(v));
        Ok(())
    }

    fn float_binop(&mut self, f: fn(f64, f64) -> f64) -> Result<(), RuntimeErrorKind> {
        let b = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
        let a = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
        match (a, b) {
            (Value::Float(x), Value::Float(y)) => {
                self.stack.push(Value::Float(f(x, y)));
                Ok(())
            }
            _ => Err(RuntimeErrorKind::ArithTypeMismatch),
        }
    }

    /// Absolute operand-stack index a borrow handle points at, or `None`
    /// for any other value (0.0.2 U09).
    fn ref_index(&self, v: &Value) -> Option<usize> {
        match v {
            Value::Ref { base, slot } => {
                let idx = *base as usize + *slot as usize;
                (idx < self.stack.len()).then_some(idx)
            }
            _ => None,
        }
    }

    /// Read-only dereference: a handle yields the slot it points at, any
    /// other value yields itself (TICKETS U09).
    ///
    /// Handles use absolute stack indices, so the target is found even after
    /// operand-stack reallocation. Typeck guarantees a handle only points at
    /// a frame below the current call chain (which therefore outlives it);
    /// the range check here is the defensive backstop for hand-written
    /// bytecode, firing [`RuntimeErrorKind::BorrowOutOfRange`].
    fn read_ref<'s>(&'s self, v: &'s Value) -> Result<&'s Value, RuntimeErrorKind> {
        match self.ref_index(v) {
            Some(idx) => {
                let target = &self.stack[idx];
                // Handle-to-handle cannot arise from the compiler: the only
                // handle producer, `MakeRefLocal`, forwards an existing
                // handle instead of re-wrapping one.
                debug_assert!(
                    !matches!(target, Value::Ref { .. }),
                    "handle-to-handle: MakeRefLocal forwarding must prevent re-wrapping"
                );
                Ok(target)
            }
            None if matches!(v, Value::Ref { .. }) => Err(RuntimeErrorKind::BorrowOutOfRange),
            None => Ok(v),
        }
    }

    fn eq_op(&mut self, f: fn(bool) -> bool) -> Result<(), RuntimeErrorKind> {
        let b = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
        let a = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
        // Borrow handles dereference before comparing (BYTECODE.md §3.2);
        // the derived structural `PartialEq` on `Ref` is for tests only.
        let a = self.read_ref(&a)?;
        let b = self.read_ref(&b)?;
        let eq = match (a, b) {
            (Value::Int(x), Value::Int(y)) => x == y,
            (Value::Float(x), Value::Float(y)) => x == y,
            (Value::Bool(x), Value::Bool(y)) => x == y,
            (Value::Str(x), Value::Str(y)) => x == y,
            (Value::Unit, Value::Unit) => true,
            (Value::Func(x), Value::Func(y)) => x == y,
            // A box observes — and compares — its pointee (§3.2, matching
            // Rust's `Box: PartialEq`).
            (Value::Boxed(x), Value::Boxed(y)) => x == y,
            (Value::Ok(x), Value::Ok(y)) => x == y,
            (Value::Err(x), Value::Err(y)) => x == y,
            (Value::Ok(_), Value::Err(_)) | (Value::Err(_), Value::Ok(_)) => false,
            _ => return Err(RuntimeErrorKind::ArithTypeMismatch),
        };
        self.stack.push(Value::Bool(f(eq)));
        Ok(())
    }

    fn ord_op(&mut self, f: impl Fn(std::cmp::Ordering) -> bool) -> Result<(), RuntimeErrorKind> {
        let b = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
        let a = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
        // Same handle discipline as `eq_op` (a `ref string` parameter can
        // reach `<`/`<=`/... through ordinary comparison expressions).
        let a = self.read_ref(&a)?;
        let b = self.read_ref(&b)?;
        let r = match (a, b) {
            (Value::Int(x), Value::Int(y)) => x.cmp(y),
            (Value::Float(x), Value::Float(y)) => x
                .partial_cmp(y)
                .ok_or(RuntimeErrorKind::ArithTypeMismatch)?,
            (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
            (Value::Str(x), Value::Str(y)) => x.cmp(y),
            _ => return Err(RuntimeErrorKind::ArithTypeMismatch),
        };
        self.stack.push(Value::Bool(f(r)));
        Ok(())
    }
}
