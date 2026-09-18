# kemi

kemi is a local review tool. It shows a set of changes in your browser, lets
you attach comments and suggestions to lines, and returns everything you wrote
to the terminal that started it, as a single JSON document. The name comes
from the Japanese verb *kemi suru* (to review).

- Single binary. It serves on loopback by default; `--bind` can expose the page
  to your LAN.
- No browser extension, no build step for the UI.
- kemi never edits your files. Suggestions are returned in the JSON for an
  agent to apply.

## Install

```sh
mise use -g github:ba0918/kemi
```

Supported targets: `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`
(static), `x86_64-apple-darwin`, `aarch64-apple-darwin`,
`x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc`.

### Windows

Download the `kemi-<target>.zip` for your machine from the latest GitHub
Release, extract `kemi.exe`, and put it somewhere on your `PATH`. `git` (e.g.
Git for Windows) must be on the `PATH` as well.

## Agent skill

`skills/kemi/` is a document that teaches an agent how to drive kemi: how to
start it and wait for it, which input to pick, how to write a manifest, and how
to read the JSON that comes back. It describes this tool; it is not a rule about
how anyone ought to review.

```sh
gh skill install ba0918/kemi kemi
```

The skill carries no version of its own. `gh skill install` takes the newest
tagged release, or the default branch when there is no release yet, so a newer
skill reaches you by installing it again.

## Usage

```sh
kemi manifest.json                   # a manifest (legacy diff-review compatible)
kemi -                               # read the manifest from stdin
kemi --from main                     # the branch as one diff, one entry per path
kemi --from main --group-by commit   # start with one group per commit in main..HEAD
kemi --worktree                      # HEAD versus the working tree
kemi --worktree --bind 0.0.0.0       # also reachable from other devices
kemi --staged                        # HEAD versus the index
kemi --from main --digest            # print the review map and exit
kemi --result                        # print the last submitted result here and exit
kemi --resume                        # pick an interrupted review (terminal only)
kemi --resume <id>                   # continue that review, from any directory
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
| `--bind <addr>` | listen address as an IPv4 literal (default `127.0.0.1`; `0.0.0.0` is every IPv4 interface) |
| `--no-open` | do not open the browser |
| `--digest-top <n>` | number of top files in the digest (default `100`) |
| `--result` | print the latest result for this repository and exit |
| `--any` | with `--result`: the latest result from anywhere |
| `--workspace <path>` | with `--result`: the latest result for that place instead of the current one |
| `--resume [<id>]` | continue an interrupted review; without an id, choose one (see [Sessions](#sessions)) |

When the server starts, kemi prints one line to stderr. With the default
`--bind 127.0.0.1` it looks like this:

```
kemi: http://127.0.0.1:<port>/s/<token>/
```

Open that URL, read the change, and press **Approve** or **Request changes**.
kemi prints the JSON below to stdout and exits with 0
(approved), 1 (changes requested), 2 (usage, startup, or runtime error), or
130 (interrupted before submit). An interrupted review is kept as a session,
so nothing you wrote on the page is lost; `kemi --resume` picks it up again.

### Exposing kemi to other devices

`--bind 0.0.0.0` listens on every IPv4 interface. kemi then prints a URL for
your machine's default-route address, followed by a warning that the page is
exposed, and opens your browser on `127.0.0.1`. `--bind <your-ip>` listens on
that address only. When the address cannot be determined, the URL keeps
`127.0.0.1` and kemi says so.

Whether another device can reach kemi depends on the OS and the network, not
on kemi. A local firewall may block the port, and on WSL2 the firewall and
forwarding settings on the Windows side also matter. kemi never configures
firewalls or forwarding itself. Pass a fixed `--port` when you expose kemi
repeatedly, so the URL and any forwarding rule stay stable. The review page
and the API are plain HTTP: anyone who can reach the port and knows the URL
can read the diff and submit.

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
  `~/.local/state/kemi/results/` when `XDG_STATE_HOME` is unset or relative
  (unix). On Windows it is `%LOCALAPPDATA%\kemi\results\`.
  kemi prints the location to stderr when the server starts.
- One file per submit, readable by you only (`0600`, directory `0700`; unix
  only — on Windows the default ACL of `%LOCALAPPDATA%` applies). The 20
  newest results are kept across all repositories; older ones are removed.
- A result belongs to the top of the git repository kemi was started in (or
  to that directory outside git). The JSON itself is unchanged.
- `kemi --result` prints nothing and exits with 2 when there is no matching
  result. A result that cannot be saved only produces a warning; stdout and
  the exit code stay the same.

## Sessions

An interrupted review (Ctrl+C, SIGTERM, or a runtime error) is kept as a
session under the same state root as results:
`$XDG_STATE_HOME/kemi/sessions/`, or `~/.local/state/kemi/sessions/` when
`XDG_STATE_HOME` is unset or relative (unix); on Windows it is
`%LOCALAPPDATA%\kemi\sessions\`. A session holds the review as it was when it
started, together with the comments, seen marks, folding, and resolutions.
Permissions match result files (`0600`/`0700`; unix only), and the newest 100
sessions or 500 MB are kept across all workspaces.

```sh
kemi --resume          # in a terminal: choose from the list
kemi --resume <id>     # continue a known session, from any directory
```

- The diff is frozen: changing the working tree, or deleting the repository
  itself, does not change what the page shows. Origin notes become "unknown"
  when the repository is gone.
- Seen marks, folding, comments, and resolutions come back. Live reload and
  its update badge are off, and the URL token is new.
- Resuming continues the same session; it does not create another one. A
  submit from a resumed review writes its result file for the workspace the
  review was started from, and returns the manifest `approval` unchanged.
- Without an id and without a terminal, kemi prints the resumable sessions as
  one tab-separated line each — id, last update, workspace, mode, seen/total —
  newest first, and exits with 0; with none it prints nothing and exits with 2.
- `--resume` accepts only `--port`, `--bind`, `--no-open`, and `--serve`.
  `--digest` and `--result` do not create sessions.

The `kemi: resume with: kemi --resume <id>` line on stderr is how a script
finds the id after an interruption.

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
`renamed_from` names the path a rename came from; with `status` omitted it
makes the entry a rename.

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
- `suggestion` is `null` when the comment carries no suggestion, and always
  `null` for old-side and file-wide comments. An empty replacement means the
  lines should be deleted.
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
