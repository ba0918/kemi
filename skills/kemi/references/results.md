# Result reference

## Reading the result

```json
{
  "kemi": 2,
  "title": "Review",
  "verdict": "approved",
  "approval": [{"path": "src/a.rs", "identity": "sha256:..."}],
  "comments": [
    {
      "id": "c1",
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
  "messages": [{"id": "m1", "author": "reviewer", "body": "a note on the whole review"}]
}
```

- `kemi` is the version of this contract, `2` today. Read it before trusting the shape, and do
  not read a version you do not know by guessing. Version `1` results (written by kemi before
  replies and messages existed) have `replies` as plain strings and no `page` or `messages`;
  `kemi --result` prints such a stored result unchanged.
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
- `page` is `null` except on a comment the person left on a running page (`kemi --live`). Such a
  comment has `group_id` `page` and `group_title` `Page`; its `path`, `side`, `start_line`,
  `end_line`, and `suggestion` are `null`, `quote` is `[]`, and `outdated` is always `false`.
  Its `page` is:

  ```json
  {
    "url": "/products?x=1",
    "width": 390,
    "places": [
      {
        "n": 1,
        "kind": "element",
        "points": [],
        "elements": [{"selector": "#buy", "text": "Buy", "rect": {"x": 0, "y": 120, "w": 200, "h": 60}}]
      }
    ],
    "image": null
  }
  ```

  `url` is the path and query the person was viewing, and `width` the viewport width in CSS
  pixels. `places` are where the comment points, in order of `n`; the body refers to them by
  number, and numbers may skip (a place removed while writing keeps its number gone). `kind` is
  `element` (a chosen element, `points` is `[]`), `arrow` (the points of the arrow, and the
  element at its head), or `pen` (the points of the line, and up to five elements it encloses,
  largest overlap first; elements that contain the whole line, such as a wrapper around the
  page, are left out, and a line drawn inside one element names that innermost element). Each element carries a `selector`, its `text`, and its `rect`; all
  coordinates are CSS pixels of the page, independent of scrolling. For an element inside an
  open shadow root, `selector` joins the host's selector and the selector inside the shadow root
  with ` >>> ` (for example `#host >>> p:nth-of-type(2)`), which is not CSS: query the host in the
  document, then its `shadowRoot` with the part after ` >>> ` (repeated for nested roots).
  `image` is the absolute path of a PNG of the area with the places drawn on it, given only in
  what `kemi wait` returns; in the result it is always `null`, because the file is removed with
  the session on submit.
- `messages` are notes on the whole review rather than on one comment, in creation order, with
  the same `author` values; `[]` when there are none.
- `comments` come in creation order. `approval` is returned exactly as it was given, and is `[]`
  when none was.

## Fetching a result you missed

Every accepted submit is also written to a result file, so losing stdout is not losing the
review.

```text
kemi --result                     # the latest result for the repository you are in
kemi --result --any               # the latest result from anywhere
kemi --result --workspace ../app  # the latest result for another place
```

`kemi --result` prints that JSON to stdout and exits with the verdict's own code (`0` approved,
`1` changes requested). With no matching result it prints nothing and exits `2`. A result
belongs to the top of the git repository kemi was started in. Only the newest twenty results
are kept across all repositories, so read a result soon after the review rather than assuming
it will still be there. No result is written for an interrupted or failed run; the review
itself is kept as a session instead when it has state or a copy to keep, and `kemi --resume`
continues it once its copy completed within the size limit (see the input reference).

`--result` goes together only with `--any` or `--workspace`. Anything else beside it, including
an input mode, is a usage error (exit `2`). `--any` and `--workspace` are alternatives to each
other: giving both is a usage error, and so is either of them without `--result`.
