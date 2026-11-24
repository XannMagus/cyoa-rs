# REPL MVP and future roleplay roadmap

**Subsequent correction:** the working headless REPL is already the lean MVP.
The [iso-functional cleanup](iso-functional-cleanup.md) is complete. The
enhancements listed below remain roadmap material rather than prerequisites for
declaring an MVP, including basic stats/OOC additions.

Scope decision, 2026-11-02. This is the current scope split, superseding earlier
documents that describe every agreed roleplay feature as required for MVP.
The [full decision record](../decisions/2026-10-30-roleplay-and-session.md) retains
the detailed requirements. **Deferred means preserved for future work, not removed
or weakened.** No feature implementation is claimed or authorized by this record.

## MVP: a convenient playable REPL

- Existing brief -> world review -> cast selection -> play, free-form actions and
  suggestions, both co-equal inference backends, save/load, cancellation and explicit
  retry. A vague brief is enough; no mandatory interview.
- Application/presentation separation sufficient for the shared use cases and
  REPL; preserve existing storage, identity and cancellation guarantees.
- Explicit generic typed stats and basic AI-managed rules/bookkeeping. No engine
  enforcing the rules or generic scripting runtime.
- Character-limited narration, separate narrator/player-facing memory and OOC
  questions. Preserve the agreed explicit `/ooc`/`hrp` intent, no time advance,
  attemptability/character-justified likelihood guidance and separate explicit
  decision before any offered action is performed.
- Concise initial character descriptions and optional macro-level questions;
  no requirement for bespoke creation or mutually exclusive candidate groups.
- Straightforward rewind restoring recorded fictional state. After rewind, new
  generation can vary. Retained branches, outcome caches and selective causal
  replay are not needed to call the MVP complete.
- REPL interaction and help sufficient to use these capabilities conveniently.
  Advanced editing gets its own authority specification before it is exposed;
  do not build an unrestricted editor just to satisfy the word REPL.

Narrator knowledge discipline is required; a formal fact/belief graph is deferred.
Keep machine validation honest: structural bookkeeping checks do not prove the AI's
arithmetic, adjudication or spoiler avoidance correct. Existing implemented
contracts/tests remain mandatory; this scope split is not permission to weaken them.

## Future roadmap: requirements remain intact

| Capability | Preserved requirement |
| --- | --- |
| Conversational brief builder | Optional chatbot-style interviewer discovers preferences and produces a brief for world generation; dedicated role/subagent is an implementation possibility |
| Rich character setup | Ruleset-dependent custom creation with optional concept only on that path; mixed distinct people and mutually exclusive variants; narrator resolves unchosen variants using excitement/intrigue/closure |
| Shared-world protagonist roster | Initial potential protagonists continue on one fictional timeline, together or separately; track off-screen outlines and rules state; ordinary NPCs need not all have playable sheets |
| Death and succession | Death commits independently; narrator chooses successor, no player browsing; knowledge-safe handoff; two fictional rotations, technical failures pause for explicit retry without consuming attempts; no survivor or exhausted search ends adventure |
| Formal knowledge model | Explicit truth versus beliefs/suspicions/uncertainty and per-character discovery, supporting deception and viewpoint changes |
| Branching history and exact replay | One linear active path in a retained history tree; repeat exact action under unchanged circumstances reuses recorded results; complete rewind includes time, sheets, off-screen world and succession |
| Selective replay after settings changes | Preserve unaffected events/mechanical success; permit prose changes; severity may change consequences and roleplay adjudication may alter success when justified; reevaluate affected dependent future while retaining original branch |
| Scriptable enforced rules | AI can devise rules expressible through a future supported script system; engine enforces mechanics per turn, storyteller retains its agreed DM role |
| TUI and GUI | Alternative presentations of shared application features; aesthetic/discoverability improvements don't move game policy into presentation |
| Export | Markdown/EPUB remain intended future work, outside REPL MVP |
| Other later polish/images | Existing future configuration/import/help/style/editing and disabled image seams remain documented; no need to implement nonessential additions to finish MVP |

The full [technical proposal](2026-11-02-technical-contract.md) and
[slice plan](2026-11-02-implementation-plan.md) remain design material for this
roadmap. They are not an all-or-nothing MVP work order. Update the implementation
sequence to select the MVP subset before starting feature work. No inference that
future requirements are cancelled simply because their implementation is deferred.
