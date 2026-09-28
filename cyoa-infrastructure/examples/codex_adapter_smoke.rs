//! Opt-in bounded real-adapter outline smoke, never run by the offline gate.
//! cargo run -p cyoa-infrastructure --example codex_adapter_smoke -- OUTPUT_DIR
use cyoa_application::cancellation::CancellationSource;
use cyoa_core::{text::Brief, world::WorldOutline};
use cyoa_infrastructure::{
    backend::{Backend, BackendError, GenerationRequest},
    backends::codex_cli::{CodexCliBackend, CodexExecutable, CodexInvocationConfig},
    generation::{templates::GenerationTemplates, wire::WorldOutlineWire},
};
use serde_json::json;
use std::{env, fs, path::PathBuf, time::Instant};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(
        env::args_os()
            .nth(1)
            .ok_or("supply a new output directory")?,
    );
    fs::create_dir(&output)?; // Never overwrite a previous attempt's evidence.
    let path = env::var_os("PATH").ok_or("PATH missing")?;
    let executable = CodexExecutable::resolve("codex".as_ref(), &path, &env::current_dir()?)?;
    let config = CodexInvocationConfig::new(
        executable,
        env::var_os("HOME").ok_or("HOME missing")?.into(),
        path,
        env::var_os("CODEX_HOME").map(PathBuf::from),
        None,
    )?;
    let cancellation = CancellationSource::default();
    let started = Instant::now();
    let request = GenerationTemplates::bundled()?.world_request(&Brief::new("A quiet harbour where two unrelated lighthouse keepers both named Ajax uncover a missing bell.")?)?;
    fs::write(
        output.join("instructions.txt"),
        request.instructions().as_str(),
    )?;
    fs::write(output.join("prompt.txt"), request.prompt().as_str())?;
    fs::write(
        output.join("schema.json"),
        serde_json::to_vec_pretty(request.schema())?,
    )?;
    let mut emissions = vec![];
    let result = CodexCliBackend::connect(config, &cancellation.token()).and_then(|mut backend| {
        backend.generate(
            GenerationRequest {
                instructions: request.instructions().as_str(),
                prompt: request.prompt().as_str(),
                schema: request.schema(),
            },
            &cancellation.token(),
            &mut |text| emissions.push(text.to_owned()),
        )
    });
    match result {
        Ok(response) => {
            fs::write(output.join("stdout.jsonl"), response.diagnostics().stdout())?;
            fs::write(output.join("stderr.bin"), response.diagnostics().stderr())?;
            fs::write(output.join("payload.json"), response.raw_response())?;
            let world: WorldOutline =
                serde_json::from_value::<WorldOutlineWire>(response.value().clone())?.try_into()?;
            let usage = response.usage();
            fs::write(
                output.join("result.json"),
                serde_json::to_vec_pretty(&json!({
                    "accepted":true, "elapsed_ms":started.elapsed().as_millis(), "requested_model":null, "observed_model":null,
                    "emissions":emissions.len(), "exact_emission":emissions == [response.raw_response()],
                    "input_tokens":usage.input.map(|v| v.total()), "cached_input_tokens":usage.input.and_then(|v| v.cached()), "output_tokens":usage.output,
                    "domain_outline_valid":true,
                }))?,
            )?;
            println!("Accepted outline: {}", world.title().as_str());
            Ok(())
        }
        Err(error) => {
            let (raw, diagnostics) = match &error {
                BackendError::Cancelled {
                    raw_response,
                    diagnostics,
                }
                | BackendError::Timeout {
                    raw_response,
                    diagnostics,
                } => (raw_response.as_deref().unwrap_or(""), diagnostics),
                BackendError::Generation {
                    raw_response,
                    diagnostics,
                    ..
                } => (raw_response.as_str(), diagnostics),
                BackendError::Transport {
                    raw_response,
                    diagnostics,
                    ..
                } => (raw_response.as_str(), diagnostics.as_ref()),
                BackendError::Unavailable { diagnostics, .. } => ("", diagnostics),
            };
            fs::write(output.join("stdout.jsonl"), diagnostics.stdout())?;
            fs::write(output.join("stderr.bin"), diagnostics.stderr())?;
            fs::write(output.join("payload.json"), raw)?;
            fs::write(
                output.join("result.json"),
                serde_json::to_vec_pretty(
                    &json!({"accepted":false,"elapsed_ms":started.elapsed().as_millis(),"error":error.to_string(),"emissions":emissions.len()}),
                )?,
            )?;
            Err(error.into())
        }
    }
}
