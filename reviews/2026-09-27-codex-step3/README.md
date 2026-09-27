# Codex adapter step 3 — private protocol decoder

Baseline: `d25d004`, clean working tree. The next unfinished dependency was the
[step-3 handoff](../../docs/plans/phase1-codex-step3-handoff.md), after frozen
0.157.1 discovery and offline request preparation. This implements that bounded
slice in one atomic commit with its tests, registry and evidence. The user's
request explicitly authorized implementation and atomic commits.

## Implementation and boundaries

`backends::codex_cli::protocol` is private infrastructure. Its state enum moves
from awaiting a thread, turn and candidate to protocol completion or a latched
failure. Required fields and identities are checked. Multiple candidates (even
identical or reused IDs), unsupported events/items, malformed UTF-8/JSON, wrong
order and duplicate/conflicting terminals are rejected. A top-level error notice
is distinguished from a vendor terminal failure; reconnect notices are not
misrepresented as universally terminal vendor errors.

Decoded payload text is retained byte-for-byte, including invalid or empty text.
Payload JSON syntax is checked at end-of-stream, after message ambiguity has been
ruled out. No trimming, reserialization, concatenation, code-fence repair or
first/last-parseable heuristic is used. Schema/domain validation remains downstream.
Any later failure retains the first candidate with the original located error;
subsequent records cannot turn a failed decoder into a successful one.

Usage counts are optional and independently normalized. Missing or malformed
counts are unknown, zero is known, and invalid cached counts use the existing
normalizer. Reasoning/cache-write counts are not added again. Completion carries
no invented model or monetary provenance.

The codec has no process runner, callback, public GenerationResponse constructor,
auth access or game mutation. Its private module temporarily allows dead code
because step 4 has not wired the parent adapter. Its completion is only protocol
evidence: the future adapter must reconcile exit status, stdin delivery,
cancellation, bounds, I/O, reaping and workspace cleanup. Existing supervisor,
shared templates/schema/wire code and Claude compatibility remain unchanged.

## Tests and observed development evidence

Nine unit tests exercise the codec itself against frozen fixtures and additional
hand-authored edges. Seven live discovery transcripts match their pre-existing
exact payload files and independently transcribed token counts. The tool canary
is rejected despite recorded CLI exit 0. All 20 distinct synthetic transcript
files are covered; the manifest's 21st case repeats success with process exit 1,
which intentionally belongs to step 4 rather than this process-free decoder.
CRLF and final records without LF are replayed under the supervisor's existing
record contract; these tests do not claim to re-verify byte framing or cleanup.

- [Test setup error](evidence/test-setup-error.log): an unavailable byte-slice
  helper was corrected before runtime testing. This is not red evidence.
- [Errors red](evidence/errors-red.log): the initial skeleton failed three test
  functions at runtime (missing record location, malformed fields accepted, and
  missing candidate evidence). [Errors green](evidence/errors-green.log) follows
  the state machine implementation. Later cases within each loop are expanded
  coverage, not separately observed red cases.
- [Edges red](evidence/edges-red.log): empty payload incorrectly completed and
  known token counts were lost. [Edges green](evidence/edges-green.log) follows
  payload syntax validation and telemetry normalization. Exact-byte and sticky
  failure tests passed on their first run and are labelled expanded coverage.
- [Nominal replay](evidence/nominal.log): all nine tests pass; the live transcript
  tests were added green-first against the already implemented frozen profile.

No model was invoked this session. These are offline replays of the separate
2026-09-26 live discovery evidence, not verification of an executable Backend.
No open live question is relabelled confirmed. Full isolation, new versions,
model selection, cancellation and composed live story acceptance stay pending.

## Contracts and next step

Affected IDs: **ARCH-003, TEXT-001, STREAM-001**. Registry entries enforce actual
codec behavior without promoting BACKENDS-001. Existing story contracts and all
eleven mutations remain unchanged. Step 6 owns new persistent codec mutations.

Next: adapter-plan step 4, implementing `Backend::generate` using this codec and
the existing supervisor. Audit candidate evidence for cancellation/timeout before
wiring success; exercise the actual adapter with fixture children. Claude's
implementation and live acceptance remain independent, co-equal obligations.

## Final verification

Passed `cargo fmt --all --check`, warnings-denied offline workspace Clippy, and
`bash scripts/check_contracts.sh`: **217 Rust tests including doctests**, registry
and architecture checks, and **all eleven retained behavioral mutations**.
[Full gate log](evidence/contracts.log), [Clippy](evidence/clippy.log) and
[format check](evidence/fmt.log) are retained. `git diff --check` also passed.
