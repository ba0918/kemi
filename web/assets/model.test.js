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
  withRowIndex,
  navStops,
  navCurrentIndex,
  navNextTarget,
  navPrevTarget,
  nextFileIndex,
  hasStops,
  commitTypeBox,
  statusLetter,
  rangeAfterSkip,
  sideTone,
  unitSwitchOrder,
  unitSwitchTarget,
  originJumpTarget,
  seenProgress,
  currentStopIndex,
  rulerMarks,
  firstLine,
  commentedLines,
  describeComment,
  submitSummary,
  commentCountChanges,
  treeOrder,
  hasLoadedStops,
  collapseLoadedRows,
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
 * @param {string} oldText
 * @param {string} newText
 * @returns {import("./model.js").LogicalRow}
 */
function replaceAt(number, oldText, newText) {
  return {
    kind: "replace",
    old: { number, text: oldText },
    new: { number, text: newText },
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

test("unified_shows_removed_block_before_added_block", () => {
  const rows = [
    equal(1, "a"),
    replaceAt(2, "old2", "new2"),
    replaceAt(3, "old3", "new3"),
    replaceAt(4, "old4", "new4"),
    equal(5, "e"),
  ];

  const lines = toDisplayLines(rows, "unified");

  assert.deepEqual(
    lines.map((line) => line.kind),
    ["equal", "replace-old", "replace-old", "replace-old", "replace-new", "replace-new", "replace-new", "equal"],
  );
  assert.deepEqual(
    lines.slice(1, 7).map((line) => (line.oldLine ?? line.newLine)?.text),
    ["old2", "old3", "old4", "new2", "new3", "new4"],
  );
});

test("unified_keeps_leftover_deletes_with_the_removed_block_and_inserts_with_the_added_block", () => {
  const rows = [replaceAt(2, "old2", "new2"), deleted(3, "gone"), equal(4, "d"), replaceAt(5, "o", "n"), inserted(6, "added"), skip(7)];

  const lines = toDisplayLines(rows, "unified");

  assert.deepEqual(
    lines.map((line) => line.kind),
    ["replace-old", "delete", "replace-new", "equal", "replace-old", "replace-new", "insert", "skip"],
  );
});

test("unified_keeps_word_highlights_with_each_side_of_the_pair", () => {
  const lines = toDisplayLines([replaceAt(2, "old2", "new2"), replaceAt(3, "old3", "new3")], "unified");

  assert.deepEqual(lines[1].oldSegments, [{ text: "old3", changed: true }]);
  assert.deepEqual(lines[1].newSegments, []);
  assert.deepEqual(lines[3].newSegments, [{ text: "new3", changed: true }]);
  assert.deepEqual(lines[3].oldSegments, []);
});

test("unified_reordering_keeps_logical_index_for_expanding_skips", () => {
  const rows = [skip(3), replaceAt(4, "o4", "n4"), replaceAt(5, "o5", "n5"), skip(2)];

  const lines = toDisplayLines(rows, "unified");

  assert.deepEqual(
    lines.map((line) => line.logicalIndex),
    [0, 1, 2, 1, 2, 3],
  );
  assert.equal(lines[5].skip, rows[3]);
});

test("unified_shows_a_500k_line_file_rewritten_wholesale", () => {
  const count = 500_000;
  const rows = Array.from({ length: count }, (_, index) => replaceAt(index + 1, `o${index}`, `n${index}`));

  const lines = toDisplayLines(rows, "unified");

  assert.equal(lines.length, count * 2);
  assert.equal(lines[count - 1].oldLine?.text, `o${count - 1}`);
  assert.equal(lines[count].newLine?.text, "n0");
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


test("range_comment_sits_under_last_line", () => {
  const display = toDisplayLines(
    [equal(1, "a"), equal(2, "b"), equal(3, "c"), equal(4, "d")],
    "unified",
  );
  const comments = [{ id: "c1", side: "new", start_line: 2, end_line: 3 }];

  const placed = placeThreads(display, comments);

  assert.deepEqual(
    (placed.byLine.get(2) || []).map((comment) => comment.id),
    ["c1"],
  );
  assert.equal(placed.byLine.get(1), undefined);
});

test("with_row_index_numbers_rows_by_their_place_in_the_whole_file", () => {
  const rows = [
    { kind: "skip", count: 3, from: 0, to: 3 },
    equal(4, "d"),
    replaceAt(5, "o", "n"),
    { kind: "skip", count: 10, from: 5, to: 15 },
    equal(16, "p"),
  ];

  assert.deepEqual(
    withRowIndex(rows).map((row) => row.row),
    [0, 3, 4, 5, 15],
  );
});

test("origin_lines_sit_above_each_change_block_only_when_asked", () => {
  const rows = withRowIndex([equal(1, "a"), replaceAt(2, "o", "n"), equal(3, "c"), inserted(4, "x")]);

  const withOrigin = toDisplayLines(rows, "unified", { origin: true });
  const without = toDisplayLines(rows, "split");

  assert.deepEqual(
    withOrigin.map((line) => line.kind),
    ["equal", "origin", "replace-old", "replace-new", "equal", "origin", "insert"],
  );
  assert.deepEqual(
    withOrigin.filter((line) => line.kind === "origin").map((line) => line.block),
    [1, 3],
  );
  assert.ok(!without.some((line) => line.kind === "origin"));
  assert.deepEqual(
    toDisplayLines(rows, "split", { origin: true }).map((line) => line.kind),
    ["equal", "origin", "replace", "equal", "origin", "insert"],
  );
});

test("nav_stops_at_each_change_block_start", () => {
  const display = toDisplayLines(
    [equal(1, "a"), replaceAt(2, "o", "n"), deleted(3, "d"), equal(4, "b"), inserted(5, "x")],
    "unified",
  );

  assert.deepEqual(navStops(display, []), [1, 5]);
});

test("nav_stops_at_origin_line_when_the_block_has_one", () => {
  const display = toDisplayLines(
    withRowIndex([equal(1, "a"), replaceAt(2, "o", "n")]),
    "unified",
    { origin: true },
  );

  assert.deepEqual(navStops(display, []), [1]);
  assert.equal(display[1].kind, "origin");
});

test("nav_stops_at_first_line_of_a_comment_outside_blocks", () => {
  const display = toDisplayLines(
    [equal(1, "a"), equal(2, "b"), equal(3, "c"), inserted(4, "x")],
    "unified",
  );
  const comments = [{ id: "c1", side: "new", start_line: 2, end_line: 3 }];

  assert.deepEqual(navStops(display, comments), [1, 3]);
});

test("nav_stops_do_not_add_a_stop_for_a_comment_starting_inside_a_block", () => {
  const display = toDisplayLines(
    [equal(1, "a"), inserted(2, "x"), inserted(3, "y"), equal(4, "b")],
    "unified",
  );
  const comments = [{ id: "c1", side: "new", start_line: 3, end_line: 4 }];

  assert.deepEqual(navStops(display, comments), [1]);
});

test("nav_stops_ignore_file_wide_comments", () => {
  const display = toDisplayLines([equal(1, "a"), inserted(2, "x")], "unified");
  const comments = [{ id: "c1", side: "new", start_line: null, end_line: null }];

  assert.deepEqual(navStops(display, comments), [1]);
});

test("mid_file_the_current_stop_is_the_last_one_above_the_baseline_and_n_p_step_one", () => {
  const tops = [100, 400, 900];
  // 基準線は 52 + 48 = 100。止まる場所 0 をちょうど越えたところ。
  const view = { scrollTop: 52, viewportHeight: 800, contentHeight: 4000 };

  assert.equal(navCurrentIndex(tops, view, 48), 0);
  assert.equal(navNextTarget(tops, view, 48), 1);
  assert.equal(navPrevTarget(tops, view, 48), null);
  assert.equal(navPrevTarget(tops, { ...view, scrollTop: 852 }, 48), 1);
});

test("at_maximum_scroll_every_visible_stop_counts_as_reached_and_n_leaves_the_file", () => {
  const tops = [100, 400, 900, 1200, 1500];
  // 末尾までスクロールしきった状態。止まる場所 2..4 は基準線より下だが見えている。
  const view = { scrollTop: 800, viewportHeight: 800, contentHeight: 1600 };

  assert.equal(navCurrentIndex(tops, view, 48), 4);
  assert.equal(navNextTarget(tops, view, 48), null);
  // 2..4 へ送っても末尾で切り詰められて動かない。実際に上へ戻れる 1 が前。
  assert.equal(navPrevTarget(tops, view, 48), 1);
});

test("a_file_too_short_to_scroll_starts_at_the_last_stop_and_n_p_leave_the_file", () => {
  const tops = [0, 120, 240];
  const view = { scrollTop: 0, viewportHeight: 800, contentHeight: 400 };

  assert.equal(navCurrentIndex(tops, view, 48), 2);
  assert.equal(navNextTarget(tops, view, 48), null);
  assert.equal(navPrevTarget(tops, view, 48), null);
});

test("a_stop_exactly_on_the_baseline_is_current_and_p_moves_to_the_one_before_it", () => {
  const tops = [100, 400, 900];
  // 基準線は 352 + 48 = 400。止まる場所 1 の上端とちょうど同じ。
  const view = { scrollTop: 352, viewportHeight: 800, contentHeight: 4000 };

  assert.equal(navCurrentIndex(tops, view, 48), 1);
  assert.equal(navPrevTarget(tops, view, 48), 0);
  assert.equal(navNextTarget(tops, view, 48), 2);
});

test("after_a_shrink_clamps_the_view_the_current_stop_is_still_the_last_one", () => {
  const tops = [100, 400, 900, 1300];
  // 開いていた吹き出しを畳んで全体が縮み、スクロール位置が末尾へ切り詰められた後。
  const clamped = { scrollTop: 567, viewportHeight: 800, contentHeight: 1367 };

  assert.equal(navCurrentIndex(tops, clamped, 48), 3);
  assert.equal(navNextTarget(tops, clamped, 48), null);
});

test("next_file_follows_the_visible_order_across_groups_and_stops_at_the_ends", () => {
  const files = [file("a.rs", 1, 0), file("b.rs", 2, 0), file("c.rs", 3, 0)];
  const navigable = () => true;

  assert.equal(nextFileIndex(files, 0, 1, navigable), 1);
  assert.equal(nextFileIndex(files, 2, 1, navigable), null);
  assert.equal(nextFileIndex(files, 0, -1, navigable), null);
  assert.equal(nextFileIndex(files, 2, -1, navigable), 1);
});

test("next_file_skips_noise_binary_and_files_without_stops_but_not_seen_ones", () => {
  const files = [
    file("a.rs", 1, 0),
    file("Cargo.lock", 5, 5, { noise: true, collapsed: true }),
    file("logo.png", 0, 0, { binary: true }),
    file("moved.rs", 0, 0, { status: "rename" }),
    file("seen.rs", 1, 1, { seen: true }),
  ];
  const navigable = (/** @type {FileEntry} */ entry) => hasStops(entry, [], {});

  assert.equal(nextFileIndex(files, 0, 1, navigable), 4);
  assert.equal(nextFileIndex(files, 4, -1, navigable), 0);
});

test("next_file_counts_a_line_comment_as_a_stop", () => {
  const moved = file("moved.rs", 0, 0, { status: "rename" });

  assert.equal(hasStops(moved, [{ side: "new", start_line: 1, end_line: 1 }], {}), true);
  assert.equal(hasStops(moved, [{ side: "new", start_line: null, end_line: null }], {}), false);
  assert.equal(
    hasStops(file("Cargo.lock", 5, 5, { noise: true, collapsed: true }), [], { "f-Cargo.lock": false }),
    true,
  );
});

test("commit_type_box_splits_a_conventional_subject", () => {
  assert.deepEqual(commitTypeBox("feat: 足す"), { type: "feat", title: "足す" });
  assert.deepEqual(commitTypeBox("fix(ui)!: 直す"), { type: "fix(ui)!", title: "直す" });
});

test("commit_type_box_leaves_other_subjects_alone", () => {
  assert.equal(commitTypeBox("Merge branch 'main'"), null);
  assert.equal(commitTypeBox("Feat: 大文字"), null);
  assert.equal(commitTypeBox("feat:空白なし"), null);
  assert.equal(commitTypeBox("docs(:x"), null);
});

test("status_letter_is_one_character", () => {
  assert.equal(statusLetter("add"), "A");
  assert.equal(statusLetter("delete"), "D");
  assert.equal(statusLetter("rename"), "R");
  assert.equal(statusLetter("modify"), "M");
});

test("skip_row_shows_old_and_new_lines_of_the_range_below_it", () => {
  const rows = [
    { kind: "skip", count: 3, from: 0, to: 3 },
    equal(4, "d"),
    replaceAt(5, "o", "n"),
    inserted(6, "x"),
    { kind: "skip", count: 9, from: 6, to: 15 },
    equal(16, "p"),
  ];

  assert.deepEqual(rangeAfterSkip(rows, 0), {
    old: { start: 4, end: 5 },
    new: { start: 4, end: 6 },
  });
  assert.deepEqual(rangeAfterSkip(rows, 4), {
    old: { start: 16, end: 16 },
    new: { start: 16, end: 16 },
  });
});

test("skip_row_at_the_end_has_no_range_below_it", () => {
  const rows = [equal(1, "a"), { kind: "skip", count: 9, from: 1, to: 10 }];

  assert.equal(rangeAfterSkip(rows, 1), null);
});

test("skip_row_range_leaves_out_a_side_without_lines", () => {
  const rows = [{ kind: "skip", count: 3, from: 0, to: 3 }, inserted(4, "x")];

  assert.deepEqual(rangeAfterSkip(rows, 0), { old: null, new: { start: 4, end: 4 } });
});

test("split_colors_the_old_side_as_removed_and_the_new_side_as_added", () => {
  assert.deepEqual([sideTone("replace", "old"), sideTone("replace", "new")], ["del", "add"]);
  assert.deepEqual([sideTone("delete", "old"), sideTone("delete", "new")], ["del", "empty"]);
  assert.deepEqual([sideTone("insert", "old"), sideTone("insert", "new")], ["empty", "add"]);
  assert.deepEqual([sideTone("equal", "old"), sideTone("equal", "new")], ["", ""]);
});

test("unified_colors_each_line_by_the_side_it_shows", () => {
  assert.equal(sideTone("replace-old", null), "del");
  assert.equal(sideTone("delete", null), "del");
  assert.equal(sideTone("replace-new", null), "add");
  assert.equal(sideTone("insert", null), "add");
  assert.equal(sideTone("equal", null), "");
});

/**
 * @param {string} groupId
 * @param {FileEntry} entry
 */
function inGroup(groupId, entry) {
  return { file: entry, group: { id: groupId } };
}

test("unit_switch_shows_the_first_file_with_the_same_path", () => {
  const entries = [
    inGroup("c1", file("a.rs", 1, 0)),
    inGroup("c2", file("b.rs", 1, 0)),
    inGroup("c3", file("b.rs", 2, 0)),
  ];

  assert.equal(unitSwitchTarget(entries, "b.rs"), 1);
  assert.equal(unitSwitchTarget(entries, "missing.rs"), 0);
  assert.equal(unitSwitchTarget([], "a.rs"), -1);
});

test("unit_switch_lists_final_form_before_per_commit_whatever_the_startup_unit", () => {
  const startedByCommit = [
    { unit: "commit", state: "ready" },
    { unit: "file", state: "building" },
  ];
  const startedByFile = [
    { unit: "file", state: "ready" },
    { unit: "commit", state: "building" },
  ];

  assert.deepEqual(
    unitSwitchOrder(startedByCommit).map((status) => status.unit),
    ["file", "commit"],
  );
  assert.deepEqual(
    unitSwitchOrder(startedByFile).map((status) => status.unit),
    ["file", "commit"],
  );
});

test("origin_jump_goes_to_the_path_at_that_commit", () => {
  const renamed = file("new.rs", 1, 1, { status: "rename", old_path: "old.rs" });
  const entries = [
    inGroup("c1", file("old.rs", 1, 0)),
    inGroup("c2", renamed),
    inGroup("c3", file("new.rs", 1, 0)),
  ];

  assert.equal(originJumpTarget(entries, "c1", { path: "old.rs", side: "new", line: 3 }), 0);
  assert.equal(originJumpTarget(entries, "c3", { path: "new.rs", side: "new", line: 3 }), 2);
  assert.equal(originJumpTarget(entries, "c2", { path: "old.rs", side: "old", line: 7 }), 1);
  assert.equal(originJumpTarget(entries, "c9", { path: "new.rs", side: "new", line: 1 }), -1);
});

test("seen_progress_counts_seen_files_and_marks_done", () => {
  assert.deepEqual(seenProgress([file("a", 1, 0, { seen: true }), file("b", 1, 0)]), {
    seen: 1,
    total: 2,
    done: false,
  });
  assert.deepEqual(seenProgress([file("a", 1, 0, { seen: true })]), { seen: 1, total: 1, done: true });
  assert.deepEqual(seenProgress([]), { seen: 0, total: 0, done: false });
});

test("current_stop_is_the_last_one_at_or_above_the_position", () => {
  const tops = [100, 400, 900];

  assert.equal(currentStopIndex(tops, 50), -1);
  assert.equal(currentStopIndex(tops, 100), 0);
  assert.equal(currentStopIndex(tops, 898), 1);
  assert.equal(currentStopIndex(tops, 5000), 2);
});

test("ruler_marks_place_changes_and_comments_by_position_and_merge_per_pixel", () => {
  const kinds = ["", "add", "add", "", "del", "note"];
  const offsets = [0, 10, 20, 30, 40, 50, 60];

  const marks = rulerMarks(kinds, offsets, 60);

  assert.deepEqual(marks, [
    { kind: "add", top: 10, bottom: 30 },
    { kind: "del", top: 40, bottom: 50 },
    { kind: "note", top: 50, bottom: 60 },
  ]);
});

test("ruler_marks_stay_bounded_for_huge_files", () => {
  const kinds = new Array(500_000).fill("add");
  const offsets = Array.from({ length: 500_001 }, (_, index) => index * 24);

  const marks = rulerMarks(kinds, offsets, 800);

  assert.ok(marks.length <= 800, `${marks.length} marks`);
  assert.equal(marks[0].top, 0);
  assert.equal(marks[marks.length - 1].bottom, 800);
});

test("keyAction_moves_between_changes_and_toggles_seen", () => {
  assert.deepEqual(keyAction("n", "unified", 3, 0), { type: "nav", direction: 1 });
  assert.deepEqual(keyAction("p", "unified", 3, 0), { type: "nav", direction: -1 });
  assert.deepEqual(keyAction("v", "unified", 3, 0), { type: "seen" });
});

test("chip_shows_the_first_line_of_the_body", () => {
  assert.equal(firstLine("一行目\n二行目"), "一行目");
  assert.equal(firstLine("\n\n本文"), "本文");
  assert.equal(firstLine(""), "");
});

test("commented_lines_cover_the_whole_range_on_its_side", () => {
  const display = toDisplayLines(
    [equal(1, "a"), replaceAt(2, "o", "n"), equal(3, "c"), equal(4, "d")],
    "unified",
  );
  const comments = [
    { side: "new", start_line: 2, end_line: 3 },
    { side: "old", start_line: 4, end_line: 4 },
    { side: "new", start_line: null, end_line: null },
  ];

  assert.deepEqual([...commentedLines(display, comments)].sort(), [2, 3, 4]);
});

test("comment_list_names_the_unit_and_the_commit", () => {
  const groups = new Map([["abc", "feat: 足す"]]);

  assert.deepEqual(
    describeComment({ group_id: "all", group_title: "x...y" }, { range: true, commitGroups: groups }),
    { unit: "file", subject: "", vanished: false },
  );
  assert.deepEqual(
    describeComment({ group_id: "abc", group_title: "feat: 足す" }, { range: true, commitGroups: groups }),
    { unit: "commit", subject: "feat: 足す", vanished: false },
  );
  assert.deepEqual(
    describeComment({ group_id: "g1", group_title: "" }, { range: false, commitGroups: null }),
    { unit: null, subject: "", vanished: false },
  );
});

test("comment_list_marks_comments_of_vanished_commits", () => {
  const groups = new Map([["new-sha", "fix: 後"]]);

  assert.deepEqual(
    describeComment({ group_id: "old-sha", group_title: "fix: 前" }, { range: true, commitGroups: groups }),
    { unit: "commit", subject: "fix: 前", vanished: true },
  );
  assert.equal(
    describeComment({ group_id: "old-sha", group_title: "fix: 前" }, { range: true, commitGroups: null }).vanished,
    false,
  );
});

test("submit_summary_counts_comments_suggestions_and_unseen_files", () => {
  const comments = [
    { id: "c1", suggestion: { replacement: "x" } },
    { id: "c2", suggestion: null },
    { id: "c3", suggestion: { replacement: "" } },
  ];
  const files = [file("a", 1, 0, { seen: true }), file("b", 1, 0), file("c", 1, 0)];

  assert.deepEqual(submitSummary(comments, files), {
    comments: 3,
    suggestions: 2,
    seen: 1,
    total: 3,
    unseen: 2,
  });
});

test("comment_badges_to_update_are_the_files_whose_comment_count_changed", () => {
  const first = { id: "c1", group_id: "g1", path: "a.rs", body: "x" };
  const second = { id: "c2", group_id: "g1", path: "b.rs", body: "y" };
  const sameFileInAnotherGroup = { id: "c3", group_id: "g2", path: "a.rs", body: "z" };
  const before = [first, second];

  assert.deepEqual(commentCountChanges(before, [...before, sameFileInAnotherGroup]), [
    { group_id: "g2", path: "a.rs" },
  ]);
  assert.deepEqual(commentCountChanges(before, [second]), [{ group_id: "g1", path: "a.rs" }]);
  assert.deepEqual(commentCountChanges(before, [{ ...first, body: "edited" }, second]), []);
});

test("next_file_order_is_the_drawn_tree_order_when_sorting_by_size", () => {
  const files = [
    file("src/a.txt", 10, 10),
    file("b.txt", 5, 5),
    file("src/c.txt", 1, 1),
    file("crlf.txt", 40, 40),
  ];
  const sorted = filterAndSortFiles(files, { focusOnly: false, sortBySize: true });

  const order = treeOrder(sorted.map((sortedFile) => entry(sortedFile)));

  assert.deepEqual(
    order.map((item) => item.file.path),
    ["crlf.txt", "src/a.txt", "src/c.txt", "b.txt"],
  );
});

test("next_file_order_keeps_files_of_a_group_together_even_if_the_input_splits_them", () => {
  const g1 = { id: "g1", title: "g1", why: "", watch: "" };
  const g2 = { id: "g2", title: "g2", why: "", watch: "" };
  const entries = [
    entry(file("lib/x.rs", 1, 0), g1),
    entry(file("y.rs", 1, 0), g2),
    entry(file("z.rs", 1, 0), g1),
    entry(file("lib/w.rs", 1, 0), g1),
  ];

  assert.deepEqual(
    treeOrder(entries).map((item) => `${item.group.id}:${item.file.path}`),
    ["g1:lib/x.rs", "g1:lib/w.rs", "g1:z.rs", "g2:y.rs"],
  );
});

test("drawing_the_tree_from_its_own_order_draws_the_same_tree", () => {
  const g2 = { id: "g2", title: "g2", why: "", watch: "" };
  const entries = [
    entry(file("src/a.rs", 1, 0)),
    entry(file("b.rs", 1, 0), g2),
    entry(file("README.md", 1, 0)),
    entry(file("src/sub/c.rs", 1, 0)),
    entry(file("src/d.rs", 1, 0)),
  ];

  assert.deepEqual(buildTree(treeOrder(entries)), buildTree(entries));
});

test("a_loaded_file_whose_changes_vanish_on_screen_has_no_stops", () => {
  // 改行コードだけ、または最終行の改行だけの変更。増減数はあっても、表示では変更ブロックが無い。
  const rows = [equal(1, "a"), equal(2, "b"), equal(3, "c")];

  assert.equal(hasLoadedStops(rows, []), false);
  assert.equal(hasLoadedStops(rows, [{ side: "new", start_line: null, end_line: null }]), false);
});

test("a_loaded_file_has_stops_at_a_change_block_or_a_line_comment", () => {
  assert.equal(hasLoadedStops([equal(1, "a"), inserted(2, "x")], []), true);
  assert.equal(
    hasLoadedStops([equal(1, "a"), equal(2, "b")], [{ side: "new", start_line: 2, end_line: 2 }]),
    true,
  );
});

/**
 * 40 行のファイルの全行。`changed` の行番号だけ書き換えにする。
 * @param {number[]} changed
 * @returns {LogicalRow[]}
 */
function fortyLines(changed) {
  return Array.from({ length: 40 }, (_, index) =>
    changed.includes(index + 1)
      ? replaceAt(index + 1, `line${index + 1}`, "changed")
      : equal(index + 1, `line${index + 1}`),
  );
}

/**
 * 畳まれずに出ている行の新側の行番号。
 * @param {LogicalRow[]} rows
 * @returns {number[]}
 */
function shownNewLines(rows) {
  return rows.filter((row) => row.kind !== "skip" && row.new).map((row) => Number(row.new?.number));
}

/**
 * 折りたたみをすべて、ファイル全体の行 `whole` で開いた行。
 * @param {LogicalRow[]} rows
 * @param {LogicalRow[]} whole
 * @returns {LogicalRow[]}
 */
function expandWith(rows, whole) {
  return rows.flatMap((row) =>
    row.kind === "skip" ? whole.slice(Number(row.from), Number(row.to)) : [row],
  );
}

/**
 * 変更の行の間にある折りたたみの数。
 * @param {LogicalRow[]} rows
 * @returns {number}
 */
function foldsBetweenChanges(rows) {
  const changes = rows.flatMap((row, index) => (row.kind === "replace" ? [index] : []));
  return rows
    .slice(changes[0], changes[changes.length - 1])
    .filter((row) => row.kind === "skip").length;
}

test("collapsing_loaded_rows_keeps_context_beside_each_change_and_folds_the_middle", () => {
  const whole = fortyLines([5, 30]);

  const collapsed = collapseLoadedRows(whole, [], 3);

  const shown = shownNewLines(collapsed);
  for (const number of [4, 6, 29, 31]) {
    assert.ok(shown.includes(number), `new line ${number} next to a change is hidden: ${shown}`);
  }
  assert.ok(!shown.includes(18), `the middle of the run is shown: ${shown}`);
  assert.equal(foldsBetweenChanges(collapsed), 1);
});

test("collapsing_loaded_rows_leaves_folds_that_open_to_the_rows_they_hid", () => {
  const whole = fortyLines([5, 30]);

  const collapsed = collapseLoadedRows(whole, [], 3);

  assert.deepEqual(expandWith(collapsed, whole), whole);
  for (const row of collapsed.filter((candidate) => candidate.kind === "skip")) {
    assert.equal(row.count, Number(row.to) - Number(row.from));
  }
});

test("collapsing_loaded_rows_keeps_commented_lines_and_their_neighbours_visible", () => {
  const whole = fortyLines([20]);
  const comments = [{ id: "c1", side: "new", start_line: 1, end_line: 1 }];

  const shown = shownNewLines(collapseLoadedRows(whole, comments, 3));

  for (const number of [1, 2, 17, 18, 19]) {
    assert.ok(shown.includes(number), `new line ${number} is hidden: ${shown}`);
  }
  assert.ok(!shown.includes(10), `the middle of the run is shown: ${shown}`);
});

test("collapsing_loaded_rows_keeps_old_side_commented_lines_visible", () => {
  const whole = fortyLines([20]);
  const comments = [{ id: "c1", side: "old", start_line: 35, end_line: 36 }];

  const collapsed = collapseLoadedRows(whole, comments, 3);

  const shownOld = collapsed
    .filter((row) => row.kind !== "skip" && row.old)
    .map((row) => Number(row.old?.number));
  assert.ok(shownOld.includes(35) && shownOld.includes(36), `old lines 35-36 are hidden: ${shownOld}`);
});
