# Headless/demo implementation and acceptance — 2026-10-02

Phase 1 items 7–8: Linux memory-only play through the existing controller/runtime,
with explicit peer backend selection or the credential-free harbour-v1 demo.
No TUI, persistence, autosave, exports or arbitrary prompt overrides are enabled.
Implementation commit: `976f945`. The live record and handoff are a separate
documentation/evidence commit.

## Implementation and offline evidence

Presentation owns Clap intent, the append-only story/control view, Linux descriptor
readiness and a scoped SIGINT flag. Main wires either concrete adapter or the
scripted demo into the same inward application use cases and controller. Auth
preflight runs before the UI on a joined thread with cancellation. No blocked stdin
thread exists. Descriptor flags are restored on drop. Output is nonblocking:
backpressure or a broken pipe is an I/O failure, followed by cancellation/join.

Outline replacement is a two-line title/description interaction. Selection validates
against the generated world and launches one explicit opening turn. Empty input has
stage-specific meaning. `/action N` is one-based, ordinary numbers are player input,
`//` escapes slash commands, unsupported commands never become inference, retry
repeats the failed intent only after cleanup. `/inspect` reads canonical chapter,
summary, identities and events; `/diagnostics` displays retained candidate/transport
failure evidence with escaped controls and explicitly lossy transport display.

Preview output is marked tentative on stderr before being flushed to stdout.
Only a validated turn gets committed/actions markers. Failed previews are discarded;
differing final prose is explicitly corrected; matching preview text is not printed
twice. A progress event and joined completion in one poll still print final prose
once, even though the controller clears the preview on acceptance.

The first binary test failed against the scaffold before implementation
([red](red.txt)). The remaining acceptance tests were added after that initial
implementation; they are not all claimed as red-first TDD. One exposed a real Clap
error: `requires=backend` alone accepted vendor settings with demo through the
required source group. Explicit demo conflicts fixed this without changing tests.

Thirteen binary tests run the actual shipped executable. Both concrete adapters
use authentic argv/protocol paths with synthetic real children, not vendor login:
full outline/edit/cast/select/five-turn story, one opening identity check, namesake
IDs, retitling/chapter break, player/action/continuation and explicit retry with
byte-equal repeated request. Separate errors cover empty brief, invalid selection,
commands/options, chosen missing executable, invalid turn/candidate evidence, broken
stdout, invalid UTF-8 input, EOF/quit/SIGINT during silent output and actual PID and
workspace disappearance. Edges cover repeated cancel, quoted Unicode/multiline
prose, literal slash/numeric input, buffered multi-line reads, complete-only output,
streamed failure/disagreement/matching final and credential-free demo exhaustion.
The large buffered-input test confirms readiness polling does not strand buffered
lines while waiting for another kernel byte. Native reads bypass Stdin's BufReader.

All 34 prior mutations remain. `ignore-headless-interrupt` disables only active
SIGINT cancellation. Its exact binary test waits at most two seconds, then explicitly
quits and verifies cleanup before the behavioral assertion. A timeout/compiler/setup
failure cannot stand in for its required `idle SIGINT cancellation regression`.

Affected decisions: PRODUCT-001, STREAM-001, BACKENDS-001, TEXT-001,
IDENTITY-001/002, CHAPTER-001, PROMPTS-001 and ACCEPTANCE-001. ARCH-001/002
boundaries remain intact; ARCH-003 shared and peer adapters are untouched.
PROMPTS-003, persistence, TUI, exports and unexercised hosts remain pending.

Validation: workspace tests/doc tests, 13 binary tests, Clippy with `-D warnings`,
and `bash scripts/check_contracts.sh` including all 35 baseline/mutant/restored checks.
See retained logs; live evidence below has separate scope.

## Live headless gate

**Passed for Codex 0.159.3** through the shipped command, bundled templates and
subscription preflight; no model override (observed model not reported by this
profile). Outline/cast plus six accepted turns, two chapter breaks, ordinary input,
empty-line continuation and displayed actions, then clean quit. A real request was
cancelled before the explicit retry committed turn 2. Canonical query bodies before
and after cancellation are equal ([before](codex-live/before-cancel.txt),
[after](codex-live/after-cancel.txt)); turn count and revision remained 1 and 4.

Tessa Brine became Tessa Reed with the same `tessa-brine` ID, no alias fork, and
stable subsequent identity across chapter boundaries. Earlier harbour clues and
permissions remained relevant to the crossing and island evidence. Final inspection
shows six turns and chapter 2 `The Warning Station`
([canonical query](codex-live/final-state.txt)). This is one story, not a promise of
arbitrary future model fidelity. The current cast had four playables, but the two
Ajax name fields included role qualifiers; the literal-name request was not fully
met. No model output or shared schema was silently corrected to manufacture success.

All eight observed generation children (cast onwards) and their private workspaces
disappeared, including the cancelled child. The initial outline predates observation.
Nine generation requests total: outline, cast, six accepted turns and one cancelled
request. No automatic retry, auth-mode change or vendor fallback occurred. All six
passages arrived complete, with no simulated incremental streaming. The observer
finished after application exit; the PTY command returned 0.

[Command](codex-live/run.sh), [PTY transcript](codex-live/session.txt),
[PID/workspace observer](codex-live/observe.sh) and [cleanup](codex-live/cleanup.txt).
The transcript mixes stdout/stderr because it records an actual interactive PTY;
stream separation is independently checked at the binary boundary offline.
The [notes](codex-live/notes.txt) distinguish observed fiction/state from unobserved
exact successful wire payloads, telemetry and null-vs-list delta shapes. Rate limits,
long-story scale, every possible descendant and remote server termination remain
unexercised. No credentials or environment values were recorded by the observer.
Claude's live headless gate remains pending and must be exercised by its own
session. Its completed live adapter gate does not substitute for this one.
