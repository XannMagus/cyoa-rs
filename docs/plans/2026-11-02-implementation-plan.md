# Implementation plan for review

**Current scope authority:** [REPL MVP and future roadmap](mvp-and-roadmap.md).
The advanced slices below remain future design material, not MVP obligations.
Earlier notes about deferrals awaiting agreement are superseded by that scope split.

**Scope update:** user requested a lean MVP after the Calibre comparison. The
[decision record](../decisions/2026-10-30-roleplay-and-session.md) now defers
engine-enforced selective causal replay. C4's dependency-tracking recommendation
and corresponding selective-replay work are superseded: preserve exact recorded
replay only with unchanged settings; use narrator guidance for changed-settings
continuations. Broader feature deferrals require explicit scope agreement before
this slice plan can be treated as the MVP implementation order.

Status: **proposed**, 2026-11-02. No implementation authorization inferred.
Baseline: commit `f433d3d23c07d86162e122419c3ade14950c2599` plus the current
uncommitted documentation. Detailed design:
[technical contract](2026-11-02-technical-contract.md). Product authority:
[accepted decisions](../decisions/2026-10-30-roleplay-and-session.md).

## Readable plan

The work has two tracks in sequence. First move the existing session coordination
and worker execution into the agreed layers while keeping current play working.
Then introduce the new shared-world model and expose its capabilities through the
headless interface. Comfortable REPL editing/history/completion follows those
capabilities; the TUI follows the same application interface.

The new model explicitly stores rules and typed sheets, established actors with
stable IDs, truth versus character beliefs, shared time, life/POV transitions and
branching history. The AI proposes fictional outcomes; the program accepts checked
records and owns saving/recovery. Setup character questions and OOC questions are
separate use cases. OOC guidance can offer an action, but never performs it without
explicit player intent. Death is saved before the independent succession process.

Retained branches make recorded actions replayable without inference. All state,
including off-screen developments and POV, rewinds together. Succession uses a
persisted narrator-ranked order and two fictional rotations; technical failures
pause on the same candidate for explicit retry.

The replay/settings review is now settled: preserve established outcomes except
where changed settings reasonably affect them, allowing prose rewriting and
selective consequence regeneration. Mechanical successes generally remain fixed;
roleplay-dependent adjudication may be revisited. C4 records the resulting design work.

## Slices and dependencies

These are reviewable implementation slices, not tickets already created. Each
has a concrete stopping point, required evidence and no hidden permission to
weaken tests. The suggested new test names are acceptance specifications only,
not registered tests or claims of existing enforcement.

| Slice | Depends on | Scope and completion evidence |
| --- | --- | --- |
| R1 Inward business vocabulary | Plan approval | Map every existing generation/repository signature type; separate domain operation outcome from application execution evidence. Compile all crates/tests without changing behavior |
| R2 Opaque save attempts | R1 | Associated attempt type with private infrastructure backing, required publication and external capture; existing exact-write recovery still passes |
| R3 Execution placement | R1–R2 | Move generation/storage threads into infrastructure behind application execution ports; preserve demo-generator reuse, nonblocking poll, cancellation wake and joining |
| R4 Application session | R3 | Move canonical state/admission/autosave/recovery/close policy into application; headless uses commands/views; output draining remains presentation. Remove superseded controller ownership |
| F1 Domain records | R4 | Checked typed stats, groups/variants, actor IDs, knowledge, snapshots/time/history and legal outcome states; independently tested without vendors/storage |
| F2 Expanded codec | F1 | New versioned save envelope for expanded model, strict restore and branch validation; explicit v0 format transition; existing atomic storage and recovery unchanged |
| F3 Setup and establishment | F1–F2 | Brief -> concise world review -> rules/candidates; optional macro-level character questions; conditional creation; narrator resolves variants; save zero-action state before one opening |
| F4 Shared-world action | F3 | Atomic action/sheet/time/knowledge/off-screen acceptance, cadence notices, continuing/fatal outcomes and autosave through headless |
| F5 OOC and offered actions | F4 | `/ooc` and `/hrp`, knowledge-safe answers, attemptability/estimated likelihood, explicit separate action confirmation, persisted exchange ancestry |
| F6 Branch rewind and replay | F4–F5 | Whole-state action rewind, retained branches, exact recorded replay, stale-offer rejection, discarded OOC exclusion, save/reload evidence |
| F7 Succession | F4, F6 | Accepted death and persisted search, narrator order, two rotations, technical-failure pause, handoff/end, rewind/replay through all intermediate states |
| F8 End-to-end parity gate | F3–F7 | Expanded scripted demo and both real adapter fixture paths; own authenticated backend live evidence, handoff for other backend; full contract/mutation gate |
| P1 REPL ergonomics | R4, F8 | Line editing/history/completion/help around working application use cases; public editing capabilities only after their own specified authority |
| P2 TUI | P1 | Shared application interface, play screen then lifecycle; preserve prior TUI/export/image pending obligations |

F1 defines target domain contracts first; F2–F7 wire capabilities without redesigning
those contracts casually. Keep dependent incomplete paths inaccessible until their
own acceptance is ready. No second permanent legacy game model. Temporary internal
feature staging must not expose half-supported new saves as stable gameplay.

R1–R4 are behavior-preserving architecture work. F1 onward deliberately changes
the game/format and requires contract amendments grounded in the accepted decisions.
R1–R4 preserve current fixtures, including the nine-request Phase 0 scenario.
The expanded scenario is additional coverage; its request count is independently
specified, not silently substituted for the old count during refactoring.

## Acceptance specifications by slice

### R1–R4: preserve the working product while moving ownership

- R1: `business_ports_import_no_execution_or_vendor_types`; category mapping for
  each Cancelled/Unavailable/Timeout/Transport/InvalidResponse/Configuration error,
  empty-versus-missing response and successful diagnostics.
- R2: `attempt_is_published_before_mutation_and_survives_worker_panic`;
  `fresh_repository_reconciles_exact_handle_without_new_metadata`;
  `failed_copy_preserves_previous_uncertain_handle_and_binding`; compile-fail
  example proving application cannot access encoded attempt contents. Replaced
  visibility must remain distinguishable from durability and cleanup success.
- R3: `poll_returns_while_worker_is_provably_blocked`;
  `idle_child_cancel_wakes_reaps_and_joins_before_retry`;
  `demo_generator_state_survives_multiple_jobs`; Drop cleanup remains bounded by
  existing transport policy. No scheduler-luck-only latency assertions.
- R4: retain all existing presentation session/persistence assertion coverage in
  application location; `final_save_success_does_not_erase_prior_io_or_worker_failure`;
  `application_completion_does_not_wait_for_terminal_output`. Existing startup/load,
  deferred opening, reserved counters and stale/cancelled acceptance still tested.

### F1–F2: establish valid records and restoration

- `stat_values_match_definition_and_resource_bounds`: every kind, decimal equality/
  overflow, optional versus missing, false versus absent, invalid choice, duplicate
  patch targets and atomic definition+sheet additions.
- `variant_groups_keep_namesakes_and_establish_exactly_one_version_per_person`;
  `pov_changes_preserve_actor_ids_and_ordinary_npcs_need_no_roster_sheet`.
- `false_belief_never_replaces_world_truth_or_leaks_through_player_view`;
  `active_phase_requires_living_roster_member_and_finished_can_have_nonempty_cast`.
- `restore_rejects_cycles_bad_references_incoherent_pov_and_invalid_search_cursor`;
  `restored_archive_and_all_retained_branches_roundtrip_without_inference`;
  `unsupported_development_format_fails_before_auth_or_session_replacement`.
- Retain byte-exact audit, strict duplicate-key handling, save metadata checks,
  current/original restore policy and all local/supervised storage crash tests.

### F3–F4: complete setup and one shared-world action

- `candidate_questions_need_no_pov_and_cannot_choose_or_collapse_variants`;
  `setup_revision_rejects_old_answers_after_outline_change`;
  `predefined_only_rejects_creation_before_inference`.
- `created_protagonist_preserves_initial_successor_roster`;
  `selection_exposes_brief_and_short_cards_not_secret_biographies`;
  `established_zero_action_save_precedes_one_opening_and_failed_save_blocks_it`.
- `action_accepts_sheet_clock_knowledge_and_offscreen_changes_atomically`;
  `failed_cancelled_or_stale_action_preserves_complete_snapshot`;
  `cadence_notice_occurs_initially_and_only_when_duration_changes`.
- `fatal_outcome_has_no_fake_actions_while_continuing_requires_quick_actions`;
  preserve kind diversity, unknown-action-kind tolerance, chapters/retitling,
  bridge context, blank/nullable prose delta semantics and limits.

### F5–F7: OOC, history and death

- `ooc_aliases_share_intent_and_unmarked_question_is_an_action`;
  `ooc_can_offer_attempt_but_never_changes_clock_or_sheet`;
  `unknown_likelihood_and_secret_obstacle_do_not_become_exact_probability`;
  `perform_offer_requires_explicit_current_offer_and_normal_action_admission`.
- `ooc_save_failure_preserves_accepted_answer_and_never_runs_offered_action`;
  `rewind_discards_future_ooc_context_even_when_replay_revisits_same_node`.
- `rewind_one_restores_action_parent_and_all_successor_consequences`;
  `repeated_exact_action_reuses_stored_outcome_without_generator_call`;
  `different_action_branches_without_sibling_memory_or_overwrite`;
  `partial_replayed_chain_resumes_only_unrecorded_continuation`.
- `death_is_saved_before_any_successor_request`;
  `two_complete_fictional_rotations_finish_once_and_preserve_attempt_evidence`;
  `timeout_invalid_response_cancel_and_spawn_failure_do_not_consume_attempt`;
  `no_survivors_ends_without_ranking_call`;
  `handoff_keeps_clock_actor_identity_sheets_and_existing_knowledge`.
- `restart_after_each_accepted_cursor_step_continues_from_exact_candidate`;
  `failed_cursor_autosave_blocks_rotation_until_reconciliation`.

### F8: complete playable evidence

Use a frozen scripted adventure with two candidate versions of one person, a
distinct namesake, ordinary NPCs, optional creation, false beliefs, numeric/resource
and nonnumeric stats, off-screen consequences, OOC action guidance, a failed
generation, rewind/replay and death/handoff through a failed candidate attempt.
Acceptance asserts current context and disk state, not only displayed narrative.
Include no-survivor and two-exhausted-rotation terminal scenarios separately.

Replay with chunked and complete-only generators, both production adapters over
synthetic children, and shipped headless commands. These are not live observations.
Each authenticated backend session runs its own bounded live new-schema/setup/
action/OOC/succession probes, preserving observed version/argv/transcripts and
confirmed versus open questions. Neither backend is primary; no unrun claim is
confirmed. A missing second live gate is a reported remaining release obligation,
not evidence fabricated by the other backend's session.

Run pinned Nextest workspace runtime checks, Cargo doctests/compile-fail examples,
registered-test presence, architecture checks, 45 handwritten mutations (updated
when targets move) and automated domain-only mutations through
`bash scripts/check_contracts.sh`. Never count compiler failures or stale patches
as detected regressions, or allowlist behavior-changing survivors.

## Contract audit against the current worktree

The following is a source/registry audit, not a passing runtime gate. Relevant
sources: [contracts](../decisions/README.md),
[registry](../decisions/required-tests.json),
[domain game](../../cyoa-core/src/game.rs),
[world](../../cyoa-core/src/world.rs),
[generation port](../../cyoa-application/src/generation.rs),
[repository port](../../cyoa-application/src/persistence.rs),
[session](../../cyoa-presentation/src/session.rs),
[coordinator](../../cyoa-presentation/src/persistence.rs).

| ID / existing constraint | Refinement and required action |
| --- | --- |
| ARCH-001 | Current ports live in application and session policy in presentation. R1–R4 move business vocabulary inward and policy to application. Explicit ownership decision supports amendment; preparation observer remains required, enumeration stays application, execution evidence doesn't enter domain |
| ARCH-002 / TEXT-001 | Preserve checked private aggregates, consuming transforms, normalized business text and exact response/transport evidence. New typed records do not justify serde/JSON in core. Preserve `None` versus empty response and successful diagnostics |
| STATE-001 | Existing owning SelectedWorld prevents an index from being used with the wrong cast. New checked establishment/POV methods must prove same membership guarantee while allowing transitions to another living actor. Update compile-fail/runtime registry deliberately at F1, not during R refactor |
| IDENTITY-001/002 | Exact records deduplicate, namesakes survive, IDs/order remain stable. Variant elimination is explicit checked group selection, not name-based deduplication. Fixed `protagonist` identity usage in old game/prompt/save must be updated coherently |
| TEXT-002 | Existing lowercase matching/slug Unicode policy unchanged. Do not introduce full casefold or fuzzy matching for replay/variants |
| LIMITS-001 | Positive playable minimum, zero NPC cap, explicit current/original policy and all-snapshot rebinding preserved. Minimum counts usable candidate versions as today, not necessarily distinct groups; one established person after variant collapse is legal. No invention of an extra surviving protagonist |
| CHAPTER-001 | Retitling/rewind and first-turn chapter rule preserved along each active narrative path. New history nodes don't each create a chapter or force OOC into prose |
| PROMPTS-001 | Opening still includes identity review exactly once, inside the opening request. Establishment chooses explicit variants, not another semantic deduplication call. Do not repurpose the old SummaryUpdate into a deletion command |
| PROMPTS-002/003 | New schemas/contexts/snapshots are deliberate feature work; structural same-instance rendering and field documentation remain. No public arbitrary override loading or claimed semantic validator |
| ARCH-003 / BACKENDS-001 | New shared business DTOs are vendor-neutral; tolerance transformations remain per backend. Prior live evidence doesn't prove new request compatibility. Both backend gates remain required |
| PRODUCT-001 | UI-owned canonical state wording is superseded by agreed application ownership, pending implementation/test amendments. New save format is allowed under Q33, but exact writes/backups/recovery/expected stamps are preserved. New mechanics/time/POV/OOC require real pending-to-enforced tests |
| STREAM-001 | Preview is tentative, final result authoritative, cancellation/key checks preserved. Setup answers/OOC have distinct progress kinds, complete-only adapters remain first-class. No promise that streaming prose is semantically spoiler-free |
| ACCEPTANCE-001 | Frozen old scenario and request count stay during ownership migration. Expanded acceptance adds new guarantees; exact replay intentionally differs from old regeneration after rewind at feature transition. Record explicit rationale and keep unrelated old assertions |
| TESTING-001 / TOOLING-001 | Both mutation gates/profile separation remain; no Python tooling and no new unreviewed core/application dependency |

Registered evidence inspected includes `world::tests::selection_is_checked_against_the_world_it_owns`,
the SelectedWorld compile-fail example, `stored_history_cannot_be_normalized_capped_or_reidentified`,
`current_limits_apply_to_every_snapshot_and_survive_commit_and_rewind`,
`opening_turn_prompt_includes_the_identity_review_addition_exactly_once`,
`full_story_preserves_contracts_through_failure_retry_cancellation_and_rewind`,
and the presentation persistence failure/cancellation/load/counter-exhaustion tests.
These currently protect the old implementation; their presence does not enforce
the future feature merely because its design references them.

## Review-sensitive points and unresolved conflicts

**C1 — Stable identity versus fixed protagonist role.** Accepted succession requires
the role to move without actor identity changing. The design replaces the fixed-ID
assumption in setup/prompt/save while retaining STATE-001/IDENTITY membership and
stability guarantees. Review the replacement interface before F1; don't weaken
those invariants to make a POV setter easy.

**C2 — Terminal outcomes versus mandatory suggestions.** Existing StoryTurn admits
only nonempty QuickActions. Accepted death/end behavior needs distinct outcome
variants. Keep continuing turns unchanged and add terminal-specific tests. This
is an explicit lifecycle extension, not permission to accept any empty action list.

**C3 — Retained branches versus old linear rewind/format.** Accepted tree/replay
changes truncation and no-legacy requirements remove v1 read-compatibility duty.
Migrate feature-level fixtures/contracts/metadata together in F2/F6. Preserve
unrelated malformed-input, exact-audit, restoration and storage guarantees. This
refinement alone does not alter existing expected results.

**C4 — Selective replay after settings changes (product direction settled).**
Preserve unaffected events and mechanical resolution; presentation can change,
severity can affect consequences, and roleplay-based success may be revisited
when reasonably affected. Preserve old branches. The prior blanket settings-key
proposal is rejected by the user's clarification. Implementation needs recorded
resolution basis and consequence dependencies, plus tests proving prose rewrites
cannot mutate facts and affected consequences cannot silently overwrite branches.
Memory-limit rebinding still obeys LIMITS-001 without itself rerolling outcomes.

**C5 — Actor knowledge versus narrator-generated prose.** Structurally safe views
cannot prove the narrator's answer/preview lacks semantic spoilers. This is a
capability limit to evaluate, not a reason to leak full private state in views.
Adversarial tests/live evidence must not be described as universal guarantees.

**C6 — Protected memory and capacity.** New rules/sheets/branches grow state and
existing save byte caps apply. Do not invent an automatic forgetting/compaction
policy that erases accepted facts or drops required sheets. Implement measured
limits and explicit errors; alternate product behavior needs review if needed.

**C7 — Editing authority.** Character editing is a promised eventual power-user
capability, but exact scope/history effects remain unspecified. Do not expose
unrestricted editing as part of the refactor. It needs its own use-case refinement
before P1 claims complete coverage of editing. This does not block R1–R4.

The fixed-hero/minimum issue is resolved technically by counting candidate versions
before explicit group collapse, rather than silently changing the minimum. Review
that interpretation in F1 alongside examples, not as a request to invent more NPCs.

## Review outcome and next action

Review this plan and the concrete contract first. C4's product choice is now
settled and needs selective-replay technical refinement; C1–C3 are specified contract amendments grounded in accepted
features, C5–C6 are explicit technical limits, and C7 is deferred authority design.
If approved, start **R1 only**, preserving behavior and existing tests, then follow
dependencies. No code changes, test weakening, tool installation, authenticated
inference, issue creation, commit or publication was performed for this design step.

## Design-goal completion audit

| Goal requirement | Evidence in current deliverables |
| --- | --- |
| Concrete domain types | Technical contract sections 2–6: IDs, stats enum/validation, actors, life/POV, knowledge, time, tree, search |
| Command/query inputs and outcomes | Sections 4 and 7: typed inputs, admission, result/continuation, view ownership and failure effects |
| Lifecycle transitions | Sections 2, 6–8: fictional/application/persistence states, transition matrix, joined acceptance and publication protocol |
| Setup questions | Section 4 Candidate questions; F3 examples and stale-answer tests |
| OOC guidance | Section 4 OOC contract: attemptability, character-limited estimates, explicit offer/action split; F5 checks |
| Branching/replay | Section 5: parent tree, action rewind, full snapshots, exact-key replay, unfinished chains, OOC lineage; F6 and C4 |
| Ordered implementation slices | R1–R4, F1–F8, P1–P2 dependency table; behavior-preserving versus feature scopes separated |
| Acceptance tests | Slice-specific scenarios and old registered evidence; no new enforcement falsely claimed |
| Contract audit/conflicts | Audit matrix for every existing contract family; C1–C7 with unresolved versus preserved distinctions |
| Concise reviewable plan | Opening readable plan and final user-facing summary; this document and technical contract linked from earlier proposal |
| No implementation before review | Design/documentation files only; existing source/registry/tooling unchanged by this goal |

Validation performed for this documentation step: `git diff --check` passed;
`bash scripts/check_architecture.sh` passed; every relative file link in the two
November 2 refinement documents resolved; the existing registry parsed successfully
and the audit's representative registered test names were checked against it.
The source/registry/tooling worktree contains no changes from this goal; pre-existing
documentation changes were retained. Runtime, doctest, mutation and live-backend
gates were not run: the proposed types/tests are not implementation. Goal completion
means delivery of the proposal and review plan, not approval or implementation.
