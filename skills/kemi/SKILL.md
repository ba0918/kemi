---
name: kemi
description: "Show a set of changes to this user for review or approval by serving a local review page with the `kemi` command, then waiting for the verdict, comments and suggestions it returns as one JSON document when the user submits. Covers a range of commits, uncommitted or staged work, and a hand-written manifest for drafts and prose whose groups you write yourself. Use whenever changes are handed to the user to read, review, approve, or reject rather than described in the conversation."
license: MIT OR Apache-2.0
---

# kemi

`kemi` is a local review tool. It serves a review page on `127.0.0.1`; the person reads the
change there, may attach comments and suggestions to lines, and ends the review with the
approve button (承認) or the request-changes button (変更要求). kemi then prints one JSON
document to stdout and exits. It never edits files of its own: applying a suggestion is your
job, not the tool's.

If your environment has a norm for how changes must be presented to a person (how to group
them, what reason to carry with each group, what to name as the approved bytes), follow it.
This document covers only the tool.

## How a review starts and ends

kemi ends by itself. It runs until the person submits; at that moment it answers the page,
stops serving, saves the result, prints the JSON to stdout and exits. stderr carries the log,
including two lines printed when the server comes up: `kemi: <url>`, and the directory where
results are kept.

1. Start kemi as a background process whose exit your environment tells you about, with stdout
   redirected to a file of its own so the JSON cannot mix with the log. Keep stderr where you
   can read it back too — a file of its own, or whatever your environment records a background
   process's output in — since the URL arrives there.
2. Take the URL out of the `kemi: <url>` line on stderr and hand it to the person.
3. Stop and wait for the exit. Do not sit in a loop asking whether it is done, and do not begin
   work that assumes a verdict.
4. When it exits, read the exit code and the file you redirected stdout to.

| Exit code | Meaning | stdout |
|---|---|---|
| `0` | approved | the result JSON |
| `1` | changes requested | the result JSON |
| `2` | startup, usage, or runtime error | empty |
| `130` | interrupted before any submit | empty |

The review lives in kemi's memory alone. If it ends with `130`, or you kill it, or the machine
takes it away, the person's comments are gone: no JSON on stdout and no saved result. Say that
plainly and offer a fresh review. Never report a verdict that did not arrive.

Never:

- run kemi in the foreground, where a timeout on your side kills the review with it;
- stop or restart it while the person may still be reading;
- treat what the person writes in the conversation as the page's verdict. If they say they
  approve while kemi is still up, that is their decision and not a submit: no JSON will come,
  so stop kemi yourself and record the decision as one made in the conversation.

## Choosing the input

One input mode per run. Giving two is an error (exit `2`).

| Input | Command | Groups |
|---|---|---|
| A range of commits | `kemi --from <base> [--to <head>]` | both groupings, see below |
| The working tree | `kemi --worktree` | one group, untracked files included |
| The index | `kemi --staged` | one group |
| Anything whose reasons only you know | `kemi <manifest.json>`, or `kemi -` to read it from stdin | the groups you write |

A commit range carries both of its groupings at once, and the page switches between them
without a restart:

- **Final form** (`--group-by file`, the default): the whole range as one diff, each path
  appearing once in its end state, with the commit that made each change block shown above it.
- **Per commit** (`--group-by commit`): one group per non-merge commit in the range, the commit
  subject as its title and the commit message body as its reason.

`--group-by` only picks which of the two the page opens on, so pass `--group-by commit` when
what each commit did is the point of the review. Without `--from` it is a usage error
(exit `2`). Choose a base that leaves out work the person has already reviewed.

Other flags worth knowing:

| Flag | Meaning |
|---|---|
| `--no-open` | do not open a browser. Use it whenever a browser should not appear on the person's machine |
| `--port <n>` | listen on a fixed port; the default `0` picks a free one |
| `--focus <path>` | a JSON file that adds focus marks and watch points to any input mode |
| `--base <dir>` | where relative paths in a manifest and in `--focus` resolve from; default `.` |

The `--focus` file looks like this:

```json
{
  "groups": { "<group-id>": { "watch": "what to judge in this group" } },
  "files": [{ "path": "src/a.rs", "focus": true, "note": "why this file matters most" }]
}
```

A group id is `all` for the final form, the full commit sha for a per-commit group, `worktree`,
`staged`, or an id you chose in a manifest. An id or a path that does not exist is an error
(exit `2`); kemi never quietly ignores one.

## Surveying a large change first

`--digest` prints a bounded map of the review to stdout and exits without serving a page. Use
it to decide what to put in front of the person, or to orient yourself in a change too large to
read straight through. It gives the totals, the per-group and per-directory counts, and the
files with the most changed lines, with no line content at all, so it stays small even for tens
of thousands of files. `--digest-top <n>` sets how many top files it asks for (default `100`).
For a commit range it follows `--group-by`, so it counts the change the same way the page will
show it.

## Writing a manifest

A manifest is for everything git cannot group for you: drafts, prose, a set of changes whose
reasons live only in your head. The groups and their reasons are your work, not the tool's.

```json
{
  "title": "short name of the change",
  "subtitle": "one sentence on what is being decided",
  "meta": {"label": "value"},
  "groups": [
    {
      "id": "stable-anchor",
      "title": "what this group changes",
      "why": "why it changes, not a restatement of the lines",
      "watch": "what the person should judge here: a choice that could have gone otherwise",
      "diffs": [
        {
          "path": "docs/example.md",
          "old_path": "before/example.md",
          "new_path": "docs/example.md",
          "status": "modify",
          "focus": true,
          "note": "why this file matters most"
        }
      ]
    }
  ],
  "approval": [{"path": "docs/example.md", "identity": "sha256:..."}]
}
```

- `why` and `watch` are what make the page worth reading. `why` is the reason the change was
  made; `watch` is the thing you want judged. Neither can be derived from the diff, so leave one
  out rather than filling it with a restatement of the lines.
- A diff entry gives its two sides either as literal text in `old` and `new`, or as files in
  `old_path` and `new_path`, resolved against `--base`. A side that is absent means the file was
  added or deleted. Giving both `old` and `old_path` (or both `new` and `new_path`), or naming a
  path that does not exist, is an error (exit `2`), never a silently empty side.
- `status` is `add`, `delete`, `rename`, or `modify`, and is inferred when left out.
  `renamed_from` names the old path a rename came from; with `status` left out it makes the
  entry a rename.
- `id` may be left out (`g1`, `g2` and so on are generated), but choose stable ids if a
  `--focus` file will refer to them.
- Fill `approval` whenever the person is being asked to approve something, and say in the
  conversation that the approval is of those bytes. kemi displays `identity` and does not check
  it; you compute it yourself from the exact bytes in question, for instance with `sha256sum` on
  the file, or `git show :<path>` piped into `sha256sum` for a staged one.

## What to tell the person after it starts

The page organises the reading; your message says where to look first. Once the URL is on
stderr, tell them:

- the URL, and that the review ends when they press 承認 (approve) or 変更要求 (request changes);
- how many groups there are and what each one is, in a line or two;
- the single decision you most want checked;
- which assumptions in the change are yours rather than theirs.

## Reading the result

```json
{
  "kemi": 1,
  "title": "Review",
  "verdict": "approved",
  "approval": [{"path": "src/a.rs", "identity": "sha256:..."}],
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
      "suggestion": {"replacement": "the replacement text"}
    }
  ]
}
```

- `kemi` is the version of this contract, `1` today. Read it before trusting the shape.
- `verdict` is `approved` or `changes_requested`, and agrees with the exit code.
- On `approved` the person accepts the bytes named in `approval`. Before acting on the approval,
  compute each `identity` again and compare. If a file changed after the review began, say so
  and stop, rather than proceeding on an approval of bytes that no longer exist. Comments may
  still be present on an approval; read them and address or report them.
- On `changes_requested`, address every comment, then offer a new review of the result.
- Line numbers are 1-based and inclusive. `side` is `new` or `old`, and `old` means the old
  file's own numbering. A comment on a whole file has `start_line` and `end_line` `null`,
  `quote` `[]`, and `side` `new`.
- `suggestion` replaces new-side lines `start_line` through `end_line` of `path` with
  `replacement`; an empty `replacement` means those lines should go. It is `null` for old-side
  and whole-file comments. kemi never applies a suggestion, you do: check `quote` against the
  file as it stands now before you write.
- `outdated` is `true` when the file changed after the comment was written, or when the commit
  the comment was sitting on left the range. kemi does not renumber a comment, so do not trust
  the line numbers of an outdated one: locate the place by `quote`, and ask the person when it
  cannot be found.
- `replies` and `resolved` are always `[]` and `false` today; they are in the contract so they
  can be filled later.
- `comments` come in creation order. `approval` is returned exactly as it was given, and is `[]`
  when none was.

## Fetching a result you missed

Every accepted submit is also written to a result file, so losing stdout is not losing the
review.

```text
kemi --result                     # the latest result for the repository you are in
kemi --result --any               # the latest result from anywhere
kemi --result --workspace ../app  # the latest result for another place
```

`kemi --result` prints that JSON to stdout and exits with the verdict's own code (`0` approved,
`1` changes requested). With no matching result it prints nothing and exits `2`. A result
belongs to the top of the git repository kemi was started in. Only the newest twenty results
are kept across all repositories, and nothing at all is written for an interrupted or failed
run, so read a result soon after the review rather than assuming it will still be there.

`--result` goes together only with `--any` or `--workspace`. Anything else beside it, including
an input mode, is a usage error (exit `2`).

## Notes for specific agents

**Claude Code.** Start kemi with the Bash tool and `run_in_background: true`, sending stdout to
a file under your scratchpad directory. The call returns at once and you are notified when the
process exits; read the `kemi: <url>` line from the background task's output, give the person
the URL, then end your turn. A foreground Bash call is the mistake to avoid here: its timeout
would kill the review along with the person's comments.
