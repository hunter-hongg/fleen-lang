//! Core interpreter loop.

use std::rc::Rc;

use fleen_compiler::codegen::{Const, FuncId, Module, Opcode};

use crate::error::RuntimeError;
use crate::frame::CallFrame;
use crate::value::Value;

const MAX_CALL_DEPTH: usize = 1024;

/// The virtual machine.
///
/// Executes the bytecode produced by `fleen-compiler::codegen`. Builtin
/// functions run host-side. A global whose descriptor has `mutable ==
/// false` may only be written while the currently executing function is
/// named `__init__` (the module initializer); writes elsewhere raise
/// [`RuntimeError::ImmutableGlobal`].
pub struct Vm {
    module: Module,
    stack: Vec<Value>,
    frames: Vec<CallFrame>,
    globals: Vec<Value>,
}

impl Vm {
    /// Create a VM ready to execute `module`.
    ///
    /// All globals are initialized to [`Value::Unit`]; the module's
    /// `__init__` function is expected to overwrite them.
    pub fn new(module: Module) -> Self {
        let n_globals = module.globals.len();
        Vm {
            module,
            stack: Vec::new(),
            frames: Vec::new(),
            globals: vec![Value::Unit; n_globals],
        }
    }

    /// Run the module's `entry` function to completion.
    pub fn run(&mut self) -> Result<Value, RuntimeError> {
        if self.module.version != 1 {
            return Err(RuntimeError::UnsupportedVersion(self.module.version));
        }
        let entry = self.module.entry;
        self.call(entry, 0)?;
        // No globals/`__init__` path pushes extra frames; run until empty.
        loop {
            if self.frames.is_empty() {
                return Ok(self.stack.pop().unwrap_or(Value::Unit));
            }
            match self.step()? {
                Step::Continue => {}
                Step::Returned(v) if self.frames.is_empty() => return Ok(v),
                Step::Returned(_) => {}
            }
        }
    }

    /// Name of a function (for builtin dispatch and `__init__` detection).
    fn func_name(&self, fid: FuncId) -> Result<&str, RuntimeError> {
        let f = self
            .module
            .functions
            .get(fid.0 as usize)
            .ok_or(RuntimeError::FuncOutOfRange(fid))?;
        match &self.module.constants[f.name.0 as usize] {
            Const::Str(s) => Ok(s),
            _ => Err(RuntimeError::StackShape),
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
    fn call(&mut self, fid: FuncId, argc: u16) -> Result<(), RuntimeError> {
        let func = self
            .module
            .functions
            .get(fid.0 as usize)
            .ok_or(RuntimeError::FuncOutOfRange(fid))?
            .clone();
        if func.params != argc {
            return Err(RuntimeError::StackShape);
        }
        if func.is_builtin {
            let name = self.func_name(fid)?.to_string();
            let args: Vec<Value> = (0..argc)
                .map(|_| self.stack.pop().ok_or(RuntimeError::StackUnderflow))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .rev()
                .collect();
            let ret = self.exec_builtin(&name, &args)?;
            self.stack.push(ret);
            return Ok(());
        }
        if self.frames.len() >= MAX_CALL_DEPTH {
            return Err(RuntimeError::CallDepthExceeded);
        }
        let base = self.stack.len() - argc as usize;
        // Reserve slots for locals beyond the parameters.
        if (func.locals as usize) < argc as usize {
            return Err(RuntimeError::StackShape);
        }
        self.stack.resize(base + func.locals as usize, Value::Unit);
        self.frames.push(CallFrame {
            func: fid,
            ip: 0,
            stack_base: base,
        });
        Ok(())
    }

    fn exec_builtin(&self, name: &str, args: &[Value]) -> Result<Value, RuntimeError> {
        match name {
            "print" => {
                let line: String = args.iter().map(|a| a.to_string()).collect();
                println!("{line}");
                Ok(Value::Unit)
            }
            _ => Err(RuntimeError::UnknownBuiltin(name.to_string())),
        }
    }

    fn step(&mut self) -> Result<Step, RuntimeError> {
        let frame = *self.frames.last().ok_or(RuntimeError::StackUnderflow)?;
        let func = &self.module.functions[frame.func.0 as usize];
        let code = &func.code;
        if frame.ip >= code.len() {
            return Err(RuntimeError::BadJumpTarget);
        }
        let op = code[frame.ip];
        let opcode = Opcode::from_byte(op).ok_or(RuntimeError::InvalidOpcode(op))?;
        let mut ip = frame.ip + 1;

        // Read an operand of `n` bytes; defensive length checks.
        macro_rules! take {
            ($n:expr) => {{
                let n: usize = $n;
                if ip + n > code.len() {
                    return Err(RuntimeError::BadJumpTarget);
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
                    return Err(RuntimeError::BadJumpTarget);
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
                    Some(Const::Str(s)) => Value::Str(Rc::from(s.as_ref())),
                    None => {
                        return Err(RuntimeError::ConstOutOfRange(
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
                self.stack.pop().ok_or(RuntimeError::StackUnderflow)?;
            }
            Opcode::Dup => {
                let v = self
                    .stack
                    .last()
                    .cloned()
                    .ok_or(RuntimeError::StackUnderflow)?;
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
                    .ok_or(RuntimeError::StackShape)?;
                self.stack.push(v);
            }
            Opcode::StoreLocal => {
                let s = take!(2);
                let slot = u16::from_le_bytes([s[0], s[1]]) as usize;
                let v = self.stack.pop().ok_or(RuntimeError::StackUnderflow)?;
                let idx = frame.stack_base + slot;
                if idx >= self.stack.len() {
                    return Err(RuntimeError::StackShape);
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
                    .ok_or(RuntimeError::GlobalOutOfRange(
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
                    .ok_or(RuntimeError::GlobalOutOfRange(
                        fleen_compiler::codegen::GlobalId(id as u32),
                    ))?;
                if !g.mutable && !self.current_frame_is_init() {
                    return Err(RuntimeError::ImmutableGlobal(
                        fleen_compiler::codegen::GlobalId(id as u32),
                    ));
                }
                let v = self.stack.pop().ok_or(RuntimeError::StackUnderflow)?;
                self.globals[id] = v;
            }
            Opcode::IAdd => self.int_binop(|a, b| a.checked_add(*b))?,
            Opcode::ISub => self.int_binop(|a, b| a.checked_sub(*b))?,
            Opcode::IMul => self.int_binop(|a, b| a.checked_mul(*b))?,
            Opcode::IDiv => {
                let (a, b) = self.pop_int_pair()?;
                if b == 0 {
                    return Err(RuntimeError::DivisionByZero);
                }
                let v = a.checked_div(b).ok_or(RuntimeError::ArithmeticOverflow)?;
                self.stack.push(Value::Int(v));
            }
            Opcode::IMod => {
                let (a, b) = self.pop_int_pair()?;
                if b == 0 {
                    return Err(RuntimeError::DivisionByZero);
                }
                let v = a.checked_rem(b).ok_or(RuntimeError::ArithmeticOverflow)?;
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
                let v = self.stack.pop().ok_or(RuntimeError::StackUnderflow)?;
                match v {
                    Value::Bool(b) => self.stack.push(Value::Bool(!b)),
                    _ => return Err(RuntimeError::ArithTypeMismatch),
                }
            }
            Opcode::NegI => {
                let v = self.stack.pop().ok_or(RuntimeError::StackUnderflow)?;
                match v {
                    Value::Int(i) => self.stack.push(Value::Int(
                        i.checked_neg().ok_or(RuntimeError::ArithmeticOverflow)?,
                    )),
                    _ => return Err(RuntimeError::ArithTypeMismatch),
                }
            }
            Opcode::NegF => {
                let v = self.stack.pop().ok_or(RuntimeError::StackUnderflow)?;
                match v {
                    Value::Float(x) => self.stack.push(Value::Float(-x)),
                    _ => return Err(RuntimeError::ArithTypeMismatch),
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
                match self.stack.pop().ok_or(RuntimeError::StackUnderflow)? {
                    Value::Bool(false) => ip = self.checked_target(frame.func, target)?,
                    Value::Bool(true) => ip += 2,
                    _ => return Err(RuntimeError::ArithTypeMismatch),
                }
            }
            Opcode::JumpIfTrue => {
                let s = take_untracked!(2);
                let target = u16::from_le_bytes([s[0], s[1]]) as usize;
                match self.stack.pop().ok_or(RuntimeError::StackUnderflow)? {
                    Value::Bool(true) => ip = self.checked_target(frame.func, target)?,
                    Value::Bool(false) => ip += 2,
                    _ => return Err(RuntimeError::ArithTypeMismatch),
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
                    .ok_or(RuntimeError::FuncOutOfRange(fid))?
                    .params;
                self.frames
                    .last_mut()
                    .ok_or(RuntimeError::StackUnderflow)?
                    .ip = ip;
                self.call(fid, params)?;
                return Ok(Step::Continue);
            }
            Opcode::LoadFunc => {
                let s = take!(2);
                let id = u16::from_le_bytes([s[0], s[1]]) as u32;
                if id as usize >= self.module.functions.len() {
                    return Err(RuntimeError::FuncOutOfRange(FuncId(id)));
                }
                self.stack.push(Value::Func(FuncId(id)));
            }
            Opcode::CallValue => {
                let s = take!(1);
                let argc = s[0] as u16;
                if self.stack.len() < argc as usize + 1 {
                    return Err(RuntimeError::StackUnderflow);
                }
                let idx = self.stack.len() - argc as usize - 1;
                let f = self.stack.remove(idx);
                match f {
                    Value::Func(fid) => {
                        self.frames
                            .last_mut()
                            .ok_or(RuntimeError::StackUnderflow)?
                            .ip = ip;
                        self.call(fid, argc)?;
                        return Ok(Step::Continue);
                    }
                    _ => return Err(RuntimeError::ArithTypeMismatch),
                }
            }
            Opcode::Return => {
                let ret = self.stack.pop().ok_or(RuntimeError::StackUnderflow)?;
                let done = self.frames.pop().ok_or(RuntimeError::StackUnderflow)?;
                self.stack.truncate(done.stack_base);
                self.stack.push(ret.clone());
                return Ok(Step::Returned(ret));
            }
            Opcode::BindMatch => {
                let s = take!(2);
                let slot = u16::from_le_bytes([s[0], s[1]]) as usize;
                let v = self
                    .stack
                    .last()
                    .cloned()
                    .ok_or(RuntimeError::StackUnderflow)?;
                let idx = frame.stack_base + slot;
                if idx >= self.stack.len() {
                    return Err(RuntimeError::StackShape);
                }
                self.stack[idx] = v;
            }
        }
        self.frames
            .last_mut()
            .ok_or(RuntimeError::StackUnderflow)?
            .ip = ip;
        Ok(Step::Continue)
    }

    fn checked_target(&self, fid: FuncId, target: usize) -> Result<usize, RuntimeError> {
        let code = &self.module.functions[fid.0 as usize].code;
        if target > code.len() {
            return Err(RuntimeError::BadJumpTarget);
        }
        Ok(target)
    }

    fn pop_int_pair(&mut self) -> Result<(i64, i64), RuntimeError> {
        let b = self.stack.pop().ok_or(RuntimeError::StackUnderflow)?;
        let a = self.stack.pop().ok_or(RuntimeError::StackUnderflow)?;
        match (a, b) {
            (Value::Int(x), Value::Int(y)) => Ok((x, y)),
            _ => Err(RuntimeError::ArithTypeMismatch),
        }
    }

    fn int_binop(&mut self, f: impl Fn(&i64, &i64) -> Option<i64>) -> Result<(), RuntimeError> {
        let (a, b) = self.pop_int_pair()?;
        let v = f(&a, &b).ok_or(RuntimeError::ArithmeticOverflow)?;
        self.stack.push(Value::Int(v));
        Ok(())
    }

    fn float_binop(&mut self, f: fn(f64, f64) -> f64) -> Result<(), RuntimeError> {
        let b = self.stack.pop().ok_or(RuntimeError::StackUnderflow)?;
        let a = self.stack.pop().ok_or(RuntimeError::StackUnderflow)?;
        match (a, b) {
            (Value::Float(x), Value::Float(y)) => {
                self.stack.push(Value::Float(f(x, y)));
                Ok(())
            }
            _ => Err(RuntimeError::ArithTypeMismatch),
        }
    }

    fn eq_op(&mut self, f: fn(bool) -> bool) -> Result<(), RuntimeError> {
        let b = self.stack.pop().ok_or(RuntimeError::StackUnderflow)?;
        let a = self.stack.pop().ok_or(RuntimeError::StackUnderflow)?;
        let eq = match (&a, &b) {
            (Value::Int(x), Value::Int(y)) => x == y,
            (Value::Float(x), Value::Float(y)) => x == y,
            (Value::Bool(x), Value::Bool(y)) => x == y,
            (Value::Str(x), Value::Str(y)) => x == y,
            (Value::Unit, Value::Unit) => true,
            (Value::Func(x), Value::Func(y)) => x == y,
            _ => return Err(RuntimeError::ArithTypeMismatch),
        };
        self.stack.push(Value::Bool(f(eq)));
        Ok(())
    }

    fn ord_op(&mut self, f: impl Fn(std::cmp::Ordering) -> bool) -> Result<(), RuntimeError> {
        let b = self.stack.pop().ok_or(RuntimeError::StackUnderflow)?;
        let a = self.stack.pop().ok_or(RuntimeError::StackUnderflow)?;
        let r = match (&a, &b) {
            (Value::Int(x), Value::Int(y)) => x.cmp(y),
            (Value::Float(x), Value::Float(y)) => {
                x.partial_cmp(y).ok_or(RuntimeError::ArithTypeMismatch)?
            }
            (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
            (Value::Str(x), Value::Str(y)) => x.cmp(y),
            _ => return Err(RuntimeError::ArithTypeMismatch),
        };
        self.stack.push(Value::Bool(f(r)));
        Ok(())
    }
}

/// Outcome of one interpreter step.
enum Step {
    Continue,
    Returned(Value),
}
