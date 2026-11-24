# Accepted session architecture and roleplay direction

Follow-up: [source capability inventory and remaining design decisions](../plans/2026-10-30-capability-inventory.md).
Next step: [domain/application contract proposal](../plans/2026-10-30-domain-application-contract.md),
written for review and not yet accepted as an implementation contract.
Technical follow-up: [November 2 refinement](../plans/2026-11-02-technical-contract.md)
and [implementation plan with contract audit](../plans/2026-11-02-implementation-plan.md).
These remain proposals; their technical recommendations are not additional user
decisions simply because they are linked here.

Explicit user decisions from the 2026-10-30 interview, Q1–Q25. Accepted direction;
implementation pending. The detailed use-case specification was authorized but
explicitly deferred. The interview did not authorize starting implementation.
Existing required regressions remain intact; this document claims no new enforcement.

## Architecture and sequence (Q1–Q7)

- Stabilize domain capabilities/data structures and application contracts first.
  Review a use-case inventory (inputs, admission, outcomes and failures) together
  before migration. Questions about intended domain capabilities are welcome.
- Refactor using existing headless play, improve the interactive REPL, then build
  the TUI. A future GUI should reuse the same application capabilities.
- Application owns canonical session state, admission, acceptance, autosave and
  recovery policy. Domain owns business-facing ports; infrastructure owns execution
  resources; presentation handles interaction and terminal concerns. The October 17
  ownership direction stands; concrete interfaces and migration slices await review.
  A single `AdventureSession` type is a proposal, not a required API shape.
- Commands/queries represent product use cases. A new use case may justify adding
  a command/query composing existing capabilities without removing justified older
  operations. Presentation must not orchestrate business policy. Preserve agreed
  domain behavior/public operations; client inconvenience does not authorize casual
  contract changes. Q4's blanket internal-interface replacement permission was
  superseded by this constraint.
- Retain uncertain writes through opaque handles to exact save attempts.
  Infrastructure privately retains prepared contents; application retains/returns
  handles. Evidence survives worker failure so fresh workers reconcile the same
  attempt. Preserve publication-before-mutation, failed-copy retention and existing
  shutdown/diagnostic guarantees. Associated types remain a proposed representation.
  Recovery across application restarts remains outside this refactor.
- REPL eventually exposes all implemented use cases. Advanced editing can require
  exploring commands, help or documentation rather than appearing in the default flow.

## Setup and product goal (Q8–Q14)

The goal is one-on-one roleplay with the AI as storyteller/DM, combining gamebook
mechanics with freedom beyond fixed paragraph choices. Suggestions and free-form
attempts coexist. Success/failure need not map directly to good/bad consequences.

A basic, even vague brief starts world generation. AI fills gaps to make an engaging,
coherent game; rough edges are acceptable. Preliminary clarification conversations
are deferred. Show a concise world introduction and pause for acceptance before
cast generation; editing is an advanced option. Separate internal world information
from the introduction, which respects starting character knowledge. Explicit advanced
inspection/editing may expose internal information outside normal play.

Devising a bespoke ruleset is part of setup. It may include stats, dice and resources
suited to the fiction. For MVP the AI tracks/applies rules and mechanical state,
with machine bookkeeping for convenience and DM discretion to bend rules for drama
or unlucky streaks. Deterministic generic mechanics and AI-scripted, program-enforced
rulesets are future possibilities, not MVP requirements.

Explain common rules before character selection/creation, then show the resulting
character's values/abilities before play. This is information, not an approval step.
Rule discussion/rewrite options are deferred beyond MVP. *L'Homme aux Cent Visages*
and *Sorcellerie!* (*Sorcery!*) illustrate premise-specific systems, preparation,
resources and consequences; their exact rules were not adopted. Explicit mechanical
rules are a new capability beyond the copied Calibre engine's narrative approach.

## Knowledge and OOC questions (Q12, Q15–Q16)

Narration reveals only what the current protagonist knows or perceives. Appropriate
actions such as searching reveal more. Suggestions can express justified hunches
(instinct, knowledge, experience) without revealing hidden information.

Canonical `/ooc <question>` and equivalent `/hrp <question>` explicitly ask the DM
out of character. Answers do not advance fictional time or change fictional state.
If discovery requires action, explain that instead of silently acting. Presentation
passes distinct intent to application; do not infer OOC from wording. Unmarked
questions are in-character speech or thoughts and advance play. OOC need not be
prominent in the default interface.

## Creation, shared timeline and succession (Q17–Q25)

Rulesets/premises decide whether bespoke character creation is available; some
require predefined protagonists. Where allowed, creation is an alternative to
selecting an initial candidate. Only after initiating creation may the player offer
an optional concept/name. The narrator fills unspecified identity, description,
backstory and relationships, and generates/rolls values according to the ruleset.
Creating a character preserves the initial roster and the same succession options.

Follow-up clarification: the candidate roster may mix distinct people and mutually
exclusive versions of one person. Selecting a version establishes that person;
their other versions disappear from the established world. For people not chosen
as the first protagonist, the narrator selects the version that exists, using the
same excitement, intrigue and closure criteria as protagonist succession. Only
the established versions continue in the world and remain eligible successors.
Tracking these remaining characters is already required, not a new user decision.

Everyone shares the same fictional timeline, including ordinary NPCs and alternative
protagonists. An hour passing for one passes for all, without requiring equal activity
or attention. The narrator tracks off-screen developments as outlines, writing full
passages when they become the played story. There is no separate encounter simulator.

Initial alternative protagonists continue together or separately according to genre,
relationships and circumstances. They are narrator-controlled until becoming POV.
This initial roster additionally needs rules/stat bookkeeping and succession eligibility;
do not require full playable sheets or succession support for every ordinary NPC.

Death and succession are required for MVP. The narrator chooses a surviving successor
for excitement, intrigue or closure; the player does not browse/select candidates.
Introduce their journey and effects of the previous POV through what the new
protagonist knows/perceives. Connections can be recognisable to the human without
revealing secrets unavailable to the new character. If no eligible protagonist
survives, the adventure ends.

## Follow-up decisions: explicit state and lifecycle (Q26–Q33)

The MVP supports stats-based rulesets, with explicit rules and a generic,
adventure-defined stats system. Stat definitions use a supported enum of value
types, including numeric and descriptive values and conditions; equipment and
abilities remain explicit records alongside stats. The AI adjudicates and proposes
updates; the program retains accepted state. A future scriptable ruleset may
allow engine enforcement each turn. This is not a promise to express arbitrary
non-stats-based rules. Concrete Rust types and validation are implementation
design to propose, not further product questions.

World truth and character knowledge/beliefs, including uncertainty and mistakes,
must remain distinct. This follows from already accepted character-limited
information; Q28 did not introduce an optional new principle. Concrete memory
representation remains design work.

The narrator chooses action duration, including hours/days, on the common timeline.
Make elapsed time explicit when it changes. Explain repeated cadence once (for
example at the start of combat), then repeat only when that cadence changes.

Death is committed independently of the subsequent succession process. The
narrator rotates eligible candidates automatically when a candidate is fictionally
unable to take over. After two full rotations without a successful handoff, the
death is final and the adventure ends. Technical failures (including timeouts,
invalid responses and backend unavailability) pause for explicit player retry;
they do not consume the fictional rotation limit or end the adventure. No technical
failure resurrects the protagonist or repeats the fatal action. Concrete candidate
ordering, attempt-state representation and race/failure boundaries need specification.

Rewind restores the entire fictional state, including time, sheets, off-screen
developments and POV across death/succession. Established outcomes should be as
deterministic as practical once they have happened; exact replay/RNG semantics
remain to be proposed rather than claiming AI regeneration is deterministic.

OOC exchanges are saved separately from story passages and tied to the fictional
state at which they were asked. Exchanges from a discarded future remain readable
as history but do not inform storyteller answers in the restored state.

There is no requirement for a legacy narrative-only continuation mode or backward
save compatibility for this v0 evolution. This explicit Q33 decision supersedes
earlier blanket preservation language for old saves in this refactor discussion;
it does not authorize weakening unrelated persistence/recovery regressions.
Any actual format change must update its contracts/tests explicitly.

## November 2 review: brief interview, OOC guidance and history

Future, **outside MVP**: offer a conversational alternative to supplying a brief.
An AI interviewer behaves like a regular chatbot, using a quiz/conversation to
discover what the player wants and synthesize a brief for world generation.
Its responsibility is requirements discovery and handoff, analogous to a product
owner preparing work for refinement, rather than narrating or judging the game.
A dedicated subagent is a useful role analogy, not a mandated implementation or
authorization to spawn one now. MVP still accepts a direct brief. Interview
workflow, persistence and handoff details remain for future specification.

OOC answers must never reveal information the character should not have. When
the question requires an in-world action, the narrator explains whether that
action can be attempted and, if the character can evaluate it, how likely it is
to succeed, without using hidden facts to supply an unjustified assessment.
Ask whether the player wants to perform the action, or explain that they could
try it but would need to advance fictional time. Do not perform it or run the
clock without a separate player decision. Merely attemptable does not mean
guaranteed success; unknown likelihood must remain unknown.

Proposal review items 1, 2 and the rewind clarification are accepted:

- Retain abandoned branches and replay the recorded outcome when the exact same
  action is repeated from the same checkpoint, without inference. Different input
  creates a different branch; semantic paraphrase matching is not promised.
- Fictional successor rejection is temporary unsuitability, not automatic death;
  selection advances no fictional time. Persist the cursor across both rotations;
  technical failures pause for explicit retry without consuming a candidate attempt.
- `/rewind 1` undoes one accepted story action and any resulting succession,
  restoring the complete prior fictional state. History forms a branching tree,
  with one linear active path, analogous to version-control history. Rewinding
  does not erase other recorded branches or import their knowledge into this path.

Review item 3 is settled by the subsequent clarification: at initial character
selection, show the player's brief and concise descriptions of the candidates.
The player may optionally ask the narrator questions to understand who a candidate
is at a macro level. Keep disclosure sparse, comparable in intent to the user's
Baldur's Gate 3 premade-character selection example; it is not a promise to copy
that game's interface. Do not reveal secret biography, private knowledge or future
events in candidate descriptions or answers. These questions are setup inquiries,
not in-world actions or a preliminary brief-building interview. Detailed handling
is to be specified as part of selection, not inferred to be unavailable merely
because no protagonist has been chosen yet.

Initial selection is not browsing simulated ongoing adventures. The narrator
chooses a successor after death, with no player-facing candidate-selection step.
The chosen POV's personal knowledge is introduced after selection/handoff as needed.
Earlier common-premise-introduction wording must not be interpreted as requiring
extra world exposition beyond the brief and concise character descriptions at
the selection step. The separately agreed concise world-review step remains.

No approval of the entire technical proposal or implementation is implied.

## Replay after settings changes: accepted clarification

Changing settings must preserve established events and outcomes where those
settings would not reasonably affect them. Prose may be rewritten to reflect
the new narrative style. Do not treat any settings change as permission to
regenerate the entire causal chain.

Success/failure generally follows mechanics and stats: a mechanically successful
action remains successful after switching to a less forgiving narrator. Its
consequences may change where the new narrative severity reasonably affects
them. For roleplay-dependent adjudication, the change can justify revisiting
success itself. Distinguish prose presentation, adjudication and consequences;
do not collapse all three into an undifferentiated style/replay cache key.

Preserve unaffected facts and reevaluate only affected consequences and their
dependent future. The previous branch remains recorded, never overwritten.
This supersedes technical proposal C4's blanket new-branch generation rule.
Concrete causal evidence and selective-replay validation remain technical design
work; no claim that the MVP engine already enforces these distinctions.

## Lean MVP clarification

Following the comparison with Calibre, the user requested a lean MVP rather than
engine-enforced selective causal replay. This supersedes the preceding clarification
as an MVP implementation guarantee: restore complete recorded state on rewind;
replay recorded results for the same action with unchanged settings; changed
settings permit a newly generated continuation with instructions to preserve
established mechanical results where appropriate. That preservation is AI guidance,
not a mechanically proven causal invariant. Defer dependency graphs, automatic
causal invalidation and a dedicated prose-only regeneration workflow.

Other previously agreed capabilities have not individually been removed by this
scope instruction. Their priority/deferment needs an explicit scope agreement;
do not silently discard succession, stats or OOC behavior under the word "lean".

## Confirmed MVP versus future roadmap

The user confirmed preserving sophisticated requirements intact for the future,
outside the lean MVP, and excluded everything beyond REPL from MVP. The
[scope split](../plans/mvp-and-roadmap.md) records the accepted lean feature set
and deferred features, including succession, retained branches/replay, richer
character creation/variants, formal fact/belief records and all later presentations
and export. This supersedes earlier wording requiring those features for MVP.
The sophisticated selective-replay clarification remains the future requirement;
the simpler intermediate replay proposal is not a replacement future specification.
Current MVP requires no replay cache. Existing implemented guarantees stay intact.

## Contract impact and unresolved design (after scope split)

This direction supersedes contrary future sketches, not existing regression evidence.
Architecture changes affect ARCH-001, ARCH-002, TEXT-001, STREAM-001 and PRODUCT-001.
New roleplay capabilities extend PRODUCT-001. Preserve and explicitly reconcile
STATE-001 selection ownership, IDENTITY-001/002, LIMITS-001 and ACCEPTANCE-001
with the extended lifecycle; do not weaken tests opportunistically.

Still pending: the use-case and domain/application contract proposal, concrete
ports/state structures, stats/knowledge/off-screen memory representation,
context retention limits, succession attempt details, replay precision, save-format
transition, cancellation/failure boundaries and actual domain/boundary/orchestration
tests and mutations. The follow-up decisions above settle product-level time,
rewind, OOC history and succession failure policy, not their technical design.
Update contracts and required-tests.json with implementation and
run the shared contract gate; no new feature is enforced merely by this record.
