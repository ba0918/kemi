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
kemi --live http://localhost:5173/   # a page on a loopback dev server, plus the working tree
kemi --live docs/mock.html           # a local HTML file, plus the working tree
kemi --from main --digest            # print the review map and exit
kemi --result                        # print the last submitted result here and exit
kemi --resume                        # pick an interrupted review (terminal only)
kemi --resume <id>                   # continue that review, from any directory
kemi wait <id> [--timeout <seconds>] # (agent) wait for what the reviewer hands over
kemi reply <id> < writes.json        # (agent) answer in the page
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
| `--live <url\|file>` | review a running page (see [Live review](#live-review)) |
| `--live-port <n>` | listen port of the `--live` page (default `0`, pick a free one); with `--live` or `--resume` only |

When the server starts, kemi prints one line to stderr. With the default
`--bind 127.0.0.1` it looks like this:

```
kemi: http://127.0.0.1:<port>/s/<token>/
```

Open that URL, read the change, and press **Approve** or **Request changes**.
kemi prints the JSON below to stdout and exits with 0
(approved), 1 (changes requested), 2 (usage, startup, or runtime error), or
130 (interrupted before submit). An interrupted review is kept as a session,
so nothing you wrote on the page is lost; `kemi --resume` picks it up again
when its frozen copy completed within the size limit (see [Sessions](#sessions)).

### Live review

`kemi --live <url|file>` reviews a page that is running, next to the code
that makes it. The page is either an `http://` URL on a loopback host
(`localhost` or `127.0.0.0/8`), such as a dev server or Storybook, or a
`.html`/`.htm` file inside the directory kemi serves: the git working tree
when kemi starts inside one, otherwise the launch directory. Symlinks count
where they point, and nothing inside `.git/` is served. Anything else exits
with 2.

kemi relays the dev server's origin on a second port and prints its URL after
the review line:

```
kemi: live http://127.0.0.1:<port>/<path>
```

The relayed page runs on its own origin and talks to the review page only
through `postMessage`, so the page under development cannot reach kemi's API.
The relay port answers only a browser that has opened the review URL first:
opening it sets an `HttpOnly`, `SameSite=Strict` cookie, which kemi removes
before forwarding a request to the dev server. WebSocket traffic, such as hot
module reloading, passes through unchanged. kemi drops `X-Frame-Options` and
narrows the CSP `frame-ancestors` to the review page, adding only its own
script to `script-src`. When the dev server is not running, the page says so
and waits; it loads once the server starts. `--bind` applies to the relay
too, and its LAN warning adds that the page under development is visible.

A file page is served on the same port, together with the other files in the
served directory that it references. Saving the page or any file it loaded
reloads it, and a link to another HTML file there moves to that page. Paths
outside the directory, symlinks pointing outside, and `.git/` return 404. If
the file is gone, the page says so and waits for it.

The page has two views, switched in the band above it:

- **Page**: the running page next to what it is compared with, at the same
  width — 390, 768, 1280, or any width from 320 to 3840 — scaled down
  together when they do not fit. The page may change on its own (hot reload,
  a saved file). The left column lists the pages: the one shown and every
  page with a snapshot, a comment, or a mock, with the widths of its
  snapshots and comments and the number of its comments; click a page or a
  width to go there. On a narrow screen the two are shown one at a
  time, the page list is a drawer, and the conversation is a sheet.
- **Code**: the same diff as `--worktree`. Outside a git repository there is
  no code view; the page says why.

What the page is compared with is chosen per page (path and query):

- A **snapshot** is a script-free copy of the page's DOM and CSS, including
  shadow DOM, canvas contents, and form values, shown in a sandboxed frame
  that runs no script. kemi takes one when the review starts (or when the
  page first loads), one each time you hand to the agent, and one whenever
  you press **Record now**; one over 2 MB is not taken, and the page says so.
  By default the page is compared with the snapshot of the same page and
  width taken at the last hand-over, then at the start, then the last one
  recorded by hand. You can pick another point; it stays picked when the
  width changes, and a width it lacks says it has not been recorded.
- A **mock** is an HTML file in the served directory that you assign to the
  page by its path. A page with a mock is compared with it by default;
  removing the mock goes back to the snapshots. The mock runs its scripts in
  a sandboxed frame on an opaque origin, and kemi serves it and the files it
  references under a path holding a per-review secret instead of the review
  token, so the mock reaches neither the review page nor kemi's API. kemi
  reads the mock when it starts showing it (choosing it, moving to the page,
  changing the width or the way of comparing, or **Reload mock**), not when
  the file changes.

When the page is compared with a snapshot, kemi matches the two element by
element and lists the changes under the shown page in the page list, with
their count. Main changes come first: a change of look (color, font size,
corner radius, and the like, with the value before and after), of text, or
an element added or removed. Elements that only moved or resized are folded
below. An element inserted between siblings does not turn the siblings after
it into changes, and elements with an `id` are matched by it. Each change is
marked on the page: red for a main change, green for an added element, and a
light dashed outline for one that only moved; a removed element is marked on
the snapshot. The list and the marks follow the page as it changes. A page
compared with a mock gets neither.

You can comment on the page itself. The tools **Element**, **Arrow**, and
**Pen** in the band put places on the running page (never on what it is
compared with; while overlaid, a click reaches the page underneath), and
**Interact** uses the page as usual. An element place is the element you
click, and clicking it again takes it away; an arrow names the element at its
head; a pen line names up to five elements it encloses, largest overlap
first, leaving out elements that contain the whole line (a line drawn inside
one element names that element). Places are numbered from 1 on the page and in the comment box below
it, so the comment can refer to them; removing one, undoing the last, or
pressing Esc while drawing never renumbers the rest. Saving also makes a PNG
of the area with the places drawn on it, inside the page and without driving
the browser, so fonts and some CSS details may differ from the real page. A
saved comment keeps its places: the conversation panel shows its URL, width,
and number of places, the page shows its places as quiet numbered marks at
that URL and width (stronger while its thread is open), and a comment left
at another width shows that width and switches to it when pressed. Only its
text can be edited. On a narrow screen, places are drawn with a finger and
elements chosen by tapping. The agent gets the places and the image path
through `kemi wait`; see `page` in the [submit contract](#submit-contract).

**Side by side** puts the two next to each other. **Overlay** lays what the
page is compared with over the page at the same width, with an opacity
slider; it follows the page's scroll and lets clicks through to the page
underneath. `--live` takes only `--port`, `--bind`, `--no-open`,
`--live-port`, `--focus`, and `--serve`. The review title is
`Live review of <url>`; for a file the URL is its path from the served
directory, such as `/docs/mock.html`.

### Talking with an agent before the submit

When kemi keeps a session for the review, stderr also prints
`kemi: review <id>` after the URL (and after the line naming where results are
saved). An agent can then hold a conversation with you while the review is
open:

- `kemi wait <id> [--timeout <seconds>]` waits until you press **Hand to
  agent** (or submit) and prints everything handed over since its previous
  call as one JSON document: `{"kemi": 2, "review": "<id>", "events": [...]}`.
  A `handed` event lists comments as `added`, `edited`, or `deleted` (id
  only), new replies, and new messages; a `submitted` event carries the
  result JSON. Exit codes: 0 (events; an approval if submitted), 1 (submitted
  with changes requested), 2 (no running review — read a submitted one with
  `kemi --result` — or the review stopped, or another `kemi wait` is
  waiting), 3 (`--timeout` passed, stdout empty).
- `kemi reply <id>` reads `{"writes": [{"type": "reply", "comment_id": "c1",
  "body": "…"}, {"type": "message", "body": "…"}]}` from stdin and prints
  `{"ids": [...]}`. Either all writes are stored or none: a missing comment,
  a body over 64 KB, more than 50 agent replies on one comment, or more than
  200 agent messages exits with 2. Agents cannot open or resolve comments.

What you write is not sent as you type: **Hand to agent** delivers every
change since the last hand-over at once. Threads and notes on the whole
review live in the conversation panel on the right of the page. It lists
notes and threads in one column, ordered by their latest write, and can be
narrowed to all, unresolved, or the file on screen; pressing a thread turns
the whole panel into that thread, where you reply, resolve, fold, edit,
delete, or jump to its line. In the diff each comment is a one-line chip with
the first line of its body and its reply count; pressing it opens the thread
in the panel. The panel starts folded to a thin rail and opens from the
comment count in the top bar; the page remembers whether it is open and how
wide it is (drag its left edge). A new agent reply marks its chip and the
thread as new, and the folded rail counts what is new; the panel never opens
by itself, and its list follows new writes only while you are at its bottom.
The panel, the agent status in its header (not connected, waiting, working,
not responding), replies, and resolving are there in every review; only the
**Hand to agent** button waits until `kemi wait` has been called once in that
review. The agent API listens on
`127.0.0.1` whatever `--bind` says, accepts only
requests without an `Origin` carrying its own token, and the token is kept in
`<id>.endpoint` next to the session (owner-only; on Windows under
`%LOCALAPPDATA%\kemi\sessions\`), never on the page or stderr. A
subcommand name comes first; open a manifest named `wait` as `kemi ./wait`.

### Rendered view

Markdown (`.md`, `.markdown`), CSV / TSV, and images also get a rendered view
next to the line diff. The file header has a **Rendered | Source** switch
(key `r`) for Markdown, CSV / TSV, and SVG; it starts on Source, except SVG,
which starts rendered. Other images have no switch and are always shown as
images: old and new side by side in two-column mode, stacked in one-column
mode, each with its byte count. A rename with identical bytes shows one image
marked "unchanged".

The rendered view shows the new document as a whole, without folding. Added
blocks get a green left bar; deleted blocks stay at their old position with a
red one; a rewritten paragraph or heading with the same structure is shown
once, with deleted words struck through and added words highlighted. Code
blocks use the file's highlighting setting; CSV / TSV rows are table rows with
the first row as the header. Hover a block and press `+` to comment on its
source lines: the comment is an ordinary line comment in the submitted JSON,
on the new side (with an optional suggestion) or on the old side for a deleted
block. `n` / `p` stop at changed blocks and comments.

Relative image paths in Markdown are read from the repository at the version
of that side (working tree or `HEAD`, index or `HEAD`, `to` or `from`, the
commit or its parent). A manifest or a resumed session only shows images that
are part of the review; anything else, a path outside the repository, a
symlink, or a file over 5 MB becomes a placeholder showing the path. Raw HTML
is escaped, only `http(s)`, `#`, and relative links are kept, and relative
links are shown as text with the path in their tooltip. A Markdown or CSV /
TSV file over 10,000 lines or 1 MB on one side, or a CSV / TSV with an
unbalanced quote, is not rendered: the switch is disabled and its tooltip says
why. An image over 5 MB on one side has no rendered view and shows only its
byte counts, like any other binary file; so does an untracked image over 1 MB,
whose content kemi does not read.

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
can read the diff and submit. With `--live`, they can also see the page under
development.

On a narrow screen (a viewport narrower than 720px, such as a phone) the page
rearranges itself: the file tree becomes a drawer opened from the top bar, the
diff is always one column with line wrapping on, and the display options
(wrap, important only, sort by change size, theme) sit behind the "..." button.
Comments are added by tapping line numbers: tap a line number to select the
line, tap another line number on the same side to extend the selection to a
range, and press the `+` that appears at the end of the selection to open the
comment editor. Tapping the same line again, or tapping the diff body anywhere
other than a line number, clears the selection; taps on the top bar or the
file header do not. While the editor is open the selection stays, and
cancelling the editor clears it. In the rendered view, tap a block to show its
`+`. The conversation panel opens as a full-screen sheet from the comment
count in the top bar, and always starts closed on a narrow screen; tapping a
chip opens its thread in the sheet, and jumping to the line from a thread
closes the sheet. "Comments" behind the "..." button hides the chips; the
comment count in the top bar and the coloured line beside the line numbers
still show where they are. Tap the title to see the subtitle and meta.
Resizing across 720px switches the layout in place and closes the sheet.

### Keys in the page

These are ignored while typing in a text field.

| Key | Action |
|---|---|
| `n` / `p` | next / previous change or comment, moving on to the next / previous file |
| `v` | mark the current file as seen, or unmark it |
| `j` / `k` | next / previous file |
| `g` / `G` | first / last file |
| `u` / `s` | one column (unified) / two columns (split) |
| `w` | toggle line wrapping |
| `r` | rendered view / source view (Markdown, CSV / TSV, SVG) |

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
Only a session whose copy completed within the size limit can be resumed; one
whose copy is unfinished or too large keeps the state alone and stays out of
the list and `--resume` (its id exits with 2), and a review with neither state
nor copy leaves no session. Permissions match result files (`0600`/`0700`;
unix only), and the newest 100 sessions or 500 MB are kept across all
workspaces.

```sh
kemi --resume          # in a terminal: choose from the list
kemi --resume <id>     # continue a known session, from any directory
```

- The diff is frozen: changing the working tree, or deleting the repository
  itself, does not change what the page shows. Origin notes read "Cannot
  determine" when the repository is gone.
- Seen marks, folding, comments, and resolutions come back. Live reload and
  its update badge are off, and the URL token is new.
- Resuming continues the same session; it does not create another one. A
  submit from a resumed review writes its result file for the workspace the
  review was started from, and returns the manifest `approval` unchanged.
- Without an id and without a terminal, kemi prints the resumable sessions as
  one tab-separated line each — id, last update, workspace, mode, seen/total —
  newest first, and exits with 0; with none it prints nothing and exits with 2.
- A `--live` review keeps no frozen copy. Resuming reads the working tree as
  it is now and keeps watching it. Snapshots and mock assignments are kept
  only while kemi runs; they do not come back on resume yet. It is kept only when it holds a comment, a
  reply, or a message; seen marks and folding alone are not kept. Its mode
  column reads `live <url>`.
- A resumed `--live` review connects to the same URL again and prints a new
  `kemi: live` line.
- `--resume` accepts only `--port`, `--bind`, `--no-open`, `--serve`, and
  `--live-port`.
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
  "kemi": 2,
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
      "suggestion": { "replacement": "the replacement text" }
    }
  ],
  "messages": [{ "id": "m1", "author": "reviewer", "body": "a note on the whole review" }]
}
```

- `kemi` is the contract version, `2`. Version `1` results (from kemi before
  replies and messages) are printed unchanged by `kemi --result`; do not read
  a version you do not know by guessing.
- `side` is `new` or `old`. File-wide comments are `new` with `start_line` and
  `end_line` `null`, and `quote` `[]`.
- `suggestion` is `null` when the comment carries no suggestion, and always
  `null` for old-side and file-wide comments. An empty replacement means the
  lines should be deleted.
- `outdated` is `true` when the file changed after the comment was written.
  kemi never moves a comment to a new line number.
- `replies` are in creation order. `author` is `reviewer` or `agent`;
  `variants`, `chosen`, and `applied` are `[]`, `null`, `null` outside a
  review of a running page.
- `page` is `null` except on a comment left on a running page (`--live`).
  Such a comment is in the group `page` ("Page"), has `path`, `side`,
  `start_line`, `end_line`, and `suggestion` `null`, `quote` `[]`, and
  `outdated` `false`, and its `page` is
  `{ "url": "/products?x=1", "width": 390, "places": [...], "image": null }`.
  Each place has a number `n` (in order, possibly with gaps), a `kind`
  (`element`, `arrow`, or `pen`), `points` (`[]` for an element), and
  `elements` with a `selector`, `text`, and `rect`, in CSS pixels of the
  page. For an element inside an open shadow root, `selector` is the host's
  selector and the selector inside the shadow root joined by ` >>> `, which
  is not CSS: query the host, then its `shadowRoot` with the inner part.
  `image`, the PNG of the area with the places drawn on it, is an
  absolute path only in what `kemi wait` returns, and `null` in the result.
- `messages` are notes on the whole review, in creation order.
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
