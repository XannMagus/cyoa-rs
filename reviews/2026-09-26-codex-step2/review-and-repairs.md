# Independent step-2 review and authorized repairs — 2026-09-26

Baseline: HEAD `70a669c7e5a67e9ce4f4dc507f925d37775567fa`, plus Luna's
uncommitted implementation described in [the original report](README.md).
The supervisor, shared schema/wire/templates, Claude adapter and step-1 captures
predate Luna's changes. The untracked step-2 handoff was pre-existing according
to Luna's report; git alone cannot establish an untracked file's earlier contents.
The initial review changed no repository files. The user subsequently authorized
repairs and a plan for the next step, without authorizing its implementation.

## Findings and resolution

All three findings were P2. Frozen bundled requests passed, but edge cases exposed
violations of the preparation contract. A standalone Rust probe under `/tmp`
reproduced each against the original implementation; permanent regressions now
encode independent expectations.

1. **Existing required constraints were lost.** With properties containing
   `visible` and required containing `visible` and `must_survive`, adaptation
   silently deleted `must_survive`. The adapter now rejects a required name
   outside an existing properties map, identifying the exact required array index
   and name. A standalone required constraint without a properties map stays
   unchanged. The observed all-properties-required transformation remains intact.
2. **PATH lookup chose a non-executable regular file.** Given a mode-0644
   `first/codex` and a mode-0755 `second/codex`, the first was selected. Absolute
   directory paths also passed construction; empty PATH components were skipped.
   Lookup now checks regular-file status and Unix execute access using effective
   credentials through the existing rustix dependency. It continues to later PATH
   entries, rejects unusable explicit paths, and resolves empty/relative PATH
   entries against the caller's supplied base directory before changing child cwd.
   This does not install or select a new version: the first runnable Codex in the
   selected PATH wins. OS launch remains authoritative for races, executable
   format and interpreter availability. Non-Unix launching remains unsupported.
3. **Unsupported structures were silently accepted.** Legacy schema-valued
   dependencies passed through without adapting their nested properties. The
   adapter now rejects non-2020-12 explicit dialects and unsupported dependencies,
   additionalItems, dynamic/recursive references and vocabulary declarations with
   locations. Absent dialect means the project's implicit 2020-12 subset.
   This is a bounded compatibility walker, not a complete JSON Schema validator;
   annotations/defaults/examples remain opaque. Supporting more structural forms
   requires deliberate implementation and tests, not arbitrary recursive JSON walks.

An additional test-quality improvement makes the no-launch fixture executable and
uses a positive control after preparation/drop to prove the harmless script really
creates its marker when invoked. It also checks direct prepared-request drop removes
the workspace. Only this test control launches that local fixture; production
preparation still launches no process. No installed Codex or model is invoked.

## Evidence and boundaries

Three new tests were added before the fixes. All failed at runtime for their
intended reasons: lost required constraint, wrong executable candidate, and accepted
unsupported structure. [Red log](evidence/repairs-red.log) and
[green log](evidence/repairs-green.log) are retained. Each combined test's later
edge assertions were expanded verification; only its first failing assertion is
claimed as observed red evidence. The positive-control test refinement was added
after the repairs and passed on its first run; see
[final focused log](evidence/repairs-final-focused.log). No compiler failure or
intentional production sabotage is counted as TDD evidence.

The original report's historical TDD narrative is preserved, including its admitted
invocation test-first deviation. These new logs do not retroactively verify that
history. The original review independently passed 205 tests and all eleven
mutations, formatting, Clippy and step-1 evidence hashes; passing checks did not
prevent the above findings.

New regressions are registered under ARCH-001 and ARCH-003. Overall affected IDs
remain ARCH-001, ARCH-002, ARCH-003, TEXT-001 and PROMPTS-002. Existing registry
entries and all eleven mutation patches are retained without weakening assertions.
Shared wire/schema/template code and the Claude adapter are unchanged. No codec,
Backend implementation, supervisor redesign, headless feature or prompt override
was added. BACKENDS-001 and PROMPTS-003 remain pending.

Authentication, configured-versus-observed model and isolation claims are unchanged:
request preparation does not authenticate; a selected model is not observed model
provenance; local AGENTS.md suppression is not account/tool isolation. No new live
backend evidence is claimed. Filesystem checks cannot eliminate launch-time races.

Final verification passed: formatting, warnings-denied workspace Clippy,
`git diff --check`, and `bash scripts/check_contracts.sh` with **208 Rust tests
including doctests and all eleven runtime mutations**. See
[verification](evidence/repairs-verification.log) and
[full contract log](evidence/repairs-contracts.log). The three review defects are
repaired; step 2 is ready for commit review, and remains uncommitted.
The next implementation slice is the [step-3 handoff](../../docs/plans/phase1-codex-step3-handoff.md).
