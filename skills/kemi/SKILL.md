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

Read [inputs](references/inputs.md) for manifests, commit grouping, focus marks, `--digest`, or
optional flags, and [live review](references/live.md) for `--live` and its page comments. Use
`--no-open` when a browser should not appear on the person's machine. The default bind is
`127.0.0.1`; use `--bind` only for intended network exposure (the URL and API are plain HTTP).
For approval, name the exact bytes in the progress message that shares the URL; that message
is your record. A manifest also carries them in `approval`, returned unchanged and not checked
by kemi. Git inputs return `approval: []`: name the full base and head SHAs of a commit range,
or each reviewed file's `git hash-object <path>` (`git rev-parse :<path>` when staged; a
deleted file as deleted).

## Prepare the explanation before handing off

For a human judgment or approval handoff, save an explanation in the review before sharing
its URL. The explanation is where the person reads the change; the diff, specification, or
plan is the evidence behind it. Reviews are rare and decide what gets built, so write for a
reader who has seen neither the session nor the diff: what behaves differently and how it
works, why this way rather than the alternatives, what they must decide and what each option
changes, what was decided without them, and what is unverified or could go wrong. Show
mechanisms with diagrams, pseudocode, before-and-after examples, and tables, and define terms
coined during the work. Length follows what there is to explain and what the change decides,
not the size of the diff: one block can carry a one-line fix, while a specification or plan
needs the full treatment. Separate independently judgeable points into commentable blocks. Use
typed Code references where they help the person inspect evidence, not hashes as navigation.
This requirement does not apply to self-inspection, `--digest`, or stored-result retrieval.

The person will not read the diff to catch what the explanation gets wrong, so check it before
sharing the URL: a reader who has seen neither the session nor the diff restates the change
from the explanation alone, and only then compares it with the reviewed diff. Fix every
misreading and every mismatch, and report both results, or what went unchecked, in the
message that shares the URL.

Before a new handoff, read [writing explanations](references/explanation-writing.md) for the
content, tested diagram templates, and the evidence to show at handoff, including how to run
this check, and [explanation authoring](references/explanation.md#adding-an-explanation) for
the format: write a separate JSON file with only `sections`, then pass `--explanation <file>`
with the input mode. Startup validates and saves it before publishing the review; failure
exits `2` before any URL. Do not fetch context for a review that has not started.
For an already running or resumed review, read [the update procedure](references/explanation-update.md)
instead, which also covers reading a large context selectively; `--resume` cannot take
`--explanation`.

## Run and wait

Start kemi with stdout in its own file and stderr readable separately. Use an execution
method that keeps the main process alive until submit, and retain its identifier and output
paths. A tool yielding while the process runs is not the process ending; empty stdout means
no result yet. If the environment cannot preserve the process and collect its result, report
that limitation rather than inventing a verdict.

Read stderr without the `kemi: comment …` lines, one per comment, which are for a person
watching the terminal. Every other line may matter to you: the contract lines (`kemi: <url>`,
`kemi: review <id>`, `kemi: live <url>`, `kemi: resume with: …`), warnings such as a failed
save or exposure on the LAN, and, when kemi exits `2`, the error. Any filter that drops the
lines starting with `kemi: comment ` works. For example, in a POSIX shell (in PowerShell,
`Select-String -NotMatch` does the same):

```text
grep -v '^kemi: comment ' <stderr-file>
```

Read `kemi: <url>` from stderr. Share it in a progress message only after successful startup
with `--explanation`, or after an explanation update printed success JSON and exited `0`.
Fix or report failures before handoff. Tell the person what needs judgment and that **Approve**
or **Request changes** submits the review. When conversation is available, stderr also prints
`kemi: review <id>`.

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
Keep waiting for the main process too. When `kemi wait` returns `submitted`, its `result` is
the review result: take only the exit code from the main process, and read its stdout only in
the cases [the loop](references/conversation.md#the-loop) names. Without the review line or a
call to wait, the review is submit-only.

When the explanation's account, judgment points, or caveats become stale, read
[the update procedure](references/explanation-update.md). An update is not a reply: answer the
thread with `kemi reply` as well.

## Resume an interrupted review

When the user still wants an interrupted review, continue the same id with `kemi --resume <id>`
from stderr's resume line, rather than rebuilding it. Read [resume](references/resume.md)
for eligibility, chooser/flag rules, and restored versions. Non-live resume shows the frozen
startup diff; `--live` is the exception and rereads the working tree and reconnects the page.

## Handle the result

After the main process exits, collect its exit code and the result (its stdout, or the
`submitted` result from `kemi wait`), and read [results](references/results.md) before acting
on it: it covers the contract version, every field, and `kemi --result` recovery. Check that
`verdict` agrees with the exit code, and before using an approval, compare your latest record
with what you are about to act on, taken with the same commands.
