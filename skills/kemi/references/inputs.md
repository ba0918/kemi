# Input reference

## Choosing the input

One input mode per run. Giving two is an error (exit `2`).

| Input | Command | Groups |
|---|---|---|
| A range of commits | `kemi --from <base> [--to <head>]` | both groupings, see below |
| The working tree | `kemi --worktree` | one group, untracked files included |
| The index | `kemi --staged` | one group |
| A running page and the working tree | `kemi --live <url\|file>` | the working tree, as `--worktree` |
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

## Reviewing a running page

`kemi --live <url|file>` puts a running page under review together with the working tree.
The URL must be `http://` on `localhost` or `127.0.0.0/8` (a dev server or Storybook). A file
must be a `.html` or `.htm` inside the git working tree kemi starts in, or inside the launch
directory outside git; a symlink counts where it points, and nothing in `.git/` is served.
Anything else is a usage error (exit `2`). The code view is the same as `--worktree`; outside
a git repository there is none. `--live` combines only with `--port`, `--bind`, `--no-open`,
`--live-port <n>`, `--focus`, and `--serve`. The review title is `Live review of <url>`, and
for a file the URL is its path from that directory, such as `/docs/mock.html`.

kemi prints `kemi: live <url>` after the review line: the page relayed through a second port
(`--live-port`, default a free one). It opens only in a browser that opened the review URL first,
so send the person the review URL, not this one. When the dev server is not running, the page
says so and waits for it. A file page is served on that port too, reloads when it or a file it
loaded is saved, and waits when the file is missing.

The page has a **Page** view (the running page beside what it is compared with, at a chosen width
from 320 to 3840) and a **Code** view (the working-tree diff). What the page is compared with is a
snapshot — a script-free copy kemi takes at the start, at each hand-over to you, and on
**Record now** — or a mock, an HTML file in the served directory the person assigns to the page.
They can be shown side by side or overlaid with adjustable opacity. Handing over is the person's
action: their next hand-over (or **Record now**) takes a fresh snapshot.

A `--live` review keeps no frozen copy. Resuming it reads the working tree as it is then,
connects to the same URL again, and it is kept only when it holds a comment, a reply, or a message.
Snapshots and mock assignments do not come back on resume yet.

## Surveying a large change first

`--digest` prints a bounded map of the review to stdout as one JSON document and exits without
serving a page. Use it to decide what to put in front of the person, or to orient yourself in
a change too large to read straight through. It gives the totals, the per-group and
per-directory counts, and the files with the most changed lines, with no line content at all,
so it stays small even for tens of thousands of files. `--digest-top <n>` sets how many top
files it asks for (default `100`). For a commit range it follows `--group-by`, so it counts
the change the same way the page will show it.

## Continuing an interrupted review

An interrupted run (exit `130`) keeps its comments, seen marks, folding, and resolutions in a
session. Only a session whose copy completed within the size limit can be listed or resumed;
one whose copy is unfinished or too large keeps the state alone, and a run with nothing to keep
leaves no session. Continue it instead of rebuilding the input:

```text
kemi --resume <id>   # the id is in the `kemi: resume with:` line on stderr
kemi --resume        # in a terminal, choose from the list of sessions
```

The resumed page shows the diff as frozen when the review started, so the working tree or the
repository may have moved on; only origin notes then read "Cannot determine". `--resume` is not an
input mode to combine with the others: it takes only `--port`, `--bind`, `--no-open`, and
`--serve`, and any input mode or `--digest` beside it is a usage error (exit `2`). Without an
id and without a terminal it prints the resumable sessions as tab-separated lines (id, last
update, workspace, mode, seen/total, newest first) and exits `0`, or exits `2` with nothing
on stdout when there are none.

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
