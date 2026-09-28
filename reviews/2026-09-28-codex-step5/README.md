# Codex adapter step 5 — composed story acceptance

Baseline: `2076aa6`, clean working tree. The next planned dependency was step 5:
prove story contracts through the existing actual adapter. This atomic slice adds
six composed tests, a frozen synthetic story derivative, a test-child cleanup
obstruction and contract registrations. Production engine, adapter, supervisor,
shared schemas/templates/wire code and Claude compatibility code are unchanged.

## What the tests exercise

Every scenario uses `StoryUseCases<GenerationEngine<CodexCliBackend>>` with the
actual subprocess fixture executable and existing supervisor. There is no alternate
Backend/StoryGenerator standing in for the adapter. The harness changes the fixture
scenario between synchronous commands and reads the real child's argv/stdin/schema
report. A separate child-written launch log counts one subscription preflight and
exactly one new generation child per generation command. Selection, editing and
rewind launch no children. PID disappearance and request-directory cleanup are
asserted after each call.

The frozen [story fixture](../../cyoa-infrastructure/tests/fixtures/codex/story.json)
is a derivative of the retained Phase 0 fixture. Two distinct playable Ajaxes and
two distinct NPC Ajaxes survive, while the complete duplicate merchant collapses.
Generated-ID collisions become `ajax` and `ajax-2`; rename and later updates target
those IDs independently. The fixture's README states its synthetic provenance and
incoming-default/zero-NPC tolerance cases. Tests wrap these hand-authored payloads
in the known four-event Codex profile, using CRLF and absent optional telemetry.
No fixture is presented as a new live model observation.

| Test | Evidence established |
|---|---|
| `late_transport_protocol_and_domain_failures_leave_state_equal_and_retry_once` | Candidate plus nonzero exit, terminal failure with exit zero, malformed payload, invalid domain narrative and missing terminal preserve exact game state and available raw evidence. Each explicit retry launches exactly one child with identical prompt/instructions. |
| `real_workspace_cleanup_failure_preserves_the_game_and_candidate` | Actual child deliberately replaces only its prepared request directory with a marker file. The real supervisor reports WorkspaceCleanup, the adapter emits nothing, application state remains equal, raw candidate/diagnostics survive, and explicit retry succeeds with the same prompt. |
| `cancellation_and_output_cap_preserve_state_context_and_explicit_retry` | A child-start handshake triggers idle cancellation. A separate complete-candidate callback cancellation preserves full state and exact payload. Output cap retains the exact bounded prefix and prefix metadata. Each retry receives unchanged context and makes one new call. |
| `both_restore_policies_control_actual_requests_snapshots_and_chapter_bridge` | Original versus current event caps and prose-bridge settings reach child stdin/schema, all restored snapshots and later commits/rewind. Original metadata survives; stricter generation-only bounds do not prune/reject established cast. |
| `zero_npc_limit_survives_cast_selection_opening_and_continuation` | Actual cast instructions/schema ask for no NPCs; boundary filtering discards supplied NPC candidates. Selection, opening and continuation retain a protagonist-only cast with no phantom NPC IDs. |
| `composed_story_preserves_namesakes_repaired_ids_retitling_and_rewind_context` | Edited outline drives cast generation; checked selection, opening identity instruction exactly once, no extra review calls, unknown action kind recovery, five successful turns, null-versus-empty threads, first-turn chapter rule, later retitling and restored rewind context. Actual prepared schema is adapted while shared/Claude schemas remain unchanged. |

Nominal callbacks yield narrative exactly once. Raw payload strings survive into
turn records; absent metadata remains unknown. Non-UTF-8 stderr is preserved on
completed transports. On immediate protocol rejection, cleanup can stop the child
before its subsequent stderr write, so that case does not assert bytes the child
may never have produced. An invalid domain candidate may have been previewed, but
never commits. Game equality assertions include all stored state, not just turn
counts; subsequent prompt assertions rule out failed/cancelled prose becoming context.

The cleanup fixture extension is test-only. It removes only `schema.json` and then
an empty, checked `cyoa-request-*` directory; it never recursively deletes an
arbitrary path. The parent asserts/reclaims the marker. A Drop backstop reclaims it
if a test assertion panics, without hiding normal-path cleanup assertions.

## Development evidence

Existing production behavior already satisfied the composed tests. This is expanded
acceptance coverage, not a claim that every assertion was written red-first or
that this slice fixed a production defect.

- [Initial setup error](evidence/test-setup-error.log): the test called a nonexistent
  `GenerationFailure::message()` accessor. It now uses Display. Compiler failure
  is not behavioral red evidence.
- [First error run](evidence/errors-first.log): the late-failure/retry table passed.
  The cleanup-obstruction case failed at runtime because the child did not yet
  implement that scenario flag and a payload was emitted normally. After adding
  the fixture behavior, [both tests passed](evidence/errors-green.log). This red/
  green establishes the new fixture capability, not a production cleanup repair.
- [First extended run](evidence/acceptance-first.log): five tests passed. The zero-NPC
  test incorrectly expected digit `0` in the user prompt. The existing bundled
  contract instead states “Create no NPCs; return an empty npcs list.” in the
  instructions, with “No NPCs; return an empty list.” in schema documentation.
  The assertion was corrected to those independently documented expectations;
  no production behavior or registered expectation was changed to force a pass.
- [Expanded green](evidence/acceptance-green.log) and
  [final focused run](evidence/acceptance-final.log) pass all six tests. Thread
  persistence/clearing/restoration assertions also inspect subsequent child prompts.

## Scope, contracts and next step

Affected IDs: BACKENDS-001, ARCH-002, ARCH-003, TEXT-001, STREAM-001,
IDENTITY-001, IDENTITY-002, PROMPTS-001, LIMITS-001, CHAPTER-001, ACCEPTANCE-001.
New checks register actual tests under each applicable contract. BACKENDS-001 stays
partial: composed synthetic-child acceptance is not both-vendor live acceptance.
All earlier regressions, Phase 0 fixture expectations and eleven existing mutations
remain intact.

Complete-only Codex offers no pre-terminal narrative preview. The post-candidate
cancellation case here cancels from the completed payload's callback, before domain
commit, after process cleanup. It is distinct from idle forceful cancellation and
from the prior step's timeout-after-unemitted-candidate test. Incremental preview/
final disagreement is not supported by this profile and is not falsely claimed.

No authenticated call was needed or made for this offline slice. Step 4's single
live outline remains separate evidence. Full cast/turn/live cancellation acceptance
and Claude's independent implementation remain pending. Next is step 6: retain
focused codec/schema/evidence mutations in the shared gate, then step 7's full live
adapter harness and peer handoff. No headless UI or persistence is added here.

## Final verification

`cargo fmt --all --check`, warnings-denied offline workspace Clippy, and
`bash scripts/check_contracts.sh` passed: **239 Rust tests including doctests**,
architecture/registry/coverage checks, and **all eleven retained behavioral
mutations**. See [contracts.log](evidence/contracts.log),
[clippy.log](evidence/clippy.log) and [fmt.log](evidence/fmt.log).
`git diff --check` also passed. Live status remains unchanged.
