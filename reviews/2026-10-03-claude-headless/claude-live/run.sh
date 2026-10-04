#!/usr/bin/env sh
# Interactive live acceptance of the shipped binary, no prompt overrides.
echo "$$" > reviews/2026-10-03-claude-headless/claude-live/app.pid
exec target/debug/cyoa play --headless --backend claude
