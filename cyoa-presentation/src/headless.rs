//! Append-only terminal view over the canonical session controller.
use crate::{
    commands::{PersistenceCommand, persistence_command},
    persistence::{HeadlessSession, PersistenceEvent, SaveBinding, Shutdown},
    runtime::Intent,
    session::{Acceptance, Failure, Phase, Stage},
};
use cyoa_application::{generation::TurnDirection, persistence::*};
use cyoa_core::{
    text::{Brief, PlayerInput, WorldDescription, WorldTitle},
    world::{PlayablePosition, WorldOutline},
};
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};
mod output;
use output::QueuedOutput;

pub enum InputEvent {
    Pending,
    Line(String),
    Eof,
    Interrupt,
}
pub trait Input {
    fn poll(&mut self, timeout: Duration) -> io::Result<InputEvent>;
}

enum Edit {
    None,
    Title,
    Description(WorldTitle),
}
struct View {
    printed: String,
    edit: Edit,
}
impl Default for View {
    fn default() -> Self {
        Self {
            printed: String::new(),
            edit: Edit::None,
        }
    }
}
const HELP: &str = "Commands: /help /quit /cancel /retry /inspect /diagnostics /save /save-copy /list [AFTER_ID]\n/load SAVE_ID --limits current|original [--backup]; /rewind N\nOutline: empty line accepts; /edit replaces title and description.\nPlay: empty line continues; /action N selects a one-based action; /event requests an interesting event. Ordinary numbers are player text. // escapes an initial slash.\nCtrl-C cancels generation; idle Ctrl-C and EOF exit. Selection, accepted turns and rewind autosave. During storage, help, inspection, diagnostics and quit remain available.\n";

/// Always close and join workers, including on input/output errors.
/// Sinks must be nonblocking and unbuffered (the Linux terminal guards provide
/// this). Order is preserved within each stream, not between stdout and stderr.
pub fn run(
    runtime: &mut dyn HeadlessSession,
    input: &mut dyn Input,
    story: &mut dyn Write,
    control: &mut dyn Write,
    demo: bool,
) -> io::Result<()> {
    let mut story = QueuedOutput::new(story);
    let mut control = QueuedOutput::new(control);
    let result = drive(runtime, input, &mut story, &mut control, demo);
    let _ = runtime.dispatch(Intent::Quit);
    // A terminal fault cannot bypass the obligatory canonical save. Keep polling
    // input and the healthy output sink while both owned workers finish.
    let mut story_ok = !story.is_faulted();
    let mut control_ok = !control.is_faulted();
    while !matches!(
        runtime.shutdown(),
        Shutdown::DrainingOutput | Shutdown::Closed
    ) {
        if let Ok(InputEvent::Interrupt | InputEvent::Eof) = input.poll(Duration::from_millis(25)) {
            let _ = runtime.dispatch(Intent::Quit);
        }
        for event in runtime.poll() {
            if control_ok && let Err(_) = storage_event(runtime, event, &mut control) {
                control_ok = false;
            }
        }
        if story_ok && story.pump(Instant::now()).is_err() {
            story_ok = false;
        }
        if control_ok && control.pump(Instant::now()).is_err() {
            control_ok = false;
        }
    }
    if result.is_ok() && runtime.exit_failed() {
        return Err(io::Error::other(
            "session closed with a worker failure or unsaved canonical revision; in-memory state may be lost",
        ));
    }
    result
}
fn drive(
    runtime: &mut dyn HeadlessSession,
    input: &mut dyn Input,
    story: &mut QueuedOutput<'_>,
    control: &mut QueuedOutput<'_>,
    demo: bool,
) -> io::Result<()> {
    announce(runtime, control, demo)?;
    let mut view = View::default();
    loop {
        story.pump(Instant::now())?;
        control.pump(Instant::now())?;
        // Input first: a cancellation observed before acceptance wins the race.
        let event = input.poll(Duration::from_millis(25))?;
        handle_input(runtime, &mut view, event, control)?;
        for event in runtime.poll() {
            handle_event(runtime, &mut view, event, story, control)?;
        }
        show_preview(runtime, &mut view, story, control)?;
        if runtime.shutdown() == Shutdown::DrainingOutput {
            return drain_on_exit(runtime, &view, input, story, control);
        }
        control.flush()?;
        story.pump(Instant::now())?;
        control.pump(Instant::now())?;
        if runtime.controller().phase() == Phase::Closing {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
fn announce(
    runtime: &mut dyn HeadlessSession,
    control: &mut QueuedOutput<'_>,
    demo: bool,
) -> io::Result<()> {
    writeln!(
        control,
        "Autosave enabled: selection, accepted turns, rewind and quit save canonical state."
    )?;
    if demo {
        writeln!(
            control,
            "Demo harbour-v1: five prerecorded turns; your choices do not alter the recorded fiction. Exhaustion never switches to a live backend."
        )?;
    }
    if runtime.controller().game().is_some() {
        writeln!(
            control,
            "Resumed game; loading makes no inference call. Empty line continues."
        )?;
        inspect(runtime, control)?;
    } else {
        writeln!(
            control,
            "Brief: enter a nonblank adventure idea. /help lists commands."
        )?;
    }
    control.flush()
}
fn handle_input(
    runtime: &mut dyn HeadlessSession,
    view: &mut View,
    event: InputEvent,
    control: &mut QueuedOutput<'_>,
) -> io::Result<()> {
    match event {
        InputEvent::Pending => (),
        InputEvent::Eof => {
            runtime.dispatch(Intent::Quit).map_err(io::Error::other)?;
        }
        InputEvent::Interrupt => {
            if matches!(
                runtime.controller().phase(),
                Phase::Running | Phase::Cancelling
            ) {
                runtime.dispatch(Intent::Cancel).map_err(io::Error::other)?;
                writeln!(control, "Cancellation requested; waiting for cleanup.")?;
            } else {
                runtime.dispatch(Intent::Quit).map_err(io::Error::other)?;
            }
        }
        InputEvent::Line(line) => view.line(runtime, &line, control)?,
    }
    Ok(())
}
fn handle_event(
    runtime: &mut dyn HeadlessSession,
    view: &mut View,
    event: PersistenceEvent,
    story: &mut QueuedOutput<'_>,
    control: &mut QueuedOutput<'_>,
) -> io::Result<()> {
    let PersistenceEvent::Generation { acceptance, .. } = event else {
        if matches!(&event, PersistenceEvent::Loaded { .. }) {
            view.edit = Edit::None;
            view.printed.clear();
        }
        return storage_event(runtime, event, control);
    };
    match acceptance {
        Acceptance::Committed => view.committed(runtime, story, control)?,
        Acceptance::Failed => {
            if !view.printed.is_empty() {
                writeln!(story)?;
                writeln!(control, "[tentative preview discarded; no turn committed]")?;
            }
            view.printed.clear();
            show_failure(runtime, control)?;
            if runtime.controller().phase() == Phase::Faulted {
                writeln!(control, "The worker is unavailable; use /quit.")?;
            } else {
                writeln!(
                    control,
                    "Use /retry to repeat the failed intent, or /quit. /diagnostics shows retained evidence."
                )?;
            }
        }
        Acceptance::Closed | Acceptance::Ignored => (),
    }
    Ok(())
}
fn show_preview(
    runtime: &mut dyn HeadlessSession,
    view: &mut View,
    story: &mut QueuedOutput<'_>,
    control: &mut QueuedOutput<'_>,
) -> io::Result<()> {
    if runtime.controller().phase() != Phase::Running {
        return Ok(());
    }
    let preview = runtime.controller().preview();
    if preview.len() > view.printed.len() {
        if view.printed.is_empty() {
            writeln!(control, "[tentative preview; final validation pending]")?;
            control.flush()?;
        }
        story.write_all(&preview.as_bytes()[view.printed.len()..])?;
        story.flush()?;
        // The preview only grows while running: append the new tail, never recopy.
        view.printed.push_str(&preview[view.printed.len()..]);
    }
    Ok(())
}
fn drain_on_exit(
    runtime: &mut dyn HeadlessSession,
    view: &View,
    input: &mut dyn Input,
    story: &mut QueuedOutput<'_>,
    control: &mut QueuedOutput<'_>,
) -> io::Result<()> {
    if !view.printed.is_empty() {
        writeln!(story)?;
        writeln!(control, "[tentative preview discarded on exit]")?;
    }
    writeln!(
        control,
        "Session closed; durability={:?}.",
        runtime.durability()
    )?;
    control.flush()?;
    // The worker is already joined. Drain queued bytes before reporting
    // success, with an absolute bound even if a reader keeps trickling.
    let deadline = Instant::now() + output::STALL_LIMIT;
    let check_deadline = || {
        if Instant::now() >= deadline {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "terminal output did not drain on exit",
            ))
        } else {
            Ok(())
        }
    };
    loop {
        check_deadline()?;
        story.pump(Instant::now())?;
        control.pump(Instant::now())?;
        check_deadline()?;
        if story.is_empty() && control.is_empty() {
            runtime.output_drained();
            return Ok(());
        }
        if matches!(
            input.poll(Duration::from_millis(25))?,
            InputEvent::Interrupt
        ) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "terminal output drain interrupted",
            ));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
impl View {
    fn dispatch(
        &mut self,
        runtime: &mut dyn HeadlessSession,
        intent: Intent,
        control: &mut dyn Write,
    ) -> io::Result<bool> {
        match runtime.dispatch(intent) {
            Ok(()) => {
                if runtime.controller().phase() == Phase::Running {
                    self.printed.clear();
                    writeln!(
                        control,
                        "Generating; /cancel or Ctrl-C cancels, /quit exits."
                    )?;
                }
                Ok(true)
            }
            Err(e) => {
                writeln!(control, "Rejected: {e}")?;
                Ok(false)
            }
        }
    }
    fn line(
        &mut self,
        runtime: &mut dyn HeadlessSession,
        line: &str,
        control: &mut dyn Write,
    ) -> io::Result<()> {
        let trimmed = line.trim();
        match trimmed {
            "/quit" => {
                self.dispatch(runtime, Intent::Quit, control)?;
                return Ok(());
            }
            "/cancel" => {
                self.dispatch(runtime, Intent::Cancel, control)?;
                writeln!(
                    control,
                    "Cancellation requested (only active generation is affected)."
                )?;
                return Ok(());
            }
            "/help" => {
                write!(control, "{HELP}")?;
                return Ok(());
            }
            "/inspect" => {
                inspect(runtime, control)?;
                return Ok(());
            }
            "/diagnostics" => {
                diagnostics(runtime, control)?;
                return Ok(());
            }
            _ => (),
        }
        if let Some(command) = persistence_command(trimmed) {
            let result = match command {
                Ok(PersistenceCommand::Save(slot)) => runtime.save(slot),
                Ok(PersistenceCommand::List(page)) => runtime.list(page),
                Ok(PersistenceCommand::Load(command)) => runtime.load(command),
                Ok(PersistenceCommand::Rewind(count)) => runtime.dispatch(Intent::Rewind(count)),
                Err(message) => {
                    writeln!(control, "Rejected: {message}")?;
                    return Ok(());
                }
            };
            match result {
                Ok(()) => writeln!(
                    control,
                    "Storage operation started; /inspect and /quit remain available."
                )?,
                Err(error) => writeln!(control, "Rejected: {error}")?,
            }
            return Ok(());
        }
        if runtime.storage_busy() || runtime.shutdown() != Shutdown::Open {
            writeln!(control, "Rejected: storage or shutdown is in progress.")?;
            return Ok(());
        }
        if matches!(
            runtime.controller().phase(),
            Phase::Running | Phase::Cancelling | Phase::Closing | Phase::Closed | Phase::Faulted
        ) {
            writeln!(
                control,
                "Rejected: generation or cleanup is in progress, or the worker is unavailable. Use /cancel or /quit."
            )?;
            return Ok(());
        }
        if trimmed == "/retry" {
            self.dispatch(runtime, Intent::Retry, control)?;
            return Ok(());
        }
        let edit = std::mem::replace(&mut self.edit, Edit::None);
        match edit {
            Edit::Title => {
                match WorldTitle::new(line) {
                    Ok(title) => {
                        self.edit = Edit::Description(title);
                        writeln!(control, "Description: enter a nonblank replacement.")?;
                    }
                    Err(e) => {
                        self.edit = Edit::Title;
                        writeln!(control, "Rejected: {e}. Title:")?;
                    }
                }
                return Ok(());
            }
            Edit::Description(title) => {
                match WorldDescription::new(line) {
                    Ok(description) => {
                        if self.dispatch(
                            runtime,
                            Intent::ReplaceOutline(WorldOutline::new(title, description)),
                            control,
                        )? {
                            show_stage(runtime, control)?;
                        }
                    }
                    Err(e) => {
                        self.edit = Edit::Description(title);
                        writeln!(control, "Rejected: {e}. Description:")?;
                    }
                }
                return Ok(());
            }
            Edit::None => (),
        }
        if trimmed == "/edit" && matches!(runtime.controller().stage(), Stage::OutlineReview { .. })
        {
            self.edit = Edit::Title;
            writeln!(control, "Title: enter a nonblank replacement.")?;
            return Ok(());
        }
        if let Some(number) = trimmed.strip_prefix("/action ") {
            match number.parse::<usize>() {
                Ok(number) => {
                    self.dispatch(runtime, Intent::Action(number), control)?;
                }
                Err(_) => {
                    writeln!(control, "Rejected: /action requires a one-based number.")?;
                }
            }
            return Ok(());
        }
        if trimmed == "/event" {
            self.dispatch(
                runtime,
                Intent::Turn(TurnDirection::InterestingEvent),
                control,
            )?;
            return Ok(());
        }
        let text = if let Some(escaped) = trimmed.strip_prefix("//") {
            format!("/{escaped}")
        } else {
            if trimmed.starts_with('/') {
                writeln!(
                    control,
                    "Rejected: unsupported command. /help lists commands; // sends a literal slash."
                )?;
                return Ok(());
            }
            line.to_owned()
        };
        match runtime.controller().stage() {
            Stage::Brief => match Brief::new(&text) {
                Ok(brief) => {
                    self.dispatch(runtime, Intent::SubmitBrief(brief), control)?;
                }
                Err(e) => {
                    writeln!(control, "Rejected: {e}. Enter a nonblank brief.")?;
                }
            },
            Stage::OutlineReview { .. } => {
                if trimmed.is_empty() {
                    self.dispatch(runtime, Intent::AcceptOutline, control)?;
                } else {
                    writeln!(
                        control,
                        "Rejected: empty line accepts the outline; /edit replaces it."
                    )?;
                }
            }
            Stage::CastSelection { .. } => {
                let position = trimmed.parse::<usize>().ok().and_then(|n| n.checked_sub(1));
                if let Some(position) = position {
                    self.dispatch(
                        runtime,
                        Intent::Select(PlayablePosition::new(position)),
                        control,
                    )?;
                } else {
                    writeln!(
                        control,
                        "Rejected: choose a one-based playable character number."
                    )?;
                }
            }
            Stage::Playing { .. } => {
                let direction = if text.trim().is_empty() {
                    TurnDirection::Continue
                } else {
                    TurnDirection::Player(PlayerInput::new(text).expect("nonblank player text"))
                };
                self.dispatch(runtime, Intent::Turn(direction), control)?;
            }
        }
        Ok(())
    }
    fn committed(
        &mut self,
        runtime: &dyn HeadlessSession,
        story: &mut dyn Write,
        control: &mut dyn Write,
    ) -> io::Result<()> {
        if let Some(game) = runtime.controller().game() {
            let final_text = game
                .turns()
                .last()
                .expect("accepted turn")
                .turn()
                .narrative()
                .as_str();
            if final_text.starts_with(&self.printed) && !runtime.controller().preview_incomplete() {
                story.write_all(&final_text.as_bytes()[self.printed.len()..])?;
            } else {
                writeln!(story)?;
                writeln!(
                    control,
                    "[authoritative final replaces the tentative preview]"
                )?;
                control.flush()?;
                story.write_all(final_text.as_bytes())?;
            }
            writeln!(story)?;
            story.flush()?;
            writeln!(control, "[committed turn {}]", game.turns().len())?;
            for (i, action) in game
                .turns()
                .last()
                .unwrap()
                .turn()
                .quick_actions()
                .as_slice()
                .iter()
                .enumerate()
            {
                writeln!(
                    control,
                    "Action {}: {} ({:?})",
                    i + 1,
                    action.text().as_str(),
                    action.kind()
                )?;
            }
            writeln!(
                control,
                "Play: enter text, empty line to continue, or /action N."
            )?;
        } else {
            show_stage(runtime, control)?;
        }
        self.printed.clear();
        Ok(())
    }
}
fn show_stage(runtime: &dyn HeadlessSession, control: &mut dyn Write) -> io::Result<()> {
    match runtime.controller().stage() {
        Stage::OutlineReview { outline, .. } => {
            writeln!(
                control,
                "Outline: {}\n{}\nOutline review: empty line accepts; /edit replaces title and description.",
                outline.title().as_str(),
                outline.description().as_str()
            )?;
        }
        Stage::CastSelection { world, .. } => {
            for (i, c) in world.cast().playable().iter().enumerate() {
                writeln!(
                    control,
                    "Character {}: {} — {}\n{}",
                    i + 1,
                    c.name().as_str(),
                    c.description().as_str(),
                    c.backstory().as_str()
                )?;
            }
            writeln!(control, "Character selection: enter a one-based number.")?;
        }
        _ => (),
    }
    Ok(())
}
fn show_failure(runtime: &dyn HeadlessSession, control: &mut dyn Write) -> io::Result<()> {
    match runtime.controller().failure() {
        Some(Failure::Generation(e)) => {
            writeln!(control, "Generation failed: {e}. State unchanged.")
        }
        Some(Failure::Cancelled { .. }) => writeln!(
            control,
            "Generation cancelled after cleanup. State unchanged."
        ),
        Some(Failure::Worker(e)) => writeln!(control, "Worker failed: {e}. State unchanged."),
        None => Ok(()),
    }
}
fn diagnostics(runtime: &dyn HeadlessSession, control: &mut dyn Write) -> io::Result<()> {
    if let Some(error) = runtime.storage_failure() {
        writeln!(
            control,
            "Storage failure: operation={:?} stage={:?} kind={:?} visibility={:?}: {}",
            error.operation(),
            error.stage(),
            error.kind(),
            error.visibility(),
            error
        )?;
    }
    let failure = match runtime.controller().failure() {
        Some(Failure::Generation(e)) | Some(Failure::Cancelled { rejected: Err(e) }) => Some(e),
        _ => None,
    };
    if let Some(e) = failure {
        // Debug string display escapes control characters. The retained values
        // remain byte-exact; lossy transport display is only a terminal view.
        match e.raw_response() {
            Some(raw) => writeln!(control, "Candidate: {:?}", raw.as_str())?,
            None => writeln!(control, "Candidate: none (no backend call was made)")?,
        }
        writeln!(
            control,
            "Transport stdout (lossy display): {:?}\nTransport stderr (lossy display): {:?}",
            e.diagnostics().stdout_lossy(),
            e.diagnostics().stderr_lossy()
        )?;
    } else {
        writeln!(control, "No generation failure diagnostics available.")?;
    }
    Ok(())
}
fn inspect(runtime: &dyn HeadlessSession, control: &mut dyn Write) -> io::Result<()> {
    let c = runtime.controller();
    writeln!(
        control,
        "Inspection: phase={:?} revision={}",
        c.phase(),
        c.revision().get()
    )?;
    match runtime.binding() {
        SaveBinding::Unbound => writeln!(
            control,
            "Save: unbound durability={:?}",
            runtime.durability()
        )?,
        SaveBinding::Bound {
            id, disk_revision, ..
        } => writeln!(
            control,
            "Save: {} disk_revision={} durability={:?}",
            id.as_str(),
            disk_revision.get(),
            runtime.durability()
        )?,
    }
    if let Some(game) = c.game() {
        writeln!(control, "Turns: {}", game.turns().len())?;
        writeln!(
            control,
            "Active limits: {:?}\nOriginal limits: {:?}",
            game.limits(),
            game.original_limits()
        )?;
        if let Some(chapter) = game.current_chapter() {
            writeln!(
                control,
                "Chapter: {} title={:?}",
                chapter.number().get(),
                chapter.title().map(|t| t.as_str())
            )?;
        }
        let summary = game.current_summary();
        writeln!(
            control,
            "World: {:?}\nSituation: {:?}",
            summary.world().as_str(),
            summary.current_situation().as_str()
        )?;
        for character in summary.characters().iter() {
            writeln!(
                control,
                "Identity: {} name={:?} state={:?}",
                character.id().as_str(),
                character.name().as_str(),
                character.details().current_state().map(|s| s.as_str())
            )?;
        }
        for e in summary.major_events().events().iter() {
            writeln!(control, "Major event: {:?}", e.as_str())?;
        }
        for e in summary.upcoming_events().iter() {
            writeln!(control, "Upcoming event: {:?}", e.as_str())?;
        }
    } else {
        writeln!(control, "No game selected.")?;
    }
    Ok(())
}

fn storage_event(
    runtime: &dyn HeadlessSession,
    event: PersistenceEvent,
    control: &mut dyn Write,
) -> io::Result<()> {
    match event {
        PersistenceEvent::Saved(receipt) => writeln!(
            control,
            "Saved: {} disk_revision={} turns={} durability={:?}",
            receipt.metadata.id.as_str(),
            receipt.metadata.revision.get(),
            runtime.controller().game().map_or(0, |g| g.turns().len()),
            runtime.durability()
        ),
        PersistenceEvent::Loaded {
            old_id,
            backup,
            unrecognized_fields,
        } => {
            if backup {
                writeln!(
                    control,
                    "Recovered backup from {}; saving into a fresh slot.",
                    old_id.as_str()
                )?;
            } else {
                writeln!(control, "Loaded: {}", old_id.as_str())?;
            }
            for field in unrecognized_fields {
                writeln!(
                    control,
                    "Unknown optional field (omitted on resave): {}",
                    display_text(field.location())
                )?;
            }
            inspect(runtime, control)
        }
        PersistenceEvent::Listed(page) => render_listing(&page, control),
        PersistenceEvent::Failed(error) => {
            writeln!(
                control,
                "Storage failed: {error}; visibility={:?}; durability={:?}.",
                error.visibility(),
                runtime.durability()
            )?;
            if matches!(
                error.operation(),
                StorageOperation::Create | StorageOperation::Replace | StorageOperation::Reconcile
            ) {
                writeln!(
                    control,
                    "Canonical story remains in memory; save failed; use /save. The in-memory revision may be lost on exit."
                )?;
            }
            if error.operation() == StorageOperation::Load {
                writeln!(
                    control,
                    "Primary loads never fall back automatically. To inspect or recover a backup, use --backup explicitly."
                )?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
/// Display-only escaping/truncation; retained save values remain untouched.
fn display_text(text: &str) -> String {
    let mut clipped: String = text.chars().take(512).collect();
    if text.chars().nth(512).is_some() {
        clipped.push_str(" [display truncated]");
    }
    format!("{clipped:?}")
}
pub fn render_listing(page: &SavePageResult, out: &mut dyn Write) -> io::Result<()> {
    if page.entries.is_empty() {
        writeln!(out, "No saves.")?;
    }
    for entry in &page.entries {
        write!(out, "{} ", entry.id.as_str())?;
        let (primary, backup) = match &entry.status {
            SaveListingStatus::Inspected { primary, backup } => {
                (primary, *backup == BackupCopy::Present)
            }
            // Not inspected under the slot lock: no claim about either copy.
            SaveListingStatus::Busy => {
                writeln!(out, "status=Busy backup=unknown")?;
                continue;
            }
            SaveListingStatus::Unreadable => {
                writeln!(out, "status=Unreadable backup=unknown")?;
                continue;
            }
        };
        match primary {
            PrimaryCopy::Valid(summary) => writeln!(
                out,
                "title={} turns={} source={:?} saved_at_unix={}.{:03} backup={backup}",
                display_text(summary.title.as_str()),
                summary.turns.get(),
                summary.source,
                summary.saved_at.unix_seconds(),
                summary.saved_at.nanoseconds() / 1_000_000,
            )?,
            PrimaryCopy::Missing if backup => writeln!(out, "status=BackupOnly backup=true")?,
            PrimaryCopy::Missing => writeln!(out, "status=Missing backup=false")?,
            PrimaryCopy::Corrupt => writeln!(out, "status=Corrupt backup={backup}")?,
            PrimaryCopy::FutureVersion { version } => writeln!(
                out,
                "status=FutureVersion version={} backup={backup}",
                version.get()
            )?,
            PrimaryCopy::Unreadable => writeln!(out, "status=Unreadable backup={backup}")?,
        }
    }
    if let Some(next) = &page.next {
        writeln!(
            out,
            "Next page: --after {} (during play: /list {})",
            next.as_str(),
            next.as_str()
        )?;
    }
    Ok(())
}
pub fn render_saved_game(stored: &StoredGame, out: &mut dyn Write) -> io::Result<()> {
    writeln!(
        out,
        "Save: {} copy={:?} disk_revision={} source={:?}",
        stored.metadata.id.as_str(),
        stored.copy,
        stored.metadata.revision.get(),
        stored.snapshot.source()
    )?;
    writeln!(
        out,
        "Title: {}\nTurns: {}\nActive limits: {:?}\nOriginal limits: {:?}",
        display_text(stored.snapshot.game().world().outline().title().as_str()),
        stored.snapshot.game().turns().len(),
        stored.snapshot.game().limits(),
        stored.snapshot.game().original_limits()
    )?;
    for field in &stored.unrecognized_fields {
        writeln!(
            out,
            "Unknown optional field (omitted on resave): {}",
            display_text(field.location())
        )?;
    }
    Ok(())
}

/// Read-only commands share the finite output queue and absolute drain bound.
pub fn query_output(
    sink: &mut dyn Write,
    render: impl FnOnce(&mut dyn Write) -> io::Result<()>,
    mut interrupted: impl FnMut() -> bool,
) -> io::Result<()> {
    let mut output = QueuedOutput::new(sink);
    render(&mut output)?;
    let deadline = Instant::now() + output::STALL_LIMIT;
    loop {
        if interrupted() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "terminal output did not drain on exit",
            ));
        }
        output.pump(Instant::now())?;
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "terminal output did not drain on exit",
            ));
        }
        if output.is_empty() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
