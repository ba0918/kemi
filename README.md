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
kemi manifest.json                 # a manifest (legacy diff-review compatible)
kemi -                             # read the manifest from stdin
kemi --from main                   # commits in main..HEAD, one group per commit
kemi --from main --group-by file   # the branch as one diff, one entry per path
kemi --worktree                    # HEAD versus the working tree
kemi --staged                      # HEAD versus the index
kemi --digest                      # print the review map and exit
```

Useful flags:

| Flag | Meaning |
|---|---|
| `--to <ref>` | end of the commit range (default `HEAD`) |
| `--group-by <commit\|file>` | commit range grouping (default `commit`) |
| `--focus <path>` | JSON file with focus flags and notes |
| `--base <dir>` | base for manifest and focus paths (default `.`) |
| `--port <n>` | listen port (default `0`, pick a free one) |
| `--no-open` | do not open the browser |
| `--digest-top <n>` | number of top files in the digest (default `100`) |

When the server starts, kemi prints one line to stderr:

```
kemi: http://127.0.0.1:<port>/s/<token>/
```

Open that URL, read the change, and press **承認** (approve) or **変更要求**
(request changes). kemi prints the JSON below to stdout and exits with 0
(approved), 1 (changes requested), 2 (usage, startup, or runtime error), or
130 (interrupted before submit).

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
      "replies": ["a reply"],
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
```

The specification is `docs/spec/kemi.md` (Japanese). The UI and docs are
Japanese; this README is English.

## License

MIT OR Apache-2.0. See `LICENSE-MIT` and `LICENSE-APACHE`.
