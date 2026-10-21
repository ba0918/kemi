# Resume reference

## Continuing an interrupted review

An interrupted run (exit `130`) keeps comments, seen marks, folding, and resolutions in a
session. Only a session whose copy completed within the size limit can be listed or resumed;
an unfinished or oversized copy keeps state alone. A run with nothing to keep leaves no
session. A `--live` review keeps no copy and is retained only when it holds a comment, reply,
message, or explanation. Continue the same review instead of rebuilding its input:

```text
kemi --resume <id>   # from any directory; use the id from stderr's resume line
kemi --resume        # in a terminal, choose from the session list
```

Resuming restores the frozen startup diff, seen marks, folding, comments, and resolutions.
It does not reread the original input: editing the working tree or moving away from the
repository does not change the page. Origin notes read "Cannot determine" if the repository
is gone. Live reload and its update badge are off.

`--live` is the exception: resume rereads the current working tree, keeps live reload and
the update badge, and reconnects the same page. Snapshots and mock assignments return too;
see [live review](live.md#reviewing-a-running-page).

Resume continues the same session id. A later submit stores its result for the workspace
where the review originally started, with manifest `approval` unchanged. Keep the resumed
process alive and collect its exit/result as for a new review.

## Chooser and flags

Without an id and without a terminal, `kemi --resume` prints resumable sessions as tab-separated
lines: id, last update, workspace, mode, seen/total, newest first. It exits `0`, or `2` with
empty stdout when none exist. In a terminal, cancelling the chooser (Esc or Ctrl+C) exits
`130`; no review starts and no resume line is printed.

`--resume` takes only `--port`, `--bind`, `--no-open`, `--serve`, and `--live-port`.
An input mode, `--digest`, or `--explanation` beside it is a usage error (exit `2`).
`--digest` and `--result` never create sessions. To change a resumed explanation, use
[explain](explanation-update.md#updating-the-explanation), not the original explanation file.

## Restored content and versions

Sessions write format 4 and read formats 2, 3, and 4. Old body strings stay plain text;
typed parts, complete original explanation quotes, retained versions, and undelivered events
keep their content. The original explanation input file is not needed.

A `--live` resume rescans the working tree; until a rescan succeeds, Code is unavailable. Do
not guess unknown session versions or treat unsaved state as guaranteed restored.
