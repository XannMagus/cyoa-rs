# Review: generation boundaries, prompts, and consuming transforms

Reviewed on 2026-09-24 by Codex, from baseline
`221e7ed078bb33d7d522151b88f130436a50f655` through
`a163990baeee161ed70f7b9e96c6365a47ae577e`. The working tree was clean.
The three commits carry a Claude Sonnet 5 co-author trailer; this review evaluates
the code and observed behavior, not the author's intentions or model capabilities.

## Scope and verdict

| Commit | Change | Assessment |
|---|---|---|
| `ef1985fa38208d6e69414a700b6a9f7a027aff12` | Prompt rendering, schema generation, wire DTOs and mappings | Useful boundary separation, but R1–R5 need attention before application integration. |
| `19cc56ea8cb900e6fb615b00de633c59dc40a22c` | Live Claude schema evidence and ARCH-003 backend compatibility isolation | Appropriate separation and narrowly checked root-key removal. Claude evidence was inspected, not independently rerun in this session. |
| `a163990baeee161ed70f7b9e96c6365a47ae577e` | Consuming transforms and corresponding guidance | No behavioral regression found in the changed domain paths. Ownership changes preserve existing results. |

Existing namesake preservation, suffix allocation, verbatim text, owned selection,
and restore-limit contracts remain intact. However, passing their domain tests does
not establish that the newly added model-facing layer asks for compatible behavior.
No production repairs are included in this review. Recommended fixes below are
review proposals, not newly adopted business decisions.

## Findings

### R1 — P1: cast generation requires the cast it is supposed to generate

Introduced by `ef1985f`. Location:
[`prompts.rs:315`](../../cyoa-infrastructure/src/generation/prompts.rs#L315).
`cast_generation_prompt` accepts `&World`, but a valid `World` already contains a
validated `WorldCast`. The lifecycle reaches this call with a `WorldOutline`, before
the cast exists. The function only reads outline information. Tests hide the
problem by creating a complete world first.

This blocks composing the intended outline → cast generation → world construction
flow without inventing placeholder characters or weakening the domain invariant.
Python's earlier permissive world representation is not a reason to weaken Rust's
aggregate (ARCH-002).

**Recommended fix:** accept `&WorldOutline`. Add a lifecycle integration test that
maps an outline response, renders the cast request, maps a cast response, constructs
the world, and selects a character without any fabricated intermediate cast.

### R2 — P2: valid configured limits produce contradictory generation requests

Introduced by `ef1985f`. Locations:
[`prompts.toml:39`](../../cyoa-infrastructure/src/generation/defaults/prompts.toml#L39),
[`schema_docs.toml:22`](../../cyoa-infrastructure/src/generation/defaults/schema_docs.toml#L22),
and [`prompts.rs:315`](../../cyoa-infrastructure/src/generation/prompts.rs#L315).

The prompt and schema descriptions demand three to five playable characters,
regardless of `min_playable_characters`. They demand between three and the configured
NPC maximum even when that maximum is zero, one, or two. All these settings are
legal domain values. The probes reproduce all three inverted NPC ranges and a
playable minimum of six with instructions still asking for three to five.

Following the instructions can therefore fail world construction or waste a
generation on NPCs that must be discarded. LIMITS-001 explicitly permits zero NPCs;
fixing this by banning that setting would violate the decision.

**Recommended fix:** derive consistent generation ranges from active limits, using
the same policy for prompt and schema text. Express zero as no NPCs. Test zero,
small caps, and a playable minimum above five, as well as restored active limits
when that integration exists.

### R3 — P2: the startup self-check accepts templates that panic at render time

Introduced by `ef1985f`. Locations:
[`prompts.rs:110–174`](../../cyoa-infrastructure/src/generation/prompts.rs#L110) and
[`prompts.rs:178`](../../cyoa-infrastructure/src/generation/prompts.rs#L178).

The self-check supplies one superset of every context variable to every template
and silently skips non-string values. Actual renderers have smaller contexts and
use panicking string lookup/render helpers. Two reproduced cases:

- Replace `prose_contract.dialogue_and_sensory` with integer `42`: the startup check
  succeeds; `prose_contract` panics because the expected string is absent.
- Replace `fragments.markdown` with `{{ brief }}`: the startup check succeeds because
  its context includes `brief`; rendering the fragment panics because its actual
  context does not.

XDG loading is still future work, so this is a defect in the exposed validation
and rendering building blocks, not a claim that today's executable loads overrides.
PROMPTS-002 currently describes stronger typo protection than these checks provide.

**Recommended fix:** validate required keys and value types into a prompt-set type;
exercise templates with their actual per-call context shapes; return boundary
errors rather than panic. All render entry points should use that validated set
once configuration loading is connected.

### R4 — P2: schema instructions suppress the deliberate chapter-retitling feature

Introduced by `ef1985f`. Location:
[`schema_docs.toml:98`](../../cyoa-infrastructure/src/generation/defaults/schema_docs.toml#L98).

The loaded description requires `chapter_title` to be null unless
`starts_new_chapter` is true. CHAPTER-001 deliberately allows a later continuing
turn to supply a new title. The domain and wire mapping still support it, but the
new model-facing instruction asks the generator never to exercise that path.
The probe confirms the restriction is present in the generated schema.

This is an example of source parity conflicting with a documented extension:
copying the source description verbatim is insufficient even while domain tests pass.

**Recommended fix:** make the loaded description and relevant prompt instruction
allow retitling an existing chapter. Preserve the copied reference as historical
source. Add a regression covering the rendered instruction and a continuing turn's
title through commit and rewind.

### R5 — P2: two required response fields silently become optional

Introduced by `ef1985f`. Locations:
[`wire.rs:74–75`](../../cyoa-infrastructure/src/generation/wire.rs#L74) and
[`wire.rs:275–276`](../../cyoa-infrastructure/src/generation/wire.rs#L275).

`#[serde(default)]` on NPC `relationships` and turn `scene_description` both accepts
omitted fields as empty strings and removes them from the generated required lists.
The corresponding Calibre NamedTuple fields have no defaults
([NPC](../../reference/calibre/cyoa.py#L89),
[turn](../../reference/calibre/cyoa.py#L320)); its
[structured-output instantiator](../../reference/calibre/structured.py#L304)
rejects missing non-nullable fields without defaults. Explicit empty
strings and missing fields are different boundary states. No deliberate deviation
for these omissions is recorded.

The probes deserialize both omissions successfully. A response can consequently
lose requested relationship or scene information without reporting incomplete
output. This also contributes to the Codex schema incompatibility observed below.
Nullable `chapter_title` omission is not included in this finding: the source
instantiator permits nullable fields to become `None`.

**Recommended fix:** retain requiredness for these two wire fields while allowing
their explicit empty values where the source does. Test JSON deserialization and
schema required lists; manually constructing a DTO does not test missing fields.
If tolerant omission is desired, record that as a deliberate decision first.

## Gaps in the protection added around these changes

These are narrower claims than the defects above, but matter for future handoffs:

- `schema_docs_cover_every_field_and_no_others` checks fields of generated types,
  but does not reject an entirely unknown documentation table. In an isolated
  export of reviewed HEAD, appending `[ReviewOnlyOrphan]` with a `stale_field`
  string still passed that exact test. Compare the complete set of documented
  types as well, allowing explicitly intentional non-generated tables.
- PROMPTS-001 now says **Enforced** and asserts no extra paid call. The registered
  test checks substring presence on the first prompt and absence on a later one;
  it neither counts occurrences despite its name nor observes generator calls.
  Prompt insertion is implemented, but call-count enforcement must remain pending
  until a scripted generator can observe orchestration. The earlier acceptance
  requirement should not disappear when only rendering is implemented.
- Prompt tests mostly assert substrings rather than the planned complete rendered
  snapshots. They missed contradictory configured ranges and the chapter-title
  restriction. Add complete representative renders plus targeted contract tests.
- PROMPTS-002's remaining work should include connecting validated merged settings
  to render entry points, not just completing backend adapters. The final entry
  points currently use cached defaults; the merge/self-check helpers are separate.

The registry protects test presence and execution, not the strength of an assertion.
It cannot prevent a test and its declared coverage from being weakened together.

## Live Codex evidence and backend handoff

Following the repository's own-backend verification rule, ran Codex CLI **0.155.1**
with ChatGPT login confirmed. The shared generated cast schema was rejected with
HTTP 400 `invalid_json_schema`, exit code **1**: every object's `required` array
must include all properties; `relationships` was specifically reported missing.

On a temporary copy only, set each object's `required` to all of its property names.
The second call succeeded with exit code **0**, preserving the root `$schema` and
`$defs`/`$ref`. It emitted a complete JSON string in an `item.completed` agent
message, followed by `turn.completed` with token usage. No partial text events,
tool calls, or cost field appeared in that successful transcript. One cast call
does not establish general streaming, isolation, turn-schema, or failure behavior.

This is a discovered requirement for the future Codex adapter, **not** a regression
in a backend already claimed implemented. Per ARCH-003 it belongs in its own
compatibility file. Do not strip the generic schema or reuse Claude's root-key fix.
Handling optional fields generally needs an explicit nullable/required policy;
the temporary all-required transform is evidence, not a production implementation.

The successful result contained three playables and three NPCs, despite the probe
asking for two and one. This is consistent with the schema's three-character
minimum descriptions; it reinforces the need for coherent instructions but is not
a controlled proof of which instruction caused the counts.

Evidence: [original schema](evidence/cast.schema.json),
[rejected call](evidence/codex-cast.jsonl),
[adapted schema](evidence/cast.codex-test.schema.json), and
[successful call](evidence/codex-cast-adapted.jsonl).
The dated confirmed/open status is in
[`reference/02-codex-cli.md`](../../reference/02-codex-cli.md).

## Validation and reproduction

Against reviewed HEAD, all existing checks passed:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`
- `bash scripts/check_contracts.sh` — architecture/registry checks, **85 runtime
  tests and seven compile-fail doc tests**.

Dependencies first needed `cargo fetch --locked`; the checks then ran offline.
Additional [historical probes](probes.rs) reproduced R2–R5. Their assertions describe
defects at reviewed HEAD and must not become acceptance tests for those defects.
R1 follows directly from the public signature and validated aggregate constructors.

To run the probes, create a temporary Cargo package outside this workspace with
edition 2024, path dependencies on this checkout's `cyoa-core` and
`cyoa-infrastructure`, plus `serde_json = "1"` and `toml = "1.1.6"`. Copy
`probes.rs` to its `src/main.rs`, then run `cargo run --offline` there. Expected
output reports R2–R5; caught panic messages for R3 are intentional. It also writes
the generated cast and turn schemas in the temporary working directory.

The two live calls ran from that temporary directory, with `cast.schema.json`
and then `cast.codex-test.schema.json` as the schema argument:

```sh
timeout 50s codex exec --json --ephemeral --sandbox read-only \
  --ignore-user-config --ignore-rules --skip-git-repo-check --color never \
  --output-schema cast.schema.json \
  'Generate a tiny cast for a quiet harbour: two playable characters and one NPC, each with a very short description and backstory. Fill relationships. Do not use tools or read files. This is only a structured-output compatibility probe.'
```

The schema-only transformation used between calls was:

```sh
jq 'walk(if type == "object" and has("properties") then .required = (.properties | keys) else . end)' \
  cast.schema.json > cast.codex-test.schema.json
```

For the documentation-coverage probe, export reviewed HEAD to a temporary tree,
append the unknown TOML table described above to its schema documentation, and run
`cargo test -p cyoa-infrastructure schema_docs_cover_every_field_and_no_others`.
The reviewed implementation passes despite that stale table.

## Repair follow-up

R1 repaired: the cast prompt accepts `WorldOutline`; the registered
`generation_lifecycle` regression covers JSON outline → cast prompt → JSON cast →
world selection → opening prompt, including namesakes and repaired IDs. No
placeholder cast is needed. Application orchestration remains pending.

R2 and R4 repaired: cast prompt/schema requests share limit-derived ranges, and
zero NPCs is explicit. Loaded chapter instructions permit continuing-turn retitles.
Registered regressions exercise configured extremes and JSON retitles through
commit, null-title preservation, and rewind. Reference captures remain unchanged.

R5 repaired: removed the two unintended serde defaults. Registered JSON/schema
regressions require presence while accepting explicit empty strings, reject null,
and preserve the unrelated defaulted and nullable fields. R3 and the broader
configuration, orchestration, and test-harness gaps remain open.

R3 repaired in step 4 (2026-09-25): `GenerationTemplates` validates source shape,
style tables, per-template contexts and schema documentation, owns compiled
rendering, and returns errors instead of panicking. Arbitrary override loading
remains unavailable pending the newly required semantic invariant validator.
See [the TDD record](../2026-09-25-template-validation/README.md). No-extra-call
orchestration is correctly marked partial until step 5 can observe actual calls.
