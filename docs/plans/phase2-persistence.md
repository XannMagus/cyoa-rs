# Phase 2: persistence implementation plan

Planning baseline: `9d7b3a9` on `phase-2/persistence`, 2026-10-04; runtime
baseline `1663f62`, with Phase 1 live evidence updated by `4fb6775`. This is the deliverable commissioned by the
[planning handoff](phase2-persistence-handoff.md). **All new interfaces, policies,
tests and steps below are proposals, not implemented or enforced contracts.**
Existing contracts retain their authority and coverage. No runtime/dependency/test
changes accompany this plan. The numbered sections and acceptance IDs are stable
handoff references, not required-test registrations.

Implementation update (2026-10-04): **S1 is complete**. The checked established
cast constructor and its four registered domain regressions passed the full
gate, including domain mutation scrutiny. See
[S1 evidence](../../reviews/2026-10-04-persistence-s1/README.md).
**S2 is complete**: inward persistence vocabulary/use cases, strict version-one
codec, synthetic-only migration dispatch and independent acceptance fixtures.
See [S2 evidence](../../reviews/2026-10-04-persistence-s2/README.md).
**S3 implements the blocking Linux atomic repository**; see
[S3 evidence](../../reviews/2026-10-04-persistence-s3/README.md).
**S4 implements supervised storage execution** through the internal helper and
presentation storage runner; see [S4 evidence](../../reviews/2026-10-04-persistence-s4/README.md).
**S5 implements canonical persistence coordination**; see
[S5 evidence](../../reviews/2026-10-04-persistence-s5/README.md).
**S6 implements public headless commands and resume**; see
[S6 evidence](../../reviews/2026-10-04-persistence-s6/README.md).
S7's composed fault acceptance and focused persistence mutations remain planned.
The proposals and audit below retain their planning context.

## 1. Outcome, scope and authority

Implement save/load/list, version-1 JSON saves with a migration dispatcher, atomic
replacement and one backup, autosave, explicit restore settings and headless
save/load/rewind/quit integration. An accepted turn survives restart; a failed
write is visible without regenerating or rolling back that turn. Loading never
silently repairs identities or replaces a working session with partial data.

Keep Linux as the exercised headless/storage platform for this slice. No TUI,
export, character editing, image generation, Calibre import, arbitrary prompt
configuration, cloud sync, save renaming/deletion UI or automatic backend switching.
Reserve the image directory without creating or interpreting assets. Save only a
selected `GameState`; unfinished brief/outline/cast stages are not save documents.
Both vendor adapters remain peers. Both live headless gates are now complete
(the older planning handoff predates Claude's recorded gate). This work requires
no real model calls.

### Contract and evidence matrix

| Contract | Existing authoritative evidence inspected | Phase 2 obligation / acceptance |
|---|---|---|
| ARCH-001, ARCH-002 | Five workspace crates; architecture script; checked constructors; application generation port | Inward repository port, separate save DTOs, checked reconstruction; A1–A4, A12 |
| STATE-001 | `World::select`, owning `SelectedWorld`; selection compile-fail and restore tests | Check persisted position against reconstructed world; A1, A2 |
| LIMITS-001 | `core/tests/restore_limits.rs`; `generation_use_cases::restored_limits_reach_captured_prompt_schema_and_committed_memory` | Original metadata, explicit policy, every snapshot, repeated disk cycles; A3 |
| IDENTITY-001/002, TEXT-002 | `WorldCast::new`, `CharacterCast::new`, existing namesake/merge tests | Preserve role order, distinct namesakes and repaired IDs; never repair a corrupt save by deduplication; A1, A2 |
| TEXT-001 | Verbatim text types, `TurnRecord`, restore tests, transport diagnostic tests | Exact decoded UTF-8 audit strings; optional prompt traces/provenance; A1, A2 |
| CHAPTER-001 | Marker-derived chapters, last-title-wins, rewind tests | Store markers and snapshots, no stored chapter index; A1, A3, A9 |
| PRODUCT-001 | Controller/headless tests; saves, migration and export obligations still pending | Actual persistence and command acceptance; A1–A12; TUI/export stay pending |
| STREAM-001 | `SessionController::complete`, worker joins; session/worker tests; headless backpressure tests | Save only accepted canonical revision; keep cancellation and shutdown responsive; A7–A10 |
| ACCEPTANCE-001, PROMPTS-001 | Full Phase 0 story; controller story and both fixture adapters | Disk round trip within the full story; zero-turn resume issues exactly one opening request; A1, A9, A11 |
| PROMPTS-002/003 | Bundled-only `GenerationTemplates`; private style fallback | No new override loader; preserve unknown style key with existing fallback; A2, A3 |
| BACKENDS-001, ARCH-003 | Both fixture adapters and own live records; Codex and Claude live headless evidence | Save source policy is vendor-neutral; no schema/adapter edits or invented provenance; A11 |
| TOOLING-001, TESTING-001 | Bash/Rust gate, pinned Nextest/mutants; 37 handwritten mutations | Error → edge → nominal tests; registry only for real tests; keep both mutation gates; A12 |

[Project contracts](../decisions/README.md), their
[registry](../decisions/required-tests.json) and
[Phase 0 acceptance](../decisions/phase0-acceptance.md) supersede historical PLAN
sketches. In particular, runtime types have no serde, chapter indices are derived,
and stored summaries must not be rebuilt by replaying all deltas.

## 2. Actual seams and proposed ownership

### Findings from code

- `core/game.rs`: private fields have all needed getters. `restore` takes original
  limits and `RestoreLimits`, then rebinds each snapshot; it does not validate a
  historical chain. `commit_turn` computes a snapshot; loading must use
  `TurnRecord::new` and `GameState::restore`, never `commit_turn`.
- `core/world.rs`: `WorldCast::new` deduplicates, caps NPCs and checks a generation
  minimum. There is no established-cast constructor. Add only
  `WorldCast::restore(playable: Vec<PlayerCharacter>, npcs: Vec<NonPlayerCharacter>)
  -> Result<Self, InvalidRestoredWorldCast>`: require at least one playable and
  no exact duplicate records within either role; preserve all input and ordering,
  including namesakes across roles. No `Limits` argument and no pruning. Leave
  `new` and its existing behavior untouched. This preserves the existing normalized
  cast invariant without treating generation settings as historical validity.
- `CharacterCast::new`, `QuickActions::select`, `EventList::new`, nonblank text
  constructors and `MajorEvents::new` normalize/repair. Save decoding validates
  inputs before using them and checks normalization is lossless (§4); no public
  mutable fields, serde derives or unchecked constructors need adding to core.
- `TurnRecord` retains input, turn, exact raw response, post-turn summary,
  provenance and optional `PromptTrace`. Successful transport bytes and token
  usage are not stored there; failed/preview diagnostics belong to presentation.
  Phase 2 does not pretend to recover or persist those absent fields.
- `application/generation.rs` already supplies rewind independently of inference.
  No persistence port exists. Keep generation use cases independent of storage.
- `SessionController::from_game` seeds an already-valid in-memory game and resets
  counters; it is not a decoder or an in-session load operation. Add an admitted
  `replace_game` transition that advances existing counters rather than replacing
  the controller. `complete` returns `Committed` for outline/cast as well as turns;
  autosave must distinguish accepted **turn** outcomes explicitly.
- `SessionRuntime` joins before delivering completions. Headless currently calls
  quit on every error and waits for controller `Closed`; extend runtime closure
  to include storage completion, without making the generation controller own I/O.
- `main.rs` currently authenticates before play. Dispatch list/inspect/load
  validation before vendor resolution; invalid saves and read-only operations
  must not run auth. `HarbourDemo::turn` chooses by committed turn count, not a
  mutable replay cursor. `StyleTable::resolve` falls back to its first entry for
  absent or unknown keys.

### Inward interfaces

Add `cyoa-application/src/persistence.rs`. All following types are owned there;
fields are private where they establish invariants. `SaveId`, `StorageRequestId`,
`SaveRevision` and `ContentStamp` are distinct types. No JSON, paths, concrete
filesystem handles or presentation types cross this port.

```rust
trait GameRepository {
    fn create(&mut self, snapshot: SaveSnapshot, cancel: &CancellationToken)
        -> Result<SaveReceipt, StorageFailure>;
    fn replace(&mut self, target: SaveTarget, snapshot: SaveSnapshot,
        cancel: &CancellationToken) -> Result<SaveReceipt, StorageFailure>;
    fn reconcile(&mut self, attempt: PendingWrite, cancel: &CancellationToken)
        -> Result<SaveReceipt, StorageFailure>;
    fn load(&mut self, id: &SaveId, copy: SaveCopy, cancel: &CancellationToken)
        -> Result<StoredGame, StorageFailure>;
    fn list(&mut self, page: SavePage, cancel: &CancellationToken)
        -> Result<SavePageResult, StorageFailure>;
}
```

`SaveSnapshot { game: GameState, source: StorySource }` is an owned canonical
snapshot, not a schema DTO. `StorySource = Live | Demo { scenario: DemoScenarioId }`;
initial supported scenario is `harbour-v1`. `StoredGame` contains a validated game
restored with its saved active limits, source, summary metadata, copy origin and
content stamp. Repository mapping is responsible for establishing these facts.
`SaveTarget` pairs ID with expected stamp. `SaveReceipt` has ID, new stamp,
`SavedAt`, and `SaveRevision`; successful receipts mean directory sync completed.
`StorageFailure` has operation, stage, kind, safe diagnostic and
`WriteVisibility::{Unchanged, Replaced { stamp }, Unknown}`. No error erases its
initiating cause in favor of cleanup failure. `NotFound`, `Busy`, `Conflict`,
`Corrupt { location }`, `FutureVersion`, `Unsupported`, `TooLarge`, `Io`,
`Cancelled`, `Timeout` and `WorkerFault` remain distinguishable.

`ContentStamp` is a 32-byte SHA-256 of exact primary file bytes, constructed by
infrastructure, used for optimistic conflict checks, not a security signature.
`SaveRevision` is an on-disk monotonically checked u64 write counter (starts at 1),
independent of presentation revisions and timestamps. `SavedAt` is a checked UTC
instant; infrastructure supplies it via a small private injectable clock. No wall
clock value orders writes or establishes freshness. App-owned named metadata
values may use primitives internally; JSON timestamp parsing stays outside.

`PersistenceUseCases<R>` exposes commands `SaveGame` (create/replace), `LoadGame`
(explicit `RestoreLimits` and `SaveCopy::{Primary, Backup}`), and queries
`ListSaves`, `InspectSave`. It takes owned values, delegates storage, applies
`GameState::restore` again with the user's selected policy to the loaded game,
and returns `LoadedGame` plus the unchanged disk stamp. No shared state mutex.
Application fake repositories test call counts, error propagation and restore
selection. `InspectSave` returns a read-only game/metadata view and does not bind
or rewrite a save. It uses stored active limits, not an implicit user restore choice.

Infrastructure adds `persistence/{dto,codec,migrations,repository,filesystem,
helper}.rs`; mapping is colocated with DTOs as in the generation boundary but does
not reuse its tolerant wire structs. Presentation adds `storage.rs` for an owned
runner and `PersistedSession` coordinating controller/runtime with these use cases.
Keep `SessionRuntime<G>` usable by existing tests; a surrounding coordinator owns
it and a storage runner, gates intents and observes typed accepted transitions.
`main.rs` composes both and resolves storage paths. Public CLI options remain in
presentation. Concrete adapters never enter application or presentation tests.

### Identity and effect flow

Presentation owns `SaveBinding::{Unbound, Bound { id, stamp, disk_revision }}`,
`Durability::{Clean, Dirty, Uncertain}`, and one
`StorageOperation::{Idle, Running { key, base_revision, intent }, Failed { error }}`.
A successful create binds the slot; subsequent autosaves replace only that slot.
A completed load changes binding and game together. Failure evidence/retry intent
for generation remains separate from storage failure. `/save` cannot mean `/retry`.

Each canonical selection, accepted turn, rewind or load advances the existing
`SessionRevision`. Storage requests carry a fresh checked storage ID and that
revision; responses cannot mark a different revision clean. No revision wraps.
Quit reserves its final storage operation even from generation `Faulted` state.
Capture an owned snapshot only after acceptance, never from worker progress or the
worker-owned game. No generation or canonical mutation is admitted while a write,
load or list is pending. This intentionally serializes disk effects and bounds
pending snapshots to one. It avoids coalescing away an obligated turn autosave.

## 3. Execution, filesystem scope and bounded work

### Scheduling choice

Use a dedicated storage worker thread, polled/joined like the existing generation
runner, whose repository adapter supervises **one isolated storage helper process
per read/write suboperation**. Create/reconcile/load/list use one child; replacement
uses a read-only preparation child then one mutating child, sharing one total deadline.
The S2 `SaveTarget` has a content stamp but no disk revision: the preliminary read
obtains the checked revision so the worker can encode and publish exact retry material
before dispatching the mutating child. This S4 refinement preserves the application
port, keeps filesystem work isolated and performs no automatic write retry.
The helper is an internal `cyoa` invocation wired before public
Clap/auth handling, with request/response pipes; it contains filesystem work and
codec validation. Parent-side serialization/decoding also runs on the storage
worker, never on the input loop. It owns snapshots only. One operation is in flight;
no async runtime, persistent daemon or shared mutable game is needed.

This is more code than a plain thread, but ordinary file reads, locks and `fsync`
can block, and Rust cannot cancel a thread in such a syscall. Blocking file I/O on
the loop, joining an arbitrary disk thread on quit, or detaching a thread that
may later overwrite a save are rejected. Reuse vendor-neutral process supervision
where its finite capture/input/cleanup semantics fit; do not change a vendor
adapter or its limits. Private helper framing is one bounded JSON request and one
bounded JSON result, with no model protocol and no shell. The composition root
passes its resolved executable path to the adapter. Test fixtures provide separate
controlled helpers, not production fault-injection environment variables.

Proposed limits: 64 MiB encoded save, 160 MiB helper request/response (allows JSON
escaping of the save text plus envelope), JSON nesting 128, 10 seconds total per
storage operation including lock wait/serialization allowance, and at most 100
listing rows per page. Use checked arithmetic and capped readers/writers; never
read an unbounded file and check afterward. In-memory allocation is additionally
bounded by these inputs and one snapshot, though cloning a very large canonical
game takes CPU/memory; the format limit is an explicit eventual save failure,
not a reason to truncate history. Report actual size and the bound. A later limit
increase is a compatibility-preserving implementation change, not a migration.

Parent polling remains at most 25 ms as today; `/help`, `/inspect`, `/diagnostics`
and signals stay available during storage work. Storage timeout kills/reaps the
helper and returns uncertain write visibility when replacement may have happened.
Cancelled writes are never assumed rolled back. Use the existing cancellation
source, finite process cleanup and join only after thread completion. No success
from a response received before helper exit/cleanup verification.

The guarantee is for tested Linux local filesystems supporting same-directory
atomic rename, advisory locks and file/directory sync. A kernel task stuck in
uninterruptible I/O cannot be made reliably killable by user space; neither this
plan nor the existing process supervisor proves a hard wall-clock exit on broken
hardware or arbitrary NFS/FUSE mounts. Unsupported sync/lock operations fail
explicitly. Linux temp-directory tests establish ordering and process liveness,
not physical power-loss durability on every device. Other OS support is deferred,
returning `Unsupported` without modifying files.

### Paths, names, ownership and concurrent access

Use `ProjectDirs::from("", "", "cyoa").data_dir().join("saves")`;
on Linux this is `$XDG_DATA_HOME/cyoa/saves` (fallback user data directory).
Add global `--data-dir PATH`, meaning the app data directory (append `saves`), for
portable use/tests; this does not load prompt config. Explicit paths must be
absolute. Invalid relative XDG input is an actionable configuration error, not a
fallback into the working directory. Missing/unavailable home/data location is
an error; read-only listing of a missing saves directory returns empty.

New save IDs: ASCII title slug (lowercase ASCII alphanumerics, separator runs to
`-`, trim, max 40 chars, fallback `story`) plus `-` and 32 lowercase random hex
digits from OS randomness. `SaveId` accepts that exact grammar (1–40 slug chars,
no empty segments, suffix exactly 32 hex), max 73 bytes. Display titles retain
Unicode. Do not accept user paths or titles as IDs. IDs never change when a title
changes; no rename operation this phase. Reserve `<id>.assets/`; do not create it.
Primary `<id>.json`, backup `<id>.json.bak`, stable lock `<id>.lock`. Dot-prefixed
unique temporary files are excluded from listing. Randomness failure is an error.
Collision is not overwrite permission: choose a new ID, at most eight attempts.

Create private app/saves directories (0700) and files (0600) on Linux; do not chmod
preexisting user directories. Canonicalize the configured root once, allowing its
chosen parent path to resolve, then require a user-owned non-group/world-writable
saves directory. Reject symlink saves directories and symlink/nonregular primary,
backup and lock entries. Open relative to an owned directory handle with no-follow
semantics (safe `rustix` APIs); inspect file type/owner after opening. Reject
multi-link managed files to avoid modifying an unrelated hard-link target.
Recheck target entries under the lock before rename. Same-user malicious root
replacement is outside the threat model; do not claim a sandbox. No directory
recursion; no assets or arbitrary external files are opened.

Use nonblocking advisory exclusive locking on the **stable lock file**, never on
the primary inode which is replaced. Locks live for each helper operation and are
released by close/process exit; never delete a lock file to clear it. Reads use
the same lock, ensuring a coherent primary/backup observation. Busy is immediate
(no hidden retry). A stale zero-byte lock file is harmless. Compare actual primary
stamp to the expected stamp under lock on replace; mismatch/missing file is
`Conflict`. A lock alone would permit two sequential writers to lose updates;
the stamp check rejects the second writer. Non-cooperating manual writers are
outside the lock guarantee, but the pre-write hash catches changes already made.
No force-overwrite flag. On conflict, `/save-copy` creates a fresh slot while
preserving the current game; it changes the binding only after durable success.

## 4. Version 1 schema, mapping and compatibility

### Envelope and DTO inventory

Writer emits UTF-8 JSON plus one final newline. File whitespace/key order need not
round-trip; all decoded audit strings must round-trip byte-for-byte. Arrays are
ordered. Nonnegative integers use JSON u64, checked when converted to platform
`usize`; version is positive u32. Numbers cannot be fractional, negative or overflow.
Duplicate JSON object keys are rejected before `Value` (a private serde visitor),
so last-key-wins cannot hide a version, ID or payload. No compression or checksum
inside the document; the external content stamp covers exact file bytes.

Notation below: **R** = required; **D(x)** = omitted defaults to x; null is accepted
only where explicitly stated. All newly introduced identity/limits metadata is
required. The saved representation follows source defaultedness where fields
correspond, while retaining Rust-specific provenance and original-limit facts.

| DTO | Fields and wire forms |
|---|---|
| `SaveEnvelopeV1` | R `version: 1`, `id: string`, `revision: u64 > 0`, `title: string`, `saved_at: string` (UTC RFC3339, canonical writer milliseconds and `Z`), `turn_count: u64`, `source: SourceSave`, `game: GameSaveV1` |
| `SourceSave` | R `kind: "live"` or `"demo"`; demo additionally R `scenario: "harbour-v1"`; live has no configured vendor/model credentials |
| `GameSaveV1` | R `brief: string`, `world: WorldSave`, `character_index: u64`, `original_limits: LimitsSave`, `active_limits: LimitsSave`; D([]) `turns: [TurnRecordSave]`; D("") each of **flat** `art_style`, `pace`, `tone`, `narration` strings |
| `LimitsSave` | R `max_major_events`, `max_generated_npcs`, `prose_bridge_turns`, `min_playable_characters` (u64); positivity/zero rules exactly as core |
| `WorldSave` | R `title`, `world_description` strings; D([]) `characters: [PlayerSave]`, `npcs: [NpcSave]`; an empty playable cast fails stored-state validation |
| `PlayerSave` | R `name`, `description`, `backstory` strings, each nonblank |
| `NpcSave` | R player fields and `relationships: string`; empty relationships means absent, not omitted/null |
| `TurnRecordSave` | R `player_input: string` (empty = None), `raw_response: string` (verbatim), `turn: TurnSave`, `summary: SummarySave`; D({}) `provenance: ProvenanceSave`; D(null) `prompt_trace: PromptTraceSave or null` |
| `TurnSave` | R `narrative: string`, `quick_actions: [ActionSave]`, `scene_description: string` (empty = None), `summary_update: UpdateSave`, `starts_new_chapter: bool`, `chapter_title: string or null` |
| `ActionSave` | R `text: string`; D("other") `kind: string`, known values cautious/bold/social/investigate/other |
| `SummarySave` | R `world: string`, `major_events: [string]`, `characters: [CharacterSave]`, `current_situation: string`, `upcoming_events: [string]` |
| `CharacterSave` | R `name`, `description`, `backstory`, `relationships` strings; D("") `current_state`, `id` strings; empty details map to absent, at least description or backstory nonempty; a v1 stored character's ID must be nonblank and unique |
| `UpdateSave` | R `current_situation: string`, `character_updates: [DeltaSave]`, `new_major_events: [string]`; D(null) `upcoming_events: [string] or null`; D("") `world: string`; D([]) `consolidated_major_events: [string]` |
| `DeltaSave` | R `id`, `current_state` strings (empty maps None); D("") `name`, `description`, `backstory`, `relationships` strings |
| `ProvenanceSave` | D(null) `provider`, `model` (nonblank string or null); D(null) `list_price_estimate: { amount: finite nonnegative f64, currency: nonblank string }`; amount/currency both R when present |
| `PromptTraceSave` | R `instructions: string`, `prompt: string`, both verbatim including empty |

Source field defaultedness was checked against `reference/calibre/cyoa.py`;
the table is the proposed complete v1 contract, not reuse of the generated schema.
A source default can still fail stored-state validation: notably omitted `id`
defaults to empty at decoding but cannot establish v1 identity. Do not synthesize
an ID on load. There is no fabricated historical save needing that repair.

No per-record chapter number, cached chapter title list, prose-context cache,
current-summary duplicate, provider session ID, runtime revision/request token,
working directory, executable, auth home, preview or failed-response buffer is
serialized. `source` selects replay safety only; observed per-turn provenance
remains optional and does not claim the currently configured model was observed.
Keep list-price estimates labelled estimates, never subscription spending.

Writer preserves an existing `Some(PromptTrace)` in full. Default generation still
produces None; persistence does not suddenly collect prompts, or strip traces
from restored records. No new public debug configuration in this slice. No prompt
is reconstructed from raw responses. Raw responses are audit text, not reparsed
as the source of turns; even empty or invalid JSON strings remain legitimate audit.

### Validation and history authority

Decode into a temporary candidate; publish nothing until all checks pass:

1. Bound bytes, reject malformed UTF-8/JSON/duplicate keys/depth, read positive
   version, migrate `Value`, then typed DTO. Missing version, zero or future version
   is an error. Validate envelope ID matches the requested filename (including for
   its backup); title equals canonical world title and turn_count equals log length.
   Parse timestamp, require revision positive, and validate source discriminant.
2. Construct all text with existing checked types. A present stored business string
   must equal its normalized representation; reject noncanonical leading/trailing
   whitespace rather than silently rewriting it. Empty-string sentinels alone mean
   absence. Unknown nonblank style keys remain unchanged and use the existing first
   style fallback in future prompts. Blank style strings mean default; null is an
   error. Raw response/trace strings bypass all normalization.
3. Restore world with the new checked `WorldCast::restore`, then `World::select`.
   Do not call generation wire mapping or `WorldCast::new`. Distinct namesakes are
   legal. Exact duplicates within a role are corrupt; their removal could change
   character_index. No check against old or current generation NPC/minimum bounds.
4. For stored summary casts validate nonempty and unique nonblank IDs, then call
   `CharacterCast::new` and compare the ordered input to output. Reject anything
   that would remove/re-ID a record. Check details using `CharacterDetails::new`.
   Do not require names to match world records or require protagonist first: later
   renames and valid domain snapshots are authoritative; no semantic consolidation.
5. Check event arrays are already normalized, nonblank and Unicode-lowercase
   deduplicated by comparing `EventList::new` output to input. Stored major-event
   count must fit **saved active** cap before `MajorEvents::new`; reject excessive
   history rather than silently capping corrupt data. Delta lists likewise must be
   lossless, but character delta repetition is legal: merge's first-update-wins
   behavior does not make the stored proposal invalid.
6. Actions must contain 1–3 usable, unique texts and round-trip through
   `QuickActions::select` without trimming/dropping/reordering. Unknown kind strings
   retain the established mapping to Other; this is a documented cosmetic
   compatibility exception. Preserve null/omitted update upcoming_events as Keep
   and [] as Replace(empty). Turn markers map directly; first-turn new-chapter is
   valid and has the existing chapter-zero interpretation.
7. Construct `StoryTurn`, `StorySummary`, `TurnRecord`, then `GameState::restore`
   with `RestoreLimits::Current(saved_active_limits)`. There is no replay check
   asserting snapshot == merge(previous, delta): smaller historical restore caps
   and future durable character edits can make that false. Validated snapshots
   are the durable memory authority; deltas/raw text are retained evidence. A
   structurally and domain-valid but fictionally inconsistent snapshot is accepted,
   not repaired or falsely authenticated. Reject constructor/aggregate violations,
   not differences in narrative meaning. No historical setting timeline is invented.
8. Only the `LoadGame` use case applies the explicit current/original policy.
   Rebind every snapshot through core restore; original remains creation metadata.
   Selected current limits come from the existing bundled/default configuration,
   or injected typed settings in tests. No public arbitrary TOML/limit override.
   If policy changes active settings/events, mark the accepted loaded game dirty
   and immediately save once to its slot, using the read stamp. Failure is a
   **load success plus save failure**, not a failed load that changed old state.

There is no lossless recovery of events previously capped away. Saving after a
smaller-cap load records the truncated snapshots; selecting original settings on
a subsequent load cannot resurrect them from delta replay or `.bak` implicitly.

### Migration scaffold and optional evolution

`migrations::upgrade(Value) -> Result<Value, SaveDecodeError>` dispatches from the
actual input version to CURRENT=1; v1 is identity, versions <1 invalid, >1 fail
with “Save version N is newer than supported version 1; use a newer cyoa.” No v0,
Calibre-v4 or imaginary older Rust format is accepted. Future steps add explicit
vN→vN+1 functions before typed decoding and freeze every shipped source fixture;
never overwrite the original on a read/query. Migration does not imply write
permission. Test the chain machinery with private synthetic transformations,
clearly labelled scaffolding tests, not support for shipped historical formats.

Unknown object fields are ignored, including an additive optional future field;
known fields of wrong type still fail. Same-version additions must be explicitly
optional and safe for older readers to omit on re-save. Extensions requiring
preservation or changing meaning require a version bump. List/inspect/load report
“unrecognized optional fields will not be retained when saving” if present.
Freeze `v1-minimal.json`, `v1-full-story.json`, `v1-optional-extra.json`, invalid
fixtures and independent expected domain views. Test optional omissions, added
unknown fields and a later defaulted style field in a test-only DTO, not a fake
production v2 migration. Calibre import stays deferred; absent original limits
would require known source constants or an explicit boundary choice, never guessing.

## 5. Atomic replacement, backups and uncertain outcomes

### Write protocol

Implement a private filesystem interface around open/read/write/sync/rename/lock
and randomness/clock for deterministic failures; production uses real filesystem
operations. Do not mock the entire repository as the only durability evidence.
`NamedTempFile::persist` is the replacement primitive specified in PLAN. Its local
3.27.0 source explicitly says persistence alone synchronizes neither contents nor
directory; call `sync_all` explicitly. The two file replacements are not one atomic
transaction, and the protocol must not rename primary away before the new one exists.

Under the slot lock:

1. Validate paths and expected stamp. For replace, read/fully validate the existing
   primary under its saved active limits. Corrupt/future/oversized primary rejects
   replacement, leaving backup untouched. No auto-fallback or rotation of corrupt
   bytes into a good backup. Check revision increment for overflow.
2. Build envelope with canonical title/count, stable ID, new revision and injected
   UTC time. Encode through a bounded writer to a `NamedTempFile` **in saves/**,
   mode 0600; finish serialization, flush then sync the new file. Any failure here
   leaves the primary and backup unchanged. Never truncate the primary directly.
3. If replacing, copy the exact validated old primary bytes into a second temp,
   flush/sync, `persist` to `<id>.json.bak`, then sync the saves directory. If this
   fails, abort primary replacement; primary is unchanged, backup may be old or
   refreshed. It is still a valid complete document. First create has no backup.
4. Recheck primary against expected stamp and entry policy; reject unexpected
   change. Replace primary using the new temp's `persist`. For create use
   `persist_noclobber` under the new slot lock and reject any existing primary,
   backup or assets identity. The no-clobber operation may leave an extra temp
   link on failure; do not call this a universal atomic multi-file transaction.
5. Sync saves directory. Only then return durable success plus stamp/revision.
   Closing/removing temps and process/workspace cleanup are checked. A failure
   after primary rename returns Replaced (known stamp) or Unknown, never Unchanged
or “saved successfully.” Failure to remove an unused temp is separately
   reported and does not undo a durable primary. Do not retry automatically.

After creating directories, sync new directories and their parent entries before
claiming the first save durable. Directory creation failure is Unchanged. Source
bytes for backup are validated before copying, and the old primary stays at its
name until replacement. Old backup is retained when no primary exists for a
recovery operation; recovery creates a fresh slot instead (§6).

### Crash/failure matrix

| Stop/failure point | Visible primary | Backup/recovery evidence | Report / next action |
|---|---|---|---|
| Before new-temp sync | Old or absent | Old backup | Unchanged; dirty, `/save` |
| Backup temp write/sync | Old | Old backup; disposable temp | Unchanged primary; dirty |
| After backup rename, before its directory sync | Old | Old or refreshed backup after crash | Abort on sync error; no primary replacement |
| After backup directory sync, before primary rename | Old | Durable old primary in backup | Safe explicit retry; no inference |
| After primary rename, before directory sync | New visible now; crash durability uncertain | Durable preceding primary | Replaced/Unknown; reconcile before retry |
| After final sync, before receipt delivery | New durable | Previous valid primary | Lost reply is Unknown; verify by reread, not blind overwrite |
| First create during rename/sync | Absent or new; may leave temp link | No backup yet | Recover by reserved ID/stamp; never create an empty placeholder primary |
| Helper killed/panics or response malformed | Depends on last completed step | Existing complete files, possible temps | Unknown unless pre-write failure is proven; no clean marker |

The parent reserves the create ID and expected encoded write identity before
starting its helper (ID generation/encoding occurs on storage worker). A retry
retains an attempt token containing ID and intended content stamp, so a lost create
receipt cannot produce duplicate stories. On explicit `/save` after Unknown,
read under lock: if primary exactly matches intended bytes, re-sync file/directory
and acknowledge that write without backup rotation; if it matches the old stamp,
perform the pending write; otherwise report Conflict. Missing/invalid primary is
not overwrite permission. New metadata time/revision is not generated for an
uncertain retry. Successful known writes followed by cleanup failure use the same
reconciliation route. Temporary-file cleanup targets only paths owned by that
attempt; do not delete arbitrary stale files on startup. Listing ignores them.

Carry this retry material through the port as an opaque app-owned `PendingWrite`
token: ID, previous optional stamp, intended stamp and bounded encoded bytes;
the application treats the bytes as opaque and neither parses nor edits them.
`StorageFailure` carries this token for a prepared failed write; `SaveGame` with
that token calls `reconcile`, never `create` again. It also retains the original
canonical revision/source for eligibility. This is boundary retry evidence, not a
second domain model. The infrastructure worker must publish the prepared token
before dispatching a mutating helper; if it panics later, the coordinator retains
the token for a fresh runner. Before preparation completes, no helper may write,
so a worker panic at that point is Unchanged. Encoding metadata is prepared once
on the worker with an injected clock; the helper validates the request and
performs all filesystem checks under lock. No JSON work moves onto the event loop.

## 6. Commands, session transitions and shutdown

### User surface

Proposed syntax (all IDs are checked; no shell-style path expansion):

```text
cyoa [--data-dir ABS_PATH] list [--after SAVE_ID] [--limit 1..100]
cyoa [--data-dir ABS_PATH] inspect SAVE_ID [--backup]
cyoa [--data-dir ABS_PATH] play --headless --backend claude|codex
cyoa [--data-dir ABS_PATH] play --headless --demo
cyoa [--data-dir ABS_PATH] play --headless --load SAVE_ID --limits current|original --backend claude|codex
cyoa [--data-dir ABS_PATH] play --headless --load SAVE_ID --limits current|original --demo [--backup]
/save
/save-copy
/list [AFTER_ID]
/load SAVE_ID --limits current|original [--backup]
/rewind N
/quit
```

`--backup` is also valid for live `play --load`; displayed on one example for
brevity. `--limits` is required with every load and invalid without load. No silent
restore default. `inspect` is read-only and uses stored active settings (§2).
Existing `/inspect` continues to query canonical state; `/diagnostics` additionally
shows latest storage error without overwriting generation diagnostics. Parsing
errors/zero/overflow rewind counts make no storage or inference call. Existing
ordinary-number and `//` semantics remain. `/save-copy` always creates another
slot; there is no arbitrary save-as path or overwrite existing ID command.

List and inspect never locate/authenticate a vendor, even if neither CLI exists.
Startup resume first validates the save and chosen source, then authenticates only
the explicitly chosen live backend; it must not autosave a restored-policy change
until the session is actually admitted. Loading does not generate a turn. A
zero-turn loaded game waits for explicit continuation, which makes one opening
call. For a new game, selection first saves the zero-turn state, then the existing
automatic opening starts only after that save succeeds. Save failure keeps the
selected zero-turn game; `/save` then finishes the pending opening intent exactly
once, unless a later quit cleared that intent. No save prompts.

Before selection, `/save`, `/save-copy` and `/rewind` reject with “Select a character
before saving or rewinding.” `/list` and `/load` are allowed while idle in any
stage; successful load discards unsaved lifecycle drafts and edit buffers. A failed
load keeps drafts, current game, revision, binding, retry intent and view exactly
as they were. At faulted generation state permit save/list/inspect/quit; load or
new generation remains unavailable until restart because the generator was lost.

Live source records are backend-neutral: either explicitly selected backend can
resume them, using the saved game and newly supplied runtime settings. There is no
default/fallback vendor and no saved auth data. Demo saves require `--demo` and an
exact supported scenario ID; live saves reject demo and demo saves reject a live
source. In-session load must match the already-selected Live/Demo source; switching
requires restarting. Persist no replay cursor: `HarbourDemo` indexes committed
turn count, so save/load/rewind/retry naturally select the right passage. Unknown
scenario or more than five committed harbour turns rejects demo resume; exhaustion
at five remains the existing explicit generation failure, never live inference.
A save is not proof that prose was produced by that scenario: validate structural
compatibility and label replay choices as fixed fiction, not semantic authenticity.

### Listing and recovery

List primary and backup-only IDs, sorted lexicographically by ID with exclusive
`after` cursor, at most 100 rows. Enumerate at most 10,000 directory entries per
operation and keep only the next page in memory; over that limit return an explicit
“save directory exceeds listing scan limit” error without presenting a complete
listing. Apply the overall storage deadline. Rows distinguish
`Valid { title, saved_at, turn_count, source }`, `Corrupt`, `FutureVersion`,
`Unreadable`, `Busy`, and `BackupOnly`; per-row errors do not fail other rows.
Use bounded full decode to verify metadata rather than display an untrusted title
as verified. Future-version rows expose the filename/version only. Probe backup
validity separately for error rows; no automatic recovery, deletion or rename.
Escaped/truncated display strings have an explicit display-only truncation marker;
do not alter the save. Avoid dumping raw JSON/diagnostic content in error messages.

`/load ID --limits ... --backup` explicitly reads the backup and validates its
embedded ID against ID. It recovers into an **unbound** game, marks it dirty and
creates a fresh slot automatically; preserves both damaged primary and backup for
inspection. Show old/new IDs. New-slot save failure leaves the recovered game in
memory and originals intact. Never rotate a corrupt primary over the recovery
source. No backup is an ordinary NotFound; future-version backup cannot recover.
Reading the primary with a healthy backup still returns the primary's error and
suggests this explicit backup command. No automatic data loss or overwrite prompt.

### Admission and transitions

“Ready” below includes generation Failed with an intact generator, but not Running,
Cancelling, Closing, Closed or Faulted unless stated. Dirty/Uncertain is a storage
condition, independent of generation failure.

| Intent/event | In-memory result | Disk/effect and subsequent admission |
|---|---|---|
| Select valid protagonist | New zero-turn game/revision, Dirty | Create slot; opening deferred until durable success |
| Accepted successful turn | Replace canonical game once, advance revision, Dirty | Exactly one autosave; committed prose shown separately from save status |
| Generation failure/cancel/stale/duplicate | Canonical game/revision unchanged | No autosave; same prior disk; explicit generation retry only when storage clean |
| `/save` Ready or Faulted with game | Canonical unchanged | Write/reconcile current snapshot; storage retry never calls generator or commit |
| Successful save receipt for same request/revision | Clean, bind/update stamp | Clear storage error only; pending opening may now start |
| Save failure before replace | Keep canonical, Dirty | Keep old binding/stamp; show “Turn committed in memory; save failed; use /save” |
| Replace/sync uncertainty | Keep canonical, Uncertain | Keep expected and intended stamps; require reconciliation on `/save` |
| While Dirty/Uncertain after failure | Keep playable state inspectable | Block turn/action/event/retry/rewind/load; allow save, save-copy, list, diagnostics, quit; prevents accumulating unsaved turns or discarding them |
| `/save-copy` with game | Canonical unchanged | Create new slot, switch binding only after success; old files intact |
| `/load` Ready and Clean, or idle preselection | Keep old state until whole read/validation/policy/source succeeds | On success atomic game+binding replacement, advance revision, clear preview/retry/edit; autosave only if policy changed data/settings, or recovery needs a slot |
| Load read/decode/policy/source failure | Exact old state/binding/revision/retry/edit retained | No write and no generation; error is separate storage evidence |
| `/rewind N` Ready and Clean | Existing checked rewind, advance revision, Dirty | Autosave rewound game immediately; failed save leaves rewind in memory, not rolled back; next generation blocked until saved |
| `/save`, `/load`, `/rewind` during generation/cancellation | Reject Busy, canonical unchanged | User may `/cancel`, wait for join, then retry command; no deferred load racing completion |
| Any mutation during storage work | Reject Busy | No queued turn/load/rewind; queries and quit/signals still work |
| Stale storage result/base revision | Ignore for canonical/clean state; retain diagnostics | Never update new binding; physical write may already exist, report as such; IDs never reused |
| `/list` idle or Faulted | No canonical changes | Bounded repository query; while active generation reject Busy to keep single effect policy |
| Counter exhaustion | Reject before changing state | No write or generation; quit may save unchanged state without advancing session revision |

Generation acceptance now reports a typed kind (outline/cast/turn), or an equally
explicit `CanonicalChange` event. Do not use `turn_count` change alone to identify
all saves: selection/rewind/load also matter and rewind can repeat an old count.
On in-session load, `replace_game` uses fresh revision, leaves request ID counter
monotonic, rejects busy state and clears prior failure only on success. Synthetic
late completions must still fail `complete` eligibility after load even though
normal policy rejects overlap.

### Quit and error paths

Use coordinator shutdown states `Open → StoppingGeneration → SavingFinal →
DrainingOutput → Closed`, retaining both an initiating failure and a storage result.
Controller `Closed` alone no longer means application shutdown complete. During
StoppingGeneration invalidate acceptance, cancel and join through the existing
runner; only then snapshot canonical state. A turn accepted before quit remains;
a queued success after quit/cancel does not. No post-close generation dispatch.

| Trigger | Behavior and final save | Exit status |
|---|---|---|
| `/quit`, EOF, idle SIGINT | Close admission; cancel/join if needed; one final save if selected game, including zero turns; no draft save | 0 only after successful storage and output drain; otherwise 1 |
| Active-generation SIGINT or `/cancel` | Preserve today's cancel-and-join, return to failed/cancelled play; **does not quit** or save rejected result | Session continues; later quit saves last canonical game |
| SIGINT during storage while session open | Request quit; allow current write its remaining bounded deadline, no abrupt claim of rollback | Same as normal quit |
| Quit during write | Finish/reconcile that operation; if it durably saved the same revision, it satisfies quit save (no duplicate rotation); otherwise one final save/reconciliation attempt | 1 if final durability cannot be established |
| Quit during load/list | Cancel read helper, join, discard unaccepted candidate; save old canonical game | As above |
| Input UTF-8/read error, output broken/stalled/overflow | Latch original error; cancel/join generation, bounded final canonical save; do not require working output to save | Always 1; best-effort nonblocking error report |
| Generation worker panic | Preserve canonical state and faulted generator; allow inspection/save/quit; on shutdown final save independent of destroyed generator | 1 on eventual exit for worker fault, even if save succeeds |
| Storage worker panic/helper failure | Preserve canonical, Unknown when write may have begun; mark storage fault; final shutdown gets at most one new reconciliation attempt via a fresh storage worker | 1 if no durable receipt; retain failure cause |
| Repeated quit/EOF/SIGINT while closing | Idempotent; never restart deadline or enqueue unlimited saves | Same latched outcome |

Quit autosave is the specified automatic final save attempt, including after an
earlier failed autosave; it is not a regeneration retry. At most one current
operation (remaining 10-second deadline) plus one final 10-second attempt, then
existing finite helper cleanup. Do not retry an unsuccessful final attempt forever
or wait for a user answer at EOF. Failure exits nonzero and explicitly says the
in-memory revision may be lost. No claim of successful durability from a failed
write. If an existing unknown operation must be reconciled, that counts as the
final attempt, not extra unbounded work.

Once generation and storage are joined, retain the existing **absolute one-second**
output drain and 1 MiB per-stream queue limits, 64 KiB / 32-attempt pump budgets.
Storage waiting must continue pumping/observing input; an output fault is latched
and stops rendering to that sink but cannot skip cleanup/save. At the final drain,
SIGINT still interrupts with an error. Error reporting after terminal guards are
restored remains best-effort nonblocking. Update memory-only/discard notices only
when wired persistence is real; report save ID, canonical turn count and durability
separately from generation completion.

## 7. Ranked risks and resolved proposals

| Rank | Choice and rationale | Rejected alternative / failure behavior | Acceptance |
|---|---|---|---|
| 1 | Serialize accepted canonical snapshots; separate Dirty/Uncertain and generation result | Rolling back paid-for accepted prose or retrying inference after ENOSPC loses/duplicates story | A7, A8 |
| 2 | Backup copy+sync before primary replace+directory sync | Rename-away primary creates a missing-primary window; blind backup rotation destroys last good copy | A5, A6 |
| 3 | Isolated bounded helper for blocking filesystem work | Main-thread I/O breaks cancellation; detached threads can write after shutdown | A6, A10 |
| 4 | Checked established-world reconstruction; snapshots authoritative | Generation constructor prunes history; delta replay resurrects capped events | A1–A3 |
| 5 | Stable lock plus exact-byte optimistic stamp | Lock-only last-writer-wins silently loses another process's turn | A5, A8 |
| 6 | Busy rejection and monotonic revision on load | Deferred load while generation completes increases cancellation/identity races | A8, A9 |
| 7 | Versioned v1 with strict known-field typing, ignorable optional extensions | Invented legacy migrations or all-fields-defaulted loading conceals corruption | A2, A4 |
| 8 | Stable random-suffix IDs and explicit backup recovery into new slot | Title-only paths collide; implicit backup fallback hides corruption | A5, A6 |
| 9 | Live source neutral, demo scenario pinned | Persisting a concrete generator/cursor causes replay drift or accidental live calls | A11 |
| 10 | Block further mutations after save failure | Unlimited dirty play increases loss; automatic retry can hide persistent storage errors | A7, A10 |

No genuinely unresolved product decision blocks this scoped implementation plan.
The proposals above choose ordinary policies within the handoff: no save prompts,
no new override permissions, no automatic backend changes, and preservation of
existing accepted state. They are not newly recorded user decisions or enforced
contracts. If implementation wants draft persistence, force-overwrite, live/demo
conversion, silently lossy repair, weaker durability or changed active-SIGINT
semantics, stop and obtain an explicit product decision rather than treating this
plan as authorization. Physical crash durability on untested filesystems, future
schema migrations remain named evidence limits,
not questions for the user to settle before vendor-independent work.

## 8. Acceptance matrix (planning targets; S1–S6 evidence above)

IDs below denote test groups with concrete targets and expected observations.
Assertions use independent fixture expectations, not encoder output as the sole
oracle. Error → edge → nominal order applies to each group.

| ID / target | Required regression cases and pass evidence |
|---|---|
| A1 `infrastructure/tests/persistence_roundtrip.rs` | Compose existing full story through real generation use cases, save, reconstruct, rewind and continue; equal known game fields under same limits, exact raw/trace UTF-8 bytes (empty, whitespace, CRLF, Unicode), input, provenance zero-vs-absent cost, cast role/order/namesakes/IDs, selected position, four styles, delta null/empty, chapters/retitles and snapshots. Compare next captured prompt/request counts. No commit during decode. Freeze independent full fixture and a zero-turn selected-second-playable fixture. |
| A2 `core/tests/restored_world.rs`, `infrastructure/tests/save_validation.rs` | First reject empty/duplicate restored roles without changing generation tests; accept one established playable despite raised minimum and NPCs despite zero cap. Reject invalid selection, duplicate/missing IDs, metadata title/count/ID mismatch, malformed fields, blank required text, lossy event/action normalization, invalid costs/limits/overflow, duplicate keys, oversize/depth, symlink/nonregular files. Required/default/null table exercised. Unknown action maps Other; unknown style preserved/fallback. Snapshot differing legitimately from delta replay accepted without changing it. |
| A3 `infrastructure/tests/persistence_limits.rs` | Creation cap 4 → disk load current cap 1/bridge 0/NPC 0/minimum above existing cast → save → original cap 4 → save/load/rewind/continue, with known newest-event lists for **every** snapshot. Original metadata unchanged, no resurrected events, no pruned characters, both policies in captured prompts/schema, zero-turn opening uses selected cap. |
| A4 `infrastructure/tests/save_compatibility.rs` | Frozen v1 minimal/full/additive fixtures; reject missing/zero/future versions and wrong known types; omitted defaulted fields produce expected domain values; required fields missing fail; optional extras warn and known fields survive re-save; synthetic migration-chain dispatcher tests explicitly not legacy support. Reads never rewrite files. |
| A5 `infrastructure/tests/filesystem_repository.rs` | Missing root list empty; first create, subsequent replace and exact previous-byte backup; constant clock cannot break ordering; injected entropy collision/failure and revision exhaustion; lock busy, two independent writers loading same stamp then one conflicts; random IDs with hostile/Unicode titles, traversal IDs rejected; zero/no-follow/permissions/hardlink paths; stable lock survives primary rename and crash release. Actual files verified and no clobber on create. |
| A6 `infrastructure/tests/persistence_failures.rs` | Fault each open/read/encode/write/flush/file sync/backup persist/directory sync/primary persist/cleanup step. Assert primary/backup exact bytes and visibility status from §5. Real helper kill handshakes at each crash window, receipt loss, first-create uncertainty reconciliation, corrupt primary + healthy backup preserved, backup-only/future/corrupt/missing recovery into fresh slot. Power-loss claims limited to protocol ordering, not simulated process kill. |
| A7 `application/tests/persistence.rs`, `presentation/tests/persistence.rs` | Fake repo errors before/after replacement leave accepted turn in memory with Dirty/Uncertain; explicit `/save` retries storage only; call counts show no inference or recommit. Only selection/turn/rewind/policy-changing load triggers saves, no preview/outline/cast/failed/cancelled/stale/duplicate completion. Dirty admission rules, deferred opening exactly once, save-copy rebinding only on success. |
| A8 `presentation/tests/persistence.rs` | Synthetic storage keys/base-revision mismatch cannot clean/rebind loaded game; load errors preserve entire old stage/selection/revision/binding/retry/edit; successful load advances counters and rejects injected old generation success; busy rejection while generating/cancelling/writing; quit wins queued success; two writer conflicts and uncertain retry never overwrite different file. |
| A9 `cli/tests/headless_persistence.rs` | Shipped binaries in temp data roots and credential-free fixture adapters: explicit limits required, save/list/inspect/load/rewind across process restart, zero-turn load waits for one opening, repeated changed-limit cycles (typed fixture composition for nondefault settings), rewind persists before next request, canonical queries reflect restored game; list/inspect/invalid load launch no vendor auth executable. Both fixture backend choices exercised symmetrically. |
| A10 `cli/tests/persistence_shutdown.rs` | Controlled silent/blocked helpers while input remains open; SIGINT/quit/EOF/input error/broken or full stdout+stderr; actual generation child and helper exit/reap; fixed deadline not extended by repeated signals; last canonical snapshot only. Save failure yields exit 1 even with successful generation; I/O/worker errors remain nonzero after successful save; no selected game means no file; retained output backpressure and absolute final drain regressions pass. |
| A11 `cli/tests/demo_persistence.rs` | Play harbour first N turns, save/restart under demo, next passage is N+1; cancel consumed result/retry, rewind and resume all five; source mismatch/unknown scenario/future save make zero inference calls; demo exhaustion never switches vendor; live saves accept either explicitly chosen fixture backend with optional observed provenance unchanged. |
| A12 shared gate and documentation | `check_architecture.sh`, fmt/Clippy, workspace Nextest, Cargo doctests, all existing 37 behavioral mutations and new registered regressions, domain-only mutants including restored-world API. Update moved patch targets without weakening assertions. No runtime credential/Python dependency, no placeholder registrations, precise partial contract statuses and evidence README. |

Fault injection: private `FileOps` methods and deterministic `Clock`/ID source,
real same-directory temp files, helper barrier pipes for termination windows,
controlled spawn failures, fake application repositories and synthetic controller
completion keys. Handshakes establish ordering; test watchdog expiration is a
harness failure, not proof that behavior was detected. Read-only permission tests
must work when run as an ordinary user; inject permission failure as well rather
than assume chmod reproduces ENOSPC/EIO. Crash tests inspect complete file bytes
and recovery on a new process. Add focused handwritten mutations for accepting a
stale storage receipt, autosaving an unaccepted turn, rotating corrupt primary,
marking post-rename failure clean and retrying generation on save failure; each
must fail its specific registered behavioral assertion, not setup/compiler errors.

## 9. Dependency-ordered atomic implementation commits

Each row is a passing thematic commit including real tests, registry entries for
its implemented obligations and truthful status. Run focused red tests first,
edge tests next, nominal integration last, then refactor. No knowingly broken
intermediate commits. Names above are proposed files, not files that already exist.

| Step | Depends on / changed modules | Error → edge → nominal evidence and registration |
|---|---|---|
| S1 Checked established cast | Baseline; `core/world.rs`, new core restored-world tests | A2: empty/exact duplicate rejection, namesakes and generation-limit independence, lossless ordered valid cast. Register under ARCH-002/STATE-001/LIMITS-001/IDENTITY-001; core domain sweep includes API. No serde. |
| S2 App persistence vocabulary and save codec | S1; `application/persistence.rs` types/ports; infrastructure DTO/codec/migrations; frozen fixtures | A1–A4 decode failures first, omission/null/unknown-field edges, full/zero-turn round trips. Fake repository/use-case tests restore policy and error propagation. Register real tests under ARCH-001/002, TEXT-001/002, STATE-001, LIMITS-001, IDENTITY-001/002, CHAPTER-001, PRODUCT-001. Migration support only v1. |
| S3 Local atomic repository | S2; infrastructure repository/filesystem; scoped Cargo deps/lockfile for directories/time/hash/random/locking as needed | A5/A6 injected failures, concurrent conflicts and lost-reply reconciliation, normal create/backup/list/recovery. Private clock/FileOps; directory-sync ordering. Register PRODUCT-001/TEXT-001/ARCH-001. Do not expose blocking adapter to headless yet. |
| S4 Supervised storage execution | S3; infrastructure helper transport, CLI internal helper entrypoint, presentation storage runner | A6/A10 spawn/panic/timeout/blocked-helper cases, cancellation and reply-loss edges, actual child successful CRUD; no auth, no global cwd changes. Register PRODUCT-001/STREAM-001. Application port unchanged; no concrete dependency in presentation. |
| S5 Canonical persistence coordination | S4; presentation coordinator/session/runtime, application rewind integration; CLI composition tests | A7/A8 storage error then stale/cancel races, selection/deferred-opening/save/load/rewind/quit nominal path. Counter capacity checks and independent failure views. Register PRODUCT-001/STREAM-001/LIMITS-001; preserve existing session/worker tests and refresh moved mutations. |
| S6 Headless commands and resume | S5; commands/headless/main and CLI tests, README usage | A9–A11 invalid syntax/source/limits first; zero-turn, backup recovery, demo retry/exhaustion, both fixture backend restart stories. Auth-free list/inspect branch, temp roots in **all existing play tests** so new autosave cannot write real user data. Register PRODUCT-001/STREAM-001/BACKENDS-001/ACCEPTANCE-001. Replace memory-only assertions with real persistence assertions only as required by this Phase 2 feature, preserving cleanup and story assertions. |
| S7 Fault acceptance, mutations and completion record | S6; composed full-story tests, mutation manifest/patches, docs/status/evidence | Run all A1–A12, add the focused mutations above and validate their exact failing assertions. Full shutdown/output matrix, existing mutations and domain sweep. Only now claim implemented Phase 2; remaining TUI/export/overrides stay pending. |

Every step runs `cargo fmt --all -- --check`, relevant Clippy with warnings denied,
focused tests and `bash scripts/check_contracts.sh` before considering implementation
complete/committing its completion claim. Install pinned cargo-mutants 27.1.0 and
cargo-nextest 0.9.132 as documented in [domain mutations](../testing/domain-mutations.md)
and fetch locked dependencies before offline gates. The full gate runs workspace
Nextest profile `workspace` (no retries/no fail-fast), Cargo doctests, handwritten
mutations, and core-only mutations with the separate `domain-mutations` profile.
Compile-invalid mutants and per-test nontermination remain distinct from assertion
failures. No behavior-changing survivor allowance or weakened historical tests.

## 10. Handoff completeness audit

This maps every requirement/decision in the handoff to concrete work and evidence.
“Covered” here means specified and testable in this historical planning audit.
Actual S1–S6 enforcement and remaining S7 work are recorded above and in their
evidence files; this table alone does not establish feature enforcement.

| Handoff item | Plan location | Acceptance / implementation |
|---|---|---|
| Task: save/load/list, atomic backup, migrations, autosave, headless integration; documentation-only atomic commit | §§1–9; planning verification below | A1–A12 / S1–S7 |
| Read-first contracts, notes, own backend, acceptance, README, controller/presentation handoffs and actual runtime | §§1–2 baseline evidence; no new live claim | Existing source/evidence audit; A12 |
| Existing requirement 1: layers/CQRS | §2 ports and ownership | A7, A12 / S2,S4,S5 |
| Existing 2: original/current, all caps/context, metadata, established cast, checked position | §§2,4 | A2,A3,A9 / S1,S2,S5,S6 |
| Existing 3: envelope/title/time/count, flat style, complete state/audit, prompt trace, required/default fields | §4 DTO table and mapping | A1,A2,A4 / S2 |
| Existing 4: Value migrations, future rejection, v1 fixtures/additions, no invented import metadata | §4 compatibility | A4 / S2 |
| Existing 5: NamedTempFile, one backup, XDG/assets, turn/quit autosaves without prompts | §§3,5,6 | A5–A7,A10 / S3–S6 |
| Existing 6: canonical only, stale/cancel/duplicate, joins/revisions, no mutex/model coupling | §§2,3,6 | A7,A8,A10 / S4,S5 |
| Existing 7: identity/order, retitles/rewind, snapshots/null-empty/exact audit, derived views | §4 | A1–A3,A9 / S1,S2,S5 |
| Existing 8: user commands and deferred features | §§1,6 | A9,A11 / S6 |
| Seam: GameState/WorldCast constructor limitations | §2 direct findings, §4 | A1–A3 / S1,S2 |
| Seam: turn/character/summary/limits/style/text checked types and audit | §§2,4 | A1–A4 / S2 |
| Seam: application generation/rewind, no repository | §2 new inward port, §6 rewind | A7,A9 / S2,S5 |
| Seam: session canonical ownership/from_game, runtime/worker join | §§2,6 load and closure | A7,A8,A10 / S5 |
| Seam: headless/output/terminal bounds and notices | §§3,6 shutdown | A10,A12 / S4–S6 |
| Seam: main/commands auth ordering | §6 startup/read-only paths | A9,A11 / S6 |
| Seam: named existing regression files and generation wire reference only | §§1,2,8; retain full gate | A1–A4,A8–A12 / S1–S7 |
| Decision 1: exact DTOs, IDs/optionals, both limits, history/provenance/traces, corrupt metadata/history | §4 complete table and validation | A1–A4 / S1,S2 |
| Decision 2: ports/use cases, checked IDs/list views, mapping, clock, session identity | §§2–4 | A5,A7,A8 / S2–S5 |
| Decision 3: collisions/traversal/rename/symlink/overwrite/multiple writers/platform scope | §3 paths and locking | A5,A6 / S3,S4 |
| Decision 4: temp placement, sync order, backups/first-save/crash/recovery, before/after errors, corrupt primary | §5 and §6 recovery | A5,A6 / S3,S4 |
| Decision 5: committed turn versus failed write, dirty/retry/admission without inference | §6 transition table | A7 / S5,S6 |
| Decision 6: commands while active, revisions/identity, exact load-failure preservation | §6 admission | A8,A9 / S5,S6 |
| Decision 7: quit/EOF/SIGINT/I/O/panic snapshots, status/order/bounds/scheduling | §§3,6 shutdown | A10 / S4–S6 |
| Decision 8: preselection/zero-turn/drafts/first timing/stage availability | §§1,6 surface and table | A1,A7,A9,A10 / S2,S5,S6 |
| Decision 9: rewind durability, restored history/limits and next prompt | §§4,6 | A1,A3,A9 / S2,S5,S6 |
| Decision 10: demo/backend source, no replay drift/live fallback/runtime serialization | §§2,4,6 | A11 / S2,S6 |
| Decision 11: explicit limits without overrides, unknown styles/defaults | §§4,6 | A2,A3,A9 / S2,S6 |
| Decision 12: corrupt/future listing, recovery, input/filesystem bounds and truthful diagnostics | §§3,5,6 | A2,A4–A6,A10 / S2–S6 |
| Deliverable 1–4: scope/status, contract evidence, named architecture, format/restore/compatibility | §§1–4 | A1–A12 / S1–S7 |
| Deliverable 5–7: transitions, ranked choices/uncertainty, ordered atomic commits | §§5–7,9 | A5–A12 / S3–S7 |
| Deliverable 8: full acceptance/fault injection, temporary roots, no credentials | §8 every row and injection paragraph | A1–A12 / S1–S7 |
| Deliverable 9: every requirement mapped, uncertainty honest | This audit and §7 | Planning audit; A12 / S7 |
| Verification: planning checks, unchanged normative coverage, atomic docs commit; no push/main/runtime work | Below | Planning only |
| Later implementation discipline: red/edge/nominal, registry, full gate, mutation scope/failure truth | §§8–9 | A12 / every step |

### Planning verification and remaining evidence

The source inspection above establishes the implementation seams; test names and
current coverage were checked against the existing registry. New acceptance names
are deliberately confined to this document. Planning validation uses
`bash scripts/check_architecture.sh` and `git diff --check`, plus a local relative-link
check. No model calls or mutation sweeps are needed for this documentation-only
stage under the handoff. Runtime guarantees await the implementation and its full
contract gate; none are inferred from this document. Commit this plan and the
README/handoff navigation links together; do not push, merge or modify main.
