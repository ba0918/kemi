# kemi

English | [日本語](README-ja.md)

kemi lets you review changes in a browser and return comments to the terminal
as JSON. You can comment on code, documents, and a running web page. An AI agent
can read your feedback and reply while the review stays open.

kemi never edits your files or applies suggestions. It runs as a single binary,
with no browser extension. The name comes from the Japanese verb *kemi suru*,
meaning "to review".

## Install

On Linux or macOS, install with [mise](https://mise.jdx.dev/):

```sh
mise use -g github:ba0918/kemi
```

On Windows, download the ZIP for your machine from
[GitHub Releases](https://github.com/ba0918/kemi/releases/latest), extract
`kemi.exe`, and add its directory to `PATH`.

Releases support x86_64 and ARM64 on Linux, macOS, and Windows. Linux binaries
use musl and are statically linked.
For repository reviews, Git must be on `PATH` on every platform.

## Review your changes

Run this from the root of a Git repository to review uncommitted changes, including
untracked files:

```sh
kemi --worktree
```

1. Read the changes in the browser that opens. If it does not open, use the
   review URL printed in the terminal.
2. Select lines and add comments or suggested replacements.
3. Press **Approve** or **Request changes** to finish.

kemi prints one JSON result to stdout and exits. To save it, keep the output
file outside the repository so it does not become part of the review:

```sh
kemi --worktree > ../review.json
```

| Exit code | Meaning |
|---|---|
| `0` | Approved |
| `1` | Changes requested |
| `2` | Usage, startup, or runtime error |
| `130` | Interrupted before submission |

The result includes your decision, comments, suggestions, and conversation.
See the [result reference](skills/kemi/references/results.md) to process the JSON.

## Choose what to review

| What you want to review | Command |
|---|---|
| Your branch compared with `main` | `kemi --from main` |
| Uncommitted changes, including untracked files | `kemi --worktree` |
| Staged changes | `kemi --staged` |
| Files or drafts grouped in a JSON manifest | `kemi manifest.json` |
| A manifest supplied through stdin | `kemi -` |

A commit-range review combines the changes into one diff and lists each path
once. To read each non-merge commit separately, use:

```sh
kemi --from main --group-by commit
```

You can switch between the two groupings in the page without restarting.
Use `--to <ref>` to end the range somewhere other than `HEAD`.

For Markdown, CSV, TSV, and SVG, you can switch between rendered content and
source. Other images display directly.

Use `--explanation <file>` to include an explanation document. It opens before
the diff. You can comment on the explanation and follow its links to code or
live pages.

For a large change, `kemi --from main --digest` prints a summary without line
contents and exits without opening a review.

## Work with an AI agent

Install the skill that teaches an agent to start reviews and read the results:

```sh
gh skill install ba0918/kemi kemi
```

Ask the agent to explain the changes and the decisions you need to make before
it shares the review URL. The agent must keep kemi running until you submit.
Install the skill again to get the version from the newest tagged release.

You can discuss the change before submitting. The agent uses the review ID
printed by kemi to wait for feedback and send replies:

```sh
kemi wait <id>
kemi reply <id> < writes.json
```

Write comments in the page, then press **Hand to agent** to send them together.
The button becomes available once the agent has called `kemi wait`. Replies
appear in the conversation panel. The review stays open until you press
**Approve** or **Request changes**.

See the [conversation reference](skills/kemi/references/conversation.md) for
the reply format, timeouts, and agent exit codes.

## Review a running page

Start your development server, then run:

```sh
kemi --live http://localhost:5173/
```

You can also review a local HTML file:

```sh
kemi --live docs/mock.html
```

The **Live page** tab lets you inspect the page at different widths. Compare
it with a saved snapshot or an HTML mock. Use Element, Arrow, or Pen to mark
places and comment on them.

The **Code changes** tab shows the working-tree diff when you start inside a
Git repository. On a phone, the tabs are labelled **Page** and **Code**.

The URL must use HTTP on `localhost` or an IPv4 loopback address such as
`127.0.0.1`. A local HTML file must be inside the repository. Outside a
repository, it must be inside the directory where you start kemi.
Share the review URL, not the separate `kemi: live` URL.

If kemi cannot embed resources needed for a comment image, it saves the comment
and its places without an image and shows the reason.
See [live inputs](skills/kemi/references/inputs.md#reviewing-a-running-page)
for allowed options and comparison behaviour.

## Resume a review or read its result

After interrupting a review with Ctrl+C, use:

```sh
kemi --resume          # choose a saved review in a terminal
kemi --resume <id>     # resume a known review from any directory
```

Code and manifest reviews restore the original diff, even if the files have
changed. They can resume only if kemi finished saving the review copy within
the size limit. A live review reads the current working tree again.
See the [resume reference](skills/kemi/references/resume.md) for storage limits
and what is restored.

Submitted results are saved separately. To retrieve the latest result for the
current repository:

```sh
kemi --result
```

## Share with another device

By default, only your own machine can reach the review. To listen on all IPv4
interfaces with a fixed port:

```sh
kemi --worktree --bind 0.0.0.0 --port 3000
```

Use the network URL printed by kemi. Anyone who can reach that port and knows
the URL can read the changes and submit a review. The connection uses plain
HTTP. With `--live`, it also exposes the page under development.

Your firewall or network may block the port. WSL2 may also need Windows-side
firewall and forwarding settings. kemi does not change those settings.

## Reference

The English references below accompany the agent skill. The specifications
in `docs/spec/` are the authoritative contracts and are written in Japanese.

| Topic | Read next |
|---|---|
| Input modes, flags, manifests, focus files, and digest | [Input reference](skills/kemi/references/inputs.md) |
| Agent replies and feedback delivery | [Conversation reference](skills/kemi/references/conversation.md) |
| Explanation JSON, references, and updates | [Explanation reference](skills/kemi/references/explanation.md) |
| Result JSON and saved results | [Result reference](skills/kemi/references/results.md) |
| Interrupted reviews and saved sessions | [Resume reference](skills/kemi/references/resume.md) |
| Detailed page controls, rendered files, and keyboard shortcuts | [Reading](docs/spec/reading.md), [rendering](docs/spec/render.md), and [narrow screens](docs/spec/narrow.md) |
| Live-page comparisons and comments | [Live review](docs/spec/live.md) and [comparison](docs/spec/live-compare.md) |
| All specifications and UI references | [Documentation index](docs/README.md) |
| Changes between releases | [Changelog](CHANGELOG.md) |

Run `kemi --help` for command-line usage.

## Development

The UI uses native JavaScript modules and CSS, with no frontend build step.
UI and CLI text are in English. Specifications are in Japanese.

```sh
cargo build --release
cargo test --workspace --locked
node --test web
npx tsc -p web --noEmit
```

See [PROJECT.md](PROJECT.md) for the repository layout and other checks.

## License

MIT OR Apache-2.0. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
