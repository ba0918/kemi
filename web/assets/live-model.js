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
 * @typedef {{ page: string, widths: number[], current: boolean, mock: boolean, comments: number }} PageTreeItem
 */

/**
 * ページのツリー（R-PAGE-VIEW）。表示中のページと、スナップショット・コメント・モックの割り当てのいずれかが
 * あるページを、パスの順に並べる。各ページには、スナップショットを取った表示幅とコメントを付けた表示幅を
 * 小さい順に、コメントの数とともに。
 * @param {{ current: string, snapshots: PageWidth[], mocks: Set<string>, comments: PageWidth[] }} input
 * @returns {PageTreeItem[]}
 */
export function buildPageTree({ current, snapshots, mocks, comments }) {
  /** @type {Map<string, { widths: Set<number>, comments: number }>} */
  const pages = new Map();
  const add = (/** @type {string} */ page) => {
    let item = pages.get(page);
    if (!item) {
      item = { widths: new Set(), comments: 0 };
      pages.set(page, item);
    }
    return item;
  };
  add(current);
  for (const snapshot of snapshots) {
    add(snapshot.page).widths.add(snapshot.width);
  }
  for (const comment of comments) {
    const item = add(comment.page);
    item.widths.add(comment.width);
    item.comments += 1;
  }
  for (const page of mocks) {
    add(page);
  }
  return [...pages.entries()]
    .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
    .map(([page, item]) => ({
      page,
      widths: [...item.widths].sort((left, right) => left - right),
      current: page === current,
      mock: mocks.has(page),
      comments: item.comments,
    }));
}

/**
 * スナップショットの見出し（中身は持たない）。
 * `unsaved` は、セッションがあるのに保存できなかったもの（live.md の R-PAGE-SESSION）。
 * @typedef {{ id: string, kind: string, page: string, width: number, unsaved?: boolean }} SnapshotSummary
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

/**
 * 開始時のスナップショットを今取るか（live.md の R-PAGE-SNAPSHOT）。預けた一覧がまだ分からない間は
 * 取らない: 復元したレビューでは一覧に開始時のものがあり、先に取ると既定の比べる相手が今の見た目に
 * 替わる（R-PAGE-SESSION の反例）。
 * @param {{ taken: boolean, reachable: boolean, snapshots: SnapshotSummary[] | null }} at
 *   taken は取りに行ったか、snapshots は預けた一覧（読む前は null）
 * @returns {boolean}
 */
export function startSnapshotDue({ taken, reachable, snapshots }) {
  return !taken && reachable && snapshots !== null && !snapshots.some((snapshot) => snapshot.kind === "start");
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

/** 選択肢の中の時点の並び（live-compare.md の R-PAGE-REF）。 */
const OPTION_KINDS = ["handed", "start", "manual"];

/**
 * 比べる相手の選択に並べる時点。そのページのスナップショットを、渡した時点 → 開始時 → 手で取った時点の順に、
 * 同じ種類の中は新しい順に、表示幅を添えて。
 * @param {SnapshotSummary[]} snapshots 取った順
 * @param {string} page
 * @returns {{ id: string, label: string }[]}
 */
export function snapshotOptions(snapshots, page) {
  const here = snapshots.filter((snapshot) => snapshot.page === page);
  return OPTION_KINDS.flatMap((kind) => here.filter((snapshot) => snapshot.kind === kind).reverse()).map((snapshot) => ({
    id: snapshot.id,
    label: optionLabel(snapshots, snapshot),
  }));
}

/**
 * @param {SnapshotSummary[]} snapshots 取った順
 * @param {SnapshotSummary} snapshot
 * @returns {string}
 */
function optionLabel(snapshots, snapshot) {
  const label = `${snapshotLabel(snapshots, snapshot)} · ${snapshot.width}`;
  return snapshot.unsaved ? `${label} · not saved` : label;
}

/** 自動の選び方の短い説明。選択肢のそばに出す（R-PAGE-REF）。 */
export const AUTO_RULE = "Auto picks, in order: the last snapshot you handed → Start → the last one you recorded.";

/**
 * 比べる相手の選択肢（R-PAGE-REF）。自動 → 渡した時点 → 開始時 → 手で取った時点 → モック。自動の名前には、いまの
 * ページと表示幅で自動が選んでいる時点を、その時点の選択肢と同じ名前で入れる。
 * @param {{ snapshots: SnapshotSummary[], page: string, width: number, mock: string | null }} input
 * @returns {{ value: string, label: string }[]}
 */
export function referenceOptions({ snapshots, page, width, mock }) {
  const picked = chooseSnapshot(snapshots, page, width, null);
  const auto = picked ? `Auto — ${optionLabel(snapshots, picked)}` : "Auto — not recorded at this width";
  return [
    { value: "latest", label: auto },
    ...snapshotOptions(snapshots, page).map((option) => ({ value: option.id, label: option.label })),
    ...(mock === null ? [] : [{ value: "mock", label: mockLabel(mock) }]),
  ];
}

/**
 * いま比べている相手の名前。選択肢の項目と同じ作り（R-PAGE-REF）。
 * @param {SnapshotSummary[]} snapshots 取った順
 * @param {Reference} reference
 * @returns {string}
 */
export function referenceName(snapshots, reference) {
  switch (reference.type) {
    case "snapshot":
      return optionLabel(snapshots, reference.snapshot);
    case "mock":
      return mockLabel(reference.path);
    case "none":
      return "Not recorded";
  }
}

/**
 * @param {string} path
 * @returns {string}
 */
function mockLabel(path) {
  return `Mock: ${path}`;
}

/**
 * 見比べ方の見出し（R-PAGE-REF）。重ねて透かす間は、両方の名前と透かし具合。透かし具合が端のときは見えている方を
 * 出し、重ねていることも分かる書き方にする（見る対象だけの見出しと同じにしない）。狭い画面では見比べ方によらず
 * 1 枚ずつ見る（R-PAGE-VIEW）ので、見ている 1 枚に合わせる: 動いているページなら見る対象だけの見出し、比べる相手
 * ならその名前。
 * @param {{ compare: "now" | "side" | "overlay", reference: string, opacity: number, shown?: "live" | "ref" | null }} input
 *   reference は比べる相手の名前、opacity は重ねた比べる相手の不透明度（0〜100）、shown は狭い画面で見ている 1 枚
 *   （広い画面では null）
 * @returns {string}
 */
export function compareHeading({ compare, reference, opacity, shown = null }) {
  if (shown === "ref") {
    return reference;
  }
  if (compare === "now" || shown === "live") {
    return "Now";
  }
  if (compare === "side") {
    return "Side by side";
  }
  if (opacity <= 0) {
    return "Overlay — Now only visible";
  }
  if (opacity >= 100) {
    return `Overlay — ${reference} only visible`;
  }
  return `Overlay — Now ⇄ ${reference} · ${opacity}%`;
}

/**
 * 保存できなかったスナップショットの知らせ。レビューの間は使えるが、復元すると消える
 * （live.md の R-PAGE-SESSION）。保存したものでは空。
 * @param {SnapshotSummary} snapshot
 * @returns {string}
 */
export function unsavedSnapshotNotice(snapshot) {
  return snapshot.unsaved ? "Not saved: this snapshot is gone when the review is resumed" : "";
}

/**
 * ページへのコメントを足した応答から、画像を保存できなかった知らせを作る（20 MB の規則。
 * live.md の R-PAGE-SESSION）。保存したとき・画像が無いときは空。
 * @param {{ image_unsaved?: boolean }} answer
 * @returns {string}
 */
export function imageUnsavedNotice(answer) {
  return answer.image_unsaved === true
    ? "The comment was saved without its image: the session's files would exceed 20 MB"
    : "";
}

/**
 * @typedef {{ type: "mock", path: string } | { type: "snapshot", snapshot: SnapshotSummary } | { type: "none" }} Reference
 */

/**
 * 比べる相手（live-compare.md の R-PAGE-REF）。選んでいなければ、モックを割り当てたページは
 * モック、無ければスナップショットの既定の順。`latest` はモックがあってもスナップショットの
 * 既定の順で、スナップショットの id はその時点。モックを選んでいても割り当てが無ければ
 * スナップショットに戻る。
 * @param {{ snapshots: SnapshotSummary[], mock: string | null, page: string, width: number, chosen: string | undefined }} input
 * @returns {Reference}
 */
export function chooseReference({ snapshots, mock, page, width, chosen }) {
  if (mock !== null && (chosen === undefined || chosen === "mock")) {
    return { type: "mock", path: mock };
  }
  const point = chosen === undefined || chosen === "mock" || chosen === "latest" ? null : chosen;
  const snapshot = chooseSnapshot(snapshots, page, width, point);
  return snapshot ? { type: "snapshot", snapshot } : { type: "none" };
}

/**
 * 重ねて透かすときの比べる相手の置き方（live-compare.md の DC2）。比べる相手は自分では
 * スクロールしない（スクリプトを止めた枠、別のオリジンの枠）ので、中身の高さで描き、
 * 見る対象のスクロールの分だけ外側でずらす。
 * @param {{ scale: number, viewportHeight: number, scrollX: number, scrollY: number, contentHeight: number }} input
 * @returns {{ height: number, transform: string }}
 */
export function overlayPlacement({ scale, viewportHeight, scrollX, scrollY, contentHeight }) {
  const height = Math.max(viewportHeight / scale, contentHeight);
  const x = -scrollX * scale;
  const y = -scrollY * scale;
  return { height, transform: `translate(${x === 0 ? 0 : x}px, ${y === 0 ? 0 : y}px) scale(${scale})` };
}

/**
 * 書いている途中のページへのコメントの場所（live.md の R-PAGE-COMMENT）。場所は最初の場所の URL と表示幅の
 * ものなので、それも覚える。`next` は次に振る番号で、消しても戻さない（本文が番号で指すため）。
 * @typedef {{ selector: string, text: string, rect: { x: number, y: number, w: number, h: number } }} PlaceElement
 * `at` は要素の場所の押した点（文書の座標）。画像の頼みにだけ載せ、保存する場所には入れない。
 * @typedef {{ kind: "element" | "arrow" | "pen", points: { x: number, y: number }[], elements: PlaceElement[], at?: { x: number, y: number } }} NewPlace
 * @typedef {NewPlace & { n: number }} Place
 * @typedef {{ url: string, width: number, places: Place[], next: number }} PlaceDraft
 */

/**
 * @param {string} url
 * @param {number} width
 * @returns {PlaceDraft}
 */
export function emptyDraft(url, width) {
  return { url, width, places: [], next: 1 };
}

/**
 * 書きかけのコメントの場所が、見ている URL と表示幅のものでないとき、その URL と表示幅（R-PAGE-COMMENT）。
 * 場所が 1 つも無ければ、どこにも結びついていない。
 * @param {PlaceDraft} draft
 * @param {string} url
 * @param {number} width
 * @returns {{ url: string, width: number } | null}
 */
export function draftElsewhere(draft, url, width) {
  if (draft.places.length === 0 || (draft.url === url && draft.width === width)) {
    return null;
  }
  return { url: draft.url, width: draft.width };
}

/**
 * 場所を足す。同じ要素をもう一度選んだら、その要素の場所を外す。場所は最初の場所の URL と表示幅のものだけなので、
 * 別の URL か表示幅では足さない。場所が 1 つも無ければ、足す場所の URL と表示幅に移る（番号は戻さない）。
 * @param {PlaceDraft} draft
 * @param {NewPlace} place
 * @param {string} url
 * @param {number} width
 * @returns {PlaceDraft}
 */
export function addPlace(draft, place, url, width) {
  if (draftElsewhere(draft, url, width)) {
    return draft;
  }
  const base = { ...draft, url, width };
  const selector = place.kind === "element" ? place.elements[0]?.selector : undefined;
  const chosen = base.places.find((item) => item.kind === "element" && selector !== undefined && item.elements[0]?.selector === selector);
  if (chosen) {
    return removePlace(base, chosen.n);
  }
  return { ...base, places: [...base.places, { ...place, n: base.next }], next: base.next + 1 };
}

/**
 * @param {PlaceDraft} draft
 * @param {number} n
 * @returns {PlaceDraft}
 */
export function removePlace(draft, n) {
  return { ...draft, places: draft.places.filter((item) => item.n !== n) };
}

/**
 * 最後に足した場所を外す。
 * @param {PlaceDraft} draft
 * @returns {PlaceDraft}
 */
export function undoPlace(draft) {
  const last = Math.max(0, ...draft.places.map((item) => item.n));
  return removePlace(draft, last);
}
