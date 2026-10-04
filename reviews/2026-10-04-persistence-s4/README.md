# Phase 2 S4: supervised storage execution

S3 baseline `0f7ff1f`. Implements S4, not S5–S7.
Affected contracts: PRODUCT-001, STREAM-001, ARCH-001 and TEXT-001;
existing domain, backend and mutation contracts remain intact.

| Requirement | Actual implementation / evidence |
|---|---|
| Internal helper before public commands/auth | `helper::run_internal` and `main::execute`; shipped binary CRUD runs with empty child environment. No CLI/backend flags, credentials or network. |
| Bounded off-loop serialization and disk effects | `SupervisedRepository` and `StorageRunner`; capped 160 MiB protocol, 64 MiB saves, strict unique-key/depth/type/version checks, one shared operation deadline (10 s default), explicit capture limits. Parent JSON runs on worker, disk in child. Finite CPU work is input-bounded; no guarantee against kernel uninterruptible I/O. |
| Prepared identity before possible write | `PreparedWriteEvidence` publishes ID/previous/intended stamps and exact bytes before helper dispatch. Missing-helper/expired-preparation/entropy/clock failures stay Unchanged; launched-helper failures are conservatively Unknown. Actual helper errors retain structured visibility and cleanup causes. |
| Checked successful CRUD and retry | Four shipped-binary tests: create/read/list/replace/exact backup/conflict/reconcile, actual runner save/load, no global cwd change, missing/pre-cancelled helpers, real silent helper close/drop. Receipts must match prepared stamp/revision/time and await child exit/cleanup. |
| Timeout/cancel/reply loss | Four infrastructure transport tests use real controlled children and handshakes. Cancellation/deadline, invalid/multiple/capped/mismatched replies never authorize durability; no automatic write retry. Kernel PID checks establish reaping; recorded cwd is absent; child environment is empty. Reconciliation yields one slot with no repeated backup rotation. |
| Process kill at crash windows | Private FileOps barriers in a real test child before temp sync, after backup rename, before primary rename, after primary rename and after final sync. Parent verifies Busy lock, kills/reaps child, checks complete visible primary/backup bytes and released lock, and reconciles twice without changing revision/slot. Both first-create and replacement exercised. Process kill is not physical power loss. Orphan temp files are ignored, not deleted indiscriminately. |
| Worker lifecycle | Three port-only presentation tests plus private spawn-failure test: finite poll, Prepared before Finished (including completion race), key preservation, busy rejection, before/after-preparation panic, cancellation/close/drop and verified join. Faulted runner requires fresh factory; storage and generation remain independent. |
| Boundary/path edges | Private protocol and preparation-failure tests; extra real-file title/retitle and subprocess-isolated XDG resolution regressions. No test changes ambient env or writes real user data. |

## Scheduling refinement

S2 froze `SaveTarget` with expected stamp but no disk revision. To prepare bytes
once on the worker **before** a mutating helper starts, replacement first needs
one supervised read child to obtain the validated revision. It then launches one
mutating child with expected stamp; the latter rechecks under lock. Both share one
original deadline, and conflict never triggers a write retry. Create/reconcile/
load/list each use one child (create entropy collisions allow the specified eight
ID attempts). This preserves the inward port and publication order rather than
moving disk reads onto the presentation loop or encoding an uninformed revision.
The transport regression asserts Read→Apply for replace, Read only on conflict.
The plan records this implementation refinement explicitly.

## Development and verification

The initial API-red run is `/tmp/phase2-s4-red.log`. Focused failure/edge/nominal
runs passed through actual helper children and the port-only runner. The first
shipped CRUD run revealed an incorrect harness cwd expectation (Nextest runs at
the package directory); the test now captures its real initial cwd and verifies
that it stays unchanged. This was a harness failure, not a behavioral detection.

No shared vendor DTO/template/schema or peer-backend adapter changes, no model
calls and no Python tooling. The existing 37 handwritten mutation targets,
profiles and core-only automated mutation scope remain unchanged. Autosave,
canonical coordination, public commands and full shutdown/signal matrices wait
for S5–S7; existing headless behavior/tests remain in force.

Completion verification: `cargo fmt --all -- --check`, explicit rustfmt check of
the included private filesystem test file, workspace/all-target Clippy with
warnings denied and `git diff --check` passed. The full gate
`/tmp/phase2-s4-gate.log` passed **396 runtime tests, none skipped**, seven
compile-fail doctests, all 37 handwritten behavioral mutations and 179 domain
mutants: 100 caught, 79 unviable. Two caught cases were per-test nontermination,
reported distinctly; compiler-invalid mutants are not behavioral detections.
All registered S3/S4 requirements have executable evidence above. Public autosave
and save/load commands remain subsequent work, not completion evidence for S4.
