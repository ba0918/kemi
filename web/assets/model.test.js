import assert from "node:assert/strict";
import { test } from "node:test";

import {
  collapseDefault,
  commentLabel,
  filterAndSortFiles,
  formatBytes,
  keyAction,
  lineOffsets,
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
