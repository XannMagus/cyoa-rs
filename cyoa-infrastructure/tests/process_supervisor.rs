//! Real-process tests for the vendor-neutral process supervisor (Phase 1
//! item 3). Every test here drives the real `subprocess_fixture` binary
//! through `cyoa_infrastructure::backends::process::run`; none of them use a
//! mock child, per the plan's "Require an actual child test for idle
//! cancellation and backpressure; mocks alone cannot establish these."
//!
//! Unix-only: the supervisor itself is `#[cfg(unix)]`-gated and this test
//! file is not compiled or run on any other platform.
#![cfg(unix)]

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

// --- Error tests ------------------------------------------------------

#[test]
fn missing_executable_is_a_spawn_error_not_a_panic() {
    let spec = ProcessSpec {
        program: "/definitely/not/a/real/executable-cyoa-test".into(),
        args: vec![],
        env: EnvPolicy::new(),
        stdin: Vec::new(),
        bounds: short_bounds(),
    };
    let source = CancellationSource::default();
    let error = run(&spec, &source.token(), &mut noop).unwrap_err();
    assert!(matches!(error, SupervisorError::Spawn(_)));
}

#[test]
fn cancellation_before_spawn_launches_no_process() {
    let path = report_path("cancel-before-spawn");
    let scenario = fixture_backend::scenario_with_report(&[], &[], 0, false, Some(&path));
    let spec = spec(scenario, short_bounds());
    let source = CancellationSource::default();
    source.cancel();
    let error = run(&spec, &source.token(), &mut noop).unwrap_err();
    assert!(matches!(error, SupervisorError::Cancelled { .. }));
    assert!(
        !path.exists(),
        "no process should have been launched, so no report file should exist"
    );
}

#[test]
fn cancellation_while_the_child_is_silent_still_wakes_the_poll_loop_and_returns_promptly() {
    // The child sleeps far longer than we're willing to wait, producing no
    // output at all: if the supervisor only checked cancellation on output
    // readiness (the exact bug this design exists to prevent), this test
    // would hang until its own external timeout kills it instead of
    // returning quickly.
    let scenario = serde_json::json!({
        "stdout": [], "stderr": [], "drain_stdin": false,
        "spawn_descendant_holding_stdout_ms": null, "report_path": null, "exit_code": 0,
    })
    .to_string();
    let long_bounds = bounds(Duration::from_secs(30), 1024 * 1024, 1024 * 1024);
    let spec = ProcessSpec {
        program: FIXTURE_EXE.into(),
        args: vec![OsString::from(scenario)],
        env: EnvPolicy::new().set("SUBPROCESS_FIXTURE_SLEEP_MS", "30000"),
        stdin: Vec::new(),
        bounds: long_bounds,
    };
    let source = CancellationSource::default();
    let token = source.token();

    let start = std::time::Instant::now();
    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        source.cancel();
    });
    let error = run(&spec, &token, &mut noop).unwrap_err();
    canceller.join().unwrap();
    let elapsed = start.elapsed();

    assert!(matches!(error, SupervisorError::Cancelled { .. }));
    assert!(
        elapsed < Duration::from_secs(5),
        "cancellation while idle must wake the poll loop promptly, took {elapsed:?}"
    );
}

#[test]
fn nonzero_exit_is_reported_with_diagnostics() {
    let scenario = fixture_backend::scenario(&[], &[b"boom"], 7);
    let spec = spec(scenario, short_bounds());
    let source = CancellationSource::default();
    let error = run(&spec, &source.token(), &mut noop).unwrap_err();
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
    let error = run(&spec, &source.token(), &mut |record| {
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
    // The fixture's sleep re-exec path (SUBPROCESS_FIXTURE_SLEEP_MS) isn't
    // reachable without spawning the fixture itself with that env var, which
    // is exactly what a "hangs forever" scenario looks like from the
    // supervisor's point of view: no output, no exit, within our bound.
    let scenario = serde_json::json!({
        "stdout": [],
        "stderr": [],
        "drain_stdin": false,
        "spawn_descendant_holding_stdout_ms": null,
        "report_path": null,
        "exit_code": 0,
    })
    .to_string();
    let tight_bounds = bounds(Duration::from_millis(150), 1024, 1024);
    let spec = ProcessSpec {
        program: FIXTURE_EXE.into(),
        args: vec![OsString::from(scenario)],
        env: EnvPolicy::new().set("SUBPROCESS_FIXTURE_SLEEP_MS", "60000"),
        stdin: Vec::new(),
        bounds: tight_bounds,
    };
    let source = CancellationSource::default();
    let start = std::time::Instant::now();
    let error = run(&spec, &source.token(), &mut noop).unwrap_err();
    let elapsed = start.elapsed();
    assert!(matches!(error, SupervisorError::Timeout { .. }));
    assert!(elapsed < Duration::from_secs(5), "took {elapsed:?}");
}

#[test]
fn output_bound_exceeded_stops_the_child_instead_of_buffering_forever() {
    let big: Vec<u8> = vec![b'x'; 4096];
    let scenario = fixture_backend::scenario(&[&big], &[], 0);
    let tiny_bounds = bounds(Duration::from_secs(5), 100, 1024);
    let spec = spec(scenario, tiny_bounds);
    let source = CancellationSource::default();
    let error = run(&spec, &source.token(), &mut noop).unwrap_err();
    assert!(matches!(
        error,
        SupervisorError::OutputBoundExceeded {
            stream: cyoa_infrastructure::backends::process::OutputStream::Stdout,
            ..
        }
    ));
}

#[test]
fn consumer_rejection_triggers_cleanup_and_reports_the_reason() {
    let scenario = fixture_backend::scenario(&[b"not json at all\n"], &[], 0);
    let spec = spec(scenario, short_bounds());
    let source = CancellationSource::default();
    let error = run(&spec, &source.token(), &mut |record| {
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
}

// --- Edge tests ---------------------------------------------------------

#[test]
fn final_record_without_a_trailing_newline_is_still_delivered() {
    let scenario = fixture_backend::scenario(&[b"no newline here"], &[], 0);
    let spec = spec(scenario, short_bounds());
    let source = CancellationSource::default();
    let mut records = Vec::new();
    let outcome = run(&spec, &source.token(), &mut |record| {
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
    run(&spec, &source.token(), &mut |record| {
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
    let outcome = run(&spec, &source.token(), &mut noop).unwrap();
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
    // Give the group-kill a brief moment to land, then confirm the
    // descendant is actually gone (not just that `run` returned).
    std::thread::sleep(Duration::from_millis(200));
    let alive = std::path::Path::new(&format!("/proc/{descendant_pid}")).exists();
    assert!(
        !alive,
        "descendant pid {descendant_pid} should have been killed with the group"
    );
}

#[test]
fn broken_stdin_delivery_does_not_hang_and_the_process_still_completes() {
    // drain_stdin is false, so the fixture never reads stdin at all; the
    // supervisor's writes should hit a full-then-broken pipe and abandon
    // delivery rather than hanging, per this module's documented policy.
    let large_stdin = vec![b'a'; 2 * 1024 * 1024];
    let scenario = fixture_backend::scenario(&[b"done\n"], &[], 0);
    let spec = ProcessSpec {
        program: FIXTURE_EXE.into(),
        args: vec![OsString::from(scenario)],
        env: EnvPolicy::new(),
        stdin: large_stdin,
        bounds: short_bounds(),
    };
    let source = CancellationSource::default();
    let start = std::time::Instant::now();
    let outcome = run(&spec, &source.token(), &mut noop).unwrap();
    assert_eq!(outcome.exit_code, 0);
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn a_child_that_never_reads_stdin_still_completes() {
    let scenario = fixture_backend::scenario(&[b"ok\n"], &[], 0);
    let spec = ProcessSpec {
        program: FIXTURE_EXE.into(),
        args: vec![OsString::from(scenario)],
        env: EnvPolicy::new(),
        stdin: b"unread request payload".to_vec(),
        bounds: short_bounds(),
    };
    let source = CancellationSource::default();
    let outcome = run(&spec, &source.token(), &mut noop).unwrap();
    assert_eq!(outcome.exit_code, 0);
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
        program: FIXTURE_EXE.into(),
        args: vec![OsString::from(scenario)],
        env: EnvPolicy::new(),
        stdin: b"the exact prompt".to_vec(),
        bounds: short_bounds(),
    };
    let source = CancellationSource::default();
    let mut records = Vec::new();
    let outcome = run(&spec, &source.token(), &mut |record| {
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
        program: FIXTURE_EXE.into(),
        args: vec![OsString::from(scenario)],
        env: EnvPolicy::new().set("ONLY_THIS_VAR", "present"),
        stdin: Vec::new(),
        bounds: short_bounds(),
    };
    let source = CancellationSource::default();
    run(&spec, &source.token(), &mut noop).unwrap();

    let report = read_report(&path);
    assert_eq!(
        report.env.get("ONLY_THIS_VAR").map(String::as_str),
        Some("present")
    );
    assert!(
        !report.env.contains_key("CARGO_MANIFEST_DIR"),
        "child must not inherit the supervisor's own ambient environment"
    );
}

#[test]
fn multiple_records_are_delivered_in_order_and_success_requires_zero_exit() {
    let scenario = fixture_backend::scenario(&[b"one\ntwo\nthree\n"], &[], 0);
    let spec = spec(scenario, short_bounds());
    let source = CancellationSource::default();
    let mut records = Vec::new();
    let outcome = run(&spec, &source.token(), &mut |record| {
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
    let error = run(&spec, &token, &mut |_record| {
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
    let error = run(&spec, &token, &mut |record| {
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

/// Enough short records that the child can exit before the parent has
/// finished reading everything the child already wrote (a full/near-full
/// pipe at exit time). Every record delivered to the consumer must be
/// backed by the same bytes reflected in diagnostics — a drain that only
/// updates diagnostics without also feeding the record accumulator would
/// silently drop records the child wrote between the parent's last read and
/// exit detection.
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
    let outcome = run(&spec, &source.token(), &mut |record| {
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
/// runs). Uses `/proc/<pid>` (Linux-only, matching this test file's other
/// process-existence checks) rather than a second rustix dependency in the
/// test crate.
#[test]
fn a_panicking_consumer_still_gets_the_child_cleaned_up() {
    let path = report_path("panicking-consumer");
    let scenario =
        fixture_backend::scenario_with_report(&[b"trigger\n"], &[], 0, false, Some(&path));
    let spec = spec(scenario, short_bounds());
    let source = CancellationSource::default();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run(&spec, &source.token(), &mut |_record| {
            panic!("consumer panics mid-record");
        })
    }));
    assert!(
        result.is_err(),
        "expected the panic to propagate out of run()"
    );

    // The fixture only writes its report before emitting stdout, so it may
    // not exist if the parent already tore the child down before that
    // point; when it does exist, the pid it names must be gone.
    if path.exists() {
        let report = read_report(&path);
        std::thread::sleep(Duration::from_millis(200));
        let alive = std::path::Path::new(&format!("/proc/{}", report.pid)).exists();
        assert!(
            !alive,
            "pid {} should have been cleaned up by ChildGuard::drop",
            report.pid
        );
    }
}
