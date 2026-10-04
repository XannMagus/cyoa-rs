//! Real-process tests for the vendor-neutral process supervisor (Phase 1
//! item 3). Every test here drives the real `subprocess_fixture` binary
//! through `cyoa_infrastructure::backends::process::run`; none of them use a
//! mock child, per the plan's "Require an actual child test for idle
//! cancellation and backpressure; mocks alone cannot establish these."
//!
//! Unix-only: the supervisor itself is `#[cfg(unix)]`-gated and this test
//! file is not compiled or run on any other platform.
#![cfg(all(
    unix,
    not(any(
        target_os = "cygwin",
        target_os = "horizon",
        target_os = "openbsd",
        target_os = "redox"
    ))
))]

mod support;
use support::fixture_backend::{self, read_report, report_path};

use cyoa_application::cancellation::CancellationSource;
use cyoa_infrastructure::backends::process::{
    EnvPolicy, MaxStderrBytes, MaxStdoutBytes, ProcessBounds, ProcessSpec, SupervisorError, run,
};
use std::ffi::OsString;
use std::time::Duration;

const FIXTURE_EXE: &str = env!("CARGO_BIN_EXE_subprocess_fixture");

fn spec(scenario_json: String, bounds: ProcessBounds) -> ProcessSpec {
    ProcessSpec {
        workspace: cyoa_infrastructure::backends::process::RequestWorkspace::new().unwrap(),
        program: FIXTURE_EXE.into(),
        args: vec![OsString::from(scenario_json)],
        env: EnvPolicy::new(),
        stdin: Vec::new(),
        bounds,
    }
}

/// Test-only convenience: every real call site is required to pick explicit,
/// finite bounds (never `ProcessBounds::generous_default()`), but unwrapping
/// two checked-nonzero newtypes and a `Result` inline at every test call
/// site would bury the bound values that matter under noise.
fn bounds(deadline: Duration, max_stdout_bytes: usize, max_stderr_bytes: usize) -> ProcessBounds {
    ProcessBounds::new(
        deadline,
        MaxStdoutBytes::new(max_stdout_bytes).unwrap(),
        MaxStderrBytes::new(max_stderr_bytes).unwrap(),
    )
    .unwrap()
}

fn short_bounds() -> ProcessBounds {
    bounds(Duration::from_secs(5), 1024 * 1024, 1024 * 1024)
}

fn noop(_: &[u8]) -> Result<(), String> {
    Ok(())
}

/// Polls for a file to exist (the fixture's report is written atomically —
/// temp file plus rename — so its mere existence at a given path is a safe
/// handshake, not a partial-write race), bounded so a missing handshake
/// fails fast instead of hanging the test.
fn wait_for_file(path: &std::path::Path, bound: Duration) -> bool {
    let deadline = std::time::Instant::now() + bound;
    while std::time::Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

/// Ask the kernel whether this PID exists; missing Linux procfs must never
/// make a Unix cleanup assertion pass vacuously.
fn assert_process_gone(pid: u32, context: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        let pid = rustix::process::Pid::from_raw(pid.try_into().expect("PID fits i32"))
            .expect("nonzero PID");
        match rustix::process::test_kill_process(pid) {
            Err(rustix::io::Errno::SRCH) => return,
            Ok(()) => {}
            Err(error) => panic!("{context}: unable to establish process liveness: {error}"),
        }
        if std::time::Instant::now() >= deadline {
            panic!("{context}: pid {pid:?} should have been cleaned up but is still alive");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

// --- Error tests ------------------------------------------------------

#[test]
fn missing_executable_is_a_spawn_error_not_a_panic() {
    let spec = ProcessSpec {
        workspace: cyoa_infrastructure::backends::process::RequestWorkspace::new().unwrap(),
        program: "/definitely/not/a/real/executable-cyoa-test".into(),
        args: vec![],
        env: EnvPolicy::new(),
        stdin: Vec::new(),
        bounds: short_bounds(),
    };
    let source = CancellationSource::default();
    let error = run(spec, &source.token(), &mut noop).unwrap_err();
    assert!(matches!(error, SupervisorError::Spawn(_)));
}

#[test]
fn cancellation_before_spawn_launches_no_process() {
    let path = report_path("cancel-before-spawn");
    let scenario = fixture_backend::scenario_with_report(&[], &[], 0, false, Some(&path));
    let spec = spec(scenario, short_bounds());
    let source = CancellationSource::default();
    source.cancel();
    let error = run(spec, &source.token(), &mut noop).unwrap_err();
    assert!(matches!(error, SupervisorError::Cancelled { .. }));
    assert!(
        !path.exists(),
        "no process should have been launched, so no report file should exist"
    );
}

#[test]
fn cancellation_while_the_child_is_silent_still_wakes_the_poll_loop_and_returns_promptly() {
    // The child writes its report (with its own pid) immediately, then
    // hangs silently for far longer than our deadline before producing any
    // output. The canceller thread waits for that report file to exist — a
    // real handshake, not a fixed sleep guessing when the child is ready —
    // then cancels while the child is genuinely idle. If the supervisor
    // only checked cancellation on output readiness (the exact bug this
    // design exists to prevent), or only via a bounded poll tick with no
    // real wake, this test would need to wait out most of the deadline
    // instead of returning almost immediately.
    let path = report_path("idle-cancel");
    let scenario = fixture_backend::scenario_hanging(
        &[],
        &[],
        0,
        false,
        Some(&path),
        None,
        Some(30_000), // hang_ms: far longer than our 3s deadline below
    );
    let deadline_bounds = bounds(Duration::from_secs(3), 1024 * 1024, 1024 * 1024);
    let spec = spec(scenario, deadline_bounds);
    let source = CancellationSource::default();
    let token = source.token();

    let path_for_thread = path.clone();
    let start = std::time::Instant::now();
    let canceller = std::thread::spawn(move || {
        assert!(
            wait_for_file(&path_for_thread, Duration::from_secs(2)),
            "fixture never wrote its report"
        );
        source.cancel();
    });
    let error = run(spec, &token, &mut noop).unwrap_err();
    canceller.join().unwrap();
    let elapsed = start.elapsed();

    assert!(matches!(error, SupervisorError::Cancelled { .. }));
    assert!(
        elapsed < Duration::from_secs(1),
        "cancellation while idle must wake the poll loop promptly (well under the 3s deadline), took {elapsed:?}"
    );

    let report = read_report(&path);
    assert_process_gone(report.pid, "idle cancel");
}

#[test]
fn nonzero_exit_is_reported_with_diagnostics() {
    let scenario = fixture_backend::scenario(&[], &[b"boom"], 7);
    let spec = spec(scenario, short_bounds());
    let source = CancellationSource::default();
    let error = run(spec, &source.token(), &mut noop).unwrap_err();
    match error {
        SupervisorError::NonzeroExit {
            exit_code,
            diagnostics,
        } => {
            assert_eq!(exit_code, 7);
            assert_eq!(diagnostics.stderr(), b"boom");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn failure_after_a_candidate_record_still_reports_nonzero_exit_not_success() {
    let scenario = fixture_backend::scenario(&[b"{\"ok\":true}\n"], &[], 3);
    let spec = spec(scenario, short_bounds());
    let source = CancellationSource::default();
    let mut records = Vec::new();
    let error = run(spec, &source.token(), &mut |record| {
        records.push(record.to_vec());
        Ok(())
    })
    .unwrap_err();
    assert!(matches!(
        error,
        SupervisorError::NonzeroExit { exit_code: 3, .. }
    ));
    assert_eq!(records, vec![b"{\"ok\":true}".to_vec()]);
}

#[test]
fn deadline_exceeded_kills_the_child_and_reports_timeout() {
    let path = report_path("deadline-cleanup");
    let scenario =
        fixture_backend::scenario_hanging(&[], &[], 0, false, Some(&path), None, Some(60_000));
    let spec = spec(scenario, bounds(Duration::from_millis(150), 1024, 1024));
    let source = CancellationSource::default();
    let start = std::time::Instant::now();
    let error = run(spec, &source.token(), &mut noop).unwrap_err();
    let elapsed = start.elapsed();
    assert!(matches!(error, SupervisorError::Timeout { .. }));
    assert!(elapsed < Duration::from_secs(5), "took {elapsed:?}");
    assert_process_gone(read_report(&path).pid, "deadline cleanup");
}

#[test]
fn output_bound_exceeded_stops_the_child_instead_of_buffering_forever() {
    let path = report_path("output-bound");
    let big: Vec<u8> = vec![b'x'; 4096];
    let scenario = fixture_backend::scenario_with_report(&[&big], &[], 0, false, Some(&path));
    let tiny_bounds = bounds(Duration::from_secs(5), 100, 1024);
    let spec = spec(scenario, tiny_bounds);
    let source = CancellationSource::default();
    let error = run(spec, &source.token(), &mut noop).unwrap_err();
    assert!(matches!(
        error,
        SupervisorError::OutputBoundExceeded {
            stream: cyoa_infrastructure::backends::process::OutputStream::Stdout,
            ..
        }
    ));
    let report = read_report(&path);
    assert_process_gone(report.pid, "output bound exceeded");
}

#[test]
fn consumer_rejection_triggers_cleanup_and_reports_the_reason() {
    let path = report_path("consumer-rejection");
    let scenario =
        fixture_backend::scenario_with_report(&[b"not json at all\n"], &[], 0, false, Some(&path));
    let spec = spec(scenario, short_bounds());
    let source = CancellationSource::default();
    let error = run(spec, &source.token(), &mut |record| {
        if record == b"not json at all" {
            Err("rejected: not valid JSON".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    match error {
        SupervisorError::ConsumerRejected { reason, .. } => {
            assert_eq!(reason, "rejected: not valid JSON");
        }
        other => panic!("unexpected error: {other}"),
    }
    let report = read_report(&path);
    assert_process_gone(report.pid, "consumer rejection");
}

// --- Edge tests ---------------------------------------------------------

#[test]
fn final_record_without_a_trailing_newline_is_still_delivered() {
    let scenario = fixture_backend::scenario(&[b"no newline here"], &[], 0);
    let spec = spec(scenario, short_bounds());
    let source = CancellationSource::default();
    let mut records = Vec::new();
    let outcome = run(spec, &source.token(), &mut |record| {
        records.push(record.to_vec());
        Ok(())
    })
    .unwrap();
    assert_eq!(outcome.exit_code, 0);
    assert_eq!(records, vec![b"no newline here".to_vec()]);
}

#[test]
fn crlf_records_are_delivered_with_the_carriage_return_intact() {
    let scenario = fixture_backend::scenario(&[b"one\r\ntwo\r\n"], &[], 0);
    let spec = spec(scenario, short_bounds());
    let source = CancellationSource::default();
    let mut records = Vec::new();
    run(spec, &source.token(), &mut |record| {
        records.push(record.to_vec());
        Ok(())
    })
    .unwrap();
    assert_eq!(records, vec![b"one\r".to_vec(), b"two\r".to_vec()]);
}

#[test]
fn a_descendant_retaining_the_inherited_pipe_does_not_hang_the_supervisor() {
    let path = report_path("descendant-holding-pipe");
    // The child exits almost immediately but first spawns a detached
    // descendant that inherits stdout/stderr and sleeps for much longer
    // than our deadline. Without group cleanup + "drain, don't wait for
    // EOF", the supervisor would block on stdout forever waiting for a
    // close that only the descendant's own exit would produce.
    let scenario = fixture_backend::scenario_full(&[], &[], 0, false, Some(&path), Some(60_000));
    let bounds = bounds(Duration::from_secs(5), 1024 * 1024, 1024 * 1024);
    let spec = spec(scenario, bounds);
    let source = CancellationSource::default();
    let start = std::time::Instant::now();
    let outcome = run(spec, &source.token(), &mut noop).unwrap();
    let elapsed = start.elapsed();
    assert_eq!(outcome.exit_code, 0);
    assert!(
        elapsed < Duration::from_secs(5),
        "must not wait for the descendant's own EOF, took {elapsed:?}"
    );

    let report = read_report(&path);
    let descendant_pid = report
        .descendant_pid
        .expect("fixture reports the descendant pid");
    assert_process_gone(report.pid, "direct child with descendant");
    assert_process_gone(descendant_pid, "descendant retaining pipes");
}

#[test]
fn broken_stdin_delivery_is_reported_without_hanging() {
    // drain_stdin is false, so the fixture never reads stdin at all; the
    // supervisor's writes should hit a full-then-broken pipe and abandon
    // delivery rather than hanging, per this module's documented policy.
    let large_stdin = vec![b'a'; 2 * 1024 * 1024];
    let scenario = fixture_backend::scenario(&[b"done\n"], &[], 0);
    let spec = ProcessSpec {
        workspace: cyoa_infrastructure::backends::process::RequestWorkspace::new().unwrap(),
        program: FIXTURE_EXE.into(),
        args: vec![OsString::from(scenario)],
        env: EnvPolicy::new(),
        stdin: large_stdin,
        bounds: short_bounds(),
    };
    let source = CancellationSource::default();
    let start = std::time::Instant::now();
    let error = run(spec, &source.token(), &mut noop).unwrap_err();
    assert!(matches!(error, SupervisorError::IncompleteInput { .. }));
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn a_child_that_never_reads_stdin_still_completes() {
    // Delivery to the pipe (not consumption by the child) is required. An
    // immediately exiting fixture races the write and can correctly produce
    // IncompleteInput instead. Wait for writer closure without reading bytes.
    let input = b"unread request payload";
    let path = report_path("unread-stdin");
    let scenario = serde_json::json!({
        "stdout": [{"bytes": b"ok\n"}],
        "unread_stdin_bytes_after_close": input.len(),
        "report_path": path,
        "exit_code": 0,
    })
    .to_string();
    let spec = ProcessSpec {
        workspace: cyoa_infrastructure::backends::process::RequestWorkspace::new().unwrap(),
        program: FIXTURE_EXE.into(),
        args: vec![OsString::from(scenario)],
        env: EnvPolicy::new(),
        stdin: input.to_vec(),
        bounds: short_bounds(),
    };
    let source = CancellationSource::default();
    let outcome = run(spec, &source.token(), &mut noop).unwrap();
    assert_eq!(outcome.exit_code, 0);
    assert_eq!(outcome.diagnostics.stdout(), b"ok\n");
    let report = read_report(&path);
    assert!(
        report.stdin.is_empty(),
        "fixture must not consume the request"
    );
    assert_process_gone(report.pid, "non-reading child after complete delivery");
}

// --- Nominal tests --------------------------------------------------------

#[test]
fn exact_argv_and_stdin_reach_the_child_and_both_pipes_are_drained() {
    let path = report_path("argv-and-stdin");
    let scenario = fixture_backend::scenario_with_report(
        &[b"hello "],
        &[],
        0,
        true, // drain_stdin
        Some(&path),
    );
    let spec = ProcessSpec {
        workspace: cyoa_infrastructure::backends::process::RequestWorkspace::new().unwrap(),
        program: FIXTURE_EXE.into(),
        args: vec![OsString::from(scenario)],
        env: EnvPolicy::new(),
        stdin: b"the exact prompt".to_vec(),
        bounds: short_bounds(),
    };
    let source = CancellationSource::default();
    let mut records = Vec::new();
    let outcome = run(spec, &source.token(), &mut |record| {
        records.push(record.to_vec());
        Ok(())
    })
    .unwrap();
    assert_eq!(outcome.exit_code, 0);
    assert_eq!(records, vec![b"hello ".to_vec()]);

    let report = read_report(&path);
    assert_eq!(report.stdin, b"the exact prompt");
}

#[test]
fn env_policy_is_an_explicit_allowlist_not_ambient_inheritance() {
    // This test process (the harness cargo builds) has CARGO_MANIFEST_DIR
    // set by cargo itself; a child launched with an empty `EnvPolicy` must
    // not see it, proving `run` calls `env_clear()` rather than inheriting.
    assert!(std::env::var("CARGO_MANIFEST_DIR").is_ok());
    let path = report_path("env-policy");
    let scenario = fixture_backend::scenario_with_report(&[], &[], 0, false, Some(&path));
    let spec = ProcessSpec {
        workspace: cyoa_infrastructure::backends::process::RequestWorkspace::new().unwrap(),
        program: FIXTURE_EXE.into(),
        args: vec![OsString::from(scenario)],
        env: EnvPolicy::new().set("ONLY_THIS_VAR", "present"),
        stdin: Vec::new(),
        bounds: short_bounds(),
    };
    let source = CancellationSource::default();
    run(spec, &source.token(), &mut noop).unwrap();

    let report = read_report(&path);
    assert!(
        report.env_keys.contains("ONLY_THIS_VAR"),
        "the one explicitly set var must reach the child"
    );
    assert!(
        !report.env_keys.contains("CARGO_MANIFEST_DIR"),
        "child must not inherit the supervisor's own ambient environment"
    );
    assert_eq!(
        report.env_keys.len(),
        1,
        "an empty-allowlist-plus-one-var policy must leave exactly one variable, got {:?}",
        report.env_keys
    );
}

#[test]
fn multiple_records_are_delivered_in_order_and_success_requires_zero_exit() {
    let scenario = fixture_backend::scenario(&[b"one\ntwo\nthree\n"], &[], 0);
    let spec = spec(scenario, short_bounds());
    let source = CancellationSource::default();
    let mut records = Vec::new();
    let outcome = run(spec, &source.token(), &mut |record| {
        records.push(String::from_utf8(record.to_vec()).unwrap());
        Ok(())
    })
    .unwrap();
    assert_eq!(outcome.exit_code, 0);
    assert_eq!(records, vec!["one", "two", "three"]);
}

#[test]
fn cancelling_from_inside_the_record_consumer_wins_over_a_same_tick_exit() {
    let scenario = fixture_backend::scenario(&[b"only-record\n"], &[], 0);
    let spec = spec(scenario, short_bounds());
    let source = CancellationSource::default();
    let token = source.token();
    let error = run(spec, &token, &mut |_record| {
        source.cancel();
        Ok(())
    })
    .unwrap_err();
    assert!(matches!(error, SupervisorError::Cancelled { .. }));
}

/// The child exits (almost) immediately after writing a record with no
/// trailing newline. That record can only be delivered from the exit
/// branch's own final-record handling (there is no other newline to split
/// on). Cancelling from inside that delivery must still win over the
/// same-tick successful exit the way the mid-stream case already does.
#[test]
fn cancelling_from_inside_the_final_no_newline_record_still_wins_over_exit() {
    let scenario = fixture_backend::scenario(&[b"final-no-newline"], &[], 0);
    let spec = spec(scenario, short_bounds());
    let source = CancellationSource::default();
    let token = source.token();
    let mut delivered = Vec::new();
    let error = run(spec, &token, &mut |record| {
        delivered.push(record.to_vec());
        source.cancel();
        Ok(())
    })
    .unwrap_err();
    assert!(
        matches!(error, SupervisorError::Cancelled { .. }),
        "expected Cancelled, got {error:?}"
    );
    assert_eq!(delivered, vec![b"final-no-newline".to_vec()]);
}

/// Many short, distinct records the child writes in one `write_all` right
/// before exiting. This does NOT exceed a real pipe buffer (argv has its
/// own ~128 KiB OS size limit, and JSON byte-array encoding costs about
/// 3.5x the raw bytes, so this scenario tops out around 20 KiB) — that
/// larger case is `stdout_larger_than_pipe_capacity_is_fully_delivered_and_diagnostics_agree`,
/// which uses `repeated_chunk_scenario` to route around the argv limit.
/// What this test isolates instead: ordering and completeness of many
/// *distinct* records delivered from the exit branch specifically. A drain
/// that only updates diagnostics without also feeding the record
/// accumulator would silently drop records the child wrote between the
/// parent's last read and exit detection.
#[test]
fn records_written_right_before_exit_are_not_silently_dropped() {
    // One chunk (the fixture writes it in one `write_all`) containing many
    // newline-separated records: enough total bytes to exceed a pipe's
    // buffer, so the child can finish writing and exit before the parent
    // has drained everything — argv has its own OS size limit, so this
    // stays a single JSON chunk rather than one array entry per record.
    let mut expected = Vec::new();
    let mut stdout = Vec::new();
    for i in 0..4000u32 {
        expected.push(i.to_string());
        stdout.extend_from_slice(format!("{i}\n").as_bytes());
    }
    let scenario = fixture_backend::scenario(&[&stdout], &[], 0);
    let spec = spec(scenario, short_bounds());
    let source = CancellationSource::default();
    let mut records = Vec::new();
    let outcome = run(spec, &source.token(), &mut |record| {
        records.push(String::from_utf8(record.to_vec()).unwrap());
        Ok(())
    })
    .unwrap();
    assert_eq!(outcome.exit_code, 0);
    assert_eq!(
        records.len(),
        expected.len(),
        "no record may be silently dropped at exit"
    );
    assert_eq!(records, expected);
}

/// A consumer that panics must not leak the child: `ChildGuard::drop` is the
/// backstop for exactly this case (a `finish()`-routed exit path never
/// runs). Checks process existence through the same kernel probe as normal exits.
#[test]
fn a_panicking_consumer_still_gets_the_child_cleaned_up() {
    let path = report_path("panicking-consumer");
    let scenario =
        fixture_backend::scenario_with_report(&[b"trigger\n"], &[], 0, false, Some(&path));
    let spec = spec(scenario, short_bounds());
    let source = CancellationSource::default();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run(spec, &source.token(), &mut |_record| {
            panic!("consumer panics mid-record");
        })
    }));
    assert!(
        result.is_err(),
        "expected the panic to propagate out of run()"
    );

    // The fixture writes its report (atomically) before emitting any
    // output, and the consumer only panics once a record arrives, so the
    // report must exist by the time the panic can have happened.
    assert!(
        wait_for_file(&path, Duration::from_secs(2)),
        "fixture never wrote its report before the consumer could have panicked"
    );
    let report = read_report(&path);
    assert_process_gone(report.pid, "panicking consumer");
}

// --- Backpressure (real child, not a mock) ---------------------------------

/// The plan requires "input larger than pipe capacity" to be exercised
/// against a real child, not a mock: a typical pipe buffer is 64 KiB, so
/// over 1 MiB of stdin that the child actually reads (`drain_stdin: true`)
/// cannot be written in one non-blocking `write()` and must be spread
/// across multiple `POLLOUT`-ready ticks.
#[test]
fn stdin_larger_than_pipe_capacity_is_delivered_in_full_when_the_child_reads_it() {
    let path = report_path("stdin-backpressure");
    let input: Vec<u8> = (0..1_500_000u32).map(|i| (i % 251) as u8).collect();
    let scenario = fixture_backend::scenario_with_report(&[b"ok\n"], &[], 0, true, Some(&path));
    let spec = ProcessSpec {
        workspace: cyoa_infrastructure::backends::process::RequestWorkspace::new().unwrap(),
        program: FIXTURE_EXE.into(),
        args: vec![OsString::from(scenario)],
        env: EnvPolicy::new(),
        stdin: input.clone(),
        bounds: short_bounds(),
    };
    let source = CancellationSource::default();
    let outcome = run(spec, &source.token(), &mut noop).unwrap();
    assert_eq!(outcome.exit_code, 0);

    let report = read_report(&path);
    assert_eq!(
        report.stdin.len(),
        input.len(),
        "the child must receive every stdin byte, not a pipe-buffer-sized prefix"
    );
    assert_eq!(report.stdin, input);
}

/// The stdout counterpart: "output larger than pipe capacity" against a
/// real child. `repeated_chunk_scenario` keeps the scenario JSON itself
/// small (argv has its own OS size limit) while the fixture's own
/// `write_all` loop produces real backpressure on the pipe.
#[test]
fn stdout_larger_than_pipe_capacity_is_fully_delivered_and_diagnostics_agree() {
    let line = b"the quick brown fox jumps over the lazy dog\n";
    let repeat = 4000u32; // ~180 KiB total, well over a typical 64 KiB pipe buffer
    let scenario = fixture_backend::repeated_chunk_scenario(line, repeat, 0, None);
    let big_bounds = bounds(Duration::from_secs(5), 4 * 1024 * 1024, 1024 * 1024);
    let spec = spec(scenario, big_bounds);
    let source = CancellationSource::default();
    let mut record_count = 0u32;
    let outcome = run(spec, &source.token(), &mut |record| {
        assert_eq!(record, &line[..line.len() - 1]); // newline stripped by framing
        record_count += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(outcome.exit_code, 0);
    assert_eq!(record_count, repeat);
    assert_eq!(
        outcome.diagnostics.stdout().len(),
        line.len() * repeat as usize
    );
}

#[test]
fn incomplete_request_delivery_cannot_report_success() {
    let spec = ProcessSpec {
        workspace: cyoa_infrastructure::backends::process::RequestWorkspace::new().unwrap(),
        program: FIXTURE_EXE.into(),
        args: vec![fixture_backend::scenario(&[b"{\"ok\":true}\n"], &[], 0).into()],
        env: EnvPolicy::new(),
        stdin: vec![b'a'; 2 * 1024 * 1024],
        bounds: short_bounds(),
    };
    let result = run(spec, &CancellationSource::default().token(), &mut noop);
    assert!(
        result.is_err(),
        "known undelivered request bytes must not produce success"
    );
}

#[test]
fn cancelled_token_wins_even_before_its_notifier_runs() {
    let path = report_path("delayed-notifier");
    let source = CancellationSource::default();
    let token = source.token();
    let (release, wait_release) = std::sync::mpsc::channel();
    token.on_cancel(move || {
        wait_release.recv().unwrap();
    });
    let signal_path = path.clone();
    let canceller = std::thread::spawn(move || {
        assert!(wait_for_file(&signal_path, Duration::from_secs(2)));
        source.cancel();
    });
    let scenario =
        serde_json::json!({"stdout":[],"report_path":path,"hang_ms":200,"exit_code":0}).to_string();
    let result = run(spec(scenario, short_bounds()), &token, &mut noop);
    let signalled = token.is_cancelled();
    release.send(()).unwrap();
    canceller.join().unwrap();
    let report = read_report(&path);
    assert_process_gone(report.pid, "delayed notifier");
    assert!(signalled);
    assert!(
        matches!(result, Err(SupervisorError::Cancelled { .. })),
        "cancelled token yielded {result:?}"
    );
}

#[test]
fn captured_output_never_exceeds_either_configured_bound() {
    for stderr in [false, true] {
        let path = report_path(if stderr { "stderr-cap" } else { "stdout-cap" });
        let bytes = vec![b'x'; 4096];
        let scenario = if stderr {
            fixture_backend::scenario_with_report(&[], &[&bytes], 0, false, Some(&path))
        } else {
            fixture_backend::scenario_with_report(&[&bytes], &[], 0, false, Some(&path))
        };
        let spec = spec(scenario, bounds(Duration::from_secs(2), 100, 100));
        let error = run(spec, &CancellationSource::default().token(), &mut noop).unwrap_err();
        assert!(matches!(error, SupervisorError::OutputBoundExceeded { .. }));
        let diagnostics = error.diagnostics();
        let capture = if stderr {
            diagnostics.stderr_capture()
        } else {
            diagnostics.stdout_capture()
        };
        assert_eq!(
            capture.completeness(),
            cyoa_application::diagnostics::CaptureCompleteness::Prefix
        );
        let captured = if stderr {
            diagnostics.stderr()
        } else {
            diagnostics.stdout()
        };
        let report = read_report(&path);
        assert_process_gone(report.pid, "capture limit");
        assert_eq!(captured, vec![b'x'; 100]);
    }
}

#[test]
fn capture_at_the_exact_bound_is_complete_and_preserves_arbitrary_bytes() {
    use cyoa_application::diagnostics::CaptureCompleteness;
    let bytes = [0xff, b'\r', b'\n', 0xfe];
    let scenario = fixture_backend::scenario(&[&bytes], &[&bytes], 0);
    let outcome = run(
        spec(scenario, bounds(Duration::from_secs(5), 4, 4)),
        &CancellationSource::default().token(),
        &mut noop,
    )
    .unwrap();
    assert_eq!(outcome.diagnostics.stdout(), bytes);
    assert_eq!(outcome.diagnostics.stderr(), bytes);
    assert_eq!(
        outcome.diagnostics.stdout_capture().completeness(),
        CaptureCompleteness::Complete
    );
    assert_eq!(
        outcome.diagnostics.stderr_capture().completeness(),
        CaptureCompleteness::Complete
    );
}

#[test]
fn cancellation_and_deadline_stop_continuous_writers_with_bounded_diagnostics() {
    for stderr in [false, true] {
        for cancel in [false, true] {
            let path = report_path(&format!("continuous-{stderr}-{cancel}"));
            let chunks = serde_json::json!([{"bytes": vec![b'x'; 1024], "repeat": u32::MAX, "repeat_delay_ms": if cancel { 0 } else { 1 }}]);
            let scenario = serde_json::json!({
                "stdout": if stderr { serde_json::json!([]) } else {chunks.clone()},
                "stderr": if stderr { chunks } else {serde_json::json!([])},
                "report_path": path, "exit_code": 0,
            })
            .to_string();
            let source = CancellationSource::default();
            let token = source.token();
            let handshake = path.clone();
            let canceller = cancel.then(|| {
                std::thread::spawn(move || {
                    assert!(wait_for_file(&handshake, Duration::from_secs(2)));
                    source.cancel();
                })
            });
            let start = std::time::Instant::now();
            let error = run(
                spec(
                    scenario,
                    bounds(
                        Duration::from_millis(80),
                        64 * 1024 * 1024,
                        64 * 1024 * 1024,
                    ),
                ),
                &token,
                &mut noop,
            )
            .unwrap_err();
            if let Some(canceller) = canceller {
                canceller.join().unwrap();
            }
            assert!(
                matches!(
                    error,
                    SupervisorError::Cancelled { .. } | SupervisorError::Timeout { .. }
                ),
                "{error}"
            );
            assert!(start.elapsed() < Duration::from_secs(2));
            assert!(error.diagnostics().stdout().len() <= 64 * 1024 * 1024);
            assert!(error.diagnostics().stderr().len() <= 64 * 1024 * 1024);
            assert_process_gone(read_report(&path).pid, "continuous writer");
        }
    }
}

#[test]
fn requests_do_not_inherit_the_repository_working_directory() {
    let parent = std::env::current_dir().unwrap();
    let path = report_path("cwd-isolation");
    let scenario = fixture_backend::scenario_with_report(&[], &[], 0, false, Some(&path));
    run(
        spec(scenario, short_bounds()),
        &CancellationSource::default().token(),
        &mut noop,
    )
    .unwrap();
    let report = read_report(&path);
    assert_eq!(
        std::env::current_dir().unwrap(),
        parent,
        "global cwd must not change"
    );
    assert_ne!(report.cwd, parent, "the child inherited the repository");
    assert!(
        !report.cwd.exists(),
        "request directory must be removed after completion"
    );
}

#[test]
fn request_files_live_through_execution_and_are_removed_on_every_outcome() {
    for ending in [
        "success", "nonzero", "reject", "panic", "bound", "timeout", "cancel", "spawn",
    ] {
        let scenario = fixture_backend::scenario_hanging(
            &[b"record\n"],
            &[],
            if ending == "nonzero" { 7 } else { 0 },
            false,
            None,
            None,
            (ending == "timeout").then_some(30_000),
        );
        let mut spec = spec(
            scenario,
            bounds(
                Duration::from_millis(150),
                if ending == "bound" { 1 } else { 100 },
                100,
            ),
        );
        if ending == "spawn" {
            spec.program = "/not/a/cyoa-executable".into();
        }
        let directory = spec.workspace.path().to_owned();
        let schema = directory.join("request.schema.json");
        std::fs::write(&schema, b"{}").unwrap();
        let source = CancellationSource::default();
        if ending == "cancel" {
            source.cancel();
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run(spec, &source.token(), &mut |_| {
                assert_eq!(std::fs::read(&schema).unwrap(), b"{}");
                match ending {
                    "panic" => panic!("consumer panic"),
                    "reject" => Err("consumer rejected".into()),
                    _ => Ok(()),
                }
            })
        }));
        if ending == "panic" {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap().is_ok(), ending == "success", "{ending}");
        }
        assert!(
            !directory.exists(),
            "scratch directory leaked after {ending}"
        );
    }
}

#[test]
fn request_workspace_is_private_and_unique() {
    use cyoa_infrastructure::backends::process::RequestWorkspace;
    use std::os::unix::fs::PermissionsExt;
    let first = RequestWorkspace::new().unwrap();
    let second = RequestWorkspace::new().unwrap();
    assert_ne!(first.path(), second.path());
    assert_eq!(
        std::fs::metadata(first.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o077,
        0
    );
}
