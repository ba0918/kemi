# Input reference

## Choosing the input

One input mode per run. Giving two is an error (exit `2`).

| Input | Command | Groups |
|---|---|---|
| A range of commits | `kemi --from <base> [--to <head>]` | both groupings, see below |
| The working tree | `kemi --worktree` | one group, untracked files included |
| The index | `kemi --staged` | one group |
| Anything whose reasons only you know | `kemi <manifest.json>`, or `kemi -` to read it from stdin | the groups you write |

`--to` defaults to `HEAD`. A commit range carries both of its groupings at once, and the page
switches between them without a restart:

- **Final form** (`--group-by file`, the default): the whole range as one diff, each path
  appearing once in its end state, with the commit that made each change block shown above it.
- **Per commit** (`--group-by commit`): one group per non-merge commit in the range, the commit
  subject as its title and the commit message body as its reason.

`--group-by` only picks which of the two the page opens on, so pass `--group-by commit` when
what each commit did is the point of the review. Without `--from` it is a usage error
(exit `2`). Choose a base that leaves out work the person has already reviewed.

Other flags worth knowing:

| Flag | Meaning |
|---|---|
| `--no-open` | do not open a browser. Use it whenever a browser should not appear on the person's machine |
| `--port <n>` | listen on a fixed port; the default `0` picks a free one |
| `--bind <addr>` | listen address as an IPv4 literal; the default `127.0.0.1` keeps the page local, while `0.0.0.0` listens on every IPv4 interface and exposes it to the network |
| `--focus <path>` | a JSON file that adds focus marks and watch points to any input mode |
| `--base <dir>` | where relative paths in a manifest and in `--focus` resolve from; default `.` |

The `--focus` file looks like this:

```json
{
  "groups": { "<group-id>": { "watch": "what to judge in this group" } },
  "files": [{ "path": "src/a.rs", "focus": true, "note": "why this file matters most" }]
}
```

A group id is `all` for the final form, the full commit sha for a per-commit group, `worktree`,
`staged`, or an id you chose in a manifest. An id or a path that does not exist is an error
(exit `2`); kemi never quietly ignores one.

## Surveying a large change first

`--digest` prints a bounded map of the review to stdout as one JSON document and exits without
serving a page. Use it to decide what to put in front of the person, or to orient yourself in
a change too large to read straight through. It gives the totals, the per-group and
per-directory counts, and the files with the most changed lines, with no line content at all,
so it stays small even for tens of thousands of files. `--digest-top <n>` sets how many top
files it asks for (default `100`). For a commit range it follows `--group-by`, so it counts
the change the same way the page will show it.

## Writing a manifest

A manifest is for everything git cannot group for you: drafts, prose, a set of changes whose
reasons live only in your head. The groups and their reasons are your work, not the tool's.

```json
{
  "title": "short name of the change",
  "subtitle": "one sentence on what is being decided",
  "meta": {"label": "value"},
  "groups": [
    {
      "id": "stable-anchor",
      "title": "what this group changes",
      "why": "why it changes, not a restatement of the lines",
      "watch": "what the person should judge here: a choice that could have gone otherwise",
      "diffs": [
        {
          "path": "docs/example.md",
          "old_path": "before/example.md",
          "new_path": "docs/example.md",
          "status": "modify",
          "focus": true,
          "note": "why this file matters most"
        }
      ]
    }
  ],
  "approval": [{"path": "docs/example.md", "identity": "sha256:..."}]
}
```

- `why` and `watch` are what make the page worth reading. `why` is the reason the change was
  made; `watch` is the thing you want judged. Neither can be derived from the diff, so leave one
  out rather than filling it with a restatement of the lines.
- A diff entry gives its two sides either as literal text in `old` and `new`, or as files in
  `old_path` and `new_path`, resolved against `--base`. A side that is absent means the file was
  added or deleted. Giving both `old` and `old_path` (or both `new` and `new_path`), or naming a
  path that does not exist, is an error (exit `2`), never a silently empty side.
- `status` is `add`, `delete`, `rename`, or `modify`, and is inferred when left out.
  `renamed_from` names the old path a rename came from; with `status` left out it makes the
  entry a rename.
- `id` may be left out (`g1`, `g2` and so on are generated), but choose stable ids if a
  `--focus` file will refer to them.
- Fill `approval` whenever the person is being asked to approve something, and say in the
  conversation that the approval is of those bytes. kemi neither shows nor checks `identity`;
  it returns the list unchanged in the result, so that after approval you can compare it with
  the bytes you actually commit or apply. You compute it yourself from the exact bytes in
  question, for instance with `sha256sum` on the file, or `git show :<path>` piped into
  `sha256sum` for a staged one.
