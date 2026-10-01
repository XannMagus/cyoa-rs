# Next slice: a Codex CLI adapter with independently verified completion

Status: **steps 1–7 complete**, including the 2026-10-02 live adapter gate on
Codex 0.159.3. [Evidence](../../reviews/2026-10-02-codex-step7/README.md).
Next: [Claude adapter handoff](phase1-claude-adapter-handoff.md), then presentation.
Planning baseline: `8f8d253`, 2026-09-25; step 1 starts from `4ac538d` with no
subsequent changes and records CLI 0.157.1 on 2026-09-26 in
[the discovery evidence](../../reviews/2026-09-26-codex-profile/README.md).
This expands item 4 of [Phase 1](phase1-headless-backends.md); it does not replace
that plan or the [project contracts](../decisions/README.md). The preceding
[repair trace](../../reviews/2026-09-25-supervisor-repairs/README.md) is mandatory
context: 191 tests and eleven mutations protect the repaired starting point.

## Outcome and boundaries

Deliver `CodexCliBackend: Backend`, usable by the existing `GenerationEngine` and
`StoryUseCases`, for outline, cast, opening and continuation requests. A successful
call must have an authoritative protocol payload, an acceptable terminal event,
successful process completion, complete request delivery, no observed cancellation
or I/O failure, and successful cleanup. Domain validation still happens afterward.

Codex goes first because this is a Codex development environment, not because it
is the preferred product backend. Authentication must be rechecked before live
work. Claude remains a co-equal next implementation, with its own protocol and
live acceptance. This slice ends before presentation workers, headless gameplay,
TUI, persistence, arbitrary prompt overrides or automatic retry. A small opt-in
Rust verification harness is acceptable; it must use the real adapter/use cases
and must not become a second game engine or a shipped gameplay interface.

## Evidence and decisions to settle first

The [Codex reference](../../reference/02-codex-cli.md) records one successful cast
from CLI 0.155.1, with a temporary schema transformation making object properties
required. That call returned one complete agent-message JSON string followed by a
terminal event. It did not establish long-narrative streaming, all three schemas,
full account/tool isolation or how to select a final message among several.

Planning checked local `codex --version` and `codex exec --help`: the installed
version remains 0.155.1. Help still describes stdin input, JSONL output, a schema
file and the isolation flags recorded in the reference. These are help observations,
not authenticated generation evidence. No model request was made for this plan.

The [official non-interactive documentation](https://learn.chatgpt.com/docs/non-interactive-mode)
was consulted on 2026-09-25. It documents JSONL events, terminal outcomes, a final
schema-constrained response and saved CLI authentication. It does not establish
which message is authoritative in every transcript our installed version can
produce. Repository transcripts and new bounded live probes must settle that.
Do not copy API-key CI examples into this subscription-backed desktop application.

| Question | Probe/evidence | Consequence |
|---|---|---|
| How are shared instructions and user text delivered? | Real bundled requests; capture argv and exact stdin separately; verify a documented instruction channel or an explicitly framed combined stdin envelope | Never drop instructions, interpolate player text as template code, or move the large user prompt to argv |
| Which agent message is final? | Longer turn with any commentary/items, followed through terminal completion | Use a documented/observed discriminator; ambiguous candidates fail instead of choosing the first parseable JSON |
| Is genuine narrative streaming available? | Observe a longer opening/continuation before final completion | Start with complete-only delivery if partial structured output is not established; never simulate streaming |
| Which schema transformations are necessary? | Current bundled outline/cast/turn schemas, nested nullable fields and zero-NPC case | Codex-local transformation only; preserve shared wire meaning |
| Does isolation preserve subscription auth? | Clean request cwd and explicit environment; inspect harmless instruction/tool canaries and actual event output | An empty cwd or absence of a tool event alone proves neither tool disablement nor global instruction isolation |
| What usage and model metadata are actually available? | Final envelope, including absent/zero counts | Normalize known counts; requested model is not evidence of observed model; no invented cost |

Probes have finite deadlines/output caps and use synthetic story content. Record
version, auth **mode only**, invocation, selected model when explicit, request/schema
artifacts, raw stdout/stderr, exit and elapsed time. Do not copy credential files or
environment values into fixtures. Check auth using the same home/config selection
that the eventual invocation uses. Never change account login mode to make a probe
pass. Missing auth blocks live verification, not independent offline implementation.

Use temporary, labelled adaptations for discovery; commit evidence before production
code depending on it. Do not intentionally exhaust a rate limit. Synthetic failure
fixtures are appropriate when their provenance is explicit. Vendor-internal
reconnects are distinct from our one-generation-child-per-command guarantee.

## Architecture and type boundaries

```text
StoryUseCases -> StoryGenerator / GenerationEngine
                                  |
                               Backend
                                  |
                         CodexCliBackend
                          /            \
                Codex protocol      process::run
                state machine       (vendor blind)
```

All new vendor types stay in infrastructure. Application/domain gain no JSON,
process, filesystem or Codex types. The adapter owns a private protocol state machine
and invocation configuration; the existing supervisor owns process mechanics.
**The adapter reconciles protocol state with the supervisor result.** The supervisor
must not learn `turn.completed`, `agent_message`, Claude block names or vendor usage.

Suggested homes (names may change; responsibilities may not):

| Home | Responsibility |
|---|---|
| `backends/codex_cli.rs` plus private child modules as needed | Adapter, checked invocation configuration, private event DTOs and protocol transitions |
| `generation/backend_compat/codex_cli.rs` | Consuming transformation of this backend's owned schema copy; any supported prompt-envelope adaptation |
| Existing `backend.rs` and application mapping, only if required | Vendor-neutral failure evidence; no Codex event model |
| `tests/fixtures/codex/` | Frozen transcripts with live/redacted/synthetic provenance and independently specified expectations |
| `tests/codex_backend.rs` | Actual adapter + existing fixture child + real supervisor + engine/use cases |
| An opt-in Rust example/test harness | Explicit authenticated verification using bundled requests; never part of the credential-free gate |

Use named executable/model concepts where mixing values is a defect. Invocation
configuration must not be an unrestricted arbitrary-argv escape hatch around
isolation or PROMPTS-003. Resolve an executable before switching the child's cwd;
relative paths containing directory components must not silently change meaning.
Create a `RequestWorkspace`, prepare its schema, and move it in the `ProcessSpec`
consumed by `process::run`. Never add another process runner or call global chdir.

For protocol state, prefer a private runtime enum with required data in each
variant: awaiting output, candidate available, protocol completed with a candidate,
or failed. A candidate is not a successful `GenerationResponse`. Only final
reconciliation may construct success. Use typed item identities if correlation is
needed; avoid independent `has_result`/`is_complete`/`is_error` flags. Resource
methods may mutate their own state; standalone transformations consume/return values.

Proposed protocol policy, to freeze against evidence in the first commit:

- Candidate JSON comes only from an eligible structured agent message. Reasoning,
  tool output, progress text and metadata never enter `on_json` or the final payload.
- Do not concatenate multiple assistant messages. Until evidence establishes a
  stronger final-message discriminator, support an unambiguous single eligible
  message and reject ambiguous candidates explicitly.
- An incomplete/truncated JSONL record, invalid protocol UTF-8, malformed required
  event, missing candidate or missing terminal event cannot become success.
- Duplicate terminal events are rejected, including identical duplicates, for the
  initial supported protocol profile. A future tolerance requires evidence and an
  explicit regression; conflicting success/failure can never use last-event-wins.
- Ignore only identified harmless metadata, including harmless additions to known
  envelopes. Unknown events required to establish completion are protocol failures.
  A generic “unknown means ignore” branch is insufficient.
- A terminal failure invalidates any earlier candidate. Classify top-level error
  notices using the observed profile: do not assume every reconnect diagnostic is
  terminal, or that every error-looking event is harmless.
- Complete-only mode emits one complete payload after successful reconciliation.
  Any future partial path must correlate one structured item and avoid duplicate
  final emission. Final payload remains authoritative if it differs from preview.
- Retain the decoded payload string byte-for-byte. Do not trim or reserialize it.
  JSONL transcript bytes remain separate diagnostics. Claude's later embedded-object
  extraction will need raw-span preservation; do not impose that shape on Codex.

These are adapter acceptance policies, not changes to Calibre game semantics.
Document a justified adjustment if the evidence contradicts a proposed protocol
policy before registering a test that would accidentally enforce the wrong behavior.

## Atomic implementation sequence

Each numbered item is its own coherent commit with implementation, tests, registry
updates and evidence together. Evidence-only commits contain no placeholder claim
that later implementation already works. Dependencies follow table order; existing
behavior is retained throughout.

### 1. Freeze the supported Codex protocol profile

**Complete, 2026-09-26; no production adapter claim.** The
[current profile](../../reference/02-codex-cli.md#supported-profile-01571-implemented-also-exercised-on-01593)
freezes complete-only, one-agent-message acceptance, framed combined stdin,
all-properties-required plus annotated-ref union adaptation, and partial isolation.
The live tool canary has commentary and a second agent message with no final
discriminator; it is explicitly outside the supported profile. Unknown/reasoning/tool
events and error-recovery streams are initially rejected, not treated as harmless.
Requiredness alone was insufficient: the recorded `$ref`-description rejection
requires the second Codex-local preparation change in step 2. Local AGENTS.md canary
suppression needs `project_doc_max_bytes=0`; full account/tool isolation remains open.
Synthetic error/order fixtures specify future tests without pre-registering them.

Read both backend references, recheck help/version/auth, then run the bounded probes
above with actual bundled requests. Preserve the original schema alongside any
adapted copy. Capture outline, cast, opening and continuation; use an independent
hand-built expected state when interpreting the response. Freeze representative
success transcripts and labelled synthetic error/order variants.

Exit: `reference/02-codex-cli.md` distinguishes actual observations from open
questions; payload selection, complete-only versus incremental delivery, instruction
transport and schema requirements are precise enough to implement. Neither a single
cast nor command help may be presented as verification of the full adapter.

### 2. Implement the isolated schema/invocation preparation

Add the Codex compatibility file and required module registration. Reuse shared
rendering and consume a schema copy. Walk actual schema-bearing nodes (`properties`,
array items, definitions, unions) rather than treating arbitrary description/default
objects as schemas. Preserve property ordering, `$schema`, `$defs`/`$ref`, descriptions
and `additionalProperties` unless this backend's evidence requires otherwise.

Error cases: an unsupported schema shape is an explicit preparation failure with
location/context; invalid executable/model configuration cannot spawn. Edge cases:
nullable chapter title; nullable versus empty upcoming events; genuinely defaulted
delta fields; required-but-possibly-empty NPC relationships and scene description;
zero NPCs; unknown quick-action kinds through unchanged shared wire mapping. Nominal:
all three adapted schemas and exact invocation/stdin/schema-file contents.

Making a property required does **not** authorize adding null to its type. Preserve
existing nullability, blank/default meaning and incoming DTO tolerance. Shared
schemas and Claude's adaptation must remain unchanged. Use allowed-environment
names supported by evidence (auth home and executable resolution included), not a
copy of all ambient variables. Exclude metered-auth overrides; do not claim an env
allowlist alone verifies the saved account's auth mode.

Exit: preparation returns a coherent owned request; golden diffs explain every
backend-specific change. The existing peer/generic schema regressions still pass.

**Implemented 2026-09-26:** Codex-local schema adaptation and fixed invocation
preparation now live in `generation::backend_compat::codex_cli` and
`backends::codex_cli`. The prepared request owns its schema and `RequestWorkspace`,
uses only the named HOME/PATH/CODEX_HOME environment, and transfers to the existing
`ProcessSpec` without launching a child. It does not implement the protocol codec or
`Backend::generate`; live isolation and the optional model override remain unverified.
See [the step-2 review record](../../reviews/2026-09-26-codex-step2/README.md).
The subsequent [review repairs](../../reviews/2026-09-26-codex-step2/review-and-repairs.md)
reject constraint loss and unsupported structural forms, correct executable PATH
resolution, and strengthen the no-launch test. The
[step-3 handoff](phase1-codex-step3-handoff.md) defines the next bounded slice;
it is planning only.

### 3. Implement the private event state machine without subprocesses

**Implemented offline, 2026-09-27.** The private codec consumes complete byte
records, latches located failures while retaining candidates, and returns protocol
completion only. Nine registered tests replay the frozen synthetic/live captures
and additional edge cases. [Evidence and boundaries](../../reviews/2026-09-27-codex-step3/README.md).
No process reconciliation, executable Backend or new live acceptance is claimed.

Drive it with complete byte records from frozen fixtures. Write error cases before
edge cases before nominal cases. Cover missing/failed/duplicate terminal outcomes,
malformed UTF-8/JSON, ineligible messages, ambiguous candidates, conflicting results
and unexpected ordering. Cover harmless metadata, CRLF, trailing record without
newline, empty strings/fragments and missing usage. A real child is unnecessary for
these pure protocol decisions.

Exit: the codec produces a final **candidate and protocol outcome**, never public
success by itself. It preserves exact payload evidence and cannot equate preview
with completion. Each supported transcript has independent expected events/payload;
do not use the implementation to generate its own expected answer.

### 4. Reconcile transport, protocol and failure evidence in the real adapter

**Implemented 2026-09-27.** `CodexCliBackend` verifies subscription auth at
construction and launches one generation child per call via the existing
supervisor. Cancellation/timeout now retain explicit candidates; structured
transport causes retain nested failures without embedding diagnostic buffers in
messages. Real-child fixtures and a bounded live outline smoke passed.
[Evidence and limitations](../../reviews/2026-09-27-codex-step4/README.md).
Step 5 subsequently added composed fixture acceptance; step 7 completed the
bounded live adapter gate on 2026-10-02.

Implement `Backend::generate` by invoking the existing supervisor once, feeding its
records to the codec, and accepting a response only after both parts succeed.
Callback rejection must trigger supervisor cleanup. Check cancellation again at the
adapter acceptance boundary; never remove the supervisor's authoritative checks.

Audit the existing failure API **before** wiring it: `BackendError::Cancelled` and
`Timeout` currently carry diagnostics but no separate candidate payload, while the
engine reconstructs their raw response from emitted fragments. In complete-only
mode a candidate can exist without any fragment having been emitted. Add a minimal
vendor-neutral evidence attachment to those outcomes if needed, with all mappings
and callers updated in this commit. Do not smuggle evidence through the preview
callback, discard a captured candidate, or call a transport transcript the payload.

Exhaustively map `SupervisorError`, including incomplete input, caps, I/O and nested
cleanup failures. Preserve the initiating failure and cleanup failure together with
all captured bytes and prefix metadata. Do not flatten an entire diagnostic buffer
into a formatted error string. Auth/missing executable, timeout, cancellation,
protocol failure and cleanup failure remain distinguishable at the appropriate
boundary. No automatic retry, fallback backend or JSON-repair model request.

Normalize usage through the existing checked boundary policy, retaining unknown
versus zero and not adding cached tokens to a total that already includes them.
Propagate observed provenance; do not relabel a configured model as a reported model.
`GenerationEngine` currently preserves provenance but does not expose `TokenUsage`
in `Generated<T>`. Keep usage available at the low-level response for this slice;
application token reporting/persistence is a separate decision, not an implied
feature of this adapter. No monetary field is a subscription charge.

Exit: error/edge/nominal tests exercise the **actual adapter** against the fixture
executable through `process::run`. A test-only alternate backend cannot stand in
for this. Fixture mode must record argv/stdin/schema visibility without making its
scenario JSON the production adapter's argv contract.

### 5. Prove story contracts through the composed adapter

**Implemented offline, 2026-09-28.** Six registered composed tests drive the
actual adapter and supervisor through fixture children. They cover identity,
edited outlines, both restore policies, zero NPCs, chapters/rewind, late failures,
cancellation, output caps and a real workspace-cleanup failure. Entire state and
subsequent captured prompts remain unchanged on failed attempts; explicit retry
launches one new generation child. [Evidence and scope](../../reviews/2026-09-28-codex-step5/README.md).
No production engine/backend change or new live verification was needed.

Replay vendor-shaped fixtures through `CodexCliBackend -> GenerationEngine ->
StoryUseCases`. Include outline acceptance/edit, cast, protagonist selection,
opening, continuation and a chapter boundary. Handcrafted response fixtures should
include namesakes, exact duplicates, colliding IDs, retitling, and null/empty threads.
Use both restore-limit policies and a zero-NPC world. Assert subsequent prompts,
not only the final state; count actual generation children.

Failure cases: plausible candidate then nonzero exit, terminal error with exit zero,
malformed/domain-invalid payload, missing terminal, idle cancellation, cancellation
after candidate, output cap and cleanup failure. Complete game state stays unchanged;
retry is explicit and launches exactly one new generation child. Edge cases: preview
versus final disagreement (if supported), complete-only output without duplicate
narrative, absent metadata and preserved non-UTF-8 stderr. Nominal cases use the same
live adapter code and demonstrate one opening identity instruction and no extra
cast-review call.

Register these under BACKENDS-001, ARCH-002/003, TEXT-001, STREAM-001 and applicable
identity/prompt/limit/chapter contracts. BACKENDS-001 remains partial until both
adapters and their acceptance evidence exist. Never mark a whole contract enforced
because one backend's fixture passes.

### 6. Make adapter regressions fail the persistent mutation gate

Implemented 2026-09-28: seven adapter mutations join the eleven existing ones.
Each new mutant must reach its named behavioral assertion; protocol regressions
check child/workspace cleanup first. Shared and Claude schema isolation and
candidate/diagnostic retention have separate mutants. See the
[step-6 record](../../reviews/2026-09-28-codex-step6/README.md).

Keep all eleven current mutations. Add focused mutations for accepting an ineligible
message, accepting a candidate without terminal completion, treating conflicting
terminal events as success, leaking schema adaptation into the shared/peer path,
and losing candidate/diagnostic evidence on an error. Bind each mutation to an exact
registered assertion; split commits if a mutation belongs naturally with an earlier
implementation item rather than postponing its protection.

A protocol mutant must fail within a finite time and still clean its child/workspace.
Compiler errors, fixture-launch failures, absent tests and external watchdog kills
are harness failures, not successful mutation detection. Recheck architectural
ownership after green rather than weakening invariants to make tests pass.

### 7. Verify the finished adapter live and write the peer handoff

**Complete, 2026-10-02.** Bundled outline/cast/opening/continuation passed through
the real adapter and use cases on 0.159.3. Controlled cancellation returned with
the observed PID/workspace absent and the entire game unchanged. Four successes
emitted one complete payload each; model identity remained unreported. The initial
outer-sandbox failure and explicitly approved rerun are retained separately in
the linked evidence above. The peer handoff does not claim Claude implementation
or live headless acceptance. Exact namesake collisions, null/empty thread updates,
restored-limit policies and chapter breaks retain their distinct fixture evidence.

Run the real adapter's opt-in harness against authenticated Codex using bundled
outline/cast/opening/continuation requests. Include one controlled cancellation with
actual PID/workspace cleanup evidence. Record selected and observed model separately,
terminal/exit outcome, usage, exact payload and progress behavior. No automatic
retry; any rerun is an explicit diagnostic action, recorded as a separate attempt.

The smoke test validates this adapter slice, not headless play or every model's
future behavior. A model that emits a bad story does not authorize deleting a
namesake or loosening a domain invariant. Record provider failure distinctly from
adapter defects and leave unexercised behavior open.

Prepare the Claude item-5 handoff: shared supervisor/acceptance obligations, Claude's
own existing fixtures, block-index correlation, `is_error` precedence, raw embedded
payload extraction, schema adaptation, and the locally appended advisor instruction.
Do not reuse Codex's terminal semantics in Claude. Before implementing that peer,
check its current official documentation and its own installed/live behavior. Offline
scaffolding may proceed without peer auth; live status remains unverified.

Exit: Codex has a concrete supported protocol profile, real adapter, offline
composed tests, retained mutations and a dated live result (or explicitly blocked
live gate). Claude and presentation work retain their independent completion gates.

## Per-commit TDD and completion checklist

For each new behavior: write an error test, observe a meaningful runtime failure,
implement the minimum and see it pass. Repeat for relevant edge cases, then nominal
behavior. Compilation/fixture errors do not count as red. If a test already passes,
record it as existing coverage; do not intentionally damage correct production code
or alter expectations to manufacture TDD history. Mutation evidence is reported
separately from red-before-implementation evidence.

Every implementation commit records affected contracts, test/evidence paths and
remaining uncertainty. After green, refine ownership/types and run:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
bash scripts/check_contracts.sh
```

Fetch new locked dependencies before the offline gate when necessary. Ordinary tests
must need no vendor executable, credentials or network. Keep production source,
fixtures, mutations, registry and status mutually consistent. Do not pre-register
hypothetical tests or promote live status based on this planning document.
