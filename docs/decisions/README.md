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
(`cyoa-core::text`) is a byte-backed (not `String`-backed) type retaining a
subprocess's captured stdout/stderr separately, because a diagnostic stream may
contain invalid UTF-8 and lossy display must never replace the retained bytes.
`BackendError`'s `Cancelled`/`Unavailable`/`Timeout`/`Generation` variants each
carry it, distinct from `Generation`'s `raw_response` (the actual structured
payload). A real process boundary (the `subprocess_fixture` test binary in
`cyoa-infrastructure`) proves CRLF, non-UTF-8 bytes, and exact quoted/Unicode
payloads all survive unmodified. Evidence: `cyoa-infrastructure/tests/backend_contract.rs`.

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

## Required future behavior — not implemented or claimed tested

### BACKENDS-001 Two co-equal subscription CLI backends

**Pending.** Claude and Codex are peers behind inward-owned ports, not primary and
fallback. Use stateless calls and subscription auth; never silently switch to paid
API auth. Test the shared contract against scripted and both concrete adapters.
Each backend needs its own actual live evidence before "verified" or v1 completion;
the other backend remains explicitly unverified when auth is unavailable. Optional
list-price estimates are not subscription charges. CLI flag facts come from each
reference file's evidence, not inference from the other vendor. No fabricated live
tests and no credentials/network required by this contract gate.

### PRODUCT-001 Standalone TUI, persistence, export, and image boundary

**Pending.** Rust TUI and headless demo replace the Qt/calibre host. Keep vendor and
filesystem work in adapters, one generation in flight, UI-owned canonical state,
worker progress messages, cancellable children, cached narrative wrapping. Do not
hold a state mutex during a model call. Save atomically with backup and versioned
migrations; Markdown/EPUB export are in v1, images are behind a disabled port in v1.
Acceptance: scripted lifecycle/cancellation/state-unchanged-on-error tests, frozen
save migrations, original-limit round trips, synthetic UI-event/cache tests, and
export fixtures. PLAN retains the full build order and acceptance scenarios.

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

**Partial: scanner and scripted generation enforced; real subprocess framing and
cancellation pending.** `StreamingStringField` extracts only the requested root
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
against the registry and runs four isolated behavioral mutations (see
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
