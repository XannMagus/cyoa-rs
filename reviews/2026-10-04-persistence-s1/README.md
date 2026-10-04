# Phase 2 step 1: checked established-world reconstruction

Affected decisions: ARCH-002, STATE-001, LIMITS-001, IDENTITY-001.

Added `WorldCast::restore` with no generation limits parameter. It rejects empty
playables and exact duplicates in either role, preserving distinct namesakes and
all role order. Existing generation construction remains unchanged. Four domain
regressions check rejection, preserved order/selection, raised playable minimum
and zero NPC cap during game restore, and a one-playable zero-turn game.

The initial focused run failed to compile because the requested API did not exist
(`/tmp/phase2-s1-red.log` in this session). After implementation, all four focused
Nextest tests passed. This initial API red run is not behavioral mutation evidence.
Formatting and workspace Clippy passed. `bash scripts/check_contracts.sh` passed
341 runtime tests, seven compile-fail doctests, all 37 handwritten mutations and
the domain sweep: 179 tested, 100 caught, 79 compiler-invalid. Two caught cases
were per-test nontermination, distinguished from assertion failures. No mutation
exclusions or assertions were weakened.
Persistence DTOs, filesystem behavior and headless integration are not claimed.
