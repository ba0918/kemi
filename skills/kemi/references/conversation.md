# Conversation reference

While a review is open, the person can hand you comments, replies, and notes before submitting,
and you can answer in the page. The review still ends with one submit, and the main `kemi`
process still prints the result JSON and exits as usual.

## The review id

When the agent endpoint is ready, stderr carries:

```text
kemi: review <id>
```

The id is the session id. A review that could not keep a session (no place to store state)
prints no such line, and the conversation is not available: the review works through the
submit alone. A resumed review prints the same id again.

## Waiting for what the person hands over

```text
kemi wait <id> [--timeout <seconds>]
```

Run it in the background, the same way you wait for the main `kemi` process. It returns when
the person presses **Hand to agent**, or when the review is submitted, and prints one JSON
document with everything that happened since the previous `kemi wait` returned:

```json
{
  "kemi": 4,
  "review": "<id>",
  "events": [
    {
      "type": "handed",
      "comments": [
        {"change": "added", "comment": {"id": "c1", "body": "the comment", "...": "..."}},
        {"change": "edited", "comment": {"id": "c2", "body": "the new wording", "...": "..."}},
        {"change": "deleted", "comment": {"id": "c3"}}
      ],
      "replies": [{"comment_id": "c1", "reply": {"id": "r2", "author": "reviewer", "body": "...", "parts": null, "variants": [], "chosen": null, "applied": null}}],
      "messages": [{"id": "m1", "author": "reviewer", "body": "a note on the whole review", "parts": null}]
    },
    {"type": "submitted", "result": {"kemi": 4, "verdict": "approved", "...": "..."}}
  ]
}
```

- `events` are in the order they happened. A `handed` event is one press of **Hand to agent**;
  `submitted` carries the same result JSON the main process prints.
- Nothing that happened is lost, but the same events can arrive twice: if `kemi wait` stops
  after printing them and before the review learns they were received, the next `kemi wait`
  returns them again. Tell repeats apart by the comment, reply, and message `id`s.
- A comment, reply, or message has the same shape as in the result (see the result reference).
  This includes explanation targets and typed parts restored from a session. Old plain
  undelivered events are emitted in version `4` shape with `parts: null`, without interpreting
  their bodies as Markdown. Deleted comments still carry only their id.
  The one difference: on a comment left on a running page, `page.image` is the absolute path of
  the PNG with the places drawn on it (or `null` when it could not be made). Read it before the
  submit; the file goes away with the session.
- `added` is a comment you have not seen; `edited` is one you were handed before whose text
  changed since; `deleted` carries only the id, so drop any work on it.
- `replies` are the person's new replies in threads, and `messages` their new notes on the
  whole review. A comment's own `replies` list holds the whole thread.

| Exit code | Meaning |
|---|---|
| `0` | Events were printed (a submit, if any, was an approval) |
| `1` | Events were printed and include a submit that requested changes |
| `2` | No running review with that id (it may have been submitted: read it with `kemi --result`), the review was suspended or failed while you waited, another `kemi wait` is already waiting, or a usage error. The reason is on stderr |
| `3` | `--timeout` passed with nothing to report; stdout is empty |

Only one `kemi wait` may wait on a review at a time. What the person hands over while you are
working is kept until the next `kemi wait` collects it. A submit is different: if the person
submits while no `kemi wait` is waiting, the review ends at once, and the next `kemi wait` exits
`2` because the review is no longer running. The result is then the JSON the main `kemi` process
printed when it exited, also available from `kemi --result`.

## Answering in the page

```text
kemi reply <id>
```

Write one JSON document to its stdin:

```json
{
  "writes": [
    {"type": "reply", "comment_id": "c1", "body": "Renamed it to parse_range."},
    {"type": "message", "body": "All three comments are addressed; the tests pass."}
  ]
}
```

A `reply` goes into the thread of that comment; a `message` is a note on the whole review. On
success stdout is `{"ids": ["r3", "m1"]}`, in the order of `writes`, and the exit code is `0`.
The writes appear in the page at once, marked as written by the agent.

Each reply or message accepts exactly one of `body` or `parts`, never both,
even when one is null. Old bodies remain plain. Parts preserve reading order
and type in File, Page, and explanation threads. For example:

```json
{"writes":[{"type":"reply","comment_id":"c1","parts":[{"type":"text","body":"An **example** follows."},{"type":"code","body":"read_premise()","language":"pseudocode","role":"example"}]}]}
```

A diagram is another ordered part, for example:

```json
{"writes":[{"type":"reply","comment_id":"c1","parts":[{"type":"text","body":"The request and answer are separate steps."},{"type":"sequence","title":"Request example","role":"example","participants":[{"id":"reader","label":"Reader"},{"id":"server","label":"Server"}],"events":[{"type":"message","from":"reader","to":"server","label":"Request","kind":"call"},{"type":"message","from":"server","to":"reader","label":"Answer","kind":"return"}]}]}]}
```

Text uses safe Markdown with images disabled. Code is literal, read-only and
copyable. Diagrams may fail browser layout despite structurally valid input;
the page then shows the reason and complete original JSON. Role labels express
the author's intent, not a correctness verdict. Part fields and inclusive
limits are in the [content contract](https://github.com/ba0918/kemi/blob/main/docs/spec/content-parts.md).
One malformed part in a later write refuses the whole batch. Explanation
comments cannot receive suggestions or Page variants.

The person comments on an explanation by selecting text in a text or code
block, or with the comment button of a diagram. An explanation thread is
still about the whole block: answer from the complete block quote, and use the
comment's `selection` (the selected text and which match of it, or null for a
diagram) to see which part of the block the question points at.

Either every write is stored or none is. A write to a comment that does not exist, a body over
64 KB (UTF-8 bytes), more than 50 agent replies on one comment, more than 200 agent messages,
or JSON of the wrong shape exits `2` with the reason on stderr and writes nothing. You cannot
open a comment or resolve one; only the person does that.

## The loop

1. Start the review and read the `kemi: review <id>` line.
2. Start `kemi wait <id>` in the background and tell the person they can hand you comments
   before they submit.
3. When it returns with `handed`, address the comments within the scope the person asked for,
   then answer with `kemi reply <id>`: say what you changed, or why you did not. If you changed
   reviewed files, say so and name their new identities in the reply as the approval target.
   The person sees the change by pressing the update badge in a `--worktree` or `--live` review,
   in a commit range whose `--to` is a branch or `HEAD` once you commit it, and for manifest
   files given by path. Otherwise the page keeps its startup diff, as in a `--staged` review or a
   resumed one other than `--live`: offer a new review.
4. Start `kemi wait <id>` again. Repeat until an event of type `submitted` arrives, or until
   `kemi wait` exits `2` because the review is no longer running (the person submitted while
   you were working, or the main `kemi` process exited). A `submitted` event's `result` is the
   review result, the same document the main process prints: check the main process's exit
   code, but do not read its stdout again. When `kemi wait` exits `2` instead, or its output
   reached you cut short, take the result from the main process's stdout, or `kemi --result`.

## Hand-over status

Reply in each thread you were handed (a hand-over of messages only waits for a message): until
you do, the person's page shows that thread as being worked on. After `kemi wait` returns, the
page shows you as not responding once 10 minutes pass with neither `kemi wait` nor a
`kemi reply` that writes something, unless your writes have already answered the hand-over.
When the work takes longer, write partway: a progress `message` restarts the 10 minutes and
leaves the threads shown as being worked on (on a hand-over of messages only, any message
answers it), while a reply in a thread marks that thread
answered. The person can hand you comments only after you run `kemi wait <id>` in that review;
a review where you never call it stays submit-only.
