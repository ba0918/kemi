// @ts-check
// `--live` のページの見方（live.md の R-PAGE-VIEW）の純粋な読み。要素も通信も触らない。
// `--live` のレビューでだけ読み込む（R-VERIFY: ほかのレビューはページ用のファイルを読まない）。

/** 表示幅の既定の候補（R-PAGE-VIEW）。 */
export const WIDTH_CHOICES = [390, 768, 1280];
export const WIDTH_MIN = 320;
export const WIDTH_MAX = 3840;

/**
 * 入れた表示幅を読む。範囲の外と整数でないものは、範囲を添えて断る。
 * @param {string} text
 * @returns {{ ok: true, width: number } | { ok: false, message: string }}
 */
export function parseWidth(text) {
  const trimmed = text.trim();
  const width = /^\d+$/.test(trimmed) ? Number(trimmed) : NaN;
  if (Number.isInteger(width) && width >= WIDTH_MIN && width <= WIDTH_MAX) {
    return { ok: true, width };
  }
  return { ok: false, message: `Enter a width from ${WIDTH_MIN} to ${WIDTH_MAX}` };
}

/**
 * 表示幅のページを枠の幅に収める倍率。縮めるだけで、広げない。
 * @param {number} paneWidth
 * @param {number} width
 * @returns {number}
 */
export function fitScale(paneWidth, width) {
  if (paneWidth <= 0 || width <= 0) {
    return 1;
  }
  return Math.min(1, paneWidth / width);
}

/**
 * ページを区別する鍵。パスとクエリで、`#` から後ろは見ない（R-PAGE-VIEW）。
 * @param {string} path
 * @returns {string}
 */
export function pageKey(path) {
  const withoutFragment = path.split("#")[0];
  return withoutFragment === "" ? "/" : withoutFragment;
}

/**
 * 中継のオリジン。レビュー画面と同じホスト名で開く（中継用の cookie はホストで決まり、
 * ポートを区別しないので、同じホスト名なら付く）。
 * @param {string} protocol
 * @param {string} hostname
 * @param {number} port
 * @returns {string}
 */
export function liveOrigin(protocol, hostname, port) {
  return `${protocol}//${hostname}:${port}`;
}

/**
 * @typedef {{ page: string, width: number }} PageWidth
 * @typedef {{ page: string, widths: number[], current: boolean, mock: boolean }} PageTreeItem
 */

/**
 * ページのツリー（R-PAGE-VIEW）。表示中のページと、スナップショットかモックの割り当てが
 * あるページを、パスの順に並べる。各ページにはスナップショットのある表示幅を小さい順に。
 * @param {{ current: string, snapshots: PageWidth[], mocks: Set<string> }} input
 * @returns {PageTreeItem[]}
 */
export function buildPageTree({ current, snapshots, mocks }) {
  /** @type {Map<string, Set<number>>} */
  const pages = new Map();
  const add = (/** @type {string} */ page) => {
    if (!pages.has(page)) {
      pages.set(page, new Set());
    }
    return /** @type {Set<number>} */ (pages.get(page));
  };
  add(current);
  for (const snapshot of snapshots) {
    add(snapshot.page).add(snapshot.width);
  }
  for (const page of mocks) {
    add(page);
  }
  return [...pages.entries()]
    .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
    .map(([page, widths]) => ({
      page,
      widths: [...widths].sort((left, right) => left - right),
      current: page === current,
      mock: mocks.has(page),
    }));
}

/**
 * スナップショットの見出し（中身は持たない）。
 * @typedef {{ id: string, kind: string, page: string, width: number }} SnapshotSummary
 */

/**
 * 比べる相手のスナップショット（live-compare.md の R-PAGE-REF）。時点を選んでいればその
 * スナップショットで、ページか表示幅が違えば無い。選んでいなければ、そのページと表示幅の
 * ものから、最後に渡した時点 → 開始時 → 最後に手で取ったもの、の順に最初に見つかったもの。
 * @param {SnapshotSummary[]} snapshots 取った順
 * @param {string} page
 * @param {number} width
 * @param {string | null} chosen 選んだ時点（スナップショットの id）
 * @returns {SnapshotSummary | null}
 */
export function chooseSnapshot(snapshots, page, width, chosen) {
  const here = snapshots.filter((snapshot) => snapshot.page === page && snapshot.width === width);
  if (chosen !== null) {
    return here.find((snapshot) => snapshot.id === chosen) ?? null;
  }
  for (const kind of ["handed", "start", "manual"]) {
    const found = here.filter((snapshot) => snapshot.kind === kind).at(-1);
    if (found) {
      return found;
    }
  }
  return null;
}

/** @type {Record<string, string>} */
const KIND_LABELS = { start: "Start", handed: "Handed", manual: "Recorded" };

/**
 * 時点の呼び名。開始時はひとつ、渡した時点と手で取った時点は種類ごとに数える。
 * @param {SnapshotSummary[]} snapshots 取った順
 * @param {SnapshotSummary} snapshot
 * @returns {string}
 */
export function snapshotLabel(snapshots, snapshot) {
  const label = KIND_LABELS[snapshot.kind] ?? snapshot.kind;
  if (snapshot.kind === "start") {
    return label;
  }
  const index = snapshots.filter((other) => other.kind === snapshot.kind).indexOf(snapshot);
  return `${label} ${index + 1}`;
}

/**
 * 比べる相手の選択に並べる時点。そのページのスナップショットを新しい順に、表示幅を添えて。
 * @param {SnapshotSummary[]} snapshots 取った順
 * @param {string} page
 * @returns {{ id: string, label: string }[]}
 */
export function snapshotOptions(snapshots, page) {
  return snapshots
    .filter((snapshot) => snapshot.page === page)
    .reverse()
    .map((snapshot) => ({ id: snapshot.id, label: `${snapshotLabel(snapshots, snapshot)} · ${snapshot.width}` }));
}
