# Technical refinement: shared adventure contract

**Current scope authority:** [REPL MVP and future roadmap](mvp-and-roadmap.md).
This full contract preserves advanced future capabilities. It does not require
implementing the entire model for MVP. Selective causal replay is deferred, not
discarded; its richer accepted behavior remains the future target.

**MVP scope update:** the [lean-MVP clarification](../decisions/2026-10-30-roleplay-and-session.md)
supersedes selective causal replay below. Do not implement consequence-dependency
graphs, engine-enforced partial invalidation or a dedicated prose rewrite workflow.
Same action/settings reuses recorded state; changed settings allow a new generated
continuation guided to preserve established mechanical results. Other sections
remain proposals awaiting broader scope review, not an instruction to build them all.

Status: **proposed for review**, 2026-11-02. Design/documentation only; none of the
new types or tests below is claimed implemented or compiler-verified. This refines
the [earlier proposal](2026-10-30-domain-application-contract.md) using the
[accepted decisions and November 2 clarifications](../decisions/2026-10-30-roleplay-and-session.md).
Where they differ, explicit user decisions govern. The earlier proposal is no
longer a list of unanswered product questions. See the
[implementation plan and contract audit](2026-11-02-implementation-plan.md).

## 1. Interface and ownership

Keep five crates and the inward dependency graph. Presentation receives an
application session interface with commands, views and `advance()`. It never sees
a mutable domain aggregate, repository, worker or controller. Use cohesive methods
with typed parameters/results; the command names below do not require a message
bus or one handler class per command.

Domain owns business capabilities and checked state transitions. Application owns
one canonical session and executes its product policies. Infrastructure implements
business ports and the two application execution ports. The CLI wires them.
Execution completion is joined/cleaned work, distinct from application acceptance;
accepted canonical changes are distinct from durable acknowledgement. Terminal
output draining/restoration remains a presentation responsibility.

Preserve existing domain operations until a specified successor operation replaces
their responsibility; additive application use cases are appropriate. Do not
preserve two canonical controllers as the migration's permanent architecture.

## 2. Identity, setup and persistent state

Names here denote separate newtypes with checked construction, not interchangeable
aliases. Counters are checked, monotonic within their owning adventure/session,
and reject overflow. Identity membership is checked against the owning aggregate.

| Identity | Ownership and meaning |
| --- | --- |
| `AdventureId` | Stable identity of the fictional adventure, including retained branches; distinct from save slot |
| `SetupRevision` | Invalidates stale setup work/questions when world or cast changes |
| `CandidateGroupId`, `CandidateId` | One person and one possible version of that person before establishment |
| `CharacterId` | Persistent established actor identity, independent of name and current POV |
| `HistoryNodeId`, `ActionId` | Accepted historical position and the action owning its consequences |
| `ExchangeId`, `OfferId` | A setup/OOC exchange and one explicit offer to perform an in-world action |
| `SessionRevision`, `OperationId` | Application acceptance freshness; never rewound/reused |
| `SaveId`, `ContentStamp`, save revision | Existing storage identity/concurrency evidence, never a substitute for story position |

### Setup

`SetupDraft` holds the brief, checked world outline, concise world-review text,
private narrator memory and rules. `CandidateRoster` owns nonempty candidate
groups, descriptions safe for selection, private background and proposed sheets.
The group explicitly identifies mutually exclusive alternatives; name similarity
never creates a group. Exact-record deduplication still preserves distinct namesakes.

Selection views show the brief and short character descriptions, not full secret
biographies or simulated adventures. Common rules are informational. The prior
concise world-review pause stays before cast generation; it is not duplicated as
an extra preselection exposition screen.

`CreationPolicy` is `PredefinedOnly` or `Allowed { procedure }`. A creation concept
is accepted only after starting that alternative path. The creation result must
have all the same established character data as a selected candidate. A separate
checked establishment result contains exactly one selected version of each original
group, plus the created protagonist if applicable. Chosen versions are fixed;
the narrator resolves unchosen groups for excitement/intrigue/closure. Discarded
versions are absent from active memory/succession but remain in setup audit.

Do not assign all current POVs the ID `protagonist`. Once established, Alice keeps
Alice's ID even when Bob becomes POV. Restore validates every POV/knowledge/sheet
reference before replacing the current session.

### Adventure aggregate

Proposed private storage:

```text
Adventure
  id, established_setup, original_limits, active_limits, style
  history: HistoryTree             # accepted fiction and control continuations
  cursor: HistoryNodeId            # single active linear path through tree

WorldSnapshot
  world_instant, last_announced_cadence
  actors: ordered CharacterId -> ActorState
  ruleset, narrator_memory
  phase: AwaitingOpening | Active | AwaitingSuccessor | Finished

ActorState
  identity, background, relationships, life: Alive | Dead { at }
  situation, knowledge
  role: RosterMember { sheet } | OrdinaryNpc
```

An `Active` snapshot carries a living roster member's ID and nonempty suggestions.
`AwaitingOpening` carries the established living POV with no passage yet.
`AwaitingSuccessor` carries a committed death record and search state; no player
action can be directed through the dead POV. `Finished` carries
`NoSurvivors` or `NoViableSuccessor`; other endings are future use cases, not an
unrequested AI power to terminate an otherwise live adventure.

Normal NPC memory needs no full playable sheet. A sheet can describe mechanically
relevant enemies without making them successors; eligibility belongs to the
established protagonist roster, not possession of numeric data. No automatic
promotion of every newly encountered character into the succession pool.

## 3. Generic stats and AI adjudication

`Ruleset` owns instructions and ordered `StatDefinition`s, creation policy, item/
ability definitions and narrator procedures. `StatDefinition` has `StatId`, name,
meaning, value kind and explicit applicability. Definition changes are deliberate
domain operations; summary consolidation cannot rewrite them.

Concrete initial value kinds (proposed technical choices):

| Kind | Representation and checked invariants |
| --- | --- |
| Integer | `i64`, optional inclusive bounds; checked range ordering |
| Decimal | Normalized coefficient `i64` and scale `0..=6`; canonical equality (1.0 = 1); checked `i128` comparisons; no binary floating point |
| Text | Nonblank business text |
| Choice | Stable option ID in a nonempty, unique option collection |
| Flag | Boolean, with false distinct from missing |
| Resource | Numeric current/maximum of one declared numeric kind; declared lower bound <= current <= maximum; maximum within permitted range |

A resource whose rules permit debt/negative values can declare a negative lower
bound. A ruleset that needs values above a nominal maximum must represent that
nominal cap separately or explicitly update its actual maximum. Rust does not
silently clamp an AI result. The storyteller can bend fictional rules, but must
still return coherent typed state.

`CharacterSheet` contains an ordered map `StatId -> StatValue`, explicit inventory,
abilities and conditions. Missing is legal only for an explicitly optional/not-
applicable definition; zero/false are actual values. Inventory has stable entry
IDs, positive quantities and referenced item definitions; consuming the last unit
removes the entry via an explicit operation. Abilities and conditions have IDs,
descriptions and optional typed properties. A condition expiry, if used, is an
explicit world-time value; no rule interpreter secretly expires it.

`SheetPatch` contains explicit add/set/remove operations for stats, definitions,
inventory, abilities and conditions. Reject duplicate targets in one patch,
unknown references, kind changes under an existing ID and invalid bounds before
acceptance. A definition addition includes all newly required sheet values in the
same transaction. These strict operations are separate from the existing tolerant
prose `CharacterDelta` merge: its first-update-wins and blank-means-unchanged
behavior must not be changed accidentally by reusing a new patch validator.

Rolls are narrator-reported resolution evidence attached to accepted actions,
not a claim that engine RNG generated them. Replay stores/reuses their accepted
values. Future scripts can consume the records; no interpreter dependency now.

## 4. Knowledge, OOC and setup questions

`NarratorMemory` separates established `WorldFact`s, hidden intentions, off-screen
outlines and possible future plans. `CharacterKnowledge` holds assertions with
`Known`, `Believed` or `Suspected` status, provenance and discovery position.
Different actors can hold contradictory beliefs. A false belief cannot overwrite
truth, and player views never expose its hidden truth-link or another actor's
private memory. Assertions may be prose; avoid modelling every sentence as logic.

The narrator must respect this distinction semantically. Types enforce routing
and references, not a proof that arbitrary natural-language output has no spoilers.
Streaming remains tentative and has the same semantic limitation. Tests cover
constructed leaks and prompt/context selection; live evaluations remain separate.

### OOC answer contract

`AskOoc { question }` is admitted only with an active living POV, idle generation
and resolved storage obligations. Slash aliases map explicitly to this command;
unmarked text is an ordinary action, not a guessed question mode.

```text
OocAnswer
  explanation: player-safe answer, based on character knowledge
  action_guidance:
    None
    NotAttemptable { explanation }
    Attemptable {
      proposal: PlayerIntent,
      requires_time: true,
      time_estimate: Unknown | Known(FictionalDuration),
      assessment: Unknown | CharacterEstimate { description, basis }
    }
```

An answer cannot carry a fictional mutation patch. Known likelihood need not be
an exact percentage; a justified qualitative estimate is useful. Secret target
numbers/obstacles cannot be revealed just to produce a numeric probability.
An unknown time/likelihood remains unknown. An impossible action is explained
using available information, without disclosing the hidden reason.

Application assigns an `OfferId` to accepted Attemptable guidance and returns it
with the answer. `PerformOfferedAction { offer }` is a separate explicit command:
it validates the current checkpoint/POV and uses the normal action/save path.
Nothing in answering or displaying a prompt runs the clock. Generic "yes" is
not automatically confirmation; presentation submits the offer command only for
an explicit confirmation interaction. On any fictional move/rewind/load the old
offer becomes stale. An intervening local query does not invalidate it.

`OocExchange` belongs to an application-owned persistent conversation archive,
outside the fictional snapshot, with adventure/checkpoint/branch provenance.
Accepted answers increment persisted session revision and trigger save, without
advancing fictional time. Historical discarded-future exchanges remain readable
and labelled historical but are excluded from current narrator context. A failed
or cancelled answer is diagnostic evidence, not an accepted answer in context.

### Candidate questions

`AskCandidate { setup_revision, candidate_id, question }` works during initial
selection, with no established POV. It returns a `CandidateAnswer` explaining
macro-level identity/playstyle using the brief and public candidate descriptions.
Do not send secret biography as necessary public context or accidentally route
this through an Active-only OOC handler. No world action, character choice,
variant collapse or fictional clock change occurs. Same-generation error/cancel
rules apply; changing the setup invalidates stale answers. Archive successful
setup exchanges in setup provenance when the adventure is established; restarting
setup need not acquire a new persistent pre-game save format.

This is not the future brief interviewer. The interviewer is documented as a
non-MVP chatbot role that produces a brief; no current command for it is required.

## 5. Linear active history, branches, rewind and replay

`HistoryTree` stores immutable accepted nodes with a single parent (root excepted).
Each action owns its resolved passage and a sequence of automatic consequence
nodes (death search decisions, successor handoff/end). The active cursor selects
one linear ancestral path. There are no merges between histories in this MVP.
Nodes are immutable; cursor movement and branch creation are atomic aggregate
operations. A sibling's knowledge/sheets never enter current context.

The root is the zero-turn established state. Opening is the first action; rewinding
it returns to AwaitingOpening. `/rewind N` counts accepted story actions, including
Continue/InterestingEvent and opening, not OOC questions or successor attempts.
Rewinding a fatal action skips all its consequence nodes and restores its parent's
complete snapshot. Invalid counts change nothing and dispatch no work.

Action lookup is the parent history node plus checked intent; recorded generation
settings are compared to decide selective reuse. Intent distinguishes `Opening`, `Continue`, `InterestingEvent`
and normalized `Player(PlayerInput)`; selecting a suggestion resolves its current
text into Player intent. Use existing business-text normalization, no fuzzy
paraphrase matching. Keep original literal exchange input in audit independently.
Unchanged settings select an existing child without inference. With changed
settings, preserve established events/resolution except where the change could
reasonably affect them. Presentation-only changes may regenerate prose without
changing facts. Changed narrative severity can alter consequences while preserving
mechanical success; roleplay-dependent adjudication may itself be reassessed.
Retain resolution evidence and affected-consequence dependencies; changed prose
cannot carry unauthorized fictional deltas. Selective regeneration produces a new
recorded branch variant without overwriting the original, with a fresh session
revision and autosave. A different intent is a new action branch as before.

If a recorded consequence chain is unfinished, replay its accepted prefix then
resume only the missing continuation; never fabricate a successful handoff or
consume an extra attempt. Cancellation/failure is not an accepted fictional node.
Finalized branches cannot be overwritten by a later model response. This produces
repeatable recorded outcomes, not deterministic AI generation of unseen outcomes.

Settings metadata distinguishes presentation, adjudication/consequence policy and
active memory limits. Current/original restore still rebinds every snapshot under
LIMITS-001; event-cap changes do not themselves reroll actions or resurrect dropped
events. Replay must respect those caps without pretending a storage rebind changed
what happened. Selective causal replay replaces the earlier blanket settings-key
proposal; see the accepted clarification and C4 in the implementation plan.

OOC context uses persistent conversation branch lineage as well as fictional
checkpoint identity. Rewind seals the old conversation branch and starts a new
lineage anchored at the target; OOC exchanges from its discarded future do not
become current merely because replay later revisits the same fictional node.
An offer is tied to the current lineage/checkpoint and cannot be revived by replay.

## 6. Fictional time and succession

`WorldInstant` and nonnegative `FictionalDuration` are separate checked integer-
second types with checked addition. Calendar labels are optional prose metadata.
Generation wall time and OOC questions do not contribute. Clock updates describe
one global interval; all actors share the resulting world instant. Material off-
screen changes are explicit outlines, not mandatory rewrites of every actor.
Flashbacks don't reverse the authoritative clock.

`TimeAdvance` supplies duration plus optional player-facing explanation. Domain
compares the prior announced cadence and returns `TimeNotice::{First,Changed,None}`;
presentation formats it. First combat round announces the cadence, repeats omit
it, and a longer search announces its new duration. Snapshotting cadence makes
rewind/replay agree with the original display.

Death is accepted and saved before succession work. A proposed precise state:

```text
SuccessorSearch
  death_action: ActionId
  surviving_roster: nonempty ordered CharacterId set
  progress:
    NeedsRanking
    Trying { order: checked permutation, round: First|Second, position }
  rejected_attempts: ordered (round, candidate, reason, audit)
```

No survivors transitions straight to Finished(NoSurvivors). Ranking comes from
the narrator using excitement/intrigue/closure, not player selection. Once ranked,
persist the order; each living candidate appears once per pass. `TrySuccessor`
returns either `Handoff { same_candidate, introduction, suggestions }` or
`Unsuitable { same_candidate, reason }`. Wrong ID/round, duplicate rejection or
invalid/dead handoff is an invalid response; it consumes no slot.

Accepted rejection advances one position, then saves before starting the next.
Pass two reconsiders living candidates for temporary unsuitability using prior
reasons; no artificial time advance/death is introduced to make a retry viable.
Exhausting pass two yields Finished(NoViableSuccessor). Technical failure/cancel
keeps exact cursor and waits for explicit retry. Death never becomes uncommitted.

Handoff changes only POV/introduction/available suggestions and search status;
facts, sheets, clock and actor identity remain those already established. It may
surface existing personal knowledge but cannot invent a new off-screen adventure
during selection. Save before accepting player input. Accepted death/search
states survive a normal restart; no unresolved filesystem attempt journal is
needed for this fictional lifecycle persistence.

Continuing passages carry nonempty `QuickActions`. Fatal/terminal outcomes have
their own enum cases, without irrelevant action lists. Keep `QuickActions`
nonempty and its kind preference/order behavior intact. Opening/turn/handoff/OOC
are distinct output kinds, never coerced into an ordinary malformed turn.

## 7. Application commands, results and lifecycle

One canonical owner has private orthogonal states:

```text
SessionStage = Brief | WorldReview(SetupDraft) | CandidateSelection(Setup)
             | Adventure(Adventure)
Execution    = Idle | Running(OperationKey, RetryIntent) | Cancelling(...) | Faulted
Persistence  = Clean(revision) | Dirty(revision, continuation)
             | Writing(key, prior_status, continuation)
             | FailedKnownUnwritten(...) | Unconfirmed(attempt_handle, ...)
Closing      = Open | Joining | SettlingWrite | FinalSave | Complete(outcome)
```

Constructor/transition checks prevent cross-product nonsense; not every combination
is valid. Deferred opening and succession are stored continuation variants, not
independent booleans. Save failure preserves previous uncertainty during save-copy.

`submit` returns `Rejected(AdmissionError)`, `Applied { revision }`, or
`Started { operation }`. Started is not completed. `advance()` applies available
events and returns bounded notifications, never joins a live worker or performs
filesystem calls. Notifications identify accepted/persisting/durable states;
clients may inspect tentative content without mistaking it for accepted narrative.

Admission errors distinguish WrongStage, Busy, PersistenceUnresolved, StaleChoice,
CreationForbidden, InvalidInput/Count, CounterExhausted and Closed. Queries confer
no mutation authority; commands recheck current admission.

| Command and inputs | Admission | Result / owned continuation |
| --- | --- | --- |
| `SubmitBrief(Brief)` | Brief, idle | Setup generation -> WorldReview |
| `ReplaceOutline(WorldOutline)` | WorldReview, idle | New setup revision; obsolete setup replies cannot apply |
| `AcceptWorld` | WorldReview, idle | Cast generation -> CandidateSelection; display common rules |
| `AskCandidate(SetupRevision, CandidateId, Question)` | Current candidate setup, idle | Public setup answer; remain selecting |
| `SelectCandidate(CandidateId)` | Current candidate setup, idle | Establish roster -> zero-action adventure -> save -> opening |
| `BeginCreation` | CandidateSelection, allowed | Enable concept input without generation or choosing anyone |
| `CreateCharacter(Option<Concept>)` | Creation path, idle | Create and establish other roster -> same save/opening path |
| `CancelCreation` | Creation path, idle | Return to unchanged candidate selection |
| `AttemptAction(PlayerInput)` / `ChooseSuggestion(current_id)` | Active, idle, clean | Replay existing child or resolve new action; accept -> save -> succession if needed |
| `Continue` / `InterestingEvent` | Active, idle, clean | Same action acceptance path with distinct intent |
| `AskOoc(Question)` | Active, idle, clean | Accept archive entry/offer -> save; no fiction change |
| `PerformOfferedAction(OfferId)` | Offer current, Active, idle, clean | Normal action path; re-adjudicate possibility in current fiction |
| `RetryGeneration` | Joined failed/cancelled operation, idle | Same failed intent/base; never replay fatal action to retry successor |
| `Cancel` | Live work or pending automatic continuation | Cancel/join; retain accepted fiction and durable write observations |
| `Rewind(PositiveActionCount)` | Adventure including finished/search states, idle, storage resolved | Restore checkpoint/branch lineage -> save, no model call |
| `Save` / `SaveCopy` | Existing adventure; existing recovery admission | Reconcile exact handle or write snapshot; copy binds only on success |
| `Load(SaveId, Copy, RestoreLimits)` | Existing permitted load admission, no concurrent generation | Validate -> coherent replacement -> save policy/recovery changes |
| `FinishSession` | Any stage | Join generation -> settle write -> bounded final save; report failures |

Lower-level generation commands remain meaningful for application tests/composition;
interfaces do not require presentation to call establishment/save/opening separately.
No unrestricted character editor is inferred; its authority remains for a later
use-case specification. Existing style domain operation remains valid, with replay
settings handled explicitly rather than rewriting accepted history.

### Views and queries

Provide `SetupView`, `StoryView`, `CharacterView`, `AvailableActions`, `ProgressView`,
`PersistenceView`, `OocHistory` and `CandidateConversation`. Fields are player-safe
values or formatted descriptions, never a mutable controller. `AdvancedInspection`
and byte-exact `Diagnostics` are separate explicit queries that may expose secrets.
List/inspect saves retain their existing read-only use-case semantics, even when
the repository method uses `&mut self` for owned execution resources.

### Acceptance and transitions

Before admitting a state-producing request, reserve checked counter capacity for
its immediate save/follow-up and the obligatory final save. Each automatic
succession step repeats that reservation before dispatch; technical retries do
not get an assumed unbounded counter allocation. Counter exhaustion pauses with
an explicit application capacity failure, never a fictional rejection or an
invented end. Existing reserved shutdown authority remains available.

| Trigger | Canonical transition | Save/next operation |
| --- | --- | --- |
| Preview | No change | None |
| Joined current valid action | Exactly one owned accepted snapshot replaces prior | Save then only its continuation |
| Wrong key/base/kind, stale or duplicate | No change | Discard result, retain appropriate evidence |
| Cancellation wins before acceptance | No change | Joined cleanup precedes explicit retry |
| Accepted fatal action | AwaitingSuccessor or NoSurvivors | Save death first |
| Accepted rejection | Advance search cursor once | Save then next attempt |
| Technical successor failure | Same death/cursor | Explicit retry; no budget decrement |
| Accepted OOC | Same fiction; updated archive/save revision | Save; offer awaits explicit action |
| Failed autosave | Accepted state retained, further mutation blocked | Storage recovery, never regenerate |
| Unknown write outcome | Same canonical state plus opaque attempt | Reconcile exact attempt |
| Accepted load/rewind | Fresh session revision, matching new state | Save required change before new work |
| Input/output failure | Preserve exit failure | Safe application closing, then presentation drain |

## 8. Business and execution ports

Domain `StoryGenerator` has typed methods `setup`, `cast`, `candidate_answer`,
`establish`, `create_character`, `action`, `ooc`, `rank_successors`, `try_successor`.
Requests own or borrow only domain values and read-only operation context; results
are distinct checked types with exchange audit. No raw JSON/serde or application
token types enter domain. Each method is independently callable where it has a
business meaning; application owns the sequences. An internal shared transport
method is not the public business interface.

Domain cancellation/preview capabilities are observation-only. Infrastructure
retains concrete application cancellation tokens and scoped wake subscriptions;
the generic domain observation does not replace the supervisor's wake path.
Operation-level domain failures distinguish rejected/invalid/configuration results;
execution failure reports retain the full technical category and evidence.

Application `GenerationExecution` owns requests with operation ID, base revision
and operation kind. `start` returns admitted or explicit start failure retaining
the request for retry; `poll` yields bounded progress or one joined completion;
`cancel`/`close` preserve resource ownership through join. Infrastructure can run
the pure application resolution handler on an owned worker snapshot, returning an
already resolved owned aggregate. The main session accepts it once; it must not
commit the returned passage again. This retains current failure isolation without
large game reconstruction on the event loop. Completion carries separate operation
audit and execution diagnostics on success and failure.

Preserve existing technical categories: Cancelled, Unavailable, Timeout, Transport,
InvalidResponse and Configuration; worker panic/start failure remain separately
located execution faults. Missing response is `None`; an empty body from a real
call is `Some(empty)`. Do not throw away successful stdout/stderr when normalizing
domain results. Transport errors never become fictional successor rejection.

Repository sketch (proposed interface, not compile-tested code):

```text
GameRepository::Attempt: Clone + Send
  write(SaveDestination, SaveSnapshot, AttemptObserver<Attempt>, Cancellation)
    -> WriteOutcome<Receipt, Attempt>
  reconcile(Attempt, AttemptObserver<Attempt>, Cancellation)
    -> WriteOutcome<Receipt, Attempt>
  load(SaveId, Copy, Cancellation) -> StoredAdventure

SaveDestination = Create | Replace { id, expected_stamp }
WriteOutcome = Saved(Receipt)
             | NotWritten(RepositoryFailure)
             | Unconfirmed { attempt, visibility, failure }
```

The observer is required and publishes before first possible mutation. A reported
replacement/durable receipt is not discarded after late cancellation or cleanup
failure: application gets both visibility and execution failure, allowing final
save policy without duplicate replacement. Domain outcomes carry checked business
identity/visibility; execution reports carry helper stage, byte captures, cleanup
and worker fault. Retain `NotWritten` versus visible-but-not-durable distinction
through concrete outcome refinement, not a binary success/error simplification.

`StorageExecution<Attempt>` retains its own preparation capture outside the worker.
Infrastructure's concrete handle privately owns the exact bytes/metadata and can
be passed to a fresh repository. No public byte accessor on the handle. Application
retains it while recovery needs it; no global token registry. Failed save-copy
preserves the previous handle/binding. Repository enumeration uses a separate
application query port with listing types; it doesn't move serialization/helper
types into domain. Existing bounded helper and atomic storage protocols stay.

## 9. Persistence and context

Expanded versioned snapshot includes adventure history/cursor, rules, actors,
knowledge, clock, successor progress and branch-aware conversation archive. Never
persist thread IDs, live cancellation authority or an unresolved filesystem handle
as fictional state. A fictional pending handoff is resumable; an application-
restart write journal remains out of scope. New format may reject v1 development
saves with an actionable unsupported-version error under Q33.

Restore validates parent links/acyclicity, all memberships and typed values,
phase/POV/life coherence, search permutations/cursors, archive provenance and
metadata before replacing session state. Rebuild replay indexes from validated
IDs; no model calls or rerolled mechanics on restore. Current/original limits
policy applies to every retained branch, not just current path.

Request context includes current rules, sheets, narrator/knowledge memory and
selected active-path prose/summary; never unrelated branches or old candidate
variants. Preserve current chapter bridge/retitling and major-event semantics.
Do not introduce automatic model compaction as a hidden retry/extra call.

Existing preview/output bounds and save-byte cap remain. A configurable request-
byte budget is proposed as capacity policy, checked after rendering before launch;
no silently dropped authoritative rules/sheets. Limits for new memory collections
must be measured against representative fixtures in the implementing slice. No
token guarantee is inferred from byte length. If current-chapter prose exceeds
budget, explicit failure is safer than silently changing bridge semantics; an
alternative compaction product behavior requires a separate decision.

## 10. Completion criteria for this design step

The implementation plan maps these types and operations to source, dependencies,
acceptance cases and contract changes. Names are concrete enough for implementation
review but remain proposals. No new test name below is registered as enforced
until a real test exists. Validation for this documentation step is local-link,
whitespace and existing architecture checks; full runtime/mutation/live gates
belong to implementation. Remaining product conflicts are explicit in the plan,
not silently settled by type sketches.
