use super::{GenerationTemplates, RenderError, RenderedGeneration, TemplateContext};
use cyoa_core::{
    game::GameState,
    limits::Limits,
    style::StoryStyle,
    summary::StorySummary,
    text::{Instructions, RenderedPrompt},
    turn::TurnRecord,
    world::{PlayerCharacter, WorldOutline},
};
use schemars::JsonSchema;
use serde_json::{Value as JsonValue, json};

impl GenerationTemplates {
    fn prompt_part(&self, path: &str) -> Result<String, RenderError> {
        self.render("prompts.toml", path, TemplateContext::Empty)
    }
    fn prose_contract(&self, style: &StoryStyle) -> Result<String, RenderError> {
        let pace = self
            .styles
            .pace
            .resolve(style.pace.as_ref().map(|v| v.as_str()));
        let narration = self
            .styles
            .narration
            .resolve(style.narration.as_ref().map(|v| v.as_str()));
        let tone = self
            .styles
            .tone
            .resolve(style.tone.as_ref().map(|v| v.as_str()));
        let mut parts = vec![
            self.render(
                "styles.toml",
                &format!("pace.{}", pace.key),
                TemplateContext::Empty,
            )?,
            format!(
                "Write it {}, and keep to that throughout.",
                self.render(
                    "styles.toml",
                    &format!("narration.{}", narration.key),
                    TemplateContext::Empty
                )?
            ),
        ];
        let tone = self.render(
            "styles.toml",
            &format!("tone.{}", tone.key),
            TemplateContext::Empty,
        )?;
        if !tone.is_empty() {
            parts.push(tone);
        }
        parts.push(self.prompt_part("prose_contract.dialogue_and_sensory")?);
        parts.push(self.render(
            "prompts.toml",
            "fragments.markdown",
            TemplateContext::Markdown("all narrative and descriptive text"),
        )?);
        Ok(parts.join(" "))
    }
    pub(super) fn quick_action_instructions(&self) -> Result<String, RenderError> {
        let mut lines=vec!["- quick_actions: exactly three actions the reader could have the protagonist take next, each a short imperative phrase and each tagged with the kind of approach it takes. They must be three genuinely different approaches to the situation, not three phrasings of the obvious next step, so give one action of each of these kinds, in this order:".to_string()];
        for group in &self.quick_actions.requested {
            let keys = group
                .iter()
                .map(|k| format!("\"{k}\""))
                .collect::<Vec<_>>()
                .join(" or ");
            let meanings = group
                .iter()
                .map(|key| {
                    self.render(
                        "prompts.toml",
                        &format!("quick_actions.kinds.{key}.meaning"),
                        TemplateContext::Empty,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?
                .join("; or ");
            lines.push(format!("  * {keys}: {meanings}."));
        }
        lines.push("  Use whichever kind of the last group the scene affords. Use \"other\" only for an action that genuinely fits none of the kinds. Every action must be something the protagonist can actually do from where they are right now, and must follow from the passage you have just written rather than from the story in general.".into());
        Ok(lines.join("\n"))
    }
    pub fn turn_instructions(
        &self,
        style: &StoryStyle,
        world: &WorldOutline,
        player: &PlayerCharacter,
        limits: &Limits,
    ) -> Result<Instructions, RenderError> {
        let mut parts = vec![
            self.render("prompts.toml", "turn.role", TemplateContext::Role(player))?,
            self.prose_contract(style)?,
            "Rules for the fields of your response:".into(),
            self.prompt_part("turn.rule_narrative")?,
            self.quick_action_instructions()?,
        ];
        for key in [
            "rule_scene_description",
            "rule_summary_update",
            "rule_character_updates",
            "rule_character_ids",
        ] {
            parts.push(self.prompt_part(&format!("turn.{key}"))?);
        }
        parts.push(self.render(
            "prompts.toml",
            "turn.rule_new_major_events",
            TemplateContext::Limits(limits),
        )?);
        for key in [
            "rule_upcoming_events",
            "rule_current_situation",
            "rule_starts_new_chapter",
            "rule_field_order",
        ] {
            parts.push(self.prompt_part(&format!("turn.{key}"))?);
        }
        parts.push(self.render(
            "prompts.toml",
            "turn.world_and_protagonist_template",
            TemplateContext::WorldAndProtagonist(world, player),
        )?);
        Ok(Instructions::new(parts.join("\n")))
    }
    pub fn turn_prompt(
        &self,
        state: &GameState,
        input: Option<&str>,
        interesting: bool,
    ) -> Result<RenderedPrompt, RenderError> {
        let summary = state.current_summary();
        // JSON Value has no fallible custom serializers.
        let summary = summary_as_json(&summary);
        let mut parts = vec![
            self.prompt_part("turn.prompt_parts.summary_header")?,
            format!("{summary:#}"),
            String::new(),
        ];
        let (bridge, transcript) = state.prose_context();
        for (turns, key) in [(bridge, "bridge_header"), (transcript, "transcript_header")] {
            if !turns.is_empty() {
                parts.push(self.prompt_part(&format!("turn.prompt_parts.{key}"))?);
                parts.push(String::new());
                parts = add_prose(parts, turns);
            }
        }
        let input = input.filter(|text| !text.trim().is_empty());
        if state.turns().is_empty() {
            if let Some(input) = input {
                parts.push(self.render(
                    "prompts.toml",
                    "turn.prompt_parts.opening_with_input",
                    TemplateContext::PlayerInput(input),
                )?);
            }
            parts.push(self.render(
                "prompt_additions.toml",
                "turn.prompt_parts.opening_identity_review",
                TemplateContext::Empty,
            )?);
            parts.push(self.prompt_part("turn.prompt_parts.opening_instruction")?);
        } else {
            if interesting {
                parts.push(self.prompt_part("turn.prompt_parts.interesting_event_prompt")?);
                let summary = state.current_summary();
                if !summary.upcoming_events().is_empty() {
                    parts.push(
                        self.prompt_part(
                            "turn.prompt_parts.interesting_event_with_threads_header",
                        )?,
                    );
                    for thread in summary.upcoming_events().iter() {
                        parts.push(format!("- {}", thread.as_str()));
                    }
                    parts.push(self.prompt_part("turn.prompt_parts.interesting_event_fallback")?);
                }
            } else if let Some(input) = input {
                parts.push(self.render(
                    "prompts.toml",
                    "turn.prompt_parts.player_directs",
                    TemplateContext::PlayerInput(input),
                )?);
            } else {
                parts.push(self.prompt_part("turn.prompt_parts.player_no_direction")?);
            }
            parts.push(self.prompt_part("turn.prompt_parts.continue_seamlessly")?);
        }
        Ok(RenderedPrompt::new(parts.join("\n")))
    }
    pub fn turn_request(
        &self,
        state: &GameState,
        input: Option<&str>,
        interesting: bool,
    ) -> Result<RenderedGeneration, RenderError> {
        Ok(RenderedGeneration {
            instructions: self.turn_instructions(
                state.style(),
                state.world().outline(),
                state.protagonist(),
                &state.limits(),
            )?,
            prompt: self.turn_prompt(state, input, interesting)?,
            schema: self
                .schema::<super::super::wire::StoryTurnWire>("StoryTurn", &state.limits())?,
        })
    }
    pub(super) fn schema<T: JsonSchema>(
        &self,
        name: &str,
        limits: &Limits,
    ) -> Result<JsonValue, RenderError> {
        let mut value = super::super::schema::structure::<T>();
        value = self.describe(value, name, limits)?;
        if let Some(defs) = value.get_mut("$defs").and_then(JsonValue::as_object_mut) {
            let names = defs.keys().cloned().collect::<Vec<_>>();
            for name in names {
                if let Some(def) = defs.remove(&name) {
                    defs.insert(name.clone(), self.describe(def, &name, limits)?);
                }
            }
        }
        Ok(value)
    }
    fn describe(
        &self,
        mut value: JsonValue,
        name: &str,
        limits: &Limits,
    ) -> Result<JsonValue, RenderError> {
        let Some(object) = value.as_object_mut() else {
            return Ok(value);
        };
        let description = self.render(
            "schema_docs.toml",
            &format!("{name}._doc"),
            TemplateContext::Limits(limits),
        )?;
        if !description.is_empty() {
            object.insert("description".into(), description.into());
        }
        if let Some(properties) = object
            .get_mut("properties")
            .and_then(JsonValue::as_object_mut)
        {
            for (field, definition) in properties {
                let description = self.render(
                    "schema_docs.toml",
                    &format!("{name}.{field}"),
                    TemplateContext::Limits(limits),
                )?;
                if !description.is_empty() {
                    definition["description"] = description.into();
                }
            }
            object.insert("additionalProperties".into(), false.into());
        }
        Ok(value)
    }
}
fn summary_as_json(summary: &StorySummary) -> JsonValue {
    json!({
        "world": summary.world().as_str(),
        "major_events": summary.major_events().events().iter().map(|e| e.as_str()).collect::<Vec<_>>(),
        "characters": summary.characters().iter().map(|c| json!({
            "name": c.name().as_str(),
            "description": c.details().description().map(cyoa_core::text::CharacterDescription::as_str).unwrap_or(""),
            "backstory": c.details().backstory().map(cyoa_core::text::Backstory::as_str).unwrap_or(""),
            "relationships": c.details().relationships().map(cyoa_core::text::Relationships::as_str).unwrap_or(""),
            "current_state": c.details().current_state().map(cyoa_core::text::CharacterSituation::as_str).unwrap_or(""),
            "id": c.id().as_str(),
        })).collect::<Vec<_>>(),
        "current_situation": summary.current_situation().as_str(),
        "upcoming_events": summary.upcoming_events().iter().map(|e| e.as_str()).collect::<Vec<_>>(),
    })
}

fn add_prose(mut parts: Vec<String>, turns: &[TurnRecord]) -> Vec<String> {
    for turn in turns {
        if let Some(input) = turn.input() {
            parts.push(format!("[The reader directs: {}]", input.as_str()));
            parts.push(String::new());
        }
        parts.push(turn.turn().narrative().as_str().to_string());
        parts.push(String::new());
    }
    parts
}
