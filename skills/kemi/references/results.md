# Result reference

## Reading the result

```json
{
  "kemi": 4,
  "title": "Review",
  "verdict": "approved",
  "approval": [{"path": "src/a.rs", "identity": "sha256:..."}],
  "diff_display_version": "01K00000000000000000000000:1",
  "explanation": null,
  "comments": [
    {
      "id": "c1",
      "target_type": "file",
      "explanation": null,
      "group_id": "g1",
      "group_title": "First change",
      "path": "src/a.rs",
      "side": "new",
      "start_line": 12,
      "end_line": 14,
      "quote": ["the line text when the comment was written"],
      "body": "the comment",
      "page": null,
      "replies": [
        {
          "id": "r1",
          "author": "reviewer",
          "body": "a reply in the thread",
          "parts": null,
          "variants": [],
          "chosen": null,
          "applied": null
        }
      ],
      "resolved": false,
      "outdated": false,
      "suggestion": {"replacement": "the replacement text"}
    }
  ],
  "messages": [{"id": "m1", "author": "reviewer", "body": "a note on the whole review", "parts": null}]
}
```

- `kemi` is the version of this contract, `4` for new result and wait output. Read it before
  trusting the shape, and do not read a version you do not know by guessing. `kemi --result`
  prints a stored result as it was saved, version included. A version `3` result has the same
  shape, except that `explanation.versions` always holds the retained versions and no page
  points were rounded or thinned. Version `1` results (written by kemi before replies and
  messages existed) have `replies` as plain strings and no `page` or `messages`. Resuming an
  old session and submitting it creates a new version `4` result.
- `verdict` is `approved` or `changes_requested`, and agrees with the exit code.
- On `approved` the person accepts the bytes named in `approval`. Before acting on the approval,
  compute each `identity` again and compare. If a file changed after the review began, say so
  instead of proceeding on an approval of bytes that no longer exist. Comments may still be
  present on an approval; read them and address or report them.
- On `changes_requested`, address every comment, then offer a new review of the result.
- Line numbers are 1-based and inclusive. `side` is `new` or `old`, and `old` means the old
  file's own numbering. A comment on a whole file has `start_line` and `end_line` `null`,
  `quote` `[]`, and `side` `new`.
- `suggestion` replaces new-side lines `start_line` through `end_line` of `path` with
  `replacement`; an empty `replacement` means those lines should go. It is `null` when the
  comment carries no suggestion, and always `null` for old-side and whole-file comments. kemi
  never applies a suggestion, you do: check `quote` against the file as it stands now before
  you write.
- `outdated` is `true` when the file changed after the comment was written, or when the commit
  the comment was sitting on left the range. kemi does not renumber a comment, so do not trust
  the line numbers of an outdated one: locate the place by `quote`, and ask the person when it
  cannot be found.
- `replies` is the thread under the comment, in creation order, and `[]` when there is none.
  `author` is `reviewer` (the person) or `agent` (you, through `kemi reply`). `variants`,
  `chosen`, and `applied` are `[]`, `null`, and `null` outside a review of a running page.
- `resolved` is `true` when the person closed the thread. Only the person resolves.
- `page` is `null` except on a comment the person left on a running page (`kemi --live`);
  its shape is in [page comments](live.md#page-comments).
- `messages` are notes on the whole review rather than on one comment, in creation order, with
  the same `author` values; `[]` when there are none.
- `comments` come in creation order. `approval` is returned exactly as it was given, and is `[]`
  when none was.
- `body` and `parts` are both required in every reply and message. Plain content is a string
  body, including an empty string, with null parts. Typed content is a null body and a nonempty
  parts array. Keep the entire typed content, including roles, optional fields, coordinates,
  and order; do not turn it into a plain body.
- `target_type` distinguishes file, page, and explanation comments. For file/page,
  `explanation` is null. For an explanation, group/file/line/page/suggestion fields are null
  and the line quote is `[]`. `explanation` contains its original `revision`, `block_id`,
  a quote of `section_title` and the complete `block` with its part and references, and
  `selection`. `selection` is `{text, occurrence}` when the person selected text in a text or
  code block (`text` is the selected part of the block's displayed text, `occurrence` the
  0-based index of that match among all matches of `text`, overlapping ones included), and
  `null` for a diagram or a comment without a selection. The comment's target is still the
  whole block: treat the block quote as authoritative and `selection` as a hint to where the
  person pointed; kemi does not check that `text` occurs in the block. For example:
  `"selection": {"text": "goal", "occurrence": 0}`.
  Those original values never move to a new revision. `outdated` compares part and references
  with the current block of the same id; moving it or changing its section title does not
  make it outdated. Identical content returning under the same id makes it current again.
- Top-level `explanation` is null when the review had no explanation, otherwise
  `{current_revision, versions}`. `versions` is `[]` when no comment targets the explanation;
  its text is then what you wrote (your explanation file, or the last complete replacement you
  sent). With at least one explanation comment, `versions` holds every retained version in
  ascending revision order; read the retained original when a comment targets an old version.
- `diff_display_version` is the review's display version at submit (see
  [explanation updates](explanation-update.md#updating-the-explanation)); it is not an approval
  target.

## Fetching a result you missed

An accepted submit attempts to save the same JSON to a result file. If saving fails, stderr
and the completion screen warn, but the main process's stdout JSON and verdict exit code
remain unchanged. Keep that stdout as the authoritative result; the `result` of a `submitted`
event from `kemi wait` is the same document, so it serves equally. Before relying on `--result`
for recovery, look in stderr for `kemi: could not save the result file`: a failed save may
leave no result or an older stored result, not this submit.

```text
kemi --result                     # the latest result for the repository you are in
kemi --result --any               # the latest result from anywhere
kemi --result --workspace ../app  # the latest result for another place
```

`kemi --result` prints that JSON to stdout and exits with the verdict's own code (`0` approved,
`1` changes requested). It reads what is already stored and never waits for a review still open. With no matching result it prints nothing and exits `2`. A result
belongs to the top of the git repository kemi was started in. Only the newest twenty results
are kept across all repositories, so read a result soon after the review rather than assuming
it will still be there. No result is written for an interrupted or failed run; the review
itself is kept as a session instead when it has state or a copy to keep; see the
[resume reference](resume.md) for eligibility and continuation.

`--result` goes together only with `--any` or `--workspace`. Anything else beside it, including
an input mode, is a usage error (exit `2`). `--any` and `--workspace` are alternatives to each
other: giving both is a usage error, and so is either of them without `--result`.
