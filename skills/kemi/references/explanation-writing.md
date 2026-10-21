# Writing an explanation

This reference covers what an explanation says and how it shows it: the bar it must reach, a
reading order, what each kind of change needs, which part to use for what, and tested diagram
templates. The JSON format, references, and limits are in [explanation](explanation.md), and the
update procedure in [explanation updates](explanation-update.md).

## The bar

The explanation is where the person reads the change; the diff, specification, or plan is the
evidence behind it. A review is a rare moment that decides what gets built. A specification or a
plan approved without being understood gets the wrong thing built, and nobody notices until the
work is done. Spend the effort here.

Write for a reader who has seen neither the session nor the diff. From the explanation alone they
must be able to say:

- what behaves differently afterwards, and how it works;
- why it is done this way, and which alternatives were rejected and why;
- what they are asked to decide, the options, and what each option changes next;
- what the author decided alone rather than agreed with them;
- what is unverified, and what could go wrong.

Length follows what there is to explain and what the change decides, not the size of the diff.
One block can carry a one-line fix; a specification or a plan, which decides what gets built,
needs every part below. Do not pad, and do not compress meaning into labels to save space: a short
explanation fails when it leaves only names, file lists, or "please check X".

## Reading order

A reading order, not a fixed set of chapters. Leave a part out only when it has nothing to say.

1. **The change in plain words.** One or two sentences on what is different, then before and
   after, often as a table of situations. Say what is out of scope.
2. **Terms.** Define every term coined during the work, and every project term the reader may
   not hold, before it is used.
3. **The whole picture.** A diagram of the structure or flow the change touches, so each later
   section has a place to sit.
4. **One section per mechanism or decision.** What it does, shown by a diagram, pseudocode, or a
   concrete input-and-output example; why this way, with the rejected alternatives; typed
   references to where it lives.
5. **Decisions requested.** One block each: the question, the options, what each changes next,
   and your recommendation with its grounds. Mark what you added without agreement.
6. **Risks and unverified points.** What was not checked, what could break, and what would
   reveal it.

Keep process records out of the body. Hashes and the approval target belong in the message
that shares the URL (and a manifest's `approval`); test counts and command logs fit in at most
one short final block.

## What each kind of change needs

**Specification.**

- The behaviour model: states and transitions, or the decision rules, as a flow or a decision
  table.
- A situation table: each realistic situation, what happens before, what happens after.
- Each rule with one concrete example, including the boundaries: empty, missing, failing,
  concurrent.
- Rejected alternatives, each with the reason it lost.
- What the person agreed during the interview, separated from what the author decided alone.

**Plan.**

- What will exist when the plan is done: components and how they connect.
- Step order and dependencies, with the gates where work stops if a check fails.
- Per step: what changes, how it is verified, and what could go wrong.
- Key algorithms or data changes in pseudocode.
- Provisional answers the plan took, each with the condition that would overturn it.

**Code.**

- Behaviour before and after, as a user or caller observes it.
- The call or data flow through the changed parts.
- The core logic in pseudocode or a short excerpt; label an excerpt `confirmed` only when it
  matches the reviewed code.
- The tests that pin the new behaviour, named by what they check.

## Choosing the part

| To show | Use |
|---|---|
| Messages between components over time | `sequence`, with `alt` or `loop` for branches |
| A decision, retry logic, or a state machine | `flow` with `decision` nodes |
| Structure: components, layers, dependencies, what is new | `diagram` (grid) |
| Logic, an algorithm, a rule with conditions | `code` with `language: "pseudocode"` |
| One rule across many situations | Markdown table in `text` |
| An output, message, file, or screen layout | `code` with `language: "text"` |

Put prose next to every diagram, snippet, and table that says what to notice in it. Set `role`
by what the person may rely on: `confirmed` only for a verbatim excerpt of the reviewed bytes;
`proposal` for what the change introduces in any other form, including pseudocode or a diagram
of code already written, since the person has not accepted it yet (say beside it that the code
exists, or readers take `proposal` as not yet built); `example` for anything else, such as a
synthetic input, a hypothetical caller, or a paraphrase of how things worked before.

## Contrasts

A file-shaped account, and the mechanism:

```text
Bad:  retry.rs: changed the backoff. config.toml: added max_attempts.
      docs/spec/delivery.md: updated section 3.
Good: A failed delivery is now retried with growing waits (30 s, 60 s, 120 s, ...
      capped at one hour) instead of every 60 s, and only for errors that can pass:
      timeouts, 5xx, 408, and 429. [flow of the decision] [pseudocode of the wait]
```

A term from the session, and the same term defined:

```text
Bad:  Poison deliveries now go to the DLQ once the budget is spent.
Good: A delivery that keeps failing now moves to a dead-letter queue, a holding
      queue that nothing retries automatically, after its 8 attempts are used up.
```

A request to check, and a decision the person can make:

```text
Bad:  Please check whether the retry count is OK.
Good: Maximum attempts: 8 (about an hour of retrying) or 5 (about 8 minutes)?
      8 rides out a typical deploy window; 5 alerts sooner. I chose 8 without
      data behind it.
```

## Diagram templates

These templates rendered without failure in a test browser. What makes a diagram fail:

- Geometry is checked only when the page draws a diagram. Startup accepts a structurally valid
  diagram that the page then shows as `Diagram unavailable: <reason>` with its JSON.
- Text is measured with the reader's fonts, so a diagram that draws in your browser can fail in
  theirs. Leave slack: a few words per node and per edge, and the spacing below.
- A flow node's `position` is its centre. Without positions every node sits in one column, so a
  branch draws an arrow through a node and fails (`Arrow N enters shape X interior`). Give every
  node a position once there is a branch.
- Leave about 180 between rows and 360 between columns. A decision is 220 wide and grows taller
  with a long label; widen it with `width` instead.
- An edge label sits beside the edge's longest segment and fails when it touches any shape
  (`Label N overlaps unrelated shape X`). Keep it to a word or two, and route a back edge along a
  lane outside the nodes with `via`.
- Inline, a large diagram shows only part of itself until the reader zooms out or opens it
  separately. Split a flow past about a dozen nodes.
- In a grid, `id = ? label` draws a dashed box, for example for a part that does not exist yet.

A retry decision with a loop (`flow`):

```json
{"type": "flow", "title": "After one delivery attempt", "role": "example",
 "nodes": [
  {"id": "attempt", "kind": "start", "label": "Send the delivery", "position": {"x": 320, "y": 60}},
  {"id": "ok", "kind": "decision", "label": "2xx response?", "position": {"x": 320, "y": 240}},
  {"id": "delivered", "kind": "end", "label": "Delivered", "position": {"x": 680, "y": 240}},
  {"id": "retryable", "kind": "decision", "label": "Timeout, 5xx, 408 or 429?", "width": 260, "position": {"x": 320, "y": 440}},
  {"id": "dead", "kind": "end", "label": "Dead-letter and alert", "position": {"x": 680, "y": 440}},
  {"id": "left", "kind": "decision", "label": "Attempts left?", "position": {"x": 320, "y": 640}},
  {"id": "wait", "kind": "process", "label": "Wait: backoff or Retry-After", "position": {"x": 320, "y": 820}}
 ],
 "edges": [
  {"from": "attempt", "to": "ok"},
  {"from": "ok", "to": "delivered", "from_port": "east", "to_port": "west", "label": {"text": "yes"}},
  {"from": "ok", "to": "retryable", "label": {"text": "no"}},
  {"from": "retryable", "to": "dead", "from_port": "east", "to_port": "west", "label": {"text": "no"}},
  {"from": "retryable", "to": "left", "label": {"text": "yes"}},
  {"from": "left", "to": "dead", "from_port": "east", "to_port": "south", "via": [{"x": 680, "y": 640}], "label": {"text": "no"}},
  {"from": "left", "to": "wait", "label": {"text": "yes"}},
  {"from": "wait", "to": "attempt", "from_port": "west", "to_port": "west", "via": [{"x": 110, "y": 820}, {"x": 110, "y": 60}], "label": {"text": "retry"}}
 ]
}
```

One exchange, branching on the response (`sequence` with `alt`):

```json
{"type": "sequence", "title": "One delivery, by response", "role": "example",
 "participants": [
  {"id": "worker", "label": "Worker"},
  {"id": "endpoint", "label": "Endpoint"},
  {"id": "queue", "label": "Queue"}
 ],
 "events": [
  {"type": "message", "from": "worker", "to": "endpoint", "label": "POST event (attempt n)", "kind": "call"},
  {"type": "alt", "branches": [
   {"condition": "2xx", "messages": [
    {"type": "message", "from": "endpoint", "to": "worker", "label": "200 OK", "kind": "return"},
    {"type": "message", "from": "worker", "to": "queue", "label": "ack", "kind": "call"}]},
   {"condition": "429 with Retry-After", "messages": [
    {"type": "message", "from": "endpoint", "to": "worker", "label": "429, Retry-After: 120", "kind": "return"},
    {"type": "message", "from": "worker", "to": "queue", "label": "requeue in 120 s", "kind": "call"}]},
   {"condition": "timeout or 5xx", "messages": [
    {"type": "message", "from": "worker", "to": "queue", "label": "requeue in backoff(n)", "kind": "call"}]}
  ]}
 ]
}
```

Where the change sits, with new parts dashed (`diagram`):

```json
{"type": "diagram", "title": "Where the change sits", "role": "example",
 "source": "producer = Producer\nqueue = Delivery queue\nworker = Worker\nendpoint = Customer endpoint\ndlq = ? Dead-letter queue (new)\nalert = ? Alert (new)\n| producer | queue | worker | endpoint |\n| . | . | dlq | alert |\nproducer -> queue : enqueue\nqueue -> worker : take\nworker -> queue : requeue\nworker -> endpoint : POST\nworker -> dlq : give up\ndlq -> alert : notify"
}
```

Each goes in a block as its `part`, with an `id` and `references`, like any other part.

## A worked outline

A synthetic specification change, "retry failed webhook deliveries with backoff", laid out to
this reference:

| Section | Blocks |
|---|---|
| The change in plain words | text: one-sentence summary; text: table of situations (timeout, 500, 404, 429 with Retry-After, endpoint down for hours) with before and after |
| Terms | text: delivery attempt, backoff, dead-letter queue, Retry-After |
| The whole picture | grid: where the change sits |
| When a delivery is retried | flow: after one attempt; text: why 4xx other than 408 and 429 is not retried |
| How long to wait | pseudocode: `delay(n) = min(30s * 2^(n-1), 1h) * jitter(0.8..1.2)`, with Retry-After taking precedence; text: why jitter; sequence: one delivery by response |
| Decisions requested | text: 8 or 5 attempts, with what each changes; text: whether a 429 counts as an attempt |
| Risks and unverified points | text: a long outage fills the dead-letter queue; the jitter range has no measurement behind it |

## Before handing off

Show this evidence in the message that shares the URL rather than asserting the explanation is
clear:

- **Every diagram drew.** The review page shows no `Diagram unavailable` in any block, or the
  message says the page could not be opened to check. A failure the person reports from their
  own browser still needs fixing: their fonts decide.
- **A cold reader understood it, and it matches the diff.** A cold reader has seen neither the
  session nor the diff, such as another agent or a fresh session. Ask in two steps:
  1. Give only the explanation file, and ask for the five points in [the bar](#the-bar) in the
     reader's own words. Withhold the diff, the repository, and the session.
  2. Once the restatement is back, give the same reader the reviewed diff, and ask what the
     explanation claims that the diff does not support, and what the diff changes that the
     explanation leaves out. If you cannot send the same reader a second message, ask a
     second cold reader for this comparison alone, giving the explanation before the diff.

  Fix each misreading and each mismatch in the explanation or in the change. A fix that adds or
  changes a claim, a number, a step, or a decision, not only the wording, goes to a new cold
  reader for both steps once; fixes after that second reader are not checked again. If no cold
  reader was available, or a fix went unchecked, the message says so. Skip the check only when
  the whole explanation is a sentence or two because there is nothing more to say, such as a
  typo or a version bump; a small diff that changes a rule or a behaviour does not qualify.

The order matters: a reader who has seen the diff fills the explanation's gaps from it and can no
longer tell you what the explanation alone conveys. The comparison matters because the person
will not read the diff: an explanation that is clear but wrong is approved as it stands.
