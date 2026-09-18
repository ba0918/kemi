---
name: kemi
description: "Present changes or drafts for human review in kemi and collect the submitted verdict and comments. Use when handing changes to the user for review or approval."
license: MIT OR Apache-2.0
---

# kemi

kemi serves a review page, by default on `127.0.0.1` only. `--bind` can expose it to the
LAN, and kemi then warns that the URL and the API are plain HTTP. The user can comment on
changes and submit **Approve** or **Request changes**. It then prints one result JSON
document to stdout and exits. It does not apply suggestions or edit the reviewed files.

Complete the review handoff by collecting the exit code and result, reading the comments,
and acting or reporting within the user's requested scope. Sharing the URL is an intermediate
step. If the run fails or is interrupted, report that outcome instead of a verdict.
Follow the environment's presentation conventions when provided.

## Choose the input

Use one input mode per run:

| Review target | Command |
|---|---|
| Commits since a base | `kemi --from <base> [--to <head>]` |
| Working tree, including untracked files | `kemi --worktree` |
| Staged changes | `kemi --staged` |
| Drafts or groups you define | `kemi <manifest.json>` or `kemi -` for stdin |

Read [the input reference](references/inputs.md) when writing a manifest, choosing commit
presentation, adding focus marks, or surveying a large change with `--digest`. It also covers
`--no-open`, `--port`, `--bind`, and `--base`. Choose a review scope that leaves out already
reviewed work.
For approval, identify the exact bytes being approved; the manifest's `approval` field is
returned unchanged, not checked by kemi.

## Run and wait

Start `kemi` with stdout in its own file and stderr readable separately, choosing an execution
method that keeps the process alive until the user submits the review. Retain the run's
identifier and output paths. A tool that yields while the process keeps running is not the
process ending.

Read the `kemi: <url>` line from stderr and send the URL to the user in a progress message.
stderr also reports the result storage directory. Explain the groups, what needs judgment,
any assumptions, and that **Approve** or **Request changes** submits the review.

Wait for the same process to exit using the execution environment's normal process lifecycle.
Do not terminate or replace the run merely to check its status, and avoid rapid status checks.
Keep the turn active unless completion notifications are confirmed to resume you automatically;
otherwise retain the run's identifier and keep using the environment's wait and output
mechanisms.

After the process exits, collect its exit code and stdout. For a submitted result, use
[the result reference](references/results.md) to interpret the JSON and handle comments.

If the execution environment cannot preserve a long-running process and later collect its
result, report that limitation instead of inventing a verdict.

| Exit code | Outcome | What to read |
|---|---|---|
| `0` | Approved | Result JSON, including any comments |
| `1` | Changes requested | Result JSON; this is not an execution failure |
| `2` | Startup, usage, or runtime error | stderr; stdout is empty |
| `130` | Interrupted before submit | No result; stderr may carry the resume line |

Keep the process alive while the user reads. An interrupted review is not lost: the comments,
seen marks, folding, and resolutions are saved in a session, and stderr prints one line,
`kemi: resume with: kemi --resume <id>`, when the session can be reopened. An empty stdout
file while the process runs means no result has arrived yet.

If the user approves in the conversation instead of submitting on the page, record it as a
conversation decision. Stop only your own run by its retained identifier, never by process
name; that path yields no review JSON. The review stays as a session, so report the resume
command if the user may want to continue it.

## Resume an interrupted review

When a run ends without a submit and the user still wants that review, continue it instead
of starting over:

```text
kemi --resume <id>     # from any directory; the id comes from the resume line
kemi --resume          # in a terminal: choose from the list of sessions
```

Resuming shows the diff as it was when the review started, with the seen marks, folding,
comments, and resolutions restored. The original input is not read again, so editing the
working tree or moving away from the repository does not change the page; only origin notes
read "Cannot determine" if the repository is gone. Live reload and its update badge are off.
Resuming continues the same session rather than creating one, and a submit that follows
writes its result for the workspace the review was started from, with the manifest
`approval` unchanged.

Without an id and without a terminal, `kemi --resume` prints the resumable sessions as one
tab-separated line each — id, last update, workspace, mode, seen/total, newest first — and
exits `0`; with none it exits `2`. `--resume` takes only `--port`, `--bind`, `--no-open`,
and `--serve`; pairing it with an input mode or `--digest` is a usage error (exit `2`), and
`--digest` / `--result` never create sessions.

## Handle the result

Check the JSON contract version (`kemi: 1`) and that `verdict` agrees with the exit code.
Read comments on both approval and requests for changes. Before using an approval, compare
its identities with the current bytes. Before applying a suggestion, compare its `quote`
with the file; locate outdated comments by their text rather than trusting old line numbers.
Use the result reference for field details and `kemi --result` recovery if stdout was lost.
Recovery retrieves a stored result; it does not wait for the current review to finish.
