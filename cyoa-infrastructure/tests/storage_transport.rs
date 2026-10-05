#![cfg(target_os = "linux")]
use cyoa_application::{cancellation::CancellationSource, persistence::*};
use cyoa_infrastructure::{
    backends::process::{MaxStderrBytes, MaxStdoutBytes, ProcessBounds},
    persistence::{
        codec,
        helper::{HelperConfig, SupervisedRepository},
        repository::LocalRepository,
    },
};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
fn snapshot() -> SaveSnapshot {
    let id = SaveId::new("harbour-0123456789abcdef0123456789abcdef").unwrap();
    codec::decode(
        include_bytes!("fixtures/saves/v1-full-story.json"),
        &id,
        SaveCopy::Primary,
    )
    .unwrap()
    .snapshot
}
fn config(root: &std::path::Path, deadline: Duration, cap: usize) -> HelperConfig {
    HelperConfig::new(
        PathBuf::from(env!("CARGO_BIN_EXE_storage_fixture")),
        root.into(),
    )
    .unwrap()
    .with_bounds(
        ProcessBounds::new(
            deadline,
            MaxStdoutBytes::new(cap).unwrap(),
            MaxStderrBytes::new(65536).unwrap(),
        )
        .unwrap(),
    )
}
fn wait(root: &std::path::Path) -> rustix::process::Pid {
    let end = Instant::now() + Duration::from_secs(5);
    while !root.join(".fixture-ready").exists() {
        assert!(Instant::now() < end, "child handshake missing");
        std::thread::sleep(Duration::from_millis(2));
    }
    rustix::process::Pid::from_raw(
        fs::read_to_string(root.join(".fixture-pid"))
            .unwrap()
            .parse()
            .unwrap(),
    )
    .unwrap()
}
fn gone(pid: rustix::process::Pid) {
    assert_eq!(
        rustix::process::test_kill_process(pid),
        Err(rustix::io::Errno::SRCH)
    );
}
fn isolated_and_cleaned(root: &std::path::Path) {
    assert!(fs::read(root.join(".fixture-env")).unwrap().is_empty());
    let cwd = PathBuf::from(fs::read_to_string(root.join(".fixture-cwd")).unwrap());
    assert_ne!(cwd, std::env::current_dir().unwrap());
    assert!(!cwd.exists());
}
#[test]
fn silent_helper_cancellation_and_timeout_reap_children_and_keep_prepared_identity() {
    for cancel in [true, false] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(".fixture-mode"), "silent").unwrap();
        let source = CancellationSource::default();
        let token = source.token();
        let evidence = PreparedWriteEvidence::default();
        let retained = evidence.clone();
        let mut repo = SupervisedRepository::new(config(
            root.path(),
            if cancel {
                Duration::from_secs(5)
            } else {
                Duration::from_millis(250)
            },
            160 * 1024 * 1024,
        ));
        let worker = std::thread::spawn(move || repo.create(snapshot(), &evidence, &token));
        let pid = wait(root.path());
        let pending = retained
            .pending()
            .expect("published before helper handshake");
        let start = Instant::now();
        if cancel {
            source.cancel();
        }
        let error = worker.join().unwrap().unwrap_err();
        assert_eq!(
            error.kind,
            if cancel {
                StorageFailureKind::Cancelled
            } else {
                StorageFailureKind::Timeout
            }
        );
        assert_eq!(error.visibility, WriteVisibility::Unknown);
        assert_eq!(error.pending.as_deref(), Some(&pending));
        assert!(start.elapsed() < Duration::from_secs(2));
        gone(pid);
        isolated_and_cleaned(root.path());
        assert_eq!(
            fs::read_to_string(root.path().join(".fixture-requests")).unwrap(),
            "Apply\n"
        );
        assert!(!root.path().join("saves").exists());
    }
}
#[test]
fn lost_invalid_multiple_or_capped_replies_never_authorize_success_and_reconcile_exact_write() {
    for mode in [
        "lost-reply",
        "malformed",
        "wrong-receipt",
        "double",
        "capped",
    ] {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join(".fixture-mode"),
            if mode == "capped" { "lost-reply" } else { mode },
        )
        .unwrap();
        let evidence = PreparedWriteEvidence::default();
        let mut repo = SupervisedRepository::new(config(
            root.path(),
            Duration::from_secs(5),
            if mode == "capped" {
                20
            } else {
                160 * 1024 * 1024
            },
        ));
        let token = CancellationSource::default().token();
        let error = repo.create(snapshot(), &evidence, &token).unwrap_err();
        assert_eq!(error.visibility, WriteVisibility::Unknown, "{mode}");
        let pending = error.pending.unwrap();
        assert_eq!(evidence.pending().as_ref(), Some(pending.as_ref()));
        gone(wait(root.path()));
        isolated_and_cleaned(root.path());
        assert_eq!(
            fs::read_to_string(root.path().join(".fixture-requests")).unwrap(),
            "Apply\n"
        );
        let mut local = LocalRepository::new(root.path().into()).unwrap();
        let receipt = local
            .reconcile(*pending.clone(), &evidence, &token)
            .unwrap();
        assert_eq!(receipt.stamp, pending.intended_stamp());
        assert_eq!(&receipt.metadata.id, pending.target());
        assert_eq!(receipt.metadata.revision.get(), 1);
        assert_eq!(
            local
                .list(SavePage::new(None, 100).unwrap(), &token)
                .unwrap()
                .entries
                .len(),
            1
        );
        assert!(
            !root
                .path()
                .join("saves")
                .join(format!("{}.json.bak", pending.target().as_str()))
                .exists()
        );
    }
}

#[test]
fn preparation_exhausting_the_total_deadline_launches_no_helper_or_disk_write() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(".fixture-mode"), "silent").unwrap();
    let evidence = PreparedWriteEvidence::default();
    let mut repo = SupervisedRepository::new(config(
        root.path(),
        Duration::from_nanos(1),
        160 * 1024 * 1024,
    ));
    let token = CancellationSource::default().token();
    let error = repo.create(snapshot(), &evidence, &token).unwrap_err();
    assert_eq!(error.kind, StorageFailureKind::Timeout);
    assert_eq!(error.visibility, WriteVisibility::Unchanged);
    assert_eq!(error.pending.as_deref(), evidence.pending().as_ref());
    assert!(!root.path().join(".fixture-ready").exists());
    assert!(!root.path().join("saves").exists());
}

#[test]
fn replacement_prepares_with_one_read_child_and_dispatches_one_mutating_child() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(".fixture-mode"), "normal").unwrap();
    let evidence = PreparedWriteEvidence::default();
    let mut repo = SupervisedRepository::new(config(
        root.path(),
        Duration::from_secs(5),
        160 * 1024 * 1024,
    ));
    let token = CancellationSource::default().token();
    let first = repo.create(snapshot(), &evidence, &token).unwrap();
    fs::write(root.path().join(".fixture-requests"), "").unwrap();
    let second = repo
        .replace(
            SaveTarget {
                id: first.metadata.id.clone(),
                expected_stamp: first.stamp,
            },
            snapshot(),
            &evidence,
            &token,
        )
        .unwrap();
    assert_eq!(second.metadata.revision.get(), 2);
    assert_eq!(
        fs::read_to_string(root.path().join(".fixture-requests")).unwrap(),
        "Read\nApply\n"
    );
    assert_eq!(evidence.pending().unwrap().intended_stamp(), second.stamp);
    let bytes = fs::read(
        root.path()
            .join("saves")
            .join(format!("{}.json.bak", first.metadata.id.as_str())),
    )
    .unwrap();
    assert_eq!(codec::stamp(&bytes), first.stamp);
    fs::write(root.path().join(".fixture-requests"), "").unwrap();
    let error = repo
        .replace(
            SaveTarget {
                id: first.metadata.id,
                expected_stamp: first.stamp,
            },
            snapshot(),
            &evidence,
            &token,
        )
        .unwrap_err();
    assert_eq!(error.kind, StorageFailureKind::Conflict);
    assert_eq!(error.visibility, WriteVisibility::Unchanged);
    assert_eq!(
        fs::read_to_string(root.path().join(".fixture-requests")).unwrap(),
        "Read\n"
    );
}
