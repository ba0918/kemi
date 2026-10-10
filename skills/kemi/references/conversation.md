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
  "kemi": 3,
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
    {"type": "submitted", "result": {"kemi": 3, "verdict": "approved", "...": "..."}}
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
  undelivered events are emitted in version `3` shape with `parts: null`, without interpreting
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

## Updating the explanation

Use reply for an ordinary question or additional information in a thread. Use
explain for a correction or an agreed change to the document itself. Updating
the document does not resolve its threads; only the person resolves them.

```text
kemi explain <id> --context
kemi explain <id> --file update.json
kemi explain <id> < update.json
```

Context reads no stdin. It returns one version-3 JSON object with `review`,
`diff_display_version`, `explanation_revision`, the current saved `explanation`
or null, and `references:{code,pages}`. Code entries carry `unit`, `group_id`,
`path`, side-presence booleans `old_present`/`new_present`, and nullable total
line counts `old_lines`/`new_lines`. Pages are registered live `{url}` values.
Context is one consistent server metadata read. It neither provides all file
contents nor updates a reader's Code display; it is refused with a reason
while a required grouping is not ready.

An update is `{expected_revision,base_display_version,sections}`. Use 0 for
an absent explanation, otherwise the revision from context. Success prints
`{revision,base_display_version}` and exits 0. `--file` excludes `--context`.
Invalid JSON, duplicate keys, references, limits, revision conflicts, or save
failures print no success JSON, report a reason on stderr, and exit 2.
Concurrent requests with the same expected revision accept only one. Submit
and explanation updates are ordered; after submit claims the review, an
update is refused. Successful updates are saved atomically before notifying
readers.

Current-base Code references must match an exact group/path/unit and existing
side; line references need known total lines. A known older base accepts Code
reference shapes without claiming historical existence. Page references must
already be registered, even for an older base. Context and explain share the
local same-user endpoint protection of wait/reply. They do not answer a hand,
clear its working line, reset the response clock, or add conversation events.
Reply to the question separately when a reply is needed.
Code references use `unit: "file"` or `"commit"` in a commit range, and null
otherwise, for old bases too. A known base can still be used to update an
explanation while resumed Code is unavailable; context is refused until a
successful scan provides metadata again. The first argument `explain` selects
the subcommand; a manifest of that name is `./explain`.

An explanation update sends a notice. A reader's open revision and reading
position stay put until they open the notice. References use that screen's
imported Code list or an already registered live Page; unavailable targets
show a reason without automatically importing a newer Code list. Returning
from a reference keeps the original explanation revision and position.
The open revision's freshness and the new notice's freshness are compared
separately with the screen's Code token. Matching bases offer an information
detail about the lack of byte and correctness guarantees; different bases
show a separate notice with both versions. Context always describes the
server's current metadata, not any particular reader's Code display.
The screen uses known side presence and total lines for a Code reference;
status labels alone do not establish them. A failed target load or changed
line range gives a reason rather than silently shortening the range.

The page shows one of five states: not connected (`kemi wait` never called), waiting (`kemi wait`
is waiting, so the person can hand), working (`kemi wait` returned and the person waits for your
answer), replied (you have answered every thread the last returned `kemi wait` carried — a
message answers a hand-over of messages only; a thread the person deletes no longer needs an
answer, so once the remaining threads are answered and you have written at least once since that
return, the state is replied), and not responding (working for 10 minutes with
neither `kemi wait` nor `kemi reply`; replied never turns into it). Reply to each thread you were
handed, so the person can see which ones you have answered: each thread you were handed shows a
"working" line until you reply in it (a hand-over of messages only waits for a message). **Hand to agent** is always shown but cannot be pressed until `kemi wait` has been called in that
review; until then the page tells the person that you can be handed comments once you run
`kemi wait <id>`, with that command to copy. A review where you never call it stays submit-only.
