// @ts-check
// ページの状態と、その状態から決まる読み。ここは api も要素も触らない。

import { balloonShown, collapseDefault, effectiveDisplay, seenProgress } from "./model.js";

/** @typedef {import("./model.js").RenderedBlock} RenderedBlock */
/** @typedef {{ url: string, size: number }} ImageSide */
import { loadMode, loadTheme } from "./storage.js";

/** @typedef {import("./model.js").FileEntry} FileEntry */
/** @typedef {import("./model.js").LogicalRow} LogicalRow */
/** @typedef {import("./model.js").DisplayLine} DisplayLine */

export const ROW_HEIGHT = 24;

export const OVERSCAN = 12;

/** 変更間の移動で、止まる場所を画面の上端からこの分だけ下に置く（前の文脈を見せる）。 */
export const NAV_MARGIN = 48;

/** @type {Record<string, string>} */
const UNIT_LABELS = { file: "Final state", commit: "Per commit" };

/**
 * グループ単位の画面での呼び名。知らない単位は、サーバが返した名前のまま出す。
 * @param {string} unit
 * @returns {string}
 */
export function unitLabel(unit) {
  return UNIT_LABELS[unit] || unit;
}

/** @type {Record<string, string>} */
export const THEME_LABELS = {
  auto: "Auto",
  light: "light",
  dark: "dark",
  "solarized-light": "solarized light",
  "solarized-dark": "solarized dark",
};

/**
 * @typedef {{ file: FileEntry, group: any }} Entry
 * @typedef {{ fileId: string, side: string, start: number, end: number, anchor: number }} Selection
 * @typedef {{ fileId: string, side: string, start: number, end: number, anchor: number, wide: boolean, body: string, suggestion: string, suggestionOn: boolean, needsFocus: boolean, editId?: string }} Editor
 */

/** @type {{
 *   review: any,
 *   entries: Entry[],
 *   visible: Entry[],
 *   current: Entry|null,
 *   index: number,
 *   mode: "unified" | "split",
 *   wrap: boolean,
 *   narrow: boolean,
 *   narrowWrap: boolean,
 *   narrowComments: boolean,
 *   narrowOnlyComment: string | null,
 *   drawerOpen: boolean,
 *   horizontal: import("./model.js").HorizontalState,
 *   horizontalMeasurePending: boolean,
 *   focusOnly: boolean,
 *   sortBySize: boolean,
 *   theme: string,
 *   cache: Map<string, any>,
 *   commentStore: Map<string, any[]>,
 *   rows: LogicalRow[],
 *   display: DisplayLine[],
 *   displayFileId: string | null,
 *   heights: number[],
 *   staleRows: Set<number>,
 *   threads: { byLine: Map<number, any[]>, floating: any[] },
 *   binary: boolean,
 *   collapsedOverrides: Record<string, boolean>,
 *   measureNext: boolean,
 *   highlightOverrides: Map<string, "on" | "off">,
 *   highlightCapable: boolean,
 *   highlightEnabled: boolean,
 *   dark: boolean,
 *   cacheKey: string,
 *   selectGeneration: number,
 *   comments: any[],
 *   selection: Selection | null,
 *   editor: Editor | null,
 *   dragging: { fileId: string, side: string } | null,
 *   submitted: boolean,
 *   updateAvailable: boolean,
 *   treeItems: Map<string, HTMLButtonElement>,
 *   treeActiveId: string | null,
 *   treeVersion: number,
 *   treeRenderedVersion: number,
 *   groupOpen: Map<string, boolean>,
 *   dirOpen: Map<string, boolean>,
 *   groupHeaderOpen: Map<string, boolean>,
 *   commentOpen: Map<string, boolean>,
 *   commented: Set<number>,
 *   loading: boolean,
 *   navigating: boolean,
 *   origins: Map<string, any>,
 *   originForced: Set<string>,
 *   originOpen: Map<string, string>,
 *   skipRanges: Map<number, any>,
 *   units: any[],
 *   unit: string | null,
 *   reviews: Map<string, any>,
 *   pendingUnit: { unit: string, jump: any } | null,
 *   allComments: any[],
 *   stops: number[],
 *   rulerDirty: boolean,
 *   pendingJump: { side: string, line: number } | "first" | "last" | null,
 *   toastTimer: number,
 *   groupHeads: Map<string, { root: HTMLElement, count: HTMLElement, bar: HTMLElement }>,
 *   modalAction: (() => void) | null,
 *   landing: { index: number, top: number, scrollTop: number, margin: number } | null,
 *   renderedFiles: Map<string, boolean>,
 *   renderInfo: { target: string | null, toggle: boolean, initial: string, reason: string | null } | null,
 *   renderCache: Map<string, any>,
 *   renderedActive: boolean,
 *   renderLoading: boolean,
 *   renderFailure: string | null,
 *   rendered: {
 *     kind: "document" | "image",
 *     html: string,
 *     blocks: RenderedBlock[],
 *     oldLines: string[],
 *     newLines: string[],
 *     image: { old: ImageSide | null, new: ImageSide | null, same: boolean } | null,
 *   } | null,
 *   renderedStops: number[],
 *   renderedThreads: { byBlock: Map<number, any[]>, top: any[], floating: any[] },
 * }} */
export const state = {
  review: null,
  entries: [],
  visible: [],
  current: null,
  index: 0,
  mode: loadMode(),
  wrap: false,
  // 狭い画面（R-NARROW）。幅は app.js の matchMedia が入れる。折返しの値はページを
  // 開いている間だけ覚え、localStorage には入れない。
  narrow: false,
  narrowWrap: true,
  // 狭い画面の吹き出し。narrowComments が真なら畳んだ札で出す（既定）。「Comments」の
  // 切り替え（ページを開いている間だけ覚える）で札ごと隠し、隠している間はコメント一覧から
  // 選んだそのコメントだけの印で出す。
  narrowComments: true,
  narrowOnlyComment: null,
  drawerOpen: false,
  horizontal: { entry: null, width: 0, left: 0 },
  horizontalMeasurePending: false,
  focusOnly: false,
  sortBySize: false,
  theme: loadTheme(),
  cache: new Map(),
  commentStore: new Map(),
  rows: [],
  display: [],
  displayFileId: null,
  heights: [],
  staleRows: new Set(),
  threads: { byLine: new Map(), floating: [] },
  binary: false,
  collapsedOverrides: {},
  measureNext: false,
  highlightOverrides: new Map(),
  highlightCapable: false,
  highlightEnabled: false,
  dark: false,
  cacheKey: "",
  selectGeneration: 0,
  comments: [],
  selection: null,
  editor: null,
  dragging: null,
  submitted: false,
  updateAvailable: false,
  treeItems: new Map(),
  treeActiveId: null,
  treeVersion: 0,
  treeRenderedVersion: -1,
  groupOpen: new Map(),
  dirOpen: new Map(),
  groupHeaderOpen: new Map(),
  commentOpen: new Map(),
  commented: new Set(),
  loading: false,
  navigating: false,
  origins: new Map(),
  originForced: new Set(),
  originOpen: new Map(),
  skipRanges: new Map(),
  units: [],
  unit: null,
  reviews: new Map(),
  pendingUnit: null,
  allComments: [],
  stops: [],
  rulerDirty: true,
  pendingJump: null,
  toastTimer: 0,
  groupHeads: new Map(),
  modalAction: null,
  landing: null,
  // 描画表示（R-RENDER）。ファイルごとの切り替えはページを開いている間だけ覚え、
  // localStorage にもサーバのセッション状態にも書かない。
  renderedFiles: new Map(),
  renderInfo: null,
  renderCache: new Map(),
  renderedActive: false,
  renderLoading: false,
  renderFailure: null,
  rendered: null,
  renderedStops: [],
  renderedThreads: { byBlock: new Map(), top: [], floating: [] },
};

export function currentEntry() {
  return state.current;
}

/** 描画に使う表示モード。狭い画面では常に 1 列（R-NARROW）。 */
export function displayMode() {
  return effectiveDisplay(state).mode;
}

/** 描画に使う折返し。狭い画面では狭い画面用の値（R-NARROW）。 */
export function displayWrap() {
  return effectiveDisplay(state).wrap;
}

/**
 * 吹き出し（畳んだ札を含む）を描くコメントだけに絞る。狭い画面で「Comments」で隠して
 * いる間は描かない（R-NARROW）。編集中のコメントは、入力欄が消えないよう常に描く。
 * @param {any[]} comments
 * @returns {any[]}
 */
export function shownComments(comments) {
  const editing = state.editor ? state.editor.editId : undefined;
  return comments.filter((comment) => comment.id === editing || balloonShown(state, comment.id));
}

/**
 * 表示中のファイルが指定のファイルと一致するか。
 *
 * 応答待ちの間に別ファイルへ切り替えて戻っても表示を更新できるよう、
 * 一致は選択の世代ではなくファイル id で判定する。
 * @param {string} fileId
 * @returns {boolean}
 */
export function isShowingFile(fileId) {
  const entry = currentEntry();
  return entry !== null && entry.file.id === fileId;
}

/**
 * @param {any} review
 * @returns {Entry[]}
 */
export function flatten(review) {
  /** @type {Entry[]} */
  const entries = [];
  for (const group of review.groups) {
    for (const file of group.files) {
      entries.push({ file, group });
    }
  }
  return entries;
}

/**
 * ファイルのコメント（ファイル全体のコメントを含む）。コメントの JSON は submit の契約の
 * 形でファイル id を持たないので、サーバが id を振るのと同じ（グループ, パス）で対応付ける。
 * @param {Entry} entry
 * @returns {any[]}
 */
export function commentsOf(entry) {
  return state.allComments.filter(
    (comment) => comment.group_id === entry.group.id && comment.path === entry.file.path,
  );
}

/**
 * 表示中のファイルを描画表示で見たいか。切り替えていなければ `api/file` の既定に従う。
 * @param {Entry} entry
 * @returns {boolean}
 */
export function renderedWanted(entry) {
  const chosen = state.renderedFiles.get(entry.file.id);
  if (chosen !== undefined) {
    return chosen;
  }
  return Boolean(state.renderInfo && state.renderInfo.initial === "rendered");
}

/**
 * 描画表示の切り替えが押せるか。対象でない、取得中、事前に分かる描画不可では押せない。
 * @returns {boolean}
 */
export function renderToggleEnabled() {
  const info = state.renderInfo;
  return Boolean(info && info.toggle && !info.reason) && !state.loading && !state.renderLoading;
}

/**
 * ファイルの行データのキャッシュの鍵。着色は表示色とハイライトの指定で変わる。
 * @param {Entry} entry
 * @returns {string}
 */
export function fileCacheKey(entry) {
  const id = entry.file.id;
  const override = state.highlightOverrides.get(id);
  return `${id}|${state.dark ? 1 : 0}|${override ?? "auto"}`;
}

/**
 * @param {string} fileId
 * @returns {string}
 */
export function originKey(fileId) {
  return `${fileId}|${state.originForced.has(fileId) ? 1 : 0}`;
}

/** 最終形（由来を持つ単位）を表示しているか。 */
export function originAvailable() {
  return Boolean(state.review && state.review.unit === "file");
}

/** 表示中のファイルで由来の行を出すか。上限を超えるファイルは有効にしたときだけ。 */
export function originShown() {
  const entry = currentEntry();
  if (!entry || !originAvailable() || state.binary) {
    return false;
  }
  return state.highlightCapable || state.originForced.has(entry.file.id);
}

/** 表示中のファイルの由来。取得中は "pending"、まだなら undefined。 */
export function currentOrigin() {
  const entry = currentEntry();
  return entry ? state.origins.get(originKey(entry.file.id)) : undefined;
}

/** 表示中のファイルで止まれる場所。畳まれたノイズやバイナリでは止まらない。 */
export function reachableStops() {
  const entry = currentEntry();
  if (!entry || state.binary || collapseDefault(entry.file, state.collapsedOverrides)) {
    return [];
  }
  return state.stops;
}

/**
 * @param {string} groupId
 * @returns {{ seen: number, total: number, done: boolean }}
 */
export function groupProgressFor(groupId) {
  return seenProgress(
    state.entries.filter((entry) => entry.group.id === groupId).map((entry) => entry.file),
  );
}

export function allLinesExpanded() {
  const data = state.cache.get(state.cacheKey);
  if (!data || !data.collapsedRows) {
    return false;
  }
  return (
    state.rows !== data.collapsedRows &&
    !state.rows.some((row) => row.kind === "skip")
  );
}

/**
 * @param {"old" | "new"} side
 * @param {number} number
 * @returns {boolean}
 */
export function selectionContains(side, number) {
  const selection = state.selection;
  return Boolean(
    selection &&
      selection.side === side &&
      number >= selection.start &&
      number <= selection.end,
  );
}

/**
 * 選択中の範囲の最後の行か。狭い画面では、この行にだけ `+` を出す（R-NARROW）。
 * @param {"old" | "new"} side
 * @param {number} number
 * @returns {boolean}
 */
export function selectionEndsAt(side, number) {
  const selection = state.selection;
  return Boolean(selection && selection.side === side && selection.end === number);
}

export function selectionText() {
  const selection = state.selection;
  if (!selection) {
    return "";
  }
  const texts = [];
  for (const row of state.rows) {
    const line = selection.side === "new" ? row.new : row.old;
    if (
      line &&
      Number(line.number) >= selection.start &&
      Number(line.number) <= selection.end
    ) {
      texts.push(line.text);
    }
  }
  return texts.join("\n");
}

/** コミットごとの単位の、グループ id と件名（読んであれば）。 */
export function commitGroups() {
  const review = state.reviews.get("commit");
  if (!review) {
    return null;
  }
  return new Map(review.groups.map((/** @type {any} */ group) => [group.id, group.title]));
}
