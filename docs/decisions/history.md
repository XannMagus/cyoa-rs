# Decision history and implementation evidence

Dated evidence records moved out of [the contracts](README.md) on 2026-10-05 so that
file holds only current contract text. These records are unedited: they describe
what was true when each was written, and later contract text supersedes them.

## Supervisor repair decisions (2026-09-25)

STREAM-001/TEXT-001 require finite work per readiness iteration and capped capture
on every exit path. Diagnostics retain exact available bytes up to each configured
bound, with explicit `Complete` versus `Prefix` metadata; a prefix never invents a
missing-byte count. Group shutdown precedes final capture. Observing exit must not
reap/release the child's PID before signaling its group. Success requires verified
reaping, complete input delivery and no fatal I/O/cleanup failure. Initiating and
cleanup failures remain independently inspectable. The guard's best-effort Drop
is only an unwinding backstop. Resource-owning capture/framer/supervisor methods
implement these transitions; callers cannot bypass them through mutable buffers.
See the [repair evidence](../../reviews/2026-09-25-supervisor-repairs/README.md).

Cancellation wake registrations own their callback lifetime. Request-scoped
subscriptions are removed on drop, releasing their pipe descriptors without
waiting for source destruction. The flag is the source of truth. A callback panic
does not skip later callbacks: notifications run outside the lock and the first
panic resumes after all callbacks are attempted. Callbacks must finish promptly;
deregistration cannot undo a callback already taken by concurrent cancellation.

Each process request owns a unique scratch directory (`RequestWorkspace`, Unix
mode 0700); the supervisor consumes its `ProcessSpec`, uses that cwd without
changing global cwd, keeps prepared files through child cleanup and removes them
on completion/unwinding. Normal directory-cleanup errors are observable. Tests
must confirm actual PID disappearance, including timeout, through kernel liveness
checks; absence of `/proc` is not evidence of cleanup. The exercised platform is
Linux; other waitid-capable Unix builds remain unverified, and platforms without
that primitive return `Unsupported`.

The process gate also retains seven STREAM-001 mutations alongside the four
Phase 0 mutations. The wake mutation disables registration while holding the pipe
writer open, so neither a byte nor EOF can wake the poll; an isolated long-poll
test distinguishes the wake from periodic token checks. Production correctness
checks must never be removed simply to make a redundant mechanism testable.

## Phase 2 step 1 reconstruction evidence (2026-10-04)

`WorldCast::restore` now checks an established cast without applying generation
NPC caps or playable minimums. It rejects an empty playable role and exact duplicate
records within each role instead of silently changing saved positions. Distinct
namesakes and role order remain intact, and selection still uses `World::select`.
Registered core regressions cover these ARCH-002/STATE-001/LIMITS-001/IDENTITY-001
obligations. `WorldCast::new` retains its generation behavior. Save DTO mapping,
disk round trips and user-facing persistence remain pending; this constructor
alone does not establish them.

## Phase 2 step 2 save-boundary and application evidence (2026-10-04)

Application-owned persistence values, repository ports and `PersistenceUseCases`
now separate save commands from load-policy application and read-only list/inspect
queries (ARCH-001/002). Fake repositories establish one call per operation, no
automatic retries, preservation of storage visibility/pending/cleanup evidence,
pre-cancel admission, post-read cancellation and durable-write receipts. Loading
applies the selected current/original policy without writing automatically;
a later presentation coordinator must persist accepted policy changes.

Infrastructure's separate version-one save DTOs/codec reject duplicate JSON keys,
wrong known-field types, missing required values, invalid metadata and lossy
text/event/action/cast reconstruction. Unknown optional fields are reported and
omitted on re-save. Only version one is supported; private synthetic migration
chains test dispatch mechanics without inventing legacy format support. UTC
metadata uses checked millisecond precision (finer clock precision rounds down);
write revision and exact-byte SHA-256 stamps establish ordering/conflict evidence,
not the clock. Files are bounded to 64 MiB including their final newline.
A serde default is allowed only where the defaulted value can be valid (user
decision, 2026-10-05): the world cast (`characters`) and stored character `id`
are required fields, because an empty cast or ID never reconstructs. Their
earlier defaults advertised a tolerance that always failed later, and they are
now reported as the missing field itself.

Registered round trips use independent minimal/full/additive fixtures and actual
scripted generation through failure, cancellation, rewind and the next request.
Selection, role order/namesakes/stable IDs, Unicode-lowercase distinctions,
chapter markers/retitles, null/empty updates, flat styles, observed provenance,
zero-versus-absent list-price estimates and exact raw/trace audit text survive
(STATE-001, IDENTITY-001/002, TEXT-001/002, CHAPTER-001). Snapshot memory remains
authoritative without delta replay. Repeated codec cycles narrow every snapshot,
preserve original settings, then restore original settings without resurrecting
lost events or applying generation-only cast bounds (LIMITS-001).

These are application/codec checks for PRODUCT-001, not filesystem durability,
process/helper acceptance, autosave or headless persistence. Those remain S3–S7.
The existing mutation gates and domain-only scope remain intact. See
`reviews/2026-10-04-persistence-s2/README.md` for commands, observed failures,
verification and the acceptance-scope audit.

## Phase 2 step 3 local storage evidence (2026-10-04)

LocalRepository implements the inward port with Linux directory-relative no-follow
reads, checked entry ownership/link/type, stable nonblocking locks, exact-byte
optimistic conflict checks, bounded paging and atomic primary/backup replacement.
New file/backup sync and backup directory sync precede primary replacement; only
final directory sync authorizes a receipt. Post-replacement failure preserves
visibility and exact prepared bytes for reconciliation. Corrupt primary never
rotates into a good backup. Explicit recovery creates a fresh slot. Registered
real-file and private fault regressions enforce PRODUCT-001/TEXT-001/ARCH-001;
repeated disk restore cycles also cover LIMITS-001. See
`reviews/2026-10-04-persistence-s3/README.md`. This is blocking adapter evidence,
not supervised helper execution, autosave or headless persistence (S4–S7).

## Phase 2 step 4 supervised storage evidence (2026-10-04)

SupervisedRepository executes bounded, strictly typed private helper requests
through the existing vendor-neutral process supervisor. The internal binary
entrypoint precedes Clap/auth, uses no ambient child environment, and owns a
private scratch cwd. Reply candidates cannot override child failure, cancellation,
timeout, output bounds or cleanup failure. Mutating failures retain exact prepared
identity; lost replies reconcile rather than create another slot. The unchanged
GameRepository port is still inward-owned. Replacement needs a read-only preparation
child to obtain the revision missing from SaveTarget, then one mutating child under
the same operation deadline; this ordinary S4 scheduling refinement is documented
in the plan. The worker publishes retry evidence before a mutating helper launches.

Presentation StorageRunner accepts owned commands/queries and request/revision
keys. It never performs JSON/disk work or joins a live thread during polling.
Preparation and terminal events remain separate; panic before preparation is
Unchanged, panic after preparation retains Unknown and the token. Closing/dropping
cancels and joins supervised work without a detached writer. Registry entries under
PRODUCT-001/STREAM-001 cover real binary CRUD, fixture child cancellation/deadlines/
reply loss and kernel PID disappearance, process-kill crash-window file observations,
lock release, reconciliation, worker spawn/panic/close and strict protocol admission.
No headless/autosave coordination or Phase 2 completion is claimed (S5–S7).
See `reviews/2026-10-04-persistence-s4/README.md` for the scope audit and gate.

## Phase 2 step 5 canonical coordination evidence (2026-10-04)

PersistedSession now coordinates the generation runtime with the storage runner
through inward ports (PRODUCT-001/STREAM-001/LIMITS-001). Typed accepted turn
changes trigger autosaves; previews, outlines, casts, failed/cancelled/stale results
never save a turn. Initial selection saves zero-turn state before one deferred
opening. Dirty/Uncertain failure preserves canonical state and blocks generation;
storage retry reconciles exact prepared identity without recommitting/regenerating.
Save-copy binds only on success and a failed copy retains an earlier uncertain
attempt. Load advances existing revision/request counters, preserves the old
session on failure, checks source compatibility, and persists policy changes or
backup recovery into a fresh slot. Shutdown joins generation before final storage,
lets an active durable write satisfy quit, cancels read-only work, reserves counter
capacity and bounds the final attempt. Worker panic remains an exit failure even
when quit precedes polling its completion. Both error views remain independent.

Port-based tests, private stale/counter tests, session restore admission and actual
shipped-helper primary/backup observations are registered. The full shared gate
passed; see `reviews/2026-10-04-persistence-s5/README.md`. Public headless commands
and output/shutdown composition remain S6; complete Phase 2 fault/mutation
acceptance remains S7. No new live backend claim or schema/prompt change.

## Phase 2 step 6 public headless persistence evidence (2026-10-04)

Public list/inspect are credential-free, and startup resume validates the save,
explicit current/original policy and Live/Demo source before vendor resolution
or authentication. Only admitted sessions persist restore-policy changes. Both
backend fixtures create and resume live saves under the peer backend, retaining
original records and captured future context (BACKENDS-001/ACCEPTANCE-001).
Demo resumes select by committed count, preserve all five passages through
consumed-result cancellation/retry and rewind, and reject source/scenario/version/
excess-count mismatches without inference or vendor fallback.

The headless driver now uses the canonical persistence coordinator for autosave,
/save, /save-copy, /list, /load, /rewind and final save. Zero-turn resumed games wait
for explicit opening; backup recovery creates a fresh slot and preserves both
original files. Successful load clears edits; failed load preserves them. Storage
evidence is separate from generation diagnostics. The same finite output queues
and absolute final drain remain, after both owned workers finish. Input/output
errors still save canonical state and remain errors; controlled silent helpers
prove input responsiveness, two-attempt shutdown bounds and actual PID reaping.
Selection reserves its opening request/revision capacity before canonical change.
Every existing play test has a temporary data root, with story/cleanup/output
assertions retained and actual five-turn save/audit bytes also checked.

These PRODUCT-001/STREAM-001/BACKENDS-001/ACCEPTANCE-001 regressions are registered.
The S6 evidence audit is `reviews/2026-10-04-persistence-s6/README.md`. S7's full
composed fault acceptance and focused persistence mutations remain pending;
Phase 2 completion, TUI, exports and prompt overrides are not claimed. Backend
schema/prompt/protocol code and live verification records are unchanged.

## Phase 2 step 7 acceptance and mutation evidence (2026-10-05)

The A1–A12 audit now records executable domain, codec, application, local-file,
helper, coordinator and public binary evidence separately. The full generated
story is saved to actual atomic files, reloaded, rewound, saved/reloaded and
continued with exact request/state/audit and previous-byte backup assertions.
Both restore policies additionally reach captured generation prompts/schema,
accepted snapshots and subsequent disk inspection. Original settings and every
snapshot remain authoritative (LIMITS-001/STATE-001/TEXT-001/IDENTITY-001/002/
CHAPTER-001/ACCEPTANCE-001).

Explicit binary backup recovery covers corrupt, future-version and missing
primaries without rewriting the originals. Accepted-turn shutdown combines
blocked mutating helpers with broken/full stdout and stderr, retaining canonical
state, exact previous primary bytes, current+final write bounds and actual reaping
of every preparation/write child. Real backend-fixture quit/EOF/cancellation/input
and broken-output regressions now also inspect the final save (PRODUCT-001/
STREAM-001/BACKENDS-001).

Five additional isolated mutations protect stale storage acceptance, autosaving
unaccepted results, rotating corrupt primary bytes, marking post-replacement
failure clean and retrying inference after save failure. All 42 handwritten
mutations require their exact registered tests; the five new entries additionally
require specific assertion messages. Compiler/setup errors and stale patches
remain gate failures. The existing domain-only automated scope is unchanged.

Phase 2 is complete on the exercised Linux platform. Physical power-loss behavior
on arbitrary filesystems is not inferred from process-kill tests. Only save v1 is
supported; imports and future migrations, TUI, exports, images and arbitrary
prompt overrides remain later work. LIMITS-001 is now enforced by its actual
complete restore coverage; PRODUCT-001 remains partial for TUI/export. See
`reviews/2026-10-05-persistence-s7/README.md` for the requirement audit and checks.
