//! Append-only terminal view over the canonical session controller.
use crate::{
    runtime::{Intent, SessionRuntime},
    session::{Acceptance, Failure, Phase, Stage},
};
use cyoa_application::generation::{StoryGenerator, TurnDirection};
use cyoa_core::{
    text::{Brief, PlayerInput, WorldDescription, WorldTitle},
    world::{PlayablePosition, WorldOutline},
};
use std::{
    io::{self, Write},
    time::Duration,
};

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
const HELP: &str = "Commands: /help /quit /cancel /retry /inspect /diagnostics\nOutline: empty line accepts; /edit replaces title and description.\nPlay: empty line continues; /action N selects a one-based action; /event requests an interesting event. Ordinary numbers are player text. // escapes an initial slash.\nCtrl-C cancels generation; idle Ctrl-C and EOF exit. Input during generation is rejected except help, inspection, diagnostics, cancellation and quit.\n";

/// Always close and join workers, including on input/output errors.
pub fn run<G: StoryGenerator + Send + 'static>(
    runtime: &mut SessionRuntime<G>,
    input: &mut dyn Input,
    story: &mut dyn Write,
    control: &mut dyn Write,
    demo: bool,
) -> io::Result<()> {
    let result = drive(runtime, input, story, control, demo);
    let _ = runtime.dispatch(Intent::Quit);
    while runtime.controller().phase() != Phase::Closed {
        runtime.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    result
}
fn drive<G: StoryGenerator + Send + 'static>(
    runtime: &mut SessionRuntime<G>,
    input: &mut dyn Input,
    story: &mut dyn Write,
    control: &mut dyn Write,
    demo: bool,
) -> io::Result<()> {
    writeln!(
        control,
        "This session is memory only: no saves or autosave."
    )?;
    if demo {
        writeln!(
            control,
            "Demo harbour-v1: five prerecorded turns; your choices do not alter the recorded fiction. Exhaustion never switches to a live backend."
        )?;
    }
    writeln!(
        control,
        "Brief: enter a nonblank adventure idea. /help lists commands."
    )?;
    control.flush()?;
    let mut view = View::default();
    loop {
        // Input first: a cancellation observed before acceptance wins the race.
        match input.poll(Duration::from_millis(25))? {
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
        for acceptance in runtime.poll() {
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
        }
        if runtime.controller().phase() == Phase::Running {
            let preview = runtime.controller().preview();
            if preview.len() > view.printed.len() {
                if view.printed.is_empty() {
                    writeln!(control, "[tentative preview; final validation pending]")?;
                    control.flush()?;
                }
                story.write_all(&preview.as_bytes()[view.printed.len()..])?;
                story.flush()?;
                view.printed = preview.to_owned();
            }
        }
        if runtime.controller().phase() == Phase::Closed {
            if !view.printed.is_empty() {
                writeln!(story)?;
                writeln!(control, "[tentative preview discarded on exit]")?;
            }
            writeln!(control, "Session closed; in-memory state discarded.")?;
            control.flush()?;
            return Ok(());
        }
        control.flush()?;
        if runtime.controller().phase() == Phase::Closing {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl View {
    fn dispatch<G: StoryGenerator + Send + 'static>(
        &mut self,
        runtime: &mut SessionRuntime<G>,
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
    fn line<G: StoryGenerator + Send + 'static>(
        &mut self,
        runtime: &mut SessionRuntime<G>,
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
                    if self.dispatch(
                        runtime,
                        Intent::Select(PlayablePosition::new(position)),
                        control,
                    )? {
                        // Selection and the single opening call remain explicit intents.
                        self.dispatch(runtime, Intent::Turn(TurnDirection::Continue), control)?;
                    }
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
    fn committed<G: StoryGenerator + Send + 'static>(
        &mut self,
        runtime: &SessionRuntime<G>,
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
fn show_stage<G: StoryGenerator + Send + 'static>(
    runtime: &SessionRuntime<G>,
    control: &mut dyn Write,
) -> io::Result<()> {
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
fn show_failure<G: StoryGenerator + Send + 'static>(
    runtime: &SessionRuntime<G>,
    control: &mut dyn Write,
) -> io::Result<()> {
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
fn diagnostics<G: StoryGenerator + Send + 'static>(
    runtime: &SessionRuntime<G>,
    control: &mut dyn Write,
) -> io::Result<()> {
    let failure = match runtime.controller().failure() {
        Some(Failure::Generation(e)) | Some(Failure::Cancelled { rejected: Err(e) }) => Some(e),
        _ => None,
    };
    if let Some(e) = failure {
        // Debug string display escapes control characters. The retained values
        // remain byte-exact; lossy transport display is only a terminal view.
        writeln!(
            control,
            "Candidate: {:?}\nTransport stdout (lossy display): {:?}\nTransport stderr (lossy display): {:?}",
            e.raw_response().as_str(),
            e.diagnostics().stdout_lossy(),
            e.diagnostics().stderr_lossy()
        )?;
    } else {
        writeln!(control, "No generation failure diagnostics available.")?;
    }
    Ok(())
}
fn inspect<G: StoryGenerator + Send + 'static>(
    runtime: &SessionRuntime<G>,
    control: &mut dyn Write,
) -> io::Result<()> {
    let c = runtime.controller();
    writeln!(
        control,
        "Inspection: phase={:?} revision={}",
        c.phase(),
        c.revision().get()
    )?;
    if let Some(game) = c.game() {
        writeln!(control, "Turns: {}", game.turns().len())?;
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
