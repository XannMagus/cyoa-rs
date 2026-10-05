//! Event-loop output regressions, with deterministic sinks rather than scheduler luck.
use cyoa_application::{
    cancellation::CancellationToken,
    generation::{Generated, GenerationFailure, StoryGenerator, StoryUseCases, TurnDirection},
};
use cyoa_core::{
    game::GameState,
    limits::Limits,
    style::StoryStyle,
    text::Brief,
    turn::StoryTurn,
    world::{WorldCast, WorldOutline},
};
use cyoa_infrastructure::generation::demo;
use cyoa_presentation::{
    headless::{self, Input, InputEvent},
    persistence::{HeadlessSession, PersistedSession},
    runtime::{Intent, SessionRuntime},
    session::{Phase, SessionController},
    storage::StorageRunner,
    worker::PreviewLimit,
};
use std::{
    collections::VecDeque,
    io::{self, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
struct Lines {
    lines: VecDeque<InputEvent>,
    fallback_interrupt: bool,
}
impl Input for Lines {
    fn poll(&mut self, timeout: Duration) -> io::Result<InputEvent> {
        if let Some(event) = self.lines.pop_front() {
            return Ok(event);
        }
        thread::sleep(timeout.min(Duration::from_millis(5)));
        Ok(if self.fallback_interrupt {
            InputEvent::Interrupt
        } else {
            InputEvent::Eof
        })
    }
}
fn runtime<G: StoryGenerator + Send + 'static>(
    generator: G,
) -> PersistedSession<
    G,
    cyoa_infrastructure::persistence::helper::SupervisedRepository,
    impl Fn() -> cyoa_infrastructure::persistence::helper::SupervisedRepository,
> {
    let runtime = SessionRuntime::new(
        SessionController::new(Limits::default(), StoryStyle::default()),
        StoryUseCases::new(generator),
        PreviewLimit::default(),
    );
    let root = tempfile::tempdir().unwrap();
    let config = cyoa_infrastructure::persistence::helper::HelperConfig::new(
        std::path::PathBuf::from(env!("CARGO_BIN_EXE_cyoa")),
        root.path().into(),
    )
    .unwrap();
    PersistedSession::new(
        runtime,
        StorageRunner::new(move || {
            let _keep_root_alive = &root;
            cyoa_infrastructure::persistence::helper::SupervisedRepository::new(config.clone())
        }),
        cyoa_application::persistence::StorySource::Demo {
            scenario: cyoa_application::persistence::DemoScenarioId::HarbourV1,
        },
    )
}
struct Paused {
    until: Instant,
    written: Vec<u8>,
}
impl Write for Paused {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if Instant::now() < self.until {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let n = bytes.len().min(1024);
        self.written.extend_from_slice(&bytes[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn paused_control_reader_resumes_and_final_drain_delivers_every_help_once() {
    let mut runtime = runtime(demo::harbour_v1().unwrap());
    let mut input = Lines {
        lines: std::iter::repeat_with(|| InputEvent::Line("/help".into()))
            .take(400)
            .chain([InputEvent::Line("/quit".into())])
            .collect(),
        fallback_interrupt: false,
    };
    let mut control = Paused {
        until: Instant::now() + Duration::from_millis(100),
        written: vec![],
    };
    let mut story = vec![];
    headless::run(&mut runtime, &mut input, &mut story, &mut control, true).unwrap();
    let text = String::from_utf8(control.written).unwrap();
    assert_eq!(text.matches("Commands: /help").count(), 400);
    assert!(text.ends_with("Session closed; durability=Clean.\n"));
    assert_eq!(runtime.controller().phase(), Phase::Closed);
}
struct Trickle {
    next: Instant,
}
impl Write for Trickle {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if Instant::now() < self.next {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        self.next = Instant::now() + Duration::from_millis(10);
        Ok(bytes.len().min(1))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn trickling_reader_cannot_extend_final_drain_indefinitely() {
    let mut runtime = runtime(demo::harbour_v1().unwrap());
    let mut input = Lines {
        lines: [InputEvent::Line("/quit".into())].into(),
        fallback_interrupt: false,
    };
    let mut control = Trickle {
        next: Instant::now(),
    };
    let start = Instant::now();
    let error =
        headless::run(&mut runtime, &mut input, &mut vec![], &mut control, true).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(error.to_string().contains("did not drain on exit"));
    // The TimedOut error already proves the drain deadline fired; hang guard only.
    assert!(start.elapsed() < Duration::from_secs(30));
    assert_eq!(runtime.controller().phase(), Phase::Closed);
}
struct Silent<G> {
    inner: G,
    started: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
}
impl<G: StoryGenerator> StoryGenerator for Silent<G> {
    fn outline(
        &mut self,
        brief: &Brief,
        cancel: &CancellationToken,
    ) -> Result<Generated<WorldOutline>, GenerationFailure> {
        let deadline = Instant::now() + Duration::from_secs(30);
        self.started.store(true, Ordering::SeqCst);
        while !cancel.is_cancelled() {
            assert!(Instant::now() < deadline, "cancellation watchdog");
            thread::sleep(Duration::from_millis(1));
        }
        self.cancelled.store(true, Ordering::SeqCst);
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
        self.inner.turn(game, direction, cancel, preview)
    }
}
#[test]
fn cancellation_and_quit_join_active_worker_while_control_output_is_blocked() {
    for cancel_first in [false, true] {
        let cancelled = Arc::new(AtomicBool::new(false));
        let started = Arc::new(AtomicBool::new(false));
        let mut runtime = runtime(Silent {
            inner: demo::harbour_v1().unwrap(),
            started: started.clone(),
            cancelled: cancelled.clone(),
        });
        runtime
            .dispatch(Intent::SubmitBrief(Brief::new("harbour").unwrap()))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while !started.load(Ordering::SeqCst) {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        let mut input = CancelInput {
            cancel_first,
            sent: false,
            quit: false,
            observed: cancelled.clone(),
            deadline: Instant::now() + Duration::from_millis(300),
        };
        let mut control = Paused {
            until: Instant::now() + Duration::from_secs(10),
            written: vec![],
        };
        let start = Instant::now();
        let error =
            headless::run(&mut runtime, &mut input, &mut vec![], &mut control, true).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert!(cancelled.load(Ordering::SeqCst));
        assert_eq!(runtime.controller().phase(), Phase::Closed);
        assert!(runtime.controller().game().is_none());
        assert!(
            // Half the 10 s stall: only waiting out the stalled sink crosses this.
            start.elapsed() < Duration::from_secs(5),
            "output stall delayed cancellation/join"
        );
    }
}

struct CancelInput {
    cancel_first: bool,
    sent: bool,
    quit: bool,
    observed: Arc<AtomicBool>,
    deadline: Instant,
}
impl Input for CancelInput {
    fn poll(&mut self, _: Duration) -> io::Result<InputEvent> {
        if !self.sent {
            self.sent = true;
            if self.cancel_first {
                return Ok(InputEvent::Interrupt);
            }
            self.quit = true;
            return Ok(InputEvent::Line("/quit".into()));
        }
        if !self.quit {
            // Quit itself cancels. Observe the earlier Interrupt's effect first.
            if self.observed.load(Ordering::SeqCst) {
                self.quit = true;
                return Ok(InputEvent::Line("/quit".into()));
            }
            if Instant::now() >= self.deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "interrupt did not cancel the blocked-output worker",
                ));
            }
            thread::sleep(Duration::from_millis(1));
            return Ok(InputEvent::Pending);
        }
        Ok(InputEvent::Interrupt)
    }
}

#[cfg(target_os = "linux")]
struct ObservedPipe {
    inner: cyoa_presentation::terminal::Flags<std::os::fd::OwnedFd>,
    notify: Option<std::sync::mpsc::Sender<()>>,
    blocked: bool,
}
#[cfg(target_os = "linux")]
impl Write for ObservedPipe {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let result = self.inner.write(bytes);
        if matches!(&result,Err(e) if e.kind()==io::ErrorKind::WouldBlock) {
            self.blocked = true;
            if let Some(notify) = self.notify.take() {
                notify.send(()).unwrap();
            }
        }
        result
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
#[cfg(target_os = "linux")]
fn full_pipe_with_delayed_reader() -> (ObservedPipe, thread::JoinHandle<Vec<u8>>, usize) {
    use rustix::{
        fs::{OFlags, fcntl_getfl, fcntl_setfl},
        pipe::{PipeFlags, pipe_with},
    };
    use std::io::Read;
    let (reader, writer) = pipe_with(PipeFlags::NONBLOCK).unwrap();
    let mut filled = 0;
    loop {
        match rustix::io::write(&writer, &[b'x'; 1024]) {
            Ok(n) => filled += n,
            Err(rustix::io::Errno::AGAIN) => break,
            result => panic!("fill pipe: {result:?}"),
        }
    }
    assert!(filled > 0);
    let flags = fcntl_getfl(&reader).unwrap();
    fcntl_setfl(&reader, flags & !OFlags::NONBLOCK).unwrap();
    let (notify, blocked) = std::sync::mpsc::channel();
    let reader = thread::spawn(move || {
        blocked
            .recv_timeout(Duration::from_secs(5))
            .expect("app never exercised full-pipe backpressure");
        thread::sleep(Duration::from_millis(100));
        let mut bytes = vec![];
        std::fs::File::from(reader).read_to_end(&mut bytes).unwrap();
        bytes
    });
    (
        ObservedPipe {
            inner: cyoa_presentation::terminal::Flags::new(writer).unwrap(),
            notify: Some(notify),
            blocked: false,
        },
        reader,
        filled,
    )
}
#[cfg(target_os = "linux")]
#[test]
fn initially_full_real_control_pipe_recovers_without_losing_help_or_close() {
    let (mut control, reader, filled) = full_pipe_with_delayed_reader();
    let mut runtime = runtime(demo::harbour_v1().unwrap());
    let mut input = Lines {
        lines: std::iter::repeat_with(|| InputEvent::Line("/help".into()))
            .take(400)
            .chain([InputEvent::Line("/quit".into())])
            .collect(),
        fallback_interrupt: false,
    };
    let result = headless::run(&mut runtime, &mut input, &mut vec![], &mut control, true);
    assert!(
        control.blocked,
        "control pipe must actually return WouldBlock"
    );
    drop(control);
    let bytes = reader.join().unwrap();
    result.unwrap();
    assert_eq!(&bytes[..filled], vec![b'x'; filled]);
    let text = std::str::from_utf8(&bytes[filled..]).unwrap();
    assert_eq!(text.matches("Commands: /help").count(), 400);
    assert!(text.ends_with("Session closed; durability=Clean.\n"));
}
#[cfg(target_os = "linux")]
#[test]
fn initially_full_real_story_pipe_drains_exact_committed_narrative_on_exit() {
    use cyoa_core::world::PlayablePosition;
    use std::{cell::RefCell, rc::Rc};
    let (mut story, reader, filled) = full_pipe_with_delayed_reader();
    let mut runtime = runtime(demo::harbour_v1().unwrap());
    fn settle(runtime: &mut dyn HeadlessSession) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while runtime.controller().phase() == Phase::Running {
            runtime.poll();
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
    }
    runtime
        .dispatch(Intent::SubmitBrief(Brief::new("harbour").unwrap()))
        .unwrap();
    settle(&mut runtime);
    runtime.dispatch(Intent::AcceptOutline).unwrap();
    settle(&mut runtime);
    runtime
        .dispatch(Intent::Select(PlayablePosition::new(0)))
        .unwrap();
    struct Capture(Rc<RefCell<Vec<u8>>>);
    impl Write for Capture {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    struct AfterCommit {
        bytes: Rc<RefCell<Vec<u8>>>,
        deadline: Instant,
    }
    impl Input for AfterCommit {
        fn poll(&mut self, _: Duration) -> io::Result<InputEvent> {
            if String::from_utf8_lossy(&self.bytes.borrow()).contains("[committed turn 1]") {
                return Ok(InputEvent::Line("/quit".into()));
            }
            assert!(Instant::now() < self.deadline, "opening watchdog");
            thread::sleep(Duration::from_millis(1));
            Ok(InputEvent::Pending)
        }
    }
    let bytes = Rc::new(RefCell::new(vec![]));
    let mut control = Capture(bytes.clone());
    let mut input = AfterCommit {
        bytes,
        deadline: Instant::now() + Duration::from_secs(2),
    };
    let result = headless::run(&mut runtime, &mut input, &mut story, &mut control, true);
    assert!(story.blocked, "story pipe must actually return WouldBlock");
    drop(story);
    let bytes = reader.join().unwrap();
    result.unwrap();
    let narrative = runtime.controller().game().unwrap().turns()[0]
        .turn()
        .narrative()
        .as_str();
    assert_eq!(&bytes[filled..], format!("{narrative}\n").as_bytes());
    assert_eq!(runtime.controller().phase(), Phase::Closed);
}

#[test]
fn reader_resuming_after_final_deadline_cannot_turn_expiry_into_success() {
    struct DelayedInput {
        quit: bool,
    }
    impl Input for DelayedInput {
        fn poll(&mut self, _: Duration) -> io::Result<InputEvent> {
            if !self.quit {
                self.quit = true;
                return Ok(InputEvent::Line("/quit".into()));
            }
            // Model descheduling during an input poll. The sink becomes writable
            // during this pause, after the absolute closing deadline has expired.
            thread::sleep(Duration::from_millis(1100));
            Ok(InputEvent::Pending)
        }
    }
    let mut runtime = runtime(demo::harbour_v1().unwrap());
    let mut input = DelayedInput { quit: false };
    let mut control = Paused {
        until: Instant::now() + Duration::from_millis(1050),
        written: vec![],
    };
    let error =
        headless::run(&mut runtime, &mut input, &mut vec![], &mut control, true).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(error.to_string().contains("did not drain on exit"));
    assert!(
        control.written.is_empty(),
        "expired drain must not pump newly writable output"
    );
    assert_eq!(runtime.controller().phase(), Phase::Closed);
}
