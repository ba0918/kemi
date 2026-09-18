# Result reference

## Reading the result

```json
{
  "kemi": 1,
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
      "replies": [],
      "resolved": false,
      "outdated": false,
      "suggestion": {"replacement": "the replacement text"}
    }
  ]
}
```

- `kemi` is the version of this contract, `1` today. Read it before trusting the shape.
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
- `replies` and `resolved` are always `[]` and `false` today; they are in the contract so they
  can be filled later.
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
