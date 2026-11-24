//! A frontend-style client: no presentation dependency, parser, stdin or renderer.
#![cfg(target_os = "linux")]
use cyoa_application::{
    cancellation::CancellationToken,
    diagnostics::TransportDiagnostics,
    execution::PreviewLimit,
    generation::{FailureKind, Generated, GenerationFailure, StoryUseCases, TurnDirection},
    persistence::*,
    runtime::{CanonicalChange, Intent, SessionRuntime},
    session::{Phase, SessionController, Stage},
    session_persistence::{
        Durability, PersistedSession, PersistenceEvent, SaveBinding, SaveSlot, Shutdown,
    },
};
use cyoa_core::{
    game::{GameState, TurnCount},
    limits::{Limits, RestoreLimits},
    style::StoryStyle,
    text::{Brief, RawResponse, WorldDescription, WorldTitle},
    turn::StoryTurn,
    world::{PlayablePosition, WorldCast, WorldOutline},
};
use cyoa_infrastructure::{
    execution::{generation::WorkerRunner, storage::StorageRunner},
    generation::demo::{self, HarbourDemo},
    persistence::helper::{HelperConfig, SupervisedRepository},
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

struct Generator {
    inner: HarbourDemo,
    fail: Arc<AtomicBool>,
    block: Arc<AtomicBool>,
    started: mpsc::Sender<()>,
}
impl cyoa_core::generation::StoryGenerator for Generator {
    type Context = CancellationToken;
    type Failure = GenerationFailure;
    type Report<T> = Generated<T>;
    fn outline(
        &mut self,
        brief: &Brief,
        cancel: &CancellationToken,
    ) -> Result<Generated<WorldOutline>, GenerationFailure> {
        self.inner.outline(brief, cancel)
    }
    fn cast(
        &mut self,
        brief: &Brief,
        outline: &WorldOutline,
        limits: &Limits,
        cancel: &CancellationToken,
    ) -> Result<Generated<WorldCast>, GenerationFailure> {
        self.inner.cast(brief, outline, limits, cancel)
    }
    fn turn(
        &mut self,
        game: &GameState,
        direction: &TurnDirection,
        cancel: &CancellationToken,
        preview: &mut dyn FnMut(&str),
    ) -> Result<Generated<StoryTurn>, GenerationFailure> {
        if self.fail.swap(false, Ordering::SeqCst) {
            return Err(GenerationFailure::new(
                FailureKind::InvalidResponse,
                "controlled failure",
                Some(RawResponse::new("candidate\r\né")),
                TransportDiagnostics::new(vec![255], vec![13, 10]),
            ));
        }
        if self.block.swap(false, Ordering::SeqCst) {
            let (wake, wait) = mpsc::channel();
            let _registration = cancel.subscribe(move || {
                let _ = wake.send(());
            });
            self.started.send(()).unwrap();
            wait.recv_timeout(Duration::from_secs(30))
                .expect("cancel must wake the worker");
            return Err(GenerationFailure::new(
                FailureKind::Cancelled,
                "controlled cancellation",
                None,
                TransportDiagnostics::empty(),
            ));
        }
        self.inner
            .turn(game, direction, cancel, preview)
            .map(|report| {
                report.with_diagnostics(TransportDiagnostics::new(vec![0, 255], vec![254, 13, 10]))
            })
    }
}

type Factory = Box<dyn Fn() -> SupervisedRepository + Send + Sync>;
type Client =
    PersistedSession<WorkerRunner<Generator>, StorageRunner<SupervisedRepository, Factory>>;
fn settle(client: &mut Client) -> Vec<PersistenceEvent> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut events = vec![];
    loop {
        events.extend(client.poll());
        if !client.storage_busy()
            && matches!(
                client.view().phase(),
                Phase::Ready | Phase::Failed | Phase::Closed
            )
        {
            return events;
        }
        assert!(
            Instant::now() < deadline,
            "library operation failed to finish"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn id(client: &Client) -> SaveId {
    let SaveBinding::Bound { id, .. } = client.binding() else {
        panic!("save must bind after success")
    };
    id.clone()
}

#[test]
fn library_client_owns_full_session_without_presentation_and_uses_explicit_storage_helper() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join(".fixture-mode"), "normal").unwrap();
    // This is NOT current_exe(): the client isn't a storage-helper executable.
    let config = HelperConfig::new(
        PathBuf::from(env!("CARGO_BIN_EXE_storage_fixture")),
        root.path().into(),
    )
    .unwrap();
    let fail = Arc::new(AtomicBool::new(false));
    let block = Arc::new(AtomicBool::new(false));
    let (started, waiting) = mpsc::channel();
    let generator = Generator {
        inner: demo::harbour_v1().unwrap(),
        fail: fail.clone(),
        block: block.clone(),
        started,
    };
    let runtime = SessionRuntime::new(
        SessionController::new(Limits::default(), StoryStyle::default()),
        WorkerRunner::new(StoryUseCases::new(generator), PreviewLimit::default()),
    );
    let helper = config.clone();
    let factory: Factory = Box::new(move || SupervisedRepository::new(helper.clone()));
    let mut client = PersistedSession::new(
        runtime,
        StorageRunner::new(factory),
        StorySource::Demo {
            scenario: DemoScenarioId::HarbourV1,
        },
    );
    client
        .dispatch(Intent::SubmitBrief(
            Brief::new("A harbour adventure").unwrap(),
        ))
        .unwrap();
    settle(&mut client);
    assert!(matches!(client.view().stage(), Stage::OutlineReview { .. }));
    client
        .dispatch(Intent::ReplaceOutline(WorldOutline::new(
            WorldTitle::new("Edited harbour").unwrap(),
            WorldDescription::new("Edited coast").unwrap(),
        )))
        .unwrap();
    client.dispatch(Intent::AcceptOutline).unwrap();
    settle(&mut client);
    assert!(
        client
            .dispatch(Intent::Select(PlayablePosition::new(999)))
            .is_err()
    );
    client
        .dispatch(Intent::Select(PlayablePosition::new(1)))
        .unwrap();
    assert!(client.view().game().unwrap().turns().is_empty());
    let events = settle(&mut client);
    let saved = events
        .iter()
        .position(|e| matches!(e, PersistenceEvent::Saved(_)))
        .unwrap();
    let opening = events
        .iter()
        .position(|e| {
            matches!(
                e,
                PersistenceEvent::Generation {
                    change: Some(CanonicalChange::Turn),
                    ..
                }
            )
        })
        .unwrap();
    assert!(
        saved < opening,
        "application must save selection before the opening"
    );
    assert_eq!(client.view().game().unwrap().turns().len(), 1);
    assert_eq!(
        client
            .view()
            .game()
            .unwrap()
            .world()
            .outline()
            .title()
            .as_str(),
        "Edited harbour"
    );
    let original = id(&client);
    client.dispatch(Intent::Action(1)).unwrap();
    settle(&mut client);
    let evidence = client
        .view()
        .evidence()
        .expect("successful execution evidence");
    assert_eq!(evidence.diagnostics.stdout(), &[0, 255]);
    assert_eq!(evidence.diagnostics.stderr(), &[254, 13, 10]);
    assert!(evidence.raw_response.is_some());
    assert!(evidence.provenance.is_some());
    let before = client.view().game().unwrap().clone();
    fail.store(true, Ordering::SeqCst);
    client
        .dispatch(Intent::Turn(TurnDirection::Continue))
        .unwrap();
    settle(&mut client);
    assert_eq!(client.view().game(), Some(&before));
    let Some(cyoa_application::session::Failure::Generation(failure)) = client.view().failure()
    else {
        panic!("failure missing")
    };
    assert_eq!(failure.raw_response().unwrap().as_str(), "candidate\r\né");
    assert_eq!(failure.diagnostics().stdout(), &[255]);
    client.dispatch(Intent::Retry).unwrap();
    settle(&mut client);
    assert_eq!(client.view().game().unwrap().turns().len(), 3);
    let before = client.view().game().unwrap().clone();
    block.store(true, Ordering::SeqCst);
    client
        .dispatch(Intent::Turn(TurnDirection::Continue))
        .unwrap();
    waiting.recv_timeout(Duration::from_secs(30)).unwrap();
    client.dispatch(Intent::Cancel).unwrap();
    settle(&mut client);
    assert_eq!(client.view().game(), Some(&before));
    client.dispatch(Intent::Retry).unwrap();
    settle(&mut client);
    assert_eq!(client.view().game().unwrap().turns().len(), 4);
    client
        .dispatch(Intent::Rewind(TurnCount::new(1).unwrap()))
        .unwrap();
    settle(&mut client);
    assert_eq!(client.view().game(), Some(&before));
    client.save(SaveSlot::NewCopy).unwrap();
    settle(&mut client);
    assert_ne!(id(&client), original);
    client
        .dispatch(Intent::Turn(TurnDirection::Continue))
        .unwrap();
    settle(&mut client);
    client
        .load(LoadGame {
            id: original.clone(),
            copy: SaveCopy::Primary,
            limits: RestoreLimits::Original,
        })
        .unwrap();
    settle(&mut client);
    assert_eq!(id(&client), original);
    assert_eq!(client.view().game(), Some(&before));
    client.quit();
    let deadline = Instant::now() + Duration::from_secs(30);
    while client.shutdown() != Shutdown::Complete {
        client.poll();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(client.durability(), Durability::Clean);
    assert!(!client.exit_failed());
    let token = cyoa_application::cancellation::CancellationSource::default().token();
    let mut queries = PersistenceUseCases::new(SupervisedRepository::new(config));
    let listed = queries
        .list_saves(
            ListSaves {
                page: SavePage::new(None, PageSize::new(100).unwrap()),
            },
            &token,
        )
        .unwrap();
    assert_eq!(listed.entries.len(), 2);
    let saved = queries
        .inspect_save(
            InspectSave {
                id: original,
                copy: SaveCopy::Primary,
            },
            &token,
        )
        .unwrap();
    assert_eq!(saved.snapshot.game(), &before);
}
