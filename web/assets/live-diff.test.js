import assert from "node:assert/strict";
import { test } from "node:test";

import { changesByElement, diffDescriptions, marksOf } from "./live-diff.js";

/**
 * @typedef {{ tag: string, id?: string, cls?: string, text?: string, box: number[], style?: Record<string, string>, children?: Spec[] }} Spec
 */

const PLAIN = { color: "rgb(0, 0, 0)", "background-color": "rgba(0, 0, 0, 0)", "font-size": "16px", "border-radius": "0px" };

/**
 * 入れ子で書いた要素から、ページが返す形の記述を作る（文書の順、親の番号、スタイルの表）。
 * @param {Spec} root
 * @returns {import("./live-diff.js").Description}
 */
function describe(root) {
  /** @type {import("./live-diff.js").Description} */
  const description = { width: 1280, height: 800, styles: [], elements: [] };
  /** @type {Map<string, number>} */
  const styles = new Map();
  /** @param {Spec} spec @param {number} parent */
  const add = (spec, parent) => {
    const style = { ...PLAIN, ...spec.style };
    const key = JSON.stringify(style);
    if (!styles.has(key)) {
      styles.set(key, description.styles.length);
      description.styles.push(style);
    }
    const index = description.elements.length;
    description.elements.push({
      parent,
      tag: spec.tag,
      id: spec.id ?? "",
      cls: spec.cls ?? "",
      text: spec.text ?? "",
      box: spec.box,
      style: /** @type {number} */ (styles.get(key)),
    });
    for (const child of spec.children ?? []) {
      add(child, index);
    }
  };
  add(root, -1);
  return description;
}

/**
 * html > body > 子の並び。body と html は子に合わせて高さが変わる。
 * @param {Spec[]} children
 */
function page(children) {
  const height = Math.max(0, ...children.map((child) => child.box[1] + child.box[3]));
  return describe({
    tag: "html",
    box: [0, 0, 1280, height],
    children: [{ tag: "body", box: [0, 0, 1280, height], children }],
  });
}

/** @param {string[]} items @param {number} [top] */
function list(items, top = 0) {
  return {
    tag: "ul",
    id: "list",
    box: [0, top, 1280, items.length * 20],
    children: items.map((text, index) => ({ tag: "li", cls: "item", text, box: [0, top + index * 20, 1280, 20] })),
  };
}

/**
 * @param {import("./live-diff.js").Description} description
 * @param {string} text
 */
function indexOfText(description, text) {
  return description.elements.findIndex((element) => element.text === text);
}

/** @param {import("./live-diff.js").Change[]} changes @param {string} kind */
const ofKind = (changes, kind) => changes.filter((change) => change.kind === kind);

test("an element inserted in the middle of its siblings is the only one added, and the siblings after it only shift", () => {
  const before = page([list(["one", "two", "three", "four"])]);
  const now = page([list(["one", "two", "inserted", "three", "four"])]);
  const changes = diffDescriptions(before, now);
  assert.deepEqual(
    ofKind(changes, "added").map((change) => change.now),
    [indexOfText(now, "inserted")],
  );
  assert.deepEqual(ofKind(changes, "removed"), []);
  assert.deepEqual(ofKind(changes, "text"), []);
  assert.deepEqual(ofKind(changes, "visual"), []);
  const shifted = ofKind(changes, "shifted").map((change) => change.now);
  assert.ok(shifted.includes(indexOfText(now, "three")));
  assert.ok(shifted.includes(indexOfText(now, "four")));
});

test("among siblings that differ only in appearance, an element inserted in the middle is the only one added", () => {
  /** @param {string[]} colors */
  const buttons = (colors) =>
    page(
      colors.map((color, index) => ({
        tag: "button",
        cls: "buy",
        text: "Buy",
        box: [0, index * 40, 100, 40],
        style: { "background-color": color },
      })),
    );
  const before = buttons(["rgb(255, 0, 0)", "rgb(0, 0, 255)", "rgb(0, 128, 0)"]);
  const now = buttons(["rgb(255, 0, 0)", "rgb(255, 255, 0)", "rgb(0, 0, 255)", "rgb(0, 128, 0)"]);
  const changes = diffDescriptions(before, now);
  assert.deepEqual(
    changes.filter((change) => change.kind !== "shifted").map((change) => [change.kind, change.now]),
    [["added", 3]],
  );
});

test("an element removed from the middle of its siblings is the only one removed", () => {
  const before = page([list(["one", "two", "gone", "three", "four"])]);
  const now = page([list(["one", "two", "three", "four"])]);
  const changes = diffDescriptions(before, now);
  assert.deepEqual(
    ofKind(changes, "removed").map((change) => change.before),
    [indexOfText(before, "gone")],
  );
  assert.deepEqual(ofKind(changes, "added"), []);
  assert.deepEqual(ofKind(changes, "text"), []);
  assert.deepEqual(ofKind(changes, "visual"), []);
});

test("elements with an id are paired by their id when their order changes", () => {
  const red = { "background-color": "rgb(255, 0, 0)" };
  const blue = { "background-color": "rgb(0, 0, 255)" };
  const before = page([
    { tag: "div", id: "a", box: [0, 0, 100, 50], style: red },
    { tag: "div", id: "b", box: [0, 50, 100, 50], style: blue },
  ]);
  const now = page([
    { tag: "div", id: "b", box: [0, 0, 100, 50], style: blue },
    { tag: "div", id: "a", box: [0, 50, 100, 50], style: red },
  ]);
  const changes = diffDescriptions(before, now);
  assert.deepEqual(changes.filter((change) => change.kind !== "shifted"), []);
  const pairs = ofKind(changes, "shifted").map((change) => [change.before, change.now]);
  assert.deepEqual(pairs.filter(([index]) => index !== null && index >= 2).sort(), [[2, 3], [3, 2]]);
});

test("elements with different ids are not paired even in the same place", () => {
  const before = page([{ tag: "div", id: "old", box: [0, 0, 100, 50] }]);
  const now = page([{ tag: "div", id: "new", box: [0, 0, 100, 50] }]);
  const changes = diffDescriptions(before, now);
  assert.deepEqual(ofKind(changes, "removed").map((change) => change.before), [2]);
  assert.deepEqual(ofKind(changes, "added").map((change) => change.now), [2]);
});

test("an element whose background color alone changed is a main change carrying both colors", () => {
  const before = page([{ tag: "button", text: "Buy", box: [0, 0, 120, 40], style: { "background-color": "rgb(49, 89, 214)" } }]);
  const now = page([{ tag: "button", text: "Buy", box: [0, 0, 120, 40], style: { "background-color": "rgb(214, 69, 69)" } }]);
  const changes = diffDescriptions(before, now);
  assert.deepEqual(
    ofKind(changes, "visual").map(({ before: from, now: to, property, was, is }) => ({ from, to, property, was, is })),
    [{ from: 2, to: 2, property: "background-color", was: "rgb(49, 89, 214)", is: "rgb(214, 69, 69)" }],
  );
  assert.deepEqual(changes.filter((change) => change.kind !== "visual"), []);
});

test("when one element grows, the element below it only shifts and is not a main change", () => {
  const before = page([
    { tag: "div", cls: "hero", text: "Sale", box: [0, 0, 1280, 100], style: { "font-size": "16px" } },
    { tag: "p", text: "Below", box: [0, 100, 1280, 20] },
  ]);
  const now = page([
    { tag: "div", cls: "hero", text: "Sale", box: [0, 0, 1280, 150], style: { "font-size": "32px" } },
    { tag: "p", text: "Below", box: [0, 150, 1280, 20] },
  ]);
  const changes = diffDescriptions(before, now);
  const below = indexOfText(now, "Below");
  assert.ok(ofKind(changes, "shifted").some((change) => change.now === below));
  assert.deepEqual(
    changes.filter((change) => change.kind !== "shifted").map((change) => [change.kind, change.now]),
    [["visual", indexOfText(now, "Sale")]],
  );
});

test("a changed text is a main change", () => {
  const before = page([{ tag: "p", cls: "price", text: "$89", box: [0, 0, 100, 20] }]);
  const now = page([{ tag: "p", cls: "price", text: "$79", box: [0, 0, 100, 20] }]);
  const changes = diffDescriptions(before, now);
  assert.deepEqual(changes.map((change) => change.kind), ["text"]);
});

test("two descriptions of the same page have no changes", () => {
  const description = page([list(["one", "two"]), { tag: "p", text: "end", box: [0, 40, 100, 20] }]);
  assert.deepEqual(diffDescriptions(description, structuredClone(description)), []);
});

test("every changed element gets one mark: main changes on the page, removed ones on the snapshot, shifted ones apart", () => {
  const before = page([
    { tag: "p", cls: "price", text: "$89", box: [0, 0, 100, 20], style: { color: "rgb(200, 0, 0)" } },
    { tag: "p", text: "gone", box: [0, 20, 100, 20] },
    { tag: "p", text: "stays", box: [0, 40, 100, 20] },
  ]);
  const now = page([
    { tag: "p", cls: "price", text: "$79", box: [0, 0, 100, 20], style: { color: "rgb(0, 0, 200)" } },
    { tag: "div", text: "new", box: [0, 20, 100, 30] },
    { tag: "p", text: "stays", box: [0, 50, 100, 20] },
  ]);
  const marks = marksOf(diffDescriptions(before, now));
  const price = indexOfText(now, "$79");
  const added = indexOfText(now, "new");
  const stays = indexOfText(now, "stays");
  assert.deepEqual(
    marks.now.filter((mark) => [price, added, stays].includes(mark.index)).sort((x, y) => x.index - y.index),
    [
      { index: price, kind: "main" },
      { index: added, kind: "added" },
      { index: stays, kind: "shifted" },
    ],
  );
  assert.deepEqual(marks.before, [{ index: indexOfText(before, "gone"), kind: "removed" }]);
});

test("the color, background and border of one button changed together make one element with three changes", () => {
  const button = (/** @type {Record<string, string>} */ style) =>
    page([{ tag: "button", id: "buy", text: "Buy now", box: [0, 0, 120, 40], style }]);
  const before = button({ color: "rgb(0, 0, 0)", "background-color": "rgb(49, 89, 214)", "border-color": "rgb(0, 0, 0)" });
  const now = button({ color: "rgb(255, 255, 255)", "background-color": "rgb(214, 69, 69)", "border-color": "rgb(255, 0, 0)" });
  const elements = changesByElement(diffDescriptions(before, now));
  assert.deepEqual(
    elements.map(({ kind, side, index, tag, excerpt, label }) => ({ kind, side, index, tag, excerpt, label })),
    [{ kind: "main", side: "now", index: 2, tag: "button", excerpt: "Buy now", label: "button#buy" }],
  );
  assert.deepEqual(
    elements[0].changes.map((change) => change.property).sort(),
    ["background-color", "border-color", "color"],
  );
});

test("a removed element and another element now at the same number stay apart", () => {
  const before = page([list(["one", "two", "gone", "three", "four"])]);
  const now = page([list(["one", "two", "three", "four"])]);
  const gone = indexOfText(before, "gone");
  assert.equal(indexOfText(now, "three"), gone, "the element after the removed one takes its number");
  const elements = changesByElement(diffDescriptions(before, now));
  const removed = elements.filter((element) => element.kind === "removed");
  assert.deepEqual(
    removed.map(({ side, index, excerpt }) => ({ side, index, excerpt })),
    [{ side: "before", index: gone, excerpt: "gone" }],
  );
  const three = elements.find((element) => element.side === "now" && element.index === gone);
  assert.equal(three?.kind, "shifted");
  assert.equal(three?.excerpt, "three");
});

test("an added and a removed element are one element each", () => {
  const before = page([{ tag: "p", text: "gone", box: [0, 0, 100, 20] }]);
  const now = page([{ tag: "div", text: "new", box: [0, 0, 100, 20] }]);
  const elements = changesByElement(diffDescriptions(before, now));
  assert.deepEqual(
    elements.map(({ kind, side, index, tag, excerpt }) => ({ kind, side, index, tag, excerpt })),
    [
      { kind: "added", side: "now", index: 2, tag: "div", excerpt: "new" },
      { kind: "removed", side: "before", index: 2, tag: "p", excerpt: "gone" },
    ],
  );
});

test("html and body are not listed as shifted when the whole page moves", () => {
  const before = page([list(["one", "two", "three", "four"])]);
  const now = page([list(["one", "two", "inserted", "three", "four"])]);
  const tags = ofKind(diffDescriptions(before, now), "shifted").map((change) => now.elements[/** @type {number} */ (change.now)].tag);
  assert.ok(tags.length > 0);
  assert.ok(!tags.includes("html") && !tags.includes("body"), JSON.stringify(tags));
});

test("a changed background of body is a main change", () => {
  const before = describe({ tag: "html", box: [0, 0, 1280, 100], children: [{ tag: "body", box: [0, 0, 1280, 100] }] });
  const now = describe({
    tag: "html",
    box: [0, 0, 1280, 100],
    children: [{ tag: "body", box: [0, 0, 1280, 100], style: { "background-color": "rgb(255, 250, 230)" } }],
  });
  const elements = changesByElement(diffDescriptions(before, now));
  assert.deepEqual(
    elements.map(({ kind, tag }) => ({ kind, tag })),
    [{ kind: "main", tag: "body" }],
  );
});
