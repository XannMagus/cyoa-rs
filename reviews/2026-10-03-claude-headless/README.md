# Claude live headless gate — 2026-10-03

Phase 1 item 9 for Claude: the same live walking-skeleton gate Codex passed on
2026-10-02 ([record](../2026-10-02-headless/README.md)), through the shipped binary.
Baseline `1663f62`. Evidence only: no production, shared DTO, schema, template or
test change. Affected decisions: BACKENDS-001, STREAM-001, PRODUCT-001,
IDENTITY-001/002, CHAPTER-001, LIMITS-001 (coverage states and the registry are
unchanged; only status prose moves from pending to passed).

## Setup

Installed CLI **2.1.286** ([version](claude-live/version.txt)); `auth status` modes
only: `claude.ai`, `firstParty`, team ([auth](claude-live/auth-status.json)). No
credential file, email, organisation ID or environment dump was recorded.
[Command](claude-live/run.sh): `target/debug/cyoa play --headless --backend claude`,
built from the baseline. Bundled templates, no model override (the headless view does
not report the model; step 8 observed the account default `claude-sonnet-5-5`, but
this run makes no claim about it). The interactive session ran in a real PTY
(`tmux` + `script`), driven by keystrokes; the brief is the same synthetic harbour
brief as the Codex gate. The [observer](claude-live/observe.sh) is the Codex
observer with one change: it starts before the application and waits for its PID,
so unlike the Codex run the outline child is observed too.

## Observed live result — passed

| Gate item | Observation |
|---|---|
| Outline / cast | Outline "The Bell That Kept the Tide" accepted with an empty line; cast of four playables |
| Turns | **12 accepted turns**: opening, 5 × `/action N`, 2 player-prose turns, 1 empty-line continuation, 1 explicit retry |
| Chapter break | No break in the first ten turns (chapter 0 retitled "The Silent Tower" → "The Empty Yoke"); scenario extended per the plan; turn 11 opened chapter 1 "The Keeper of the Green Lantern" |
| Cancellation | `/action 3` cancelled ~1 s after its child appeared; "Generation cancelled after cleanup. State unchanged." |
| State after cancel | Canonical inspection bodies byte-equal ([before](claude-live/before-cancel.txt) Ready rev 4, [after](claude-live/after-cancel.txt) Failed rev 4, both one turn) |
| Retry | Explicit `/retry` started one new child and committed turn 2; no implicit retry |
| Rename | Player prose asked Hester Quayle to be called Hester Marrow; inspection shows `hester-quayle name="Hester Marrow"`, no alias fork, stable through the chapter break to the end |
| Memory | Summary, character states and upcoming events carried across all turns and the chapter break ([final state](claude-live/final-state.txt)) |
| Streaming | **Genuine incremental previews on all 12 accepted turns** (`[tentative preview; final validation pending]` mid-sentence, then the remainder) |
| Cleanup | All 16 observed children and private workspaces absent; app exit 0 ([observer log](claude-live/cleanup.txt)) |

The 16 observed children are: one at startup before the brief (consistent with the
construction-time auth check in `claude_cli/adapter.rs`; its argv was not recorded), outline, cast, twelve accepted turns and the cancelled request.
No `/tmp/cyoa-request-*` directory remained after exit.

Actions offered three kinds on every turn (Cautious/Bold/Social, or Investigate in
place of Social). The fiction stayed coherent: the harbour log entry, Hester's
account, the green light and the Skerry carried into the crossing, the island landing
and the chapter-1 encounter with Corra Vane. This is one story, not a promise of
arbitrary future model fidelity.

## Findings and limitations

- **Live preview/final disagreement.** On turns 10 and 12 the authoritative final
  narrative differed in wording from the streamed preview (for example `—*good*—`
  became `... *good*...`, and bold markers were dropped), not only in JSON spacing
  ([diff](claude-live/preview-vs-final.txt)). The presentation took the designed path:
  `[authoritative final replaces the tentative preview]`, then the full committed
  prose. This is the first live exercise of that path. The cause is not observed:
  headless play does not retain successful wire streams. Step 8 saw a two-message
  call, which is consistent with a first `StructuredOutput` attempt superseded by a
  second one, but that is unverified here.
- **Major-event cap reached.** Claude added several fine-grained major events per
  turn and never sent `consolidated_major_events`. The summary reached the faithful
  `MAX_MAJOR_EVENTS = 30` cap at turn 9 (29 events after turn 8), and from then on
  the oldest events (including the
  rename event) were dropped by the keep-last-30 rule. The rename itself survives in
  the identity table. This is model compliance with the consolidation instruction,
  not an adapter or merge defect, and is recorded for prompt-quality work.
- **Ajax naming.** As with Codex, both Ajax names include a role qualifier
  (`Ajax (the sailor)`, `Ajax (the mason)`); the literal-name request is not fully
  met. No output was corrected to manufacture success.
- **Not observed live:** exact successful wire payloads, telemetry, null vs list
  `upcoming_events` deltas (enforced by the scripted/adapter regressions), rate
  limits, the advisor-tax interaction under headless play, descendants beyond direct
  children, and remote server termination.
- The [PTY transcript](claude-live/session.txt) mixes stdout and stderr, so the
  control line for a corrected final appears after the final prose; stream
  separation is checked offline at the binary boundary.

## Gate

`bash scripts/check_contracts.sh` passed after the documentation updates
([log](contracts.log)), with pinned cargo-mutants 27.1.0 / cargo-nextest 0.9.132
(domain sweep: 176 mutants, 98 caught, 78 unviable). Registry and coverage states
are unchanged.

## Outcome

Claude's live headless gate passed. With Codex's gate from 2026-10-02, both
co-equal backends have now cleared the same Phase 1 walking-skeleton gate through
the shipped command, so both-backend Phase 1 acceptance is complete. Persistence
(Phase 2), TUI, export, PROMPTS-003 and unexercised hosts remain pending.
