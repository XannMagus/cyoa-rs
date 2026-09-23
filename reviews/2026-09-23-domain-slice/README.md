# Review: limits, character construction, and game domain

Reviewed on 2026-09-23 against `main` at
`5b7e9824d11cdebca85f5d4d093e18ef820813c6`, with baseline
`7a98aca`. Reviewer: Codex. The three commits carry a Claude Sonnet 5
co-author trailer. Findings below concern observable behavior, not conclusions
about the model or the author's intentions.

## Scope and disposition

### Follow-up: 2026-09-24

The original findings below describe the reviewed commits. R1 is now fixed:
world construction deduplicates complete records within each role and preserves
namesakes across roles. Tests cover the playable minimum, exact deduplication,
and NPC namesakes reaching the summary with distinct, independently updateable IDs.
This is an explicitly requested deviation from calibre's name-based filtering.
The historical probes target the reviewed HEAD; use that revision to reproduce
the original defects as subsequent repairs change behavior and APIs.

| Commit | Change | Review disposition |
| --- | --- | --- |
| [b5fe6c0929fb83e07412afae70b82abb017132ea][limits-commit] | Typed engine limits; move `MajorEventLimit` | Sound extraction. Zero is explicitly legal for NPC and bridge limits. Consumers must honor that contract. |
| [6290701f1eeacfe22c9cccafaa5c4c195a94c2c3][details-commit] | Named `CharacterDetailsFields` construction | Sound readability improvement; existing checked construction and merge semantics remain intact. |
| [5b7e9824d11cdebca85f5d4d093e18ef820813c6][domain-commit] | World, style, turn, game state, text types, tests, documentation | Request changes: identity loss, transferable checked index, audit-text normalization, and conflicting restoration limits. Zero NPC cap fixed during this review. |

Reviewed all 12 changed files, including test call-site migrations and plan/readme
changes. Compared the new algorithms with the copied Python source and the
previously recorded project decisions. No dependency changes or outward domain
dependencies were introduced. Gameplay, save DTOs, prompts, and live backend
adapters are not implemented by these commits; their absence is not a regression.
No backend behavior was verified in this review.

Priority definitions: **P1** loses valid characters or permits a public API panic;
**P2** violates a supported configuration or data-preservation contract.

## Findings

### R1 — P1: world construction drops distinct namesakes

**Introduced:** `5b7e982`, [world.rs:144–169][world-names]. **Status: open.**

`WorldCast::new` retains playables only when their lowercased name is new. It then
seeds another name set from the surviving playables and uses it to reject NPCs.
Consequently it drops a second playable Ajax, a second NPC Ajax, or an NPC Ajax
sharing a name with any playable, regardless of differences in their biographies.
Two distinct playable Ajaxes alone also fail the default two-player minimum.

This came directly from Python's
[`validated_player_characters` and `validated_npcs`][python-world], including the
assumption that an NPC sharing a playable name is the same person. The Rust
constructor documents that policy; its test
`duplicate_playable_names_collapse_and_npc_colliding_with_playable_is_dropped`
enshrines it. The new [PLAN paragraph][plan-second-slice] repeats the same rule.

That is faithful to this particular Python function but conflicts with our
accepted deviation: exact-record deduplication, suffix repair for ID collisions,
and preservation of distinct namesakes. That policy already exists in
[PLAN.md][identity-policy] and the earlier `776691d`/`7a98aca` history.
`CharacterCast` still implements it correctly. The regression is **upstream**:
`WorldCast` destroys entries before `GameState::initial_summary` can assign their
distinct IDs. Suffixing cannot recover a discarded character.

Observed probe: Ajax (Telamon's son), Ajax (Oileus's son), and Odysseus produce
only two playable entries, including one Ajax.

**Recommended repair:** deduplicate complete normalized world-character records,
preserving order, then apply the NPC cap. A shared name must never by itself
exclude an NPC or playable. Keep different records for later identity review;
assign distinct stable IDs when constructing story characters. For records shared
between playable/NPC roles, define identity and role precedence explicitly rather
than inferring either from the name. Test both Ajaxes, exact duplicates, NPC/NPC
and playable/NPC namesakes, suffix IDs, and later updates targeting each ID.

The evidence establishes the code path and policy conflict. It does not establish
what context Sonnet saw or why it prioritized that rule. The likely process gap
is checking parity function by function without testing the accepted deviation
through the entire construction path.

### R2 — P1: `PlayableIndex` validates a number, not its owning cast

**Introduced:** `5b7e982`, [world.rs:116–126,191–198][world-index],
[game.rs:127–166][game-construction]. **Status: open.**

`PlayableIndex` privately wraps a `usize`, but is freely copyable and carries no
association with the cast that checked it. `playable_at`, `GameState::start`, and
`restore` accept it with any cast. An index of 2 obtained from a three-character
cast panics when used with a two-character cast. An index of 0 from another cast
silently selects a different person. Both were exercised using public APIs.
Python's `start_game` and `deserialize_game` check the index against the actual
world; the Rust port removed that check on an invalid proof of safety.

**Recommended design:** introduce an owning `SelectedWorld` (name provisional)
with private world and selected-position fields. A checked selection operation
consumes a `World`, resolves a candidate position against that exact world, and
returns `Result<SelectedWorld, InvalidSelection>`. `GameState::start` then accepts
the selected aggregate, so a caller cannot separate its validated position from
its owner. Save restoration must use the same construction boundary. A semantic
`PlayablePosition` can express the unchecked request; it must not pretend to be
proof that an arbitrary cast contains it.

Keep unchecked indexing private inside the aggregate whose constructor maintains
the invariant. Remove or change the public cross-cast `playable_at` API as well.
Future cast mutations must preserve/revalidate the selection. Ordinary lifetimes
alone do not brand an index with a particular runtime collection; generative
branding could, but is unnecessary complexity here.

Making `playable_at` return `Option` would stop the panic, but would leave the
in-range foreign-selection ambiguity. Merely changing the documentation would
leave the requested invariant unenforced.

### R3 — P2: a zero NPC limit still admits one NPC

**Introduced:** consumer in `5b7e982`, [world.rs:170–180][world-cap]; legal zero
defined by `b5fe6c0`, [limits.rs:39–49][npc-limit].
**Status: fixed in this review's working-tree patch.**

The loop pushed a candidate before checking `len >= maximum`. Python used the
same loop with a fixed positive cap of eight. Making that cap configurable and
explicitly permitting zero exposed the boundary error; the old cap test covered
only one.

**Applied decision:** filter candidates, then `.take(maximum)`, then collect.
Zero produces no NPCs and does not consume candidates. Duplicate/rejected
candidates do not spend the quota. Existing positive-limit ordering is preserved.
This patch intentionally leaves R1's identity policy unchanged so its repair can
be reviewed separately.

Two regression tests first failed on the original loop and then passed after the
change: `npc_limit_applies_to_surviving_entries_including_zero` (caps 0, 1, 2, 3,
8, and `usize::MAX`, with rejected and duplicate candidates) and
`zero_npc_limit_does_not_consume_candidates` (a producer that panics if consumed).
The existing positive-cap test remains.

### R4 — P2: audit types silently change the recorded exchange

**Introduced:** `5b7e982`, [text.rs:61–67][audit-text],
[turn.rs:234–267][audit-record]. **Status: open.**

`RawResponse`, `Instructions`, and `RenderedPrompt` use the shared nonblank-text
macro, whose constructor calls `.trim()`. For each, input `" \n{}\n"` becomes
`"{}"`. `PromptTrace` promises the exact instructions and prompt, and raw-response
storage supports auditing the original response. Valid JSON can contain boundary
whitespace, so successful responses also trigger this loss.

The existing infrastructure `GenerationResponse` preserves exact JSON; putting it
into these new domain records would weaken that contract. Python stores raw
response and debug prompt text without this normalization.

**Recommended repair:** use distinct opaque string wrappers that preserve their
input. If nonblank validation is required for a particular field, inspect a
trimmed view but store the original string. Decide whether empty traces are legal
at the relevant boundary; absence of a trace is already represented by `Option`.
Add exact round-trip tests including leading/trailing spaces and newlines. Keep
normalization for narrative and character text where it is intentional.

### R5 — P2: restored summaries can override the game's configured event limit

**Introduced:** integration in `5b7e982`, [game.rs:149–166][game-construction],
[game.rs:198–203][current-summary], [game.rs:329–331][commit-turn].
**Status: open; restoration policy must be explicit.**

`GameState` stores `Limits`, while every `MajorEvents` also carries a limit.
`restore` accepts an arbitrary vector of already-built records without reconciling
those limits. `current_summary` returns the last stored summary, and `commit_turn`
updates it using that summary's carried cap, not `GameState.limits`.

Observed: build a game at cap 30 containing three events; restore its records with
`max_major_events = 1`; commit a fourth event. The game reports limit 1 but stores
four events under carried limit 30. This does not require a save adapter or
malformed strings; it uses the current public restoration API.

**Recommended repair:** make restoration reconcile all snapshots with the chosen
runtime event cap, or reject mismatched snapshots with a typed error. Given the
documented policy that limits are runtime configuration and not saved, rebuilding
each `MajorEvents` under the supplied cap is the natural default. Apply it to
every snapshot so rewind cannot resurrect a different cap. Record that lowering
the cap discards older summary events; do not silently invent a history-preserving
alternative. If historical snapshots must remain untouched, enforce the runtime
cap when exposing/merging their active memory instead and document that distinction.

Do not blindly replay every saved delta to validate summaries: the planned
character-edit propagation can legitimately change saved snapshots. The needed
invariant here is consistent limit enforcement, not equality to an untouched
historical replay. Likewise, generation-time NPC caps should not silently delete
established characters when loading a game with different settings.

## Other review conclusions

- `CharacterDetailsFields` is an input object, not a validated domain entity.
  Allowing its default to be incomplete is appropriate because
  `CharacterDetails::new` still enforces the durable-details invariant. No reason
  to replace this useful change with a builder or typestate hierarchy.
- Typed limits, optional style keys, nonblank narrative, bounded/nonempty quick
  actions, and derived chapter membership are useful domain modeling. `CostAmount`
  excludes NaN and infinities, making its explicit `Eq` sound. No actionable bug
  found in these implementations themselves.
- Quick-action deduplication, kind preference, fill order, and restored source
  order match Python subject to the already accepted lowercase/casefold difference.
  First-turn chapter handling and prose-bridge slicing also match the intended
  behavior, including a zero bridge window. Blank wire entries still need tolerant
  boundary filtering when DTOs are implemented.
- Retitling a chapter from a later turn is an explicit documented deviation in
  this commit, with a rewind test. This review does not establish whether the user
  approved it in the producing session. Keep its status distinct from source parity;
  it is not an accidental algorithm mismatch.
- The prose-context differential test explores 320 configurations, but compares
  only lengths at a fixed six-turn history. Compare actual positions/slices and
  include empty and shorter histories when strengthening that feature's tests.
- `current_chapter` constructs every chapter just to return the last one. A reverse
  scan can avoid the allocation if this becomes a frequently polled view; this is
  a low-priority optimization, not a correctness blocker.
- README/PLAN overstated the cast-index guarantee and centralized limit enforcement.
  This review's documentation corrections link the open findings instead of
  advertising those invariants as already enforced. The compile-fail test only
  proves that the turn vector is private; it does not prove aggregate validity.

## Decision and follow-up ledger

| Decision | Basis | Status |
| --- | --- | --- |
| Preserve distinct namesakes; exact deduplication precedes ID suffix repair | Earlier user decision and recorded identity policy | Existing requirement; violated by R1, repair pending |
| Keep named construction fields and distinct limit types | Useful distinctions with checked domain construction | Retain |
| Enforce zero NPC cap without consuming candidates | Legal configured value; user requested tighter enforcement | Implemented with regression coverage |
| Bind protagonist selection to its owned world | Cross-cast proof failure in R2 | Proposed design, not implemented |
| Preserve audit text exactly | Raw-response/debug-trace purpose; existing infrastructure contract | Repair recommended, not implemented |
| Reconcile restored event caps across all rewind snapshots | Runtime limits must agree with active memory | Proposed policy, not implemented |
| Later turns can retitle their chapter | Deliberate extension documented in `5b7e982` | Existing implementation; approval history not established here |

Recommended implementation order: R1 identity preservation and its construction-
to-summary regression; R2 aggregate construction; R5 restoration integration on
that boundary; R4 opaque audit text (independent). Keep each correction and its
regression tests together in a commit. No new commit was created by this review.

## Validation and reproducibility

At the reviewed HEAD, all 48 runtime tests and six compile-fail doc tests passed
despite R1–R5. After the NPC fix, all **50 runtime tests and six doc tests** pass.
Formatting, warning-denying Clippy, and the Bash dependency-boundary check pass:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline
bash scripts/check_architecture.sh
```

[probes.rs](probes.rs) preserves the public-API reproductions actually run for R1,
R2, R4, and R5. These assert the observed defects and are historical evidence,
not desired-behavior regression tests; repairing those findings should invalidate
their assertions. R2 intentionally catches a panic, whose default panic-hook output
still appears on stderr. The program exits successfully after checking all findings.
R3's desired behavior is now tested in `cyoa-core/src/world.rs`.

To rerun the probes from the repository root without adding a workspace member:

```sh
probe_dir=$(mktemp -d /tmp/cyoa-review.XXXXXX)
cargo init --bin --name cyoa-review-probes --vcs none "$probe_dir"
cargo add --offline --manifest-path "$probe_dir/Cargo.toml" --path "$PWD/cyoa-core"
cp reviews/2026-09-23-domain-slice/probes.rs "$probe_dir/src/main.rs"
cargo run --offline --manifest-path "$probe_dir/Cargo.toml"
```

No Python tooling or network/backend calls are needed. Commit-pinned links below
refer to the reviewed code, so line references remain meaningful after repairs.

[limits-commit]: https://github.com/XannMagus/cyoa-rs/commit/b5fe6c0929fb83e07412afae70b82abb017132ea
[details-commit]: https://github.com/XannMagus/cyoa-rs/commit/6290701f1eeacfe22c9cccafaa5c4c195a94c2c3
[domain-commit]: https://github.com/XannMagus/cyoa-rs/commit/5b7e9824d11cdebca85f5d4d093e18ef820813c6
[world-names]: https://github.com/XannMagus/cyoa-rs/blob/5b7e9824d11cdebca85f5d4d093e18ef820813c6/cyoa-core/src/world.rs#L144-L169
[world-index]: https://github.com/XannMagus/cyoa-rs/blob/5b7e9824d11cdebca85f5d4d093e18ef820813c6/cyoa-core/src/world.rs#L116-L198
[world-cap]: https://github.com/XannMagus/cyoa-rs/blob/5b7e9824d11cdebca85f5d4d093e18ef820813c6/cyoa-core/src/world.rs#L170-L180
[npc-limit]: https://github.com/XannMagus/cyoa-rs/blob/b5fe6c0929fb83e07412afae70b82abb017132ea/cyoa-core/src/limits.rs#L39-L49
[game-construction]: https://github.com/XannMagus/cyoa-rs/blob/5b7e9824d11cdebca85f5d4d093e18ef820813c6/cyoa-core/src/game.rs#L127-L166
[current-summary]: https://github.com/XannMagus/cyoa-rs/blob/5b7e9824d11cdebca85f5d4d093e18ef820813c6/cyoa-core/src/game.rs#L198-L203
[commit-turn]: https://github.com/XannMagus/cyoa-rs/blob/5b7e9824d11cdebca85f5d4d093e18ef820813c6/cyoa-core/src/game.rs#L329-L331
[audit-text]: https://github.com/XannMagus/cyoa-rs/blob/5b7e9824d11cdebca85f5d4d093e18ef820813c6/cyoa-core/src/text.rs#L61-L67
[audit-record]: https://github.com/XannMagus/cyoa-rs/blob/5b7e9824d11cdebca85f5d4d093e18ef820813c6/cyoa-core/src/turn.rs#L234-L267
[python-world]: https://github.com/XannMagus/cyoa-rs/blob/5b7e9824d11cdebca85f5d4d093e18ef820813c6/reference/calibre/cyoa.py#L1167-L1201
[identity-policy]: https://github.com/XannMagus/cyoa-rs/blob/5b7e9824d11cdebca85f5d4d093e18ef820813c6/PLAN.md#L475-L487
[plan-second-slice]: https://github.com/XannMagus/cyoa-rs/blob/5b7e9824d11cdebca85f5d4d093e18ef820813c6/PLAN.md#L513-L525
