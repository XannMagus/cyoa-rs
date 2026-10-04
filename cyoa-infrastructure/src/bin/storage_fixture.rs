//! Test-only controlled storage child, never selected by the shipped command.
use std::{
    io::{self, Read, Write},
    path::PathBuf,
    time::Duration,
};
fn main() {
    let mut input = vec![];
    io::stdin()
        .take(160 * 1024 * 1024 + 1)
        .read_to_end(&mut input)
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&input).unwrap();
    let root = PathBuf::from(value["data"].as_str().unwrap());
    let intent = value["intent"].as_object().unwrap().keys().next().unwrap();
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join(".fixture-requests"))
        .unwrap();
    writeln!(log, "{intent}").unwrap();
    let mut pids = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join(".fixture-pids"))
        .unwrap();
    writeln!(pids, "{}", std::process::id()).unwrap();
    std::fs::write(
        root.join(".fixture-cwd"),
        std::env::current_dir().unwrap().to_str().unwrap(),
    )
    .unwrap();
    std::fs::write(
        root.join(".fixture-env"),
        std::env::vars_os()
            .map(|(key, _)| key.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let mode = std::fs::read_to_string(root.join(".fixture-mode")).unwrap();
    std::fs::write(root.join(".fixture-pid"), std::process::id().to_string()).unwrap();
    std::fs::write(root.join(".fixture-ready"), b"ready").unwrap();
    if intent == "Apply" {
        std::fs::write(root.join(".fixture-apply-ready"), b"ready").unwrap();
    }
    if mode == "silent" || (mode == "silent-apply" && intent == "Apply") {
        loop {
            std::thread::sleep(Duration::from_secs(60));
        }
    }
    if mode == "malformed" {
        println!("invalid reply");
        return;
    }
    let mut output = vec![];
    cyoa_infrastructure::persistence::helper::run_internal(input.as_slice(), &mut output).unwrap();
    if mode == "wrong-receipt" {
        let mut reply: serde_json::Value = serde_json::from_slice(&output).unwrap();
        reply["result"]["Ok"]["Receipt"]["stamp"] = serde_json::json!(vec![0; 32]);
        output = serde_json::to_vec(&reply).unwrap();
        output.push(b'\n');
    }
    io::stdout().write_all(&output).unwrap();
    io::stdout().flush().unwrap();
    if mode == "double" {
        io::stdout().write_all(&output).unwrap();
    }
    if mode == "lost-reply" {
        std::process::exit(1);
    }
}
