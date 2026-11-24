# Iso-functional application cleanup

User approval: 2026-11-03, after review of
[the concrete plan](../../docs/plans/iso-functional-cleanup.md).
Scope: existing CLI/REPL behavior, reusable application policy, no new gameplay.
The working tree also contains earlier planning documents; this cleanup does not
implement their deferred features.

## Ownership audit

- Domain owns typed generation/repository business capabilities, turn intent,
  checked save identity/revision/time, source and valid save snapshots. Associated
  context/report/attempt types keep application diagnostics and cancellation
  machinery outside domain entities. List pagination remains an application query.
- Application owns canonical lifecycle, command admission, current-request/revision
  acceptance, autosave, load/rewind, recovery and bounded final-save policy.
  `SessionView` borrows immutable canonical state; persisted-session clients never
  receive the underlying controller. Execution contracts contain no concrete runner.
- Infrastructure owns generation/storage threads, joining, recovery capture,
  blocking startup jobs, helper processes and encoded documents. `PendingWrite`
  retains an immutable private adapter payload through type erasure; equality still
  compares exact payload contents, rather than only metadata or pointer identity.
  Its application API has no byte accessor. Infrastructure validates foreign
  handles before writing. This boundary assumes trusted adapters, as the original
  repository port did; it is not a hostile-plugin security boundary.
- Presentation owns slash parsing, edit buffers, terminal signals, rendering and
  bounded output draining. Application `Shutdown::Complete` means storage and
  generation obligations ended; output success/failure remains presentation-local.
- Main wires concrete adapters. Library clients explicitly configure the helper
  executable; their own `current_exe()` need not implement the helper mode.
- Successful and failed execution evidence retains raw response, available
  provenance and exact diagnostic captures separately from saved story state.
  Completion carries evidence atomically, preserving the worker event sequence.

No vendor request, schema, prompt, helper protocol, save version, CLI command or
backend compatibility rule changed. Both backends remain co-equal. Existing live
backend evidence is historical; no fresh authenticated claim is made here.

## Regression relocation

Original assertions moved with their responsibilities:

| Before | After |
| --- | --- |
| presentation session + private session tests | application session tests |
| presentation runtime/coordinator unit tests | application coordinator tests with controlled execution ports |
| presentation worker/storage tests | infrastructure executor tests |
| presentation persisted-session integration | infrastructure session-persistence integration |
| terminal scoped-job assertions | infrastructure scoped-job assertions |

The old application empty-prepared-document assertion now exercises infrastructure's
real private document constructor in `prepared_attempt`; fixture attempts in
application tests assert exact forwarding of their own private test payload.
Shutdown assertions now use `Complete` instead of `DrainingOutput`/`Closed`, because
output drain no longer changes application status. Their original sink-error,
canonical-state, deadline and cleanup assertions remain.

The required-test registry follows these moves. Handwritten patches retain their
original behavioral faults and exact assertion-failure requirements. Their target
paths/context follow the new locations. The isolated-copy harness now omits deleted
tracked paths when snapshotting an uncommitted worktree; it still includes all
existing tracked and nonignored new source and rejects stale patches/build errors.

## Added acceptance

- A complete library client without a presentation dependency uses real generation
  and storage runners plus an explicitly configured helper. It exercises brief,
  edited outline/cast, selection/save-before-opening, action, failure/evidence,
  explicit retry, cancellation, rewind, save copy, load and final shutdown.
- Private prepared document bounds, exact bytes/equality and redacted debug output;
  foreign attempt rejection leaves the existing document unchanged.
- Read-only canonical views and unavailable application byte access have compile-fail
  examples. Scoped-job cancellation waits for cleanup and rejects a late result.
- Core-only tests cover moved identity/counter/time/source/snapshot/turn-intent rules,
  independently of infrastructure and application tests.

Affected contracts: ARCH-001, ARCH-002, PRODUCT-001, STREAM-001, TEXT-001; TESTING-001
registers the added core-only coverage. Existing game behavior contracts are unchanged.

## Validation

Baseline: [460 runtime tests, 7 doctests, 45 handwritten faults and the original
domain sweep](../2026-11-02-iso-functional-baseline/README.md).

Final command: `bash scripts/check_contracts.sh`, exit **0**, 2026-11-03.
Full [gate log](check-contracts.log) and [domain outcomes](domain-outcomes.json).

- Pinned tools, registered-test presence and architecture checks passed.
- Workspace Nextest: **467 passed, 0 skipped** (baseline 460 plus seven added tests).
- Cargo doctests/compile-fail examples: **9 passed** (baseline seven plus two
  application boundary examples).
- **All 45 handwritten behavioral mutations detected**, after successful baseline
  builds/runs and with restored-source checks. No faults were allowlisted.
- Domain-only sweep: **202 mutants tested, 117 caught, 85 compiler-unviable,
  0 survivors, 0 outer mutation timeouts**. Two caught outcomes were per-test
  nontermination failures; compiler-unviable mutations are not test detections.
  The existing two behavior-equivalent exclusions are unchanged.
- `cargo fmt --all -- --check` and `git diff --check` passed.

The added valid-digit slug assertion was also exercised by the final domain-only
baseline/sweep. Test moves preserve original assertions with the representation
changes explicitly listed above. These results establish the covered behavior and
library integration, not universal equivalence or new live backend certification.
