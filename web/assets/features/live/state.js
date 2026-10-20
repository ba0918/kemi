// @ts-check
// `--live` のページの見方の共有の状態。入口（features/live.js）と features/live/ の下の部品が読み書きする。
//
// why not: 状態を部品ごとに分けない。スナップショットの一覧・表示中のページと表示幅・モックの割り当てのように、
// 1 つの値を複数の部品が読む（変化の一覧も、枠の配置も、帯も、比べる相手を決めるのに読む）ので、分けると部品どうしが
// 互いに import し合う。

import { chooseReference, emptyDraft } from "../../live-model.js";
import { state } from "../../state.js";

/**
 * `page_comment_saved` は、このレビューでページへのコメントを一度でも保存したか（始め方の案内を出すかを決める）。
 * @typedef {{ port: number, start: string, page: string, code: boolean, code_unavailable?: string|null, page_comment_saved?: boolean }} LiveInfo
 * @typedef {import("../../live-model.js").SnapshotSummary} SnapshotSummary
 * @typedef {import("../../live-diff.js").Description} Description
 * @typedef {import("../../live-diff.js").Change} Change
 */

/** 表示幅の既定。 */
const DEFAULT_WIDTH = 1280;

export const live = {
  /** @type {LiveInfo | null} */
  info: null,
  origin: "",
  /** @type {"page" | "code"} */
  view: "page",
  width: DEFAULT_WIDTH,
  /** 表示中のページ（パスとクエリ）。 */
  page: "/",
  reachable: false,
  /** @type {string[]} */
  rewrote: [],
  /** 狭い画面で見ている側。 */
  /** @type {"live" | "ref"} */
  side: "live",
  /** 枠に収めるためにかけている倍率。 */
  scale: 1,
  /**
   * 消えた要素の行を押して、比べる相手の枠を中身の高さで描き、外側で `y`（スナップショットの文書の座標）だけずらして
   * いる間の形。`key` はずらし始めたときの比べる相手・見比べ方・ページ・表示幅で、どれかが変われば元の形（枠の中で
   * 自分でスクロールする形）に戻す。比べる相手の枠はスクリプトを止めていて、中を動かせないため。
   * @type {{ key: string, y: number } | null}
   */
  refShift: null,
  /**
   * 重ねて透かす間に消えた要素の行を押し、その要素が今のページの末尾より下にあって、ページをそこまでスクロールできない
   * ときの、見せたい位置 `y`（文書の座標）。ページのスクロールで届かない分だけ両方の枠を外側で上へずらし、そろえたまま
   * （R-PAGE-REF）その要素を見せる。`key` は refShift と同じ条件で、変われば外す。ページが上へスクロールしたときと、
   * ページの上の何か（変化の行の要素や場所）を見せるときに外し、上へのホイールはまず届かない分を戻す。
   * @type {{ key: string, y: number } | null}
   */
  overscroll: null,
  /**
   * 比べる相手の側で光らせている要素の箱（スナップショットの文書の座標の [左, 上, 幅, 高さ]）と、そのスナップショット。
   * @type {{ snapshot: string, box: number[] } | null}
   */
  refGlow: null,
  /** @type {SnapshotSummary[]} 取った順 */
  snapshots: [],
  /** ページごとに選んだ時点（スナップショットの id）。無ければ既定の順で選ぶ。 */
  /** @type {Map<string, string>} */
  chosen: new Map(),
  /** 開始時のスナップショットを取りに行ったか。 */
  startTaken: false,
  /** 預けたスナップショットの一覧を一度読めたか。読めるまで開始時のスナップショットは取らない。 */
  snapshotsListed: false,
  /** 取れなかったときの知らせ。次に描くまで出す。 */
  refNotice: "",
  /** 手で取る操作の完了を待っている間、別の手で取る操作を重ねない。 */
  recording: false,
  /**
   * 手で取って比べる相手を切り替えたスナップショット。取れたことを知らせる（live-compare.md の R-PAGE-REF）。次に撮るか、
   * ページを移るか、比べる相手を選び直すまで知らせておく。
   * @type {{ page: string, id: string } | null}
   */
  recorded: null,
  /**
   * 外したモック。取り消す操作つきで知らせ、取り消すと同じパスで割り当て直す（R-PAGE-MOCK）。知らせは少しの間だけ出す。
   * @type {{ page: string, path: string } | null}
   */
  removedMock: null,
  /**
   * 外したモックを取り消しで割り当て直せなかった理由（R-PAGE-MOCK の、断ったら理由を出す）。パネルは閉じているので帯に出す。
   * 次にモックを割り当てるか外すか、そのページを手で取るか、ページを移るまで出す。
   * @type {{ page: string, text: string } | null}
   */
  undoMockFailed: null,
  /** @type {Map<string, string>} 中身の写し（id → HTML） */
  bodies: new Map(),
  /** 比べる相手の枠に今出しているスナップショット。 */
  shownSnapshot: "",
  /** 比べる相手の枠に今入れている中身（スナップショットの id と、印を付けた消えた要素の番号）。 */
  shownFrame: "",
  /** @type {Map<string, boolean>} スナップショットが要素の対応を持つか（id → 持つか）。枠に出したときに分かる。 */
  mapped: new Map(),
  /** @type {Map<string, { path: string, url: string }>} ページごとのモックの割り当て */
  mocks: new Map(),
  /** モックを出し始めたときの条件。変わったら読み直す（R-PAGE-MOCK の読むきっかけ）。 */
  mockShownKey: "",
  /** 読み直す操作を押した。 */
  mockReloadAsked: false,
  /** 出し始めるときに読めなかったモックの条件（mockShownKey と同じ形）。読めたら空。 */
  mockUnreadable: "",
  /** 見比べ方。見る対象だけ・並べる・重ねて透かす（live-compare.md の R-PAGE-REF）。画面を開いている間だけ覚える。 */
  /** @type {"now" | "side" | "overlay"} */
  compare: "now",
  /** 枠に合わせて縮めるか、等倍で枠の中をスクロールして見るか（R-PAGE-VIEW）。画面を開いている間だけ覚える。 */
  /** @type {"fit" | "full"} */
  zoom: "fit",
  /** 変化の印を出すか（R-PAGE-VIEW）。隠しても一覧は出す。画面を開いている間だけ覚える。 */
  marks: true,
  /** 重ねた比べる相手の不透明度（0〜100）。 */
  opacity: 50,
  /** 見る対象のスクロールの位置と中身の高さ（中継したページが知らせる）。 */
  scroll: { x: 0, y: 0, height: 0 },
  /** @type {Map<string, Description | null>} スナップショットの記述（id → 記述。無ければ null） */
  descriptions: new Map(),
  /** 動いているページの最後の記述と、そのページ（パスとクエリ）。 */
  /** @type {{ page: string, description: Description } | null} */
  now: null,
  /** ページが変わったと知らせてきた回数。記述を頼んだ時点の回数と違えば、その記述は古い。 */
  pageChanges: 0,
  /** 最後の記述を頼んだ時点の pageChanges。 */
  describedAt: -1,
  /** 記述を頼んで返事を待っている。 */
  describing: false,
  /** 最後に頼んだ記述が返らなかった・読めなかったときの知らせ。次のきっかけ（ページの変化、読み込み、表示幅）まで頼み直さない。 */
  describeFailure: "",
  /** @type {Set<string>} 取り寄せられなかったスナップショット（id）。次のきっかけまで取りに行かない。 */
  unloadable: new Set(),
  /** ページが読み込みを知らせてきた回数。 */
  pageLoads: 0,
  /** 表示中のページの変化の一覧。比べていなければ null（R-PAGE-DIFF）。 */
  /** @type {Change[] | null} */
  changes: null,
  /** 変化の一覧を計算したときの比べる相手と記述。変わらなければ計算し直さない。 */
  /** @type {{ snapshot: string, now: Description } | null} */
  changesFrom: null,
  /** 比べられないときに一覧の代わりに出す知らせ（R-PAGE-VIEW）。比べられたら空。 */
  changesNotice: "",
  /** ずれただけを開いているか。同じページの間だけ保ち、ページを移ったら畳む。 */
  shiftedOpen: false,
  /** 一覧に並べた項目の数（主な変化とずれただけ）。同じページの間だけ保ち、ページを移ったら戻す。 */
  listed: { main: 0, shifted: 0 },
  /** 選んでいる道具（R-PAGE-COMMENT）。既定は要素。「操作」ではページを普通に触れる。 */
  /** @type {"element" | "arrow" | "pen" | "interact"} */
  tool: "element",
  /** ページへのコメントを一度でも保存したか。保存するまで始め方の案内を出す（R-PAGE-COMMENT）。 */
  pageCommentSaved: false,
  /** 書いているコメントの場所。 */
  draft: emptyDraft("/", DEFAULT_WIDTH),
  /** 書く欄に出す知らせ（場所を置けなかった、保存できなかった）。 */
  composeError: "",
  /** 最後に描いたときの画面が狭い画面だったか。境をまたいだら描き直す。 */
  renderedNarrow: false,
};

/**
 * 骨組みの要素（views/live.js の buildShell）。startLive が setShell で 1 回だけ入れる。読む側は import した束縛で
 * 入れた後の値を読む。
 * @type {import("../../views/live.js").LiveShell | null}
 */
export let shell = null;

/** @param {import("../../views/live.js").LiveShell} built */
export function setShell(built) {
  shell = built;
}

/** 保存したページへのコメント（`page` を持つもの）。 */
export function pageComments() {
  return state.allComments.filter((comment) => comment.page);
}

/** 表示中のページの比べる相手。 */
export function currentReference() {
  return chooseReference({
    snapshots: live.snapshots,
    mock: live.mocks.get(live.page)?.path ?? null,
    page: live.page,
    width: live.width,
    chosen: live.chosen.get(live.page),
  });
}

/**
 * 部品から呼ぶ入口（features/live.js）の関数。描き直しと、見方・側・表示幅・ページの切り替えは入口が持ち、部品は
 * 入口を import しないので、ここを通して呼ぶ。
 * @typedef {{
 *   render: () => void,
 *   renderBand: () => void,
 *   renderReference: () => void,
 *   setSide: (side: "live" | "ref") => void,
 *   setWidth: (width: number) => void,
 *   openPage: (page: string) => void,
 * }} LiveActions
 */

/**
 * 入口の関数の入れ物。中身は startLive が最初に bindLiveActions で 1 回だけ入れる。
 * @type {LiveActions}
 */
export const liveActions = /** @type {LiveActions} */ ({});

/**
 * @param {LiveActions} bound
 */
export function bindLiveActions(bound) {
  Object.assign(liveActions, bound);
}
