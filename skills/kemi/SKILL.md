---
name: kemi
description: "Present changes or drafts for human review in kemi and collect the submitted verdict and comments. Use when handing changes to the user for review or approval."
license: MIT OR Apache-2.0
---

# kemi

kemi serves a review page where the person comments and submits **Approve** or **Request
changes**. It prints one result JSON to stdout and exits; it never applies suggestions or
edits reviewed files. Sharing the URL is an intermediate step: collect the exit code and
result, read the comments, then act or report within the requested scope. Failure or
interruption is not a verdict. Follow the environment's presentation conventions.

## Choose the input

Use one input mode per run and leave out already reviewed work:

| Review target | Command |
|---|---|
| Commits since a base | `kemi --from <base> [--to <head>]` |
| Working tree, including untracked files | `kemi --worktree` |
| Staged changes | `kemi --staged` |
| Running loopback page or local HTML, with the working tree | `kemi --live <url\|file>` |
| Drafts or groups you define | `kemi <manifest.json>` or `kemi -` for stdin |

Read [inputs](references/inputs.md) for manifests, commit grouping, focus marks, `--live`,
`--digest`, or optional flags. Use `--no-open` when a browser should not appear on the
person's machine. The default bind is `127.0.0.1`; use `--bind` only for intended network
exposure (the URL and API are plain HTTP). For approval, identify the exact bytes; a
manifest's `approval` is returned unchanged, not checked by kemi.

## Prepare the explanation before handing off

For a human judgment or approval handoff, save an explanation in the review before sharing
its URL. State what changed and why, what the person should judge, and unverified points or
caveats. Separate independently judgeable points into commentable blocks; one short text
block is enough for a small change. Use typed Code references where they help the person
inspect evidence, not hashes as navigation. A diagram is optional, not a required chapter.
This requirement does not apply to self-inspection, `--digest`, or stored-result retrieval.

Read [explanation authoring](references/explanation.md#adding-an-explanation) before a new
handoff: write a separate JSON file with only `sections`, then pass `--explanation <file>`
with the input mode. Startup validates and saves it before publishing the review; failure
exits `2` before any URL. Do not fetch context for a review that has not started.
For an already running or resumed review, read [the update procedure](references/explanation.md#updating-the-explanation)
instead; `--resume` cannot take `--explanation`.

## Run and wait

Start kemi with stdout in its own file and stderr readable separately. Use an execution
method that keeps the main process alive until submit, and retain its identifier and output
paths. A tool yielding while the process runs is not the process ending; empty stdout means
no result yet. If the environment cannot preserve the process and collect its result, report
that limitation rather than inventing a verdict.

Read `kemi: <url>` from stderr. Share it in a progress message only after successful startup
with `--explanation`, or after an explanation update printed success JSON and exited `0`.
Fix or report failures before handoff. Tell the person what needs judgment and that **Approve**
or **Request changes** submits the review. stderr also reports the result storage directory
and, when conversation is available, `kemi: review <id>`.

Keep waiting for that same main process through the environment's normal wait/output
mechanisms; do not terminate or replace it to check status, or check rapidly. Keep the turn
active unless completion notifications are confirmed to resume you automatically.

| Exit code | Outcome | Read |
|---|---|---|
| `0` | Approved | Result JSON, including comments |
| `1` | Changes requested, not execution failure | Result JSON |
| `2` | Startup, usage, or runtime error | stderr; stdout is empty |
| `130` | Interrupted before submit | No result; stderr may give a resume command |

If the user approves in chat rather than submitting, record a conversation decision, not a
kemi verdict. Stop only your own run by its retained identifier, never by process name;
that path yields no review JSON. Report any printed resume command if continuation matters.

## Talk with the person before the submit

When stderr prints `kemi: review <id>`, read [conversation](references/conversation.md) and
run `kemi wait <id>` in the background. Read handed comments, replies, and notes; address
them within scope, answer with `kemi reply <id>` (JSON on stdin), then wait again until submit.
This loop is independent of waiting for the main process: its exit code and stdout remain
the review result. Without the review line or a call to wait, the review is submit-only.

Reply to ordinary questions. When the explanation's account, judgment points, or caveats
become stale, read [explanation updates](references/explanation.md#updating-the-explanation)
and replace the document to match what the review actually shows, not merely newer files.
Confirm success JSON and exit `0` before claiming it saved. Explain does not reply to or
resolve a thread; send a reply separately when needed. Readers keep their open explanation
until opening the notice, and displayed Code does not auto-refresh with an explanation update.

## Resume an interrupted review

When the user still wants an interrupted review, continue the same id with `kemi --resume <id>`
from stderr's resume line, rather than rebuilding it. Read [resume](references/resume.md)
for eligibility, chooser/flag rules, and restored versions. Non-live resume shows the frozen
startup diff; `--live` is the exception and rereads the working tree and reconnects the page.

## Handle the result

After the main process exits, collect its exit code and stdout and read
[results](references/results.md) for field interpretation or `kemi --result` recovery.
Check the contract version (`kemi: 3` for new result/wait output) and that `verdict` matches
the exit code. Do not guess unknown versions; stored version 1 or 2 results are returned
unchanged. Recovery retrieves a stored result, not the current review's eventual submit.

Read comments on approvals as well as change requests. Compare approval identities with
current bytes before using approval, and suggestion `quote` with the file before applying
it. Locate outdated comments by their text rather than old line numbers. Keep typed `parts`
intact. Explanation comments quote a complete original block/revision; `selection` is only
a location hint. A display version identifies metadata, not fixed bytes or correctness.
For `--live` page comments, read the numbered places and their image from wait before submit;
the result does not retain that image path.
