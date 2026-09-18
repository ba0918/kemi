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

1. Start kemi with stdout in its own file and stderr readable separately. Retain the execution
   identifier and output paths. Choose an execution method that keeps kemi alive throughout
   the review; a tool yielding while its process runs is safe, a timeout killing it is not.
2. Read the `kemi: <url>` line from stderr and send the URL in a progress message. Explain the
   groups, what needs judgment, any assumptions, and that **Approve** or **Request changes**
   submits the review. stderr also reports the result storage directory.
3. Wait for that run to exit, using the execution tool's wait/output mechanism as needed.
   Keep the turn active unless completion notifications are confirmed to resume you
   automatically. A shell background process alone provides no such guarantee. Use bounded
   waits that allow progress updates, without rapid status checks or work assuming a verdict.
4. Read the exit code and stdout file. For a submitted result, use
   [the result reference](references/results.md) to interpret the JSON and handle comments.

| Exit code | Outcome | What to read |
|---|---|---|
| `0` | Approved | Result JSON, including any comments |
| `1` | Changes requested | Result JSON; this is not an execution failure |
| `2` | Startup, usage, or runtime error | stderr; stdout is empty |
| `130` | Interrupted before submit | No result |

Keep the process alive while the user reads. Unsubmitted comments exist only in memory and
are lost on interruption; explain the loss and offer a fresh review if that happens. An empty
stdout file while the process runs means no result has arrived yet.

If the user approves in the conversation instead of submitting on the page, record it as a
conversation decision. Stop only your own run by its retained identifier, never by process
name; that path yields no review JSON and loses unsubmitted page comments.

## Handle the result

Check the JSON contract version (`kemi: 1`) and that `verdict` agrees with the exit code.
Read comments on both approval and requests for changes. Before using an approval, compare
its identities with the current bytes. Before applying a suggestion, compare its `quote`
with the file; locate outdated comments by their text rather than trusting old line numbers.
Use the result reference for field details and `kemi --result` recovery if stdout was lost.
Recovery retrieves a stored result; it does not wait for the current review to finish.

## Notes for specific agents

Use the note matching the tools available in your environment.

**Claude Code.** Use Bash with `run_in_background: true`. Read the URL from its recorded output.
If completion notifications resume you, end the turn and collect the exit code and stdout
file on notification; otherwise use the available task-wait mechanism.

**Codex.** With `exec_command` and `write_stdin`, run kemi without shell backgrounding (`&`).
Use `yield_time_ms` to yield while it stays alive, retain `session_id`, and call `write_stdin`
with that id and empty input until exit. Share the URL through commentary; send the final
answer after collecting the result. If an outer tool also yields, resume it using its own
identifier first to obtain the execution result.

**OpenCode.** Use a returned task/session identifier with its corresponding wait/output tool
when available. With only a timeout-limited shell, background kemi with separate output files
and save its exit code when it ends. Keep the turn active with bounded waits between checking
that completion record; the launch shell's exit code is not kemi's verdict. If the environment
cannot preserve the process and let you collect its result, explain that limitation.
