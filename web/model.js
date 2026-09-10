// @ts-check
// ブラウザとテストで共有する純ロジック。DOM には触れない。

/**
 * @typedef {{ number: number, text: string }} Line
 * @typedef {{ text: string, changed: boolean }} Segment
 * @typedef {{
 *   kind: string,
 *   old?: Line|null,
 *   new?: Line|null,
 *   old_segments?: Segment[],
 *   new_segments?: Segment[],
 *   count?: number,
 *   from?: number,
 *   to?: number,
 *   old_start?: number|null,
 *   new_start?: number|null,
 * }} LogicalRow
 * @typedef {{
 *   id: string, path: string, old_path: string|null, status: string,
 *   add: number, del: number, binary: boolean, old_size: number, new_size: number,
 *   focus: boolean, note: string, noise: boolean, seen: boolean, collapsed: boolean,
 * }} FileEntry
 * @typedef {{
 *   kind: string, oldLine: Line|null, newLine: Line|null,
 *   oldSegments: Segment[], newSegments: Segment[],
 *   logicalIndex: number, skip: LogicalRow|null,
 * }} DisplayLine
 */

/**
 * 論理行を表示行に変換する。unified では replace を 2 行に分ける。
 * @param {LogicalRow[]} rows
 * @param {"unified"|"split"} mode
 * @returns {DisplayLine[]}
 */
export function toDisplayLines(rows, mode) {
  /** @type {DisplayLine[]} */
  const lines = [];
  rows.forEach((row, logicalIndex) => {
    const base = { oldSegments: [], newSegments: [], skip: null, logicalIndex };
    switch (row.kind) {
      case "equal":
        lines.push({
          ...base,
          kind: "equal",
          oldLine: row.old ?? null,
          newLine: row.new ?? null,
        });
        break;
      case "delete":
        lines.push({
          ...base,
          kind: "delete",
          oldLine: row.old ?? null,
          newLine: null,
        });
        break;
      case "insert":
        lines.push({
          ...base,
          kind: "insert",
          oldLine: null,
          newLine: row.new ?? null,
        });
        break;
      case "replace":
        if (mode === "split") {
          lines.push({
            ...base,
            kind: "replace",
            oldLine: row.old ?? null,
            newLine: row.new ?? null,
            oldSegments: row.old_segments ?? [],
            newSegments: row.new_segments ?? [],
          });
        } else {
          lines.push({
            ...base,
            kind: "replace-old",
            oldLine: row.old ?? null,
            newLine: null,
            oldSegments: row.old_segments ?? [],
          });
          lines.push({
            ...base,
            kind: "replace-new",
            oldLine: null,
            newLine: row.new ?? null,
            newSegments: row.new_segments ?? [],
          });
        }
        break;
      case "skip":
        lines.push({ ...base, kind: "skip", oldLine: null, newLine: null, skip: row });
        break;
      default:
        break;
    }
  });
  return lines;
}

/**
 * 各行の上端オフセット（最後に全体の高さ）を返す。
 * @param {number[]} heights
 * @returns {number[]}
 */
export function lineOffsets(heights) {
  /** @type {number[]} */
  const offsets = [0];
  let total = 0;
  for (const height of heights) {
    total += height;
    offsets.push(total);
  }
  return offsets;
}

/**
 * 表示中の窓を求める。`end` は含まない。
 * @param {number[]} offsets
 * @param {number} scrollTop
 * @param {number} viewportHeight
 * @param {number} overscan
 * @returns {{ start: number, end: number }}
 */
export function windowFor(offsets, scrollTop, viewportHeight, overscan) {
  const total = offsets.length - 1;
  if (total <= 0) {
    return { start: 0, end: 0 };
  }
  const top = Math.max(0, scrollTop);
  const first = firstIndexAtOrBefore(offsets, top, total);
  const bottom = top + Math.max(0, viewportHeight);
  const last = firstIndexAtOrAfter(offsets, bottom, total);
  return {
    start: Math.max(0, first - overscan),
    end: Math.min(total, last + overscan),
  };
}

/**
 * @param {number[]} offsets
 * @param {number} value
 * @param {number} total
 * @returns {number}
 */
function firstIndexAtOrBefore(offsets, value, total) {
  let low = 0;
  let high = total;
  while (low < high) {
    const middle = Math.ceil((low + high) / 2);
    if (offsets[middle] <= value) {
      low = middle;
    } else {
      high = middle - 1;
    }
  }
  return low;
}

/**
 * @param {number[]} offsets
 * @param {number} value
 * @param {number} total
 * @returns {number}
 */
function firstIndexAtOrAfter(offsets, value, total) {
  let low = 0;
  let high = total;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    if (offsets[middle] < value) {
      low = middle + 1;
    } else {
      high = middle;
    }
  }
  return low;
}

/**
 * @param {string} key
 * @param {"unified"|"split"} _mode
 * @param {number} count
 * @param {number} index
 * @param {boolean} [wrap]
 * @returns {{ type: string, index?: number, mode?: string, value?: boolean }}
 */
export function keyAction(key, _mode, count, index, wrap = false) {
  switch (key) {
    case "j":
      return { type: "file", index: Math.min(count - 1, index + 1) };
    case "k":
      return { type: "file", index: Math.max(0, index - 1) };
    case "s":
      return { type: "mode", mode: "split" };
    case "u":
      return { type: "mode", mode: "unified" };
    case "w":
      return { type: "wrap", value: !wrap };
    case "G":
      return { type: "file", index: Math.max(0, count - 1) };
    case "g":
      return { type: "file", index: 0 };
    default:
      return { type: "none" };
  }
}

/**
 * @param {FileEntry[]} files
 * @param {{ focusOnly: boolean, sortBySize: boolean }} options
 * @returns {FileEntry[]}
 */
export function filterAndSortFiles(files, options) {
  const filtered = options.focusOnly
    ? files.filter((file) => file.focus)
    : files.slice();
  if (!options.sortBySize) {
    return filtered;
  }
  return filtered.sort(
    (left, right) => right.add + right.del - (left.add + left.del),
  );
}

/**
 * @param {FileEntry} file
 * @param {Record<string, boolean>} collapsedMap
 * @returns {boolean}
 */
export function collapseDefault(file, collapsedMap) {
  if (Object.prototype.hasOwnProperty.call(collapsedMap, file.id)) {
    return collapsedMap[file.id];
  }
  return file.noise;
}

/**
 * @param {string} status
 * @returns {string}
 */
export function statusLabel(status) {
  switch (status) {
    case "add":
      return "追加";
    case "delete":
      return "削除";
    case "rename":
      return "改名";
    default:
      return "変更";
  }
}

/**
 * @param {number} bytes
 * @returns {string}
 */
export function formatBytes(bytes) {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units = ["KiB", "MiB", "GiB", "TiB"];
  let value = bytes;
  let unit = "B";
  for (const candidate of units) {
    if (value < 1024) {
      break;
    }
    value /= 1024;
    unit = candidate;
  }
  return `${value.toFixed(1)} ${unit}`;
}
