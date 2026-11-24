# Capability inventory against the accepted roleplay direction

2026-10-30. Source baseline: `f433d3d23c07d86162e122419c3ade14950c2599`
plus the current uncommitted documentation. Requirements:
[accepted interview decisions](../decisions/2026-10-30-roleplay-and-session.md).
Architecture context: [October 17 proposal](session-architecture-refactor.md).

This is a source and registered-test inventory, not the deferred use-case
specification, an approved API design, or implementation. No runtime/backend calls
or test suites were run for this inventory. Existing test references identify
coverage present in the repository, not newly observed passing results.
Documentation validation: `git diff --check` and
`bash scripts/check_architecture.sh` passed. The full contract/mutation gate was
not run because this work changes documentation only, with no implementation or
registered-test changes.

## Findings

There is a substantial reusable foundation: validated world/cast/turn values,
generation through both subprocess adapters, bounded narrative previews, canonical
acceptance checks, persistence/recovery and a working line-oriented headless loop.
Session policy and worker execution currently live in presentation, so the planned
application boundary still needs to be established.

Most newly agreed roleplay capabilities have no explicit representation or use
case yet. Free-text memory can incidentally describe inventory, death or off-screen
events, but that is not support for their required bookkeeping or lifecycle.
The largest domain change is replacing an adventure tied to one fixed selected
character with a shared-world adventure having persistent actors and changing POV.

Two especially important mismatches were found:

1. Cast instructions may generate alternative *versions of the same central person*.
   Follow-up decision: mixtures are allowed. Choosing a version eliminates that
   person's other versions; the narrator resolves unchosen people's versions using
   the same excitement/intrigue/closure criteria as successor selection. The model
   needs to distinguish mutually exclusive variants from distinct people.
2. Unchosen candidates remain in the saved world cast, but are absent from initial
   summary memory. The selected character receives the fixed ID `protagonist`.
   Succession is therefore more than adding a death flag or changing the selected index.
   Tracking the established roster is an implementation obligation already settled
   by the interview, not a request for another product decision.

## Capability map

| Requirement | Current support and evidence | Gap / affected boundary |
| --- | --- | --- |
| Brief initiates world generation | `StoryUseCases::generate_outline`; `WorldOutline` has title/description; bundled prompt invents concrete details from a brief | Already structurally supported. Quality for vague briefs remains a model-evaluation concern, not a proven invariant |
| Review world before cast | `SessionController::{replace_outline,accept_outline}`; headless empty line accepts and `/edit` replaces it | Preserve sequencing; replace full-description display with the agreed concise introduction |
| Hidden world versus player introduction | One `WorldDescription` in outline, summary and generation DTOs; headless prints it directly | New domain vocabulary, generation result and player-safe application view needed |
| Explain bespoke rules before selection | No ruleset field or setup operation in generation port, domain or save DTOs | New setup capability and persistent rules memory; information-only presentation, no approval workflow |
| AI-managed stats/resources | Character details contain description, backstory, relationships and free-text current situation | No explicit sheet, resource record or rules update contract. Decide representation and validation without building an executable rules engine |
| Choose an existing protagonist | `World::select` returns owning `SelectedWorld`; selection is checked against its cast | Reuse selection invariant; selection currently fixes the game's protagonist permanently |
| Optional bespoke character creation | `PlayerCharacter::new` constructs a value but there is no creation use case, generation request or roster insertion operation | Ruleset capability flag/procedure, optional concept and integrated creation path needed; preserve initial candidates |
| Player-directed turns and suggestions | `TurnDirection::{Continue,Player,InterestingEvent}`, `QuickActions::select`, `/action N`, free text | Reusable. Rule-governed resolution and character-grounded hunches are new prompt/state requirements |
| Character-limited information | Full summary, world and recent prose go to storyteller; descriptive fields have no visibility/knowledge classification | New knowledge model and player-safe views. Display filtering cannot establish that generated prose reveals no secrets |
| `/ooc` and `/hrp` | Neither command exists; unknown slash commands are rejected; ordinary input becomes turn direction | Distinct application/generation operation and response needed. Must bypass turn commit/time advance, retain cancellation/error behavior |
| Shared fictional time | Accepted turn ordering exists; no fictional clock, elapsed interval or shared-time update contract | New timeline vocabulary and update rules; turn count is not elapsed fictional time |
| Off-screen NPC activity | Character delta can update free-text current state; no required coverage of off-screen actors or interval | Reuse delta/summary ideas, specify persistent outline memory and synchronization semantics |
| Initial protagonist roster stays active | Original candidates persist in `WorldCast`; `initial_summary` includes only selected protagonist plus generated NPCs | Give roster members persistent identities and live memory; stop treating unchosen candidates as dormant alternatives |
| Death, succession, terminal adventure | `Stage` ends at `Playing`; `StoryTurn` requires narrative and 1–3 quick actions; no life/eligibility/end status | New legal states/transitions, narrator-selected successor and handoff result; terminal turns must be reconciled with current action invariant |
| Save/load/list/inspect/copy/reconcile | Application persistence use cases, filesystem/supervised repositories and headless commands implemented | Preserve storage machinery; extend codec for new game state and define legacy compatibility |
| Rewind | Truncates accepted turns; summary/chapter/prose derive from log; saves canonical result | New POV, time, sheets and off-screen outlines must rewind together. No policy yet for OOC history or succession boundaries |
| Editing | World-outline replacement exists; domain style setter exists; character edit operation is explicitly unported | Full character editing is a missing capability, not merely a hidden REPL command. Need specified persistence/rewind semantics |
| Comfortable interactive REPL | Responsive Linux line input, slash help, errors/retry, save commands already implemented | No explicit line-editing/history/completion facility in presentation dependencies; these are interaction enhancements, not missing game rules |
| TUI / GUI | Presentation-independent generation/persistence primitives exist; current headless session abstraction exposes presentation controller | Shared application session interface still pending. No TUI/GUI implementation claimed |
| Export and other prior scope | Export not implemented; image port is a seam; arbitrary template overrides remain blocked | Existing pending scope remains; new interview did not cancel export or authorize overrides |

## Concrete ownership and data constraints

### Domain

- [world.rs](../../cyoa-core/src/world.rs): `PlayerCharacter` has name, description
  and backstory, but no ID, relationships, sheet or life status. `NonPlayerCharacter`
  additionally has relationships. `WorldCast` requires a configured playable
  minimum during generation; restoration preserves established records without
  reapplying generation limits. A fixed-protagonist premise must be reconciled
  with these minimums rather than silently weakening LIMITS-001.
- [game.rs](../../cyoa-core/src/game.rs): `GameState` stores `SelectedWorld`, style,
  turns and original/active limits. `protagonist()` resolves the original selected
  world record. `initial_summary()` assigns `CharacterId::protagonist()` to that
  person and creates identities only for NPCs; other candidate protagonists are
  omitted. No public transition switches POV or marks the adventure finished.
- [summary.rs](../../cyoa-core/src/summary.rs) and
  [character.rs](../../cyoa-core/src/character.rs): stable IDs, identity repair,
  ordered memory and delta merges are reusable. A `CharacterCast` is nonempty;
  dead characters need not be deleted to express no survivors. There is no
  explicit alive/eligible distinction today. Ordinary summaries do not separate
  world truth, beliefs, character knowledge or hidden intentions.
- [turn.rs](../../cyoa-core/src/turn.rs): every `StoryTurn` requires quick actions.
  An adventure-ending result cannot be represented as a zero-action turn. Stored
  `TurnRecord` summaries make rewind useful, but currently contain none of the
  new independently represented state. Using prose to imply death does not create
  a reliable terminal transition.

### Application and execution

[generation.rs](../../cyoa-application/src/generation.rs) has three business
capabilities: outline, cast and turn. It mixes their signatures with application
cancellation/transport evidence. Moving declarations into domain requires inward
vocabulary for every signature type, not imports back into application. Preserve
the observed-cancellation checks and unchanged owned game on turn failure.

[persistence.rs](../../cyoa-application/src/persistence.rs) owns `GameRepository`,
save commands/queries, `PendingWrite` with accessible encoded bytes and the required
preparation sink. The opaque handle is an evidence-hiding migration of a real
existing recovery mechanism, not a reason to replace it with a new attempt.

Presentation currently owns:

- [session.rs](../../cyoa-presentation/src/session.rs): canonical lifecycle,
  request/revision checks, admission, retry and result acceptance.
- [runtime.rs](../../cyoa-presentation/src/runtime.rs): dispatch and coordination.
- [persistence.rs](../../cyoa-presentation/src/persistence.rs): autosave, bindings,
  uncertain writes, deferred opening, final-save policy and output-drained acknowledgement.
- [worker.rs](../../cyoa-presentation/src/worker.rs) and
  [storage.rs](../../cyoa-presentation/src/storage.rs): threads, joining, bounded
  previews and evidence capture outside the failing storage worker.

Move policy into application, execution into infrastructure, and retain output
draining in presentation. Existing read accessors can inform queries but exposing
the controller as the new API would preserve the ownership problem. Generator
reuse across jobs matters for the demo; repository reconstruction must retain
uncertain-attempt evidence.

### Generation and persistence boundaries

[Bundled cast instructions](../../cyoa-infrastructure/src/generation/defaults/prompts.toml)
explicitly allow variants of a central person. Both generation and save character
DTOs mirror the older fields. No instruction change alone can preserve live roster
state through saves or establish a mutable POV invariant.

[Template rendering](../../cyoa-infrastructure/src/generation/templates/rendering.rs)
sends full summary and recent chapter prose. All adapters are stateless between
requests: any AI-maintained rules, sheets or private outlines must be explicitly
stored and supplied again. Current major events are capped, but the full current
chapter and other text fields do not establish a total context/token bound. The
new memory design needs an explicit retention policy, not a claim that summaries
already make every prompt size bounded.

[Save DTOs](../../cyoa-infrastructure/src/persistence/dto.rs) store one selected
index and per-turn snapshots. [Migrations](../../cyoa-infrastructure/src/persistence/migrations.rs)
support format 1 only; the step dispatcher is a scaffold with synthetic tests,
not an implemented upgrade from existing saves to this expanded game model.
Older saves cannot supply historical mechanical rolls, hidden knowledge or
off-screen adventures that were never recorded. Defaults must not invent evidence.

New schemas require fresh backend compatibility evidence. Preserve ARCH-003:
vendor-specific tolerance stays in each adapter. Prior live gates cover their
recorded versions and old bundled requests, not these future schemas. Codex
complete-only and Claude preview behavior remain equally supported; this inventory
performed no new authenticated calls and changes neither backend reference's status.

## Existing evidence to preserve and extend

[required-tests.json](../decisions/required-tests.json) registers existing checks.
Representative groups include:

- Domain selection ownership/compile-fail cases, namesake preservation, stable
  identity repair, snapshot restoration, chapter behavior and quick-action selection.
- Presentation `session` tests such as
  `obsolete_cast_cannot_replace_an_edited_outline_and_actions_are_checked` and
  `stale_and_duplicate_completions_never_replace_current_state`.
- Presentation `persistence` tests for failed autosave, uncertain-attempt
  reconciliation, failed-copy retention, load isolation, initial zero-turn save,
  deferred opening, cancellation and bounded shutdown.
- Application persistence tests for explicit commands/queries and the caller's
  preparation sink; storage-runner tests for recovery after worker panic.
- Full controller/headless stories and both adapter fixture paths; separate live
  backend evidence in reference files and dated reviews.

These do not test the new roleplay requirements. Add independent domain, boundary,
application-transition and persisted acceptance cases when implementing them.
Move tests with their assertions and registry entries. Update handwritten mutation
targets when code moves; preserve the domain-only automated mutation scope and
workspace Nextest/Cargo doctest split. No mutation gate should become optional.

Affected decision IDs: ARCH-001/002/003, PRODUCT-001, STATE-001, IDENTITY-001/002,
TEXT-001, LIMITS-001, PROMPTS-001/002/003, STREAM-001, BACKENDS-001,
ACCEPTANCE-001 and TESTING-001. Some are preservation obligations, not requests to
change their behavior. In particular, opening identity review must remain one
addition to the existing opening call unless a reviewed new lifecycle changes it.

## Decisions needed before the contract can be stabilized

Follow-up interview status: D1–D8 now have product direction recorded in the
[accepted decisions](../decisions/2026-10-30-roleplay-and-session.md), including
Q26–Q33. The table below preserves the original inventory questions/recommendations;
its proposals do not override those answers. D2 uses explicit adventure-defined
typed stats; D3 truth/beliefs separation was already required; D4 durations are
narrator-chosen and announced when cadence changes; D5 death precedes separate
succession, with two fictional rotations and technical failures paused for explicit
retry; D6 rewinds the whole fictional state; D7 retains OOC history outside current
fictional context; D8 requires no v0 legacy continuation mode. Concrete models,
replay guarantees and transition semantics remain proposal work. D9 editing
permissions can be addressed with those use cases; D10 limits are technical design
to propose. Next step: domain/application capability and contract proposal, not
another interview repeating these settled principles.

These are unresolved questions exposed by source inspection, not newly accepted
requirements. Recommendations are proposals for the next interview.

| Decision | Why it matters | Recommendation for review |
| --- | --- | --- |
| D1: Roster variants — settled in follow-up | Distinct people and mutually exclusive versions may coexist as candidates | Establish the selected version; narrator resolves other people's versions with succession's excitement/intrigue/closure criteria. Eliminate alternatives and track the established people; concrete identity representation remains implementation design |
| D2: What is authoritative mechanical memory? | AI management still needs durable, recoverable state and consistent names/types | Store persistent rules plus explicit per-character sheets, with AI adjudication and structural validation; decide how flexible sheet values need to be before choosing Rust types |
| D3: What does a hidden fact/character knowledge record contain? | One world string cannot produce trustworthy player views, OOC answers and successor introductions | Separate narrator memory and character-facing knowledge; decide facts versus beliefs/uncertainty and what selection may reveal |
| D4: How is fictional elapsed time expressed? | Everyone shares time, but turns can span seconds or days | A single world timeline with narrator-reported advancement; decide useful precision and handling of flashbacks rather than adding per-character wall clocks |
| D5: Are death and handoff one accepted result or separate operations? | Failure between death and introduction must leave a valid resumable adventure | Preserve an accepted death even if successor-introduction generation fails; specify explicit retry and whether an awaiting-handoff state is needed |
| D6: How does rewind cross handoffs and other new state? | Current rewind removes story turns only | Restore the whole shared-world snapshot, including POV/time/sheets, when undoing its accepted action; specify OOC transcript retention separately |
| D7: What remains of OOC exchanges? | No fictional change does not imply no conversational memory or storage | Separate OOC exchanges from narrative turns and fictional time; decide persistence/context/rewind policy and incomplete/failed-answer handling |
| D8: How do old saves continue? | They lack state the expanded game requires | Preserve read compatibility and choose an explicit legacy continuation or upgrade policy; never silently fabricate historical simulation |
| D9: What can advanced character editing change? | Only outline editing exists; Calibre's durable-versus-current edit policy is historical, not implemented here | Specify identity, rules-state and past-snapshot editing authority before adding commands |
| D10: What are memory limits and consolidation rules? | Whole roster, secrets, rules and OOC memory add context | Define retention and consolidation that cannot erase authoritative rules, actor identity or unresolved recovery state; numerical budgets remain to be chosen |

## Recommended next work

1. D1 is settled. Discuss the independent decisions D2–D4; their answers shape later
   succession, history and save questions. Do not reopen AI-managed versus
   program-enforced mechanics: that choice is already settled for MVP.
2. Resume the deferred use-case specification when requested. Include the current
   successful paths and every existing admission/recovery guarantee alongside new
   setup, creation, OOC, timeline and succession paths. Exact method names are not
   user decisions unless they encode a product distinction.
3. Review the domain/application contract before migration. Refine the October 17
   migration proposal around those decisions; distinguish ownership-only moves
   from new behavior and save-format changes. Preserve one canonical session.
4. Implement dependency-ordered slices only after that review: establish inward
   vocabulary and opaque evidence; migrate execution and session policy; add new
   domain/application capabilities with save support; expose them through headless
   REPL and then improve interactive ergonomics. TUI follows that application seam.
5. Run `bash scripts/check_contracts.sh` at implementation completion, including
   both mutation gates with pinned tools. This source inventory requires no new
   live inference, dependency installation or runtime feature work.
