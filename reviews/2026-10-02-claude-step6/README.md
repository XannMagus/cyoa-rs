# Claude adapter step 6 — composed story acceptance

Baseline `0d2656a`. Adds `tests/claude_story_acceptance.rs` (eight tests) and
`tests/fixtures/claude/README.md`; no production change. The scenario content
(`fixtures/codex/story.json`) is intentionally shared with the Codex acceptance test:
it encodes backend-agnostic project decisions, so one frozen story protects both
adapters, while the transport wrapper is Claude's own. Affected decisions:
BACKENDS-001, ARCH-002/003, TEXT-001, STREAM-001, IDENTITY-001/002, PROMPTS-001,
LIMITS-001, CHAPTER-001, ACCEPTANCE-001 (broad statuses unchanged).

## Coverage

Real adapter + real supervisor + fixture children, through `GenerationEngine` and
`StoryUseCases`: edited outline reaches the cast prompt; namesake cast and repaired
IDs; opening (exactly one identity instruction) and continuation; chapter break and
retitling; rewind restoring equal state and the next request's context; both restore
policies (original vs current limits reach the instructions, schema description and
chapter bridge); zero NPCs; null vs `[]` thread updates. Requests are read from the
actual child (argv `--system-prompt`/`--json-schema`, stdin prompt) and checked for
Claude's own schema adaptation only (no root `$schema`, none of Codex's
all-required/annotated-ref changes), the local advisor suppression on every call,
and absence of any advisor text in shared instructions or prompts.

Failures leave the complete game equal and the next explicit retry launches exactly
one child with the same prompt: nonzero exit after a result, `is_error` result,
truncated and missing results, domain-invalid payload, idle cancellation, cancellation
from the first live preview with the child killed rather than awaited, cancellation
from a complete-only emission, stdout cap with prefix metadata, and a real workspace
cleanup failure. Two Claude-specific composed cases: a preview that disagrees with the
final payload (previews shown, only the final committed) and a live-shaped enforce
retry (prose in a text block of message 1 never reaches narrative or state).
Provenance on committed turns is exactly what the stream reported (model, provider,
list-price estimate).

## Differences from the Codex acceptance, deliberately

- Previews arrive incrementally (>1 fragment per turn) and can precede a failure, so
  the failure cases assert the preview is a prefix of the payload's narrative rather
  than empty; they are never committed.
- Candidate evidence differs by failure: a rejected-but-complete result (exit 7,
  `is_error`, domain-invalid) retains the payload as `raw_response`; a truncated or
  missing result has none (`""`), with all bytes in `diagnostics`.
- The child is killed at a codec rejection (`is_error`), which can precede its
  stderr, so stderr is not asserted for that case, as the Codex test does for
  `turn.failed`.
- Cap/bounds are 64 KiB because Claude streams are larger than Codex's one-message
  transcripts.

## TDD record

No red phase: written after the adapter and codec, against already-implemented
behavior, and all eight tests passed on first run (nothing was manufactured). Their
sensitivity is established by the persistent mutations in the next step.

## Registry

Registered as a `claude_story_acceptance` check under eleven contracts (the same ones
the Codex acceptance tests protect, plus the two Claude-specific tests under TEXT-001
and STREAM-001). Contract text records the step.

## Verification and limits

fmt, warnings-denied Clippy and the full gate pass: `contracts.log`. Synthetic payloads
only: no live fiction, persistence or presentation claim.
