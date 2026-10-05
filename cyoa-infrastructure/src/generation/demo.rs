//! Version 1 demo: the frozen Phase 0 story, with five successful turns.
//! Replay position comes from the inward generation stage and committed turn
//! count, never the number of attempts. Arbitrary choices do not alter fiction.
use super::{
    engine::GenerationEngine,
    scripted::ChunkedBackend,
    templates::{ConfigurationError, GenerationTemplates},
};
use crate::backend::BackendError;
use cyoa_application::{
    cancellation::CancellationToken,
    diagnostics::TransportDiagnostics,
    generation::{Generated, GenerationFailure, StoryGenerator, TurnDirection},
};
use cyoa_core::{
    game::GameState,
    limits::Limits,
    text::Brief,
    turn::StoryTurn,
    world::{WorldCast, WorldOutline},
};

pub struct HarbourDemo {
    outline: String,
    cast: String,
    turns: Vec<String>,
    engine: GenerationEngine<ChunkedBackend>,
}
pub fn harbour_v1() -> Result<HarbourDemo, ConfigurationError> {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/phase0_story.json"))
            .expect("bundled demo fixture is valid JSON");
    Ok(HarbourDemo {
        outline: fixture["outline"].to_string(),
        cast: fixture["cast"].to_string(),
        turns: fixture["turns"]
            .as_array()
            .expect("bundled demo turns")
            .iter()
            .map(ToString::to_string)
            .collect(),
        engine: GenerationEngine::new(ChunkedBackend::new([], 17), GenerationTemplates::bundled()?),
    })
}
impl HarbourDemo {
    fn select(&mut self, response: Option<String>) {
        let response = response.ok_or_else(|| BackendError::Unavailable {
            message: "script exhausted: unexpected generation call".into(),
            diagnostics: TransportDiagnostics::empty(),
        });
        self.engine
            .replace_backend(ChunkedBackend::new([response], 17));
    }
}
impl StoryGenerator for HarbourDemo {
    fn outline(
        &mut self,
        brief: &Brief,
        cancel: &CancellationToken,
    ) -> Result<Generated<WorldOutline>, GenerationFailure> {
        self.select(Some(self.outline.clone()));
        self.engine.outline(brief, cancel)
    }
    fn cast(
        &mut self,
        brief: &Brief,
        outline: &WorldOutline,
        limits: &Limits,
        cancel: &CancellationToken,
    ) -> Result<Generated<WorldCast>, GenerationFailure> {
        self.select(Some(self.cast.clone()));
        self.engine.cast(brief, outline, limits, cancel)
    }
    fn turn(
        &mut self,
        state: &GameState,
        direction: &TurnDirection,
        cancel: &CancellationToken,
        on_narrative: &mut dyn FnMut(&str),
    ) -> Result<Generated<StoryTurn>, GenerationFailure> {
        self.select(self.turns.get(state.turns().len()).cloned());
        self.engine.turn(state, direction, cancel, on_narrative)
    }
}

#[cfg(test)]
mod tests {
    use cyoa_application::persistence::DemoScenarioId;

    #[test]
    fn bundled_harbour_script_length_is_the_scenarios_declared_passages() {
        let demo = super::harbour_v1().unwrap();
        assert_eq!(demo.turns.len(), DemoScenarioId::HarbourV1.passages().get());
    }
}
