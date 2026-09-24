//! Structurally validated generation configuration; semantic override approval is separate.
use cyoa_core::{
    limits::Limits,
    text::Brief,
    world::{PlayerCharacter, WorldOutline},
};
use minijinja::{Environment, UndefinedBehavior, Value, context};
use serde::Deserialize;
use std::collections::BTreeSet;
use thiserror::Error;

const PROMPTS: &str = include_str!("defaults/prompts.toml");
const STYLES: &str = include_str!("defaults/styles.toml");
const DOCS: &str = include_str!("defaults/schema_docs.toml");
const ADDITIONS: &str = include_str!("defaults/prompt_additions.toml");

#[derive(Debug, Error)]
#[error("{file}: {path}: {message}")]
pub struct ConfigurationError {
    file: &'static str,
    path: String,
    message: String,
}
impl ConfigurationError {
    fn at(file: &'static str, path: &str, message: impl std::fmt::Display) -> Self {
        Self {
            file,
            path: path.into(),
            message: message.to_string(),
        }
    }
}

#[derive(Debug, Error)]
#[error("{template}: {source}")]
pub struct RenderError {
    template: String,
    #[source]
    source: minijinja::Error,
}

/// Owns compiled templates. Construction checks structure and representative
/// contexts; data-dependent rendering remains fallible. No public override loader
/// exists until the separate project-invariant validator is implemented.
#[derive(Debug)]
pub struct GenerationTemplates {
    environment: Environment<'static>,
    styles: Styles,
    quick_actions: QuickActions,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StyleEntry {
    key: String,
    #[serde(rename = "name")]
    _name: String,
    prompt: String,
}
#[derive(Debug)]
struct StyleTable {
    first: StyleEntry,
    rest: Vec<StyleEntry>,
}
impl StyleTable {
    fn parse(value: toml::Value, name: &str) -> Result<Self, ConfigurationError> {
        let entries: Vec<StyleEntry> = value
            .try_into()
            .map_err(|e| ConfigurationError::at("styles.toml", name, e))?;
        let mut keys = BTreeSet::new();
        for entry in &entries {
            if entry.key.trim().is_empty() || !keys.insert(&entry.key) {
                return Err(ConfigurationError::at(
                    "styles.toml",
                    name,
                    "blank or duplicate style key",
                ));
            }
        }
        let mut entries = entries.into_iter();
        let first = entries.next().ok_or_else(|| {
            ConfigurationError::at("styles.toml", name, "style table must not be empty")
        })?;
        Ok(Self {
            first,
            rest: entries.collect(),
        })
    }
    fn entries(&self) -> impl Iterator<Item = &StyleEntry> {
        std::iter::once(&self.first).chain(&self.rest)
    }
    fn resolve(&self, key: Option<&str>) -> &StyleEntry {
        self.entries()
            .find(|entry| Some(entry.key.as_str()) == key)
            .unwrap_or(&self.first)
    }
}
#[derive(Debug)]
struct Styles {
    art_style: StyleTable,
    pace: StyleTable,
    tone: StyleTable,
    narration: StyleTable,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionKind {
    key: String,
    #[serde(rename = "label")]
    _label: String,
    meaning: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuickActions {
    requested: Vec<Vec<String>>,
    kinds: Vec<ActionKind>,
}

/// These builders are shared by template validation and real rendering.
pub(super) enum TemplateContext<'a> {
    Empty,
    Brief(&'a Brief),
    Cast(&'a Brief, &'a WorldOutline),
    Limits(&'a Limits),
    Markdown(&'a str),
    Role(&'a PlayerCharacter),
    WorldAndProtagonist(&'a WorldOutline, &'a PlayerCharacter),
    PlayerInput(&'a str),
}
impl TemplateContext<'_> {
    pub(super) fn value(self) -> Value {
        match self {
            Self::Empty => context! {},
            Self::Brief(brief) => context! {brief => brief.as_str()},
            Self::Cast(brief, world) => {
                context! { brief => brief.as_str(), world => world_context(world) }
            }
            Self::Limits(limits) => super::limits_context::limits_context(limits),
            Self::Markdown(what) => context! {what},
            Self::Role(player) => {
                context! { protagonist => context! {name => player.name().as_str()} }
            }
            Self::WorldAndProtagonist(world, player) => context! {
                world => world_context(world), protagonist => context! {
                    name => player.name().as_str(), description => player.description().as_str(), backstory => player.backstory().as_str()
                }
            },
            Self::PlayerInput(player_input) => context! {player_input},
        }
    }
}
fn world_context(world: &WorldOutline) -> Value {
    context! { title => world.title().as_str(), world_description => world.description().as_str() }
}

impl GenerationTemplates {
    pub fn bundled() -> Result<Self, ConfigurationError> {
        Self::from_sources(PROMPTS, STYLES, DOCS)
    }

    fn from_sources(prompts: &str, styles: &str, docs: &str) -> Result<Self, ConfigurationError> {
        let prompts = parse(prompts, "prompts.toml")?;
        let styles = parse(styles, "styles.toml")?;
        let docs = parse(docs, "schema_docs.toml")?;
        validate_shape(
            &prompts,
            &parse(PROMPTS, "prompts.toml")?,
            "prompts.toml",
            "",
        )?;
        validate_shape(&styles, &parse(STYLES, "styles.toml")?, "styles.toml", "")?;
        validate_shape(&docs, &documentation_shape(), "schema_docs.toml", "")?;
        let prompts =
            typed_configuration::<configuration::PromptConfiguration>(prompts, "prompts.toml")?;
        let styles =
            typed_configuration::<configuration::StyleConfiguration>(styles, "styles.toml")?;
        let styles = Styles {
            art_style: StyleTable::parse(styles["art_style"].clone(), "art_style")?,
            pace: StyleTable::parse(styles["pace"].clone(), "pace")?,
            tone: StyleTable::parse(styles["tone"].clone(), "tone")?,
            narration: StyleTable::parse(styles["narration"].clone(), "narration")?,
        };
        let quick_actions: QuickActions = prompts["quick_actions"]
            .clone()
            .try_into()
            .map_err(|e| ConfigurationError::at("prompts.toml", "quick_actions", e))?;
        let keys: BTreeSet<_> = quick_actions
            .kinds
            .iter()
            .map(|kind| kind.key.as_str())
            .collect();
        if keys.len() != quick_actions.kinds.len() || keys.contains("") {
            return Err(ConfigurationError::at(
                "prompts.toml",
                "quick_actions.kinds",
                "blank or duplicate kind key",
            ));
        }
        if quick_actions.requested.is_empty()
            || quick_actions.requested.iter().any(|group| {
                group.is_empty() || group.iter().any(|key| !keys.contains(key.as_str()))
            })
        {
            return Err(ConfigurationError::at(
                "prompts.toml",
                "quick_actions.requested",
                "empty group or unknown action kind",
            ));
        }
        let mut environment = Environment::new();
        environment.set_undefined_behavior(UndefinedBehavior::Strict);
        let mut result = Self {
            environment,
            styles,
            quick_actions,
        };
        result.compile_tree("prompts.toml", "", &prompts)?;
        result.compile_tree("schema_docs.toml", "", &docs)?;
        result.compile_tree(
            "prompt_additions.toml",
            "",
            &parse(ADDITIONS, "prompt_additions.toml")?,
        )?;
        let mut fragments = Vec::new();
        for (table, entries) in [
            ("art_style", &result.styles.art_style),
            ("pace", &result.styles.pace),
            ("tone", &result.styles.tone),
            ("narration", &result.styles.narration),
        ] {
            for entry in entries.entries() {
                fragments.push((
                    "styles.toml",
                    format!("{table}.{}", entry.key),
                    entry.prompt.clone(),
                ));
            }
        }
        for entry in &result.quick_actions.kinds {
            fragments.push((
                "prompts.toml",
                format!("quick_actions.kinds.{}.meaning", entry.key),
                entry.meaning.clone(),
            ));
        }
        for (file, path, source) in fragments {
            result.compile(file, &path, &source, TemplateContext::Empty.value())?;
        }
        Ok(result)
    }

    fn compile_tree(
        &mut self,
        file: &'static str,
        path: &str,
        value: &toml::Value,
    ) -> Result<(), ConfigurationError> {
        match value {
            toml::Value::Table(table) => {
                for (key, value) in table {
                    self.compile_tree(file, &joined(path, key), value)?;
                }
            }
            toml::Value::String(source) => {
                for limits in [
                    Limits::default(),
                    Limits {
                        max_generated_npcs: cyoa_core::limits::MaxGeneratedNpcs::new(0),
                        min_playable_characters: cyoa_core::limits::MinPlayableCharacters::new(6)
                            .map_err(|e| {
                            ConfigurationError::at(file, path, e)
                        })?,
                        ..Limits::default()
                    },
                ] {
                    let ctx = example_context(file, path, &limits)?;
                    self.compile(file, path, source, ctx)?;
                }
            }
            _ => (), // Arrays are typed metadata (style presets / quick-action groups), not templates.
        }
        Ok(())
    }

    pub(super) fn render(
        &self,
        file: &str,
        path: &str,
        ctx: TemplateContext<'_>,
    ) -> Result<String, RenderError> {
        let template = format!("{file}:{path}");
        self.environment
            .get_template(&template)
            .and_then(|t| t.render(ctx.value()))
            .map_err(|source| RenderError { template, source })
    }
}

fn parse(source: &str, file: &'static str) -> Result<toml::Value, ConfigurationError> {
    toml::from_str(source).map_err(|e| ConfigurationError::at(file, "", e))
}
fn joined(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.into()
    } else {
        format!("{path}.{key}")
    }
}
fn validate_shape(
    value: &toml::Value,
    expected: &toml::Value,
    file: &'static str,
    path: &str,
) -> Result<(), ConfigurationError> {
    match (value, expected) {
        (toml::Value::Table(table), toml::Value::Table(shape)) => {
            for key in table.keys() {
                if !shape.contains_key(key) {
                    return Err(ConfigurationError::at(
                        file,
                        &joined(path, key),
                        "unknown field",
                    ));
                }
            }
            for (key, expected) in shape {
                let path = joined(path, key);
                let value = table
                    .get(key)
                    .ok_or_else(|| ConfigurationError::at(file, &path, "missing field"))?;
                validate_shape(value, expected, file, &path)?;
            }
        }
        (toml::Value::Array(values), toml::Value::Array(shape)) => {
            if let Some(expected) = shape.first() {
                for (i, value) in values.iter().enumerate() {
                    validate_shape(value, expected, file, &format!("{path}[{i}]"))?;
                }
            }
        }
        _ if value.type_str() == expected.type_str() => (),
        _ => {
            return Err(ConfigurationError::at(
                file,
                path,
                format!("expected {}", expected.type_str()),
            ));
        }
    }
    Ok(())
}

fn example_context(
    file: &'static str,
    path: &str,
    limits: &Limits,
) -> Result<Value, ConfigurationError> {
    use cyoa_core::text::{
        Backstory, CharacterDescription, CharacterName, WorldDescription, WorldTitle,
    };
    let brief = Brief::new("Synthetic brief").map_err(|e| ConfigurationError::at(file, path, e))?;
    let world = WorldOutline::new(
        WorldTitle::new("Synthetic world").map_err(|e| ConfigurationError::at(file, path, e))?,
        WorldDescription::new("Synthetic description")
            .map_err(|e| ConfigurationError::at(file, path, e))?,
    );
    let player = PlayerCharacter::new(
        CharacterName::new("Player").map_err(|e| ConfigurationError::at(file, path, e))?,
        CharacterDescription::new("Description")
            .map_err(|e| ConfigurationError::at(file, path, e))?,
        Backstory::new("Backstory").map_err(|e| ConfigurationError::at(file, path, e))?,
    );
    let ctx = if file == "schema_docs.toml" {
        TemplateContext::Limits(limits)
    } else {
        match path {
            "world.prompt" => TemplateContext::Brief(&brief),
            "cast.prompt" => TemplateContext::Cast(&brief, &world),
            "cast.instructions" | "turn.rule_new_major_events" => TemplateContext::Limits(limits),
            "fragments.markdown" => TemplateContext::Markdown("descriptive text"),
            "turn.role" => TemplateContext::Role(&player),
            "turn.world_and_protagonist_template" => {
                TemplateContext::WorldAndProtagonist(&world, &player)
            }
            "turn.prompt_parts.player_directs" | "turn.prompt_parts.opening_with_input" => {
                TemplateContext::PlayerInput("Go")
            }
            _ => TemplateContext::Empty,
        }
    };
    Ok(ctx.value())
}

impl GenerationTemplates {
    fn compile(
        &mut self,
        file: &'static str,
        path: &str,
        source: &str,
        ctx: Value,
    ) -> Result<(), ConfigurationError> {
        let name = format!("{file}:{path}");
        self.environment
            .add_template_owned(name.clone(), source.to_owned())
            .map_err(|e| ConfigurationError::at(file, path, e))?;
        let template = self
            .environment
            .get_template(&name)
            .map_err(|e| ConfigurationError::at(file, path, e))?;
        for variable in template.undeclared_variables(true) {
            let mut value = ctx.clone();
            for part in variable.split('.') {
                value = value
                    .get_attr(part)
                    .map_err(|e| ConfigurationError::at(file, path, e))?;
            }
            if value.is_undefined() {
                return Err(ConfigurationError::at(
                    file,
                    path,
                    format!("unavailable variable {variable}"),
                ));
            }
        }
        template
            .render(ctx)
            .map_err(|e| ConfigurationError::at(file, path, e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub struct RenderedGeneration {
    instructions: cyoa_core::text::Instructions,
    prompt: cyoa_core::text::RenderedPrompt,
    schema: serde_json::Value,
}
impl RenderedGeneration {
    pub fn instructions(&self) -> &cyoa_core::text::Instructions {
        &self.instructions
    }
    pub fn prompt(&self) -> &cyoa_core::text::RenderedPrompt {
        &self.prompt
    }
    pub fn schema(&self) -> &serde_json::Value {
        &self.schema
    }
}
impl GenerationTemplates {
    pub fn world_request(&self, brief: &Brief) -> Result<RenderedGeneration, RenderError> {
        let instructions =
            self.render("prompts.toml", "world.instructions", TemplateContext::Empty)?;
        let prompt = self.render(
            "prompts.toml",
            "world.prompt",
            TemplateContext::Brief(brief),
        )?;
        Ok(RenderedGeneration {
            instructions: cyoa_core::text::Instructions::new(instructions),
            prompt: cyoa_core::text::RenderedPrompt::new(prompt),
            schema: self
                .schema::<super::wire::WorldOutlineWire>("WorldOutline", &Limits::default())?,
        })
    }
}

mod rendering;
impl GenerationTemplates {
    pub fn cast_request(
        &self,
        brief: &Brief,
        world: &WorldOutline,
        limits: &Limits,
    ) -> Result<RenderedGeneration, RenderError> {
        let instructions = self.render(
            "prompts.toml",
            "cast.instructions",
            TemplateContext::Limits(limits),
        )?;
        let prompt = self.render(
            "prompts.toml",
            "cast.prompt",
            TemplateContext::Cast(brief, world),
        )?;
        Ok(RenderedGeneration {
            instructions: cyoa_core::text::Instructions::new(instructions),
            prompt: cyoa_core::text::RenderedPrompt::new(prompt),
            schema: self.schema::<super::wire::GeneratedCastWire>("GeneratedCast", limits)?,
        })
    }
}
impl GenerationTemplates {
    pub fn world_schema(&self) -> Result<serde_json::Value, RenderError> {
        self.schema::<super::wire::WorldOutlineWire>("WorldOutline", &Limits::default())
    }
    pub fn cast_schema(&self, limits: &Limits) -> Result<serde_json::Value, RenderError> {
        self.schema::<super::wire::GeneratedCastWire>("GeneratedCast", limits)
    }
    pub fn turn_schema(&self, limits: &Limits) -> Result<serde_json::Value, RenderError> {
        self.schema::<super::wire::StoryTurnWire>("StoryTurn", limits)
    }
}
fn documentation_shape() -> toml::Value {
    let mut table = toml::Table::new();
    table.insert("version".into(), 1.into());
    for (name, fields) in super::schema::documentation_fields() {
        table.insert(
            name,
            toml::Value::Table(
                fields
                    .into_iter()
                    .map(|field| (field, toml::Value::String(String::new())))
                    .collect(),
            ),
        );
    }
    toml::Value::Table(table)
}
mod configuration;
fn typed_configuration<T: serde::de::DeserializeOwned + serde::Serialize>(
    value: toml::Value,
    file: &'static str,
) -> Result<toml::Value, ConfigurationError> {
    let typed: T = value
        .try_into()
        .map_err(|e| ConfigurationError::at(file, "", e))?;
    toml::Value::try_from(typed).map_err(|e| ConfigurationError::at(file, "", e))
}
