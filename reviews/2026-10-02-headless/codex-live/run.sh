#!/usr/bin/env sh
# Interactive live acceptance of the shipped binary, no prompt overrides.
echo "$$" > reviews/2026-10-02-headless/codex-live/app.pid
exec target/debug/cyoa play --headless --backend codex
