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

**Partial: current constructors checked, future DTO/use-case mappings pending.**
Prefer domain newtypes over interchangeable primitives; aliases are semantic only.
Use checked construction for constraints and enums/typestate for legal alternatives.
Do not assume valid fields imply a valid aggregate. Keep fields private when
unchecked mutation could break invariants. Use named CharacterDetailsFields input;
only CharacterDetails is validated. Option means legitimate absence, not a hidden
mandatory empty string. Public accessors expose domain operations without requiring
callers to know tuple storage. String declaration macros must name their purpose.

Required tests include invalid constructors, independently typed values, selection
ownership, and nonblank deltas; compile-fail tests complement runtime regressions.
Future vendor/save DTOs must preserve tolerant wire defaults and null-vs-empty
semantics while mapping through those constructors. Test boundary conversion before
calling it complete. Evidence: architecture/domain decisions in PLAN, `6290701`.

### TOOLING-001 No Python tooling dependency

**Partial: current checks use Bash/Rust; future additions require review.** Small
checks use shell; larger tools use Rust. Copied Python is source reference only;
tests must not execute it to establish project-specific expectations. CI and local
verification use the same contract gate. Adding a Python interpreter or package as
a build/test dependency violates this decision even if the source uses it.

## Required future behavior — not implemented or claimed tested

### PROMPTS-001 Opening-turn identity review

**Pending.** Exact deduplication stays local. Add the prepared
`reference/prompt-additions.toml` identity check to the existing opening-turn call,
not a separate paid LLM call. Preserve namesakes and established IDs. The current
SummaryUpdate cannot delete/consolidate existing characters; prose must not act as
a hidden merge command. Acceptance: rendered opening prompt contains the addition,
subsequent prompts do not repeat it, and a scripted generator observes one call.
Any real semantic consolidation needs an explicitly approved schema/domain change.
Evidence: `7a98aca` and the user's cast-construction decision.

### BACKENDS-001 Two co-equal subscription CLI backends

**Pending.** Claude and Codex are peers behind inward-owned ports, not primary and
fallback. Use stateless calls and subscription auth; never silently switch to paid
API auth. Test the shared contract against scripted and both concrete adapters.
Each backend needs its own actual live evidence before "verified" or v1 completion;
the other backend remains explicitly unverified when auth is unavailable. Optional
list-price estimates are not subscription charges. CLI flag facts come from each
reference file's evidence, not inference from the other vendor. No fabricated live
tests and no credentials/network required by this contract gate.

### PROMPTS-002 External templates and native structured output

**Pending.** TOML overrides merge per key. Strict undefined-variable handling and
startup rendering checks are required. Port semantic instructions and Markdown
prose rules; remove JSON-format directives that conflict with native schemas.
Keep narrative first in schema/request order. Acceptance: template snapshots,
override/default merge tests, schema-description coverage, field-order tests, and
malformed template failure before generation. "Verbatim port" never overrides these
explicit adaptations. Both backends must satisfy the shared schema contract.

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
