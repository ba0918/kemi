# Explanation update reference

How to change the explanation of a review that is already running or resumed, and how to read a
large context selectively. The document format (sections, blocks, parts, references, limits) is
in [explanation](explanation.md).

## Updating the explanation

Use this procedure only for an already running or resumed review. Reply to ordinary questions
in the thread. Replace the explanation when its description, judgment points, or caveats
become stale, or for a correction/agreed document change. Match what the review actually shows.

```text
kemi explain <id> --context
kemi explain <id> --file update.json
kemi explain <id> < update.json
```

Context reads no stdin and exits `0` with one version-3 JSON object: `review`,
`diff_display_version`, `explanation_revision`, current saved `explanation` or null, and
`references:{code,pages}`. Check `kemi: 3`, the context contract's version, counted apart
from the result's `4`; do not guess an unknown contract version.
Code entries have `unit`, `group_id`, `path`, `old_present`/`new_present` booleans, and nullable
`old_lines`/`new_lines` totals. Pages are registered `{url}` values. Use this map for updated
references. It is a consistent server metadata read, not file contents or the reader's Code
display, and does not update that display. It is refused with a reason while a required
grouping is unavailable. In a commit range the other grouping starts building on first page
open; if this prevents adding a missing explanation, ask the person to open the page, then
retry context and save before the explanation handoff.

Update JSON has exactly `expected_revision`, `base_display_version`, and the **complete**
replacement `sections`, not a block patch. Use context's `explanation_revision` (0 when absent)
and, for the current review, its `diff_display_version`. For example, if context reports
revision 1 and the following synthetic display version:

```json
{"expected_revision":1,"base_display_version":"01K00000000000000000000000:1","sections":[{"id":"overview","title":"Review purpose","blocks":[{"id":"intent","part":{"type":"text","body":"The label now says Submit review. Please judge whether it clearly describes final submission. Narrow widths remain unchecked."},"references":[]}]}]}
```

Use the actual context token, never a hash, explanation revision, or file id as the base.
Known older bases are allowed but do not prove historical targets exist. A known base can
update an explanation while resumed Code is unavailable; context itself remains refused
until a successful scan supplies metadata again.

Success prints `{revision,base_display_version}` and exits `0`, after atomic save and before
notifying readers. Confirm **both** before claiming it saved or handing off after a missing
explanation was added. `--file` and `--context` are mutually exclusive. Invalid JSON, duplicate
keys, references, limits, revision conflicts, or save failures print no success JSON and exit
`2` with a reason on stderr, leaving the old state intact. Reread context after a revision
conflict rather than blindly overwriting; only one same-revision concurrent update succeeds.
After submit claims the review, updates are refused. The first argument `explain` selects the
subcommand; a manifest of that name is `./explain`.

Explain/context use the same local same-user endpoint protection as wait/reply. They do not
answer a hand-over, clear its working line, reset the response clock, generate wait events,
or resolve threads. Send a separate `kemi reply` when an answer is needed; only the person
resolves. Explanation comments retain their original revision and complete block quote.

Updates send a notice: readers keep their open revision and position until opening it.
Code lists do not refresh automatically. References use the reader's existing list; returning
keeps the original explanation revision/position. The open revision's freshness and the new
notice's freshness are compared separately with that screen's Code version. Matching bases
do not guarantee fixed file bytes or explanation correctness; context describes the server,
not what every reader has imported.

## Reading a large context

Keep the complete context in a private, ignored file; do not commit it. To reduce model input,
read an outline first and then select exact code paths or complete explanation blocks with
the bundled Python 3.9+ helper. `<skill-directory>` is this installed skill's directory.

```text
kemi explain <id> --context > <private-context.json>
python3 <skill-directory>/scripts/context_summary.py <private-context.json>
python3 <skill-directory>/scripts/context_summary.py <private-context.json> --path src/a.rs --block intent
```

The default view reports identity, current display version/revision, available counts, and
section/block IDs without bodies or full reference lists. Repeat `--path`, `--block`, or
`--page` to read specific items. A path returns every matching group/unit, not an arbitrarily
chosen match. Selected blocks retain complete typed parts and references. Missing selections
or invalid input exit `2` with empty stdout; success exits `0` with one compact JSON object.

`projection:true` marks a reading view, **not** the context contract or an update payload.
The helper makes no requests and does not change the saved input. Build a complete replacement
from the retained original `explanation.sections`, preserving unselected sections and blocks;
never send only `selected_sections` as the replacement. Read all details needed for the change,
and retrieve fresh context after a conflict. This reduces text loaded into the model, not the
server's response size. Without Python, read the retained JSON selectively using the environment's
existing tools rather than treating an incomplete view as the full document.
