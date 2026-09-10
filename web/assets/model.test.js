import assert from "node:assert/strict";
import { test } from "node:test";

import {
  buildTree,
  collapseDefault,
  commentLabel,
  draftKey,
  filterAndSortFiles,
  formatBytes,
  isDarkTheme,
  keyAction,
  lineAnchor,
  lineOffsets,
  metaItems,
  nextHighlightOverride,
  nextTheme,
  placeThreads,
  replaceComment,
  resolveTheme,
  reviewStats,
  statusLabel,
  suggestionAllowed,
  toDisplayLines,
  windowFor,
} from "./model.js";

/** @typedef {import("./model.js").LogicalRow} LogicalRow */
/** @typedef {import("./model.js").FileEntry} FileEntry */

/**
 * @param {number} number
 * @param {string} text
 * @returns {import("./model.js").LogicalRow}
 */
function equal(number, text) {
  return {
    kind: "equal",
    old: { number, text },
    new: { number, text },
    old_segments: [],
    new_segments: [],
  };
}

/**
 * @param {string} oldText
 * @param {string} newText
 * @returns {import("./model.js").LogicalRow}
 */
function replace(oldText, newText) {
  return {
    kind: "replace",
    old: { number: 5, text: oldText },
    new: { number: 5, text: newText },
    old_segments: [{ text: oldText, changed: true }],
    new_segments: [{ text: newText, changed: true }],
  };
}

/**
 * @param {number} number
 * @param {string} text
 * @returns {import("./model.js").LogicalRow}
 */
function deleted(number, text) {
  return {
    kind: "delete",
    old: { number, text },
    new: null,
    old_segments: [],
    new_segments: [],
  };
}

/**
 * @param {number} number
 * @param {string} text
 * @returns {import("./model.js").LogicalRow}
 */
function inserted(number, text) {
  return {
    kind: "insert",
    old: null,
    new: { number, text },
    old_segments: [],
    new_segments: [],
  };
}

/**
 * @param {number} count
 * @returns {import("./model.js").LogicalRow}
 */
function skip(count) {
  return {
    kind: "skip",
    count,
    from: 3,
    to: 3 + count,
    old_start: 4,
    new_start: 4,
  };
}

/**
 * @param {string} path
 * @param {number} add
 * @param {number} del
 * @param {Partial<FileEntry>} [extra]
 * @returns {FileEntry}
 */
function file(path, add, del, extra = {}) {
  return {
    id: `f-${path}`,
    path,
    old_path: null,
    status: "modify",
    add,
    del,
    binary: false,
    old_size: 0,
    new_size: 0,
    focus: false,
    note: "",
    noise: false,
    seen: false,
    collapsed: false,
    ...extra,
  };
}

test("toDisplayLines_unified_expands_replace_and_keeps_skip", () => {
  const rows = [equal(1, "a"), replace("old", "new"), deleted(6, "gone"), inserted(6, "added"), skip(7)];
  const lines = toDisplayLines(rows, "unified");

  assert.deepEqual(
    lines.map((line) => line.kind),
    ["equal", "replace-old", "replace-new", "delete", "insert", "skip"],
  );
  assert.equal(lines[0].logicalIndex, 0);
  assert.equal(lines[5].logicalIndex, 4);
});

test("toDisplayLines_split_keeps_one_line_per_logical_row", () => {
  const rows = [equal(1, "a"), replace("old", "new"), deleted(6, "gone"), skip(7)];
  const lines = toDisplayLines(rows, "split");

  assert.deepEqual(
    lines.map((line) => line.kind),
    ["equal", "replace", "delete", "skip"],
  );
  assert.equal(lines[1].oldLine?.text, "old");
  assert.equal(lines[1].newLine?.text, "new");
  assert.equal(lines[2].newLine, null);
});

test("lineOffsets_and_windowFor_use_fixed_heights", () => {
  const heights = new Array(100).fill(20);
  const offsets = lineOffsets(heights);
  assert.equal(offsets.length, 101);
  assert.equal(offsets[10], 200);
  assert.equal(offsets[100], 2000);

  const window = windowFor(offsets, 250, 100, 2);
  assert.deepEqual(window, { start: 10, end: 20 });
});

test("windowFor_clamps_at_both_ends", () => {
  const offsets = lineOffsets(new Array(10).fill(20));

  assert.deepEqual(windowFor(offsets, -50, 100, 2), { start: 0, end: 7 });
  assert.deepEqual(windowFor(offsets, 190, 100, 2), { start: 7, end: 10 });
});

test("windowFor_supports_varying_heights", () => {
  const heights = [100, 20, 20, 20, 100, 20];
  const offsets = lineOffsets(heights);

  const window = windowFor(offsets, 110, 60, 0);
  assert.deepEqual(window, { start: 1, end: 5 });
});

test("keyAction_moves_between_files_and_toggles_view", () => {
  assert.deepEqual(keyAction("j", "unified", 3, 0), { type: "file", index: 1 });
  assert.deepEqual(keyAction("k", "unified", 3, 0), { type: "file", index: 0 });
  assert.deepEqual(keyAction("s", "unified", 3, 0), { type: "mode", mode: "split" });
  assert.deepEqual(keyAction("u", "split", 3, 0), { type: "mode", mode: "unified" });
  assert.deepEqual(keyAction("w", "unified", 3, 0), { type: "wrap", value: true });
  assert.deepEqual(keyAction("x", "unified", 3, 0), { type: "none" });
});

test("filterAndSortFiles_keeps_input_order_by_default", () => {
  const files = [file("b.rs", 5, 5), file("a.rs", 1, 1), file("c.rs", 3, 3)];
  const result = filterAndSortFiles(files, { focusOnly: false, sortBySize: false });

  assert.deepEqual(
    result.map((entry) => entry.path),
    ["b.rs", "a.rs", "c.rs"],
  );
});

test("filterAndSortFiles_sorts_by_change_size_when_asked", () => {
  const files = [file("b.rs", 5, 5), file("a.rs", 1, 1), file("c.rs", 30, 30)];
  const result = filterAndSortFiles(files, { focusOnly: false, sortBySize: true });

  assert.deepEqual(
    result.map((entry) => entry.path),
    ["c.rs", "b.rs", "a.rs"],
  );
});

test("filterAndSortFiles_keeps_only_focus_when_asked", () => {
  const files = [
    file("a.rs", 1, 1),
    file("b.rs", 2, 2, { focus: true }),
    file("c.rs", 3, 3, { focus: true }),
  ];
  const result = filterAndSortFiles(files, { focusOnly: true, sortBySize: false });

  assert.deepEqual(
    result.map((entry) => entry.path),
    ["b.rs", "c.rs"],
  );
});

test("collapseDefault_uses_server_state_and_local_override", () => {
  // サーバが返す collapsed は「ノイズの既定」か「前回開いた状態」の結果。
  const noisy = file("Cargo.lock", 1, 1, { noise: true, collapsed: true });
  assert.equal(collapseDefault(noisy, {}), true);
  assert.equal(collapseDefault(noisy, { [noisy.id]: false }), false);
  const opened = file("Cargo.lock", 1, 1, { noise: true, collapsed: false });
  assert.equal(collapseDefault(opened, {}), false);
  assert.equal(collapseDefault(file("a.rs", 1, 1), {}), false);
});

test("statusLabel_translates_status", () => {
  assert.equal(statusLabel("add"), "追加");
  assert.equal(statusLabel("delete"), "削除");
  assert.equal(statusLabel("rename"), "改名");
  assert.equal(statusLabel("modify"), "変更");
});

test("formatBytes_uses_binary_units", () => {
  assert.equal(formatBytes(0), "0 B");
  assert.equal(formatBytes(512), "512 B");
  assert.equal(formatBytes(2048), "2.0 KiB");
  assert.equal(formatBytes(5 * 1024 * 1024), "5.0 MiB");
});

test("commentLabel_marks_file_wide_and_ranges", () => {
  assert.equal(commentLabel({ side: "new", start_line: null, end_line: null }), "ファイル全体");
  assert.equal(commentLabel({ side: "new", start_line: 3, end_line: 3 }), "新側 3");
  assert.equal(commentLabel({ side: "old", start_line: 4, end_line: 6 }), "旧側 4–6");
});

test("suggestionAllowed_only_on_new_side", () => {
  assert.equal(suggestionAllowed("new"), true);
  assert.equal(suggestionAllowed("old"), false);
});

test("nextHighlightOverride_toggles_one_file_at_a_time", () => {
  // 有効なら off、上限内なら既定に戻し、上限超過ならそのファイルだけ on。
  assert.equal(nextHighlightOverride(true, false), "off");
  assert.equal(nextHighlightOverride(true, true), "off");
  assert.equal(nextHighlightOverride(false, true), null);
  assert.equal(nextHighlightOverride(false, false), "on");
});

test("draftKey_separates_file_and_range", () => {
  assert.equal(draftKey("f1", null), "kemi-draft:f1:file");
  assert.equal(
    draftKey("f1", { side: "new", start: 3, end: 5 }),
    "kemi-draft:f1:new:3-5",
  );
  assert.notEqual(
    draftKey("f1", { side: "new", start: 3, end: 5 }),
    draftKey("f2", { side: "new", start: 3, end: 5 }),
  );
});

test("replaceComment_swaps_only_the_matching_id", () => {
  const first = { id: "c1", body: "a" };
  const second = { id: "c2", body: "b" };
  const updated = { id: "c1", body: "a", replies: ["r"] };

  assert.deepEqual(replaceComment([first, second], updated), [updated, second]);
});

/**
 * @param {FileEntry} file
 * @param {{ id: string, title: string, why: string, watch: string }} [group]
 * @returns {{ file: FileEntry, group: any }}
 */
function entry(file, group = { id: "g1", title: "g1", why: "", watch: "" }) {
  return { file, group };
}

test("buildTree_separates_groups_and_nests_directories", () => {
  const tree = buildTree([
    entry(file("src/a.rs", 1, 1)),
    entry(file("src/sub/b.rs", 2, 2)),
    entry(file("README.md", 3, 3)),
    entry(file("src/c.rs", 4, 4), { id: "g2", title: "g2", why: "", watch: "" }),
  ]);

  assert.equal(tree.length, 2);
  assert.deepEqual(
    tree[0].nodes.map((node) => node.type),
    ["dir", "file"],
  );
  const src = /** @type {import("./model.js").DirNode} */ (tree[0].nodes[0]);
  assert.equal(src.type, "dir");
  assert.equal(src.path, "src");
  assert.deepEqual(
    src.children.map((node) => node.name),
    ["a.rs", "sub"],
  );
  const sub = /** @type {import("./model.js").DirNode} */ (src.children[1]);
  assert.equal(sub.type, "dir");
  assert.equal(sub.path, "src/sub");
  assert.deepEqual(
    sub.children.map((node) => node.name),
    ["b.rs"],
  );
  assert.equal(tree[0].nodes[1].name, "README.md");
  assert.equal(tree[1].group.id, "g2");
  assert.deepEqual(
    tree[1].nodes.map((node) => node.name),
    ["src"],
  );
  const secondSrc = /** @type {import("./model.js").DirNode} */ (tree[1].nodes[0]);
  assert.deepEqual(
    secondSrc.children.map((node) => node.name),
    ["c.rs"],
  );
});

test("buildTree_keeps_input_order_within_a_directory", () => {
  const tree = buildTree([
    entry(file("src/z.rs", 1, 1)),
    entry(file("src/a.rs", 2, 2)),
  ]);

  const src = /** @type {import("./model.js").DirNode} */ (tree[0].nodes[0]);
  assert.deepEqual(
    src.children.map((node) => node.name),
    ["z.rs", "a.rs"],
  );
});

test("placeThreads_attaches_comments_below_their_line", () => {
  const display = toDisplayLines(
    [equal(1, "a"), replace("old", "new"), inserted(9, "x")],
    "unified",
  );
  const comments = [
    { id: "c-old", side: "old", start_line: 5 },
    { id: "c-new", side: "new", start_line: 9 },
  ];

  const placed = placeThreads(display, comments);

  assert.deepEqual(
    (placed.byLine.get(3) || []).map((comment) => comment.id),
    ["c-new"],
  );
  assert.deepEqual(
    (placed.byLine.get(1) || []).map((comment) => comment.id),
    ["c-old"],
  );
  assert.deepEqual(placed.floating, []);
});

test("placeThreads_keeps_file_wide_and_unmatched_comments_floating", () => {
  const display = toDisplayLines([equal(1, "a")], "unified");
  const comments = [
    { id: "c-wide", side: "new", start_line: null, end_line: null },
    { id: "c-missing", side: "new", start_line: 99 },
  ];

  const placed = placeThreads(display, comments);

  assert.equal(placed.byLine.size, 0);
  assert.deepEqual(
    placed.floating.map((comment) => comment.id),
    ["c-wide", "c-missing"],
  );
});

test("placeThreads_preserves_creation_order", () => {
  const display = toDisplayLines([equal(1, "a")], "unified");
  const comments = [
    { id: "c1", side: "new", start_line: 1 },
    { id: "c2", side: "new", start_line: 1 },
  ];

  const placed = placeThreads(display, comments);

  assert.deepEqual(
    (placed.byLine.get(0) || []).map((comment) => comment.id),
    ["c1", "c2"],
  );
});

test("reviewStats_sums_files_and_changes", () => {
  const review = {
    groups: [
      { id: "g1", files: [file("a.rs", 1, 2), file("b.rs", 3, 4)] },
      { id: "g2", files: [file("c.rs", 5, 6)] },
    ],
  };

  assert.deepEqual(reviewStats(review), { files: 3, groups: 2, add: 9, del: 12 });
});

test("metaItems_shows_manifest_meta_and_computed_stats", () => {
  const review = {
    meta: { author: "ba0918", target: "main..kemi-v1" },
    groups: [{ id: "g1", files: [file("a.rs", 1, 2)] }],
  };

  assert.deepEqual(metaItems(review), [
    { label: "author", value: "ba0918" },
    { label: "target", value: "main..kemi-v1" },
    { label: "ファイル", value: "1" },
    { label: "グループ", value: "1" },
    { label: "変更", value: "+1 −2" },
  ]);
});

test("metaItems_ignores_empty_meta", () => {
  const review = { meta: null, groups: [] };

  assert.deepEqual(metaItems(review), [
    { label: "ファイル", value: "0" },
    { label: "変更", value: "+0 −0" },
  ]);
});

test("nextTheme_cycles_through_presets_and_wraps", () => {
  assert.equal(nextTheme("auto"), "light");
  assert.equal(nextTheme("light"), "dark");
  assert.equal(nextTheme("dark"), "solarized-light");
  assert.equal(nextTheme("solarized-light"), "solarized-dark");
  assert.equal(nextTheme("solarized-dark"), "auto");
});

test("resolveTheme_follows_the_system_only_for_auto", () => {
  assert.equal(resolveTheme("auto", true), "dark");
  assert.equal(resolveTheme("auto", false), "light");
  assert.equal(resolveTheme("solarized-light", true), "solarized-light");
  assert.equal(resolveTheme("dark", false), "dark");
});

test("isDarkTheme_marks_dark_presets_only", () => {
  assert.equal(isDarkTheme("dark"), true);
  assert.equal(isDarkTheme("solarized-dark"), true);
  assert.equal(isDarkTheme("light"), false);
  assert.equal(isDarkTheme("solarized-light"), false);
});

test("lineAnchor_prefers_new_side_and_falls_back_to_old", () => {
  const both = /** @type {import("./model.js").DisplayLine} */ (
    /** @type {any} */ ({ newLine: { number: 12 }, oldLine: { number: 4 } })
  );
  assert.deepEqual(lineAnchor(both), { side: "new", number: 12 });

  const oldOnly = /** @type {import("./model.js").DisplayLine} */ (
    /** @type {any} */ ({ newLine: null, oldLine: { number: 4 } })
  );
  assert.deepEqual(lineAnchor(oldOnly), { side: "old", number: 4 });

  const empty = /** @type {import("./model.js").DisplayLine} */ (
    /** @type {any} */ ({ newLine: null, oldLine: null })
  );
  assert.equal(lineAnchor(empty), null);
});

