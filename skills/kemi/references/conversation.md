# Conversation reference

While a review is open, the person can hand you comments, replies, and notes before submitting,
and you can answer in the page. The review still ends with one submit, and the main `kemi`
process still prints the result JSON and exits as usual.

## The review id

Right after the URL line (and the line naming where results are saved), stderr carries:

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
  "kemi": 2,
  "review": "<id>",
  "events": [
    {
      "type": "handed",
      "comments": [
        {"change": "added", "comment": {"id": "c1", "body": "the comment", "...": "..."}},
        {"change": "edited", "comment": {"id": "c2", "body": "the new wording", "...": "..."}},
        {"change": "deleted", "comment": {"id": "c3"}}
      ],
      "replies": [{"comment_id": "c1", "reply": {"id": "r2", "author": "reviewer", "body": "..."}}],
      "messages": [{"id": "m1", "author": "reviewer", "body": "a note on the whole review"}]
    },
    {"type": "submitted", "result": {"kemi": 2, "verdict": "approved", "...": "..."}}
  ]
}
```

- `events` are in the order they happened. A `handed` event is one press of **Hand to agent**;
  `submitted` carries the same result JSON the main process prints.
- Nothing that happened is lost, but the same events can arrive twice: if `kemi wait` stops
  after printing them and before the review learns they were received, the next `kemi wait`
  returns them again. Tell repeats apart by the comment, reply, and message `id`s.
- A comment, reply, or message has the same shape as in the result (see the result reference).
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

Either every write is stored or none is. A write to a comment that does not exist, a body over
64 KB (UTF-8 bytes), more than 50 agent replies on one comment, more than 200 agent messages,
or JSON of the wrong shape exits `2` with the reason on stderr and writes nothing. You cannot
open a comment or resolve one; only the person does that.

## The loop

1. Start the review and read the `kemi: review <id>` line.
2. Start `kemi wait <id>` in the background and tell the person they can hand you comments
   before they submit.
3. When it returns with `handed`, address the comments within the scope the person asked for,
   then answer with `kemi reply <id>`: say what you changed, or why you did not.
4. Start `kemi wait <id>` again. Repeat until an event of type `submitted` arrives, or until
   `kemi wait` exits `2` because the review is no longer running (the person submitted while
   you were working, or the main `kemi` process exited). Then handle the result as usual: take
   it from the main process's stdout, or from `kemi --result`.

The page shows whether you are waiting, working, or not responding (working for 10 minutes with
neither `kemi wait` nor `kemi reply`). **Hand to agent** appears only once `kemi wait` has been
called in that review, so a review where you never call it stays submit-only.
