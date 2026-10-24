//! Cooperative task executor (0.0.4 T06, F6).
//!
//! Fleen's concurrency is **cooperative and single-threaded**: a task suspends
//! only when a suspension source (a `sleep` timer or `yield_now`, later a
//! channel send/recv in T07) executes inside an async body, and when one task
//! suspends the executor runs the next runnable task. There is no preemption
//! and no shared memory (D3), so a single worker thread is semantically
//! complete for every 0.0.4 feature — the multi-thread worker pool is a later
//! optimization, not a semantic gap.
//!
//! A *task* is a chain of suspended coroutine frames (`Vec<FutureFrame>`, in
//! resume order outermost-first, innermost-last). A root task is a single async
//! body; a task that `await`s a nested future that itself suspends carries more
//! than one suspended frame in its chain. Between tasks the frames are fully
//! detached from the shared operand stack (`suspend_chain` / `resume_chain`),
//! so only one task ever occupies the worker's stack at a time.
//!
//! The scheduler owns runnable tasks and timer-parked tasks. [`Scheduler::next_task`]
//! yields the next runnable task (blocking on a timer deadline when none is
//! runnable), so the `Vm::run` loop can drive tasks to completion in a
//! deterministic round-robin order.

use std::collections::VecDeque;

use crate::frame::FutureFrame;

/// A cooperative task: the suspended-frame chain of one async execution.
///
/// Resume order is outer-most-first — driving `Task::chain` means pushing
/// `chain[0]`, running it (which may itself resume `chain[1]`, … via nested
/// `await`), never jumping out of order. A root task pushed by
/// `std.async.run` has a single element.
#[derive(Debug, Clone)]
pub struct Task {
    /// Suspended coroutine frames, outermost-first, innermost-last.
    pub chain: Vec<FutureFrame>,
    /// A timer deadline, if this task is parked until the clock passes it.
    pub ready_at_ms: Option<i64>,
}

/// Cooperative scheduler over runnable and timer-parked tasks (0.0.4 T06).
#[derive(Debug, Default)]
pub struct Scheduler {
    /// Tasks that can run now, in FIFO order.
    ready: VecDeque<Task>,
    /// Tasks parked until their `ready_at_ms` deadline. Small by design (no
    /// `select`/`join` fan-out in 0.0.4); a linear scan is fine.
    timers: Vec<Task>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of tasks the scheduler still owns (ready or parked) — zero means
    /// `std.async.run` has nothing left to join.
    pub fn is_empty(&self) -> bool {
        self.ready.is_empty() && self.timers.is_empty()
    }

    /// Enqueue a task, routing it to the ready queue (no deadline) or the timer
    /// park (has a `ready_at_ms`).
    pub fn enqueue(&mut self, task: Task) {
        if task.ready_at_ms.is_some() {
            self.timers.push(task);
        } else {
            self.ready.push_back(task);
        }
    }

    /// Enqueue a task that can run immediately.
    pub fn push_ready(&mut self, task: Task) {
        self.ready.push_back(task);
    }

    /// Every task the scheduler owns (ready and timer-parked), in no
    /// particular order. Read-only inspection for the T07 channel-deadlock
    /// scan (`Vm::has_blocked_channel_tasks`) — the queues themselves stay
    /// private to the scheduler.
    pub fn iter_tasks(&self) -> impl Iterator<Item = &Task> {
        self.ready.iter().chain(self.timers.iter())
    }

    /// The current monotonic clock in milliseconds.
    fn now_ms(&self) -> i64 {
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_millis() as i64
    }

    /// Promote every timer task whose deadline has passed onto the ready queue.
    fn promote_timers(&mut self, now_ms: i64) {
        let mut i = 0;
        while i < self.timers.len() {
            let due = self.timers[i].ready_at_ms.is_some_and(|d| d <= now_ms);
            if due {
                let mut task = self.timers.swap_remove(i);
                task.ready_at_ms = None;
                self.ready.push_back(task);
            } else {
                i += 1;
            }
        }
    }

    /// The earliest pending deadline among timer-parked tasks, if any.
    fn next_deadline(&self) -> Option<i64> {
        self.timers.iter().filter_map(|t| t.ready_at_ms).min()
    }

    /// Return the next runnable task, or `None` if the scheduler is empty.
    ///
    /// Ready tasks always win (the scheduler never blocks on a timer while a
    /// task is runnable — otherwise an early timeout could starve a ready task
    /// that had not yet reached its own sleep). Only when no ready task exists
    /// does it sleep until the earliest pending deadline, promote the now-due
    /// timers, and return one of them.
    pub fn next_task(&mut self) -> Option<Task> {
        self.promote_timers(self.now_ms());
        if let Some(task) = self.ready.pop_front() {
            return Some(task);
        }
        if let Some(deadline) = self.next_deadline() {
            let now = self.now_ms();
            if deadline > now {
                std::thread::sleep(std::time::Duration::from_millis((deadline - now) as u64));
            }
            self.promote_timers(self.now_ms());
        }
        self.ready.pop_front()
    }
}
