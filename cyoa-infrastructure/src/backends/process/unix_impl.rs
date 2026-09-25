//! Real Unix process supervisor: `Command` + a synchronous poll loop over
//! stdout/stderr/self-pipe readiness and non-blocking stdin write-readiness,
//! per `docs/plans/phase1-headless-backends.md`'s "Cancellation and cleanup
//! must work without output" contract.
//!
//! Stdin delivery policy (explicit, since the plan requires one): if writing
//! the request payload hits a broken pipe (the child closed its stdin
//! without reading all of it), delivery is abandoned — this is not itself a
//! supervisor failure. A vendor CLI that stops reading stdin early but still
//! exits zero with valid output is not this layer's problem to diagnose;
//! only actual exit status/consumer rejection/cancellation/deadline/output
//! bound decide the outcome.
//!
//! Every exit path (including ordinary success, and a panic unwinding out of
//! this function via [`ChildGuard`]'s `Drop`) sends `SIGKILL` to the whole
//! process group before returning: this is how a descendant that has
//! inherited and retained the pipes gets cleaned up, per the plan's
//! process-group cleanup requirement.

use super::{Lifecycle, OutputStream, ProcessOutcome, ProcessSpec, SupervisorError};
use cyoa_application::cancellation::CancellationToken;
use cyoa_core::text::TransportDiagnostics;
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use rustix::pipe::{PipeFlags, pipe_with};
use rustix::process::{Pid, Signal, kill_process_group};
use std::os::fd::AsFd;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long a single `poll()` call waits before we re-check the deadline and
/// call `Child::try_wait()` again. Exit alone never wakes the poll set (a
/// descendant can hold the pipe open with no `POLLHUP`), so this tick is the
/// portable fallback the plan explicitly sanctions in place of a Linux-only
/// pidfd: bounded, and short enough that deadline/exit detection stays
/// responsive without busy-looping.
const POLL_TICK: Duration = Duration::from_millis(25);

/// How long we keep polling `try_wait()` for the reap to land after sending
/// `SIGKILL`, before giving up. A killed process should be reaped almost
/// immediately; this is a safety bound, not an expected wait. If this
/// expires the child may still be a zombie; the caller only learns this if
/// it separately checks the pid (see the supervisor's own tests) — `run`
/// itself still returns the outcome its transport observation earned.
const REAP_GRACE: Duration = Duration::from_secs(2);

fn nonblocking(fd: impl AsFd) {
    if let Ok(flags) = fcntl_getfl(&fd) {
        let _ = fcntl_setfl(&fd, flags | OFlags::NONBLOCK);
    }
}

/// Best-effort: a process that already exited is a benign ESRCH, not an
/// error worth surfacing over the outcome we already have.
fn kill_group_best_effort(pid: u32) {
    if let Some(pid) = Pid::from_raw(pid as i32) {
        let _ = kill_process_group(pid, Signal::KILL);
    }
}

/// Owns the launched child and its explicit lifecycle state. If `run`
/// returns through any path that reaches [`ChildGuard::kill_and_reap`] (and
/// so leaves `state` at [`Lifecycle::Reaped`]), `Drop` is a no-op;
/// otherwise (a panic unwinding out of `run`, or a future early-return this
/// module forgets to route through cleanup) `Drop` itself performs the same
/// kill-and-reap as a backstop. This is the guard the plan's "every exit
/// path" requirement actually needs: normal-path cleanup happens in
/// [`finish`] (via this same method), this is the backstop for the
/// abnormal paths `finish` never runs on.
struct ChildGuard {
    child: Child,
    state: Lifecycle,
}

impl ChildGuard {
    /// Kills the whole process group, then best-effort reaps within
    /// [`REAP_GRACE`], advancing `state` through `Stopping` -> `Reaped` (or
    /// straight to `Reaped` if `try_wait` had already observed the exit
    /// before this call). Idempotent: a second call is a cheap no-op.
    fn kill_and_reap(&mut self) {
        if self.state == Lifecycle::Reaped {
            return;
        }
        if self.state == Lifecycle::Spawned {
            self.state = Lifecycle::Stopping;
        }
        kill_group_best_effort(self.child.id());
        let deadline = Instant::now() + REAP_GRACE;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => {
                    self.state = Lifecycle::Reaped;
                    return;
                }
                Err(_) => {
                    // Can't observe the outcome further; leave `state` at
                    // `Stopping`/`Exited` rather than falsely claiming Reaped.
                    return;
                }
                Ok(None) => {
                    if Instant::now() >= deadline {
                        return; // REAP_GRACE expired without a confirmed reap
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.kill_and_reap();
    }
}

/// Reads everything currently available on a non-blocking fd without
/// blocking for EOF, appending to both `diag` (the full retained transport
/// record) and `acc` (bytes not yet split into records) in lockstep, so a
/// record delivered from this drain is always backed by the same bytes the
/// diagnostics report — draining into diagnostics alone would silently drop
/// records the child wrote between the last read and exit detection.
enum DrainOutcome {
    Eof,
    WouldBlockOrDone,
    BoundExceeded,
}

fn drain_into(fd: impl AsFd, diag: &mut Vec<u8>, acc: &mut Vec<u8>, bound: usize) -> DrainOutcome {
    let mut chunk = [0u8; 64 * 1024];
    loop {
        match rustix::io::read(&fd, &mut chunk[..]) {
            Ok(0) => return DrainOutcome::Eof,
            Ok(n) => {
                diag.extend_from_slice(&chunk[..n]);
                acc.extend_from_slice(&chunk[..n]);
                if diag.len() > bound {
                    return DrainOutcome::BoundExceeded;
                }
            }
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => return DrainOutcome::WouldBlockOrDone, // AGAIN or a real error either way
        }
    }
}

enum DispatchOutcome {
    Continue,
    Rejected(String),
    Cancelled,
}

/// Splits complete records off `acc` and delivers each to `on_record`,
/// re-checking cancellation after every call — the consumer runs
/// synchronously and may cancel from inside itself (e.g. "stop after the
/// record I care about"), which must win over a same-tick successful exit
/// rather than waiting for the next self-pipe wake.
fn dispatch_records(
    acc: &mut Vec<u8>,
    on_record: &mut dyn FnMut(&[u8]) -> Result<(), String>,
    cancel: &CancellationToken,
) -> DispatchOutcome {
    for record in super::split_records(acc) {
        if let Err(reason) = on_record(&record) {
            return DispatchOutcome::Rejected(reason);
        }
        if cancel.is_cancelled() {
            return DispatchOutcome::Cancelled;
        }
    }
    DispatchOutcome::Continue
}

/// Why the loop is ending, decided at the point of the decisive observation.
enum Ending {
    Cancelled,
    Timeout,
    OutputBound(OutputStream),
    ConsumerRejected(String),
    Exited(i32),
}

/// Every exit path funnels through here: kill the whole process group
/// (covers any descendant holding the pipes), drain whatever is currently
/// readable without waiting for EOF, reap within a bounded grace period,
/// mark the guard reaped so its `Drop` is a no-op, then map to the final
/// outcome.
fn finish(
    guard: &mut ChildGuard,
    stdout: impl AsFd,
    stderr: impl AsFd,
    mut stdout_diag: Vec<u8>,
    mut stderr_diag: Vec<u8>,
    ending: Ending,
) -> Result<ProcessOutcome, SupervisorError> {
    let mut discard = Vec::new();
    drain_into(&stdout, &mut stdout_diag, &mut discard, usize::MAX);
    drain_into(&stderr, &mut stderr_diag, &mut discard, usize::MAX);
    guard.kill_and_reap();
    let diagnostics = TransportDiagnostics::new(stdout_diag, stderr_diag);
    match ending {
        Ending::Cancelled => Err(SupervisorError::Cancelled { diagnostics }),
        Ending::Timeout => Err(SupervisorError::Timeout { diagnostics }),
        Ending::OutputBound(stream) => Err(SupervisorError::OutputBoundExceeded {
            stream,
            diagnostics,
        }),
        Ending::ConsumerRejected(reason) => Err(SupervisorError::ConsumerRejected {
            reason,
            diagnostics,
        }),
        Ending::Exited(exit_code) if exit_code != 0 => Err(SupervisorError::NonzeroExit {
            exit_code,
            diagnostics,
        }),
        Ending::Exited(exit_code) => Ok(ProcessOutcome {
            exit_code,
            diagnostics,
        }),
    }
}

pub fn run(
    spec: &ProcessSpec,
    cancel: &CancellationToken,
    on_record: &mut dyn FnMut(&[u8]) -> Result<(), String>,
) -> Result<ProcessOutcome, SupervisorError> {
    if cancel.is_cancelled() {
        return Err(SupervisorError::Cancelled {
            diagnostics: TransportDiagnostics::empty(),
        });
    }

    // Created before spawning so a self-pipe failure never leaves a live
    // child behind with no guard watching it.
    let (self_read, self_write) = pipe_with(PipeFlags::CLOEXEC | PipeFlags::NONBLOCK)
        .map_err(|e| SupervisorError::Spawn(std::io::Error::from_raw_os_error(e.raw_os_error())))?;
    let self_write = Arc::new(self_write);
    {
        let self_write = Arc::clone(&self_write);
        cancel.on_cancel(move || {
            let _ = rustix::io::write(self_write.as_ref(), &[1u8]);
        });
    }

    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.env_clear();
    for (key, value) in spec.env.vars() {
        command.env(key, value);
    }
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let mut child = command.spawn().map_err(SupervisorError::Spawn)?;
    let mut stdin: Option<ChildStdin> = child.stdin.take();
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    // From here on, `guard` owns the child: any early return (including a
    // panic unwinding through this function) reaches `ChildGuard::drop`.
    let mut guard = ChildGuard {
        child,
        state: Lifecycle::Spawned,
    };

    // Non-blocking so a full stdin pipe never needs a dedicated blocking
    // writer thread, and reads never block the single supervisor thread.
    if let Some(ref s) = stdin {
        nonblocking(s);
    }
    nonblocking(&stdout);
    nonblocking(&stderr);

    let mut stdout_acc: Vec<u8> = Vec::new(); // bytes not yet split into records
    let mut stdout_diag: Vec<u8> = Vec::new(); // full retained stdout, for diagnostics
    let mut stderr_diag: Vec<u8> = Vec::new(); // full retained stderr
    let mut stdout_eof = false;
    let mut stderr_eof = false;
    let mut stdin_pos = 0usize;
    if stdin.as_ref().is_some_and(|_| spec.stdin.is_empty()) {
        stdin = None; // nothing to write: close immediately so the child sees EOF
    }

    let stdout_bound = spec.bounds.max_stdout_bytes();
    let stderr_bound = spec.bounds.max_stderr_bytes();
    let start = Instant::now();

    loop {
        let elapsed = start.elapsed();
        if elapsed >= spec.bounds.deadline() {
            return finish(
                &mut guard,
                &stdout,
                &stderr,
                stdout_diag,
                stderr_diag,
                Ending::Timeout,
            );
        }
        let tick = POLL_TICK.min(spec.bounds.deadline() - elapsed);
        let ts = Timespec {
            tv_sec: tick.as_secs() as i64,
            tv_nsec: tick.subsec_nanos() as i64,
        };

        let mut close_stdin = false;
        let cancelled;
        let mut cancelled_during_record = false;
        let mut rejected: Option<String> = None;
        let mut bound_exceeded: Option<OutputStream> = None;

        {
            let mut fds: Vec<PollFd<'_>> = Vec::with_capacity(4);
            let self_read_idx = fds.len();
            fds.push(PollFd::new(&self_read, PollFlags::IN));
            let stdout_idx = (!stdout_eof).then(|| {
                let i = fds.len();
                fds.push(PollFd::new(&stdout, PollFlags::IN));
                i
            });
            let stderr_idx = (!stderr_eof).then(|| {
                let i = fds.len();
                fds.push(PollFd::new(&stderr, PollFlags::IN));
                i
            });
            let stdin_idx = stdin.as_ref().map(|s| {
                let i = fds.len();
                fds.push(PollFd::new(s, PollFlags::OUT));
                i
            });

            while let Err(rustix::io::Errno::INTR) = poll(&mut fds, Some(&ts)) {}

            // Only the self-pipe wake decides `cancelled` here (not a
            // redundant `cancel.is_cancelled()` OR-clause): that keeps the
            // notifier genuinely load-bearing for the idle case instead of
            // masked by a coincidentally-short poll tick.
            cancelled = fds[self_read_idx].revents().contains(PollFlags::IN);

            if !cancelled && let Some(i) = stdin_idx {
                let ready = fds[i]
                    .revents()
                    .intersects(PollFlags::OUT | PollFlags::ERR | PollFlags::HUP);
                if ready {
                    let s = stdin.as_ref().expect("stdin_idx implies stdin is Some");
                    match rustix::io::write(s, &spec.stdin[stdin_pos..]) {
                        Ok(n) if n > 0 => {
                            stdin_pos += n;
                            if stdin_pos >= spec.stdin.len() {
                                close_stdin = true;
                            }
                        }
                        Ok(_) | Err(rustix::io::Errno::AGAIN | rustix::io::Errno::INTR) => {}
                        Err(_) => close_stdin = true, // broken pipe: abandon delivery, not fatal
                    }
                }
            }

            if !cancelled && let Some(i) = stdout_idx {
                let ready = fds[i]
                    .revents()
                    .intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR);
                if ready {
                    match drain_into(&stdout, &mut stdout_diag, &mut stdout_acc, stdout_bound) {
                        DrainOutcome::Eof => stdout_eof = true,
                        DrainOutcome::WouldBlockOrDone => {}
                        DrainOutcome::BoundExceeded => {
                            bound_exceeded = Some(OutputStream::Stdout);
                        }
                    }
                    if bound_exceeded.is_none() {
                        match dispatch_records(&mut stdout_acc, on_record, cancel) {
                            DispatchOutcome::Continue => {}
                            DispatchOutcome::Rejected(reason) => rejected = Some(reason),
                            DispatchOutcome::Cancelled => cancelled_during_record = true,
                        }
                    }
                }
            }

            if !cancelled
                && !cancelled_during_record
                && rejected.is_none()
                && bound_exceeded.is_none()
                && let Some(i) = stderr_idx
            {
                let ready = fds[i]
                    .revents()
                    .intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR);
                if ready {
                    let mut discard = Vec::new();
                    match drain_into(&stderr, &mut stderr_diag, &mut discard, stderr_bound) {
                        DrainOutcome::Eof => stderr_eof = true,
                        DrainOutcome::WouldBlockOrDone => {}
                        DrainOutcome::BoundExceeded => {
                            bound_exceeded = Some(OutputStream::Stderr);
                        }
                    }
                }
            }
        } // `fds` (and its borrows of stdin/stdout/stderr/self_read) end here

        if close_stdin {
            stdin = None; // dropping ChildStdin closes the write end (EOF)
        }

        if cancelled || cancelled_during_record {
            return finish(
                &mut guard,
                &stdout,
                &stderr,
                stdout_diag,
                stderr_diag,
                Ending::Cancelled,
            );
        }
        if let Some(stream) = bound_exceeded {
            return finish(
                &mut guard,
                &stdout,
                &stderr,
                stdout_diag,
                stderr_diag,
                Ending::OutputBound(stream),
            );
        }
        if let Some(reason) = rejected {
            return finish(
                &mut guard,
                &stdout,
                &stderr,
                stdout_diag,
                stderr_diag,
                Ending::ConsumerRejected(reason),
            );
        }

        // Exit alone never wakes the poll set if a descendant still holds a
        // pipe open, so check every tick regardless of poll()'s result.
        if let Ok(Some(status)) = guard.child.try_wait() {
            guard.state = Lifecycle::Exited;
            // One last non-blocking drain (into diagnostics AND the record
            // accumulator, in lockstep — see `drain_into`'s doc) before we
            // drop these fds for good.
            match drain_into(&stdout, &mut stdout_diag, &mut stdout_acc, stdout_bound) {
                DrainOutcome::BoundExceeded => {
                    return finish(
                        &mut guard,
                        &stdout,
                        &stderr,
                        stdout_diag,
                        stderr_diag,
                        Ending::OutputBound(OutputStream::Stdout),
                    );
                }
                DrainOutcome::Eof | DrainOutcome::WouldBlockOrDone => {}
            }
            {
                let mut discard = Vec::new();
                if let DrainOutcome::BoundExceeded =
                    drain_into(&stderr, &mut stderr_diag, &mut discard, stderr_bound)
                {
                    return finish(
                        &mut guard,
                        &stdout,
                        &stderr,
                        stdout_diag,
                        stderr_diag,
                        Ending::OutputBound(OutputStream::Stderr),
                    );
                }
            }

            match dispatch_records(&mut stdout_acc, on_record, cancel) {
                DispatchOutcome::Continue => {}
                DispatchOutcome::Rejected(reason) => {
                    return finish(
                        &mut guard,
                        &stdout,
                        &stderr,
                        stdout_diag,
                        stderr_diag,
                        Ending::ConsumerRejected(reason),
                    );
                }
                DispatchOutcome::Cancelled => {
                    return finish(
                        &mut guard,
                        &stdout,
                        &stderr,
                        stdout_diag,
                        stderr_diag,
                        Ending::Cancelled,
                    );
                }
            }
            if !stdout_acc.is_empty() {
                // Final record without a trailing newline.
                let record = std::mem::take(&mut stdout_acc);
                if let Err(reason) = on_record(&record) {
                    return finish(
                        &mut guard,
                        &stdout,
                        &stderr,
                        stdout_diag,
                        stderr_diag,
                        Ending::ConsumerRejected(reason),
                    );
                }
                if cancel.is_cancelled() {
                    return finish(
                        &mut guard,
                        &stdout,
                        &stderr,
                        stdout_diag,
                        stderr_diag,
                        Ending::Cancelled,
                    );
                }
            }
            let exit_code = status.code().unwrap_or(-1);
            return finish(
                &mut guard,
                &stdout,
                &stderr,
                stdout_diag,
                stderr_diag,
                Ending::Exited(exit_code),
            );
        }
    }
}
