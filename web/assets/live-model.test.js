import assert from "node:assert/strict";
import { test } from "node:test";

import { WIDTH_CHOICES, buildPageTree, fitScale, liveOrigin, pageKey, parseWidth } from "./live-model.js";

test("the width choices are 390, 768 and 1280", () => {
  assert.deepEqual(WIDTH_CHOICES, [390, 768, 1280]);
});

test("a width from 320 to 3840 is accepted", () => {
  assert.deepEqual(parseWidth("320"), { ok: true, width: 320 });
  assert.deepEqual(parseWidth(" 3840 "), { ok: true, width: 3840 });
  assert.deepEqual(parseWidth("1024"), { ok: true, width: 1024 });
});

test("a width outside 320 to 3840 or not a whole number is refused with the range", () => {
  for (const text of ["319", "3841", "0", "", "12.5", "abc", "-400", "1e3"]) {
    const result = parseWidth(text);
    assert.equal(result.ok, false, text);
    assert.match(/** @type {{ ok: false, message: string }} */ (result).message, /320.*3840/, text);
  }
});

test("a page wider than its pane is scaled down, a narrower one is not enlarged", () => {
  assert.equal(fitScale(640, 1280), 0.5);
  assert.equal(fitScale(800, 390), 1);
  assert.equal(fitScale(0, 1280), 1);
});

test("a page is told apart by its path and query, ignoring the fragment", () => {
  assert.equal(pageKey("/docs/a.html?x=1#top"), "/docs/a.html?x=1");
  assert.equal(pageKey("/"), "/");
  assert.equal(pageKey(""), "/");
});

test("the relay origin uses the host name the review page was opened with", () => {
  assert.equal(liveOrigin("http:", "192.168.1.5", 5001), "http://192.168.1.5:5001");
});

test("the page tree lists the shown page and the pages with snapshots or mocks, with their widths", () => {
  const tree = buildPageTree({
    current: "/b",
    snapshots: [
      { page: "/a", width: 1280 },
      { page: "/a", width: 390 },
      { page: "/a", width: 1280 },
    ],
    mocks: new Set(["/c"]),
  });
  assert.deepEqual(tree, [
    { page: "/a", widths: [390, 1280], current: false, mock: false },
    { page: "/b", widths: [], current: true, mock: false },
    { page: "/c", widths: [], current: false, mock: true },
  ]);
});
