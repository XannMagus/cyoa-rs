# Phase 2 step 2: persistence vocabulary and save codec

Date: 2026-10-04. Step 1 baseline: `dfb1b61` on `phase-2/persistence`.
This slice implements S2 in `docs/plans/phase2-persistence.md`. S3–S7 remain
pending: there is no concrete filesystem repository, storage helper/runner,
autosave coordinator or headless save/load command in this slice.

## Scope and requirement evidence

| S2 obligation | Actual implementation / executable evidence |
|---|---|
| Inward application vocabulary, port, CQRS use cases | `application/src/persistence.rs`, fake repository `application/tests/persistence.rs`: checked IDs/counters/time/pages/prepared bytes, create/replace/reconcile/load/list, explicit restore and read-only inspect. Application has no serde/JSON/path/clock/filesystem dependency. |
| Errors and cancellation without implicit retry or writes | Fake repository failures preserve stage/kind/visibility/pending/cleanup evidence; each operation calls once; pre-cancel calls zero; late read cancel rejects result; a durable receipt survives late cancellation. Restore queries never call write methods. No generation dependency exists in persistence use cases. |
| Version-one schema and strict reconstruction (A2) | `persistence/{dto,codec,migrations}.rs`; `save_validation.rs` rejects known shape/requiredness/selection/metadata/identity/limits/cost violations, lossy events/actions/business text, invalid UTF-8/JSON, duplicate keys including ignored nested extensions, size/depth/overflow. Established casts use S1 `restore`, not generation filtering. |
| Compatibility scaffold (A4) | Frozen minimal/full/additive/invalid fixtures; `save_compatibility.rs` covers omitted defaults, required versus nullable, Keep versus Replace(empty), unknown action kinds, unknown-style fallback, extensions and warnings (including both source variants), future/missing/zero versions, unchanged input bytes. Unit-only synthetic chains verify missing/wrong-step failures and ordered dispatch; production supports v1 only. |
| Complete and zero-turn round trips (A1) | `persistence_roundtrip.rs` composes actual story use cases/templates/generation decoding/commits with an independent frozen game. It checks error/cancel isolation, exact empty/whitespace/CRLF/quoted/Unicode raw bytes, historical prompt traces and observed provenance, zero versus absent cost, role/order/namesakes/IDs, second-playable selection, all flat styles, marker-derived chapters, rewind and the exact captured next request/count. Zero-turn reconstruction issues one opening request. SHA-256 uses the independent standard abc vector. |
| Historical memory authority | Compatibility regression accepts legitimate snapshots differing from delta replay and repeated proposals, preserving snapshots and audit rather than recommitting. Roundtrip fixture audit enrichment uses `TurnRecord::new` for historical trace/provenance values that default generation deliberately does not collect. |
| Repeated explicit limit restoration (A3, codec portion) | `persistence_limits.rs`: saved cap 4 → current 1/bridge 0/NPC 0/minimum 9 → encode/decode → original 4 → encode/decode → rewind/continue. Every snapshot's newest events and cap, creation metadata, established cast, prompt/schema/bridge and zero-turn opening are checked. Previously discarded events never resurrect. Disk cycles remain S3+ acceptance. |
| Bounded output and timestamp metadata | Codec unit tests check exact encoded bound including newline, failure below bound, UTC years 0001/9999 and canonical milliseconds. `SavedAt` rounds finer clock precision down once, preserving the checked millisecond instant thereafter. Timestamps do not order writes. |
| Actual registrations and truthful partial coverage | ARCH-001/002, TEXT-001/002, STATE-001, LIMITS-001, IDENTITY-001/002, CHAPTER-001, PRODUCT-001; no placeholder tests or pending disk claims. |

## Observed red, edge and nominal runs

The initial malformed-save tests failed to compile because the new APIs did not
exist (`/tmp/phase2-s2-red.log`); this was an API-development red, not a behavioral
mutation detection. Implementing DTO mapping exposed generic constructor lifetime
errors; explicit borrowed-input lifetimes corrected them. Focused validation then
passed five error groups (`/tmp/phase2-s2-validation.log`). Compatibility/default/
null/extension edges followed, then full story/zero-turn and repeated-limit nominal
acceptance. All fixtures specify expected views independently of the encoder;
`tests/fixtures/saves/README.md` records their provenance and exact expectations.

A further real assertion failure exposed an internally tagged Serde-enum warning
gap: `additive_source_fields_are_reported_for_both_source_variants` failed with
`source additions must warn: []` (`/tmp/phase2-s2-source-warning.log`). Serde buffers
internally tagged variants before `serde_ignored` observes them. The source DTO now
explicitly reports its unknown extensions while the normal visitor reports the
rest of the document. The test passes for live and demo. This is a save-local
mapping repair; generation DTOs/vendor adapters are unchanged.

## Completion verification

Commands: `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --locked --offline -- -D warnings`,
and `bash scripts/check_contracts.sh`. The gate retains pinned workspace Nextest,
Cargo doctests, all 37
handwritten behavioral mutations and the core-only cargo-mutants sweep; no
mutation manifest/exclusion changes were required. No credential/model/network
calls or Python tooling were used; all new locked dependencies were cached.

The final tree passed formatting, workspace/all-target Clippy with warnings denied,
`git diff --check` and the full gate (`/tmp/phase2-s2-final-gate.log`): **369 runtime
tests passed, none skipped; seven compile-fail doctests passed; all 37 handwritten
mutations detected; 179 domain mutants tested, 100 caught and 79 unviable**.
Two caught domain mutants were per-test nontermination failures, separately
reported; unviable/compiler-invalid mutants are not behavioral detections.
All existing mutation manifests, patches, profiles and exclusions remain unchanged.
The first full run passed 366 runtime tests; the gate was rerun after adding the
final Unicode/source/audit edge tests and their registrations. No broad completion
claim rests on that earlier narrower run.

The S1/S2 requirement audit above is satisfied by the actual source, independent
fixtures, application fake-repository observations and these final gate results.
S1 remains in its separate `dfb1b61` commit; S2 is one passing thematic commit.
There was no push, merge, vendor adapter/shared-generation edit or live backend
claim. Next implementation is S3 local atomic storage, not already shipped by
the existence of its inward port.
