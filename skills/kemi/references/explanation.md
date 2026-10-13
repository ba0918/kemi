# Explanation reference

## Adding an explanation

For a new human handoff, author the explanation before starting kemi. The initial UTF-8
JSON file has **only** `sections`; the server sets revision 1 and the initial metadata base.
Do not use `--context` at this stage: there is no running review id yet.

1. State the change and reason, requested judgment, and unverified points or caveats.
2. Split independently judgeable points into separate blocks so each can receive its own
   comment. Keep a block's stable id when updating the same point. One short text block can
   cover a small change; neither three chapters nor references on every block are required.
3. Attach typed Code references where inspecting a particular reviewed file helps judgment.
   Use known input group/path/side values; leave both line bounds null when counts are unknown.
   Keep approval hashes in the approval record, not long identity lists in the explanation:
   byte identity and navigation have different purposes.
4. Save the file outside the reviewed working tree when it is only handoff material, otherwise
   `--worktree` includes it as an untracked file. Start, for example,
   `kemi --worktree --explanation explanation.json`.

Startup validates and persistently saves the document before publishing the URL. Invalid
JSON/references or unavailable storage exits `2` before publication. Successful startup
with the explanation needs no separate update/check before handoff.
`--explanation` reads a file even with a manifest on stdin; it cannot combine with `--digest`,
`--result`, `--resume`, wait, reply, or explain.

### Text, code, and commentable blocks

A section has `id`, plain `title`, and nonempty `blocks`. A block has `id`, exactly one
typed `part`, and `references` (possibly `[]`). Section ids are unique within the document;
block ids are unique across all sections. New ids match `[A-Za-z][A-Za-z0-9_-]{0,63}`.

| Part | Fields | Use |
|---|---|---|
| `text` | `type`, `body` | Reason, judgment, caveat; safe Markdown, images disabled |
| `code` | `type`, `body`, `language`, `role`; optional `title` | Literal, read-only snippet; not a suggestion or executed code |

Code `role` is `proposal`, `example`, or `confirmed`: the author's label, not kemi's
verification. `language` is a literal string; `pseudocode` has a special display label.
Omit optional fields rather than setting them to null. Unknown part fields are rejected.
Text bodies allow 0–16,384 Unicode scalar values, code bodies 0–32,768; titles 1–120 and
languages 1–64, neither whitespace-only. A document allows 1–128 sections, 1–512 total
blocks, and 0–32 references per block, within 524,288 UTF-8 bytes of string values and
1,048,576 bytes of canonical JSON. Do not pad a valid empty body with meaningless prose.

This complete synthetic initial document assumes the working-tree input contains the
new side of `web/submit.js`. Replace its content and target with the actual reviewed input.
Each judgment is a separate text block; the snippet is separately commentable too:

```json
{
  "sections": [
    {
      "id": "submission",
      "title": "Make the final action explicit",
      "blocks": [
        {
          "id": "wording",
          "part": {"type": "text", "body": "Changed Save to Submit review because the button sends the final verdict, not a draft. Please judge whether the wording makes that distinction clear."},
          "references": [{"type": "code", "unit": null, "group_id": "worktree", "path": "web/submit.js", "side": "new", "start_line": null, "end_line": null}]
        },
        {
          "id": "label-example",
          "part": {"type": "code", "language": "javascript", "role": "example", "title": "Label example", "body": "button.textContent = 'Submit review';"},
          "references": []
        },
        {
          "id": "narrow-width",
          "part": {"type": "text", "body": "The longer label may wrap on narrow screens. Please judge whether wrapping is acceptable. Narrow widths have not yet been checked."},
          "references": [{"type": "code", "unit": null, "group_id": "worktree", "path": "web/submit.js", "side": "new", "start_line": null, "end_line": null}]
        }
      ]
    }
  ]
}
```

### Code and Page references

Code references require every field shown in the example: `type`, `unit`, `group_id`, `path`,
`side`, `start_line`, `end_line`. They are typed links, not arbitrary file access.

| Input/grouping | `unit` | `group_id` |
|---|---|---|
| Working tree, including live Code | `null` | `"worktree"` |
| Staged | `null` | `"staged"` |
| Manifest | `null` | The manifest group id (generated `g1`, `g2`, … if omitted) |
| Commit range, final form | `"file"` | `"all"` |
| Commit range, per commit | `"commit"` | The full commit SHA |

Use the exact path in that group's entry and an existing side (`"old"` or `"new"`). For a
whole-file reference both line bounds are null. A line reference requires known total lines
and two 1-based inclusive integers with start ≤ end, within that side's total. Diff statistics
are not total line counts. Valid whole-file references to the opposite commit grouping are
accepted at startup before its full list is built; do not invent counts to target lines.

A Page reference is exactly `{"type":"page","url":"/products?x=1"}` with the live path/query,
not the review URL or an arbitrary external URL. It must already be registered in the review;
non-live or unregistered Page references are rejected. If registration is not yet known, omit
that link at startup and add it later using context's registered `references.pages`.

Current-base references are validated against exact group/path/unit, side presence, and known
line totals. Known older bases accept Code reference shapes without certifying historical
existence; Page registration is still required. Navigation resolves against the reader's
imported Code list or registered live Page. Missing targets show a reason rather than guessing
a replacement, importing a newer list, or silently shortening a line range.

### Minimal diagram guidance

Use a diagram only when order, branching, or relationships are clearer visually than in a
short paragraph. Give it its own block id and choose `diagram`, `sequence`, or `flow` with
`title` and `role`. A minimal sequence part (one request, no invented response) is:

```json
{"type":"sequence","title":"Submission example","role":"example","participants":[{"id":"reader","label":"Reader"},{"id":"server","label":"Server"}],"events":[{"type":"message","from":"reader","to":"server","label":"Submit verdict","kind":"call"}]}
```

Wrap it in a block with `id`, `part`, and `references` just like text. Do not use Markdown
images as diagrams: they do not load. Structurally valid diagrams may still fail browser
layout, which shows the reason and complete original JSON, not proof the diagram is correct.
For richer diagrams or boundary-size documents, consult the
[part fields and limits](https://github.com/ba0918/kemi/blob/main/docs/spec/content-parts.md).
Ordinary text/code blocks and references need no remote specification lookup.

An initial document opens EXPLANATION with a contents list; Conversation is shared with
Code/Page. Without a document the explanation switch is disabled until an update adds one.
Final state / Per commit is a Code-only switch, not a separate explanation.

## Updating the explanation

Use this procedure only for an already running or resumed review. Reply to ordinary questions
in the thread. Replace the explanation when its description, judgment points, or caveats
become stale, or for a correction/agreed document change. Match what the review actually shows.

```text
kemi explain <id> --context
kemi explain <id> --file update.json
kemi explain <id> < update.json
```

Context reads no stdin and exits `0` with one version-3 JSON object: `review`,
`diff_display_version`, `explanation_revision`, current saved `explanation` or null, and
`references:{code,pages}`. Check `kemi: 3`; do not guess an unknown contract version.
Code entries have `unit`, `group_id`, `path`, `old_present`/`new_present` booleans, and nullable
`old_lines`/`new_lines` totals. Pages are registered `{url}` values. Use this map for updated
references. It is a consistent server metadata read, not file contents or the reader's Code
display, and does not update that display. It is refused with a reason while a required
grouping is unavailable. In a commit range the other grouping starts building on first page
open; if this prevents adding a missing explanation, ask the person to open the page, then
retry context and save before the explanation handoff.

Update JSON has exactly `expected_revision`, `base_display_version`, and the **complete**
replacement `sections`, not a block patch. Use context's `explanation_revision` (0 when absent)
and, for the current review, its `diff_display_version`. For example, if context reports
revision 1 and the following synthetic display version:

```json
{"expected_revision":1,"base_display_version":"01K00000000000000000000000:1","sections":[{"id":"overview","title":"Review purpose","blocks":[{"id":"intent","part":{"type":"text","body":"The label now says Submit review. Please judge whether it clearly describes final submission. Narrow widths remain unchecked."},"references":[]}]}]}
```

Use the actual context token, never a hash, explanation revision, or file id as the base.
Known older bases are allowed but do not prove historical targets exist. A known base can
update an explanation while resumed Code is unavailable; context itself remains refused
until a successful scan supplies metadata again.

Success prints `{revision,base_display_version}` and exits `0`, after atomic save and before
notifying readers. Confirm **both** before claiming it saved or handing off after a missing
explanation was added. `--file` and `--context` are mutually exclusive. Invalid JSON, duplicate
keys, references, limits, revision conflicts, or save failures print no success JSON and exit
`2` with a reason on stderr, leaving the old state intact. Reread context after a revision
conflict rather than blindly overwriting; only one same-revision concurrent update succeeds.
After submit claims the review, updates are refused. The first argument `explain` selects the
subcommand; a manifest of that name is `./explain`.

Explain/context use the same local same-user endpoint protection as wait/reply. They do not
answer a hand-over, clear its working line, reset the response clock, generate wait events,
or resolve threads. Send a separate `kemi reply` when an answer is needed; only the person
resolves. Explanation comments retain their original revision and complete block quote.

Updates send a notice: readers keep their open revision and position until opening it.
Code lists do not refresh automatically. References use the reader's existing list; returning
keeps the original explanation revision/position. The open revision's freshness and the new
notice's freshness are compared separately with that screen's Code version. Matching bases
do not guarantee fixed file bytes or explanation correctness; context describes the server,
not what every reader has imported.
