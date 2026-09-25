//! Dumps the real bundled world/cast/opening/continuation requests by replaying
//! the frozen phase0 story fixture through the production templates, exactly as
//! `cyoa-infrastructure/tests/phase_zero_acceptance.rs` does, but capturing the
//! rendered requests instead of asserting on them. No network calls here.
use cyoa_application::{
    cancellation::CancellationSource,
    generation::{StoryUseCases, TurnDirection},
};
use cyoa_core::{
    limits::{Limits, MajorEventLimit},
    text::{Brief, PlayerInput},
    world::PlayablePosition,
};
use cyoa_infrastructure::generation::{
    engine::GenerationEngine, scripted::ScriptedBackend, templates::GenerationTemplates,
};
use serde_json::Value;
use std::fs;
use std::path::Path;

fn fixture(repo_root: &Path) -> Value {
    let path = repo_root.join("cyoa-infrastructure/tests/fixtures/phase0_story.json");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading fixture at {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap()
}

fn write_request(dir: &Path, name: &str, instructions: &str, prompt: &str, schema: &Value) {
    let out = dir.join(name);
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("instructions.txt"), instructions).unwrap();
    fs::write(out.join("prompt.txt"), prompt).unwrap();
    fs::write(
        out.join("schema.json"),
        serde_json::to_string_pretty(schema).unwrap(),
    )
    .unwrap();
    let adapted = cyoa_infrastructure::generation::backend_compat::claude_cli::adapt_schema(
        schema.clone(),
    );
    fs::write(
        out.join("schema.claude-adapted.json"),
        serde_json::to_string_pretty(&adapted).unwrap(),
    )
    .unwrap();
    eprintln!(
        "{name}: instructions {}B, prompt {}B, schema {}B (adapted {}B)",
        instructions.len(),
        prompt.len(),
        serde_json::to_string(schema).unwrap().len(),
        serde_json::to_string(&adapted).unwrap().len()
    );
}

fn main() {
    let mut args = std::env::args();
    let out_dir = args.nth(1).expect(
        "usage: dump_requests <output-dir> <path-to-cyoa-rs-repo-root>",
    );
    let repo_root = args
        .next()
        .expect("usage: dump_requests <output-dir> <path-to-cyoa-rs-repo-root>");
    let out_dir = Path::new(&out_dir);
    let repo_root = Path::new(&repo_root);

    let fixture = fixture(repo_root);
    let outline_response = fixture["outline"].to_string();
    let cast_response = fixture["cast"].to_string();
    let opening_response = fixture["turns"][0].to_string();
    let continuation_response = fixture["turns"][1].to_string();

    let backend = ScriptedBackend::new(
        [
            outline_response,
            cast_response,
            opening_response,
            continuation_response,
        ]
        .into_iter()
        .map(Ok),
    );
    let mut cases = StoryUseCases::new(GenerationEngine::new(
        backend,
        GenerationTemplates::bundled().unwrap(),
    ));

    let source = CancellationSource::default();
    let brief = Brief::new("Two Ajaxes, a harbour and a voyage").unwrap();
    let outline = cases
        .generate_outline(&brief, &source.token())
        .unwrap()
        .into_parts()
        .0;
    let limits = Limits {
        max_major_events: MajorEventLimit::new(2).unwrap(),
        ..Limits::default()
    };
    let world = cases
        .generate_world(&brief, outline, &limits, &source.token())
        .unwrap();
    let mut game = cyoa_core::game::GameState::start(
        brief,
        world.select(PlayablePosition::new(1)).unwrap(),
        cyoa_core::style::StoryStyle::default(),
        limits,
    );
    cases
        .take_turn(
            &mut game,
            TurnDirection::Continue,
            &CancellationSource::default().token(),
            &mut |_| {},
        )
        .unwrap();
    let input = PlayerInput::new("I ask the harbourmaster about the missing ship.").unwrap();
    cases
        .take_turn(
            &mut game,
            TurnDirection::Player(input),
            &CancellationSource::default().token(),
            &mut |_| {},
        )
        .unwrap();

    let backend = cases.into_generator().into_backend();
    let requests = backend.requests();
    assert_eq!(requests.len(), 4, "expected outline+cast+opening+continuation");
    let names = ["world", "cast", "opening_turn", "continuation_turn"];
    for (name, req) in names.iter().zip(requests.iter()) {
        write_request(out_dir, name, req.instructions(), req.prompt(), req.schema());
    }
    eprintln!("wrote 4 requests to {}", out_dir.display());
}
