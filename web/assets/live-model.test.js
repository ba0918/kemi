import assert from "node:assert/strict";
import { test } from "node:test";

import {
  WIDTH_CHOICES,
  addPlace,
  buildPageTree,
  draftElsewhere,
  editBody,
  editedSpan,
  emptyDraft,
  strayRefs,
  imageUnsavedNotice,
  removePlace,
  undoPlace,
  chooseReference,
  chooseSnapshot,
  compareHeading,
  referenceName,
  referenceOptions,
  fitScale,
  overlayPlacement,
  liveOrigin,
  pageKey,
  parseWidth,
  snapshotLabel,
  startSnapshotDue,
  snapshotOptions,
  unsavedSnapshotNotice,
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
    comments: [],
  });
  assert.deepEqual(tree, [
    { page: "/a", widths: [390, 1280], current: false, mock: false, comments: 0 },
    { page: "/b", widths: [], current: true, mock: false, comments: 0 },
    { page: "/c", widths: [], current: false, mock: true, comments: 0 },
  ]);
});

test("the page tree lists pages with only comments, with the comment count and the widths they were left at", () => {
  const tree = buildPageTree({
    current: "/",
    snapshots: [{ page: "/", width: 1280 }],
    mocks: new Set(),
    comments: [
      { page: "/cart", width: 390 },
      { page: "/cart", width: 768 },
      { page: "/cart", width: 390 },
      { page: "/", width: 1280 },
    ],
  });
  assert.deepEqual(tree, [
    { page: "/", widths: [1280], current: true, mock: false, comments: 1 },
    { page: "/cart", widths: [390, 768], current: false, mock: false, comments: 3 },
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

test("the points to choose from are the snapshots of the page: handed, then start, then recorded, newest first in each", () => {
  const snapshots = [
    snap("s1", "start"),
    snap("s2", "manual"),
    snap("s3", "handed"),
    snap("s4", "handed", "/other"),
    snap("s5", "manual", "/", 390),
    snap("s6", "handed"),
  ];
  assert.deepEqual(
    snapshotOptions(snapshots, "/").map((option) => option.id),
    ["s6", "s3", "s1", "s5", "s2"],
  );
});

test("the reference choices are auto first and the mock last", () => {
  const snapshots = [snap("s1", "start"), snap("s2", "handed")];
  const values = referenceOptions({ snapshots, page: "/", width: 1280, mock: "docs/m.html" }).map((option) => option.value);
  assert.deepEqual(values, ["latest", "s2", "s1", "mock"]);
});

test("auto is named after the snapshot it picks, the same way as that snapshot's own choice", () => {
  const before = [snap("s1", "start")];
  const handedTwice = [...before, snap("s2", "handed"), snap("s3", "manual"), snap("s4", "handed")];
  for (const { snapshots, picked } of [{ snapshots: before, picked: "s1" }, { snapshots: handedTwice, picked: "s4" }]) {
    const options = referenceOptions({ snapshots, page: "/", width: 1280, mock: null });
    const auto = options.find((option) => option.value === "latest");
    const own = options.find((option) => option.value === picked);
    assert.ok(auto && own);
    assert.ok(auto.label.includes(own.label), `${auto.label} names ${own.label}`);
  }
});

test("auto with nothing recorded at this width says so instead of naming a snapshot", () => {
  const options = referenceOptions({ snapshots: [snap("s1", "start", "/", 390)], page: "/", width: 1280, mock: null });
  const auto = options.find((option) => option.value === "latest");
  assert.ok(auto && !auto.label.includes(options[1].label));
});

test("the reference in use is named the same way as its choice", () => {
  const snapshots = [snap("s1", "start"), { ...snap("s2", "manual"), unsaved: true }];
  const options = referenceOptions({ snapshots, page: "/", width: 1280, mock: "docs/m.html" });
  for (const reference of [
    chooseReference({ snapshots, mock: null, page: "/", width: 1280, chosen: "s1" }),
    chooseReference({ snapshots, mock: null, page: "/", width: 1280, chosen: "s2" }),
    chooseReference({ snapshots, mock: "docs/m.html", page: "/", width: 1280, chosen: "mock" }),
  ]) {
    const value = reference.type === "snapshot" ? reference.snapshot.id : "mock";
    assert.equal(referenceName(snapshots, reference), options.find((option) => option.value === value)?.label);
  }
});

test("the heading while overlaid differs in the middle, at either end, and from the page alone", () => {
  const reference = "Start · 1280";
  const headings = [
    compareHeading({ compare: "now", reference, opacity: 50 }),
    compareHeading({ compare: "overlay", reference, opacity: 0 }),
    compareHeading({ compare: "overlay", reference, opacity: 50 }),
    compareHeading({ compare: "overlay", reference, opacity: 100 }),
  ];
  assert.equal(new Set(headings).size, headings.length, JSON.stringify(headings));
});

test("the heading in the middle of an overlay names both sides and the opacity", () => {
  const heading = compareHeading({ compare: "overlay", reference: "Start · 1280", opacity: 60 });
  assert.ok(heading.includes("Start · 1280") && heading.includes("60"), heading);
});

test("on a narrow screen the heading follows the one page shown, whatever the way to compare", () => {
  const reference = "Start · 1280";
  const alone = compareHeading({ compare: "now", reference, opacity: 50 });
  for (const compare of /** @type {const} */ (["now", "side", "overlay"])) {
    assert.equal(compareHeading({ compare, reference, opacity: 50, shown: "live" }), alone);
    assert.equal(compareHeading({ compare, reference, opacity: 50, shown: "ref" }), reference);
  }
});

test("a snapshot that was not saved is told apart among the points to choose from", () => {
  const saved = snap("s1", "manual");
  const [unsaved] = snapshotOptions([{ ...saved, unsaved: true }], "/");
  const [plain] = snapshotOptions([saved], "/");
  assert.notEqual(unsaved.label, plain.label);
});

test("only a snapshot that was not saved carries a notice that it is gone after resuming", () => {
  assert.notEqual(unsavedSnapshotNotice({ ...snap("s1", "manual"), unsaved: true }), "");
  assert.equal(unsavedSnapshotNotice({ ...snap("s1", "manual"), unsaved: false }), "");
  assert.equal(unsavedSnapshotNotice(snap("s1", "manual")), "");
});

test("a chosen point that is no longer among the snapshots shows that nothing is recorded", () => {
  const snapshots = [snap("s1", "start"), snap("s3", "manual")];
  assert.deepEqual(chooseReference({ snapshots, mock: null, page: "/", width: 1280, chosen: "s2" }), { type: "none" });
});

test("only a page comment whose image was not saved carries a notice", () => {
  assert.notEqual(imageUnsavedNotice({ image_unsaved: true }), "");
  assert.equal(imageUnsavedNotice({}), "");
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

/**
 * @param {"element" | "arrow" | "pen"} kind
 * @param {string} [selector]
 * @returns {import("./live-model.js").NewPlace}
 */
const place = (kind, selector = "#a") => ({
  kind,
  points: kind === "element" ? [] : [{ x: 1, y: 2 }, { x: 3, y: 4 }],
  elements: [{ selector, text: "", rect: { x: 0, y: 0, w: 10, h: 10 } }],
});

/** @param {import("./live-model.js").PlaceDraft} draft */
const numbers = (draft) => draft.places.map((item) => item.n);

test("places of a comment are numbered from 1 in the order they are added", () => {
  let draft = emptyDraft("/", 390);
  draft = addPlace(draft, place("element", "#a"), "/", 390);
  draft = addPlace(draft, place("arrow"), "/", 390);
  draft = addPlace(draft, place("pen"), "/", 390);
  assert.deepEqual(numbers(draft), [1, 2, 3]);
  assert.deepEqual(draft.places.map((item) => item.kind), ["element", "arrow", "pen"]);
});

/**
 * 場所を 3 つ置き、本文を書いた書きかけ。
 * @param {string} body
 */
const threePlaces = (body) => {
  let draft = emptyDraft("/", 390);
  for (const kind of /** @type {const} */ (["element", "arrow", "pen"])) draft = addPlace(draft, place(kind, `#${kind}`), "/", 390);
  return editBody(draft, body, { start: 0, end: 0 });
};

test("removing a place renumbers the rest from 1 in the order they were put, and the body follows", () => {
  const draft = removePlace(threePlaces("#1 と #3 と 3"), 2);
  assert.deepEqual(numbers(draft), [1, 2]);
  assert.deepEqual(draft.places.map((item) => item.kind), ["element", "pen"]);
  assert.equal(draft.body, "#1 と #2 と 3");
  assert.deepEqual(strayRefs(draft), []);
});

test("a place put after a removal takes the next number in the sequence", () => {
  const draft = addPlace(removePlace(threePlaces(""), 2), place("arrow"), "/", 390);
  assert.deepEqual(numbers(draft), [1, 2, 3]);
});

test("choosing the same element again takes its place away and renumbers the rest", () => {
  let draft = emptyDraft("/", 390);
  draft = addPlace(draft, place("element", "#a"), "/", 390);
  draft = addPlace(draft, place("element", "#b"), "/", 390);
  draft = editBody(draft, "see #2", { start: 0, end: 0 });
  draft = addPlace(draft, place("element", "#a"), "/", 390);
  assert.deepEqual(draft.places.map((item) => [item.n, item.elements[0].selector]), [[1, "#b"]]);
  assert.equal(draft.body, "see #1");
  draft = addPlace(draft, place("element", "#a"), "/", 390);
  assert.deepEqual(numbers(draft), [1, 2]);
});

test("undo renumbers nothing but leaves a reference to the undone place pointing nowhere", () => {
  const draft = undoPlace(threePlaces("#1 #3"));
  assert.deepEqual(numbers(draft), [1, 2]);
  assert.equal(draft.body, "#1 #3");
  assert.deepEqual(strayRefs(draft), [{ start: 3, end: 5, n: 3, removed: true }]);
});

test("a reference to a removed place stays as written and is told apart from the same number after renumbering", () => {
  let draft = emptyDraft("/", 390);
  draft = addPlace(draft, place("element", "#a"), "/", 390);
  draft = addPlace(draft, place("element", "#b"), "/", 390);
  draft = removePlace(editBody(draft, "#1 #2", { start: 0, end: 0 }), 1);
  assert.equal(draft.body, "#1 #1");
  assert.deepEqual(strayRefs(draft), [{ start: 0, end: 2, n: 1, removed: true }]);
});

test("a reference to a removed place moves with text written before it and is no longer stray once edited", () => {
  let draft = removePlace(threePlaces("x #2 y"), 2);
  draft = editBody(draft, "ab x #2 y", { start: 0, end: 0 });
  assert.deepEqual(strayRefs(draft), [{ start: 5, end: 7, n: 2, removed: true }]);
  assert.deepEqual(strayRefs(editBody(draft, "ab x # y", { start: 6, end: 7 })), []);
  assert.deepEqual(strayRefs(editBody(draft, "ab x  y", { start: 5, end: 7 })), []);
});

test("deleting the text before a reference to a removed place keeps that reference stray even when the deleted text looks the same", () => {
  let draft = emptyDraft("/", 390);
  draft = addPlace(draft, place("element", "#a"), "/", 390);
  draft = addPlace(draft, place("element", "#b"), "/", 390);
  draft = removePlace(editBody(draft, "#2 #1", { start: 0, end: 0 }), 1);
  assert.equal(draft.body, "#1 #1");
  assert.deepEqual(strayRefs(draft), [{ start: 3, end: 5, n: 1, removed: true }]);
  draft = editBody(draft, "#1", { start: 0, end: 3 });
  assert.deepEqual(strayRefs(draft), [{ start: 0, end: 2, n: 1, removed: true }]);
});

test("a reference inserted before a reference to a removed place points at its place, and the old one stays stray", () => {
  let draft = emptyDraft("/", 390);
  draft = addPlace(draft, place("element", "#a"), "/", 390);
  draft = addPlace(draft, place("element", "#b"), "/", 390);
  draft = removePlace(editBody(draft, "#1", { start: 0, end: 0 }), 1);
  assert.deepEqual(strayRefs(draft), [{ start: 0, end: 2, n: 1, removed: true }]);
  draft = editBody(draft, "#1 #1", { start: 0, end: 0 });
  assert.deepEqual(strayRefs(draft), [{ start: 3, end: 5, n: 1, removed: true }]);
});

test("the edited span of the text box comes from its selection before the input and its caret after", () => {
  // 選んだ 2..5 に 1 文字を打つ、3 の前を 1 文字消す（Backspace）、3 の後を 1 文字消す（Delete）。
  assert.deepEqual(editedSpan({ start: 2, end: 5 }, 3, -2), { start: 2, end: 5 });
  assert.deepEqual(editedSpan({ start: 3, end: 3 }, 2, -1), { start: 2, end: 3 });
  assert.deepEqual(editedSpan({ start: 3, end: 3 }, 3, -1), { start: 3, end: 4 });
});

test("a reference to a removed place is not rewritten by a later renumbering", () => {
  let draft = removePlace(threePlaces("#2 #3"), 2);
  assert.equal(draft.body, "#2 #2");
  draft = removePlace(draft, 1);
  assert.equal(draft.body, "#2 #1");
  assert.deepEqual(strayRefs(draft), [{ start: 0, end: 2, n: 2, removed: true }]);
});

test("a reference beyond the number of places points nowhere, and a number without # is not a reference", () => {
  const draft = threePlaces("#9 と 9 と #3");
  assert.deepEqual(strayRefs(draft), [{ start: 0, end: 2, n: 9, removed: false }]);
});

test("undo takes away the place added last", () => {
  let draft = emptyDraft("/", 390);
  draft = addPlace(draft, place("element", "#a"), "/", 390);
  draft = addPlace(draft, place("pen"), "/", 390);
  draft = undoPlace(draft);
  assert.deepEqual(numbers(draft), [1]);
  assert.deepEqual(numbers(undoPlace(undoPlace(draft))), []);
});

test("a place at another URL or width is not added to a draft that has places", () => {
  const draft = addPlace(emptyDraft("/", 390), place("element", "#a"), "/", 390);
  assert.deepEqual(addPlace(draft, place("arrow"), "/other", 390), draft);
  assert.deepEqual(addPlace(draft, place("arrow"), "/", 1280), draft);
});

test("a draft without places takes the URL and width of its first place", () => {
  const draft = addPlace(emptyDraft("/", 1280), place("element", "#a"), "/cart", 390);
  assert.deepEqual([draft.url, draft.width, numbers(draft)], ["/cart", 390, [1]]);
});

test("a draft with places is elsewhere at another URL or width", () => {
  const draft = addPlace(emptyDraft("/", 390), place("element", "#a"), "/", 390);
  assert.equal(draftElsewhere(draft, "/", 390), null);
  assert.deepEqual(draftElsewhere(draft, "/other", 390), { url: "/", width: 390 });
  assert.deepEqual(draftElsewhere(draft, "/", 1280), { url: "/", width: 390 });
  assert.equal(draftElsewhere(emptyDraft("/", 390), "/other", 1280), null);
});

test("the start snapshot waits until the saved snapshots are known, and is not taken again after resuming", () => {
  // ページが先に読み込みを知らせても、預けた一覧がまだ分からなければ取らない（live.md の R-PAGE-SESSION の反例）。
  assert.equal(startSnapshotDue({ taken: false, reachable: true, snapshots: null }), false);
  assert.equal(startSnapshotDue({ taken: false, reachable: true, snapshots: [snap("s1", "start")] }), false);
  assert.equal(startSnapshotDue({ taken: false, reachable: true, snapshots: [snap("s1", "manual")] }), true);
  assert.equal(startSnapshotDue({ taken: false, reachable: true, snapshots: [] }), true);
  assert.equal(startSnapshotDue({ taken: false, reachable: false, snapshots: [] }), false);
  assert.equal(startSnapshotDue({ taken: true, reachable: true, snapshots: [] }), false);
});
