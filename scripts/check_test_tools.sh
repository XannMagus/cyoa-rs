#!/usr/bin/env bash
# Shared version checks for workspace tests and the standalone domain sweep.
set -euo pipefail
if [[ $(cargo mutants --version) != 'cargo-mutants 27.1.0' ]] ||
   [[ $(cargo nextest --version | head -1) != cargo-nextest\ 0.9.132\ * ]]; then
  printf '%s\n' 'Install cargo-mutants 27.1.0 and cargo-nextest 0.9.132; see docs/testing/domain-mutations.md.' >&2
  exit 1
fi
