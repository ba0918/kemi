import assert from "node:assert/strict";
import { test } from "node:test";

import { diffDescriptions, marksOf, unpackDescription } from "./live-diff.js";

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

test("a description packed by the page reads back as one element per row", () => {
  const packed = {
    width: 390,
    height: 600,
    styles: [{ color: "rgb(0, 0, 0)" }],
    elements: [
      [-1, "html", "", "", "", 0, 0, 390, 600, 0],
      [0, "p", "lead", "intro", "Hello", 0, 10.5, 390, 20, 0],
    ],
  };
  assert.deepEqual(unpackDescription(packed), {
    width: 390,
    height: 600,
    styles: [{ color: "rgb(0, 0, 0)" }],
    elements: [
      { parent: -1, tag: "html", id: "", cls: "", text: "", box: [0, 0, 390, 600], style: 0 },
      { parent: 0, tag: "p", id: "lead", cls: "intro", text: "Hello", box: [0, 10.5, 390, 20], style: 0 },
    ],
  });
});

test("a description that is not in the packed form is not read", () => {
  for (const value of [null, "text", { width: 1, height: 1, styles: [], elements: [{}] }, { width: 1, height: 1, styles: [], elements: [[0, "p"]] }]) {
    assert.equal(unpackDescription(value), null, JSON.stringify(value));
  }
});
