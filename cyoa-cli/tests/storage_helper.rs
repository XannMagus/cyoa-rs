#![cfg(target_os = "linux")]
use cyoa_application::{cancellation::CancellationSource, persistence::*};
use cyoa_infrastructure::persistence::{
    codec,
    helper::{HelperConfig, SupervisedRepository},
};
use std::{fs, path::PathBuf};
fn snapshot() -> SaveSnapshot {
    let id = SaveId::new("harbour-0123456789abcdef0123456789abcdef").unwrap();
    codec::decode(
        include_bytes!("../../cyoa-infrastructure/tests/fixtures/saves/v1-full-story.json"),
        &id,
        SaveCopy::Primary,
    )
    .unwrap()
    .snapshot
}
#[test]
fn internal_helper_runs_storage_before_clap_or_auth_and_preserves_exact_documents() {
    let cwd = std::env::current_dir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let evidence = PreparedWriteEvidence::default();
    let config = HelperConfig::new(
        PathBuf::from(env!("CARGO_BIN_EXE_cyoa")),
        root.path().join("data"),
    )
    .unwrap();
    let mut repo = SupervisedRepository::new(config.clone());
    let token = CancellationSource::default().token();
    let receipt = repo.create(snapshot(), &evidence, &token).unwrap();
    assert_eq!(evidence.pending().unwrap().intended_stamp(), receipt.stamp);
    let bytes = fs::read(
        root.path()
            .join("data/saves")
            .join(format!("{}.json", receipt.metadata.id.as_str())),
    )
    .unwrap();
    assert_eq!(codec::stamp(&bytes), receipt.stamp);
    let mut reader = SupervisedRepository::new(config.clone());
    let loaded = reader
        .load(&receipt.metadata.id, SaveCopy::Primary, &token)
        .unwrap();
    assert_eq!(loaded.snapshot, snapshot());
    assert_eq!(loaded.stamp, receipt.stamp);
    let pending = evidence.pending().unwrap();
    assert_eq!(
        reader
            .reconcile(pending, &PreparedWriteEvidence::default(), &token)
            .unwrap(),
        receipt
    );
    let mut writer = SupervisedRepository::new(config.clone());
    let second = writer
        .replace(
            SaveTarget {
                id: receipt.metadata.id.clone(),
                expected_stamp: receipt.stamp,
            },
            snapshot(),
            &PreparedWriteEvidence::default(),
            &token,
        )
        .unwrap();
    assert_eq!(second.metadata.revision.get(), 2);
    assert_eq!(
        reader
            .load(&receipt.metadata.id, SaveCopy::Backup, &token)
            .unwrap()
            .stamp,
        receipt.stamp
    );
    assert_eq!(
        reader
            .replace(
                SaveTarget {
                    id: receipt.metadata.id,
                    expected_stamp: receipt.stamp
                },
                snapshot(),
                &PreparedWriteEvidence::default(),
                &token
            )
            .unwrap_err()
            .kind,
        StorageFailureKind::Conflict
    );
    assert_eq!(
        reader
            .list(SavePage::new(None, 100).unwrap(), &token)
            .unwrap()
            .entries
            .len(),
        1
    );
    assert_eq!(std::env::current_dir().unwrap(), cwd);
}
#[test]
fn unavailable_or_precancelled_helpers_cannot_report_durable_success() {
    let root = tempfile::tempdir().unwrap();
    let evidence = PreparedWriteEvidence::default();
    let mut repo = SupervisedRepository::new(
        HelperConfig::new(root.path().join("missing"), root.path().into()).unwrap(),
    );
    let source = CancellationSource::default();
    source.cancel();
    assert_eq!(
        repo.create(snapshot(), &evidence, &source.token())
            .unwrap_err()
            .kind,
        StorageFailureKind::Cancelled
    );
    assert!(evidence.pending().is_none());
    let error = repo
        .create(
            snapshot(),
            &evidence,
            &CancellationSource::default().token(),
        )
        .unwrap_err();
    assert_eq!(error.visibility, WriteVisibility::Unchanged);
    assert!(error.pending.is_some());
    assert!(evidence.pending().is_some());
    assert!(!root.path().join("saves").exists());
}

#[test]
fn presentation_storage_runner_drives_shipped_helpers_off_the_event_loop() {
    use cyoa_core::{limits::Limits, style::StoryStyle};
    use cyoa_presentation::{session::SessionController, storage::*};
    use std::time::{Duration, Instant};
    let root = tempfile::tempdir().unwrap();
    let config = HelperConfig::new(
        PathBuf::from(env!("CARGO_BIN_EXE_cyoa")),
        root.path().into(),
    )
    .unwrap();
    let mut runner = StorageRunner::new(move || SupervisedRepository::new(config.clone()));
    let key = StorageKey {
        id: StorageRequestId::new(1).unwrap(),
        revision: SessionController::new(Limits::default(), StoryStyle::default()).revision(),
    };
    runner
        .start(
            key,
            StorageIntent::Save(Box::new(SaveGame::Create(snapshot()))),
        )
        .unwrap();
    let mut prepared = None;
    let end = Instant::now() + Duration::from_secs(5);
    let receipt = loop {
        let begin = Instant::now();
        let events = runner.poll();
        assert!(begin.elapsed() < Duration::from_millis(100));
        let mut done = None;
        for event in events {
            match event {
                StorageEvent::Prepared { key: k, pending } => {
                    assert_eq!(key, k);
                    prepared = Some(pending);
                }
                StorageEvent::Finished {
                    key: k,
                    result: Ok(StorageOutcome::Saved(receipt)),
                } => {
                    assert_eq!(key, k);
                    done = Some(receipt)
                }
                _ => panic!("unexpected storage event"),
            }
        }
        if let Some(receipt) = done {
            break receipt;
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(prepared.unwrap().intended_stamp(), receipt.stamp);
    assert!(runner.is_ready());
    let next = StorageKey {
        id: key.id.next().unwrap(),
        ..key
    };
    runner
        .start(
            next,
            StorageIntent::Load(LoadGame {
                id: receipt.metadata.id,
                copy: SaveCopy::Primary,
                limits: cyoa_core::limits::RestoreLimits::Original,
            }),
        )
        .unwrap();
    loop {
        if let Some(StorageEvent::Finished {
            key: k,
            result: Ok(StorageOutcome::Loaded(loaded)),
        }) = runner.poll().pop()
        {
            assert_eq!(k, next);
            assert_eq!(loaded.stored.snapshot, snapshot());
            break;
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(2));
    }
    runner.close();
    assert!(runner.is_closed());
}

#[test]
fn closing_or_dropping_storage_runner_reaps_a_real_silent_helper() {
    use cyoa_core::{limits::Limits, style::StoryStyle};
    use cyoa_presentation::{session::SessionController, storage::*};
    use std::time::{Duration, Instant};
    let executable = escargot::CargoBuild::new()
        .manifest_path(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml"))
        .package("cyoa-infrastructure")
        .bin("storage_fixture")
        .args(["--locked", "--offline"])
        .run()
        .unwrap()
        .path()
        .to_path_buf();
    for drop_runner in [false, true] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(".fixture-mode"), "silent").unwrap();
        let config = HelperConfig::new(executable.clone(), root.path().into()).unwrap();
        let mut runner = StorageRunner::new(move || SupervisedRepository::new(config.clone()));
        let key = StorageKey {
            id: StorageRequestId::new(1).unwrap(),
            revision: SessionController::new(Limits::default(), StoryStyle::default()).revision(),
        };
        runner
            .start(
                key,
                StorageIntent::Save(Box::new(SaveGame::Create(snapshot()))),
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !root.path().join(".fixture-ready").exists() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        let pid = rustix::process::Pid::from_raw(
            fs::read_to_string(root.path().join(".fixture-pid"))
                .unwrap()
                .parse()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(rustix::process::test_kill_process(pid), Ok(()));
        let begin = Instant::now();
        let events = runner.poll();
        assert!(matches!(&events[..],[StorageEvent::Prepared{key:k,..}]if *k==key));
        assert!(begin.elapsed() < Duration::from_millis(100));
        let begin = Instant::now();
        if drop_runner {
            drop(runner);
        } else {
            runner.close();
            runner.close();
            loop {
                if let Some(StorageEvent::Finished {
                    result: Err(error), ..
                }) = runner.poll().pop()
                {
                    assert_eq!(error.kind, StorageFailureKind::Cancelled);
                    assert_eq!(error.visibility, WriteVisibility::Unknown);
                    assert!(error.pending.is_some());
                    break;
                }
                assert!(Instant::now() < deadline);
                runner.cancel();
                std::thread::sleep(Duration::from_millis(2));
            }
            assert!(runner.is_closed());
        }
        assert!(begin.elapsed() < Duration::from_secs(2));
        assert_eq!(
            rustix::process::test_kill_process(pid),
            Err(rustix::io::Errno::SRCH)
        );
        assert!(!root.path().join("saves").exists());
    }
}

#[test]
fn coordinator_rewind_load_and_quit_persist_through_shipped_helpers() {
    use cyoa_application::generation::StoryUseCases;
    use cyoa_core::{game::TurnCount, limits::RestoreLimits};
    use cyoa_presentation::{
        persistence::*,
        runtime::{Intent, SessionRuntime},
        session::SessionController,
        storage::StorageRunner,
        worker::PreviewLimit,
    };
    use std::time::{Duration, Instant};
    let root = tempfile::tempdir().unwrap();
    let config = HelperConfig::new(
        PathBuf::from(env!("CARGO_BIN_EXE_cyoa")),
        root.path().into(),
    )
    .unwrap();
    let initial = snapshot();
    let runtime = SessionRuntime::new(
        SessionController::from_game(initial.game().clone()),
        StoryUseCases::new(cyoa_infrastructure::generation::demo::harbour_v1().unwrap()),
        PreviewLimit::default(),
    );
    let mut s = PersistedSession::new(
        runtime,
        StorageRunner::new(move || SupervisedRepository::new(config.clone())),
        initial.source(),
    );
    macro_rules! settle {
        () => {{
            let deadline = Instant::now() + Duration::from_secs(5);
            while s.storage_busy()
                || s.shutdown() == Shutdown::StoppingGeneration
                || s.shutdown() == Shutdown::SavingFinal
            {
                s.poll();
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(2));
            }
        }};
    }
    s.save(false).unwrap();
    settle!();
    assert_eq!(s.durability(), Durability::Clean);
    let SaveBinding::Bound { id, .. } = s.binding().clone() else {
        panic!("missing binding")
    };
    let path = root
        .path()
        .join("saves")
        .join(format!("{}.json", id.as_str()));
    let first = fs::read(&path).unwrap();
    assert_eq!(
        codec::decode(&first, &id, SaveCopy::Primary)
            .unwrap()
            .snapshot,
        initial
    );
    s.dispatch(Intent::Rewind(TurnCount::new(1).unwrap()))
        .unwrap();
    settle!();
    let rewound = s.controller().game().unwrap().clone();
    assert_eq!(rewound.turns().len(), initial.game().turns().len() - 1);
    assert_eq!(
        codec::decode(&fs::read(&path).unwrap(), &id, SaveCopy::Primary)
            .unwrap()
            .snapshot
            .into_game(),
        rewound
    );
    assert_eq!(fs::read(path.with_extension("json.bak")).unwrap(), first);
    s.load(LoadGame {
        id: id.clone(),
        copy: SaveCopy::Primary,
        limits: RestoreLimits::Original,
    })
    .unwrap();
    settle!();
    assert_eq!(s.controller().game(), Some(&rewound));
    s.quit();
    s.quit();
    s.poll();
    settle!();
    assert_eq!(s.shutdown(), Shutdown::DrainingOutput);
    assert!(!s.exit_failed());
    assert_eq!(
        codec::decode(&fs::read(path).unwrap(), &id, SaveCopy::Primary)
            .unwrap()
            .snapshot
            .into_game(),
        rewound
    );
}
