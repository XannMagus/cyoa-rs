# Notice

This project is a Rust port of the "Create your own adventure" (CYOA) feature
built into [calibre](https://calibre-ebook.com/), originally implemented in
Python by Kovid Goyal:

- `src/calibre/ai/cyoa.py` — © 2026, Kovid Goyal <kovid at kovidgoyal.net>, GPLv3
- `src/calibre/ai/structured.py` — same license/author
- `src/calibre/gui2/cyoa/epub.py` — same license/author

The prompt text, JSON schema field descriptions, and the story-state merge
algorithm in this project are ported from those files, in places verbatim.
That makes this project a derivative work, so it is licensed **GPL-3.0**, the
same as calibre — see `LICENSE`.

Upstream project: https://github.com/kovidgoyal/calibre

`reference/calibre/` contains temporary copies of the three files above, kept
only as a porting aid. They should be deleted once the port is complete; their
removal does not change the licensing position above.
