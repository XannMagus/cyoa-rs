# Create your own adventure

A player shapes an ongoing story in a generated world by choosing a protagonist
and directing successive turns.

## Language

### World and characters

**Brief**:
The player's starting description of the adventure they want to play.
_Avoid_: System prompt, world outline

**World outline**:
The setting, premise and title of an adventure before its cast is generated.
_Avoid_: World, brief

**World**:
The adventure's setting and premise together with its playable characters and NPCs.
_Avoid_: World outline, story summary

**Playable character**:
A character offered to the player as a possible protagonist.
_Avoid_: Protagonist (before selection)

**Protagonist**:
The character currently serving as the player's viewpoint in the adventure.
_Avoid_: Player (the human directing the story)

**NPC**:
A character currently controlled by the narrator, including alternative
protagonists until they become the played viewpoint.
_Avoid_: Currently played protagonist

**Protagonist roster**:
The initial candidate protagonists who continue in the shared world and are
eligible to succeed the played protagonist.
_Avoid_: Every NPC

**Protagonist succession**:
Continuation after the protagonist's death through a surviving eligible character
chosen by the storyteller.
_Avoid_: Rewind, resurrection

**Ruleset**:
The adventure's mechanics governing character capabilities, resources and
resolution of attempted actions.
_Avoid_: Story style

**OOC question**:
An explicitly out-of-character question to the storyteller that does not advance
fictional time or change fictional state. HRP is the equivalent French term.
_Avoid_: In-character question

**World cast**:
The world's playable characters and NPCs, grouped by their roles.
_Avoid_: Summary cast

**Summary cast**:
The characters retained in story memory, including the protagonist and characters
introduced during play.
_Avoid_: World cast

**Character ID**:
A character's stable identity across changes to their name or description.
_Avoid_: Character name

**Namesakes**:
Distinct characters who share a name.
_Avoid_: Duplicates (unless the complete character records are equal)

### Story and memory

**Game**:
An adventure with its selected protagonist, story style, settings and accepted turns.
_Avoid_: Save slot, session

**Turn**:
One accepted story passage with its offered actions and changes to story memory.
_Avoid_: Generation request, narrative preview

**Opening turn**:
The first turn of an adventure, before any earlier passage exists.
_Avoid_: World generation, cast generation

**Narrative**:
The prose passage of a turn.
_Avoid_: Story summary, scene description

**Scene description**:
An optional visual description of the turn's scene.
_Avoid_: Narrative

**Quick action**:
A suggested direction the player can choose for the next turn.
_Avoid_: Command (which may instead control saving or the session)

**Chapter**:
A contiguous group of turns forming a section of the story, with an effective title.
_Avoid_: Turn

**Story summary**:
The adventure's current memory: world, characters, major events, current situation
and upcoming events.
_Avoid_: Transcript, narrative

**Summary update**:
The proposed changes to story memory accompanying a generated turn.
_Avoid_: Complete story summary

**Major events**:
The bounded record of significant developments retained in story memory.
_Avoid_: Full transcript

**Upcoming events**:
The unresolved threads or anticipated developments retained in story memory.
_Avoid_: Major events, promised future turns

**Prose context**:
The recent passages supplied to maintain continuity while generating the next turn.
_Avoid_: Full transcript, story summary

**Chapter bridge**:
The passages from the preceding chapter included in prose context while the new
chapter has too few turns of its own.
_Avoid_: Chapter summary

**Rewind**:
Removal of accepted turns from the end of the adventure, restoring its earlier
story position and memory.
_Avoid_: Generation retry

**Original limits**:
The adventure's limits when the game was created.
_Avoid_: Active limits

**Active limits**:
The limits governing the adventure now, chosen at creation or restoration.
_Avoid_: Original limits, current configuration (which may differ after restoration)

### Play and saves

**Session**:
The current play interaction, including the game and any generation or save work
in progress.
_Avoid_: Game, save slot

**Canonical game**:
The accepted game held by the session, distinct from tentative generated content.
_Avoid_: Narrative preview, durable save

**Narrative preview**:
Tentative prose shown while a turn is being generated, before the turn is accepted.
_Avoid_: Accepted turn, canonical narrative

**Story source**:
Whether the adventure uses live generation or a particular fixed demo scenario.
_Avoid_: Inference backend

**Demo scenario**:
A fixed sequence of prepared adventure passages used for play without live generation.
_Avoid_: Live story

**Save slot**:
A persistent identity for an adventure's primary save and its backup.
_Avoid_: World title, session

**Save snapshot**:
The game and story source captured for saving.
_Avoid_: Story summary, narrative preview

**Primary save**:
The current saved document in a save slot.
_Avoid_: Backup

**Backup**:
The preceding valid primary save retained for explicit recovery.
_Avoid_: Autosave (the act of saving), automatic recovery

**Save-copy**:
Saving the current adventure into a fresh slot and switching to that slot after success.
_Avoid_: Backup rotation

**Backup recovery**:
Explicitly resuming a backup into a fresh save slot while preserving the original slot.
_Avoid_: Automatic fallback, primary replacement

**Dirty game**:
A canonical game whose current state has not yet been acknowledged as durably saved.
_Avoid_: Failed generation

**Uncertain save**:
A save attempt whose durable outcome has not been established.
_Avoid_: Unchanged save, successful save

**Save reconciliation**:
Establishing the outcome of an uncertain save attempt using that same attempt's identity.
_Avoid_: New save attempt, generation retry
