# Live review reference

What `kemi --live` puts under review, and how to read the comments the person leaves on the page.

## Reviewing a running page

`kemi --live <url|file>` puts a running page under review together with the working tree.
The URL must be `http://` on `localhost` or `127.0.0.0/8` (a dev server or Storybook). A file
must be a `.html` or `.htm` inside the git working tree kemi starts in, or inside the launch
directory outside git; a symlink counts where it points, and nothing in `.git/` is served.
Anything else is a usage error (exit `2`). The code view is the same as `--worktree`; outside
a git repository there is none. `--live` combines only with `--port`, `--bind`, `--no-open`,
`--live-port <n>`, `--focus`, `--explanation`, and `--serve`. The review title is `Live review of <url>`, and
for a file the URL is its path from that directory, such as `/docs/mock.html`.

kemi prints `kemi: live <url>` after the review line: the page relayed through a second port
(`--live-port`, default a free one). It opens only in a browser that opened the review URL first,
so send the person the review URL, not this one. When the dev server is not running, the page
says so and waits for it. A file page is served on that port too, reloads when it or a file it
loaded is saved, and waits when the file is missing.

The page has a **Page** view (the running page beside what it is compared with, at a chosen width
from 320 to 3840) and a **Code** view (the working-tree diff). What the page is compared with is a
snapshot — a script-free copy kemi takes at the start, at each hand-over to you, and on
**Record now** — or a mock, an HTML file in the served directory the person assigns to the page.
They can be shown side by side or overlaid with adjustable opacity. Against a snapshot, the page
also lists and marks what changed (look, text, added and removed elements, and, folded, what only
moved); against a mock it does not. Handing over is the person's
action: their next hand-over (or **Record now**) takes a fresh snapshot.

The person can also comment on the page itself, putting numbered places on it with the
**Element**, **Arrow**, and **Pen** tools. Such a comment reaches you with `page` instead of a
file and lines: the URL and width it was left at, its places with the elements they point at,
and, from `kemi wait`, the path of a PNG of the area with the places drawn on it. Read
[page comments](#page-comments) for the shape, and look at the image before changing the page.

## Page comments

A comment the person left on the running page has `group_id` `page` and `group_title` `Page`; its `path`, `side`, `start_line`,
`end_line`, and `suggestion` are `null`, `quote` is `[]`, and `outdated` is always `false`.
Its `page` is:

```json
{
  "url": "/products?x=1",
  "width": 390,
  "places": [
    {
      "n": 1,
      "kind": "element",
      "points": [],
      "elements": [{"selector": "#buy", "text": "Buy", "rect": {"x": 0, "y": 120, "w": 200, "h": 60}}]
    },
    {
      "n": 2,
      "kind": "arrow",
      "points": [{"x": 12, "y": 340}, {"x": 60, "y": 352}, {"x": 96, "y": 380}],
      "elements": [{"selector": "#total", "text": "$12", "rect": {"x": 80, "y": 370, "w": 60, "h": 24}}]
    }
  ],
  "image": null
}
```

`url` is the path and query the person was viewing, and `width` the viewport width in CSS
pixels. `places` are where the comment points, in order of `n`; the body refers to them by
number as `#n` (`#2` is the place whose `n` is 2; a number without `#` is not a reference), and
the numbers run from 1 without gaps. `kind` is
`element` (a chosen element, `points` is `[]`), `arrow` (the points of the arrow, and the
element at its head), or `pen` (the points of the line, and up to five elements that overlap
the area it encloses, even partly, largest overlap first; elements that contain the whole line, such as a wrapper around the
page, are left out, and a line drawn inside one element names that innermost element). `html` and
`body` are never named: an arrow pointing at the empty background, or a line around nothing but
background, has `elements` `[]` and only its `points` (an area only); look at the comment's
`image` to see what it points at. Arrow and pen `points` are whole numbers, rounded and thinned
so that every dropped point lies within 1 CSS pixel of the line through the kept ones; the
first and last points are where the line began and ended (an arrow's head is at the last).
Points saved by a kemi before contract version `4` keep their saved values,
which may be fractional. There is one image per comment, not per place. Each element carries a `selector`, its `text`, and its `rect`; all
coordinates are CSS pixels of the page, independent of scrolling. For an element inside an
open shadow root, `selector` joins the host's selector and the selector inside the shadow root
with ` >>> ` (for example `#host >>> p:nth-of-type(2)`), which is not CSS: query the host in the
document, then its `shadowRoot` with the part after ` >>> ` (repeated for nested roots).
`image` is the absolute path of a PNG of the area with the places drawn on it, given only in
what `kemi wait` returns; in the result it is always `null`, because the file is removed with
the session on submit.
