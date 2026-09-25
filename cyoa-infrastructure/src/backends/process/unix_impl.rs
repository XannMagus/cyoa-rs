//! Synchronous, fair pipe polling. The child group is stopped before final
//! bounded capture and the direct child is reaped before success is possible.

use super::{
    IoFailure, IoOperation, Lifecycle, OutputStream, ProcessOutcome, ProcessSpec, SupervisorError,
};
use cyoa_application::cancellation::CancellationToken;
use cyoa_application::diagnostics::{CapturedBytes, TransportDiagnostics};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use rustix::pipe::{PipeFlags, pipe_with};
use rustix::process::{Pid, Signal, WaitId, WaitIdOptions, kill_process_group, waitid};
use std::os::fd::{AsFd, OwnedFd};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

const POLL_TICK: Duration = Duration::from_millis(25);
const CLEANUP_GRACE: Duration = Duration::from_secs(2);
const READ_CHUNK: usize = 8192;

/// Narrow fault seam: production always uses the real safe syscall APIs.
#[derive(Default, Clone)]
struct Operations {
    #[cfg(test)]
    fault: Option<IoOperation>,
    #[cfg(test)]
    poll_tick: Option<Duration>,
    #[cfg(test)]
    poll_started: Option<std::sync::mpsc::SyncSender<()>>,
    #[cfg(test)]
    interrupt_reap_until: Option<Instant>,
}

impl Operations {
    fn check(&self, operation: IoOperation) -> Result<(), IoFailure> {
        #[cfg(test)]
        if self.fault == Some(operation) {
            return Err(Self::failure(
                operation,
                std::io::Error::other("injected syscall failure"),
            ));
        }
        let _ = operation;
        Ok(())
    }
    fn failure(operation: IoOperation, source: impl Into<std::io::Error>) -> IoFailure {
        IoFailure {
            operation,
            source: source.into(),
        }
    }
    fn nonblocking(&self, fd: impl AsFd) -> Result<(), IoFailure> {
        self.check(IoOperation::Nonblocking)?;
        let result = fcntl_getfl(&fd).and_then(|flags| fcntl_setfl(&fd, flags | OFlags::NONBLOCK));
        result.map_err(|e| Self::failure(IoOperation::Nonblocking, e))
    }
    fn exited(&self, child: &Child) -> Result<bool, IoFailure> {
        self.check(IoOperation::ObserveExit)?;
        // WNOWAIT retains the child's PID until group signaling is complete;
        // try_wait here would reap it and permit PID reuse before killpg.
        waitid(
            WaitId::Pid(Pid::from_raw(child.id() as i32).expect("child PID")),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )
        .map(|status| status.is_some())
        .map_err(|e| Self::failure(IoOperation::ObserveExit, e))
    }
    fn kill(&self, child: &Child) -> Result<(), IoFailure> {
        self.check(IoOperation::KillGroup)?;
        match kill_process_group(
            Pid::from_raw(child.id() as i32).expect("child PID"),
            Signal::KILL,
        ) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
            Err(e) => Err(Self::failure(IoOperation::KillGroup, e)),
        }
    }
}

struct ChildGuard {
    child: Child,
    state: Lifecycle,
}

impl ChildGuard {
    fn try_reap(&mut self, _ops: &Operations) -> std::io::Result<Option<ExitStatus>> {
        #[cfg(test)]
        if _ops
            .interrupt_reap_until
            .is_some_and(|until| Instant::now() < until)
        {
            std::thread::sleep(Duration::from_millis(5));
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        self.child.try_wait()
    }
    fn stop(&mut self, ops: &Operations) -> Result<ExitStatus, IoFailure> {
        self.state = Lifecycle::Stopping;
        ops.kill(&self.child)?;
        let start = Instant::now();
        loop {
            if start.elapsed() >= CLEANUP_GRACE {
                return Err(Operations::failure(
                    IoOperation::Reap,
                    std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "child not reaped within cleanup grace",
                    ),
                ));
            }
            ops.check(IoOperation::Reap)?;
            match self.try_reap(ops) {
                Ok(Some(status)) => {
                    self.state = Lifecycle::Reaped;
                    return Ok(status);
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(Operations::failure(IoOperation::Reap, e)),
            }
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.state != Lifecycle::Reaped {
            let _ = self.stop(&Operations::default());
        }
    }
}

#[derive(PartialEq, Eq)]
enum CaptureState {
    Open,
    Eof,
    Truncated,
    Failed,
}

struct Capture {
    fd: OwnedFd,
    bytes: Vec<u8>,
    bound: usize,
    stream: OutputStream,
    state: CaptureState,
    nonblocking: bool,
}

impl Capture {
    fn new(fd: OwnedFd, bound: usize, stream: OutputStream) -> Self {
        Self {
            fd,
            bytes: Vec::new(),
            bound,
            stream,
            state: CaptureState::Open,
            nonblocking: false,
        }
    }
    fn configure(&mut self, ops: &Operations) -> Result<(), IoFailure> {
        ops.nonblocking(&self.fd)?;
        self.nonblocking = true;
        Ok(())
    }
    /// At most one small read per tick. One excess byte detects truncation;
    /// only the permitted prefix is retained, including during cleanup.
    fn read_once(&mut self, ops: &Operations) -> Result<bool, Ending> {
        if !self.nonblocking || self.state != CaptureState::Open {
            return Ok(false);
        }
        if let Err(e) = ops.check(IoOperation::Read) {
            self.state = CaptureState::Failed;
            return Err(Ending::Io(e));
        }
        let mut chunk = [0; READ_CHUNK];
        let remaining = self.bound - self.bytes.len();
        let length = remaining.saturating_add(1).min(chunk.len());
        match rustix::io::read(&self.fd, &mut chunk[..length]) {
            Ok(0) => {
                self.state = CaptureState::Eof;
                Ok(false)
            }
            Ok(n) => {
                self.bytes.extend_from_slice(&chunk[..n.min(remaining)]);
                if n > remaining {
                    self.state = CaptureState::Truncated;
                    Err(Ending::OutputBound(self.stream))
                } else {
                    Ok(true)
                }
            }
            Err(rustix::io::Errno::AGAIN | rustix::io::Errno::INTR) => Ok(false),
            Err(e) => {
                self.state = CaptureState::Failed;
                Err(Ending::Io(Operations::failure(IoOperation::Read, e)))
            }
        }
    }
    fn into_diagnostics(self) -> CapturedBytes {
        if self.state == CaptureState::Eof {
            CapturedBytes::complete(self.bytes)
        } else {
            CapturedBytes::prefix(self.bytes)
        }
    }
}

enum Ending {
    IncompleteInput { written: usize, expected: usize },
    Io(IoFailure),
    Cancelled,
    Timeout,
    OutputBound(OutputStream),
    ConsumerRejected(String),
    Exited,
}

impl Ending {
    fn outcome(
        self,
        status: Option<ExitStatus>,
        diagnostics: TransportDiagnostics,
    ) -> Result<ProcessOutcome, SupervisorError> {
        match self {
            Self::IncompleteInput { written, expected } => Err(SupervisorError::IncompleteInput {
                written,
                expected,
                diagnostics,
            }),
            Self::Io(failure) => Err(SupervisorError::Io {
                failure,
                diagnostics,
            }),
            Self::Cancelled => Err(SupervisorError::Cancelled { diagnostics }),
            Self::Timeout => Err(SupervisorError::Timeout { diagnostics }),
            Self::OutputBound(stream) => Err(SupervisorError::OutputBoundExceeded {
                stream,
                diagnostics,
            }),
            Self::ConsumerRejected(reason) => Err(SupervisorError::ConsumerRejected {
                reason,
                diagnostics,
            }),
            Self::Exited => {
                let exit_code = status.and_then(|s| s.code()).unwrap_or(-1);
                if exit_code == 0 {
                    Ok(ProcessOutcome {
                        exit_code,
                        diagnostics,
                    })
                } else {
                    Err(SupervisorError::NonzeroExit {
                        exit_code,
                        diagnostics,
                    })
                }
            }
        }
    }
}

struct Supervisor<'a> {
    guard: ChildGuard,
    stdin: Option<ChildStdin>,
    written: usize,
    stdout: Capture,
    stderr: Capture,
    records: super::RecordFramer,
    spec: &'a ProcessSpec,
    cancel: &'a CancellationToken,
    consumer: &'a mut dyn FnMut(&[u8]) -> Result<(), String>,
    ops: Operations,
}

impl Supervisor<'_> {
    fn configure(&mut self) -> Result<(), IoFailure> {
        self.stdout.configure(&self.ops)?;
        self.stderr.configure(&self.ops)?;
        if let Some(stdin) = &self.stdin {
            self.ops.nonblocking(stdin)?;
        }
        Ok(())
    }
    fn dispatch(&mut self, final_record: bool) -> Result<(), Ending> {
        for record in self.records.records(&self.stdout.bytes, final_record) {
            if self.cancel.is_cancelled() {
                return Err(Ending::Cancelled);
            }
            (self.consumer)(record).map_err(Ending::ConsumerRejected)?;
            if self.cancel.is_cancelled() {
                return Err(Ending::Cancelled);
            }
        }
        Ok(())
    }
    fn write_once(&mut self) -> Result<(), Ending> {
        let Some(stdin) = &self.stdin else {
            return Ok(());
        };
        self.ops.check(IoOperation::Write).map_err(Ending::Io)?;
        match rustix::io::write(stdin, &self.spec.stdin[self.written..]) {
            Ok(n) => {
                self.written += n;
                if self.written == self.spec.stdin.len() {
                    self.stdin = None;
                }
                Ok(())
            }
            Err(rustix::io::Errno::AGAIN | rustix::io::Errno::INTR) => Ok(()),
            Err(rustix::io::Errno::PIPE) => Err(Ending::IncompleteInput {
                written: self.written,
                expected: self.spec.stdin.len(),
            }),
            Err(e) => Err(Ending::Io(Operations::failure(IoOperation::Write, e))),
        }
    }
    fn supervise(&mut self, wake: &OwnedFd) -> Result<(), Ending> {
        self.configure().map_err(Ending::Io)?;
        let start = Instant::now();
        loop {
            if self.cancel.is_cancelled() {
                return Err(Ending::Cancelled);
            }
            let Some(remaining) = self.spec.bounds.deadline().checked_sub(start.elapsed()) else {
                return Err(Ending::Timeout);
            };
            let tick = POLL_TICK;
            #[cfg(test)]
            let tick = self.ops.poll_tick.unwrap_or(tick);
            let tick = tick.min(remaining);
            let ts = Timespec {
                tv_sec: tick.as_secs() as i64,
                tv_nsec: tick.subsec_nanos() as i64,
            };
            let ready = {
                let mut fds = vec![PollFd::new(wake, PollFlags::IN)];
                let out = (self.stdout.state == CaptureState::Open).then(|| {
                    let i = fds.len();
                    fds.push(PollFd::new(&self.stdout.fd, PollFlags::IN));
                    i
                });
                let err = (self.stderr.state == CaptureState::Open).then(|| {
                    let i = fds.len();
                    fds.push(PollFd::new(&self.stderr.fd, PollFlags::IN));
                    i
                });
                let input = self.stdin.as_ref().map(|s| {
                    let i = fds.len();
                    fds.push(PollFd::new(s, PollFlags::OUT));
                    i
                });
                self.ops.check(IoOperation::Poll).map_err(Ending::Io)?;
                #[cfg(test)]
                if let Some(ready) = self.ops.poll_started.take() {
                    ready.send(()).expect("test canceller is listening");
                }
                match poll(&mut fds, Some(&ts)) {
                    Ok(_) => {}
                    Err(rustix::io::Errno::INTR) => continue,
                    Err(e) => return Err(Ending::Io(Operations::failure(IoOperation::Poll, e))),
                }
                if fds.iter().any(|fd| fd.revents().contains(PollFlags::NVAL)) {
                    return Err(Ending::Io(Operations::failure(
                        IoOperation::Poll,
                        std::io::Error::other("invalid descriptor in poll set"),
                    )));
                }
                [out, err, input].map(|i| i.is_some_and(|i| !fds[i].revents().is_empty()))
            };
            if self.cancel.is_cancelled() {
                return Err(Ending::Cancelled);
            }
            if ready[2] {
                self.write_once()?;
            }
            if ready[0] {
                self.stdout.read_once(&self.ops)?;
                self.dispatch(false)?;
            }
            if ready[1] {
                self.stderr.read_once(&self.ops)?;
            }
            if self.ops.exited(&self.guard.child).map_err(Ending::Io)? {
                return Ok(());
            }
        }
    }
    fn finish(mut self, mut ending: Ending) -> Result<ProcessOutcome, SupervisorError> {
        self.stdin = None;
        // Stop before collecting buffered tail bytes. Keep the zombie PID
        // reserved until group signaling, then verify the direct child's reap.
        let mut failures = Vec::new();
        let status = match self.guard.stop(&self.ops) {
            Ok(status) => Some(status),
            Err(error) => {
                failures.push(error);
                None
            }
        };
        let start = Instant::now();
        loop {
            let mut progress = false;
            for capture in [&mut self.stdout, &mut self.stderr] {
                match capture.read_once(&self.ops) {
                    Ok(read) => progress |= read,
                    Err(Ending::Io(error)) => failures.push(error),
                    Err(error) => {
                        if matches!(ending, Ending::Exited) {
                            ending = error;
                        }
                    }
                }
            }
            let pending = [&self.stdout, &self.stderr]
                .iter()
                .any(|capture| capture.nonblocking && capture.state == CaptureState::Open);
            if !pending {
                break;
            }
            if start.elapsed() >= CLEANUP_GRACE {
                failures.push(Operations::failure(
                    IoOperation::Read,
                    std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "capture did not reach EOF within cleanup grace",
                    ),
                ));
                break;
            }
            if !progress {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        if matches!(ending, Ending::Exited) && failures.is_empty() {
            if let Err(error) = self.dispatch(true) {
                ending = error;
            }
            if self.cancel.is_cancelled() {
                ending = Ending::Cancelled;
            } else if matches!(ending, Ending::Exited)
                && status.is_some_and(|s| s.success())
                && self.written < self.spec.stdin.len()
            {
                ending = Ending::IncompleteInput {
                    written: self.written,
                    expected: self.spec.stdin.len(),
                };
            }
        }
        let diagnostics = TransportDiagnostics::from_captures(
            self.stdout.into_diagnostics(),
            self.stderr.into_diagnostics(),
        );
        if failures.is_empty() {
            ending.outcome(status, diagnostics)
        } else {
            // A failed reap is not a fabricated nonzero exit. Preserve any
            // actual initiating failure alongside all observed cleanup faults.
            let initial = if matches!(ending, Ending::Exited) {
                None
            } else {
                ending
                    .outcome(status, diagnostics.clone())
                    .err()
                    .map(Box::new)
            };
            Err(SupervisorError::Cleanup {
                initial,
                failures,
                diagnostics,
            })
        }
    }
}

pub fn run(
    spec: ProcessSpec,
    cancel: &CancellationToken,
    on_record: &mut dyn FnMut(&[u8]) -> Result<(), String>,
) -> Result<ProcessOutcome, SupervisorError> {
    let result = run_with(&spec, cancel, on_record, Operations::default());
    if let Err(error) = spec.workspace.close() {
        let diagnostics = match &result {
            Ok(outcome) => outcome.diagnostics.clone(),
            Err(error) => error.diagnostics(),
        };
        return Err(SupervisorError::Cleanup {
            initial: result.err().map(Box::new),
            failures: vec![Operations::failure(IoOperation::WorkspaceCleanup, error)],
            diagnostics,
        });
    }
    result
}

fn run_with(
    spec: &ProcessSpec,
    cancel: &CancellationToken,
    on_record: &mut dyn FnMut(&[u8]) -> Result<(), String>,
    ops: Operations,
) -> Result<ProcessOutcome, SupervisorError> {
    if cancel.is_cancelled() {
        return Err(SupervisorError::Cancelled {
            diagnostics: TransportDiagnostics::empty(),
        });
    }
    let (wake, writer) = pipe_with(PipeFlags::CLOEXEC | PipeFlags::NONBLOCK)
        .map_err(|e| SupervisorError::Spawn(e.into()))?;
    let writer = Arc::new(writer);
    let _wake_registration = cancel.subscribe(move || {
        let _ = rustix::io::write(writer.as_ref(), &[1]);
    });
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(spec.workspace.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear();
    for (key, value) in spec.env.vars() {
        command.env(key, value);
    }
    use std::os::unix::process::CommandExt;
    command.process_group(0);
    let mut child = command.spawn().map_err(SupervisorError::Spawn)?;
    let stdin = child.stdin.take().filter(|_| !spec.stdin.is_empty());
    let stdout = Capture::new(
        child.stdout.take().expect("piped stdout").into(),
        spec.bounds.max_stdout_bytes(),
        OutputStream::Stdout,
    );
    let stderr = Capture::new(
        child.stderr.take().expect("piped stderr").into(),
        spec.bounds.max_stderr_bytes(),
        OutputStream::Stderr,
    );
    let mut supervisor = Supervisor {
        guard: ChildGuard {
            child,
            state: Lifecycle::Spawned,
        },
        stdin,
        written: 0,
        stdout,
        stderr,
        records: super::RecordFramer::default(),
        spec,
        cancel,
        consumer: on_record,
        ops,
    };
    let ending = supervisor.supervise(&wake).err().unwrap_or(Ending::Exited);
    supervisor.finish(ending)
}

#[cfg(test)]
mod fault_tests {
    use super::*;
    #[test]
    fn interrupted_reaping_still_has_a_finite_deadline() {
        use std::os::unix::process::CommandExt;
        let child = Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .spawn()
            .unwrap();
        let pid = Pid::from_raw(child.id() as i32).unwrap();
        let mut guard = ChildGuard {
            child,
            state: Lifecycle::Spawned,
        };
        let result = guard.stop(&Operations {
            interrupt_reap_until: Some(Instant::now() + CLEANUP_GRACE + Duration::from_millis(300)),
            ..Operations::default()
        });
        drop(guard);
        assert_eq!(
            rustix::process::test_kill_process(pid),
            Err(rustix::io::Errno::SRCH)
        );
        assert!(
            matches!(result, Err(IoFailure { operation: IoOperation::Reap, source }) if source.kind() == std::io::ErrorKind::TimedOut)
        );
    }
    #[test]
    fn self_pipe_wakes_a_long_poll_without_removing_token_checks() {
        let mut request = spec();
        request.program = "/bin/sleep".into();
        request.args = vec!["30".into()];
        request.stdin.clear();
        request.bounds = super::super::ProcessBounds::new(
            Duration::from_secs(5),
            super::super::MaxStdoutBytes::new(100).unwrap(),
            super::super::MaxStderrBytes::new(100).unwrap(),
        )
        .unwrap();
        let source = cyoa_application::cancellation::CancellationSource::default();
        let token = source.token();
        let (ready, waiter) = std::sync::mpsc::sync_channel(1);
        let canceller = std::thread::spawn(move || {
            waiter
                .recv_timeout(Duration::from_secs(5))
                .expect("supervisor reached poll");
            source.cancel();
        });
        let start = Instant::now();
        let result = run_with(
            &request,
            &token,
            &mut |_| Ok(()),
            Operations {
                poll_tick: Some(Duration::from_secs(2)),
                poll_started: Some(ready),
                ..Operations::default()
            },
        );
        canceller.join().unwrap();
        assert!(matches!(result, Err(SupervisorError::Cancelled { .. })));
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "waited for the two-second poll tick instead of the self-pipe"
        );
    }
    fn spec() -> ProcessSpec {
        ProcessSpec {
            workspace: super::super::RequestWorkspace::new().unwrap(),
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "printf 'ready\n'; sleep 30".into()],
            env: super::super::EnvPolicy::new(),
            stdin: b"request".to_vec(),
            bounds: super::super::ProcessBounds::new(
                Duration::from_millis(50),
                super::super::MaxStdoutBytes::new(100).unwrap(),
                super::super::MaxStderrBytes::new(100).unwrap(),
            )
            .unwrap(),
        }
    }
    #[test]
    fn setup_write_read_poll_and_exit_observation_failures_are_errors() {
        for operation in [
            IoOperation::Nonblocking,
            IoOperation::Write,
            IoOperation::Read,
            IoOperation::Poll,
            IoOperation::ObserveExit,
        ] {
            let source = cyoa_application::cancellation::CancellationSource::default();
            let error = run_with(
                &spec(),
                &source.token(),
                &mut |_| Ok(()),
                Operations {
                    fault: Some(operation),
                    ..Operations::default()
                },
            )
            .unwrap_err();
            let initial = match &error {
                SupervisorError::Cleanup {
                    initial: Some(initial),
                    ..
                } => initial.as_ref(),
                other => other,
            };
            assert!(
                matches!(initial, SupervisorError::Io { failure, .. } if failure.operation == operation),
                "{error:?}"
            );
        }
    }
    #[test]
    fn cleanup_failures_preserve_the_initiating_error() {
        for operation in [IoOperation::KillGroup, IoOperation::Reap] {
            let source = cyoa_application::cancellation::CancellationSource::default();
            let error = run_with(
                &spec(),
                &source.token(),
                &mut |_| Err("bad record".into()),
                Operations {
                    fault: Some(operation),
                    ..Operations::default()
                },
            )
            .unwrap_err();
            match error {
                SupervisorError::Cleanup {
                    initial: Some(initial),
                    failures,
                    diagnostics,
                } => {
                    assert!(matches!(*initial, SupervisorError::ConsumerRejected { .. }));
                    assert!(
                        failures
                            .iter()
                            .any(|failure| failure.operation == operation)
                    );
                    assert_eq!(diagnostics.stdout(), b"ready\n");
                }
                other => panic!("{other:?}"),
            }
        }
    }
    #[test]
    fn fatal_read_error_is_not_treated_as_would_block() {
        let (_read, write) = pipe_with(PipeFlags::NONBLOCK).unwrap();
        let mut capture = Capture::new(write, 100, OutputStream::Stdout);
        capture.configure(&Operations::default()).unwrap();
        assert!(matches!(
            capture.read_once(&Operations::default()),
            Err(Ending::Io(_))
        ));
    }
}
