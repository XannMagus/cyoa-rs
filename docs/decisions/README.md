# Project contracts that override source parity

These are normative project decisions. Calibre is a reference implementation,
not the authority for reversing them. Read this file after PLAN.md and before
porting or changing code. The 2026-09-23 review demonstrated why tests against
Python alone cannot establish correctness for this project.

## Precedence and change procedure

1. Explicit user decisions take precedence. Record new decisions here and in PLAN.
2. These contracts and their required regressions override conflicting Calibre code,
   source notes, old plan sketches, and historical review recommendations.
3. Preserve Calibre behavior where no deliberate deviation is recorded.

Do not delete, ignore, weaken, or change a contract's expected results just to make
a port pass. A deliberate behavior change requires an explicit user decision,
its rationale, and corresponding contract/test changes in the same commit. A
behavior-preserving refactor may move or rename tests if the registry is updated
and the assertions still establish the same behavior. No model-specific rules:
every contributor follows the same contract.

`required-tests.json` maps decision IDs to actual Rust test names. Run
`bash scripts/check_contracts.sh`: it rejects missing/ignored registered tests,
runs all workspace tests (including compile-fail examples), and checks dependency
boundaries. CI runs the same command. Tests use project expectations, never a
Python implementation as the oracle for a deliberate deviation.

Coverage states are explicit: **enforced** means the listed implemented behavior
has executable checks; **partial** includes implemented checks and stated remaining
work; **pending** has no implementing feature yet. None of these claims that tests
prove every possible implementation correct. Review remains necessary for edits
to the contracts, tests, or CI itself.

## Implemented contracts

### IDENTITY-001 Namesakes and exact world-record deduplication

**Enforced.** Calibre discards world characters by casefolded name. We deliberately
do not: complete normalized records are deduplicated within each role, keeping
first order; distinct records with the same name survive. An NPC sharing a playable
name survives too. Roles are not merged based on a name. Count the surviving
playables for the minimum; cap NPCs only after exact deduplication.

Required examples: two distinct playable Ajaxes satisfy the minimum of two;
playable/NPC namesakes survive; two NPC Ajaxes reach the initial summary with
different IDs and receive independent subsequent updates. Exact duplicates count
once. See `a6bc079` and the earlier namesake decision in PLAN.

### IDENTITY-002 Stable IDs, suffix repair, and ordering

**Enforced.** Constructing `CharacterCast` first removes complete equal records
(including ID and every detail), then repairs remaining colliding IDs with `-2`,
`-3`, etc. Reserve all supplied IDs before repair so an earlier collision cannot
steal a later supplied ID. Do not reject a distinct character because its ID or
name collides. Preserve insertion order, ID lookup, and order-sensitive equality.
Future updates target repaired IDs independently. This extends Python's behavior;
it does not invent a claim that Python rejected duplicate input IDs.

ID-first/name-fallback update matching remains the existing Python-compatible
rule, including first-update-wins and last matching namesake for fallback. Do not
confuse construction deduplication with update matching or semantic consolidation.
Evidence: `776691d`, PLAN's first domain slice, and the registered merge tests.

### TEXT-001 Normalized business text and verbatim audit text

**Enforced.** Distinct nonblank business-text newtypes trim at construction and
reject blank values. RawResponse, Instructions, and RenderedPrompt instead use
`define_verbatim_string_type!`: preserve every UTF-8 byte, including empty input,
whitespace-only input, CRLF, and Unicode. No blanket String-wrapper normalization.
Raw diagnostics are not proof of a valid StoryTurn. Evidence: `5050fb4`; the
restoration tests also preserve audit records through state reconstruction.

The payload and its transport diagnostics are separate concerns. `TransportDiagnostics`
(`cyoa-application::diagnostics`) is a byte-backed (not `String`-backed) type retaining a
subprocess's captured stdout/stderr separately, because a diagnostic stream may
contain invalid UTF-8 and lossy display must never replace the retained bytes.
`BackendError`'s `Cancelled`/`Unavailable`/`Timeout`/`Generation` variants each
carry it, distinct from `Generation`'s `raw_response` (the actual structured
payload). A real process boundary — `subprocess_fixture`, an ungated `[[bin]]`
of `cyoa-infrastructure` never wired into `cyoa-cli`'s command surface, driven
by the test-only `FixtureBackend` in `tests/support/fixture_backend.rs` — proves
CRLF, non-UTF-8 bytes, and exact quoted/Unicode payloads all survive
unmodified, and that the fixture's reported argv/stdin match exactly what its
caller sent. Evidence: `cyoa-infrastructure/tests/backend_contract.rs`.
`FixtureBackend`'s "no stdout at all" (`Unavailable`) versus "stdout present
but the process still failed" (`Generation`) split on nonzero exit is this
harness's own adapter policy, not yet production-owned code; Phase 1 item 3's
process supervisor and item 4/5's vendor codecs are expected to adopt the same
distinction, not a claim that a real `claude`/`codex` process already does.

### TEXT-002 Unicode lowercase is the matching policy

**Enforced.** For name fallback, event/action deduplication and generated IDs, use
the accepted Unicode-lowercase approximation, not full Python casefold. `Straße`
and `STRASSE` are distinct; `Straße` and `STRAẞE` match. World exact-record
deduplication is equality of normalized fields, not case-insensitive name matching.
This is an intentional, documented port difference, not a Unicode bug to fix from
Python. A future policy change requires a migration/identity assessment.

### STATE-001 Selection belongs to its world

**Enforced.** `PlayablePosition` is an unchecked request. `World::select` validates
against the actual world and returns an owning SelectedWorld. Both start and
restore require that aggregate. Its world and selection cannot be replaced
independently or mutated through public fields. No transferable "checked index"
accepted by arbitrary casts. Runtime boundary tests and compile-fail examples
protect this stronger Rust construction guarantee. Evidence: `652f0b2`.

### LIMITS-001 Typed bounds and explicit restoration policy

**Partial: domain, prompts and orchestration enforced; persistence pending.** Different
bounds have distinct types. Event cap and playable minimum must be positive;
NPC cap and prose bridge allow zero. Zero NPC cap consumes no candidates.

Restore explicitly chooses current config or original settings from game creation.
Rebind every snapshot's event cap, preserving newest events, so rewind and future
commits obey the chosen cap. Empty logs use that cap too. Preserve original settings
across repeated restores. Raising a cap cannot resurrect discarded events. Do not
delete established characters or reject an existing world using generation-only
NPC/playable bounds. The bridge uses active settings. Evidence: `b5fe6c0`, `8e1860f`,
`2e9ff93`, and the four restore integration tests.

Before saves ship: persist original settings and validate selected position on load;
test save/load with changed config and repeated rewind. Do not invent original
metadata for old imports. Prompts and schema descriptions now use active limits, tested through application
commands against both restore choices, not global defaults.

### CHAPTER-001 Later turns can retitle a chapter

**Enforced, recorded in PLAN by `5b7e982`.** Unlike Calibre's first-turn-only title,
the last supplied title in a chapter is effective. None keeps the earlier title;
rewinding restores the earlier title. Derive chapter membership from markers;
the first turn cannot create a phantom preceding chapter. Preserve this documented
extension; do not infer permission to revert it from the Python source.

### ARCH-001 Dependencies point inward

**Partial: dependency graph and generation use cases enforced; other use cases pending.** Domain
owns vendor-independent rules. Application owns orchestration and its ports.
Presentation drives application; infrastructure implements inward-owned ports;
main wires concrete adapters. Application must not import concrete infrastructure
or presentation. No vendor/terminal/serialization DTOs inside domain entities.
The Bash architecture check covers workspace normal/dev/build dependencies and
specified outer-layer dependencies. It does not prove absence of filesystem or
process calls through std; those require code review.

Use lightweight CQRS as use cases arrive: commands change state, queries expose
read-only views. No message bus, event sourcing, or duplicate types by convention.
No concrete adapter types in use-case signatures. New use cases need port-based
tests; the original Python module layout and old two-crate plan are not precedent.

### ARCH-002 Domain types establish invariants

**Partial: constructors and wire-boundary DTO mappings checked, use-case
mappings pending.**
Prefer domain newtypes over interchangeable primitives; aliases are semantic only.
Use checked construction for constraints and enums/typestate for legal alternatives.
Do not assume valid fields imply a valid aggregate. Keep fields private when
unchecked mutation could break invariants. Use named CharacterDetailsFields input;
only CharacterDetails is validated. Option means legitimate absence, not a hidden
mandatory empty string. Public accessors expose domain operations without requiring
callers to know tuple storage. String declaration macros must name their purpose.

Required tests include invalid constructors, independently typed values, selection
ownership, and nonblank deltas; compile-fail tests complement runtime regressions.
`cyoa-infrastructure`'s `generation::wire` now maps calibre-shaped request/response
DTOs into these domain types, colocated per struct (the `clocker` `TimeLogEntryDTO`
pattern): blank wire strings map to `None`, `upcoming_events`' null-vs-empty
distinction survives the mapping, incomplete generated characters are dropped
rather than failing the whole cast, and an unrecognized `QuickActionKind` maps to
`Other` rather than erroring. Save DTOs remain future work. Evidence:
architecture/domain decisions in PLAN, `6290701`, and `wire_mapping`'s tests.

Invalid telemetry policy (2026-09-25, Phase 1 item 2): a vendor-reported
cached-token count that exceeds its own reported total is boundary-invalid
input, not evidence the generation itself failed. `normalize_input_tokens`
(`cyoa-infrastructure::backend`) treats it as unknown cache information
(`cached: None`) rather than rejecting an otherwise valid response; direct,
already-validated construction via `InputTokens::new` keeps rejecting the
same invalid pair outright, since that path is for values already known
trustworthy. Evidence:
`normalize_input_tokens_distinguishes_unknown_from_zero_and_repairs_only_invalid_cache`
and `adapter_reported_invalid_cache_accounting_normalizes_to_unknown_over_a_real_process_boundary`.

### PROMPTS-001 Opening-turn identity review

**Enforced: rendering and scripted application call counts.** `reference/prompt-additions.toml`'s `opening_identity_review` is
rendered into the opening turn's user prompt only, never repeated on a later
turn, by `GenerationTemplates::turn_prompt` — the existing opening-turn call,
not a separate paid LLM call. Preserves namesakes and established IDs; the
current `SummaryUpdate` still cannot delete/consolidate existing characters,
and prose must not act as a hidden merge command; this decision only adds the
review instruction, not consolidation itself. Evidence:
`opening_turn_prompt_includes_the_identity_review_addition_exactly_once`
(cyoa-infrastructure).

### PROMPTS-002 External templates and native structured output

**Partial: configuration structure and instance rendering enforced; arbitrary user overrides and both-backend satisfaction pending.**
`GenerationTemplates::bundled()` constructs the only public configuration entry
point. Private source DTOs reject wrong types, missing and unknown keys. A checked
style table has a first entry by construction; blank or duplicate keys, empty
style tables and dangling quick-action references fail construction. Every
compiled template has a per-call context; static variable checks include simple
nested accesses and unexecuted branches. Representative limit contexts include
zero NPCs and a playable minimum above five. Rendering still returns `Result`:
representative checks cannot prove arbitrary data-dependent templates total.

World, cast and turn requests obtain instructions, prompt and schema from the same
instance. Named results use `Instructions` and `RenderedPrompt` instead of an
interchangeable string tuple. No public raw-TOML renderer or default-only bypass
remains. Turn requests use the game's active limits. Static JSON schema structure
comes from wire types; documentation coverage checks both type and field sets,
with explicit outgoing-memory types. All schemas retain their root declaration,
`narrative` first, and forbidden extra properties. Backend quirks remain isolated
under ARCH-003; previously recorded live evidence is not expanded by these tests.

Per-key merging and configured rendering are tested internally. Arbitrary user
configuration is not exposed through an API or XDG loader until PROMPTS-003 is
implemented. Source-config validation is structural, not semantic approval.
Complete, committed request snapshots cover opening, continuation and chapter
bridge states with an overridden action meaning. They have no automatic update
mode. The registered regressions include wrong contexts, nested typos, invalid
source shapes, schema documentation, configured fallback, data-dependent errors,
literal player text, and the same-instance path for all three request kinds.

The 2026-09-25 step-4 TDD record lists observed failing and passing runs.
Application orchestration and absence of extra calls are now checked by scripted
transports (ACCEPTANCE-001). Subprocess behavior, persistence and semantic validation
of arbitrary overrides remain unclaimed.

### PROMPTS-003 Arbitrary overrides require business-invariant validation

**Pending. Explicit user decision, 2026-09-25.** Before authorizing arbitrary
prompt overrides, add a configuration validator ensuring that project invariants
and rules remain respected after any permitted parts are overridden. Validate the
complete effective configuration, including interactions between instructions,
schema descriptions and limits. Reject contradictory or unsupported configurations
with actionable errors before generation. Structural TOML validation, template
compilation, synthetic renders and snapshots alone do not satisfy this decision.

The validator's design and supported override language remain future work; do not
claim general semantic validation of arbitrary prose from keyword matching. Its
acceptance suite must include superficially valid but rule-breaking overrides
(e.g. name-only identity merging, changed stable IDs, disabled later retitling,
contradictory limits, or changed null-versus-empty update semantics), both alone
and combined across files, alongside permitted customizations. Application and
CLI loading must not offer an unchecked path around it. Until then, public
construction uses bundled defaults; internal fixtures may exercise configuration
mechanics without exposing that capability to users.

### ARCH-003 Backend-specific tolerance lives in that backend's own adapter file

**Enforced.** Code shared across backends — wire DTOs, JSON Schema
generation, prompt rendering — stays maximal and backend-agnostic: the
fullest, most standards-compliant representation available, never trimmed
for one backend's convenience. A backend's own tolerance limit (a flag it
rejects, a key it can't parse, a shape it needs different) is discovered only
against a real, authenticated call to that specific backend (PLAN.md's
"Backend parity and cross-agent handoff") and fixed in its own file under
`generation::backend_compat` — e.g. `backend_compat::claude_cli` — which
starts from the shared maximal output and removes only what it must. This is
a **separate file per backend, not a submodule nested inside the shared
`wire.rs`/`schema.rs`/`prompts.rs`**: adding a backend must not require
editing any of those files, only creating a new one plus one `pub mod` line
in `backend_compat/mod.rs` to register it. Do not fold a discovered backend
quirk into the shared generic code path. Do not let one backend's adapter
depend on, duplicate, or edit another's. Before treating a fix as generic,
ask whether building the *other* backend would ever need to touch that same
file to get its own ideal behavior; if yes, the fix is in the wrong place. A
dynamic/plugin-style backend registry (discover and (de)activate adapters at
runtime) was considered and deliberately deferred: with only two backends,
one `pub mod` line per backend is simpler than the complexity is worth;
revisit only if a third backend joins.

Evidence: `backend_compat::claude_cli::adapt_schema` strips `claude -p
--json-schema`'s rejected root `"$schema"` key (verified live,
`01-claude-cli.md`'s "Verified test #3"), in its own file, registered by one
line in `backend_compat/mod.rs`. `generic_schemas_declare_a_root_schema_key`
proves the shared builders keep the key;
`adapt_schema_strips_the_root_schema_key_and_nothing_else` proves the
adapter changes only that one key. This decision predates and generalizes
past that one example: it governs any future `wire.rs`/`prompts.rs`
backend-specific finding too, not only schema generation.

### TOOLING-001 No Python tooling dependency

**Partial: current checks use Bash/Rust; future additions require review.** Small
checks use shell; larger tools use Rust. Copied Python is source reference only;
tests must not execute it to establish project-specific expectations. CI and local
verification use the same contract gate. Adding a Python interpreter or package as
a build/test dependency violates this decision even if the source uses it.

## Backend and product completion obligations

### BACKENDS-001 Two co-equal subscription CLI backends

**Partial: Codex adapter, composed real-child fixture acceptance and bounded live
adapter acceptance implemented; Claude adapter, composed real-child fixture
acceptance and bounded live adapter acceptance implemented; both live headless gates
pending.**
Claude and Codex are peers behind inward-owned ports, not primary and
fallback. Use stateless calls and subscription auth; never silently switch to paid
API auth. Test the shared contract against scripted and both concrete adapters.
Each backend needs its own actual live evidence before "verified" or v1 completion;
the other backend remains explicitly unverified when auth is unavailable. Optional
list-price estimates are not subscription charges. CLI flag facts come from each
reference file's evidence, not inference from the other vendor. No fabricated live
tests and no credentials/network required by this contract gate.

Codex step 4 verifies subscription auth at construction with `login status`, using
the same executable/HOME/CODEX_HOME/PATH as generation. Unknown or conflicting
status fails closed; it never changes login mode. This is a point-in-time check,
not a guarantee against later account changes. Each generate call launches one
generation child through the existing supervisor, with no retry. It reconciles
the complete-only codec against transport completion and checks cancellation before
emission and after the callback. Only then can it return a GenerationResponse.
Real-child tests check failure classification, exact candidate and byte evidence,
resource cleanup, request argv/stdin/schema and no premature emission. One bundled
outline was exercised live on 0.157.1; this is not the full story acceptance gate.

Step 5 adds `codex_story_acceptance`: actual adapter → GenerationEngine →
StoryUseCases, edited outline, namesake cast/ID repair, opening/continuation,
chapter breaks and retitling, rewind, both restored-limit policies and zero NPCs.
Captured child requests and launch counts check context and no extra generation.
Nonzero exit after a candidate, terminal failure, invalid payload/domain data,
missing terminal, idle/callback cancellation, output cap and actual workspace
cleanup failure leave the entire game unchanged. Explicit retry adds exactly one
child and receives the same prompt. Fixture payloads are synthetic; this provides
offline enforcement across contracts, not a new live-fiction claim. Complete-only
candidate callback cancellation is exercised; incremental preview disagreement
is outside this supported profile. Existing Phase 0 tests remain unchanged.

Claude adapter step 3 (2026-10-02) adds `backends::claude_cli` preparation only:
the fixed `claude -p` profile frozen in `reference/01-claude-cli.md`, an explicit
HOME/PATH (and optional `CLAUDE_CONFIG_DIR`) environment, the prompt on stdin, an
empty request workspace, no `--bare` and no `--model` unless configured. The
advisor-suppression instruction is appended to this invocation only; shared
templates never contain it (ARCH-003). A single argument of 131072 bytes or more, or
containing NUL, fails preparation before any child exists. There is still no Claude
codec, `Backend` or live adapter claim; those follow in later steps.

Claude adapter step 5 (2026-10-02) adds `ClaudeCliBackend`. Construction runs
`auth status --json` through the same executable and environment and accepts only
`loggedIn` + `authMethod: claude.ai` + `apiProvider: firstParty`; anything unknown,
conflicting or a nonzero exit fails closed as `Unavailable` and launches no
generation child. Each `generate` call launches one child through the existing
supervisor with no retry. Correlated preview fragments are forwarded as they arrive
(tentative; the final `structured_output` wins when it disagrees); a stream with no
previews emits the complete payload once after reconciliation. A result candidate
cannot override a nonzero exit, incomplete delivery, observed cancellation, deadline,
output cap, protocol or cleanup failure, and the candidate and transport bytes
survive each failure up to the application boundary. Nineteen real-child tests
(`tests/claude_backend.rs`) check argv/stdin/env, classification, cleanup and
launch counts; the fixture binary gained a Claude argv mode without changing Codex's.
These tests were written after the adapter, not red-first, and are protected by the
persistent mutations of a later step. Fixture payloads are synthetic: offline evidence,
not authenticated behavior. The transport-error mapping is duplicated from the Codex
adapter, deliberately, so this slice does not edit the peer's file.

Claude adapter step 6 (2026-10-02) adds `claude_story_acceptance`: the actual
adapter → `GenerationEngine` → `StoryUseCases` over fixture children, reusing the
Codex acceptance scenario (`fixtures/codex/story.json`, a backend-agnostic synthetic
story) with Claude-shaped streams. It covers the edited outline, namesake cast and ID
repair, opening and continuation, chapter break and retitling, rewind, both restore
policies, zero NPCs, exactly one opening identity instruction and no extra review
call, and per-request argv/stdin capture. Late failures (nonzero exit after a result,
`is_error`, truncated and missing results, domain-invalid payload), idle cancellation,
cancellation from a live preview and from a complete-only emission, an output cap and
a real workspace-cleanup failure leave the whole game unchanged; an explicit retry
launches exactly one new child with the same prompt. Claude-specific: previews arrive
incrementally and may precede a failure (never committed), a disagreeing preview
yields to the final payload, a text block in an enforce-retry stream is never
narrative, and provenance carries only what the stream reported. Fixture payloads
are synthetic: offline enforcement, not live fiction.

Claude adapter step 8 (2026-10-02) is the bounded live adapter gate on 2.1.286:
bundled outline, cast, opening and continuation through the real adapter and use
cases, plus one controlled cancellation, with the shipped invocation (no `--model`).
All four generations were accepted with one result each, empty stderr, a payload
byte-equal to the CLI's compact `structured_output` and previews equal to it as JSON;
the opening took the CLI's enforce-retry route (two messages) and committed only the
structured payload; no advisor content block appeared; the cancelled call left the
game unchanged with the direct child PID and workspace absent afterwards. Evidence:
`reviews/2026-10-02-claude-step8/`. This is not headless acceptance, persistence or
presentation, covers one account/org and one run, and says nothing about `--model`,
rate limits, descendants or server-side work after cancellation. The harness's first
advisor counter over-reported (substring match on slash-command listings); the
structural recount is in the evidence. Both live headless gates remain pending.

Claude review repair 1 (2026-10-02): the codec extracts the exact `structured_output`
span before validating the result's terminal metadata, so a result whose `is_error`
or `subtype` is missing, mistyped or blank is still rejected (`InvalidField`) but now
hands its candidate to the application instead of dropping it; transport bytes were
already retained. A thirtieth mutation (`drop-candidate-on-malformed-metadata`)
protects it at the real-child boundary. See
`reviews/2026-10-02-claude-review-repairs/README.md`.

Claude review repair 2 (2026-10-02): two equal keys in an object that carries control
fields make a stream-json record ambiguous, and the codec now rejects it
(`DuplicateField`) before interpreting any field. `serde_json::Value` silently keeps
the last duplicate, so `"is_error":true,"is_error":false` had been accepted as
success (and a duplicated `apiKeySource` could have hidden a metered key behind a
trailing `"none"`). Checked objects are the record, `event`, and the event's
`content_block` and `delta`; model-authored content (assistant messages, tool inputs)
is not control data and stays tolerated, and `structured_output` keeps its own
`InvalidPayload` rule. An unambiguous payload span is retained as evidence when only
a control field is duplicated. A thirty-first mutation (`accept-last-duplicate-field`)
protects it at the real-child boundary.

### PRODUCT-001 Standalone TUI, persistence, export, and image boundary

**Partial: terminal-independent controller/worker and Linux headless/demo enforced;
TUI, persistence and export pending.** Rust TUI and headless demo replace the Qt/calibre host. Keep vendor and
filesystem work in adapters, one generation in flight, UI-owned canonical state,
worker progress messages, cancellable children, cached narrative wrapping. Do not
hold a state mutex during a model call. Save atomically with backup and versioned
migrations; Markdown/EPUB export are in v1, images are behind a disabled port in v1.
Acceptance: scripted lifecycle/cancellation/state-unchanged-on-error tests, frozen
save migrations, original-limit round trips, synthetic UI-event/cache tests, and
export fixtures. PLAN retains the full build order and acceptance scenarios.

Controller/worker item 6 is implemented (2026-10-02) through presentation session
and real-thread tests, the unchanged Phase 0 fixture through the full runtime,
and actual Claude/Codex adapters using synthetic real-child streams in CLI tests.
Three presentation mutations protect stale acceptance, cancelled success and a
second commit of a returned turn (34 total). This establishes no playable command,
stdin/signal shutdown, saves, rendering cache or exports. Evidence:
`reviews/2026-10-02-controller-worker/README.md`.

Headless item 7 adds shipped `play --headless --backend claude|codex` and
`play --headless --demo`, manual outline replacement, checked selection, one
opening call, explicit retry, action numbering, literal slash input and read-only
canonical inspection. Memory-only status is explicit. Linux readiness polling
and SIGINT flags avoid a blocked input thread; EOF, quit, invalid input and output
errors close/join workers. Nonblocking output treats pipe backpressure as an I/O
failure. Previews have tentative/discarded/final boundaries; divergent final text
replaces them and matching text is not duplicated. Thirteen binary tests exercise
demo and both actual adapters over synthetic children, not vendor auth. Item 8
retains every existing mutation and adds bounded idle-SIGINT coverage (35 total).
Live headless evidence remains separate from these offline checks.

## Review discipline

Every implementation report names affected decision IDs and runs the gate. If a
pending feature is implemented, add its regressions to the registry and change its
coverage state in the same commit. Do not satisfy a requirement with an ignored
test, a test that merely reads a comment, or a Python-derived expected value.
Historical review probes are evidence of old defects, not current acceptance tests.

Changing tests and CI together can bypass any repository-local guard. Human review
and making the CI job a required branch check are the final controls; this change
does not configure remote branch protection or invent a reviewer identity.

Generation-boundary follow-up (2026-09-24): ARCH-002 now also requires
`outline_to_selected_world_requires_no_placeholder_cast`. Cast prompts consume
`WorldOutline`, before any cast exists. The boundary regression deserializes real
JSON shapes, preserves distinct namesakes, removes exact records, selects the
actual protagonist, and checks repaired IDs reach the opening prompt. This does
not yet establish application orchestration or backend call counts.

LIMITS-001 generation request policy: playable requests range from
`max(3, min_playable_characters)` to `max(5, min_playable_characters)`; NPC requests
range from `min(3, max_generated_npcs)` to `max_generated_npcs`, with zero stated
explicitly as no NPCs/an empty list. These preferences do not strengthen domain
validation (two usable playables remain acceptable by default). Prompt and schema
rendering share the calculation. Boundary tests cover caps 0–3 and 8, minimums
1–4 and 6, and the maximum representable minimum without arithmetic overflow.
Restored-limit orchestration is enforced by the registered application test;
persistence remains pending.

CHAPTER-001 also covers loaded prompt/schema instructions and JSON turn mapping
through commit and rewind. Continuing turns can supply a new title; null preserves
it. These loaded texts deliberately differ from the unchanged Calibre reference.

ARCH-002 wire requiredness: generated NPC `relationships` and turn
`scene_description` must be present strings. Missing and null are rejected;
explicit empty strings remain valid and map to absent optional domain text.
Schemas must require both fields without imposing nonblank validation. This
preserves the source boundary, not a backend tolerance workaround. Regressions
also protect genuinely defaulted delta relationships/action kinds, nullable chapter
titles, and omitted/null versus empty-list upcoming-event updates.

Step 5 (2026-09-25): application-owned `StoryGenerator`, typed `TurnDirection`,
`Generated<T>` and `GenerationFailure` isolate the domain-facing contract from
JSON/template/backend types. `StoryUseCases` rejects pre-cancelled commands and
commits only validated successful turns after checking cancellation again. A worker
may operate on an owned snapshot; presentation must retain canonical state and
never hold its lock during generation. Concurrency/stale-worker handling remains
presentation work, not a guarantee provided by this synchronous use case.

The infrastructure `GenerationEngine` performs one transport call, decodes wire
DTOs, and applies domain constructors. `ScriptedBackend` exercises that same path
without credentials and records complete requests. Required tests observe no
retry, state equality on error, exact diagnostic bytes, no pre-cancel call, edited
outline use, namesake preservation, repaired IDs in later prompts, exactly one
opening identity instruction and no separate deduplication call. Active current
and original restore policies reach prompts, schema descriptions and merged event
caps. Persistence and live subprocess adapters remain pending.

### STREAM-001 Preview text never authorizes a state commit

Controller slice (2026-10-02): presentation retains canonical state, tracks typed
request IDs/base revisions, and rejects stale, duplicate and cancelled terminal
results. `tests/session.rs` checks full state preservation and separate failure
evidence, lifecycle selection and snapshot replacement without a second commit.
Worker execution and headless I/O are separate subsequent evidence.

The owned runner/runtime now recover use cases only after thread join, retain
cancellation authority for Drop cleanup, and fault explicitly on worker panic.
Real thread tests cover silent cancellation, cancelled queued success, retry,
spawn failure and bounded progress without consumer backpressure. Presentation
preview overflow is explicitly marked incomplete; it does not invalidate an
otherwise successful generation. Headless input/signal shutdown is implemented
on Linux and checked at the binary boundary; other hosts remain unsupported by
this terminal adapter.

**Partial: scanner, scripted generation, vendor-neutral process supervision,
Codex protocol decoding and Backend reconciliation, and offline Claude event
decoding and Backend reconciliation, enforced, with a bounded live adapter run for
each backend; controller/worker and Linux headless I/O acceptance enforced.** `StreamingStringField` extracts only the requested root
string, across valid UTF-8 chunks. It is a preview scanner, not JSON validation.
Escapes and surrogate pairs are decoded; lone surrogate halves become U+FFFD
because Rust cannot represent them as scalar values. Truncated escape sequences
remain incomplete rather than inventing final text. Leading fences can be skipped
for preview without making an invalid final response acceptable.

The generation adapter forwards decoded narrative previews through the inward port,
then independently decodes and validates the authoritative final JSON response.
On failure/cancellation, preview text does not enter state; diagnostics retain the
available raw text. Complete-only transports emit the narrative once, and streamed
prefixes are not duplicated on success. Presentation must replace transient preview
with the authoritative committed turn (including normalization); the preview is not
an audit record or authoritative prose.

A seeded ChunkedBackend replays scalar-sized fragments through the same adapter.
Required tests compare every two-part scalar split and deterministic multi-part
splits with independently known narrative text, cover escapes/nesting/surrogates,
and compare complete versus chunked committed state across 32 seeds. Failure after
preview and cancellation during preview leave complete state unchanged. This does
not verify CLI byte decoding, process killing/reaping, or vendor stream events;
each concrete backend must gain those tests and live evidence in Phase 1.

Phase 1 item 3 (`cyoa-infrastructure/src/backends/process.rs`, Linux-tested)
closes the "real subprocess framing and cancellation" gap above at the
vendor-neutral transport layer: a real `Command`-launched child, an explicit
argv/environment (never ambient inheritance), stdout split into opaque byte
records via an owning offset framer, non-blocking stdin/stdout/stderr
polled alongside a self-pipe cancellation wake so an idle child cannot hang
the observation of `cancel()`, checked/finite output-byte and deadline
bounds, and process-group `SIGKILL` cleanup on every exit path (including
ordinary success) so a descendant that inherited the pipes cannot keep them
open, including via a `ChildGuard` whose `Drop` kills-and-reaps as a backstop
for any exit path (a panic unwinding out of the supervisor's consumer
callback, or a future early return this module forgets to route through
cleanup) that never reaches its own normal-path cleanup.
`CancellationToken::subscribe` (`cyoa-application/src/cancellation.rs`)
gained a race-free wake-notifier registration: a notifier registered after
`cancel()` already fired still runs immediately, closing the exact
lost-wakeup case this design exists to prevent.
`GenerationResponse::with_diagnostics`/`diagnostics()`
(`cyoa-infrastructure/src/backend.rs`) let a successful transport call carry
its captured stdout/stderr forward; `GenerationEngine`'s two post-success
cancellation checks (the shared `generate()` tail and `turn()`'s own
post-decode check) now attach those real bytes instead of an
always-empty placeholder. This module does not implement `Backend` and does
not interpret Claude/Codex event shapes — that remains item 4/5's job, built
on top of this supervisor. Windows is explicitly unsupported: `run` returns
`SupervisorError::Unsupported` there without attempting anything
platform-specific.

Codex adapter step 3 (2026-09-27) adds a private codec for the frozen 0.157.1
profile. It requires one eligible message and terminal completion in order,
rejects unsupported/ambiguous/malformed records, and preserves candidate bytes
separately from its located error. Errors latch; later events cannot repair them.
Payload syntax is checked at end-of-stream without trimming or reserialization.
Malformed/missing telemetry counts independently become unknown; invalid cached
counts use the existing normalizer. Tests replay seven earlier live captures and
synthetic error/edge transcripts; this is offline evidence, not new live acceptance.
The codec emits no preview or GenerationResponse and does not observe child exit,
cleanup or cancellation. Step 4 now supplies those checks in its parent adapter.
Cancelled/Timeout errors carry an optional candidate separately from diagnostics;
the generation engine uses it before falling back to streamed fragments. Structured
transport causes preserve nested cleanup/initiating failures and prefix metadata;
application messages summarize the causes without embedding the diagnostic buffers.

Claude adapter step 4 (2026-10-02) adds a private codec for the frozen 2.1.286
`stream-json` profile. The payload is `result.structured_output` taken as its exact
span (a duplicate key makes it invalid, and it is never re-serialized); previews are
only `input_json_delta` fragments of the first `StructuredOutput` block, correlated
by (message ordinal, block index) because indices restart per assistant message (a
live capture has a second message after the CLI's enforce prompt). Text, thinking,
advisor and other blocks never become preview. Exactly one `result`, last; `is_error`
wins over `subtype: success`; a non-subscription `apiKeySource` is a backstop
failure. Usage totals sum the three input counts and are unknown unless all are
present; cost is a list-price estimate only when every `modelUsage` entry says so.
Tests replay this profile's live captures, three 2026-09-25 captures and 29 synthetic
variants against independently written expectations: offline evidence, not live
acceptance. The codec observes no exit, cleanup or cancellation; the adapter step does.

### ACCEPTANCE-001 — exercise the complete engine before adding external I/O

**Enforced for the scripted Phase 0 engine.** The frozen
`cyoa-infrastructure/tests/fixtures/phase0_story.json` expresses project decisions,
independently of the copied Python. Its acceptance target composes application
commands, validated templates, wire mapping, streaming and domain commits. Five
successful generations surround a malformed response, explicit retry, cancellation
and rewind; exactly nine transport requests are made including world and cast.
Rewind is an application command delegating to the checked domain operation and
makes no inference call. Error/edge tests also cover invalid rewind and returning
all the way to opening context. Never replace this scenario with direct merge tests.

The scenario protects namesakes and repaired IDs, unknown action kinds, retitling,
chapter bridges, event truncation, null/empty threads, raw diagnostics, unchanged
state on failure and exact next-request context after rewind. This is evidence of
scripted orchestration, not live backend behavior, persistence, prompt-override
semantics or presentation cancellation. Those obligations keep their own statuses.

Step 8 (2026-09-25): the shared contract gate now checks coverage declarations
against the registry and runs eleven isolated behavioral mutations (see
`scripts/mutations/manifest.json`). Each mutation names its owning decision and
an exact registered test. A passing baseline is required; only that test's actual
runtime failure counts as detection. Compiler failures, skipped/missing/different
tests, stale patches and surviving mutants fail the gate. `test_mutation_outcome.sh`
exercises the outcome checker with error, edge and nominal reports. IDENTITY-001
now registers the real generation lifecycle as well as domain tests. ACCEPTANCE-001
provides the composed story requirement. Domain, request, orchestration and live
coverage are distinguished in `phase0-acceptance.md`; subprocess and persistence
obligations remain open. These checks cannot stop an intentional rewrite of both
contracts and assertions, so contract changes still require review.

Codex adapter step 6 (2026-09-28) adds seven mutations, for eighteen total:
ineligible messages, missing completion, conflicting terminals (BACKENDS-001),
shared and peer schema adaptation leakage (ARCH-003), and candidate and diagnostic
loss on application-visible timeout (BACKENDS-001). Each new mutant requires its
specific assertion message in addition to the exact test and exit status, so a
fixture/setup panic cannot count. Protocol tests check cleanup before rejection.
Optional `CYOA_MUTATION_EVIDENCE_DIR` retains baseline/mutant/restored logs.
These are offline adapter protections, not additional live-backend evidence.

Claude adapter step 7 (2026-10-02) adds eleven mutations, for twenty-nine total,
each bound to an exact registered test and a specific assertion message:
preview forwarding from an uncorrelated block, block indices shared across
messages (STREAM-001); ignoring `is_error`, accepting a stream with no result,
accepting a duplicate result, a result overriding a nonzero exit, accepting
non-subscription auth, and discarding candidate or diagnostic evidence on an
application-visible timeout (BACKENDS-001); re-serializing `structured_output`
(TEXT-001); and leaking the advisor suppression into shared instructions
(ARCH-003). The first run of the last mutant **survived**: its test inspected only
the world request while the patched line builds turn instructions, so the unit test
now also covers the cast request and the composed acceptance test checks every
request the child receives. All existing mutations remain; the patch for the Claude
schema adapter still applies. Offline protections, not live-backend evidence.

Review repair R4 (2026-09-25): transport diagnostics belong to
`cyoa-application::diagnostics`, not domain text. Invalid outline/cast/turn responses
and cancellation after a typed generation retain those bytes through the application
boundary. Observed provider/model provenance is propagated, never invented. Required
regressions exercise errors, application cancellation and successful commit.

Review repairs R2/R3: zero child exit cannot authorize transport success when request
bytes are known to be undelivered. The earlier broken-stdin success test encoded an
incorrect policy and is replaced with a failure assertion, per the authorized review
repair. Cancellation state is authoritative; wake notifications are only an
optimization and delayed callbacks cannot override an already-cancelled token.

### Supervisor repair decisions (2026-09-25)

STREAM-001/TEXT-001 require finite work per readiness iteration and capped capture
on every exit path. Diagnostics retain exact available bytes up to each configured
bound, with explicit `Complete` versus `Prefix` metadata; a prefix never invents a
missing-byte count. Group shutdown precedes final capture. Observing exit must not
reap/release the child's PID before signaling its group. Success requires verified
reaping, complete input delivery and no fatal I/O/cleanup failure. Initiating and
cleanup failures remain independently inspectable. The guard's best-effort Drop
is only an unwinding backstop. Resource-owning capture/framer/supervisor methods
implement these transitions; callers cannot bypass them through mutable buffers.
See the [repair evidence](../../reviews/2026-09-25-supervisor-repairs/README.md).

Cancellation wake registrations own their callback lifetime. Request-scoped
subscriptions are removed on drop, releasing their pipe descriptors without
waiting for source destruction. The flag is the source of truth. A callback panic
does not skip later callbacks: notifications run outside the lock and the first
panic resumes after all callbacks are attempted. Callbacks must finish promptly;
deregistration cannot undo a callback already taken by concurrent cancellation.

Each process request owns a unique scratch directory (`RequestWorkspace`, Unix
mode 0700); the supervisor consumes its `ProcessSpec`, uses that cwd without
changing global cwd, keeps prepared files through child cleanup and removes them
on completion/unwinding. Normal directory-cleanup errors are observable. Tests
must confirm actual PID disappearance, including timeout, through kernel liveness
checks; absence of `/proc` is not evidence of cleanup. The exercised platform is
Linux; other waitid-capable Unix builds remain unverified, and platforms without
that primitive return `Unsupported`.

The process gate also retains seven STREAM-001 mutations alongside the four
Phase 0 mutations. The wake mutation disables registration while holding the pipe
writer open, so neither a byte nor EOF can wake the poll; an isolated long-poll
test distinguishes the wake from periodic token checks. Production correctness
checks must never be removed simply to make a redundant mechanism testable.
