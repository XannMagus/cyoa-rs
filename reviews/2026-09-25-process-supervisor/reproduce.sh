#!/usr/bin/env bash
# Historical failing review probes, not part of the required green test suite.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.."
review=reviews/2026-09-25-process-supervisor
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
git archive ae8cece223a75beea4ea8c59c179e67b019d438f | tar -x -C "$scratch"
cp "$review/review_regressions.rs" "$scratch/cyoa-infrastructure/tests/review_regressions.rs"
cargo test --manifest-path "$scratch/Cargo.toml" -p cyoa-infrastructure \
  --test review_regressions --locked --offline
