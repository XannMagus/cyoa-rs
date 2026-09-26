#!/usr/bin/env bash
# Offline, opt-in evidence tools; no network call is made by this script.
set -euo pipefail
repo=$(cd -- "$(dirname -- "$0")/../.." && pwd)
tool_dir=$(mktemp -d /tmp/cyoa-profile-build.XXXXXX)
trap 'rm -rf -- "$tool_dir"' EXIT
cat > "$tool_dir/Cargo.toml" <<MANIFEST
[package]
name = "cyoa-profile-tool"
version = "0.0.0"
edition = "2024"
[dependencies]
cyoa-infrastructure = { path = "$repo/cyoa-infrastructure" }
cyoa-application = { path = "$repo/cyoa-application" }
cyoa-core = { path = "$repo/cyoa-core" }
serde_json = { version = "1", features = ["preserve_order"] }
[[bin]]
name = "dump"
path = "$repo/reviews/2026-09-26-codex-profile/dump_requests.rs"
[[bin]]
name = "probe"
path = "$repo/reviews/2026-09-26-codex-profile/probe.rs"
[[bin]]
name = "inspect"
path = "$repo/reviews/2026-09-26-codex-profile/inspect.rs"
MANIFEST
cp "$repo/reviews/2026-09-26-codex-profile/tool.Cargo.lock" "$tool_dir/Cargo.lock"
cargo build --locked --offline --manifest-path "$tool_dir/Cargo.toml" --target-dir /tmp/cyoa-profile-target
