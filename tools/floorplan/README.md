# Project Floorplan

From a checkout, run:

```sh
node tools/floorplan/run.mjs
```

Requires Node.js 18 or newer and Git. The first run downloads the exact upstream
commit in `floorplan.lock.json` into an ignored local cache. Later runs reuse it.
There are no npm dependencies and building does not invoke a model. Ask uses your
locally authenticated coding CLI through Floorplan's own server.

The browser opens **App relationships**, the logical description of the app.
**Architecture layers** is an alternative grouping of the same files into
Presentation, Application, Domain and Infrastructure, with a separate composition
root. Switching tabs preserves each map's navigation state. Each view retains
Floorplan's native zoom, tours, Health and Ask interface.

This is project-owned orchestration around unmodified Floorplan 0.11.0, not an
upstream multi-view extension. Layer membership is explicit in the layer
architecture configuration; it is not inferred from Rust types. The compiler and
`scripts/check_architecture.sh` enforce dependency boundaries independently.

## Sources and generated output

Tracked inputs live here:

- `views/app/architecture.json`: logical relationships, layout, tour and notes.
- `views/layers/architecture.json`: alternative grouping, layout, tour and notes.
- `descriptions/`: shared source-file names and descriptions.
- `views.mjs`: the two-view browser switcher.
- `floorplan.lock.json`: upstream repository, version and exact revision.
- `health.json`: latest built Health findings, check coverage and build warnings.
- Optional `custom-flows.json` and `health-dismissed.json` inside either view:
  saved flows and Health decisions captured from that view.

Everything under the repository's `floorplan/` and `floorplan-layers/` directories is generated or local:
HTML, graphs, upstream checkout, scan caches, logs, server addresses and tokens.
None is committed. A new checkout rebuilds its own interactive views; there is no
tracked offline HTML snapshot.

## Commands

```sh
node tools/floorplan/run.mjs setup           # Download/verify the pinned tool only
node tools/floorplan/run.mjs build           # Rebuild both maps and Health report
node tools/floorplan/run.mjs --no-open       # Build/start and print the browser URL
node tools/floorplan/run.mjs serve           # Start/reuse the existing built maps
node tools/floorplan/run.mjs capture         # Save authored inputs after map edits
node tools/floorplan/run.mjs stop            # Stop both local servers
node --test tools/floorplan/run.test.mjs     # Tooling boundary tests
```

Servers bind to loopback, prefer ports 4378 and 4380, and exit after 120 idle
minutes. The switcher uses the actual allocated ports. Linux has been exercised;
the macOS/Windows browser-opening branches have not. Game backend platform support
is separate from this viewer.

For an existing local upstream checkout, set `FLOORPLAN_SOURCE_DIR` to its path.
It must match the pinned commit and version and have no modified tracked files.
This allows an initial offline build when the pinned source is already available;
the installed plugin's location is never assumed.

## Maintaining the map

Edit the tracked inputs directly, then run `build`. Keep both architecture
configurations aligned when adding, moving or removing mapped source files.
Descriptions are shared, while each view owns its own grouping and tour. Optional
tooling in this directory is intentionally excluded from the application map.

When using the ordinary Floorplan skill to update the generated app map, run
`capture` **before rebuilding**. It copies the app architecture and shared
descriptions back here, along with each view's saved flows and Health decisions.
It deliberately does not replace the tracked layer architecture: update that file
explicitly to preserve its grouping. Review the Git diff, build both views, then
commit the authored inputs and any changed Health report.

Both candidates must have complete descriptions and identical file coverage
before either served map is replaced. New unassigned files fail the build instead
of silently disappearing. Building does not generate fresh explanatory prose;
maintaining the descriptions remains an agent/human task.

Health findings are retained in Git, including vulnerabilities when detected.
Zero findings are not a security audit: `health.json` records which checks ran
and which were inapplicable. The current secret-looking-text warning refers to a
review-directory reference in `process.rs` documentation, not a credential; it is
retained rather than suppressed. Native Floorplan masks that text on the map.
Review Health and dismissal changes alongside code changes.

To update upstream, deliberately change the lock's version and revision, verify
compatibility, rerun the tooling tests and both-view build, and review the resulting
Health/source diff. The runner never upgrades automatically.
