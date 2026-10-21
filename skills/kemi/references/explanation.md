# Explanation reference

## Adding an explanation

For a new human handoff, author the explanation before starting kemi. The initial UTF-8
JSON file has **only** `sections`; the server sets revision 1 and the initial metadata base.
Do not use `--context` at this stage: there is no running review id yet.

1. Decide the content with [writing explanations](explanation-writing.md): what changes and
   how it works, why, the decisions requested, what was decided without the person, and what
   is unverified, shown with diagrams, pseudocode, and examples where the change has structure
   or logic.
2. Split independently judgeable points into separate blocks so each can receive its own
   comment. Keep a block's stable id when updating the same point. The number of blocks follows
   the content; references are needed only where they help the person inspect evidence.
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
It shows the format only; a real handoff carries the content that
[writing explanations](explanation-writing.md) describes.
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
Startup rejects a line reference whose total is unknown with `line count is unknown for this
display version`, which is the usual case for git inputs. Use a whole-file reference there and
name the heading or function in the block text. For a later update, context reports
`old_lines`/`new_lines` only where they are known.

A Page reference is exactly `{"type":"page","url":"/products?x=1"}` with the live path/query,
not the review URL or an arbitrary external URL. It must already be registered in the review;
non-live or unregistered Page references are rejected. If registration is not yet known, omit
that link at startup and add it later using context's registered `references.pages`.

### Diagram parts

Give each diagram its own block id. Which part to use for what is in
[writing explanations](explanation-writing.md#choosing-the-part); tested templates and the layout
rules that make a diagram fail are in its [diagram templates](explanation-writing.md#diagram-templates).
Every diagram part has `type`, `title`, and `role`, plus:

| Type | Fields |
|---|---|
| `diagram` (grid) | `source`: lines of `id = label` (`id = ? label` draws a dashed box), rows `\| a \| b \| . \|` placing ids (`.` is empty), and `a -> b : label` edges |
| `sequence` | `participants`: `{id,label,x?}`, left to right. `events`: `{type:"message",from,to,label,kind,gap_after?}` with `kind` `call` (solid) or `return` (dashed); `{type:"alt",branches:[{condition,messages}],gap_after?}` with 2–8 branches; `{type:"loop",condition,messages,gap_after?}`. Frames do not nest. Optional `layout` `{participant_gap,event_gap,self_loop_width}` and `canvas` `{width,height}` |
| `flow` | `nodes`: `{id,kind,label,position?,width?,height?}` with `kind` `start`, `end`, `process`, or `decision` and `position` `{x,y}` the centre. `edges`: `{from,to,from_port?,to_port?,via?,label?}` with ports `north`, `east`, `south`, `west` (default south to north), `via` a list of `{x,y}`, and `label` `{text,position?}`; every edge leaving a decision needs a label. Optional `canvas` |

A sequence allows up to 12 participants and 96 messages; a flow up to 32 nodes and 64 edges;
coordinates are integers from 0 to 8192. Wrap a diagram in a block with `id`, `part`, and
`references` just like text. Do not use Markdown images as diagrams: they do not load.
Structurally valid diagrams may still fail browser layout, which shows the reason and complete
original JSON, not proof the diagram is correct. For boundary-size documents, consult the
[part fields and limits](https://github.com/ba0918/kemi/blob/main/docs/spec/content-parts.md).

An initial document opens EXPLANATION with a contents list; Conversation is shared with
Code/Page. Without a document the explanation switch is disabled until an update adds one.
Final state / Per commit is a Code-only switch, not a separate explanation.
