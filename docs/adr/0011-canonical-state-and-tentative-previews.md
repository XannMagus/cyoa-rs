# Keep canonical game state separate from tentative previews

Streaming makes prose visible before generation has succeeded, and final payloads
can differ from their previews. Application owns the canonical game and accepts
only a validated, current, non-cancelled result; previews never authorize a commit
or autosave, and the final accepted narrative replaces them. Owned workers keep
generation off the event loop, while bounded transport/output work and explicit
join/cleanup preserve responsiveness without detached requests or automatic retries.

Contracts: [STREAM-001](../decisions/README.md#stream-001-preview-text-never-authorizes-a-state-commit),
[PRODUCT-001](../decisions/README.md#product-001-standalone-tui-persistence-export-and-image-boundary).
TUI rendering remains a pending obligation in those contracts.
