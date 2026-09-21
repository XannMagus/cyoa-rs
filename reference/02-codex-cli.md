# `codex exec` as a second inference backend — status: flags confirmed, streaming NOT verified

Companion to `01-claude-cli.md`. That file's rigor was "checked against the real
installed binary, not docs." This one is **half that**: the CLI and its flags were
checked against the real installed binary (`npx @openai/codex@latest`, version
0.155.1 at time of writing), but a real authenticated generation was **not**
observed — `codex exec` needs an interactive `codex login` (ChatGPT OAuth) that
couldn't be done non-interactively in the session that wrote this file. Everything
under "Confirmed" was seen directly. Everything under "Open questions" needs a live
run before `CodexCliBackend` gets implemented.

## How to reproduce / finish this verification

```bash
npx @openai/codex@latest login          # interactive, OAuth — do this in a real terminal
codex login status                       # confirm subscription (not API-key) auth
echo 'Write a two-sentence tavern scene.' | codex exec \
  --json --ephemeral --sandbox read-only --skip-git-repo-check \
  --output-schema /tmp/schema.json
```
with `/tmp/schema.json` holding e.g.
`{"type":"object","properties":{"narrative":{"type":"string"},"mood":{"type":"string"}},"required":["narrative","mood"],"additionalProperties":false}`.
Then read the JSONL stream the same way `01-claude-cli.md`'s test #2 did, and update
this file's "Confirmed" section with what actually comes back.

## Confirmed: the command exists and takes these flags

```
codex exec [OPTIONS] [PROMPT]
```

Prompt comes from the positional arg, or from stdin when no arg is given (if both are
given, stdin is appended as a `<stdin>` block) — same shape as `claude -p`.

Relevant flags from `codex exec --help` (0.155.1):

| Flag | What `--help` says |
|---|---|
| `--json` | Print events to stdout as JSONL |
| `--output-schema <FILE>` | Path to a JSON Schema file describing the model's final response shape — **a file path, not inline JSON** like `claude`'s `--json-schema`, so the Rust backend must write a temp schema file per call (or once, if the schema is static per call site) |
| `--ephemeral` | Run without persisting session files to disk |
| `-s, --sandbox <read-only\|workspace-write\|danger-full-access>` | Sandbox policy for model-issued shell commands |
| `--ignore-user-config` | Do not load `$CODEX_HOME/config.toml`; auth still uses `CODEX_HOME` |
| `--ignore-rules` | Do not load user or project execpolicy `.rules` files |
| `--skip-git-repo-check` | Allow running Codex outside a Git repository |
| `-o, --output-last-message <FILE>` | Write the final agent message to a file |
| `-m, --model <MODEL>` | Model to use (e.g. `gpt-5.2-codex`) |
| `--color <auto\|always\|never>` | Analogous to nothing in the Claude backend; keep `never` for clean piping |

No flag equivalent to `claude`'s `--tools ""` (full tool disablement) was found —
`--sandbox read-only` constrains what a shell tool call can *do*, it doesn't prevent
the model from attempting one. Unverified how that surfaces in the JSON event stream.

## Confirmed: auth mirrors the `--bare` trap exactly

```
codex login                              # ChatGPT/subscription OAuth — default, what we want
codex login --with-api-key               # reads OPENAI_API_KEY from stdin — metered billing
codex login --with-access-token          # reads a raw access token from stdin
codex login status                       # check which mode is active
```

`codex login`'s own `--help` confirms `--with-api-key` is the opt-in metered path;
default interactive login is the ChatGPT plan. This is the same shape as Claude's
`--safe-mode` (subscription-preserving) vs. `--bare` (forces `ANTHROPIC_API_KEY`) —
just inverted, in that Codex's trap is a login *mode* rather than an *exec* flag, so
there's nothing on the `codex exec` invocation itself to audit — `cyoa doctor` needs
to shell out to `codex login status` and parse it.

## Confirmed: unauthenticated failure shape

Running `codex exec --json ...` with no valid login produces this JSONL shape
(observed directly, request failed at the transport layer before reaching a model):

```json
{"type":"thread.started","thread_id":"..."}
{"type":"turn.started"}
{"type":"error","message":"Reconnecting... 1/5 (unexpected status 401 Unauthorized: ...)"}
...
{"type":"item.completed","item":{"id":"item_0","type":"error","message":"Falling back from WebSockets to HTTPS transport. ..."}}
{"type":"error","message":"..."}
{"type":"turn.failed","error":{"message":"unexpected status 401 Unauthorized: ..."}}
```

This at least confirms the event *envelope* (`thread.started` / `turn.started` /
`item.completed` / `turn.failed`) is real JSONL with a `type` discriminant, matching
the general shape `01-claude-cli.md` describes for Claude's stream — but says nothing
about what a successful `item.completed` for an agent message or a structured
response looks like.

## Open questions — resolve before writing `CodexCliBackend`

1. **Does `--output-schema` stream the structured fields incrementally?** Claude's
   backend gets this via a forced `StructuredOutput` tool call whose `input_json_delta`
   fragments are exactly the raw JSON text `StreamingStringField` wants. It is
   *unknown* whether Codex's `--output-schema` (a) streams the final message's raw
   JSON text token-by-token as ordinary `item` deltas (which would still work with the
   same scanner, just fed from a different event field), or (b) only validates/attaches
   the parsed object once the turn completes, with no partial text available at all. If
   (b), `narrative`'s incremental reveal doesn't work for this backend in v1 — either
   accept spinner-then-reveal for Codex, or find another signal to stream from.
2. **Exact `item.*` shapes for a successful turn** — event `type` values, whether
   there's a `structured_output`-equivalent field on `turn.completed` /
   `thread.completed`, whether the parsed object is handed back pre-parsed the way
   Claude's `result.structured_output` is.
3. **Does `--ignore-user-config` + `--ignore-rules` add up to full isolation**, or can
   configured MCP servers / plugins still activate during `codex exec`? Claude's
   `--safe-mode` explicitly disables MCP/plugins/hooks/skills in one flag; Codex has no
   single documented equivalent in `--help`.
4. **Exit codes and error conventions** on failure modes that matter for this project:
   rate limit, sandbox-denied action, malformed/unsatisfiable schema. Needed to
   reproduce Claude backend's "never auto-retry, surface as `Result::Err`" posture.
5. **Cost/token reporting**, if any, analogous to `total_cost_usd`/`usage` in Claude's
   result envelope — for the "estimate, not billed" display, or omit it entirely for
   this backend if nothing exposes it.
6. Whether `--model gpt-5.2-codex`-style model names need pinning the way `--model
   sonnet` does for Claude, or whether a config default is sufficient.

Update this file's "Confirmed" section with real output before implementation, the
same way `01-claude-cli.md` was written from an actual transcript rather than
`--help` text alone.
