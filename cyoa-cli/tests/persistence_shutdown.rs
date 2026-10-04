#![cfg(target_os = "linux")]
use cyoa_application::{generation::StoryUseCases, persistence::*};
use cyoa_infrastructure::{
    generation::demo,
    persistence::{
        codec,
        helper::{HelperConfig, SupervisedRepository},
        repository::LocalRepository,
    },
};
use cyoa_presentation::{
    headless::{self, Input, InputEvent},
    persistence::{Durability, PersistedSession, Shutdown},
    runtime::SessionRuntime,
    session::SessionController,
    storage::StorageRunner,
    worker::PreviewLimit,
};
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
    time::{Duration, Instant},
};
fn snapshot() -> SaveSnapshot {
    codec::decode(
        include_bytes!("../../cyoa-infrastructure/tests/fixtures/saves/v1-full-story.json"),
        &SaveId::new("harbour-0123456789abcdef0123456789abcdef").unwrap(),
        SaveCopy::Primary,
    )
    .unwrap()
    .snapshot
}
struct ClosingInput {
    root: PathBuf,
    step: u8,
    ending: u8,
    pids: Vec<i32>,
}
impl Input for ClosingInput {
    fn poll(&mut self, _: Duration) -> io::Result<InputEvent> {
        if let Ok(pid) = fs::read_to_string(self.root.join(".fixture-pid"))
            && let Ok(pid) = pid.parse()
            && !self.pids.contains(&pid)
        {
            self.pids.push(pid);
        }
        match self.step {
            0 => {
                self.step = 1;
                Ok(InputEvent::Line("/save".into()))
            }
            1 if self.root.join(".fixture-ready").exists() => {
                self.step = 2;
                Ok(InputEvent::Line("/help".into()))
            }
            1 => {
                std::thread::sleep(Duration::from_millis(1));
                Ok(InputEvent::Pending)
            }
            2 => {
                self.step = 3;
                Ok(InputEvent::Line("/inspect".into()))
            }
            _ => match self.ending {
                0 => Ok(InputEvent::Line("/quit".into())),
                1 => Ok(InputEvent::Eof),
                2 => Ok(InputEvent::Interrupt),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "controlled input error",
                )),
            },
        }
    }
}
#[test]
fn bounded_silent_storage_keeps_input_responsive_and_reaps_current_and_final_helpers() {
    let executable = escargot::CargoBuild::new()
        .manifest_path(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml"))
        .package("cyoa-infrastructure")
        .bin("storage_fixture")
        .args(["--locked", "--offline"])
        .run()
        .unwrap()
        .path()
        .to_path_buf();
    for ending in 0..4 {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(".fixture-mode"), "silent").unwrap();
        let config = HelperConfig::new(executable.clone(), root.path().into())
            .unwrap()
            .with_bounds(
                cyoa_infrastructure::backends::process::ProcessBounds::new(
                    Duration::from_millis(250),
                    cyoa_infrastructure::backends::process::MaxStdoutBytes::new(160 * 1024 * 1024)
                        .unwrap(),
                    cyoa_infrastructure::backends::process::MaxStderrBytes::new(65536).unwrap(),
                )
                .unwrap(),
            );
        let initial = snapshot();
        let runtime = SessionRuntime::new(
            SessionController::from_game(initial.game.clone()),
            StoryUseCases::new(demo::harbour_v1().unwrap()),
            PreviewLimit::default(),
        );
        let mut session = PersistedSession::new(
            runtime,
            StorageRunner::new(move |evidence| SupervisedRepository::new(config.clone(), evidence)),
            initial.source,
        );
        let mut input = ClosingInput {
            root: root.path().into(),
            step: 0,
            ending,
            pids: vec![],
        };
        let mut control = vec![];
        let begin = Instant::now();
        let error =
            headless::run(&mut session, &mut input, &mut vec![], &mut control, false).unwrap_err();
        assert!(begin.elapsed() < Duration::from_secs(3));
        assert_eq!(
            session.shutdown(),
            if ending == 3 {
                Shutdown::DrainingOutput
            } else {
                Shutdown::Closed
            }
        );
        assert_eq!(session.durability(), Durability::Uncertain);
        assert_eq!(session.controller().game(), Some(&initial.game));
        if ending == 3 {
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        } else {
            assert!(error.to_string().contains("unsaved canonical revision"));
        }
        let text = String::from_utf8(control).unwrap();
        assert!(text.contains("Commands: /help"));
        assert!(text.contains("Inspection:"));
        let requests = fs::read_to_string(root.path().join(".fixture-requests")).unwrap();
        assert_eq!(
            requests.lines().count(),
            2,
            "one current operation and one final reconciliation only"
        );
        if let Ok(pid) = fs::read_to_string(root.path().join(".fixture-pid")) {
            input.pids.push(pid.parse().unwrap());
        }
        assert!(input.pids.len() >= 2);
        for pid in input.pids {
            assert_eq!(
                rustix::process::test_kill_process(rustix::process::Pid::from_raw(pid).unwrap()),
                Err(rustix::io::Errno::SRCH),
                "helper was not reaped"
            );
        }
        assert!(!root.path().join("saves").exists());
        assert!(session.storage_failure().unwrap().pending.is_some());
    }
}
struct ErrorInput;
impl Input for ErrorInput {
    fn poll(&mut self, _: Duration) -> io::Result<InputEvent> {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "controlled input error",
        ))
    }
}
struct Broken {
    writes: usize,
}
impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        self.writes += 1;
        assert_eq!(
            self.writes, 1,
            "a failed output sink must not be retried during storage cleanup"
        );
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn input_and_output_errors_still_save_last_canonical_story_and_remain_errors() {
    for broken_output in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let initial = snapshot();
        let config = HelperConfig::new(
            PathBuf::from(env!("CARGO_BIN_EXE_cyoa")),
            root.path().into(),
        )
        .unwrap();
        let runtime = SessionRuntime::new(
            SessionController::from_game(initial.game.clone()),
            StoryUseCases::new(demo::harbour_v1().unwrap()),
            PreviewLimit::default(),
        );
        let mut session = PersistedSession::new(
            runtime,
            StorageRunner::new(move |evidence| SupervisedRepository::new(config.clone(), evidence)),
            initial.source,
        );
        let mut control = vec![];
        let mut broken = Broken { writes: 0 };
        let sink: &mut dyn Write = if broken_output {
            &mut broken
        } else {
            &mut control
        };
        let error =
            headless::run(&mut session, &mut ErrorInput, &mut vec![], sink, false).unwrap_err();
        assert_eq!(
            error.kind(),
            if broken_output {
                io::ErrorKind::BrokenPipe
            } else {
                io::ErrorKind::InvalidData
            }
        );
        assert_eq!(session.shutdown(), Shutdown::DrainingOutput);
        assert_eq!(session.durability(), Durability::Clean);
        let mut repo = LocalRepository::new(root.path().into()).unwrap();
        let token = cyoa_application::cancellation::CancellationSource::default().token();
        let page = repo
            .list(SavePage::new(None, 100).unwrap(), &token)
            .unwrap();
        assert_eq!(page.entries.len(), 1);
        let stored = repo
            .load(&page.entries[0].id, SaveCopy::Primary, &token)
            .unwrap();
        assert_eq!(stored.snapshot, initial);
    }
}
