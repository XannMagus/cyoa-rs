# Issue tracker: GitHub

Issues and specs live in GitHub Issues for `XannMagus/cyoa-rs`.
Use the `gh` CLI from this clone.

## Operations

- Create: `gh issue create --title "..." --body-file <file>`.
- Read: `gh issue view <number> --json number,title,body,labels,comments`.
- List: `gh issue list --state open --json number,title,body,labels`,
  with appropriate label filters.
- Comment: `gh issue comment <number> --body-file <file>`.
- Label: `gh issue edit <number> --add-label "..." --remove-label "..."`.
- Close: `gh issue close <number>`.
- Link a child: `gh issue edit <parent> --add-sub-issue <child>`.
  If unavailable, add `Part of #<parent>` to the child and a task-list
  link to the parent.

Write multiline bodies to a temporary file and use `--body-file`.
“Publish to the issue tracker” means create a GitHub issue.
“Fetch the relevant ticket” means read that issue.

## Pull requests as a triage surface

**PRs as a request surface: no.**

## Wayfinding

A map is an issue labelled `wayfinder:map`; its tickets are sub-issues
labelled `wayfinder:research`, `wayfinder:prototype`, `wayfinder:grilling`,
or `wayfinder:task`.

Record blockers through native GitHub issue dependencies, using database
issue IDs. If unavailable, use `Blocked by: #<number>` in the child body.
Select the first open, unassigned child in map order with no open blockers.
Claim it with `gh issue edit <number> --add-assignee @me`.
Resolve it with a findings comment, closure, and a context pointer in the
map’s Decisions-so-far.
