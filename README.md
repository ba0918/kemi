# kemi

kemi is a local review tool. It shows a set of changes in your browser, lets
you attach comments and suggestions to lines, and returns everything you wrote
to the terminal that started it, as a single JSON document. The name comes
from the Japanese verb *kemi suru* (to review).

- Single binary, served on loopback only.
- No browser extension, no build step for the UI.
- kemi never edits your files. Suggestions are returned in the JSON for an
  agent to apply.

## Install

```sh
mise use -g github:ba0918/kemi
```

Supported targets: `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`
(static), `x86_64-apple-darwin`, `aarch64-apple-darwin`.

## Usage

```sh
kemi manifest.json                   # a manifest (legacy diff-review compatible)
kemi -                               # read the manifest from stdin
kemi --from main                     # the branch as one diff, one entry per path
kemi --from main --group-by commit   # start with one group per commit in main..HEAD
kemi --worktree                      # HEAD versus the working tree
kemi --staged                        # HEAD versus the index
kemi --from main --digest            # print the review map and exit
kemi --result                        # print the last submitted result here and exit
```

A commit range has two ways to group the same changes, and the page switches
between them without restarting:

- **Final form** (`--group-by file`, the default): `from...to` as one diff, each
  path once. Every change block shows the commits that made it (its origin);
  click one to read the commit message or to open that commit's view of the
  same lines.
- **Per commit** (`--group-by commit`): one group per non-merge commit in
  `from..to`.

`--group-by` only picks the grouping shown first. **Changed:** it now defaults
to `file`; earlier versions defaulted to `commit`, so pass `--group-by commit`
to keep the old start. `--digest` follows the same grouping.

Useful flags:

| Flag | Meaning |
|---|---|
| `--to <ref>` | end of the commit range (default `HEAD`) |
| `--group-by <commit\|file>` | grouping shown first for a commit range (default `file`) |
| `--focus <path>` | JSON file with focus flags and notes |
| `--base <dir>` | base for manifest and focus paths (default `.`) |
| `--port <n>` | listen port (default `0`, pick a free one) |
| `--no-open` | do not open the browser |
| `--digest-top <n>` | number of top files in the digest (default `100`) |
| `--result` | print the latest result for this repository and exit |
| `--any` | with `--result`: the latest result from anywhere |
| `--workspace <path>` | with `--result`: the latest result for that place instead of the current one |

When the server starts, kemi prints one line to stderr:

```
kemi: http://127.0.0.1:<port>/s/<token>/
```

Open that URL, read the change, and press **承認** (approve) or **変更要求**
(request changes). kemi prints the JSON below to stdout and exits with 0
(approved), 1 (changes requested), 2 (usage, startup, or runtime error), or
130 (interrupted before submit).

Keys in the page (ignored while typing in a text field):

| Key | Action |
|---|---|
| `n` / `p` | next / previous change or comment, moving on to the next / previous file |
| `v` | mark the current file as seen, or unmark it |
| `j` / `k` | next / previous file |
| `g` / `G` | first / last file |
| `u` / `s` | one column (unified) / two columns (split) |
| `w` | toggle line wrapping |

## Result file

The same JSON that goes to stdout is also written to a result file, so an
agent that missed the output can read it later:

```sh
kemi --result                    # latest result for this repository (exit code = its verdict)
kemi --result --any              # latest result from any place
kemi --result --workspace ../app # latest result for another place
```

- Location: `$XDG_STATE_HOME/kemi/results/`, or
  `~/.local/state/kemi/results/` when `XDG_STATE_HOME` is unset or relative.
  kemi prints the location to stderr when the server starts.
- One file per submit, readable by you only (`0600`, directory `0700`). The 20
  newest results are kept across all repositories; older ones are removed.
- A result belongs to the top of the git repository kemi was started in (or
  to that directory outside git). The JSON itself is unchanged.
- `kemi --result` prints nothing and exits with 2 when there is no matching
  result. A result that cannot be saved only produces a warning; stdout and
  the exit code stay the same.

## Manifest format

```json
{
  "title": "Review",
  "subtitle": "",
  "meta": {},
  "groups": [
    {
      "id": "g1",
      "title": "First change",
      "why": "why it changed",
      "watch": "what to look at",
      "diffs": [
        {
          "path": "src/a.rs",
          "old": "…",
          "new": "…",
          "status": "modify",
          "renamed_from": "src/old.rs",
          "focus": true,
          "note": "the important part"
        }
      ]
    }
  ],
  "approval": [{ "path": "src/a.rs", "identity": "sha256:…" }]
}
```

`old`/`new` give the two sides as strings. `old_path`/`new_path` read them from
files instead. Setting both `old` and `old_path` (or `new` and `new_path`) is
an error. A missing side means an addition or a deletion. `status` is
`add`, `delete`, `rename`, or `modify`, and is inferred when omitted.

## Focus layer

`--focus focus.json` adds focus flags without editing the manifest:

```json
{
  "groups": { "g1": { "watch": "look at the boundary" } },
  "files": [{ "path": "src/a.rs", "focus": true, "note": "decision point" }]
}
```

Unknown group ids and paths are errors. kemi never guesses.

## Submit contract

The JSON printed at the end:

```json
{
  "kemi": 1,
  "title": "Review",
  "verdict": "approved",
  "approval": [{ "path": "src/a.rs", "identity": "sha256:…" }],
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
      "suggestion": { "replacement": "the replacement text" }
    }
  ]
}
```

- `side` is `new` or `old`. File-wide comments are `new` with `start_line` and
  `end_line` `null`, and `quote` `[]`.
- `suggestion` is `null` for old-side and file-wide comments. An empty
  replacement means the lines should be deleted.
- `outdated` is `true` when the file changed after the comment was written.
  kemi never moves a comment to a new line number.
- `comments` are in creation order.

## Digest

`--digest` prints a bounded map of the review without line contents:

```json
{
  "kemi": 1,
  "title": "Review",
  "totals": { "files": 30000, "add": 123456, "del": 7890, "noise_files": 12000 },
  "groups": [ { "id": "g1", "title": "…", "why": "…", "watch": "…", "files": 10, "add": 100, "del": 20 } ],
  "directories": [ { "path": "src", "files": 100, "add": 1000, "del": 200, "noise_files": 3 } ],
  "top_files": [ { "path": "src/a.rs", "status": "modify", "add": 100, "del": 20, "noise": false, "focus": true, "note": "…" } ],
  "top_files_omitted": { "files": 0, "add": 0, "del": 0 },
  "top_n": 100
}
```

## Development

```sh
cargo build --release          # the binary
cargo test                     # Rust tests
node --test web                # frontend pure logic
npx tsc -p web --noEmit        # frontend types (JSDoc, no build step)
scripts/gen-fixture.sh <dir> --files N --lines M [--commits K]
scripts/measure-startup.sh <fixture> target/release/kemi
scripts/measure-range.sh <fixture> target/release/kemi <commit|file|busy>
```

The specification is `docs/spec/kemi.md` (Japanese). The UI and docs are
Japanese; this README is English.

## License

MIT OR Apache-2.0. See `LICENSE-MIT` and `LICENSE-APACHE`.
