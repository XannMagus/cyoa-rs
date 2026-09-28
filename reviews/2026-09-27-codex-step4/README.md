# Codex adapter step 4 — reconcile protocol and transport

Baseline: `6b6962b`. This implements adapter-plan step 4 in one atomic commit:
actual `Backend::generate`, failure evidence, subscription-auth construction,
real-child fixture tests, and a bounded real-adapter outline smoke. Full composed
story acceptance, new codec mutations and the complete live gate remain steps 5–7.

## Ownership and acceptance

`CodexCliBackend::connect` runs bounded `login status` with the configured
executable and the same explicit HOME/PATH/CODEX_HOME policy used for generation.
Only the observed ChatGPT status is accepted (plus the observed PATH-alias warning).
Missing, ambiguous, unknown, API-key and nonzero-exit status cannot authorize
construction. No login mode is changed. This is a point-in-time preflight, not a
guarantee against later external account changes; account/tool isolation remains
unverified. Config permits finite transport bounds but no arbitrary flags or env.

Each `generate` prepares the fixed request and invokes the existing supervisor
once. Its record consumer drives the private codec; a rejection triggers the
supervisor's cleanup. The adapter reconciles both outcomes after the supervisor
returns. Candidate/terminal output cannot override nonzero exit, incomplete input,
I/O, caps or cleanup failure. Cancellation is checked before preparation, at
acceptance, immediately before complete emission and after the callback. Exactly
one complete JSON payload is emitted after process/workspace cleanup; no fake
streaming or automatic retry. Protocol syntax checks and GenerationResponse
construction precede emission; domain validation remains GenerationEngine's job.

Cancelled/Timeout now carry `Option<String>` candidate evidence. `None` preserves
the old streamed-fragment fallback; `Some`, including an empty string, wins over
that fallback. Shared engine changes only map this vendor-neutral evidence.
`BackendError::Transport` carries a boxed generic error source, exact candidate
and byte diagnostics. The actual source retains SupervisorError's nested cleanup
and initiating causes, input counts and prefix metadata. The user-facing message
summarizes those causes without formatting diagnostic buffers. Application errors
retain the message and lossless diagnostics; structured low-level causes remain
available at the Backend boundary. No application import of process/vendor types.

Transport/protocol/preparation errors remain distinguishable. Nonzero generation
exit with no stdout maps to Unavailable; one with stdout maps to Generation. Auth
nonzero exit is always Unavailable because its stdout is status, not story output.
Cleanup failure stays visible even when its initiating cause was cancellation.
Success preserves normalized usage; configured model is never reported as observed
model and no monetary estimate is invented.

## Fixture and test evidence

The existing subprocess fixture gained a vendor mode reading scenario data from
its fake HOME. Production argv remains the exact Codex invocation; scenario JSON
is never passed as a production flag. It records argv, stdin, prepared schema,
PID/cwd, environment names and a launch log. The nominal test observes one auth
child and exactly one generation child; emission itself asserts PID disappearance
and workspace removal. Existing scenario-argv supervisor tests are retained.

Twelve actual-adapter integration tests cover late nonzero exit, protocol rejection,
missing/truncated terminal, timeout after an unemitted candidate, idle cancellation
with a child-start handshake, pre-cancellation, caps/prefix evidence, invalid UTF-8,
missing executable/preparation error, exact success request/response, callback
cancellation, and auth failures. A composed outline timeout reaches StoryUseCases
with exact candidate/diagnostics. This is not step 5's full story scenario.

Three private reconciliation tests cover the deterministic acceptance cancellation
race, every remaining transport category and nested cleanup failures. These inject
outcomes, not syscalls; real supervisor fault tests remain the separate evidence
for generating those outcomes. The candidate-timeout test asserts observed retained
bytes and actual PID cleanup; its 500 ms deadline is not used as proof of startup.

Observed development evidence:

- [Payload red](evidence/payload-red.log) reproduced missing complete-only candidate
  text at the application boundary; [green](evidence/payload-green.log) followed
  explicit candidate mapping. The empty case passed first and is expanded coverage.
- [Adapter red](evidence/adapter-red.log) records runtime failures against a compiling
  adapter skeleton: lost late-exit status/candidate, missing protocol evidence and
  unchecked auth. [Green](evidence/adapter-green.log) follows real wiring. Later
  cases inside loops are not claimed as individually observed red assertions.
- [Edge first run](evidence/edges-first.log), [reconciliation](evidence/reconciliation.log)
  and [focused](evidence/focused.log) were green-first expanded verification.
- Review found auth stdout/nonzero exit was classified as a generation failure;
  [auth red](evidence/auth-exit-red.log) reproduces it and
  [auth green](evidence/auth-exit-green.log) records the corrected auth-specific mapping.
- Clippy requested a smaller error enum; boxing the Transport variant's diagnostic
  container resolved it without allowing the lint. The smoke example also displays
  errors via Display rather than main's default Debug buffer dump.

## Live evidence and reproduction

`codex-cli 0.157.1`; current [version](evidence/version.txt) and
[exec help](evidence/help.txt) were checked. `codex login status` reported ChatGPT;
more importantly the adapter independently checked it with its own selected
home/environment before generation. No credential file or environment value was
copied into these artifacts. No explicit model override was selected.

Run explicitly (never part of the offline gate):

```sh
cargo run -p cyoa-infrastructure --example codex_adapter_smoke --locked --offline -- NEW_OUTPUT_DIRECTORY
```

The harness refuses to overwrite an existing directory, uses bundled templates and
a synthetic harbour brief, and calls the actual adapter. Fixed default bounds are
180 seconds, 1 MiB stdout and 256 KiB stderr. `--offline` applies to Cargo dependency
resolution; the example intentionally makes an authenticated model request.

- [Sandboxed attempt](live-sandbox/result.json): auth passed, generation failed
  before producing model events because Codex's app-server runtime initialization
  needed filesystem access outside the outer sandbox. Exact stderr is retained.
- [Explicitly approved rerun](live-approved/result.json): accepted in 27.161 s,
  one complete emission equal to the final payload, domain-valid WorldOutline,
  input 14419 / cached 0 / output 778. Model identity and cost were absent.
  Raw stdout, stderr, payload, instructions, prompt and original schema are retained.
  The adapted schema for this outline is identical to the original; fixed argv and
  framing are established by the actual-adapter fixture test. The rerun retained
  the adapter's read-only sandbox flags and explicit environment.

This was a single real outline, not a cast/turn story, live cancellation or a proof
of full account/tool isolation. Success passed supervisor cleanup checks; explicit
PID/workspace liveness assertions are fixture evidence. Subsequent refinements
boxed error diagnostics, corrected nonzero-auth classification, added the final
pre-emission cancellation check and improved harness error display; no successful
outline payload/usage/invocation rule changed. Full live gate remains step 7.

## Contracts and next slice

Affected contracts: **TEXT-001, STREAM-001, BACKENDS-001**. BACKENDS-001 is now
partial for the concrete Codex adapter, not enforced for both vendors. Shared
schema/wire/templates and Claude's compatibility adapter are unchanged (ARCH-003).
All existing regressions and eleven persistent mutations are retained. New codec
mutations belong to step 6; no synthetic test is labelled live evidence.

Next is step 5: composed outline/edit/cast/selection/opening/continuation/chapter
acceptance through the actual adapter, including active limits, identity behavior,
error state isolation and explicit retry. Claude remains co-equal and independently
pending; this commit adds no headless UI, persistence or arbitrary prompt overrides.

## Final verification

Completed across the 2026-09-27/28 session boundary. Formatting, warnings-denied
workspace Clippy, `git diff --check`, and `bash scripts/check_contracts.sh` passed:
**233 Rust tests including doctests**, architecture/registry/coverage checks, and
**all eleven retained behavioral mutations**. Full output is retained in
[contracts.log](evidence/contracts.log), [clippy.log](evidence/clippy.log) and
[fmt.log](evidence/fmt.log). Compiler failures never counted as mutation detection.
