import assert from "node:assert/strict";
import { test } from "node:test";

import {
  WIDTH_CHOICES,
  buildPageTree,
  chooseReference,
  chooseSnapshot,
  fitScale,
  overlayPlacement,
  liveOrigin,
  pageKey,
  parseWidth,
  snapshotLabel,
  snapshotOptions,
} from "./live-model.js";

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

/** @param {string} id @param {string} kind @param {string} [page] @param {number} [width] */
const snap = (id, kind, page = "/", width = 1280) => ({ id, kind, page, width });

test("without a choice, the last handed snapshot of the page and width is compared", () => {
  const snapshots = [snap("s1", "start"), snap("s2", "handed"), snap("s3", "manual"), snap("s4", "handed")];
  assert.equal(chooseSnapshot(snapshots, "/", 1280, null)?.id, "s4");
});

test("before any hand-over, the start snapshot is compared, and then the last manual one", () => {
  assert.equal(chooseSnapshot([snap("s1", "manual"), snap("s2", "start")], "/", 1280, null)?.id, "s2");
  assert.equal(chooseSnapshot([snap("s1", "manual"), snap("s2", "manual")], "/", 1280, null)?.id, "s2");
});

test("only snapshots of the same page and width are compared", () => {
  const snapshots = [snap("s1", "start", "/", 1280), snap("s2", "handed", "/other", 1280)];
  assert.equal(chooseSnapshot(snapshots, "/", 390, null), null);
  assert.equal(chooseSnapshot(snapshots, "/other", 1280, null)?.id, "s2");
  assert.equal(chooseSnapshot(snapshots, "/missing", 1280, null), null);
});

test("a chosen point stays chosen when the width changes, and shows nothing at a width it lacks", () => {
  const snapshots = [snap("s1", "start", "/", 1280), snap("s2", "manual", "/", 390)];
  assert.equal(chooseSnapshot(snapshots, "/", 1280, "s1")?.id, "s1");
  assert.equal(chooseSnapshot(snapshots, "/", 390, "s1"), null);
});

test("points are named by when they were taken, counted per kind", () => {
  const snapshots = [snap("s1", "start"), snap("s2", "handed"), snap("s3", "manual"), snap("s4", "handed")];
  assert.deepEqual(snapshots.map((s) => snapshotLabel(snapshots, s)), ["Start", "Handed 1", "Recorded 1", "Handed 2"]);
});

test("the points to choose from are the snapshots of the page, newest first", () => {
  const snapshots = [snap("s1", "start"), snap("s2", "handed", "/other"), snap("s3", "manual", "/", 390)];
  assert.deepEqual(snapshotOptions(snapshots, "/"), [
    { id: "s3", label: "Recorded 1 · 390" },
    { id: "s1", label: "Start · 1280" },
  ]);
});

test("a page with a mock compares with the mock unless something else is chosen", () => {
  const snapshots = [snap("s1", "start"), snap("s2", "handed")];
  assert.deepEqual(chooseReference({ snapshots, mock: "m.html", page: "/", width: 1280, chosen: undefined }), { type: "mock", path: "m.html" });
  assert.deepEqual(chooseReference({ snapshots, mock: "m.html", page: "/", width: 1280, chosen: "latest" }), { type: "snapshot", snapshot: snapshots[1] });
  assert.deepEqual(chooseReference({ snapshots, mock: "m.html", page: "/", width: 1280, chosen: "s1" }), { type: "snapshot", snapshot: snapshots[0] });
});

test("without a mock, or once it is removed, the snapshot order applies again", () => {
  const snapshots = [snap("s1", "start"), snap("s2", "handed")];
  assert.deepEqual(chooseReference({ snapshots, mock: null, page: "/", width: 1280, chosen: undefined }), { type: "snapshot", snapshot: snapshots[1] });
  assert.deepEqual(chooseReference({ snapshots, mock: null, page: "/", width: 1280, chosen: "mock" }), { type: "snapshot", snapshot: snapshots[1] });
  assert.deepEqual(chooseReference({ snapshots: [], mock: null, page: "/", width: 1280, chosen: undefined }), { type: "none" });
});

test("an overlaid reference is drawn at the full content height and moved by the scroll of the page", () => {
  assert.deepEqual(
    overlayPlacement({ scale: 0.5, viewportHeight: 400, scrollX: 0, scrollY: 600, contentHeight: 3000 }),
    { height: 3000, transform: "translate(0px, -300px) scale(0.5)" },
  );
});

test("a short page is still drawn as tall as the pane", () => {
  assert.deepEqual(
    overlayPlacement({ scale: 1, viewportHeight: 700, scrollX: 10, scrollY: 0, contentHeight: 300 }),
    { height: 700, transform: "translate(-10px, 0px) scale(1)" },
  );
});
