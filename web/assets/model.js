// @ts-check
// ブラウザとテストで共有する純ロジック。DOM には触れない。

/**
 * @typedef {{ number: number, text: string, html?: string }} Line
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
 *   row?: number,
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
 *   block?: number,
 * }} DisplayLine
 */

/**
 * 各行に、ファイル全体を整列した行の中での位置（`row`）を付ける。サーバは畳んだ
 * 範囲を skip の `from` / `to` で返すので、その間の行は続き番号になる。由来は
 * この位置で変更ブロックを指す。
 * @param {LogicalRow[]} rows
 * @returns {LogicalRow[]}
 */
export function withRowIndex(rows) {
  let next = 0;
  return rows.map((row) => {
    if (row.kind === "skip") {
      const from = Number(row.from ?? next);
      next = Number(row.to ?? from);
      return { ...row, row: from };
    }
    const indexed = { ...row, row: next };
    next += 1;
    return indexed;
  });
}

/** 追加・削除・書き換えの表示行の種類。 */
const CHANGE_KINDS = new Set(["replace", "replace-old", "replace-new", "delete", "insert"]);

/**
 * @param {string} kind
 * @returns {boolean}
 */
export function isChangeKind(kind) {
  return CHANGE_KINDS.has(kind);
}

/**
 * 論理行を表示行に変換する。1 列（unified）では、連続する変更を「消した行の塊 →
 * 足した行の塊」の順に並べる（書き換えの旧と新を 1 行ずつ交互にしない）。
 * `origin` を指定すると、各変更ブロックのすぐ上に由来の行を置く。
 * @param {LogicalRow[]} rows
 * @param {"unified"|"split"} mode
 * @param {{ origin?: boolean }} [options]
 * @returns {DisplayLine[]}
 */
export function toDisplayLines(rows, mode, options = {}) {
  /** @type {DisplayLine[]} */
  const lines = [];
  /** @type {DisplayLine[]} */
  let removed = [];
  /** @type {DisplayLine[]} */
  let added = [];
  let inBlock = false;
  const flush = () => {
    lines.push(...removed, ...added);
    removed = [];
    added = [];
  };
  rows.forEach((row, logicalIndex) => {
    const base = { oldSegments: [], newSegments: [], skip: null, logicalIndex };
    const changed = row.kind === "replace" || row.kind === "delete" || row.kind === "insert";
    if (!changed) {
      flush();
      inBlock = false;
    } else if (!inBlock) {
      inBlock = true;
      if (options.origin) {
        lines.push({
          ...base,
          kind: "origin",
          oldLine: null,
          newLine: null,
          block: row.row ?? logicalIndex,
        });
      }
    }
    switch (row.kind) {
      case "equal":
        lines.push({
          ...base,
          kind: "equal",
          oldLine: row.old ?? null,
          newLine: row.new ?? null,
        });
        break;
      case "delete": {
        const line = { ...base, kind: "delete", oldLine: row.old ?? null, newLine: null };
        if (mode === "split") {
          lines.push(line);
        } else {
          removed.push(line);
        }
        break;
      }
      case "insert": {
        const line = { ...base, kind: "insert", oldLine: null, newLine: row.new ?? null };
        if (mode === "split") {
          lines.push(line);
        } else {
          added.push(line);
        }
        break;
      }
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
          removed.push({
            ...base,
            kind: "replace-old",
            oldLine: row.old ?? null,
            newLine: null,
            oldSegments: row.old_segments ?? [],
          });
          added.push({
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
  flush();
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
  return Boolean(file.collapsed);
}

/**
 * コメントの位置表示。ファイル全体は行を持たない。
 * @param {any} comment
 * @returns {string}
 */
export function commentLabel(comment) {
  if (comment.start_line === null || comment.start_line === undefined) {
    return "ファイル全体";
  }
  const side = comment.side === "new" ? "新側" : "旧側";
  const single =
    comment.end_line === null ||
    comment.end_line === undefined ||
    comment.end_line === comment.start_line;
  return single
    ? `${side} ${comment.start_line}`
    : `${side} ${comment.start_line}–${comment.end_line}`;
}

/**
 * suggestion は新側の行コメントにだけ付く。
 * @param {string} side
 * @returns {boolean}
 */
export function suggestionAllowed(side) {
  return side === "new";
}

/**
 * id が一致するコメントだけを差し替えた新しい配列を返す。
 * @param {any[]} comments
 * @param {any} updated
 * @returns {any[]}
 */
export function replaceComment(comments, updated) {
  return comments.map((comment) =>
    comment.id === updated.id ? updated : comment,
  );
}

/**
 * 下書きをファイルとコメント位置ごとに分ける localStorage のキー。
 * @param {string} fileId
 * @param {{ side: string, start: number, end: number } | null} selection
 * @returns {string}
 */
export function draftKey(fileId, selection) {
  if (!selection) {
    return `kemi-draft:${fileId}:file`;
  }
  return `kemi-draft:${fileId}:${selection.side}:${selection.start}-${selection.end}`;
}

/**
 * ハイライトボタンを押した後のそのファイルの指定。
 * null はファイル既定（上限内なら有効）に戻すことを表す。
 * @param {boolean} enabled
 * @param {boolean} capable
 * @returns {"on" | "off" | null}
 */
export function nextHighlightOverride(enabled, capable) {
  if (enabled) {
    return "off";
  }
  if (capable) {
    return null;
  }
  return "on";
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
 * 状態の 1 文字（A 追加 / D 削除 / R 改名 / M 変更）。
 * @param {string} status
 * @returns {string}
 */
export function statusLetter(status) {
  switch (status) {
    case "add":
      return "A";
    case "delete":
      return "D";
    case "rename":
      return "R";
    default:
      return "M";
  }
}

/**
 * 件名がコミットの種類で始まるとき（例 `feat: `、`fix(ui)!: `）、`:` の前までの種類と
 * 残りの件名に分ける。形式に合わない件名は null。
 * @param {string} subject
 * @returns {{ type: string, title: string } | null}
 */
export function commitTypeBox(subject) {
  const match = /^([a-z]+(?:\([^()]*\))?!?): (.+)$/s.exec(subject);
  return match ? { type: match[1], title: match[2] } : null;
}

/**
 * 変更間の移動で止まる場所（表示行の位置、昇順）。変更ブロックの先頭（由来の行が
 * あればその行）と、ブロックの外で始まる行コメントの範囲の最初の行。ブロックの中で
 * 始まるコメントはブロックの先頭で一緒に止まり、ファイル全体のコメントは含めない。
 * @param {DisplayLine[]} display
 * @param {any[]} comments
 * @returns {number[]}
 */
export function navStops(display, comments) {
  /** @type {Set<number>} */
  const stops = new Set();
  const inBlock = new Array(display.length).fill(false);
  let blockStart = -1;
  display.forEach((line, index) => {
    if (line.kind === "origin" || isChangeKind(line.kind)) {
      if (blockStart < 0) {
        blockStart = index;
        stops.add(index);
      }
      inBlock[index] = true;
      return;
    }
    blockStart = -1;
  });
  /** @type {Map<string, number>} */
  const anchors = new Map();
  display.forEach((line, index) => {
    for (const [side, target] of [
      ["old", line.oldLine],
      ["new", line.newLine],
    ]) {
      const key = target ? `${side}:${Number(/** @type {Line} */ (target).number)}` : null;
      if (key && !anchors.has(key)) {
        anchors.set(key, index);
      }
    }
  });
  for (const comment of comments) {
    if (comment.start_line === null || comment.start_line === undefined) {
      continue;
    }
    const index = anchors.get(`${comment.side}:${Number(comment.start_line)}`);
    if (index !== undefined && !inBlock[index]) {
      stops.add(index);
    }
  }
  return [...stops].sort((left, right) => left - right);
}

/**
 * 今の位置から見た次（`direction` 1）か前（-1）の止まる場所。無ければ null（端で止まる）。
 * @param {number[]} tops 止まる場所の上端（昇順）
 * @param {number} position 今の位置
 * @param {1 | -1} direction
 * @returns {number | null}
 */
export function nextStop(tops, position, direction) {
  if (direction > 0) {
    const index = tops.findIndex((top) => top > position + 1);
    return index < 0 ? null : index;
  }
  for (let index = tops.length - 1; index >= 0; index -= 1) {
    if (tops[index] < position - 1) {
      return index;
    }
  }
  return null;
}

/**
 * ファイルに止まる場所がありそうか。内容を読まずにメタデータで決める。バイナリと、
 * 畳まれたノイズと、変更もコメントも無いファイル（改名だけなど）は持たない。
 * @param {FileEntry} file
 * @param {any[]} comments そのファイルのコメント
 * @param {Record<string, boolean>} collapsedMap
 * @returns {boolean}
 */
export function hasStops(file, comments, collapsedMap) {
  if (file.binary || collapseDefault(file, collapsedMap)) {
    return false;
  }
  if (file.add + file.del > 0) {
    return true;
  }
  return comments.some(
    (comment) => comment.start_line !== null && comment.start_line !== undefined,
  );
}

/**
 * 見えている順で、次（`direction` 1）か前（-1）の移動先のファイル。見たのファイルも
 * 飛ばさない。端では null。
 * @param {FileEntry[]} files
 * @param {number} index 今のファイルの位置
 * @param {1 | -1} direction
 * @param {(file: FileEntry) => boolean} navigable
 * @returns {number | null}
 */
export function nextFileIndex(files, index, direction, navigable) {
  for (let next = index + direction; next >= 0 && next < files.length; next += direction) {
    if (navigable(files[next])) {
      return next;
    }
  }
  return null;
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

/**
 * @typedef {{ type: "file", name: string, file: FileEntry }} FileNode
 * @typedef {{ type: "dir", name: string, path: string, children: TreeNode[] }} DirNode
 * @typedef {FileNode | DirNode} TreeNode
 */

/**
 * 表示順のファイルを、パスのディレクトリで入れ子にしたツリーへ分ける。
 * ディレクトリとファイルの順は入力の先着順を保つ（既定は入力順のため）。
 * @param {FileEntry[]} files
 * @returns {TreeNode[]}
 */
function buildFileTree(files) {
  /** @type {TreeNode[]} */
  const roots = [];
  /** @type {Map<string, DirNode>} */
  const directories = new Map();
  for (const file of files) {
    const segments = file.path.split("/");
    let children = roots;
    let prefix = "";
    for (let index = 0; index < segments.length - 1; index += 1) {
      prefix = prefix ? `${prefix}/${segments[index]}` : segments[index];
      let directory = directories.get(prefix);
      if (!directory) {
        directory = {
          type: "dir",
          name: segments[index],
          path: prefix,
          children: [],
        };
        directories.set(prefix, directory);
        children.push(directory);
      }
      children = directory.children;
    }
    children.push({
      type: "file",
      name: segments[segments.length - 1],
      file,
    });
  }
  return roots;
}

/**
 * グループごとのディレクトリツリーを作る。
 * @param {{ file: FileEntry, group: any }[]} entries
 * @returns {{ group: any, nodes: TreeNode[] }[]}
 */
export function buildTree(entries) {
  /** @type {Map<string, { group: any, files: FileEntry[] }>} */
  const groups = new Map();
  for (const entry of entries) {
    let bucket = groups.get(entry.group.id);
    if (!bucket) {
      bucket = { group: entry.group, files: [] };
      groups.set(entry.group.id, bucket);
    }
    bucket.files.push(entry.file);
  }
  return [...groups.values()].map((bucket) => ({
    group: bucket.group,
    nodes: buildFileTree(bucket.files),
  }));
}

/**
 * コメントを、表示行の直下へ貼る位置に振り分ける。
 * ファイル全体のコメントと、表示行に見つからないコメントは floating に残す。
 * @param {DisplayLine[]} displayLines
 * @param {any[]} comments
 * @returns {{ byLine: Map<number, any[]>, floating: any[] }}
 */
export function placeThreads(displayLines, comments) {
  /** @type {Map<string, number>} */
  const anchors = new Map();
  displayLines.forEach((line, index) => {
    if (line.oldLine) {
      anchors.set(`old:${Number(line.oldLine.number)}`, index);
    }
    if (line.newLine) {
      anchors.set(`new:${Number(line.newLine.number)}`, index);
    }
  });
  /** @type {Map<number, any[]>} */
  const byLine = new Map();
  /** @type {any[]} */
  const floating = [];
  for (const comment of comments) {
    // 範囲のコメントは範囲の最後の行の直下に置き、範囲の行を上下に分けない。
    const last = comment.end_line ?? comment.start_line;
    const key =
      comment.start_line === null || comment.start_line === undefined
        ? null
        : `${comment.side}:${Number(last)}`;
    const index = key === null ? -1 : anchors.get(key) ?? -1;
    if (index < 0) {
      floating.push(comment);
      continue;
    }
    const list = byLine.get(index) || [];
    list.push(comment);
    byLine.set(index, list);
  }
  return { byLine, floating };
}

/**
 * レビュー全体の件数と増減を集計する。
 * @param {any} review
 * @returns {{ files: number, groups: number, add: number, del: number }}
 */
export function reviewStats(review) {
  let files = 0;
  let add = 0;
  let del = 0;
  const groups = (review && review.groups) || [];
  for (const group of groups) {
    for (const file of group.files || []) {
      files += 1;
      add += file.add;
      del += file.del;
    }
  }
  return { files, groups: groups.length, add, del };
}

/**
 * ヘッダに出す meta の項目。manifest の meta を先に、集計値は最後に置く。
 * @param {any} review
 * @returns {{ label: string, value: string }[]}
 */
export function metaItems(review) {
  /** @type {{ label: string, value: string }[]} */
  const items = [];
  const meta = review && review.meta;
  if (typeof meta === "string") {
    if (meta !== "") {
      items.push({ label: "", value: meta });
    }
  } else if (meta && typeof meta === "object" && !Array.isArray(meta)) {
    for (const [label, value] of Object.entries(meta)) {
      if (value === null || value === undefined || value === "") {
        continue;
      }
      const text = typeof value === "string" ? value : JSON.stringify(value);
      if (text !== undefined) {
        items.push({ label, value: text });
      }
    }
  }
  const stats = reviewStats(review);
  items.push({ label: "ファイル", value: String(stats.files) });
  if (stats.groups > 0) {
    items.push({ label: "グループ", value: String(stats.groups) });
  }
  items.push({ label: "変更", value: `+${stats.add} −${stats.del}` });
  return items;
}

/** テーマの巡回順。 */
export const THEMES = [
  "auto",
  "light",
  "dark",
  "solarized-light",
  "solarized-dark",
];

/**
 * アイコン操作のたびに次のテーマへ進む。
 * @param {string} theme
 * @returns {string}
 */
export function nextTheme(theme) {
  const index = THEMES.indexOf(theme);
  return THEMES[(index + 1) % THEMES.length];
}

/**
 * auto のときだけシステムの配色に従い、それ以外は指定のプリセット。
 * @param {string} theme
 * @param {boolean} prefersDark
 * @returns {string}
 */
export function resolveTheme(theme, prefersDark) {
  if (theme === "auto") {
    return prefersDark ? "dark" : "light";
  }
  return theme;
}

/**
 * @param {string} resolvedTheme
 * @returns {boolean}
 */
export function isDarkTheme(resolvedTheme) {
  return resolvedTheme === "dark" || resolvedTheme === "solarized-dark";
}

/**
 * 表示行が、指定した側の行番号を指すか。表示を組み直してもエディタを
 * 開いた行へ対応付けられるよう、表示行 index ではなく論理行で照合する。
 * @param {DisplayLine} line
 * @param {string} side
 * @param {number} number
 * @returns {boolean}
 */
export function lineHasAnchor(line, side, number) {
  const target = side === "old" ? line.oldLine : line.newLine;
  return target !== null && Number(target.number) === number;
}

/**
 * 行にコメントを付けるときの既定の側。新側があれば新側、無ければ旧側。
 * @param {DisplayLine} line
 * @returns {{ side: string, number: number } | null}
 */
export function lineAnchor(line) {
  if (line.newLine) {
    return { side: "new", number: Number(line.newLine.number) };
  }
  if (line.oldLine) {
    return { side: "old", number: Number(line.oldLine.number) };
  }
  return null;
}
