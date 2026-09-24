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

**Partial: domain enforced, persistence/prompt integration pending.** Different
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
metadata for old imports. Before prompts ship: use GameState's active limits in
rendered prompts and test against both restore choices, not global defaults.

### CHAPTER-001 Later turns can retitle a chapter

**Enforced, recorded in PLAN by `5b7e982`.** Unlike Calibre's first-turn-only title,
the last supplied title in a chapter is effective. None keeps the earlier title;
rewinding restores the earlier title. Derive chapter membership from markers;
the first turn cannot create a phantom preceding chapter. Preserve this documented
extension; do not infer permission to revert it from the Python source.

### ARCH-001 Dependencies point inward

**Partial: dependency graph enforced, use-case implementation pending.** Domain
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

**Enforced.** `reference/prompt-additions.toml`'s `opening_identity_review` is
rendered into the opening turn's user prompt only, never repeated on a later
turn, by `generation::prompts::turn_prompt` — the existing opening-turn call,
not a separate paid LLM call. Preserves namesakes and established IDs; the
current `SummaryUpdate` still cannot delete/consolidate existing characters,
and prose must not act as a hidden merge command; this decision only adds the
review instruction, not consolidation itself. Evidence:
`opening_turn_prompt_includes_the_identity_review_addition_exactly_once`
(cyoa-infrastructure).

### PROMPTS-002 External templates and native structured output

**Partial: templating and schema-description rules enforced; both-backend
satisfaction pending.** TOML overrides merge per key, not per file
(`toml_override_merges_per_key_not_per_file`). `UndefinedBehavior::Strict`
turns a typo'd override variable into a render error, checked by a startup
self-check function (`undefined_template_variable_is_a_render_error`). The
loaded prompts contain no JSON-format directive
(`loaded_instructions_contain_no_json_formatting_directive`) and explicitly
ask the model to emit fields in schema order
(`loaded_turn_instructions_ask_for_schema_field_order`) — a line PLAN.md
calls for that does not exist in `reference/prompts.toml` and had to be
authored. Schema descriptions are owned by `schema_docs.toml` and injected
onto `#[derive(JsonSchema)]` structure, checked field-for-field in both
directions by `schema_docs_cover_every_field_and_no_others`; `narrative`
stays the first schema property; every generated object schema forbids
additional properties. Verified live (2026-09-24) against `claude -p
--json-schema`: `$defs`/`$ref` resolve correctly, including the model
following an instruction stated only in a `$ref`'d field's description
(`01-claude-cli.md`'s "Verified test #3"). That call also found `claude -p`
rejects a root `"$schema"` key outright; see `ARCH-003` for why that fix is
scoped to a `claude_cli`-specific adapter file rather than the shared
builders. Evidence: cyoa-infrastructure's `prompt_rendering` and `lib` tests.
Remaining before this can be "Enforced": a `codex_cli` adapter, discovered
the same way against a real authenticated `codex exec` call, and
confirmation both backends satisfy the shared contract once each exists.

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
Restored-limit orchestration and persistence remain pending.

CHAPTER-001 also covers loaded prompt/schema instructions and JSON turn mapping
through commit and rewind. Continuing turns can supply a new title; null preserves
it. These loaded texts deliberately differ from the unchanged Calibre reference.
