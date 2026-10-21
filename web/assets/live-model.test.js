import assert from "node:assert/strict";
import { test } from "node:test";

import {
  WIDTH_CHOICES,
  addPlace,
  bandNotice,
  buildPageTree,
  draftElsewhere,
  draftSavable,
  editBody,
  editedSpan,
  emptyDraft,
  strayRefs,
  imageUnsavedNotice,
  removePlace,
  undoPlace,
  chooseReference,
  chooseSnapshot,
  commentShortName,
  compareHeading,
  consumeOverscroll,
  glowBox,
  referenceName,
  referenceOptions,
  fitScale,
  overlayPlacement,
  liveOrigin,
  pageKey,
  parseWidth,
  placeSets,
  revealRemoved,
  revealScrollLeft,
  shiftedPlacement,
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

test("the reference choices are auto first, then the assigned mock, and the mock file always last", () => {
  const snapshots = [snap("s1", "start"), snap("s2", "handed")];
  const values = (/** @type {string | null} */ mock) =>
    referenceOptions({ snapshots, page: "/", width: 1280, mock }).map((option) => option.value);
  assert.deepEqual(values("docs/m.html"), ["latest", "s2", "s1", "mock", "mock-file"]);
  assert.deepEqual(values(null), ["latest", "s2", "s1", "mock-file"]);
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

/**
 * 帯の知らせの入力。既定は、何も知らせることの無い広い画面の見る対象だけ。
 * @param {Partial<Parameters<typeof bandNotice>[0]>} [input]
 */
const band = (input = {}) =>
  bandNotice({
    refNotice: "",
    undoMockFailed: null,
    removedMock: null,
    recorded: null,
    chosen: undefined,
    reference: { type: "none" },
    snapshots: [],
    page: "/",
    narrow: false,
    side: "live",
    compare: "now",
    ...input,
  });

/** 手で取って比べる相手を切り替えた直後の知らせの入力（live-compare.md の R-PAGE-REF）。 */
const recordedNow = () => {
  const taken = snap("s2", "manual");
  return {
    snapshots: [snap("s1", "start"), taken],
    recorded: { page: "/", id: "s2" },
    chosen: "s2",
    reference: /** @type {const} */ ({ type: "snapshot", snapshot: taken }),
  };
};

// 帯の文言は仕様が決めていない（画面モックで決める）。何を知らせているかは kind と action と、文に入る値（パスや
// snapshotLabel の名前）で見分け、操作の名前（label・title）は空か空でないかだけを見る。

/**
 * 取ったスナップショットの名前（live-compare.md の R-PAGE-REF: 取った記録を基準に比べていることが読み取れる）。
 * @param {ReturnType<typeof recordedNow>} recorded
 */
const recordedName = (recorded) => snapshotLabel(recorded.snapshots, recorded.reference.snapshot);

test("the band tells one thing: a failure to record, then a refused undo, then a removed mock, then the recorded snapshot", () => {
  const removedMock = { page: "/", path: "m.html" };
  const undoMockFailed = { page: "/", text: "Mock not assigned again · m.html: gone" };
  const recorded = recordedNow();
  const all = { ...recorded, removedMock, undoMockFailed, refNotice: "Not recorded: the page is not loaded" };
  const failed = band(all);
  assert.deepEqual([failed.text, failed.action], ["Not recorded: the page is not loaded", null]);
  const refused = band({ ...all, refNotice: "" });
  assert.deepEqual([refused.text, refused.action], ["Mock not assigned again · m.html: gone", null]);
  const removed = band({ ...all, refNotice: "", undoMockFailed: null });
  assert.equal(removed.action, "undo-mock");
  assert.ok(removed.text.includes("m.html"), removed.text);
  const taken = band({ ...all, refNotice: "", undoMockFailed: null, removedMock: null });
  assert.equal(taken.action, "show-recorded");
  assert.ok(taken.text.includes(recordedName(recorded)), taken.text);
});

test("a snapshot that was not saved is told on the band only when nothing else is", () => {
  const unsaved = { ...snap("s1", "manual"), unsaved: true };
  const reference = /** @type {const} */ ({ type: "snapshot", snapshot: unsaved });
  assert.deepEqual(band({ reference, snapshots: [unsaved] }), {
    text: unsavedSnapshotNotice(unsaved),
    kind: "waiting",
    label: "",
    title: "",
    action: null,
  });
  assert.equal(band({ reference, snapshots: [unsaved], refNotice: "Not recorded: x" }).text, "Not recorded: x");
  assert.deepEqual(band(), { text: "", kind: "waiting", label: "", title: "", action: null });
});

test("a removed mock is told with an undo, and a refused undo without one, only on the page they belong to", () => {
  const removed = band({ removedMock: { page: "/", path: "m.html" } });
  assert.deepEqual([removed.kind, removed.action], ["done", "undo-mock"]);
  assert.ok(removed.text.includes("m.html"), removed.text);
  assert.notEqual(removed.label, "");
  assert.notEqual(removed.title, "");
  assert.equal(band({ removedMock: { page: "/other", path: "m.html" } }).text, "");
  assert.deepEqual(band({ undoMockFailed: { page: "/", text: "refused" } }), {
    text: "refused",
    kind: "waiting",
    label: "",
    title: "",
    action: null,
  });
  assert.equal(band({ undoMockFailed: { page: "/other", text: "refused" } }).text, "");
});

test("the recorded snapshot is told only while it is still the chosen reference of the page", () => {
  const recorded = recordedNow();
  assert.equal(band(recorded).kind, "done");
  assert.equal(band({ ...recorded, page: "/other" }).text, "");
  assert.equal(band({ ...recorded, chosen: "s1" }).text, "");
  assert.equal(band({ ...recorded, chosen: undefined }).text, "");
  assert.equal(band({ ...recorded, reference: { type: "none" } }).text, "");
  assert.equal(band({ ...recorded, reference: { type: "snapshot", snapshot: snap("s1", "start") } }).text, "");
});

test("the recorded snapshot carries an action to see it only while it is not shown", () => {
  const recorded = recordedNow();
  /** @param {ReturnType<typeof band>} notice */
  const shown = (notice) => {
    assert.equal(notice.kind, "done");
    assert.ok(notice.text.includes(recordedName(recorded)), notice.text);
    return notice;
  };
  // 広い画面では見る対象だけのときだけ、並べる見比べ方へ切り替える操作を付ける。
  const wide = shown(band({ ...recorded, compare: "now" }));
  assert.equal(wide.action, "show-recorded");
  assert.notEqual(wide.label, "");
  assert.notEqual(wide.title, "");
  for (const compare of /** @type {const} */ (["side", "overlay"])) {
    const seen = shown(band({ ...recorded, compare }));
    assert.deepEqual([seen.label, seen.title, seen.action], ["", "", null], compare);
  }
  // 狭い画面では見比べ方によらず、動いているページの側を見ている間だけ、比べる相手の 1 枚にする操作を付ける。
  for (const compare of /** @type {const} */ (["now", "side", "overlay"])) {
    const onLive = shown(band({ ...recorded, narrow: true, side: "live", compare }));
    assert.equal(onLive.action, "show-recorded", compare);
    assert.notEqual(onLive.label, "", compare);
    // 広い画面と狭い画面では切り替え先が違う（並べる・比べる相手の 1 枚）ので、何をするかの説明も違う。
    assert.notEqual(onLive.title, "", compare);
    assert.notEqual(onLive.title, wide.title, compare);
    const onRef = shown(band({ ...recorded, narrow: true, side: "ref", compare }));
    assert.deepEqual([onRef.label, onRef.title, onRef.action], ["", "", null], compare);
  }
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

test("the name of a saved comment written with emoji has no broken character", () => {
  // 絵文字は 2 つの符号単位で書かれる。名前を切る位置が偶数でも奇数でも 1 文字を割らないよう、前に置く文字の数を変える。
  for (const before of ["", "a"]) {
    const name = commentShortName(`${before}${"😀".repeat(500)}`);
    assert.doesNotMatch(name, /\p{Cs}/u, `a lone surrogate after ${JSON.stringify(before)}`);
  }
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

test("a draft is saved only with a place, some text, no stray reference, and at its own URL and width", () => {
  let draft = addPlace(emptyDraft("/", 390), place("element", "#a"), "/", 390);
  draft = editBody(draft, "see #1", { start: 0, end: 0 });
  assert.equal(draftSavable(draft, "/", 390), true);
  // 場所が無い、本文が空白だけ、どの場所も指さない `#n` がある（R-PAGE-COMMENT）。
  assert.equal(draftSavable({ ...editBody(draft, "looks off", { start: 0, end: draft.body.length }), places: [] }, "/", 390), false);
  assert.equal(draftSavable(editBody(draft, " \n ", { start: 0, end: draft.body.length }), "/", 390), false);
  assert.equal(draftSavable(editBody(draft, "see #2", { start: 5, end: 6 }), "/", 390), false);
  assert.equal(draftSavable(removePlace(addPlace(draft, place("pen"), "/", 390), 2), "/", 390), true);
  assert.equal(draftSavable(editBody(addPlace(draft, place("pen"), "/", 390), "see #2", { start: 5, end: 6 }), "/", 390), true);
  assert.equal(draftSavable(removePlace(editBody(addPlace(draft, place("pen"), "/", 390), "see #2", { start: 5, end: 6 }), 2), "/", 390), false);
  // 保存は書きかけの URL と表示幅に戻ってから。
  assert.equal(draftSavable(draft, "/other", 390), false);
  assert.equal(draftSavable(draft, "/", 1280), false);
});

test("saved places are drawn only at their URL and width, numbered only while their thread is open, and the draft comes last", () => {
  /** @param {string} id @param {string} url @param {number} width */
  const saved = (id, url, width) => ({ id, page: { url, width, places: [{ n: 1, id }] } });
  const comments = [saved("c1", "/", 390), saved("c2", "/", 1280), saved("c3", "/cart", 390), saved("c4", "/", 390)];
  const draft = addPlace(emptyDraft("/", 390), place("element", "#a"), "/", 390);
  // 保存したコメントの場所は、スレッドを開いている間だけ番号付きで、それ以外は番号の無い小さな印（R-PAGE-COMMENT）。
  assert.deepEqual(placeSets({ comments, draft, page: "/", width: 390, thread: "c4" }), [
    { places: [{ n: 1, id: "c1" }], look: "saved", comment: "c1" },
    { places: [{ n: 1, id: "c4" }], look: "focus", comment: "c4" },
    { places: draft.places, look: "draft" },
  ]);
  assert.deepEqual(placeSets({ comments, draft, page: "/", width: 1280, thread: null }), [
    { places: [{ n: 1, id: "c2" }], look: "saved", comment: "c2" },
  ]);
  assert.deepEqual(placeSets({ comments, draft: emptyDraft("/", 390), page: "/cart", width: 390, thread: null }), [
    { places: [{ n: 1, id: "c3" }], look: "saved", comment: "c3" },
  ]);
  assert.deepEqual(placeSets({ comments: [], draft: emptyDraft("/", 390), page: "/", width: 390, thread: null }), []);
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

test("a box already inside the frame keeps the horizontal scroll", () => {
  assert.equal(revealScrollLeft({ left: 300, width: 100, scrollLeft: 200, viewportWidth: 500 }), 200);
});

test("a box outside the frame on either side is brought to the middle of the frame", () => {
  assert.equal(revealScrollLeft({ left: 1000, width: 100, scrollLeft: 0, viewportWidth: 500 }), 800);
  assert.equal(revealScrollLeft({ left: 100, width: 100, scrollLeft: 600, viewportWidth: 500 }), 0);
});

test("a box wider than the frame is shown from its left edge", () => {
  assert.equal(revealScrollLeft({ left: 900, width: 800, scrollLeft: 0, viewportWidth: 500 }), 900);
});

/** 消えた要素の行を押したときの入力。枠の高さ 500、ページとスナップショットの中身は 3000。 */
const removedAt = (/** @type {Partial<Parameters<typeof revealRemoved>[0]>} */ input) =>
  revealRemoved({ top: 1000, height: 100, viewHeight: 500, pageHeight: 3000, snapshotHeight: 3000, overlaid: false, ...input });

test("a removed element is shown in the middle of the frame, and one taller than the frame from its top", () => {
  assert.deepEqual(removedAt({}), { y: 800, overscroll: false });
  assert.deepEqual(removedAt({ top: 1000, height: 800 }), { y: 1000, overscroll: false });
  // 文書の上端より上には動かさない。
  assert.deepEqual(removedAt({ top: 100 }), { y: 0, overscroll: false });
});

test("a shifted snapshot is moved no further than its end at the bottom of the frame, and not at all when shorter than the frame", () => {
  assert.deepEqual(removedAt({ top: 2900 }), { y: 2500, overscroll: false });
  assert.deepEqual(removedAt({ top: 200, snapshotHeight: 300 }), { y: 0, overscroll: false });
});

test("while overlaid, the page is scrolled to the removed element, and moved beyond its end only when it cannot scroll that far", () => {
  // ページは 3000 - 500 = 2500 までスクロールできる（live-compare.md の DC2: そろえたまま見せる）。
  assert.deepEqual(removedAt({ overlaid: true, top: 2700 }), { y: 2500, overscroll: false });
  assert.deepEqual(removedAt({ overlaid: true, top: 2701 }), { y: 2501, overscroll: true });
  // スナップショットの高さでは収めない（重ねた比べる相手は外側でずらす）。
  assert.deepEqual(removedAt({ overlaid: true, top: 2900, snapshotHeight: 300 }), { y: 2700, overscroll: true });
  // 今のページが枠より短いと、少しでも下ならずらす。
  assert.deepEqual(removedAt({ overlaid: true, top: 400, pageHeight: 300 }), { y: 200, overscroll: true });
  assert.deepEqual(removedAt({ overlaid: true, top: 100, pageHeight: 300 }), { y: 0, overscroll: false });
});

test("a shifted snapshot is drawn at its full height and moved up by the shift, kept between its top and its end", () => {
  assert.deepEqual(shiftedPlacement({ y: 800, viewHeight: 500, snapshotHeight: 3000, scale: 0.5 }), {
    y: 800,
    height: 3000,
    transform: "translate(0px, -400px) scale(0.5)",
    origin: { x: 0, y: -400 },
  });
  assert.equal(shiftedPlacement({ y: 9000, viewHeight: 500, snapshotHeight: 3000, scale: 1 }).y, 2500);
  const atTop = shiftedPlacement({ y: -40, viewHeight: 500, snapshotHeight: 3000, scale: 1 });
  assert.deepEqual([atTop.y, atTop.transform], [0, "translate(0px, 0px) scale(1)"]);
  assert.deepEqual(shiftedPlacement({ y: 100, viewHeight: 500, snapshotHeight: 300, scale: 1 }).height, 500);
  assert.equal(shiftedPlacement({ y: 100, viewHeight: 500, snapshotHeight: 300, scale: 1 }).y, 0);
});

test("the glow on the snapshot surrounds the scaled element box a little apart, where the snapshot is drawn", () => {
  assert.deepEqual(glowBox({ origin: { x: -20, y: -400 }, box: [100, 1000, 60, 30], scale: 0.5 }), {
    left: 26,
    top: 96,
    width: 38,
    height: 23,
  });
});

test("an upward wheel first takes back the move beyond the end of the page, and sends only the rest to the page", () => {
  assert.deepEqual(consumeOverscroll({ deltaY: -40, beyond: 100, y: 2600 }), { deltaY: 0, y: 2560 });
  assert.deepEqual(consumeOverscroll({ deltaY: -150, beyond: 100, y: 2600 }), { deltaY: -50, y: null });
  assert.deepEqual(consumeOverscroll({ deltaY: -100, beyond: 100, y: 2600 }), { deltaY: 0, y: null });
  // 下へのホイールと、ずらしていない間は、そのままページに送る。
  assert.equal(consumeOverscroll({ deltaY: 40, beyond: 100, y: 2600 }), null);
  assert.equal(consumeOverscroll({ deltaY: -40, beyond: 0, y: 2600 }), null);
});
