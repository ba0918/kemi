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

The review has two views, switched by the two tabs at the left of the top
bar:

- **Live page**: the running page at a width of 390, 768, 1280, or any width
  from 320 to 3840 (type it and press Enter or leave the field; the field
  keeps showing a width that is not one of the presets). The page may change
  on its own (hot reload, a saved file), and the reload button next to the
  page loads it again by hand. **Fit** shrinks the page to the frame;
  **100%** shows it unscaled and scrolls it inside the frame. The scale is
  always shown above the page. While this view is shown, the top bar leaves
  out the code controls and counts (display mode, wrap, focused only, sort,
  files, changes, seen) and shows that this is a live review, the page's URL
  and width, and the agent's state instead. The left column lists the pages:
  the one shown and every page with a snapshot, a comment, or a mock, with
  the widths of its snapshots and comments and the number of its comments;
  click a page or a width to go there.
- **Code changes**: the same diff as `--worktree`, with the number of
  changed files on the tab and the top bar as in other reviews. Outside a
  git repository there is no code view; the page says why. The update badge
  shows in both views; pressing it in the page view switches to the code
  view and reloads it.

On a narrow screen the tabs are shortened to **Page** and **Code** in the
first row of the top bar, whose **…** menu then holds only the theme. The
controls of the page view fit in one row: Now / Before (the running page or
what it is compared with, one at a time), the page, its width, the agent's
state, and a **…** menu with what the page is compared with, **Record now**,
Fit / 100%, the width, and **Mock file…**. Overlay is not offered there. The tools
(Element, Arrow, Pen, Interact) and **Hand to agent** float together in one
toolbar at the bottom of the screen instead of sitting above the page. The page
list is a drawer and the conversation is a sheet.

What the page is compared with is chosen per page (path and query):

- A **snapshot** is a script-free copy of the page's DOM and CSS, including
  shadow DOM, canvas contents, and form values, shown in a sandboxed frame
  that runs no script. kemi takes one when the review starts (or when the
  page first loads), one each time you hand to the agent, and one whenever
  you press **Record now**; one over 2 MB is not taken, and the page says so.
  By default the page is compared with the snapshot of the same page and
  width taken at the last hand-over, then at the start, then the last one
  recorded by hand. You can pick another point; it stays picked when the
  width changes, and a width it lacks says it has not been recorded. The
  choices are listed as Auto, then the hand-overs, the start, the recorded
  ones, the assigned mock, and last **Mock file…**; Auto names the snapshot it picks for the page and
  width shown, and a note under it says how it picks. A snapshot taken with
  **Record now** becomes what the page is compared with, as if you had
  picked it, and the band says so; while the page is shown alone, the notice
  offers **Compare →** to put it side by side (on a narrow screen, to show
  it). Handing to the agent does not move the comparison away from it;
  reloading the review page goes back to Auto.
- A **mock** is an HTML file in the served directory that you assign to the
  page. **Mock file…** opens a small panel that says what a mock is and
  lists the `.html` and `.htm` files in the served directory (in a git
  working tree, the tracked ones and the untracked ones git does not ignore;
  never anything inside `.git/` or a symlink pointing outside), with a search
  box. It shows up to 200 files, with how many it shows out of how many
  match; search to find the rest. A file not listed, such as one git
  ignores, can be assigned by typing its path into the panel. A path outside
  the directory or a file that is not HTML is refused with the reason.
  While a mock is assigned, the choice shows its path, and the **…** next to
  it holds **Reload mock** and **Remove mock**; removing it says so in the
  band with **Undo**, which assigns the same file again (assigning another
  file in the meantime drops the **Undo**; if the file can no longer be
  assigned, the band says why). A page with a mock
  is compared with it by default; removing the mock goes back to the
  snapshots. The mock runs its scripts in
  a sandboxed frame on an opaque origin, and kemi serves it and the files it
  references under a path holding a per-review secret instead of the review
  token, so the mock reaches neither the review page nor kemi's API. kemi
  reads the mock when it starts showing it (choosing it, moving to the page,
  changing the width or the way of comparing, or **Reload mock**), not when
  the file changes. If the file cannot be read then, the comparison area
  says so instead of showing it.

When the page is compared with a snapshot, kemi matches the two element by
element and lists the changes under the shown page in the page list, one row
per element, with the number of elements and the name of what the page is
compared with (the snapshot Auto picked, when Auto is chosen), whatever the
way of comparing. A row names the element by its tag and text, gives a
selector-like hint of where it is, and lists what changed in it: its look
(color, font size, corner radius, and the like; colors before and after as
swatches, other values as text), its text, or that it was added or removed.
Main changes come first. Elements that only moved or resized, with nothing
else changed, are folded below with a line saying so; `html` and `body` are
never listed there, though a change of their look or text is a main change.
An element inserted between siblings does not turn the siblings after it into
changes, and elements with an `id` are matched by it. Each change is marked on
the page: red for a main change, green for an added element, and a light
dashed outline for one that only moved; a removed element is marked on the
snapshot. A legend under the list says what the colors mean. Clicking a row
scrolls the page to that element and makes its mark flash (at 100%, the frame
also scrolls sideways to it). A removed element is always listed; clicking its
row shows the snapshot side by side (on a narrow screen, switches to it), moves
the snapshot to the element and makes it flash. As the snapshot runs no script
and cannot be scrolled from outside, it is then drawn at its full height and
moved within its frame; the wheel over it still moves it, and it goes back to
scrolling by itself once what it is compared with, the way of comparing, the
page, or the width changes. While overlaid, the page itself scrolls to the
element, and the snapshot laid over it follows. The list and the marks follow the page as it
changes. A page compared with a mock gets neither: in place of the list,
its heading names the mock and has the same **…** menu.

You can comment on the page itself. The tools **Element**, **Arrow**, and
**Pen** in the toolbar right above the page put places on the running page
(never on what it is compared with; while overlaid, a click reaches the page
underneath), and **Interact** uses the page as usual. **Element** is chosen
when the review opens, and choosing a tool does not open the comment box:
it opens with the first place. Until the first page comment of the review is
saved, three steps are shown above the page — point at a place, write and
press Comment, hand to the agent; they do not come back after that, even
when the comments are deleted or the review is suspended and resumed. An element place is the element you
click, and clicking it again takes it away (clicking the empty background,
where only `html` or `body` is, puts no place); an arrow names the element at its
head; a pen line names up to five elements that overlap the area it
encloses, even partly, largest overlap first, leaving out elements that contain the whole line (a line drawn inside
one element names that element). `html` and `body` are never named: an arrow
pointing at the background, or a line around nothing but background, is an
area only, shown as such in the comment box, and reaches the agent with
`elements` `[]`, its points, and the comment's image. Places are numbered from 1 in the order they
were put, on the page and in the comment box below it; the text refers to a
place as `#n`, and clicking a place number in the box inserts `#n` where you
are typing (a number without `#` is not a reference). Pointing at a place in
the box lights it up on the page, and clicking it scrolls the page to it and
makes it flash. Removing a place —
from the list, by undoing the last, or by choosing the same element again —
renumbers the rest from 1 and rewrites the `#n` in the text to match. A `#n`
that pointed at the removed place, or a number beyond the places, is shown
in the box, and the comment cannot be saved until the text is changed. The
places of a comment being written belong to the URL and width of its first
place: at another URL or width no place is added, and the comment box says so
and offers to go back there, where the comment is saved. Saving also makes a PNG
of the area with the places drawn on it, inside the page and without driving
the browser, so fonts and some CSS details may differ from the real page. A
saved comment keeps its places: the conversation panel shows its URL, width,
and number of places, the page shows its places at that URL and width as
small marks without numbers — pointing at one, with any tool, shows which
comment it belongs to — and numbered only while its thread is open, so they
are not mistaken for the numbered places of the comment being written; a comment left
at another width shows that width and switches to it when pressed. **Show on
page** in the thread scrolls the page to the comment's places and makes them
flash, once the page at its URL and width has loaded. Only its
text can be edited. On a narrow screen, places are drawn with a finger and
elements chosen by tapping. The agent gets the places and the image path
through `kemi wait`; see `page` in the [submit contract](#submit-contract).

The page is shown alone when the review opens. The icons above it switch to
**Side by side**, which puts what it is compared with next to it with their
tops level, or **Overlay**, which lays it over the page at the same width,
with an opacity slider; it follows the page's scroll and lets clicks through
to the page underneath, and the heading names both and the opacity, or which
one is visible at either end. The choice of what to compare with is shown
only while comparing, but the changes and the marks on the page show while
the page is alone too. **Record now** stays in the band whatever the way of
comparing. The way of comparing, Fit / 100%, and what is compared
with are kept while the review page is open and reset when it is reloaded. `--live` takes only `--port`, `--bind`, `--no-open`,
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
thread as new, and the folded rail counts what is new, as does the panel's
header while it is open; while you read one thread, a reply arriving in
another is announced above it, and pressing the notice opens that thread.
The panel never opens by itself, and its list follows new writes only while
you are at its bottom.
The panel, the agent status in its header, replies, and resolving are there
in every review. The status is one of not connected (`kemi wait` never
called), ready (`kemi wait` is waiting, so you can hand), working (it
returned and you wait for the agent), replied (the agent has answered every
thread the last returned `kemi wait` carried; it stays so however long it
takes you; if you delete a thread the agent was still to answer, it gets
there once no other carried thread is left unanswered and the agent has
written something since that return), and not responding (working for 10 minutes with neither `kemi
wait` nor `kemi reply`); the page puts it in `data-kemi-agent-state` as
`not-connected`, `waiting`, `working`, `replied`, or `no-response`. After a
hand-over, each thread it carried ends with a line saying the agent has yet
to pick it up, which turns into "working" when `kemi wait` returns it and
goes away when the agent replies in that thread (a hand-over of messages
only puts one line at the end of the list, gone at the agent's next
message); the line carries `data-kemi-hand-line="pending"` or `"working"`.
**Hand to agent** is always shown, also on the folded rail, but cannot be
pressed until `kemi wait` has been called once in that review: next to it the
page says that comments can be handed once the agent runs `kemi wait <id>`,
with that command (this review's id) to copy, also on the folded rail. It
also cannot be pressed, and
says so, while there is nothing new to hand. In a review the agent cannot
connect to (no session, so no `kemi: review` line), it stays unpressable
and says that no agent can connect. The agent API listens on
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
count in the top bar, and always starts closed on a narrow screen; **Hand to
agent** floats at the bottom of the screen in every mode, so you can hand
without opening the sheet (it can be pressed exactly when the one in the
panel can); tapping a
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
  it is now and keeps watching it. Its snapshots, including the one taken at
  the start, and its mock assignments come back, so the default comparison
  stays the same; a mock whose file is gone by then says it cannot be read.
  It is kept only when it holds a comment, a reply, or a message; seen marks,
  folding, snapshots, and mock assignments alone are not kept. Its mode
  column reads `live <url>`.
- A `--live` session keeps its snapshots and comment images in `<id>.files/`
  next to it, up to 20 MB per session. When a new one would go over, kemi
  removes older hand-over snapshots first, then older recorded ones, then
  images of deleted comments; it never removes the start snapshot or an
  image of a current comment. A removed snapshot also leaves the choices of
  what to compare with. What still does not fit is not saved: such a snapshot
  stays usable until the review ends and is marked as not saved (it is gone
  after resuming), and a comment is saved without its image, with a notice.
  `<id>.files/` counts toward the 500 MB kept across sessions.
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
  Each place has a number `n` (in order, running from 1 without gaps; the
  body refers to a place as `#n`), a `kind`
  (`element`, `arrow`, or `pen`), `points` (`[]` for an element), and
  `elements` with a `selector`, `text`, and `rect`, in CSS pixels of the
  page (`[]` for an arrow or pen on the background: an area only; `html`
  and `body` are never named). For an element inside an open shadow root, `selector` is the host's
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
scripts/gen-fixture.sh <dir> --files N --lines M [--commits K] [--ignored-dirs D] [--changed C]
scripts/measure-startup.sh <fixture> target/release/kemi
scripts/measure-range.sh <fixture> target/release/kemi <commit|file|busy>
```

The specification is `docs/spec/kemi.md` (Japanese). The UI and docs are
Japanese; this README is English.

## License

MIT OR Apache-2.0. See `LICENSE-MIT` and `LICENSE-APACHE`.
