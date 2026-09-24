# Remaining Phase 0 acceptance sequence

User-approved order: steps 1–3 repaired generation boundaries; step 4 introduces
validated configuration. Each subsequent step is its own commit with implementation,
regressions, registry and truthful status updates. Use error → edge → nominal TDD:
run new tests and inspect the intended failure before implementing, then refactor
under the green suite. Do not manufacture failures by corrupting an expectation.

## Step 5 — application ports and generation orchestration

Application owns typed world/cast/turn generation requests and results; JSON,
rendering, schema adaptation and wire decoding stay in infrastructure. Neither
application nor its tests may import concrete infrastructure (including through
dev-dependencies). Infrastructure integration tests compose the real adapter with
a scripted JSON transport. Test-only adapters around templates do not suffice.

Required failure cases: transport failure, invalid JSON, schema/wire omissions,
invalid outline, insufficient usable cast and invalid turn. Preserve raw diagnostics,
leave the caller's game exactly equal to its pre-call value, perform no retry, and
make cancellation an explicit outcome. Never commit a preview or partially mapped
response. Requests made while already cancelled must not call the transport.

Required integrated path: brief → outline response → accepted/edited outline →
cast request → namesake-preserving world → checked selection → opening turn →
continuation. No placeholder cast; captured cast request uses edited outline.
Exact duplicate records collapse; distinct namesakes reach the initial prompt with
independently usable repaired IDs. Capture every request. Assert exactly one call
per requested generation, exactly one opening identity instruction, none on later
turns, no extra deduplication call and no implicit retries. Count calls, not strings
alone. Repaired IDs must appear in the next captured request after an update.

A turn's prompt, schema descriptions and merge cap must derive from GameState's
active limits after either restore policy. Assert both current and original choices;
retained original metadata must not be silently replaced with current settings.

## Step 6 — streaming scanner and chunk replay

Keep partial JSON handling in infrastructure; application progress contains decoded
narrative only. Deterministic chunked transport exercises the same adapter as the
whole-response transport. Never parse/commit partial output as a finished turn.

Test escapes, quotes, backslashes, Unicode escapes, paired/lone surrogates, leading
fences, nested fields of the same name, narrative not first, absent narrative,
truncated streams, and every scalar boundary of small examples. Seeded/property
cases must compare arbitrary splits with whole-input extraction and an independent
expected narrative. The scanner accepts valid UTF-8 strings; byte framing remains
the subprocess adapter's responsibility. Streaming failure/cancellation preserves
state and diagnostics even after preview text was emitted. Complete-message-only
backends must work without duplicating preview text.

## Step 7 — complete no-I/O acceptance scenario

Compose application use cases, real generation infrastructure and scripted transport;
do not call domain merge directly as a substitute for exercising the adapter.
Generate world/cast and at least four successful turns. Include a chapter break,
continuing-turn retitle, same-name character updates by repaired ID, unknown action
kind recovery, malformed response, explicit retry, cancellation and rewind.

Check request order/counts, exact state equality on errors, diagnostic bytes, prior
chapter bridge contents, newest-event retention, null-versus-empty upcoming events,
chapter title restoration, and the subsequent request after rewind. Fixtures follow
project decisions independently of Python. Schema and prompt agreement alone does
not establish semantic model compliance; real-model fiction quality remains Phase 1.

## Step 8 — completion gate and mutation evidence

Register boundary and orchestration tests under the relevant decisions. Coverage
must distinguish domain enforcement, generated instructions, application behavior,
and live backend verification. An implemented renderer cannot mark a no-extra-call
contract enforced. Pending obligations need actual tests when implemented; do not
register nonexistent tests or use ignored placeholders to fabricate enforcement.

Run targeted mutations in an isolated checkout so the working tree is untouched:
name-only deduplication, duplicate opening request, active limits replaced by defaults,
and state commit before error handling. Each must fail its intended registered test;
compiler failures do not count as evidence that behavioral assertions caught a
mutation. Fail the mutation runner if a patch no longer applies, a test is absent,
or a mutation survives. No Python tooling, credentials or live model in this gate.
The shared CI/local command runs registry, architecture and tests. It cannot prevent
an actor changing both assertions and implementation; review remains required.

## Out of scope for Phase 0 completion

Real CLI subprocess adapters/headless play are Phase 1; disk persistence/migrations
are Phase 2. Arbitrary prompt overrides additionally require PROMPTS-003's semantic
validator. Never claim these complete based on scripted transports or structural
configuration checks. Live evidence belongs in each backend's own reference file.
