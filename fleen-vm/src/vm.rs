//! Core interpreter loop.

use fleen_compiler::codegen::{Const, FuncId, Module, Opcode};

use crate::error::{RuntimeError, RuntimeErrorKind};
use crate::executor::{Scheduler, Task};
use crate::fmt::fmt_scalar;
use crate::frame::{CallFrame, FutureFrame, FutureState};
use crate::value::{ChannelInner, Value};
use std::cell::RefCell;
use std::rc::Rc;

pub(crate) const MAX_CALL_DEPTH: usize = 1024;

/// The virtual machine.
///
/// Executes the bytecode produced by `fleen-compiler::codegen`. Builtin
/// functions run host-side. A global whose descriptor has `mutable ==
/// false` may only be written while the currently executing function is
/// named `__init__` (the module initializer); writes elsewhere raise
/// [`RuntimeErrorKind::ImmutableGlobal`].
pub struct Vm {
    module: Module,
    /// Shared operand stack.  `pub(crate)` so the coroutine snapshot/restore
    /// helpers (T04) and their tests can move frame regions in and out of it.
    pub(crate) stack: Vec<Value>,
    /// Live call frames, innermost last.
    pub(crate) frames: Vec<CallFrame>,
    globals: Vec<Value>,
    /// The host program's command-line arguments (0.0.3 V06), served to
    /// Fleen by `std.os.args` — without `argv[0]`.
    program_args: Vec<String>,
    /// Cooperative task scheduler (0.0.4 T06). Non-empty while `std.async.run`
    /// has spawned tasks that have not all completed; `run_inner` drives it
    /// to empty before returning.
    scheduler: Scheduler,
    /// The task chain most recently detached by a `Suspend` (0.0.4 T06). Set by
    /// the `Suspend` dispatch, consumed by `run_inner`, which parks it in the
    /// scheduler. One slot — only the innermost instruction can suspend, so a
    /// step yields exactly one task.
    pending: Option<Task>,
    /// Set when a drive must join the scheduler before the (synchronous) caller
    /// frame may continue: `std.async.run` spawning tasks, or a top-level bare
    /// `await` whose root suspended. `run_inner` drains the scheduler to empty
    /// (the join) and only then steps the caller frame.
    need_join: bool,
    /// Deadline (monotonic ms) set by `std.io.sleep` for the *exact* suspension
    /// point (function + instruction pointer after `Suspend`).  A `sleep` call
    /// inside a synchronous helper must not leak its deadline into a later
    /// unrelated `Suspend` (e.g. from `yield_now`).  Stored as
    /// `(func_id, suspend_ip, deadline_ms)` and only consumed when the
    /// `Suspend` dispatch is at that identical location.
    sleep_deadline: Option<(usize, usize, i64)>,
    /// When `std.async.run` is called from inside an async body, the ids of
    /// the tasks it spawned.  The following `Suspend` will park the caller;
    /// instead of re-enqueueing it immediately, `run_inner`/`drive_join`
    /// create a `JoinWait` and drive the scheduler until every spawned task
    /// has completed.  (An empty vec — `run([])` — means nothing to wait
    /// for: the parked caller is re-enqueued immediately.)
    nested_run_expected: Option<Vec<u64>>,
    /// Stack of suspended tasks each waiting for a nested `std.async.run` to
    /// complete all its spawned tasks (outermost first).  A stack, because a
    /// resumed waiter's own continuation may call `std.async.run` again
    /// before the outer join has released it; overwriting a single slot
    /// would drop the outer waiter's parked chain.
    waiting_join: Vec<JoinWait>,
    /// Set when the running task parked on a channel (0.0.4 T07): its chain
    /// moved into the channel's sender/receiver queue, so unlike `Suspend`
    /// there is no `pending` task to re-enqueue — the waker owns it now.
    /// Consumed by `run_inner`/`drive_join` to keep driving the remaining
    /// tasks instead of treating the park as a completion.
    suspended_on_channel: bool,
    /// One handle per channel that currently has parked tasks (T07). The
    /// parked chain lives *inside* `ChannelInner`, and a parked task's frame
    /// no longer holds the channel handle (`send`/`recv` popped it before
    /// suspending) — without this registry the last live handle dropping
    /// would deallocate the `ChannelInner` and silently lose the parked
    /// task. It also roots the T07b deadlock scan. Deduplicated by inner
    /// pointer, so it is bounded by the number of distinct channels.
    channel_waiters: Vec<Value>,
    /// Counter for `std.async.run`'s task ids (0.0.4 T06). Ids are unique per
    /// VM instance and never reused, so a completed task's id can never be
    /// mistaken for a later spawn.
    next_task_id: u64,
}

/// A suspended task waiting for a nested `std.async.run` to complete all its
/// spawned tasks before resuming.
#[derive(Debug)]
struct JoinWait {
    /// The suspended caller task.
    task: Task,
    /// Ids of the spawned tasks that have **not** completed yet.  A completion
    /// counts against the waiter whose `pending` set contains the finished
    /// task's root-frame id — never against a sibling completing at the same
    /// boundary.  Empty means the join is already satisfied.
    pending: Vec<u64>,
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
    /// exposed to Fleen through `std.os.args` (without `argv[0]`).
    pub fn with_args(module: Module, program_args: Vec<String>) -> Self {
        let n_globals = module.globals.len();
        Vm {
            module,
            stack: Vec::new(),
            frames: Vec::new(),
            globals: vec![Value::Unit; n_globals],
            program_args,
            scheduler: Scheduler::new(),
            pending: None,
            need_join: false,
            sleep_deadline: None,
            nested_run_expected: None,
            waiting_join: Vec::new(),
            suspended_on_channel: false,
            channel_waiters: Vec::new(),
            next_task_id: 0,
        }
    }

    /// Monotonic clock in milliseconds for deadline calculations.
    /// Uses a process-wide start instant so all VM instances share the same
    /// timebase (avoids clock skew between multiple VMs in the same test).
    fn now_mono_ms(&self) -> i64 {
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_millis() as i64
    }

    /// Run the module's `entry` function to completion.
    ///
    /// Accepts bytecode versions 1 (0.0.1), 2 (0.0.2), 3 (0.0.3) and 4
    /// (0.0.4). On
    /// failure the [`RuntimeError`] carries the source location of the
    /// failing instruction when one can be determined (0.0.2 U10).
    pub fn run(&mut self) -> Result<Value, RuntimeError> {
        self.run_inner().map_err(|kind| self.locate_error(kind))
    }

    fn run_inner(&mut self) -> Result<Value, RuntimeErrorKind> {
        if !(1..=4).contains(&self.module.version) {
            return Err(RuntimeErrorKind::UnsupportedVersion(self.module.version));
        }
        let entry = self.module.entry;
        self.call(entry, 0)?;
        loop {
            // A `Suspend` parks the current task in `self.pending`; park it and
            // remember the caller frame must not continue until the scheduler
            // (its join) drains. The deadline (if any) was already attached to
            // the task by the `Suspend` dispatch, keyed to the exact suspend point.
            if let Some(task) = self.pending.take() {
                match self.nested_run_expected.take() {
                    // Nested `std.async.run` from an async body: the suspended
                    // caller must wait for every spawned task to complete.
                    Some(ids) if !ids.is_empty() => {
                        self.waiting_join.push(JoinWait { task, pending: ids });
                    }
                    // Synchronous caller path, or `run([])` — nothing to wait
                    // for; the parked task re-enters the scheduler directly.
                    _ => self.scheduler.enqueue(task),
                }
                self.need_join = true;
            }
            // A channel park (0.0.4 T07) detached the task's chain into the
            // channel's sender/receiver queue — the waker owns it now — so
            // there is no `pending` task; only the join signal remains.
            if self.suspended_on_channel {
                self.suspended_on_channel = false;
                self.need_join = true;
            }
            // A re-drive boundary (a top-level `await` whose root suspended, or
            // a `std.async.run` build that spawned tasks): the synchronous
            // caller frame is parked at its join, so run every task in the
            // scheduler to completion before stepping it again.
            if self.need_join {
                self.need_join = false;
                self.drive_join()?;
                continue;
            }
            if self.frames.is_empty() {
                // The entry frame starts at base 0 and `Return` truncates to the
                // base and pushes its value back, so after full unwinding only
                // the module result (if any) can remain.
                debug_assert!(
                    self.stack.len() <= 1,
                    "stray operands left on the stack after unwinding"
                );
                return Ok(self.stack.pop().unwrap_or(Value::Unit));
            }
            self.step()?;
        }
    }

    /// Run every task in the scheduler to completion — the *join* at a
    /// synchronous boundary (`main` awaiting a root, or `std.async.run`).
    ///
    /// The synchronous caller frame (below all tasks on the shared stack) stays
    /// exactly where it is; each task is resumed onto the stack one at a time,
    /// driven until it completes (`Return` to the sync caller) or yields again
    /// (`Suspend` re-parks it, with the round-robin continuing). When the
    /// scheduler empties the awaited future's payload is already sitting on the
    /// caller's stack where its `Await`/`Call` expects it, so `run_inner` can
    /// safely resume stepping the caller.
    fn drive_join(&mut self) -> Result<(), RuntimeErrorKind> {
        // Tasks parked on channels (0.0.4 T07) are not in the scheduler —
        // their chains live in the channel's sender/receiver queues — but
        // they are live work: keep the join going while any exist.
        while !self.scheduler.is_empty()
            || !self.waiting_join.is_empty()
            || self.has_blocked_channel_tasks()
        {
            // Drop waiter roots whose channels no longer have parked tasks:
            // without pruning the registry would grow for the program's
            // lifetime (one stale `Rc` per channel ever parked on), pinning
            // `ChannelInner` buffers that are otherwise unreachable.
            self.prune_channel_waiters();
            // Ready tasks always win; only when nothing is runnable does the
            // scheduler block on the earliest pending timer (see
            // `Scheduler::next_task`), so a task that has not yet reached its
            // own sleep is never starved behind an earlier timeout.
            let Some(task) = self.scheduler.next_task() else {
                // No ready tasks and no timers at all: nothing can ever run
                // again, so any task still parked on a channel is deadlocked
                // (T07b — no `close` in 0.0.4, so "everyone blocked" is final).
                if self.has_blocked_channel_tasks() {
                    return Err(RuntimeErrorKind::ChannelDeadlock);
                }
                break;
            };
            // A completion counts against the waiter waiting for *this* task
            // (by root-frame id) — a sibling finishing at the same boundary
            // must not release a nested join early.  Captured before the
            // chain is moved onto the stack.
            let task_id = task.chain.first().map_or(0, |f| f.task_id);
            self.resume_chain(task.chain)?;
            loop {
                // The task parked on a channel: its chain moved into the
                // channel's queue (the waker owns it), so unlike a `Suspend`
                // there is no task to re-enqueue — and this is *not* a
                // completion: a nested join must not count it.
                if self.suspended_on_channel {
                    self.suspended_on_channel = false;
                    break;
                }
                if let Some(task) = self.pending.take() {
                    // A `Suspend` detached the whole task chain. If this was
                    // triggered by a nested `std.async.run` from an async body,
                    // hold the task in `waiting_join` instead of re-enqueueing.
                    match self.nested_run_expected.take() {
                        Some(ids) if !ids.is_empty() => {
                            self.waiting_join.push(JoinWait { task, pending: ids });
                        }
                        // Not a nested join, or `run([])` (nothing to wait
                        // for): the task re-enters the scheduler directly.
                        _ => self.scheduler.enqueue(task),
                    }
                    break;
                }
                match self.frames.last() {
                    // A future-marked frame is live — keep driving the task.
                    Some(f) if f.is_future => self.step()?,
                    // Back at the synchronous join caller: the task completed.
                    _ => {
                        self.complete_task(task_id);
                        break;
                    }
                }
            }
        }
        // The exit evaluation of the loop condition does not run the body, so
        // a final unpark inside the last iteration would leave one stale root
        // behind — prune once more on the way out.
        self.prune_channel_waiters();
        Ok(())
    }

    /// Drop `channel_waiters` roots whose channel has no parked task left
    /// (T07). A root is only needed while a chain is parked inside the
    /// `ChannelInner`; `remember_channel_waiter` re-adds one on the next park.
    fn prune_channel_waiters(&mut self) {
        self.channel_waiters.retain(|v| {
            matches!(v, Value::Channel(inner)
            if {
                let ch = inner.borrow();
                !ch.senders.is_empty() || !ch.receivers.is_empty()
            })
        });
    }

    /// Number of rooted waiter channels — test observation point for the
    /// pruning above (T07).
    #[cfg(test)]
    pub(crate) fn channel_waiter_count(&self) -> usize {
        self.channel_waiters.len()
    }

    /// Count a completed task against the nested-join waiter waiting for it
    /// (0.0.4 T06).  A task's identity is its root frame's `task_id`, which
    /// survives every park/wake cycle; the waiter whose `pending` set contains
    /// that id loses one entry, and when its set empties the waiter's own
    /// chain re-enters the scheduler.  A completion no waiter is waiting for
    /// (a sibling task finishing at the same boundary, or a waiter itself
    /// after its join released) is simply ignored — a completion must never
    /// be counted twice, and never against the wrong join.
    fn complete_task(&mut self, task_id: u64) {
        let Some(pos) = self
            .waiting_join
            .iter()
            .position(|w| w.pending.contains(&task_id))
        else {
            return;
        };
        self.waiting_join[pos].pending.retain(|&id| id != task_id);
        if self.waiting_join[pos].pending.is_empty() {
            // All spawned tasks done; re-enqueue the waiter (moved, never
            // copied) and drop its join record.
            let wait = self.waiting_join.remove(pos);
            self.scheduler.enqueue(wait.task);
        }
    }

    /// Check if any task is blocked on a channel (waiting in senders/receivers
    /// queues). Scans the operand stack, all live frames' locals/operands,
    /// and all suspended frames in the scheduler's ready/timer queues.
    /// Returns true if at least one channel has waiters.
    fn has_blocked_channel_tasks(&self) -> bool {
        // The waiter registry roots every channel with parked tasks — a
        // parked task's own frame no longer holds the handle, so without it
        // such a channel could be unreachable from any live value.
        if self
            .channel_waiters
            .iter()
            .any(|v| self.value_has_blocked_channel(v))
        {
            return true;
        }
        // Scan the operand stack.
        if self.stack_has_blocked_channel() {
            return true;
        }
        // Scan live call frames' locals and operands.
        for frame in &self.frames {
            let n_locals = self.func_locals(frame.func).unwrap_or(0);
            let base = frame.stack_base;
            if base + n_locals <= self.stack.len() {
                let locals = &self.stack[base..base + n_locals];
                if self.slice_has_blocked_channel(locals) {
                    return true;
                }
            }
            let operands = &self.stack[base + n_locals..];
            if self.slice_has_blocked_channel(operands) {
                return true;
            }
        }
        // Scan suspended frames in the scheduler's ready and timer queues.
        for task in self.scheduler.iter_tasks() {
            for frame in &task.chain {
                if self.frame_has_blocked_channel(frame) {
                    return true;
                }
            }
        }
        // Scan any nested-join waiters' parked chains.
        for wait in &self.waiting_join {
            for frame in &wait.task.chain {
                if self.frame_has_blocked_channel(frame) {
                    return true;
                }
            }
        }
        false
    }

    fn stack_has_blocked_channel(&self) -> bool {
        self.slice_has_blocked_channel(&self.stack)
    }

    fn slice_has_blocked_channel(&self, slice: &[Value]) -> bool {
        slice.iter().any(|v| self.value_has_blocked_channel(v))
    }

    fn frame_has_blocked_channel(&self, frame: &FutureFrame) -> bool {
        self.slice_has_blocked_channel(&frame.locals)
            || self.slice_has_blocked_channel(&frame.operands)
    }

    fn value_has_blocked_channel(&self, value: &Value) -> bool {
        match value {
            Value::Channel(inner) => {
                let ch = inner.borrow();
                !ch.senders.is_empty() || !ch.receivers.is_empty()
            }
            Value::Boxed(v) => self.value_has_blocked_channel(v),
            Value::Ok(v) => self.value_has_blocked_channel(v),
            Value::Err(v) => self.value_has_blocked_channel(v),
            Value::Struct(fields) => fields.iter().any(|f| self.value_has_blocked_channel(f)),
            Value::Arr(elems) => elems.iter().any(|e| self.value_has_blocked_channel(e)),
            Value::Range { .. } => false,
            Value::Future(f) => self.frame_has_blocked_channel(f),
            _ => false,
        }
    }

    /// Restore a suspended task chain (outermost first) as the live frames.
    ///
    /// Each frame is installed like `resume_frame` — locals/operands moved onto
    /// the top of the shared stack at whatever depth currently exists, marked
    /// `is_future`. Nested `await`s resume naturally: the outermost frame runs,
    /// and its body's own `Await`/`Poll` resumes the next frame when it reaches
    /// them. `frames` only ever holds the synchronous join caller below a
    /// single task (run_inner/drive_join framing), so a resumed chain never
    /// overlaps another task.
    fn resume_chain(&mut self, chain: Vec<FutureFrame>) -> Result<(), RuntimeErrorKind> {
        for frame in chain {
            self.resume_frame(frame)?;
        }
        Ok(())
    }

    /// Detach the running task's suspended frames from the shared stack, from
    /// the innermost frame outward, stopping at the first *synchronous* frame
    /// (a `CallFrame` with `is_future == false` — the `std.async.run` join
    /// caller or the top-level `main`). Returns the detached chain in resume
    /// order (outermost first, innermost last).
    ///
    /// This is the chain analogue of T04's single-frame `suspend_top_frame`: it
    /// keeps T04's guarantees (each value is **moved**, never copied, so a value
    /// has exactly one owner; `Drop` releases a parked task that is never
    /// resumed) but records every future frame above a synchronous boundary so a
    /// task that `await`s a nested future which suspends resumes correctly.
    fn suspend_chain(&mut self, resume_pc: usize) -> Result<Vec<FutureFrame>, RuntimeErrorKind> {
        let mut frames = Vec::new();
        let mut first = true;
        while let Some(frame) = self.frames.last() {
            if !frame.is_future {
                break;
            }
            // For the innermost frame (where Suspend fired), resume at the
            // instruction after Suspend (the passed `resume_pc`). For outer
            // frames, their `frame.ip` was already advanced by their `Await`
            // instruction, so resume at that ip.
            let this_resume_pc = if first { resume_pc } else { frame.ip };
            let resumed = self.suspend_top_frame(this_resume_pc)?;
            frames.push(resumed);
            first = false;
        }
        // Reverse the innermost-last walk into resume order (outermost first).
        // Empty means the suspend fired with no future frame live — `Suspend`
        // is only reachable from an async body (always future-marked), so this
        // is the defensive backstop for hand-written bytecode / sync misuse.
        if frames.is_empty() {
            return Err(RuntimeErrorKind::StackShape);
        }
        Ok(frames.into_iter().rev().collect())
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
    pub(crate) fn call(&mut self, fid: FuncId, argc: u16) -> Result<(), RuntimeErrorKind> {
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
            let ret = self.exec_builtin(&name, args)?;
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
        // Inherit `is_future` from the caller: a sync function called from an
        // async task is part of that task and must be suspended along with it
        // so that nested `await`s in the callee suspend the entire chain.
        let is_future = self.frames.last().map(|f| f.is_future).unwrap_or(false);
        self.frames.push(CallFrame {
            func: fid,
            ip: 0,
            stack_base: base,
            is_future,
            // Task identity stays on the chain's root frame (installed by
            // `resume_frame` from a spawned `Future`); an ordinary call is
            // never the root.
            task_id: 0,
        });
        Ok(())
    }

    /// `Func::locals` of `fid`, the width a frame's local area must have.
    fn func_locals(&self, fid: FuncId) -> Result<usize, RuntimeErrorKind> {
        let f = self
            .module
            .functions
            .get(fid.0 as usize)
            .ok_or(RuntimeErrorKind::FuncOutOfRange(fid))?;
        Ok(f.locals as usize)
    }

    /// Snapshot the innermost frame into a [`FutureFrame`] and detach it
    /// (0.0.4 T04, D1) — the mechanism behind `Suspend` (T05).
    ///
    /// `resume_pc` is where the frame continues: the instruction *after* the
    /// suspension point, which the dispatch already knows because it decoded
    /// the operands.  The frame's local slots and its operand suffix are
    /// **moved** out of the shared stack, never copied, so each value keeps
    /// exactly one owner; if the future is dropped instead of resumed, Rust
    /// `Drop` releases them (BYTECODE.md §3.2: no GC).
    ///
    /// The frame must be the innermost one: `Suspend` is an instruction of the
    /// async function itself, so no callee frame can be live above it.  A
    /// borrow handle can never appear in the snapshot (D2 rejects `ref` inside
    /// an async fn); the assertion is the guard for hand-written bytecode.
    pub fn suspend_top_frame(&mut self, resume_pc: usize) -> Result<FutureFrame, RuntimeErrorKind> {
        let frame = self.frames.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
        let n_locals = self.func_locals(frame.func)?;
        if frame.stack_base + n_locals > self.stack.len() {
            return Err(RuntimeErrorKind::StackShape);
        }
        let mut locals = self.stack.split_off(frame.stack_base);
        let operands = locals.split_off(n_locals);
        debug_assert!(
            !locals
                .iter()
                .chain(operands.iter())
                .any(|v| matches!(v, Value::Ref { .. })),
            "a suspended frame must not capture a borrow handle: Ref stores an absolute \
             operand-stack index that cannot survive a snapshot (D2 rejects ref in async fn)"
        );
        Ok(FutureFrame {
            func: frame.func,
            resume_pc,
            locals,
            operands,
            state: FutureState::Resumable,
            // Preserve the task id: the chain's outermost frame is the task
            // root, and its id must survive every snapshot/resume round trip
            // for nested joins to recognize their spawned tasks.
            task_id: frame.task_id,
        })
    }

    /// Restore a [`FutureFrame`] as the innermost call frame (0.0.4 T04).
    ///
    /// The snapshot's locals and operands move onto the top of the shared
    /// stack at *whatever depth the stack currently has*, and the new
    /// `CallFrame::stack_base` is computed from that.  Because the frame stores
    /// no `stack_base`, resuming at a different depth — or later on another
    /// worker thread (T06) — is sound.
    ///
    /// Only a [`FutureState::Resumable`] frame may be restored: a `Future` is
    /// driven once (PLAN §3.3), so resuming a completed or failed frame is the
    /// defensive [`RuntimeErrorKind::FutureNotResumable`].
    pub fn resume_frame(&mut self, frame: FutureFrame) -> Result<(), RuntimeErrorKind> {
        if !matches!(frame.state, FutureState::Resumable) {
            return Err(RuntimeErrorKind::FutureNotResumable);
        }
        if self.frames.len() >= MAX_CALL_DEPTH {
            return Err(RuntimeErrorKind::CallDepthExceeded);
        }
        let n_locals = self.func_locals(frame.func)?;
        // Defensive: snapshots are produced by `suspend_top_frame` or by the
        // async call itself, both sized from `Func::locals`.
        if frame.locals.len() != n_locals {
            return Err(RuntimeErrorKind::StackShape);
        }
        let stack_base = self.stack.len();
        self.frames.push(CallFrame {
            func: frame.func,
            ip: frame.resume_pc,
            stack_base,
            // A `FutureFrame` only ever comes from `suspend_chain` (which walks
            // the live `is_future` frames) or from `AsyncStart` (an async body),
            // so a restored frame is always part of a task's suspendable chain —
            // including a synchronous frame the chain swallowed because it was
            // called from an async body.
            is_future: true,
            // `0` for a not-yet-spawned frame (`AsyncStart`); a snapshot of a
            // task root carries its spawned id through.
            task_id: frame.task_id,
        });
        let mut locals = frame.locals;
        self.stack.append(&mut locals);
        self.stack.extend(frame.operands);
        Ok(())
    }

    fn exec_builtin(&mut self, name: &str, args: Vec<Value>) -> Result<Value, RuntimeErrorKind> {
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
            // 0.0.4 T06: cooperative executor builtins. `std.async.run` spawns
            // multiple root tasks; `std.io.sleep`/`std.async.yield_now` suspect
            // the running task (the compiler emits a `Suspend` right after their
            // call inside an async body — see `fleen-compiler/src/lower` — and
            // the VM's `Suspend` dispatch + `run_inner` park the task).
            "std.async.run" => {
                // `run(tasks: [Future<unit>])`: hand every element to the
                // scheduler as a ready root task and signal a join. Each `Future`
                // owns a not-yet-started or suspended `FutureFrame`; we move the
                // frames out of the argument array (never copy — a Future is
                // non-Copy and holds lifted suspend state, D1). Typeck guarantees
                // `[Future<unit>]`, so a non-Future element is malformed/defensive.
                let mut elems = match args.into_iter().next() {
                    Some(Value::Arr(elems)) => elems.into_vec(),
                    _ => return Err(RuntimeErrorKind::StackShape),
                };
                // Give each spawned root a task id (its chain's root frame
                // carries it across every park/wake cycle) and remember the
                // ids as the join's pending set — completions are matched by
                // id, never by counting, so a sibling finishing at the same
                // boundary cannot release the join early.
                let mut spawned_ids = Vec::with_capacity(elems.len());
                for elem in elems.drain(..) {
                    let Value::Future(boxed) = elem else {
                        return Err(RuntimeErrorKind::StackShape);
                    };
                    let mut frame = *boxed;
                    self.next_task_id += 1;
                    let id = self.next_task_id;
                    frame.task_id = id;
                    spawned_ids.push(id);
                    self.scheduler.push_ready(Task {
                        chain: vec![frame],
                        ready_at_ms: None,
                    });
                }
                // If called from inside an async body (future-marked frame),
                // record the expected task count — but do NOT signal the join
                // here. The compiler emits a `Suspend` immediately after this
                // call (T06 contract); that Suspend parks the caller and
                // `run_inner` builds the `JoinWait` (setting `need_join`
                // itself). Signaling here instead would start `drive_join`
                // while the caller is still a live frame: the spawned tasks
                // would complete "into" the future-marked caller (never
                // counted by the JoinWait), and the caller's own Suspend
                // would then create a JoinWait that never fills — the caller
                // parks forever and the awaiting caller below loses its
                // value (a StackShape at the next store).
                if self.frames.last().is_some_and(|f| f.is_future) {
                    self.nested_run_expected = Some(spawned_ids);
                } else {
                    // Synchronous caller (main): nothing will park it, so the
                    // join must be signaled by the builtin itself.
                    self.need_join = true;
                }
                Ok(Value::Unit)
            }
            "std.io.sleep" => {
                // `sleep(n: int)` — awaitable timer. Record the wake deadline for
                // the task that runs the immediately-following `Suspend`; the
                // `run_inner`/`drive_join` park step copies it onto the parked
                // task. Only set when executing inside an async (future-marked)
                // frame — a synchronous caller of `sleep` (the compiler emits no
                // `Suspend`) is a no-op, and this guard keeps a stale deadline
                // from leaking into a later unrelated suspension. Returns `unit`.
                let millis = match args.first() {
                    Some(Value::Int(n)) if *n >= 0 => *n,
                    // Negative (or non-int) sleep behaves as a zero delay; typeck
                    // guarantees an int, so a non-int is defensive.
                    Some(Value::Int(_)) => return Ok(Value::Unit),
                    _ => return Err(RuntimeErrorKind::StackShape),
                };
                if let Some(frame) = self.frames.last().filter(|f| f.is_future) {
                    // Record the deadline keyed by the exact suspension point:
                    // (function id, instruction pointer of the Suspend that follows).
                    // The Call to sleep has already advanced the caller's ip to the
                    // next instruction (the Suspend), so `frame.ip` is that ip.
                    let now = self.now_mono_ms();
                    let fid = frame.func.0 as usize;
                    let ip = frame.ip;
                    self.sleep_deadline = Some((fid, ip, now + millis));
                }
                Ok(Value::Unit)
            }
            "std.async.yield_now" => {
                // Cooperative yield: no deadline, so the task is immediately
                // re-runnable — the scheduler round-robins to the next task
                // before coming back. Runs inside an async body.
                Ok(Value::Unit)
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

    pub(crate) fn step(&mut self) -> Result<(), RuntimeErrorKind> {
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
            // 0.0.4 T05: async / await (bytecode v4, 0xC0–0xC3). These
            // operate on the coroutine frame machinery in `frame.rs`
            // (T04): `AsyncStart` produces a not-yet-started Future and
            // returns it to the caller; `Poll`/`Await` drive a Future to
            // completion (`resume_frame` + the body's own `Return` delivers
            // the payload). `Suspend` is the yield boundary that the T06
            // executor and T07 channels wire; it is unreachable from 0.0.4
            // T05 lowering (there is no suspension source), so reaching it
            // here is a malformed/unsupported program, reported defensively.
            Opcode::AsyncStart => {
                // `AsyncStart` is the head of an `async func` body. Called
                // via a normal `Call`, it packages the frame's parameter
                // slots into a not-yet-started coroutine frame (T04 `D1`:
                // `resume_pc = ip`, i.e. the body entry right after this
                // instruction) and returns that `Future` value to the
                // caller. It must be a leaf (frame-last) instruction: the
                // body that follows is driven later by `Poll`/`Await`.
                let frame = *self.frames.last().ok_or(RuntimeErrorKind::StackUnderflow)?;
                let params = self.module.functions[frame.func.0 as usize].params as usize;
                let locals = self.func_locals(frame.func)?;
                let args: Vec<Value> = (0..params)
                    .map(|i| {
                        self.stack
                            .get(frame.stack_base + i)
                            .cloned()
                            .ok_or(RuntimeErrorKind::StackShape)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let mut slots = args;
                slots.resize(locals, Value::Unit);
                let future = FutureFrame {
                    func: frame.func,
                    resume_pc: ip, // body entry = the instruction after AsyncStart
                    locals: slots,
                    operands: Vec::new(),
                    state: FutureState::Resumable,
                    // Not a spawned task yet; `std.async.run` assigns the id
                    // when (and if) it turns this frame into a root task.
                    task_id: 0,
                };
                // Unwind this call like `Return` (deliver the Future to the
                // caller). No callee frame may be live above an `AsyncStart`.
                let done = self.frames.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                self.stack.truncate(done.stack_base);
                self.stack.push(Value::Future(Box::new(future)));
                return Ok(());
            }
            Opcode::Suspend => {
                // Cooperative yield boundary (0.0.4 T06): detach the running
                // task's whole suspended-frame chain from the shared stack and
                // hand it to `run_inner`, which parks it in the scheduler and
                // runs the next task. Unlike T05's defensive error, this is now
                // the real suspension path reached by `yield_now`/`sleep`/and,
                // from T07, channel send/recv. The chain is moved (never copied)
                // and resumes at `ip` (the instruction after this one) via
                // `resume_chain`.
                // Capture the current frame's identity BEFORE suspend_chain pops it.
                let suspend_frame = self.frames.last().copied();
                let chain = self.suspend_chain(ip)?;
                // Consume a sleep deadline only if it was recorded for this
                // exact suspension point (function + ip). A stale deadline from a
                // sync helper's `sleep` (which emits no Suspend) must not leak.
                let ready_at_ms = if let Some(frame) = suspend_frame {
                    if let Some((func_id, suspend_ip, deadline)) = self.sleep_deadline {
                        // The frame's ip points to the Suspend instruction (it's
                        // advanced at the end of `step`). The deadline was recorded
                        // with this same ip at the `sleep` builtin (where the caller's
                        // ip had already been advanced to the Suspend by the Call).
                        let cur_fid = frame.func.0 as usize;
                        let cur_ip = frame.ip;
                        if func_id == cur_fid && suspend_ip == cur_ip {
                            self.sleep_deadline = None;
                            Some(deadline)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    self.sleep_deadline = None;
                    None
                };
                self.pending = Some(Task { chain, ready_at_ms });
                return Ok(());
            }
            Opcode::Poll | Opcode::Await => {
                // Drive a `Future` to completion. Pop the Future, then advance
                // *this* (the awaiting) frame's ip past the instruction so the
                // resumed body — which `resume_frame` pushes above it — hands
                // control back to the instruction after `Await` once its own
                // `Return` unwinds. `Call` does the same advance before pushing
                // a callee frame (the U09 ip-rewind lesson).
                let fut = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
                let Value::Future(f) = fut else {
                    return Err(RuntimeErrorKind::FutureExpected);
                };
                self.frames
                    .last_mut()
                    .ok_or(RuntimeErrorKind::StackUnderflow)?
                    .ip = ip;
                self.resume_frame(*f)?;
                return Ok(());
            }
            // 0.0.4 T07: channel operations (0xC4–0xC7).
            Opcode::ChannelNew => {
                // Stack: cap -> ch
                let cap = self.pop_int()?;
                if cap < 0 {
                    return Err(RuntimeErrorKind::InvalidChannelCapacity);
                }
                let inner = Rc::new(RefCell::new(ChannelInner {
                    cap: cap as usize,
                    buffer: std::collections::VecDeque::new(),
                    senders: std::collections::VecDeque::new(),
                    receivers: std::collections::VecDeque::new(),
                }));
                self.stack.push(Value::Channel(inner));
            }
            Opcode::ChannelSend => {
                // Stack: ch v -> (unit). `true` = the task parked on this
                // send (its chain is in the channel's sender queue, so the
                // frame no longer exists and the ip tail must not run);
                // `false` = completed, fall through to advance the ip.
                if self.do_channel_send(ip)? {
                    return Ok(());
                }
            }
            Opcode::ChannelRecv => {
                // Stack: ch -> v (may suspend; same contract as send).
                if self.do_channel_recv(ip)? {
                    return Ok(());
                }
            }
            Opcode::ChannelSplit => {
                // Stack: ch -> ch2
                let inner = self.pop_channel_handle()?;
                // Split returns a second handle to the same inner buffer.
                self.stack.push(Value::Channel(inner));
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

    fn pop_int(&mut self) -> Result<i64, RuntimeErrorKind> {
        let v = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
        match v {
            Value::Int(n) => Ok(n),
            _ => Err(RuntimeErrorKind::ArithTypeMismatch),
        }
    }

    /// 0.0.4 T07: `channel.send(value)` — may suspend when no receiver is
    /// waiting and (for a buffered channel) the buffer is full.
    ///
    /// The send expression has type `unit`, so on every completion path a
    /// `Value::Unit` is left on the stack (the statement wrapper pops it).
    /// Returns `Ok(true)` when the task parked: its whole frame chain was
    /// detached into the channel's sender queue with the `Unit` already on
    /// top, and the op is *complete* on resume — the frame resumes at the
    /// instruction after `ChannelSend`. `Ok(false)` means the value was
    /// accepted (handed to a parked receiver or buffered) and the sender
    /// continues in place.
    fn do_channel_send(&mut self, ip: usize) -> Result<bool, RuntimeErrorKind> {
        let value = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
        let inner = self.pop_channel_handle()?;
        let mut ch = inner.borrow_mut();

        // A parked receiver is waiting: hand the value over directly (the
        // buffer is necessarily empty — a receiver only parks then) and wake
        // it. The receiver's chain resumes *after* its `recv` instruction,
        // so the payload is injected into its innermost frame's operands.
        if let Some(receiver_chain) = ch.receivers.pop_front() {
            let chain = Self::inject_value_into_chain(receiver_chain, value);
            self.scheduler.push_ready(crate::executor::Task {
                chain,
                ready_at_ms: None,
            });
            self.stack.push(Value::Unit);
            return Ok(false);
        }

        // Buffered channel with room: park the value in the buffer (FIFO).
        if ch.cap > 0 && ch.buffer.len() < ch.cap {
            ch.buffer.push_back(value);
            self.stack.push(Value::Unit);
            return Ok(false);
        }

        // Rendezvous with no receiver, or full buffer: park the sender. The
        // payload moves into the queue together with the chain; the `Unit`
        // result stays on the (detached) operand stack so the resumed frame
        // continues as if the send had just completed.
        self.stack.push(Value::Unit);
        let chain = self.park_on_channel(ip)?;
        self.remember_channel_waiter(&inner);
        ch.senders.push_back((value, chain));
        Ok(true)
    }

    /// Pop a channel handle off the operand stack (0.0.4 T07). `StackShape`
    /// is the defensive backstop for malformed bytecode — typeck only
    /// produces `ChannelSend`/`ChannelRecv`/`ChannelSplit` under a
    /// `channel<T>` receiver, and the shared `ChannelInner` root is what
    /// all three operate on.
    fn pop_channel_handle(&mut self) -> Result<Rc<RefCell<ChannelInner>>, RuntimeErrorKind> {
        let v = self.stack.pop().ok_or(RuntimeErrorKind::StackUnderflow)?;
        match v {
            Value::Channel(inner) => Ok(inner),
            _ => Err(RuntimeErrorKind::StackShape),
        }
    }

    /// Root a channel that just gained a parked task (T07): one handle per
    /// `ChannelInner`, deduplicated by pointer so repeated park/unpark cycles
    /// on the same channel do not grow the registry.
    fn remember_channel_waiter(&mut self, inner: &Rc<RefCell<ChannelInner>>) {
        if !self
            .channel_waiters
            .iter()
            .any(|v| matches!(v, Value::Channel(existing) if Rc::ptr_eq(existing, inner)))
        {
            self.channel_waiters.push(Value::Channel(Rc::clone(inner)));
        }
    }

    /// Detach the running task's frame chain for a channel park, guarding the
    /// synchronous cases (0.0.4 T07b): a `send`/`recv` that must block is only
    /// executable inside an async body (a future-marked innermost frame). In a
    /// *synchronous* caller nothing else can ever run to satisfy the op —
    /// that is the T07b deadlock, raised here directly. A sync helper called
    /// *from* an async body cannot be snapshotted (its `CallFrame` is
    /// mid-execution), which is the defensive `StackShape`.
    fn park_on_channel(&mut self, ip: usize) -> Result<Vec<FutureFrame>, RuntimeErrorKind> {
        if !self.frames.last().is_some_and(|f| f.is_future) {
            if self.frames.iter().any(|f| f.is_future) {
                return Err(RuntimeErrorKind::StackShape);
            }
            return Err(RuntimeErrorKind::ChannelDeadlock);
        }
        self.suspended_on_channel = true;
        self.suspend_chain(ip)
    }

    /// Inject a value into the innermost frame's operands of a suspended chain.
    /// Used for the channel wake paths: the receiver's frame was suspended
    /// *after* its `recv` instruction with the channel handle popped, so the
    /// payload goes on top of its operands and the frame resumes holding it.
    fn inject_value_into_chain(mut chain: Vec<FutureFrame>, value: Value) -> Vec<FutureFrame> {
        if let Some(innermost) = chain.last_mut() {
            innermost.operands.push(value);
        }
        chain
    }

    /// 0.0.4 T07: `channel.recv()` — may suspend when no sender is waiting
    /// and the buffer is empty.
    ///
    /// Returns `Ok(true)` when the task parked (its chain is in the channel's
    /// receiver queue; the waker injects the payload — see
    /// [`Self::inject_value_into_chain`]). `Ok(false)` means a value was
    /// produced and pushed; the receiver continues in place.
    fn do_channel_recv(&mut self, ip: usize) -> Result<bool, RuntimeErrorKind> {
        let inner = self.pop_channel_handle()?;
        let mut ch = inner.borrow_mut();

        // Buffered value available: take it (FIFO). If a sender is parked the
        // buffer was full — its payload now fits into the freed slot.
        if ch.cap > 0
            && let Some(value) = ch.buffer.pop_front()
        {
            self.stack.push(value);
            if let Some((parked_value, sender_chain)) = ch.senders.pop_front() {
                ch.buffer.push_back(parked_value);
                self.scheduler.push_ready(crate::executor::Task {
                    chain: sender_chain,
                    ready_at_ms: None,
                });
            }
            return Ok(false);
        }

        // No buffered value (rendezvous, or an emptied buffer): take a parked
        // sender's payload and wake it — its send is already complete.
        if let Some((value, sender_chain)) = ch.senders.pop_front() {
            self.stack.push(value);
            self.scheduler.push_ready(crate::executor::Task {
                chain: sender_chain,
                ready_at_ms: None,
            });
            return Ok(false);
        }

        // Nothing to receive: park the receiver. On wake a sender injects the
        // payload into the innermost frame's operands (the resume is *after*
        // this instruction, with the channel handle already popped).
        let chain = self.park_on_channel(ip)?;
        self.remember_channel_waiter(&inner);
        ch.receivers.push_back(chain);
        Ok(true)
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
