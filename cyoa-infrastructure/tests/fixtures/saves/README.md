# Independent version-one save fixtures

These documents specify the persistence boundary, independently of the save
encoder. `v1-minimal.json` explicitly selects the second Ajax with a zero-turn
log and omits source-defaulted collections/styles. `v1-full-story.json` uses the
existing Phase 0 story's five turn proposals with explicitly supplied post-turn
snapshots, not encoder output. Its frozen expected views are:

- Original and active limits: events 4, NPC generation 8, bridge 3, minimum 2.
- Selected playable: Ajax the mason, position 1. Two distinct playable Ajaxes
  and two distinct NPC Ajaxes retain role order. Summary IDs are protagonist,
  ajax, ajax-2; their descriptions are mason, merchant, guard.
- Each snapshot's major events: A/B, A/B/C, A/B/C/D, B/C/D/E, C/D/E/F.
  Upcoming threads: Depart, Depart, empty, empty, empty.
- Chapters: turn 0's new-chapter marker still means chapter zero, retitled to
  Retitled by turn 1; turn 2 opens chapter one, retitled At Sea by turn 3.
- Empty, whitespace, CRLF, quoted and Unicode raw strings remain exact audit
  records. Turn 2 has explicit instructions/prompt trace. Turn 1 has observed
  provider/model and a zero list-price estimate; other provenance is absent.
- Unknown nonblank art/pace/tone/narration keys survive, using bundled prompt
  fallback. The document's revision is 7; its timestamp is UTC milliseconds.

`v1-optional-extra.json` adds ignorable optional metadata, a future style and a
character portrait reference. Loading reports their omission on re-save while
all known values survive. The portrait is a schema extension fixture, not an
implemented image or asset feature. Invalid documents cover duplicate keys and
future versions; the validation tests additionally mutate every required shape
and representative aggregate invariants. Synthetic migration unit tests exercise
only dispatcher mechanics, not support for a shipped historical format.

All tests are codec/in-memory acceptance. No disk durability, helper shutdown,
headless persistence, automatic source switching or Calibre import is claimed.
