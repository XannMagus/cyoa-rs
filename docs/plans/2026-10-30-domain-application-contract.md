# Domain and application contract proposal

Refined on 2026-11-02 by the [technical contract](2026-11-02-technical-contract.md)
and [implementation plan/contract audit](2026-11-02-implementation-plan.md).
Read those for the current proposed interfaces and implementation order. The
product review choices at the end of this earlier draft are settled; their
question wording below is retained as history, not a new approval request.

Status: **proposed for review**, 2026-10-30. No implementation or new enforcement.
User-approved requirements are in the
[decision record](../decisions/2026-10-30-roleplay-and-session.md); current-source
evidence is in the [inventory](2026-10-30-capability-inventory.md). This document
resumes the previously deferred specification work. Recommendations here do not
become accepted decisions merely by appearing in this document.

## Purpose and scope

Define the shared game capabilities before migrating presentation. REPL, TUI and
future GUI express the same product intents and consume player-facing views.
Application owns their sequencing and failure policy. The interface must be deep:
requesting an action includes acceptance and saving; clients do not manually
assemble those steps. Multiple cohesive handlers may implement this interface;
one giant public session class or a generic message bus is not required.

The MVP has AI-adjudicated stats-based rules, not an executable rules interpreter.
Rust enforces record integrity, legal lifecycle transitions and accepted-state
ownership. It does not prove rule balance, narrator arithmetic, narrative truth,
or lack of spoilers in prose. A future interpreter may replace adjudication behind
the generation seam without taking session authority away from application.

No new platform, concurrent game editing, restart-persistent uncertain-write
registry, clarification interview or rule-negotiation feature is introduced here.
Existing export scope remains pending. Ordinary NPCs do not acquire full playable
sheets merely because they inhabit the shared world.

## Ownership and proposed modules

| Layer | Owns | Excludes |
| --- | --- | --- |
| Domain (`cyoa-core`) | Adventure aggregate; world/actor identity; candidate alternatives; rules/sheets; knowledge; fictional time; accepted history; legal story transitions; generation and game repository business ports | Threads, serde/JSON, terminal commands, transport captures, helper protocol |
| Application | Canonical session; product command/query handlers; admission and result acceptance; operation keys/revisions; retry/cancel; save/load/recovery sequencing; OOC archive; execution ports and evidence | Concrete subprocesses/repositories, terminal rendering, thread joining |
| Infrastructure | Both backend adapters, prompt/schema/wire mapping, storage codec, private prepared writes, generation/storage execution adapters | Choosing to autosave, accepting a new canonical game, advancing the session independently |
| Presentation | Slash syntax and aliases, edit buffers, rendering, line editing, signals, terminal restoration and output drain | Calling workers directly, choosing successors, coordinating saves, applying deltas |
| CLI | Composition and concrete adapter selection | Product policy |

Use the existing five crates. Proposed domain modules are `adventure`, `actors`,
`rules`, `knowledge`, `timeline` and `history`, reusing existing types where their
meaning still fits. File/module names are implementation choices. Keep identity,
summary merge, chapters and limits as cohesive existing modules rather than
copying their algorithms into a new session layer.

## Domain model

### Adventure, actors and viewpoint

An `Adventure` privately owns its accepted setup, established actors, rules,
fictional snapshots and history. A playing snapshot contains world memory,
current timeline position, roster sheets, ordinary NPC memory and a coherent POV
state. A human player and their current character remain different concepts.

Candidate generation produces `CandidateGroup`s. Each group represents one
possible person with one or more mutually exclusive versions; different groups
represent distinct people, even if names match. Use distinct newtypes for
`CandidateGroupId`, `CandidateId` and established `CharacterId`. Do not group by
name similarity. Constructors check membership, unique identities and a nonempty
version list. Reuse exact-record deduplication and suffix-repair policy where
applicable; no hidden semantic merge.

`CharacterChoice` is `Select(CandidateId)` or a checked created-character result.
Selection establishes that candidate's version. A narrator operation establishes
one version for every other group, applying excitement/intrigue/closure criteria.
Discarded versions are not active actors or successors. Preserve their setup
provenance if useful for diagnostics; never send them as coexisting world actors.

Assign actor IDs once at establishment. POV is a reference to an established
actor ID; changing POV does not rename an actor to `protagonist`, erase their
history or change their identity. The old fixed `protagonist` ID is a compatibility
detail requiring an explicit contract update, not the new actor-identity model.

Every eligible roster member owns a sheet and knowledge record. Ordinary NPCs
own identity, background, relationships, situation and relevant private memory;
only mechanically relevant details need recording. A life state is explicit:
`Alive` or `Dead { established_at }`; absence of a sheet or prose containing
"dead" does not determine life state. Death need not remove anyone from memory.
Keep unavailable-for-handoff distinct from dead: a temporarily unsuitable living
candidate may be reconsidered in the second rotation.

`AdventurePhase` is proposed as:

```rust
enum AdventurePhase {
    AwaitingOpening { protagonist: CharacterId },
    Active { protagonist: CharacterId },
    AwaitingSuccessor { death: DeathRecord, search: SuccessorSearch },
    Finished { reason: EndReason },
}
```

These are conceptual shapes, not compile-tested declarations. Actual fields are
private and checked methods establish aggregate invariants. Active/AwaitingOpening
require a living roster member with a valid sheet. AwaitingSuccessor has committed
death and no acting protagonist. Finished does not imply an empty character cast.

### Rules and generic sheets

`Ruleset` has stable definitions, concise player instructions, narrator adjudication
guidance and `CreationPolicy::{PredefinedOnly, Allowed { procedure }}`. No approval
flag or rule-editor workflow is needed. Definitions describe supported data, not
expressions or executable scripts.

Proposed stat types:

| Definition | Value and checks | Example |
| --- | --- | --- |
| Integer | Checked signed integer with optional inclusive bounds | luck 7, reputation -2 |
| Decimal | Exact bounded decimal representation with checked arithmetic/scale, not NaN/infinity | weight 2.5 |
| Text | Nonblank bounded business text | fighting style: defensive |
| Choice | Stable option ID belonging to a nonempty unique definition list | stance: cautious |
| Flag | Boolean | poisoned: true |
| Resource | Current and maximum exact numeric values of the same numeric kind, checked against the definition's permitted range | stamina 8/12 |

`StatId` differs from display label. A `StatDefinition` carries label, meaning,
type, range/options and applicability. A `CharacterSheet` maps each applicable
definition ID to a matching `StatValue`, with explicit inventory, abilities and
conditions. Inventory entries have stable IDs, names, positive quantities and
optional properties described by definitions; abilities/conditions have IDs and
descriptions, with optional typed values or duration when the ruleset requires
them. Conditions need not all be numeric; do not invent a required combat engine.

Absence means not applicable only where a definition explicitly permits it.
Boolean false, zero and empty inventory are values, not missing-data sentinels.
Proposed numeric scale/size and collection/text budgets are reviewed configuration
constants, not new product questions; finalize them against fixtures before coding.

Adjudication returns atomic proposed changes keyed by stable IDs. Domain checks
references, value types, ranges, duplicate writes and aggregate consistency before
acceptance. A wrong-type or out-of-range result is an invalid response, not a silent
clamp. This validates representation rather than whether a rule was applied fairly.
New definitions discovered during play need a deliberate definition-addition delta
with matching sheet values; existing definition IDs cannot change type implicitly.
Rules text does not get rewritten during every summary consolidation.

Future scripting can consume the same state and emit the same proposed changes.
Do not introduce a script runtime, rule-expression language or authoritative dice
roller during this MVP merely to anticipate that future seam.

### World truth, beliefs and visible information

Separate `NarratorMemory` from per-actor `CharacterKnowledge`. Narrator memory
contains established facts, hidden intentions, off-screen outlines and explicitly
tentative plans. A planned twist is not automatically an established fact.
Character knowledge contains assertions with `Known`, `Believed` or `Suspected`
status and their source/discovery point. A false belief does not overwrite world
truth; actors need not know whether their belief is wrong.

References to facts/actors are checked IDs, while assertions retain readable prose.
Do not require every descriptive sentence to become an ontology. The structural
model supports the distinction; AI interpretation still determines what is known.

Player-facing application views contain only the current character's accessible
information and rendered public narrative. Knowledge views must not disclose a
truth link, hidden confidence field or another actor's private outline. An explicit
advanced inspection query is a separate use case. Transport diagnostics and saved
documents may contain secrets; they are advanced facilities, not player-safe views.

World review occurs before character choice: propose a short introduction limited
to the common starting premise. After choice/creation, provide that character's
personal introduction. Candidate cards must omit secret backstory while retaining
enough visible identity for a meaningful choice. Narrator sees full memory when
generating prose or OOC answers; Rust cannot guarantee semantic non-disclosure.
Register adversarial fixture and live evaluation cases rather than claiming a
DTO proves arbitrary prose spoiler-free. Tentative streaming has the same limitation.

### Fictional time and off-screen developments

One `WorldInstant` advances through nonnegative `FictionalDuration`, proposed as
integer seconds plus optional authored calendar labels. It has no connection to
wall-clock generation latency. An action may span seconds or days. Flashbacks are
narrative recollection rather than moving the authoritative clock backwards.

A resolved action supplies elapsed time, affected actor outlines and knowledge/
mechanical deltas at the resulting world instant. Unchanged characters inherit
that common instant; they do not require a meaningless rewritten outline every
turn. An interval with material off-screen changes needs explicit updates, not a
separate simulation engine or full prose for every actor.

Store a last-announced cadence so presentation can display elapsed duration at
the first action and when it changes. Repeated equal cadence need not repeat its
announcement. The authoritative duration is a domain value; clients must not
parse it from narrative prose. An OOC question never advances this clock.

### Accepted history, rewind and proposed replay

Keep complete fictional snapshots after accepted actions and handoffs. Snapshots
include POV, shared time, knowledge, sheets, life states and off-screen memory.
Summary compaction must not alter past accepted snapshots. Chapter membership and
effective titles remain derived as today; don't duplicate mutable chapter indices.

Propose action checkpoints as rewind units: `/rewind 1` undoes one accepted story
action and every succession consequence following it. A successful handoff does
not masquerade as a player action. Rewinding a fatal action restores the prior
living POV and all affected state. The zero-turn initial checkpoint remains valid.

To make already-established outcomes repeatable, propose retaining discarded
branches locally in the adventure history. Re-submitting the exact prior intent
from the same checkpoint restores its stored accepted outcome (including recorded
roll results and handoff), without inference. A different intent creates another
branch; unselected branches never enter current storyteller context. Match checked
command identity and parameters, not semantic similarity of player text. No claim
that regenerating the same prompt through a model is deterministic.

This replay cache is a **review choice**, not an accepted requirement yet. It adds
save size and a reason to retain discarded future state. Without it, the narrower
guarantee is exact snapshot restoration only. Don't silently substitute that
narrower guarantee for the user's determinism aim.

OOC records have separate exchange IDs, exact originating checkpoint/branch and
the question/answer/provenance. A new accepted OOC answer increments the session's
save revision and is persisted, but changes no fictional checkpoint/time/sheet.
Current generation context includes only successful exchanges on the current
ancestry. Discarded-future exchanges remain explicitly historical in the archive.

## Application command and query interface

Commands express use cases; names below are illustrative. Each reports either
admission rejection with no dispatched work, immediate accepted change, or an
admitted operation key. Asynchronous completion is observed via `AdvanceSession`.
Returning an operation key is not reporting success. Presentation never constructs
the continuation chain itself.

| Command | Admission and inputs | Accepted result / continuation | Failure behavior |
| --- | --- | --- | --- |
| SubmitBrief | Setup, no active work; nonblank brief | Generate world/private memory/public introduction/rules; enter world review | Keep brief and diagnostics; explicit retry |
| ReplaceOutline | World review, idle; checked edited outline | Replace reviewed setup data; invalidate obsolete generated candidates | Invalid edit changes nothing; prevent stale cast acceptance |
| AcceptWorld | World review, idle | Generate cast consistent with accepted world/rules; show common rules and candidate cards | Remain in review with rules/world unchanged; retry same intent |
| SelectCandidate | Candidate selection; member ID in that setup | Establish chosen version and narrator-resolved remaining roster; create zero-action adventure; save; show sheet and start one opening | No partial roster on generation failure; save failure blocks opening |
| CreateCharacter | Candidate selection; creation allowed; optional concept | Generate character and resolve initial roster, then same establishment/save/opening sequence | Reject unsupported creation before inference; failure preserves selection stage/concept |
| AttemptAction / ChooseSuggestion | Active living POV, clean durable state, idle; player text or current suggestion ID | Resolve and atomically accept fiction, sheets, time and outlines; autosave; if death, start separate succession after save | Failed/cancelled resolution changes no fiction; failed save keeps accepted action but blocks next mutation |
| Continue / InterestingEvent | Same playing admission | Narrator continuation using the same acceptance and persistence contract | Same explicit retry/failure isolation; no automatic inference retry |
| AskOoc | Active POV, idle/clean; explicit question | Generate knowledge-respecting answer; archive and save exchange; no fictional change | No partial answer accepted; no time advance; separate retry intent |
| AdvanceSession | Always callable on event loop | Accept current events; initiate eligible continuations; report prompt progress | Nonblocking; no live join/disk I/O; bounded event processing |
| RetryGeneration | Idle with a recorded failed/cancelled generation intent | Dispatch same operation against its unchanged accepted base | Never repeats a committed fatal action to retry a handoff |
| Cancel | Active cancellable work or pending continuation | Observe cancellation; join/reap through executor; retain canonical state | Late cancellation rejects unaccepted success; durable writes still report their actual outcome |
| Rewind | Idle; positive action count within current branch; storage obligations resolved | Restore full checkpoint; persist before any new generation | Invalid count changes nothing; failed save leaves restored canonical state unsaved |
| Save / SaveCopy | Existing adventure; existing recovery admission rules | Save current accepted snapshot or reconcile retained exact attempt; bind copy on success only | Preserve canonical state, original binding and any earlier uncertain attempt |
| Load | Idle or existing permitted recovery state; explicit copy/limits policy | Validate source/format and limits; coherently replace session; persist policy/recovery changes | Preserve old game, binding, retry intent and client edit buffers on failure |
| FinishSession | Any live session | Cancel/join generation, settle applicable write and bounded final save; report exit outcome | Preserve worker/I/O failure even if final save succeeds; no output-drain acknowledgement in application |

Initial world/rules generation is one proposed cohesive setup capability; backend
request count is an implementation choice except existing opening-call and retry
guarantees. Do not show rule acceptance or preliminary interview prompts. Keep
setup buffers in presentation; generated/accepted setup data belongs to application.

No broad character-edit command is approved yet. Outline replacement and existing
style operations retain their purpose. Character editing will get its own
specification for identity, sheets and past snapshots before exposure; it must not
be implemented as unrestricted mutation of private domain fields.

Read-only queries:

| Query | Returns |
| --- | --- |
| SetupView | Current public introduction, common rules, player-safe candidate cards, creation availability |
| StoryView | Current branch narrative and chapter views, active POV introduction, player-known situation |
| CharacterView | Current POV's visible sheet, inventory, abilities, conditions and knowledge |
| AvailableActions | Suggestions and session capabilities, with reasons when unavailable; these don't grant admission authority |
| ProgressView | Operation kind, bounded tentative narrative/answer, cadence and pending continuation status |
| PersistenceView | Binding, acknowledged revision, unsaved/uncertain state, independent storage/generation failure summaries |
| OocHistory | Exchanges distinguished as current-context or historical |
| AdvancedInspection / Diagnostics | Explicit narrator/internal data and exact evidence; never substituted for player views |
| ListSaves / InspectSave | Existing application read-only repository queries, without changing current game |

Queries return views, not mutable aggregates/controllers. A query needing live AI
work is a command such as AskOoc; reading an existing answer is a query.

## Succession and acceptance protocol

The accepted death checkpoint is saved first. Its `SuccessorSearch` fixes the
surviving eligible roster and a narrator-ranked order using excitement/intrigue/
closure. A roster member appears at most once per rotation. Exclude the deceased
and discarded versions. An empty surviving set immediately finishes the adventure.

Each attempt has a checked `(death checkpoint, rotation, candidate ID)` identity.
Proposed narrator results are `Handoff { candidate, introduction, knowledge_view }`
or `Unsuitable { candidate, reason }`. A technical failure is an execution/operation
error, never an Unsuitable result. A rejected candidate remains alive unless a
separately validated fictional fact establishes otherwise; the bookkeeping loop
itself advances no fictional time and must not kill candidates to repair failures.

On accepted Unsuitable, retain reason and advance the cursor, save the updated
search, then automatically try the next candidate. After all candidates in rotation
one, begin rotation two. After the second complete rejected rotation, finish with
`NoViableSuccessor`. Save that terminal state. On technical failure or cancellation,
retain the same candidate/cursor and wait for explicit retry; do not consume a
rotation slot. Save failures block further attempts until resolved.

On accepted Handoff, reference that candidate's established ID, show a knowledge-safe
summary, save and enter Active. Do not regenerate their whole prior adventure or
advance world time simply to produce an introduction. The handoff's own state
validation must reject another candidate, stale checkpoint or dead actor.

The ranked order must be established without presentation selecting a candidate.
If ranking itself fails technically, remain AwaitingSuccessor with no consumed
attempt. Exact split between ranking and first attempted handoff can be chosen
to avoid unnecessary calls while retaining explicit evidence.

This proposes a precise interpretation of two rotations: candidates can be
reconsidered on pass two; a rejection isn't automatically permanent death.
It needs review alongside the usefulness of that second pass.

Replace the universal 1–3 quick-action invariant with a checked outcome enum:
continuing passages carry nonempty `QuickActions`; fatal/terminal passages carry
their transition obligation or end reason instead. Keep `QuickActions` itself
nonempty. Do not merely permit an empty list on every ordinary turn. OOC answers
and handoff introductions are distinct results, not malformed ordinary turns.

## Business ports, execution and storage evidence

Domain business capabilities accept domain requests and return checked proposed
results/audit or operation failures. Proposed generation methods cover setup,
cast, establishment/creation, action, OOC and succession. Distinct request/result
types prevent accepting an OOC answer as a story turn. Reuse a coherent generator
adapter rather than inventing one trait per request. The two real adapters and
scripted/demo generators already supply genuine implementation variation.

Application execution ports accept owned requests with operation keys and base
revisions, expose tentative progress and joined completion, and support closing.
Executors never choose continuations or commit canonical state. Keep the stateful
demo generator across jobs and retain the existing race-free cancellation wake
mechanism; a domain cancellation-observation capability must not erase the
concrete infrastructure token's wake subscription.

Map successful and failed operation audit separately from execution evidence:
exact response/provenance in domain exchange audit; transport bytes, worker panic,
timeouts, capture limits and cleanup outcomes in application execution reports.
Keep missing response distinct from an empty received body. Preserve success
diagnostics as well as failure diagnostics. No serialization or vendor DTOs enter
domain. Schema tolerance remains local to each backend under ARCH-003.

The domain `GameRepository` retains create, expected-stamp replacement, reconciliation
and load intents through inward vocabulary. Application owns list/inspection views
and query ports. Do not move thread/helper/diagnostic types inward merely to move
the trait declaration. Its associated opaque attempt type is clonable; the concrete
infrastructure handle privately owns exact prepared bytes and metadata. Required
attempt publication precedes first disk mutation; an independent capture outside
the worker survives panic. Retained prior uncertainty survives failed save-copy.

No application query can extract encoded attempt bytes. Infrastructure can inspect
its own handle to reconcile via a fresh helper. Explicit save after uncertain write
reuses that handle. No reconciliation after acknowledged success; bounded final
save policy, exact receipts, lock/atomic-write behavior and failure distinctions
remain as currently tested. Application completion excludes terminal drain.

## Admission, revisions and durability

Keep independent checked identities for adventure/branch/checkpoint, session
revision and operation request. Rewind cannot reuse a revision to make a stale
completion current. Load invalidates old work coherently. All mutating/generating
commands recheck admission even if a previous capability query said allowed.

One generation operation in flight; no concurrent fictional changes. Every accepted
persisted change, including OOC archive and successor cursor, advances save revision.
Storage acknowledgement is separate from accepted canonical state. Unsaved/uncertain
storage blocks further state-producing work while allowing inspection, cancel,
safe completion and existing recovery commands. A purely local read query remains
available; OOC generation is not treated as a free read during storage failure.

Counter admission reserves capacity for mandatory save/closing continuations.
Represent deferred opening, successor search and saving continuations with typed
states, not independent booleans that can fire twice. Apply a generated owned
snapshot once; never commit its turn a second time during session acceptance.

## Save format and context retention

Propose a new explicitly versioned save format for the expanded model, with clear
unsupported-version errors for older development saves. Q33 removes the requirement
for legacy continuation or invented upgrades. Ownership-only migration can retain
current fixtures until the deliberate feature-format transition; update format
contracts/tests then, not by deleting failing regression coverage in advance.

New saves include accepted setup/rules, actor identities and resolved variants,
snapshots/current history, life/POV states, timeline/cadence, successor cursor,
OOC ancestry and replay history if approved. Restore validates references and
aggregate state before application replacement. It does not replay random rolls
or ask the AI to reconstruct historical sheets.

Narrator context has a protected section (rules, definitions, current sheets,
POV/eligibility, canonical identities, current knowledge) and consolidatable prose
sections (past events, off-screen outlines, relevant OOC history). Configuration
sets byte/count limits for each and a total constructed request budget. Check
budgets before dispatch. Exceeding them is an explicit capacity/configuration
failure; never silently truncate authoritative state. Off-screen consolidation
must preserve pinned facts and current sheets. Propose budgets with measured
representative fixtures rather than ungrounded universal token guarantees.

Retaining many accepted snapshots on disk is distinct from sending all history
to inference. Old branches, inaccessible secrets in player views and historical
OOC exchanges must not accidentally enter the current request renderer.

## Required acceptance and migration plan

| Slice | Required checks before considering it complete |
| --- | --- |
| Inward vocabulary and opaque save evidence | Every port-signature type owned inward; preparation published before mutation; fresh-worker panic recovery; failed-copy retains old handle; diagnostics exact; no new dependency exceptions |
| Session/execution ownership migration | Existing headless stories and both adapter fixture paths unchanged; late cancel/stale/duplicate results rejected; generator reuse; safe shutdown separated from terminal drain; registry/mutations move without weaker assertions |
| Rules, actors and setup | Every stat type and invalid value; distinct person versus variant grouping; chosen version collapse; narrator resolves other groups; allowed/forbidden creation; stable actor IDs through selection |
| Shared-world action | Atomic sheets/time/knowledge/outlines; no commit after failure/cancel; elapsed cadence announcement; false beliefs do not overwrite facts; ordinary NPCs don't require full sheets |
| Succession | Death persists before first attempt; persisted cursor; exactly two rotations on fictional rejection; technical failure/cancel consumes none; no living candidates ends immediately; restart from each intermediate state; terminal passage doesn't weaken continuing-action invariant |
| OOC, rewind and replay | Explicit aliases share intent; no clock/state mutation from answer; archive saves; branch-aware context; complete snapshot restoration; stored-outcome replay/no inference if approved; changed action takes separate branch |
| Expanded persistence and live acceptance | Strict new format restoration, malformed/unsupported version and source/policy checks; partial/crash recovery still exact; both backend-local schema fixtures and updated real live gates, each verified by an authenticated session |

Domain tests establish invariants without infrastructure. Application tests use
inward execution adapters that can deterministically deliver stale, cancelled,
wrong-kind or failed results. Wire/save tests establish mapping separately.
Semantic narrator behavior needs adversarial examples and live evidence; fixtures
do not prove arbitrary model compliance. Update affected ARCH/STATE/IDENTITY/TEXT/
LIMITS/PROMPTS/STREAM/PRODUCT/BACKENDS/ACCEPTANCE contracts and ADRs as actual slices
land. Preserve both mutation gates, domain-only scrutiny and existing runner profiles.
Run `bash scripts/check_contracts.sh` at implementation completion.

This proposal has not run runtime tests or inference because it changes no code.
Documentation and existing architecture checks are the appropriate present validation.

## Review choices before implementation

November 2 status: items 1 and 2 and action-based rewind including resulting
succession are accepted in the [decision record](../decisions/2026-10-30-roleplay-and-session.md).
Item 3 is now settled with a clarification: initial selection shows the brief and
concise character descriptions, with optional spoiler-safe macro-level questions
to the narrator. Do not require an extra exposition step or present ongoing
adventures as player-selectable successors. Selection inquiries need their own
setup admission despite there being no chosen protagonist. The decision record also specifies richer OOC action guidance
and a future, non-MVP conversational brief interviewer. These accepted clarifications
govern the proposal where earlier wording is less specific.

1. **Rewind and replay:** accept action checkpoints as rewind units and retain
   abandoned branches so exact repeated actions restore established outcomes,
   without another model call? Recommended yes; clarify that different free text
   is a different intent, even if it describes a similar action.
2. **Two successor rotations:** accept temporary unsuitability (not automatic
   death), no world-time advance during selection, and persisted attempt cursor
   with explicit retry for technical failures? Recommended yes. This makes both
   rotations meaningful while keeping the death separate and permanent.
3. **Preselection information:** accept a common-premise world introduction and
   spoiler-safe candidate cards, followed by personal knowledge after choice?
   Recommended yes. A character-specific introduction cannot precede knowing who
   the played character is.

Other representations here are proposed technical choices for review, not requests
to reconsider settled product principles. Character-edit permissions and concrete
context budgets must be specified before those particular capabilities land.
