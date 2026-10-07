// @ts-check
// `--live` のページの見方（live.md の R-PAGE-MODE・R-PAGE-VIEW・R-PAGE-SNAPSHOT、
// live-compare.md の R-PAGE-REF）。ページとコードの見方を切り替え、動いているページを選んだ
// 表示幅で描き、比べる相手（スナップショット）を同じ幅で並べ、ページのツリーを出す。
// app.js が `--live` のレビューでだけ動的に読み込む（R-VERIFY: ほかのレビューは読まない）。
// 中継したページとは postMessage だけで話す（R-PAGE-PROXY）。

import { actions } from "../actions.js";
import * as api from "../api.js";
import { dom } from "../dom.js";
import { state } from "../state.js";
import {
  AUTO_RULE,
  MOCK_FILE_CHOICE,
  WIDTH_CHOICES,
  addPlace,
  commentShortName,
  buildPageTree,
  chooseReference,
  compareHeading,
  fitScale,
  liveOrigin,
  overlayPlacement,
  pageKey,
  draftElsewhere,
  editBody,
  editedSpan,
  emptyDraft,
  imageUnsavedNotice,
  parseWidth,
  referenceName,
  referenceOptions,
  removePlace,
  revealScrollLeft,
  strayRefs,
  snapshotLabel,
  startSnapshotDue,
  undoPlace,
  unsavedSnapshotNotice,
} from "../live-model.js";
import { changesByElement, diffDescriptions, marksOf, sameChanges, unpackDescription } from "../live-diff.js";
import {
  buildShell,
  buildTopbar,
  markRemovedInSnapshot,
  renderChanges,
  renderCompareOptions,
  renderMockFiles,
  renderPageTree,
  renderPlaces,
  renderStrayRefs,
} from "../views/live.js";
import { handToAgent } from "./agent.js";
import { closeSheet } from "./conversation.js";
import { refresh } from "./files.js";
import { renderConversation } from "../views/conversation.js";
import { renderHeader } from "../views/header.js";
import { refreshCommentBadges } from "../views/tree.js";

/**
 * `page_comment_saved` は、このレビューでページへのコメントを一度でも保存したか（始め方の案内を出すかを決める）。
 * @typedef {{ port: number, start: string, page: string, code: boolean, page_comment_saved?: boolean }} LiveInfo
 * @typedef {import("../live-model.js").SnapshotSummary} SnapshotSummary
 * @typedef {import("../live-diff.js").Description} Description
 * @typedef {import("../live-diff.js").Change} Change
 */

/** 表示幅の既定。 */
const DEFAULT_WIDTH = 1280;

/** スナップショット 1 つの上限（R-PAGE-SNAPSHOT）。 */
const SNAPSHOT_LIMIT = 2 * 1024 * 1024;

/** ページが写しを返すまで待つ上限。 */
const CAPTURE_TIMEOUT = 15000;

/** ページが記述を返すまで待つ上限。 */
const DESCRIBE_TIMEOUT = 5000;

/** ページが置いた場所を決めて返すまで待つ上限。ペンは要素をすべて測るので長めに。 */
const PLACE_TIMEOUT = 5000;

/** ページがコメントの画像を返すまで待つ上限。過ぎたら画像なしで保存する。 */
const IMAGE_TIMEOUT = 15000;

/** これより短い矢印とペンの線は、押しただけとみなして場所にしない（画面のピクセル）。 */
const MIN_STROKE = 4;

const live = {
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
  /** コメントを保存している途中。 */
  saving: false,
  /** 書く欄に出す知らせ（場所を置けなかった、保存できなかった）。 */
  composeError: "",
  /** 最後に描いたときの画面が狭い画面だったか。境をまたいだら描き直す。 */
  renderedNarrow: false,
};

/** 描いている途中の線。点は枠の中の画面の座標（ページの CSS ピクセル）と、重ねた層の中の座標。 */
/** @type {{ pointer: number, points: { x: number, y: number }[], drawn: { x: number, y: number }[] } | null} */
let drawing = null;

/** @type {import("../views/live.js").LiveShell | null} */
let shell = null;

/** @type {import("../views/live.js").LiveTopbar | null} */
let topbar = null;

/** 写しか記述を頼んで返事を待っているもの。 */
/** @type {Map<number, (message: any) => void>} */
const pendingCaptures = new Map();
let nextCapture = 1;

/**
 * 中継したページに頼みごとをして、返事を待つ。時間内に返らなければ error を持つ返事にする。
 * @param {Window} frame
 * @param {"capture" | "describe" | "place" | "image" | "saved-at" | "glow-element" | "glow-places"} type
 * @param {number} timeout
 * @param {Record<string, unknown>} [details] 頼みごとの中身
 * @returns {Promise<any>}
 */
async function ask(frame, type, timeout, details = {}) {
  const id = nextCapture++;
  const answer = await new Promise((resolve) => {
    const timer = setTimeout(() => resolve({ error: "the page did not answer" }), timeout);
    pendingCaptures.set(id, (message) => {
      clearTimeout(timer);
      resolve(message);
    });
    frame.postMessage({ kemi: "live", ...details, type, id }, live.origin);
  });
  pendingCaptures.delete(id);
  return answer;
}

/**
 * ページの見方を始める。
 * @param {LiveInfo} info
 */
export function startLive(info) {
  live.info = info;
  live.origin = liveOrigin(location.protocol, location.hostname, info.port);
  live.page = pageKey(info.start);
  live.pageCommentSaved = info.page_comment_saved === true;
  const stylesheet = document.createElement("link");
  stylesheet.rel = "stylesheet";
  stylesheet.href = "assets/live.css";
  document.head.append(stylesheet);

  shell = buildShell();
  const main = dom.viewport.closest("main");
  main?.prepend(shell.band);
  main?.append(shell.stage, shell.noCode);
  dom.tree.before(shell.pageTree);
  mirrorDrawer(shell.pageTree);

  topbar = buildTopbar();
  dom.titleBlock.before(topbar.tabs, topbar.meta, topbar.agent);
  mirrorAgentState(topbar.agent);
  mirrorAgentState(shell.bandAgent);
  mirrorHand(shell.hand);
  const menu = shell.menu;
  // 狭い画面の帯のメニューは、帯のすぐ下に開く。
  menu.addEventListener("beforetoggle", () => {
    menu.style.top = `${Math.round(shell?.band.getBoundingClientRect().bottom ?? 0) + 4}px`;
  });
  topbar.tabs.addEventListener("click", (event) => {
    const view = /** @type {HTMLElement} */ (event.target).closest("button")?.dataset.view;
    if (view === "page" || view === "code") {
      setView(view);
    }
  });
  shell.widthSeg.addEventListener("click", (event) => {
    const width = /** @type {HTMLElement} */ (event.target).closest("button")?.dataset.width;
    if (width) {
      setWidth(Number(width));
    }
  });
  shell.widthInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      applyWidthInput();
    }
  });
  shell.widthInput.addEventListener("change", applyWidthInput);
  shell.sideSeg.addEventListener("click", (event) => {
    const side = /** @type {HTMLElement} */ (event.target).closest("button")?.dataset.side;
    if (side === "live" || side === "ref") {
      setSide(side);
    }
  });
  shell.compareSelect.addEventListener("change", () => {
    if (!shell) {
      return;
    }
    // 「モックのファイル」は選択を変えず、モックのパネルを開く（R-PAGE-MOCK）。
    if (shell.compareSelect.value === MOCK_FILE_CHOICE) {
      renderBand();
      openMockPanel(shell.compareSlot);
      return;
    }
    live.chosen.set(live.page, shell.compareSelect.value);
    live.recorded = null;
    render();
  });
  shell.modeSeg.addEventListener("click", (event) => {
    const mode = /** @type {HTMLElement} */ (event.target).closest("button")?.dataset.compare;
    if (mode === "now" || mode === "side" || mode === "overlay") {
      live.compare = mode;
      render();
    }
  });
  shell.zoomSeg.addEventListener("click", (event) => {
    const zoom = /** @type {HTMLElement} */ (event.target).closest("button")?.dataset.zoom;
    if (zoom === "fit" || zoom === "full") {
      live.zoom = zoom;
      render();
    }
  });
  shell.reloadButton.addEventListener("click", reloadPage);
  shell.opacity.addEventListener("input", () => {
    if (!shell) {
      return;
    }
    live.opacity = Number(shell.opacity.value);
    shell.stage.style.setProperty("--lv-opacity", String(live.opacity / 100));
    renderStageName();
  });
  // 等倍で横にスクロールした分は、重ねた（並べた）比べる相手も同じだけずらす。
  shell.liveViewport.addEventListener("scroll", () => {
    if (shell) {
      shell.refViewport.scrollLeft = shell.liveViewport.scrollLeft;
    }
  });
  shell.stage.style.setProperty("--lv-opacity", String(live.opacity / 100));
  startMockControls(shell);
  shell.recordButton.addEventListener("click", () => void capture("manual"));
  startComposing(shell);
  shell.refRecordButton.addEventListener("click", () => void capture("manual"));
  startShiftedReferenceMoves(shell.refWheel);
  shell.refNoticeAction.addEventListener("click", () => bandAction?.());
  window.addEventListener("message", receive);
  const resized = new ResizeObserver(() => {
    // 狭い画面との境をまたぐと、操作の置き場所・比べる相手の選択の出し入れ・見出し・並べたまま隠れている側が変わる。
    // 倍率が変わらないこともあるので、まるごと描き直す。
    if (live.renderedNarrow !== state.narrow) {
      render();
      return;
    }
    layoutFrames();
    syncLaidOutInert();
  });
  resized.observe(shell.stage);
  // 書く欄や始め方の案内の出し入れは舞台の大きさを変えず、見る対象の枠の外側だけを変える。外側も見ないと、欄を
  // 開いている間に縮めた枠の高さが、欄を閉じた後も残る（R-PAGE-COMMENT）。
  resized.observe(shell.liveViewport);

  shell.liveFrame.src = live.origin + live.page;
  setView("page");
  void api.listMocks().then((list) => {
    for (const mock of list.mocks ?? []) {
      live.mocks.set(mock.page, { path: mock.path, url: mock.url });
    }
    render();
  });
  void api.listSnapshots().then((list) => {
    live.snapshots = list.snapshots ?? [];
    live.snapshotsListed = true;
    render();
    // 読み込み直したページと復元したレビューでは、開始時のスナップショットはもう取ってあり、ここでは取らない。
    // ページが先に読み込みを知らせていれば、そのときは一覧を待って取らずにいたので、ここで取る。
    takeStartSnapshot();
  });
  // 20 MB の規則で消えたスナップショットを、読み込み直さずに選択肢から消す（R-PAGE-SESSION）。
  // 取りこぼした通知がそれだったかは分からないので、取りこぼしでも読み直す。
  api.onServerEvent("snapshots", () => void reloadSnapshots());
  api.onServerEvent("lagged", () => void reloadSnapshots());
}

/** 預けたスナップショットの一覧を読み直す。 */
async function reloadSnapshots() {
  const list = await api.listSnapshots().catch(() => null);
  if (list === null) {
    return;
  }
  live.snapshots = list.snapshots ?? [];
  render();
}

/**
 * 狭い画面の引き出しは、ファイルツリーと同じ開閉に従わせる（R-NARROW）。
 * @param {HTMLElement} pageTree
 */
function mirrorDrawer(pageTree) {
  const copy = () => {
    if (dom.tree.dataset.drawer) {
      pageTree.dataset.drawer = dom.tree.dataset.drawer;
    } else {
      delete pageTree.dataset.drawer;
    }
  };
  new MutationObserver(copy).observe(dom.tree, { attributes: true, attributeFilter: ["data-drawer"] });
  copy();
}

/**
 * 狭い画面の道具のツールバーの「Hand to agent」は、全モード共通の浮かぶもの（views/conversation.js が描く）の押せる・
 * 押せないと文言を写し、押すと同じく渡す。そばの案内は全モード共通のものをそのままツールバーの上に出すので、
 * その高さを `--lv-hand-note-height` に入れ、見る対象の下をその分も空ける（書く欄の操作を覆わせない）。
 * @param {HTMLButtonElement} target
 */
function mirrorHand(target) {
  new ResizeObserver(() => {
    document.body.style.setProperty("--lv-hand-note-height", `${dom.handFloatNote.getBoundingClientRect().height}px`);
  }).observe(dom.handFloatNote);
  const source = dom.handFloat;
  const copy = () => {
    target.disabled = source.disabled;
    target.textContent = source.textContent;
  };
  new MutationObserver(copy).observe(source, {
    attributes: true,
    attributeFilter: ["disabled"],
    childList: true,
    characterData: true,
    subtree: true,
  });
  copy();
  target.addEventListener("click", () => void handToAgent());
}

/**
 * 上部バーのエージェントの状態は、会話パネルの見出しの状態（views/conversation.js が描く）を写す。
 * @param {HTMLElement} target
 */
function mirrorAgentState(target) {
  const copy = () => {
    target.dataset.kemiAgentState = dom.agentStatus.dataset.kemiAgentState ?? "";
    target.textContent = dom.agentStatus.textContent;
  };
  new MutationObserver(copy).observe(dom.agentStatus, {
    attributes: true,
    attributeFilter: ["data-kemi-agent-state"],
    childList: true,
    characterData: true,
    subtree: true,
  });
  copy();
}

/**
 * @param {"page" | "code"} view
 */
function setView(view) {
  live.view = view;
  document.body.dataset.liveView = view;
  document.body.dataset.liveCode = String(Boolean(live.info?.code));
  if (view === "code" && live.info?.code) {
    // 隠れていた間に測れなかった差分の枠を測り直させる。
    window.dispatchEvent(new Event("resize"));
  }
  render();
  // 見たの進捗はページの見方の間は出さない（views/header.js が state.live を見る）。
  renderHeader();
}

/** 更新バッジを押した。ページの見方の間なら、コードの見方に切り替えてから読み直す（R-PAGE-MODE）。 */
export async function refreshFromBadge() {
  if (live.view !== "code") {
    setView("code");
  }
  await refresh();
  renderTopbar();
}

/** 上部バーのタブの変更ファイルの数と、ページの見方の間に出すページと表示幅。 */
function renderTopbar() {
  if (!topbar) {
    return;
  }
  for (const tab of topbar.tabs.querySelectorAll("button")) {
    tab.setAttribute("aria-pressed", String(tab.dataset.view === live.view));
  }
  /** @type {{ files: unknown[] }[]} */
  const groups = state.review?.groups ?? [];
  const files = groups.reduce((count, group) => count + group.files.length, 0);
  topbar.codeCount.textContent = live.info?.code ? `· ${files}` : "";
  topbar.codeCount.title = `${files} changed file${files === 1 ? "" : "s"}`;
  topbar.metaPage.textContent = live.page;
  topbar.metaWidth.textContent = `${live.width}px`;
}

/**
 * 狭い画面で見る側（動いているページか比べる相手か）。
 * @param {"live" | "ref"} side
 */
function setSide(side) {
  live.side = side;
  render();
}

/**
 * @param {number} width
 */
function setWidth(width) {
  // 保存している間は、画像を作る表示幅を変えない（R-PAGE-COMMENT の画像は場所のある表示幅で作る）。
  if (live.saving) {
    return;
  }
  live.width = width;
  forgetFailures();
  sendPlaces();
  if (shell) {
    // 選んでいる幅が分かるよう、プリセットの幅はそのボタンで、それ以外は欄に残して示す（R-PAGE-VIEW）。
    shell.widthInput.value = WIDTH_CHOICES.includes(width) ? "" : String(width);
    shell.widthError.hidden = true;
  }
  render();
}

function applyWidthInput() {
  if (!shell || shell.widthInput.value.trim() === "") {
    return;
  }
  const result = parseWidth(shell.widthInput.value);
  if (result.ok) {
    setWidth(result.width);
    return;
  }
  shell.widthError.textContent = result.message;
  shell.widthError.hidden = false;
}

/** 見る対象を手で読み込み直す（R-PAGE-VIEW）。保存している間は、画像を作るページを読み込み直さない。 */
function reloadPage() {
  if (!shell || live.saving) {
    return;
  }
  shell.liveFrame.src = live.origin + live.page;
}

/**
 * ページのツリーから、そのページへ移る。
 * @param {string} page
 */
function openPage(page) {
  // 保存している間は、画像を作るページを移らない（読み込み直しも含む）。
  if (!shell || live.saving) {
    return;
  }
  if (page !== live.page) {
    live.shiftedOpen = false;
    live.listed = { main: 0, shifted: 0 };
  }
  live.page = page;
  shell.liveFrame.src = live.origin + page;
  render();
  actions.closeDrawer();
}

/**
 * 中継したページからの知らせ。送り手が中継のオリジンの、表示中の枠であるものだけを読む。
 * @param {MessageEvent} event
 */
function receive(event) {
  if (!shell || event.origin !== live.origin || event.source !== shell.liveFrame.contentWindow) {
    return;
  }
  const message = event.data;
  if (!message || message.kemi !== "live") {
    return;
  }
  if (["captured", "described", "placed", "imaged", "saved-found", "glowed"].includes(message.type)) {
    pendingCaptures.get(Number(message.id))?.(message);
    return;
  }
  if (message.type === "saved-hover") {
    // 操作の道具の間は、ページがポインタの動きを見て、乗った印のコメントを知らせる。
    const frame = shell.liveFrame.getBoundingClientRect();
    showSavedTip(typeof message.comment === "string" ? message.comment : null, {
      x: frame.left + (Number(message.x) || 0) * live.scale,
      y: frame.top + (Number(message.y) || 0) * live.scale,
    });
    return;
  }
  if (message.type === "changed") {
    live.pageChanges += 1;
    forgetFailures();
    refreshChanges();
    return;
  }
  if (message.type === "scroll") {
    const before = live.scroll.y;
    live.scroll = {
      x: Number(message.x) || 0,
      y: Number(message.y) || 0,
      height: Number(message.height) || 0,
    };
    // 末尾より下へずらしている間にページが上へスクロールした（人が動かした）ら、ずらすのをやめる。
    if (live.overscroll !== null && live.scroll.y < before - 1) {
      live.overscroll = null;
    }
    layoutFrames();
    return;
  }
  if (message.type !== "page") {
    return;
  }
  const page = pageKey(String(message.path ?? "/"));
  const moved = page !== live.page;
  if (moved) {
    live.refNotice = "";
    live.recorded = null;
    live.undoMockFailed = null;
    live.shiftedOpen = false;
    live.listed = { main: 0, shifted: 0 };
  }
  live.page = page;
  live.pageChanges += 1;
  live.pageLoads += 1;
  live.describing = false;
  forgetFailures();
  live.reachable = message.reachable !== false;
  live.rewrote = Array.isArray(message.rewrote) ? message.rewrote.map(String) : [];
  // 移った後の文書の記述が届くまで、前のページと比べた一覧と印は出さない（前のページの一覧が新しいページの
  // 下に、前の印が新しい比べる相手の上に残らないように）。同じページを読み込み直しただけなら、新しい記述が
  // 届くまで今の一覧と印を残す: 消すと消えた要素の印が一度外れて付き直し、比べる相手の枠が作り直されて
  // スクロールが先頭に戻る。比べる相手が別のスナップショットに変われば、refreshChanges が消す。
  // 読み込み直した文書の記述が返らなければ、残した一覧は前の文書のものなので、describeNow が比べられないことに替える。
  live.now = null;
  if (moved) {
    setChanges(null, null);
  }
  render();
  takeStartSnapshot();
  // 読み込み直した文書には前の描き込みが無いので、描き直させる。
  sentPlaces = "";
  sendPlaces();
  // 「ページで見る」で移ったページなら、描き込みが済んだので場所を光らせる。
  const waiting = glowAfterLoad;
  glowAfterLoad = null;
  if (waiting && waiting.page === live.page && waiting.width === live.width) {
    void glowPlaces(waiting.comment, null, "flash");
  }
}

// ---- モック（live-compare.md の R-PAGE-MOCK）: パネル、選択の横のメニュー、外したときの取り消し ----

/** 検索の欄に打ってから一覧を頼むまで待つ時間（ミリ秒）。打っている間は頼まない。 */
const MOCK_SEARCH_WAIT = 150;

/** 外したモックの取り消しを知らせておく時間（ミリ秒）。 */
const UNDO_MOCK = 8000;

/** 最後に頼んだ一覧の番号。古い返事で新しい一覧を書き換えない。 */
let mockFilesAsked = 0;
/** @type {ReturnType<typeof setTimeout> | null} */
let mockSearchTimer = null;
/** @type {ReturnType<typeof setTimeout> | null} */
let undoMockTimer = null;

/**
 * @param {import("../views/live.js").LiveShell} shell
 */
function startMockControls(shell) {
  const panel = shell.mockPanel;
  panel.search.addEventListener("input", () => {
    if (mockSearchTimer !== null) {
      clearTimeout(mockSearchTimer);
    }
    mockSearchTimer = setTimeout(() => {
      mockSearchTimer = null;
      void loadMockFiles();
    }, MOCK_SEARCH_WAIT);
  });
  const assignTyped = () => {
    if (panel.path.value.trim() !== "") {
      void assignMock(panel.path.value.trim());
    }
  };
  panel.assign.addEventListener("click", assignTyped);
  panel.path.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      assignTyped();
    }
  });
  shell.mockOpen.addEventListener("click", () => {
    shell.menu.hidePopover();
    openMockPanel(shell.band);
  });
  shell.mockMenuButton.addEventListener("click", () => openMockMenu(shell.mockMenuButton));
  shell.mockReload.addEventListener("click", () => {
    shell.mockMenu.hidePopover();
    live.mockReloadAsked = true;
    render();
  });
  shell.mockRemove.addEventListener("click", () => {
    shell.mockMenu.hidePopover();
    void removeMock();
  });
}

/**
 * 浮かぶもの（パネルとメニュー）を、それを開いた操作のすぐ下に開く。画面からははみ出させない。
 * @param {HTMLElement} popover
 * @param {HTMLElement} anchor
 */
function openNear(popover, anchor) {
  const box = anchor.getBoundingClientRect();
  popover.style.top = `${Math.round(box.bottom + 4)}px`;
  popover.style.left = `${Math.round(Math.max(8, Math.min(box.left, innerWidth - 8 - Math.min(420, innerWidth - 16))))}px`;
  popover.showPopover();
}

/**
 * モックのパネルを開く。検索とパスの欄を空にして、一覧を読む。
 * @param {HTMLElement} anchor
 */
function openMockPanel(anchor) {
  if (!shell) {
    return;
  }
  const panel = shell.mockPanel;
  panel.search.value = "";
  panel.path.value = "";
  panel.error.hidden = true;
  if (!panel.box.matches(":popover-open")) {
    openNear(panel.box, anchor);
  }
  panel.search.focus();
  void loadMockFiles();
}

/** パネルの検索に合うファイルの一覧を読んで出す。 */
async function loadMockFiles() {
  if (!shell) {
    return;
  }
  const panel = shell.mockPanel;
  const asked = ++mockFilesAsked;
  try {
    const found = await api.listMockFiles(panel.search.value.trim());
    if (asked !== mockFilesAsked) {
      return;
    }
    renderMockFiles(panel, found, live.mocks.get(live.page)?.path ?? "", (path) => void assignMock(path));
  } catch (error) {
    if (asked === mockFilesAsked) {
      panel.error.textContent = error instanceof Error ? error.message : String(error);
      panel.error.hidden = false;
    }
  }
}

/**
 * モックのメニュー（読み直す・外す）を、押した「…」のすぐ下に開く。
 * @param {HTMLElement} anchor
 */
function openMockMenu(anchor) {
  if (!shell) {
    return;
  }
  if (shell.mockMenu.matches(":popover-open")) {
    shell.mockMenu.hidePopover();
    return;
  }
  openNear(shell.mockMenu, anchor);
}

/**
 * そのパスのモックを、表示中のページに割り当てる。断られたらパネルに理由を出す（R-PAGE-MOCK）。断られた理由を返す
 * （割り当てられたら空）。
 * @param {string} path
 * @param {string} [page] 割り当てるページ（取り消しでは外したときのページ）
 * @returns {Promise<string>}
 */
async function assignMock(path, page = live.page) {
  if (!shell) {
    return "";
  }
  const panel = shell.mockPanel;
  let refused = "";
  try {
    const mock = await api.assignMock(page, path);
    live.mocks.set(page, { path: mock.path, url: mock.url });
    // 割り当てたページは既定でモックと比べる。
    live.chosen.delete(page);
    // 選び直したので、前に外したモックへ戻す取り消しは消す（押すと選んだモックが替わってしまう。R-PAGE-REF）。
    forgetRemovedMock();
    live.undoMockFailed = null;
    panel.error.hidden = true;
    if (panel.box.matches(":popover-open")) {
      panel.box.hidePopover();
    }
  } catch (error) {
    refused = error instanceof Error ? error.message : String(error);
    panel.error.textContent = refused;
    panel.error.hidden = false;
  }
  render();
  return refused;
}

/** 表示中のページのモックを外す。比べる相手はスナップショットに戻る。取り消す操作つきで知らせる。 */
async function removeMock() {
  const page = live.page;
  const path = live.mocks.get(page)?.path;
  await api.assignMock(page, null);
  live.mocks.delete(page);
  live.undoMockFailed = null;
  if (live.chosen.get(page) === "mock") {
    live.chosen.delete(page);
  }
  if (path !== undefined) {
    live.removedMock = { page, path };
    if (undoMockTimer !== null) {
      clearTimeout(undoMockTimer);
    }
    undoMockTimer = setTimeout(() => {
      undoMockTimer = null;
      live.removedMock = null;
      renderReference();
    }, UNDO_MOCK);
  }
  render();
}

/** 外したモックの取り消しを消す。 */
function forgetRemovedMock() {
  live.removedMock = null;
  if (undoMockTimer !== null) {
    clearTimeout(undoMockTimer);
    undoMockTimer = null;
  }
}

/** 外したモックを、同じパスで割り当て直す。断られたら（外した後にファイルが消えたなど）理由を帯に出す。 */
async function undoRemovedMock() {
  const removed = live.removedMock;
  forgetRemovedMock();
  if (!removed) {
    return;
  }
  const refused = await assignMock(removed.path, removed.page);
  if (refused !== "") {
    live.undoMockFailed = { page: removed.page, text: `Mock not assigned again · ${removed.path}: ${refused}` };
    renderReference();
  }
}

/** 開始時のスナップショット。開始時につながらなければ、最初につながったとき（R-PAGE-SNAPSHOT）。 */
function takeStartSnapshot() {
  const snapshots = live.snapshotsListed ? live.snapshots : null;
  if (!startSnapshotDue({ taken: live.startTaken, reachable: live.reachable, snapshots })) {
    return;
  }
  live.startTaken = true;
  void capture("start");
}

/**
 * エージェントに渡す前に、渡す画面で表示中のページのスナップショットを取る
 * （R-PAGE-SNAPSHOT）。渡した後に取ると、渡されたエージェントがもうページを変えていることが
 * ある。取れなくても渡すのは止めない（取れなかったことは画面に出る）。
 */
export async function captureBeforeHand() {
  await capture("handed");
}

// ---- ページへのコメント（live.md の R-PAGE-COMMENT） ----
// 道具を選んでいる間は、動いているページの枠の上に重ねた層で押す・描く操作を受け、枠の中の座標をページに送る。
// どの要素を指すかと文書の座標はページの中（page.js）が決める（別のオリジンで、レビュー画面からは読めない）。
// 重ねた層は動いているページの枠の上にだけあるので、比べる相手の側では場所を置けない。重ねて透かしている間は
// 比べる相手が操作を受けないので、下の層が受け、見る対象の要素が場所になる（live-compare.md の R-PAGE-REF）。

/**
 * @param {import("../views/live.js").LiveShell} shell
 */
function startComposing(shell) {
  shell.toolSeg.addEventListener("click", (event) => {
    const tool = /** @type {HTMLElement} */ (event.target).closest("button")?.dataset.tool;
    if (tool === "element" || tool === "arrow" || tool === "pen" || tool === "interact") {
      cancelStroke();
      live.tool = tool;
      live.composeError = "";
      render();
    }
  });
  const layer = shell.capture;
  layer.addEventListener("pointerdown", (event) => {
    if (live.tool === "interact" || event.button !== 0) {
      return;
    }
    event.preventDefault();
    layer.setPointerCapture(event.pointerId);
    drawing = { pointer: event.pointerId, points: [], drawn: [] };
    extendStroke(event);
  });
  layer.addEventListener("pointermove", (event) => {
    if (drawing?.pointer === event.pointerId && live.tool !== "element") {
      extendStroke(event);
    }
    if (!drawing) {
      void findSavedAt({ x: event.clientX, y: event.clientY });
    }
  });
  layer.addEventListener("pointerleave", () => showSavedTip(null, { x: 0, y: 0 }));
  layer.addEventListener("pointerup", (event) => {
    if (drawing?.pointer === event.pointerId) {
      void finishStroke();
    }
  });
  layer.addEventListener("pointercancel", cancelStroke);
  // 道具を選んでいる間もページをスクロールできるよう、ホイールはページに送る。
  layer.addEventListener(
    "wheel",
    (event) => {
      event.preventDefault();
      const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? layer.clientHeight : 1;
      // 等倍で枠より広いページは、枠のほうを横にスクロールする。枠が動ききった残りだけをページに送る。
      const viewport = shell.liveViewport;
      const before = viewport.scrollLeft;
      viewport.scrollLeft += event.deltaX * unit;
      const x = event.deltaX * unit - (viewport.scrollLeft - before);
      let y = event.deltaY * unit;
      // 末尾より下へずらしている間は、上へのホイールでまずその分を戻す。
      const beyond = overscrolled();
      if (live.overscroll !== null && y < 0 && beyond > 0) {
        const back = Math.min(-y, beyond);
        live.overscroll = back < beyond ? { ...live.overscroll, y: live.overscroll.y - back } : null;
        y += back;
        layoutFrames();
      }
      shell.liveFrame.contentWindow?.postMessage({ kemi: "live", type: "scroll-by", x, y }, live.origin);
    },
    { passive: false },
  );
  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && drawing) {
      cancelStroke();
    }
  });
  const compose = shell.compose;
  compose.undo.addEventListener("click", () => setDraft(undoPlace(live.draft)));
  compose.cancel.addEventListener("click", () => {
    live.composeError = "";
    setDraft(emptyDraft(live.page, live.width));
  });
  compose.save.addEventListener("click", () => void savePageComment());
  compose.back.addEventListener("click", () => showPage(live.draft.url, live.draft.width));
  compose.body.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      void savePageComment();
    }
  });
  // 書き換わった区間は、入力の前の選択と入力の後のカーソルから求める（宙に浮いた参照を字面で見分けられないため）。
  let selection = { start: 0, end: 0 };
  compose.body.addEventListener("beforeinput", () => {
    selection = { start: compose.body.selectionStart, end: compose.body.selectionEnd };
  });
  compose.body.addEventListener("input", () => {
    const value = compose.body.value;
    const edit = editedSpan(selection, compose.body.selectionEnd, value.length - live.draft.body.length);
    setDraft(editBody(live.draft, value, edit));
  });
}

/**
 * 描いている線に点を足す。枠の中の座標はページの CSS ピクセルに戻す（枠は縮めて描いている）。
 * @param {PointerEvent} event
 */
function extendStroke(event) {
  if (!shell || !drawing) {
    return;
  }
  const frame = shell.liveFrame.getBoundingClientRect();
  const layer = shell.capture.getBoundingClientRect();
  drawing.points.push({ x: (event.clientX - frame.left) / live.scale, y: (event.clientY - frame.top) / live.scale });
  drawing.drawn.push({ x: event.clientX - layer.left, y: event.clientY - layer.top });
  shell.stroke.setAttribute("points", drawing.drawn.map((point) => `${point.x},${point.y}`).join(" "));
}

/** 描いている途中の線を取りやめる。 */
function cancelStroke() {
  drawing = null;
  shell?.stroke.setAttribute("points", "");
}

/** 線を描き終えた（押し終えた）。ページに場所を決めてもらい、書いているコメントに足す。 */
async function finishStroke() {
  const stroke = drawing;
  cancelStroke();
  const frame = shell?.liveFrame.contentWindow;
  // 書きかけの場所と別の URL か表示幅では足さない（書く欄がそのことと戻る操作を出している）。
  if (!stroke || !frame || live.tool === "interact" || live.saving || draftElsewhere(live.draft, live.page, live.width)) {
    return;
  }
  const kind = live.tool;
  const first = stroke.drawn[0];
  const length = stroke.drawn.reduce((most, point) => Math.max(most, Math.hypot(point.x - first.x, point.y - first.y)), 0);
  if (kind !== "element" && length < MIN_STROKE) {
    return;
  }
  // 返事を待つ間に別のページや表示幅へ移っても、場所は押したときのページのもの。
  const { page, width } = live;
  const answer = await ask(frame, "place", PLACE_TIMEOUT, {
    kind,
    points: kind === "element" ? stroke.points.slice(0, 1) : stroke.points,
  });
  // 返事を待つ間に保存を始めていれば、書きかけは変えない。
  if (live.saving) {
    return;
  }
  if (answer.kind !== kind) {
    live.composeError = `The place was not put: ${answer.error ?? "the page did not answer"}`;
    renderCompose();
    return;
  }
  live.composeError = "";
  const place = { kind, points: answer.points ?? [], elements: answer.elements ?? [] };
  setDraft(addPlace(live.draft, place, page, width));
}

/**
 * 書いているコメントの場所を差し替え、書く欄とページの上の描き込みを描き直す。
 * @param {import("../live-model.js").PlaceDraft} draft
 */
function setDraft(draft) {
  live.draft = draft;
  showDraftBody();
  renderCompose();
  sendPlaces();
}

/** 書きかけの本文を書く欄に出す（詰め直しで書き換わったとき）。入力位置は同じ所に置く。 */
function showDraftBody() {
  const body = shell?.compose.body;
  if (!body || body.value === live.draft.body) {
    return;
  }
  const caret = body.selectionStart;
  body.value = live.draft.body;
  body.setSelectionRange(Math.min(caret, body.value.length), Math.min(caret, body.value.length));
}

/**
 * 本文の入力位置に `#n` を入れる（R-PAGE-COMMENT の、場所の一覧の番号を押す）。
 * @param {number} n
 */
function insertReference(n) {
  const body = shell?.compose.body;
  if (!body || live.saving) {
    return;
  }
  const edit = { start: body.selectionStart, end: body.selectionEnd };
  body.setRangeText(`#${n}`, edit.start, edit.end, "end");
  body.focus();
  setDraft(editBody(live.draft, body.value, edit));
}

/** ポインタの下の保存したコメントをページに尋ねている間の、次に尋ねる点（尋ねている間に動いた分はまとめる）。 */
let savedAsk = /** @type {{ x: number, y: number } | null} */ (null);
let savedAsking = false;

/**
 * 場所を置く道具の層の上のポインタの下に、保存したコメントの印があるかをページに尋ね、あればその名前を出す
 * （R-PAGE-COMMENT）。層が枠を覆ってポインタがページに届かないので、レビュー画面から尋ねる。
 * @param {{ x: number, y: number }} point 画面の座標
 */
async function findSavedAt(point) {
  savedAsk = point;
  const frame = shell?.liveFrame.contentWindow;
  if (savedAsking || !shell || !frame || pageComments().length === 0) {
    return;
  }
  savedAsking = true;
  try {
    while (savedAsk) {
      const asked = savedAsk;
      savedAsk = null;
      const box = shell.liveFrame.getBoundingClientRect();
      const answer = await ask(frame, "saved-at", PLACE_TIMEOUT, {
        x: (asked.x - box.left) / live.scale,
        y: (asked.y - box.top) / live.scale,
      });
      showSavedTip(typeof answer.comment === "string" ? answer.comment : null, asked);
    }
  } finally {
    savedAsking = false;
  }
}

/**
 * 保存したコメントの短い名前を、ポインタのそばに出す。コメントが無ければ隠す。
 * @param {string | null} id
 * @param {{ x: number, y: number }} point 画面の座標
 */
function showSavedTip(id, point) {
  if (!shell) {
    return;
  }
  const tip = shell.savedTip;
  const comment = id === null ? undefined : pageComments().find((item) => item.id === id);
  tip.hidden = comment === undefined;
  if (comment === undefined) {
    return;
  }
  tip.textContent = commentShortName(String(comment.body ?? ""));
  tip.dataset.comment = comment.id;
  const box = /** @type {HTMLElement} */ (tip.parentElement).getBoundingClientRect();
  tip.style.left = `${point.x - box.left + 12}px`;
  tip.style.top = `${point.y - box.top + 12}px`;
}

/** 保存したページへのコメント（`page` を持つもの）。 */
function pageComments() {
  return state.allComments.filter((comment) => comment.page);
}

/** 最後にページへ送った描き込み。同じなら送り直さない（会話が描き直されるたびに呼ばれる）。 */
let sentPlaces = "";

/**
 * ページの上に場所を描かせる。保存したコメントの場所は、そのコメントの URL と表示幅で見ているときに控えめな
 * 印で、開いているスレッドのものは目立たせる。書いているコメントの場所も、そのページ・その幅を見ているときだけ。
 */
function sendPlaces() {
  const frame = shell?.liveFrame.contentWindow;
  if (!frame) {
    return;
  }
  const here = (/** @type {string} */ url, /** @type {number} */ width) => url === live.page && width === live.width;
  /** @type {{ places: unknown[], look: string, comment?: string }[]} */
  const sets = pageComments()
    .filter((comment) => here(comment.page.url, comment.page.width))
    .map((comment) => ({
      places: comment.page.places,
      look: state.conversation.thread === comment.id ? "focus" : "saved",
      comment: comment.id,
    }));
  const draft = live.draft;
  if (here(draft.url, draft.width) && draft.places.length > 0) {
    sets.push({ places: draft.places, look: "draft" });
  }
  const message = JSON.stringify(sets);
  if (message === sentPlaces) {
    return;
  }
  sentPlaces = message;
  frame.postMessage({ kemi: "live", type: "places", sets }, live.origin);
}

/** 会話の中身が変わった（コメントが増えた・消えた、スレッドを開いた）。印とページのツリーを合わせる。 */
export function refreshPageComments() {
  sendPlaces();
  renderTree();
}

/**
 * 「ページで見る」で別のページへ移ったときの、読み込みと描き込みが済んだら光らせるコメント。
 * @type {{ comment: string, page: string, width: number } | null}
 */
let glowAfterLoad = null;

/**
 * ページへのコメントを、付けた URL と表示幅のページの見方で見せ、その場所までスクロールして光らせる（R-PAGE-COMMENT）。
 * 別のページへ移るときは、読み込みと描き込みが済んでから光らせる。狭い画面では会話のシートを閉じてページを見せる。
 * @param {any} comment
 */
export function showPageComment(comment) {
  // 保存している間は移れない（showPage が何もしない）ので、狭い画面のシートも閉じない。
  if (!comment.page || live.saving) {
    return;
  }
  if (state.narrow) {
    closeSheet();
  }
  const moved = showPage(comment.page.url, comment.page.width);
  renderConversation();
  if (moved) {
    glowAfterLoad = { comment: comment.id, page: comment.page.url, width: comment.page.width };
    return;
  }
  glowAfterLoad = null;
  // 表示幅を変えたときは、枠の新しい大きさがページに届いてから（並べ直した後の文書の座標で）光らせる。
  requestAnimationFrame(() => requestAnimationFrame(() => void glowPlaces(comment.id, null, "flash")));
}

/**
 * ページの上の場所を光らせる（R-PAGE-COMMENT）。`flash` は場所までスクロールして明滅させ、`on` は `off` まで光らせる。
 * `flash` では、等倍で枠より広いページの枠の横のスクロールも、その場所が見える位置に合わせる。
 * @param {string | null} comment 保存したコメントの id。null なら書きかけの場所
 * @param {number | null} n 場所の番号。null ならそのコメントのすべての場所
 * @param {"on" | "off" | "flash"} mode
 */
async function glowPlaces(comment, n, mode) {
  const frame = shell?.liveFrame.contentWindow;
  if (!shell || !frame) {
    return;
  }
  if (mode !== "flash") {
    frame.postMessage({ kemi: "live", type: "glow-places", comment, n, mode, scroll: false }, live.origin);
    return;
  }
  endOverscroll();
  const answer = await ask(frame, "glow-places", GLOW_TIMEOUT, { comment, n, mode, scroll: true });
  const rect = answer?.rect;
  if (rect && typeof rect.x === "number" && typeof rect.w === "number") {
    revealSideways(shell.liveViewport, rect.x, rect.w);
  }
}

/**
 * そのページをその表示幅で、ページの見方で見せる。狭い画面では動いているページの側を見せる（比べる相手の側を
 * 見ていると、動いているページと、その上のコメントの場所が隠れたままになる）。別のページへ移ったかを返す。
 * @param {string} url
 * @param {number} width
 * @returns {boolean}
 */
function showPage(url, width) {
  // 保存している間は表示幅もページも変えられないので、見方だけを切り替えることもしない。
  if (live.saving) {
    return false;
  }
  if (live.view !== "page") {
    setView("page");
  }
  if (live.side !== "live") {
    setSide("live");
  }
  if (live.width !== width) {
    setWidth(width);
  }
  if (live.page !== url) {
    openPage(url);
    return true;
  }
  return false;
}

function renderCompose() {
  if (!shell) {
    return;
  }
  const compose = shell.compose;
  const places = live.draft.places;
  const away = draftElsewhere(live.draft, live.page, live.width);
  compose.away.hidden = away === null;
  if (away) {
    compose.awayText.textContent = `These places are on ${away.url} at ${away.width}px. Places can be added and the comment saved there.`;
    compose.back.textContent = `Back to ${away.url} at ${away.width}px`;
  }
  // 道具を選んだだけでは開かず、最初の場所を置いたときに開く（R-PAGE-COMMENT）。
  compose.box.hidden = live.view !== "page" || (places.length === 0 && live.draft.body === "");
  // 保存している間は書きかけを変えさせない（保存し終えると書く欄を空けるので、その間の変更は消えてしまう）。
  renderPlaces(
    compose,
    places,
    {
      remove: (n) => setDraft(removePlace(live.draft, n)),
      insert: insertReference,
      glow: (n, mode) => void glowPlaces(null, n, mode),
    },
    live.saving,
  );
  const stray = strayRefs(live.draft);
  renderStrayRefs(compose, stray, (ref) => {
    compose.body.focus();
    compose.body.setSelectionRange(ref.start, ref.end);
  });
  compose.undo.disabled = places.length === 0 || live.saving;
  compose.cancel.disabled = live.saving;
  compose.body.readOnly = live.saving;
  compose.save.disabled =
    places.length === 0 || away !== null || live.saving || live.draft.body.trim() === "" || stray.length > 0 || state.submitted;
  compose.error.hidden = live.composeError === "";
  compose.error.textContent = live.composeError;
}

/**
 * 書いたコメントを保存する。ページに場所の周りの画像を作らせ、コメントと 1 回で送る。画像を作れなければ
 * 画像なしで保存する（R-PAGE-COMMENT の画像は作れないこともある）。保存した後は場所を変えない。
 */
async function savePageComment() {
  if (!shell || live.saving || state.submitted) {
    return;
  }
  const draft = live.draft;
  const body = draft.body.trim();
  // 画像は場所のあるページで作るので、保存は書きかけの URL と表示幅に戻ってから（R-PAGE-COMMENT）。
  // どの場所も指さない `#n` が本文にある間は保存しない（R-PAGE-COMMENT）。
  if (draft.places.length === 0 || body === "" || strayRefs(draft).length > 0 || draftElsewhere(draft, live.page, live.width)) {
    return;
  }
  setSaving(true);
  const frame = shell.liveFrame.contentWindow;
  let image = null;
  if (frame) {
    // 狭い画面で比べる相手の側を見ていると、動いているページは並べられておらず画像を作れない。画像を決める値（ページ・
    // 表示幅・並べた画面の高さ・場所）は頼みに載せ、ページはそれだけで画像を作り、自分の場所や幅が違えば断る。
    const answer = await whileLaidOut(() =>
      ask(frame, "image", IMAGE_TIMEOUT, {
        page: draft.url,
        width: draft.width,
        height: laidOutHeight(),
        places: draft.places,
      }),
    );
    image = typeof answer.png === "string" ? answer.png : null;
  }
  // 保存する場所の形は R-SUBMIT の `page.places`。
  const places = draft.places.map(({ n, kind, points, elements }) => ({ n, kind, points, elements }));
  const request = { op: "add_page", page: { url: draft.url, width: draft.width, places }, body };
  try {
    const comment = await api.postComment({ ...request, image }).catch((error) => {
      // 本文と場所に画像を足すと要求の上限を超えるときは、画像なしで保存する（画像は作れないこともある）。
      // 上限を超えた要求はサーバが読まずに断るので、送り直しても二重にはならない。
      if (image !== null && error?.status === api.TOO_LARGE) {
        return api.postComment({ ...request, image: null });
      }
      throw error;
    });
    const before = state.allComments;
    state.allComments = [...state.allComments, comment];
    live.pageCommentSaved = true;
    const notice = imageUnsavedNotice(comment);
    if (notice !== "") {
      live.refNotice = notice;
      renderReference();
    }
    refreshCommentBadges(before);
    renderHeader();
    renderConversation();
    live.composeError = "";
    live.draft = emptyDraft(live.page, live.width);
    showDraftBody();
    sendPlaces();
  } catch (error) {
    live.composeError = `Not saved: ${error instanceof Error ? error.message : String(error)}`;
  } finally {
    setSaving(false);
  }
}

/**
 * 保存の始まりと終わり。保存している間は書きかけ・表示幅・ページを変える操作を使えないと出す。
 * @param {boolean} saving
 */
function setSaving(saving) {
  live.saving = saving;
  state.liveSaving = saving;
  renderBand();
  renderCompose();
  renderTree();
  renderConversation();
}

/**
 * 表示中のページのスナップショットを取り、預ける。2 MB を超えるものは取らず、そのことを
 * 出す。比べる相手は 1 つ前のまま（R-PAGE-SNAPSHOT）。
 * @param {"start" | "handed" | "manual"} kind
 */
async function capture(kind) {
  const frame = shell?.liveFrame.contentWindow;
  if (!frame || !live.reachable) {
    // 開始時のものは、つながったときに取り直す。
    if (kind !== "start") {
      live.refNotice = "Not recorded: the page is not loaded";
      render();
    }
    return;
  }
  const page = live.page;
  const width = live.width;
  if (kind === "manual") {
    live.recorded = null;
  }
  const answer = await whileLaidOut(() => ask(frame, "capture", CAPTURE_TIMEOUT));
  if (typeof answer.html !== "string") {
    live.refNotice = `Not recorded: ${answer.error ?? "the page could not be copied"}`;
    render();
    return;
  }
  if (new Blob([answer.html]).size > SNAPSHOT_LIMIT) {
    live.refNotice = "Not recorded: the snapshot is larger than 2 MB";
    render();
    return;
  }
  const description = await readUploadedDescription(answer.description);
  try {
    const takenPage = pageKey(String(answer.path ?? page));
    const taken = await api.takeSnapshot({
      page: takenPage,
      width,
      kind,
      html: answer.html,
      description: description === null ? null : answer.description,
    });
    // 取ったことで古いものが消えると、一覧の読み直しが先に届いて、もう入っていることがある。
    if (!live.snapshots.some((snapshot) => snapshot.id === taken.id)) {
      live.snapshots.push(taken);
    }
    live.bodies.set(taken.id, answer.html);
    live.descriptions.set(taken.id, description);
    live.refNotice = "";
    // 手で取ったら、そのページの比べる相手を取ったものに切り替え、取れたことを知らせる。見比べ方は変えない（R-PAGE-REF）。
    if (kind === "manual") {
      live.chosen.set(takenPage, taken.id);
      live.recorded = { page: takenPage, id: taken.id };
      // 取り消しを断った知らせは帯で取れたことの知らせより先に出るので、新しい知らせで置き換える。
      if (live.undoMockFailed?.page === takenPage) {
        live.undoMockFailed = null;
      }
    }
  } catch (error) {
    live.refNotice = `Not recorded: ${error instanceof Error ? error.message : String(error)}`;
  }
  render();
}

function render() {
  live.renderedNarrow = state.narrow;
  const shown = live.view === "page" ? { page: live.page, width: live.width } : null;
  const moved = JSON.stringify(shown) !== JSON.stringify(state.live);
  state.live = shown;
  if (moved) {
    // 開いているスレッドの「付けた幅」の札は、見ているページと表示幅で変わる。
    renderConversation();
  }
  renderBand();
  renderCompose();
  renderReference();
  refreshChanges();
  renderTree();
  layoutFrames();
  // 並べている間に見方や側が変わると、並べたまま隠れるものも変わる。
  syncLaidOutInert();
}

/**
 * スナップショットと一緒に預けた記述（gzip で縮めた base64 の文字列）を読む。読めなければ null（比べない）。
 * @param {unknown} value
 * @returns {Promise<Description | null>}
 */
async function readUploadedDescription(value) {
  if (typeof value !== "string" || value === "") {
    return null;
  }
  try {
    const binary = atob(value);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index++) {
      bytes[index] = binary.charCodeAt(index);
    }
    const text = await new Response(new Blob([bytes]).stream().pipeThrough(new DecompressionStream("gzip"))).text();
    return unpackDescription(JSON.parse(text));
  } catch {
    return null;
  }
}

/** 表示中のページの比べる相手。 */
function currentReference() {
  return chooseReference({
    snapshots: live.snapshots,
    mock: live.mocks.get(live.page)?.path ?? null,
    page: live.page,
    width: live.width,
    chosen: live.chosen.get(live.page),
  });
}

/**
 * 変化の一覧を今に合わせる（R-PAGE-DIFF）。比べる相手がスナップショットのときだけ比べる。
 * 動いているページの記述が古い（ページが変わった、移った、表示幅が違う）ときは頼み直し、
 * 届いてから比べる。どちらの記述も変わっていなければ計算し直さない。
 */
function refreshChanges() {
  const reference = currentReference();
  if (reference.type !== "snapshot" || !live.reachable) {
    setChanges(null, null);
    return;
  }
  const id = reference.snapshot.id;
  // 比べる相手が別のスナップショットに変わったら、前のものと比べた一覧は新しい結果が出るまで出さない。
  if (live.changesFrom !== null && live.changesFrom.snapshot !== id) {
    setChanges(null, null);
  }
  if (live.unloadable.has(id)) {
    setChanges(null, null, "Not compared: the snapshot could not be loaded");
    return;
  }
  if (!live.descriptions.has(id)) {
    void loadSnapshot(id).then(
      () => refreshChanges(),
      () => {
        live.unloadable.add(id);
        refreshChanges();
      },
    );
    return;
  }
  const before = live.descriptions.get(id) ?? null;
  if (before === null) {
    setChanges(null, null, "Not compared: the snapshot has no element description");
    return;
  }
  const now = live.now;
  if (now === null || live.describedAt !== live.pageChanges) {
    // 返らなかった記述は描くたびには頼み直さない（大きなページでは、記述を作るたびに全要素をたどる）。
    if (live.describeFailure !== "") {
      setChanges(null, null, live.describeFailure);
      return;
    }
    // 隠れている間は頼まず、今の一覧を残す。また見えるようにしたときの描き直しか、ページの変化の知らせで頼む。
    if (liveFrameHidden()) {
      return;
    }
    void describeNow();
    return;
  }
  // 移る途中や表示幅を変えた直後の記述。ページが落ち着くと知らせが来るので、それまで待つ
  // （ここで頼み直すと、移り終えるまで頼みごとが続く）。
  if (now.page !== live.page || now.description.width !== live.width) {
    setChanges(null, null);
    return;
  }
  if (live.changesFrom?.snapshot === id && live.changesFrom.now === now.description) {
    return;
  }
  const from = { snapshot: id, now: now.description };
  let changes;
  try {
    changes = diffDescriptions(before, now.description);
  } catch {
    // 形の崩れた記述は比べない。一覧の代わりにそのことを出す。
    setChanges(null, from, "Not compared: the element descriptions could not be read");
    return;
  }
  setChanges(changes, from);
}

/** 記述やスナップショットを得られなかったことを忘れ、次に描くときに頼み直させる。 */
function forgetFailures() {
  live.describeFailure = "";
  live.unloadable.clear();
}

/**
 * @param {Change[] | null} changes
 * @param {{ snapshot: string, now: Description } | null} from
 * @param {string} [notice] 比べられないとき、一覧の代わりに出す知らせ
 */
function setChanges(changes, from, notice = "") {
  const same = sameChanges(changes, live.changes) && notice === live.changesNotice;
  live.changesFrom = from;
  if (same && changes === null) {
    return;
  }
  // 結果が前と同じなら一覧は描き直さない。印は送り直す: ページは記述を作り直すたびに要素を数え直すので、
  // 要素が作り直されていれば同じ番号でも別の要素を指す（同じ要素のままなら、ページは印を作り直さない）。
  if (!same) {
    live.changes = changes;
    live.changesNotice = notice;
    if (shell) {
      renderChanges(shell.pageTree, changeHandlers, changesToList());
    }
  }
  renderMarks();
}

/**
 * 変わったところに印を付ける（R-PAGE-VIEW）。今のページにある変化は動いているページの側に、
 * 差し込んだスクリプトが付ける。消えた要素は比べる相手の側に、スナップショットの中身に足して付ける。
 */
function renderMarks() {
  if (!shell) {
    return;
  }
  const marks = live.changes === null ? { now: [], before: [] } : marksOf(live.changes);
  shell.liveFrame.contentWindow?.postMessage({ kemi: "live", type: "marks", marks: marks.now }, live.origin);
  if (live.shownSnapshot !== "") {
    void showSnapshot(live.shownSnapshot);
  }
}

/**
 * 比べる相手の枠に出すスナップショットの、印を付ける消えた要素の番号。今の変化の一覧がそのスナップショットと
 * 比べたものでなければ無し（前に比べた別のスナップショットの印を付けない）。
 * @param {string} id
 * @returns {number[]}
 */
function removedMarksOf(id) {
  if (live.changes === null || live.changesFrom?.snapshot !== id) {
    return [];
  }
  return marksOf(live.changes)
    .before.map((mark) => mark.index)
    .sort((a, b) => a - b);
}

/**
 * 動いているページに今の記述を頼む。返事を待つ間に頼み直しはしない。ページが読み込まれ直したら
 * 待つのをやめる（読み込み直す前の文書に頼んだものは返らないことがある）。返らない・読めないときは
 * 比べられないことを出し、次のきっかけまで頼み直さない。
 */
async function describeNow() {
  const frame = shell?.liveFrame.contentWindow;
  if (!frame || live.describing) {
    return;
  }
  live.describing = true;
  const asked = live.pageChanges;
  const loads = live.pageLoads;
  const answer = await ask(frame, "describe", DESCRIBE_TIMEOUT);
  if (loads !== live.pageLoads) {
    return;
  }
  live.describing = false;
  // 待つ間に隠れた枠の記述は使わない。また見えるようにしたときに頼み直す。
  if (liveFrameHidden()) {
    return;
  }
  // 動いているページの記述は postMessage で届くので縮めない（縮めて戻す手間のほうが大きい）。
  const description = unpackDescription(answer.description);
  if (description === null) {
    live.now = null;
    live.describeFailure = `Not compared: ${answer.error ?? "the page did not describe its elements"}`;
    refreshChanges();
    return;
  }
  live.now = { page: pageKey(String(answer.path ?? "/")), description };
  live.describedAt = asked;
  refreshChanges();
}

/**
 * 動いているページの枠が隠れているか（狭い画面で比べる相手の側を見ている、コードの見方）。隠れた文書の要素は
 * 並べられていないので、その記述の要素の箱はスナップショットと合わず、変わっていない要素がずれたに見える。
 */
function liveFrameHidden() {
  return shell === null || shell.liveFrame.getClientRects().length === 0;
}

/**
 * 動いているページの枠を並べた画面の高さ（ページの CSS ピクセル）。並べたままにしている間に読む。
 * @returns {number}
 */
function laidOutHeight() {
  if (!shell) {
    return 0;
  }
  const height = Number.parseFloat(shell.liveFrame.style.height);
  return Number.isFinite(height) && height > 0 ? height : shell.liveFrame.clientHeight;
}

/**
 * 並べたままにしている間の状態。重なって呼ばれても、並べるのは最初の呼び出し、戻すのは最後に終わった呼び出しだけ
 * （先に終わった方が戻すと、まだ読んでいる方が並べていない文書を読む）。
 * @type {{ running: number, ready: Promise<void> }}
 */
const laidOut = { running: 0, ready: Promise.resolve() };

/**
 * 並べたまま見えなくしている舞台か動いているページの側を、操作もフォーカスも受けないようにする。並べている間に見方や
 * 側や画面の幅が変わると隠れるものも変わるので、そのたびに合わせる。並べていなければどちらも戻す。
 */
function syncLaidOutInert() {
  if (!shell) {
    return;
  }
  const { stage, livePane } = shell;
  const hidden = (/** @type {HTMLElement} */ element) => getComputedStyle(element).visibility === "hidden";
  const inert = stage.dataset.measuring === undefined ? null : hidden(stage) ? stage : hidden(livePane) ? livePane : null;
  stage.inert = inert === stage;
  livePane.inert = inert === livePane;
}

/**
 * task が終わるまで、動いているページの枠を選んだ幅で並べたままにする。隠れていれば、見えず操作も受けないまま並べて
 * から task を呼ぶ。途中で見方や側を変えて枠が隠れても、並べたままにする。スナップショットの記述や画像を隠れた
 * 文書から作ると、隠れた文書は並べられていないので、要素の箱や文書の高さが並べたページと合わない。
 * @template T
 * @param {() => Promise<T>} task
 * @returns {Promise<T>}
 */
async function whileLaidOut(task) {
  if (!shell) {
    return task();
  }
  const { stage, liveFrame } = shell;
  if (laidOut.running === 0) {
    const wasHidden = liveFrameHidden();
    stage.dataset.measuring = "true";
    syncLaidOutInert();
    laidOut.ready = Promise.resolve();
    if (wasHidden) {
      liveFrame.getBoundingClientRect();
      // 枠の大きさがページの文書に届くのを待つ（タブが裏にあって描かれないときも長くは待たない）。
      laidOut.ready = new Promise((done) => {
        requestAnimationFrame(() => requestAnimationFrame(() => done(undefined)));
        setTimeout(done, 200);
      });
    }
  }
  laidOut.running += 1;
  try {
    await laidOut.ready;
    return await task();
  } finally {
    laidOut.running -= 1;
    if (laidOut.running === 0) {
      delete stage.dataset.measuring;
      syncLaidOutInert();
    }
  }
}

/**
 * 狭い画面では、手で取る操作・表示幅の選択・モック・比べる相手の選択・枠に合わせると等倍を帯の「…」のメニューに移し、
 * 広い画面では元の場所に戻す（R-PAGE-VIEW の狭い画面）。幅をまたいだら閉じる。
 */
function placeControls() {
  if (!shell) {
    return;
  }
  const { menu, compareSlot, zoomSeg, recordButton, widthGroup, mockOpen, refNotice, stageName, reloadButton } = shell;
  const inMenu = menu.contains(compareSlot);
  if (state.narrow && !inMenu) {
    menu.append(compareSlot, mockOpen, zoomSeg, recordButton, widthGroup);
  } else if (!state.narrow && inMenu) {
    menu.hidePopover();
    refNotice.before(widthGroup, recordButton);
    stageName.after(zoomSeg);
    reloadButton.before(compareSlot);
  }
}

function renderBand() {
  if (!shell) {
    return;
  }
  placeControls();
  shell.bandPage.textContent = live.page;
  shell.bandWidth.textContent = String(live.width);
  renderTopbar();
  for (const choice of shell.widthSeg.querySelectorAll("button")) {
    choice.setAttribute("aria-pressed", String(Number(choice.dataset.width) === live.width));
    choice.disabled = live.saving;
  }
  shell.widthInput.disabled = live.saving;
  for (const choice of shell.sideSeg.querySelectorAll("button")) {
    choice.setAttribute("aria-pressed", String(choice.dataset.side === live.side));
  }
  shell.stage.dataset.side = live.side;
  shell.stage.dataset.compare = live.compare;
  shell.stage.dataset.zoom = live.zoom;
  for (const choice of shell.modeSeg.querySelectorAll("button")) {
    choice.setAttribute("aria-pressed", String(choice.dataset.compare === live.compare));
  }
  for (const choice of shell.zoomSeg.querySelectorAll("button")) {
    choice.setAttribute("aria-pressed", String(choice.dataset.zoom === live.zoom));
  }
  shell.reloadButton.disabled = live.saving;
  // 比べる相手の選択は、比べる相手を出している間だけ出す（R-PAGE-REF）。
  shell.compareSlot.hidden = live.compare === "now" && !state.narrow;
  shell.opacity.hidden = live.compare !== "overlay";
  const mock = live.mocks.get(live.page) ?? null;
  renderCompareOptions(
    shell.compareSelect,
    referenceOptions({ snapshots: live.snapshots, page: live.page, width: live.width, mock: mock?.path ?? null }),
    live.chosen.get(live.page) ?? (mock ? "mock" : "latest"),
    AUTO_RULE,
  );
  renderStageName();
  // 割り当てている間だけ、選択の横に読み直す・外すのメニューを開く「…」を出す（R-PAGE-MOCK）。
  shell.mockMenuButton.hidden = mock === null;
  if (mock === null && shell.mockMenu.matches(":popover-open")) {
    shell.mockMenu.hidePopover();
  }
  shell.recordButton.disabled = !live.reachable;
  for (const choice of shell.toolSeg.querySelectorAll("button")) {
    choice.setAttribute("aria-pressed", String(choice.dataset.tool === live.tool));
    choice.disabled = state.submitted && choice.dataset.tool !== "interact";
  }
  shell.capture.hidden = live.tool === "interact" || state.submitted;
  shell.hint.hidden = live.pageCommentSaved;
  shell.capture.dataset.tool = live.tool;
  renderPaneLabels();
  shell.liveNotice.hidden = live.reachable && live.rewrote.length === 0;
  if (!live.reachable) {
    shell.liveNotice.textContent = "Waiting for the page";
    shell.liveNotice.dataset.kind = "waiting";
  } else if (live.rewrote.length > 0) {
    shell.liveNotice.textContent = "Frame headers rewritten";
    shell.liveNotice.dataset.kind = "rewrote";
    shell.liveNotice.title = `kemi changed ${live.rewrote.join(" and ")} so that the page can be shown here`;
  }
}

/**
 * 見る対象と比べる相手の枠の見出し。今の倍率はいつも見る対象の見出しに出し、狭い画面で比べる相手の 1 枚を見ている
 * ときは、その見出しにも出す（R-PAGE-VIEW）。倍率が変わったときにも描き直すよう、比べる相手を出す所とは分けておく。
 */
function renderPaneLabels() {
  if (!shell) {
    return;
  }
  const scale = `×${live.scale.toFixed(2)}`;
  shell.liveLabel.textContent = `${live.page} · ${live.width} · ${scale}`;
  const reference = currentReference();
  const label =
    reference.type === "mock"
      ? `Mock · ${reference.path} · ${live.width}`
      : reference.type === "none"
        ? `${live.page} · ${live.width}`
        : `${snapshotLabel(live.snapshots, reference.snapshot)} · ${reference.snapshot.page} · ${reference.snapshot.width}`;
  shell.refLabel.textContent = state.narrow && live.side === "ref" ? `${label} · ${scale}` : label;
}

/** 舞台の見出し: 見比べ方の名前。重ねて透かす間は、両方の名前と透かし具合（R-PAGE-REF）。狭い画面では見ている 1 枚の名前。 */
function renderStageName() {
  if (!shell) {
    return;
  }
  shell.stageName.textContent = compareHeading({
    compare: live.compare,
    reference: referenceName(live.snapshots, currentReference()),
    opacity: live.opacity,
    shown: state.narrow ? live.side : null,
  });
}

/**
 * 変化の一覧の操作。開いたずれただけと並べた数を、描き直しても保つために覚える。行を押したらその要素を見せる。
 * @type {import("../views/live.js").ChangeHandlers}
 */
const changeHandlers = {
  onShifted: (open) => {
    live.shiftedOpen = open;
  },
  onListed: (group, count) => {
    live.listed[group] = count;
  },
  onShow: (element) => {
    if (element.side === "now") {
      void showChangedElement(element.index);
    } else {
      showRemovedElement(element.index);
    }
  },
  onMockMenu: (anchor) => openMockMenu(anchor),
};

/** 比べる相手の側の光を出しておく長さ（ミリ秒。ページの中の光と同じ）。 */
const REF_GLOW = 2800;
/** @type {ReturnType<typeof setTimeout> | null} */
let refGlowTimer = null;

/** 比べる相手をずらした形を保つ条件: 比べる相手・見比べ方・ページ・表示幅（狭い画面では見ている側も）。 */
function refShiftKey() {
  const reference = currentReference();
  const id = reference.type === "snapshot" ? reference.snapshot.id : reference.type;
  return [id, live.compare, live.page, live.width, state.narrow ? live.side : ""].join("\n");
}

/**
 * 消えた要素の行を押した: 比べる相手の側のその要素の位置まで動かし、光らせる（R-PAGE-DIFF）。見る対象だけのときは並べる
 * 見比べ方に、狭い画面では比べる相手の 1 枚に切り替えてから（R-PAGE-VIEW）。比べる相手の枠はスクリプトを止めていて中を
 * 動かせないので、中身の高さで描いて外側をずらす（R-PAGE-SNAPSHOT）。重ねて透かすときは見比べ方を変えず、見る対象を
 * その位置までスクロールし、重ねた比べる相手もそろって動く。
 * @param {number} index 比べる相手の側の記述の中の要素の番号
 */
function showRemovedElement(index) {
  if (!shell) {
    return;
  }
  const reference = currentReference();
  const id = reference.type === "snapshot" ? reference.snapshot.id : "";
  const element = live.changesFrom?.snapshot === id ? live.descriptions.get(id)?.elements[index] : undefined;
  if (!element) {
    return;
  }
  if (state.narrow) {
    if (live.side !== "ref") {
      setSide("ref");
    }
  } else if (live.compare === "now") {
    live.compare = "side";
    render();
  }
  const [left, top, width, height] = element.box;
  live.refGlow = { snapshot: id, box: element.box };
  if (refGlowTimer !== null) {
    clearTimeout(refGlowTimer);
  }
  refGlowTimer = setTimeout(() => {
    refGlowTimer = null;
    live.refGlow = null;
    layoutFrames();
  }, REF_GLOW);
  const viewHeight = shell.refViewport.clientHeight / live.scale;
  const centered = Math.max(0, top - Math.max(0, viewHeight - height) / 2);
  const overlaid = !state.narrow && live.compare === "overlay";
  if (overlaid) {
    // 今のページが短くてそこまでスクロールできないときは、届かない分だけ両方の枠を外側でずらす（そろえたまま）。
    live.overscroll = centered > Math.max(0, live.scroll.height - viewHeight) ? { key: refShiftKey(), y: centered } : null;
    shell.liveFrame.contentWindow?.postMessage({ kemi: "live", type: "scroll-by", x: 0, y: centered - live.scroll.y }, live.origin);
  } else {
    const contentHeight = Math.max(viewHeight, live.descriptions.get(id)?.height ?? 0);
    live.refShift = { key: refShiftKey(), y: Math.min(centered, contentHeight - viewHeight) };
  }
  layoutFrames();
  // 重ねた比べる相手は、ページの横のスクロールの分だけ左にずらして描いている（layoutFrames）。描いている所で合わせる。
  revealSideways(shell.refViewport, overlaid ? left - live.scroll.x : left, width);
}

/**
 * 等倍で枠より広いページの中の箱（文書の横の位置と幅）が見えるよう、枠の横のスクロールを合わせる（R-PAGE-DIFF、
 * R-PAGE-COMMENT）。見る対象と比べる相手の枠は横のスクロールをそろえているので、両方に入れる（狭い画面で隠れている側の
 * 枠は動かないので、見えている側の枠で測る）。
 * @param {HTMLElement} viewport 箱が見えている側の枠の外側
 * @param {number} left
 * @param {number} width
 */
function revealSideways(viewport, left, width) {
  if (!shell) {
    return;
  }
  const scrollLeft = revealScrollLeft({
    left: left * live.scale,
    width: width * live.scale,
    scrollLeft: viewport.scrollLeft,
    viewportWidth: viewport.clientWidth,
  });
  shell.liveViewport.scrollLeft = scrollLeft;
  shell.refViewport.scrollLeft = scrollLeft;
}

/**
 * 比べる相手をずらした形の間、外側のずれを動かす。値は画面のピクセルで、正なら右・下の方を見せる。
 * @param {number} x
 * @param {number} y
 */
function moveShiftedReference(x, y) {
  if (!shell || live.refShift === null) {
    return;
  }
  shell.refViewport.scrollLeft += x;
  live.refShift = { ...live.refShift, y: live.refShift.y + y / live.scale };
  layoutFrames();
}

/** 比べる相手をずらした形の間、キーで動かす量（画面のピクセル。矢印のキー）。 */
const SHIFT_KEY_STEP = 40;

/**
 * 比べる相手をずらした形の間、比べる相手の上で受ける操作（ホイール・指やマウスで引く・キー）で外側のずれを動かす。
 * ずらした形の比べる相手は中身の高さで描いていて、枠の中では動かないため（狭い画面では指で引くしかない）。
 * @param {HTMLElement} layer 比べる相手の上に重ねた層
 */
function startShiftedReferenceMoves(layer) {
  layer.addEventListener(
    "wheel",
    (event) => {
      if (!shell || live.refShift === null) {
        return;
      }
      event.preventDefault();
      const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? shell.refViewport.clientHeight : 1;
      moveShiftedReference(event.deltaX * unit, event.deltaY * unit);
    },
    { passive: false },
  );
  /** @type {{ pointer: number, x: number, y: number } | null} */
  let dragging = null;
  layer.addEventListener("pointerdown", (event) => {
    dragging = { pointer: event.pointerId, x: event.clientX, y: event.clientY };
    try {
      layer.setPointerCapture(event.pointerId);
    } catch {
      // 捕まえられなくても、層の上で引いている間は動かせる。
    }
  });
  layer.addEventListener("pointermove", (event) => {
    if (dragging?.pointer !== event.pointerId) {
      return;
    }
    // 引いた向きと逆に中身が動く（指で紙を引くのと同じ）。
    moveShiftedReference(dragging.x - event.clientX, dragging.y - event.clientY);
    dragging = { pointer: event.pointerId, x: event.clientX, y: event.clientY };
  });
  const stop = () => {
    dragging = null;
  };
  layer.addEventListener("pointerup", stop);
  layer.addEventListener("pointercancel", stop);
  layer.addEventListener("keydown", (event) => {
    if (!shell || live.refShift === null) {
      return;
    }
    const page = shell.refViewport.clientHeight * 0.9;
    const moves = /** @type {Record<string, [number, number]>} */ ({
      ArrowUp: [0, -SHIFT_KEY_STEP],
      ArrowDown: [0, SHIFT_KEY_STEP],
      ArrowLeft: [-SHIFT_KEY_STEP, 0],
      ArrowRight: [SHIFT_KEY_STEP, 0],
      PageUp: [0, -page],
      PageDown: [0, page],
      " ": [0, event.shiftKey ? -page : page],
      Home: [0, -Infinity],
      End: [0, Infinity],
    });
    const move = moves[event.key];
    if (!move) {
      return;
    }
    event.preventDefault();
    // Home と End は端まで（layoutFrames が中身の高さに収める）。
    const [x, y] = move;
    moveShiftedReference(x, Number.isFinite(y) ? y : y > 0 ? Number.MAX_SAFE_INTEGER : -Number.MAX_SAFE_INTEGER);
  });
}

/** ページが要素を見せて光らせるまで待つ上限。 */
const GLOW_TIMEOUT = 5000;

/**
 * 変化の一覧の行を押した: 見る対象のその要素までスクロールし、その要素の印を光らせる（R-PAGE-DIFF）。等倍で枠より
 * 広いページでは、枠の横のスクロールもその要素が見える位置に合わせる。狭い画面で比べる相手の側を見ていたら、
 * 動いているページの側に切り替えてから。
 * @param {number} index 今の側の記述の中の要素の番号
 */
async function showChangedElement(index) {
  if (!shell) {
    return;
  }
  if (live.side !== "live") {
    setSide("live");
  }
  const frame = shell.liveFrame.contentWindow;
  if (!frame) {
    return;
  }
  endOverscroll();
  const answer = await ask(frame, "glow-element", GLOW_TIMEOUT, { index, mode: "flash" });
  const rect = answer?.rect;
  if (!rect || typeof rect.x !== "number" || typeof rect.w !== "number") {
    return;
  }
  revealSideways(shell.liveViewport, rect.x, rect.w);
}

/**
 * ページのツリーに渡す、表示中のページの変化の一覧。比べられなければその知らせ。モックと比べるときは、一覧の代わりに
 * モックの名前の見出し（R-PAGE-VIEW）。
 * @returns {import("../views/live.js").ChangeListState | import("../views/live.js").ChangeNotice | import("../views/live.js").ChangeMock | null}
 */
function changesToList() {
  const shown = currentReference();
  if (shown.type === "mock") {
    return { mock: referenceName(live.snapshots, shown) };
  }
  if (live.changes === null) {
    return live.changesNotice === "" ? null : { notice: live.changesNotice };
  }
  const snapshot = live.changesFrom?.snapshot ?? "";
  const unmarked = live.mapped.get(snapshot) === false && live.changes.some((change) => change.kind === "removed");
  // 見出しには、見比べ方によらず比べている実物の名前を出す（自動のときも、自動が選んだ実物。R-PAGE-VIEW）。
  const compared = live.snapshots.find((item) => item.id === snapshot);
  const reference = compared ? referenceName(live.snapshots, { type: "snapshot", snapshot: compared }) : "";
  return { list: changesByElement(live.changes), reference, shiftedOpen: live.shiftedOpen, listed: live.listed, unmarked };
}

function renderTree() {
  if (!shell) {
    return;
  }
  renderPageTree(
    shell.pageTree,
    buildPageTree({
      current: live.page,
      snapshots: live.snapshots,
      mocks: new Set(live.mocks.keys()),
      comments: pageComments().map((comment) => ({ page: comment.page.url, width: comment.page.width })),
    }),
    {
      onPage: openPage,
      onWidth: (page, width) => {
        setWidth(width);
        openPage(page);
      },
      ...changeHandlers,
    },
    changesToList(),
    live.saving,
  );
}

/** 比べる相手を出す。無ければ、記録されていない旨と取る操作を出す（R-PAGE-VIEW）。 */
function renderReference() {
  if (!shell) {
    return;
  }
  const mock = live.mocks.get(live.page) ?? null;
  const reference = currentReference();
  renderBandNotice(reference);
  if (reference.type !== "mock") {
    shell.refMockFrame.hidden = true;
    live.mockShownKey = "";
  }
  if (reference.type === "mock") {
    shell.refPane.dataset.reference = "mock";
    delete shell.refPane.dataset.snapshot;
    shell.refFrame.hidden = true;
    live.shownSnapshot = "";
    showMock(mock?.url ?? "");
    // 出し始めるときに読めなかったモックは、枠の代わりにそのことを出す（R-PAGE-MOCK）。
    const unreadable = live.mockUnreadable !== "" && live.mockUnreadable === live.mockShownKey;
    shell.refEmpty.hidden = !unreadable;
    shell.refRecordButton.hidden = unreadable;
    if (unreadable) {
      shell.refEmptyText.textContent = `Cannot read the mock ${reference.path}. The file may have been moved or deleted.`;
    }
    return;
  }
  if (reference.type === "none") {
    shell.refPane.dataset.reference = "none";
    delete shell.refPane.dataset.snapshot;
    shell.refFrame.hidden = true;
    shell.refEmpty.hidden = false;
    shell.refEmptyText.textContent = "This page at this width has not been recorded yet.";
    shell.refRecordButton.hidden = false;
    shell.refRecordButton.disabled = !live.reachable;
    live.shownSnapshot = "";
    return;
  }
  const snapshot = reference.snapshot;
  shell.refPane.dataset.reference = "snapshot";
  shell.refPane.dataset.snapshot = snapshot.id;
  shell.refEmpty.hidden = true;
  shell.refFrame.hidden = false;
  if (live.shownSnapshot !== snapshot.id) {
    live.shownSnapshot = snapshot.id;
    void showSnapshot(snapshot.id);
  }
}

/**
 * 帯の知らせ: 取れなかった理由、手で取って比べる相手を切り替えたこと、保存できなかったスナップショットの順に 1 つ。
 * 手で取ったことの知らせには、取ったものを見ていない間（見る対象だけ、狭い画面で動いているページの側）だけ、
 * それを見る操作を添える（live-compare.md の R-PAGE-REF）。
 * @param {import("../live-model.js").Reference} reference
 */
function renderBandNotice(reference) {
  if (!shell) {
    return;
  }
  bandAction = null;
  const removed = live.removedMock !== null && live.removedMock.page === live.page ? live.removedMock : null;
  const undoFailed = live.undoMockFailed !== null && live.undoMockFailed.page === live.page ? live.undoMockFailed.text : "";
  const recorded =
    live.recorded !== null &&
    live.recorded.page === live.page &&
    live.chosen.get(live.page) === live.recorded.id &&
    reference.type === "snapshot" &&
    reference.snapshot.id === live.recorded.id
      ? reference.snapshot
      : null;
  let text = live.refNotice === "" ? undoFailed : live.refNotice;
  let kind = "waiting";
  let label = "";
  let title = "";
  if (text === "" && removed !== null) {
    text = `Mock removed · ${removed.path}`;
    kind = "done";
    label = "Undo";
    title = `Assign ${removed.path} again`;
    bandAction = undoRemovedMock;
  } else if (text === "" && recorded !== null) {
    text = `${snapshotLabel(live.snapshots, recorded)} · now compared with it`;
    kind = "done";
    // 取ったものを見る: 広い画面では並べる見比べ方に、狭い画面では比べる相手の 1 枚に切り替える。
    if (state.narrow ? live.side === "live" : live.compare === "now") {
      label = "Compare →";
      title = state.narrow ? "Show the recorded snapshot" : "Show the recorded snapshot side by side";
      bandAction = showRecorded;
    }
  } else if (text === "" && reference.type === "snapshot") {
    text = unsavedSnapshotNotice(reference.snapshot);
  }
  shell.refNotice.hidden = text === "";
  shell.refNotice.dataset.kind = kind;
  shell.refNoticeText.textContent = text;
  shell.refNoticeAction.hidden = bandAction === null;
  shell.refNoticeAction.textContent = label;
  shell.refNoticeAction.title = title;
}

/** 帯の知らせの操作。知らせを描くたびに決める。 */
/** @type {(() => void) | null} */
let bandAction = null;

/** 手で取ったものを見る: 広い画面では並べる見比べ方に、狭い画面では比べる相手の 1 枚に切り替える。 */
function showRecorded() {
  if (state.narrow) {
    setSide("ref");
    return;
  }
  live.compare = "side";
  render();
}

/**
 * モックを出す。読むのは出し始めるときだけ: モックに切り替えたとき、ページを移ったとき、
 * 表示幅を切り替えたとき、読み直す操作を押したとき。出している間にファイルが変わっても
 * 勝手には描き直さない（R-PAGE-MOCK）。読み直すときは枠ごと作り直す。
 * @param {string} url
 */
function showMock(url) {
  if (!shell) {
    return;
  }
  const key = `${live.page}\n${live.width}\n${live.compare}\n${url}`;
  // 比べる相手がモックでなくなると mockShownKey を空にするので、同じ条件なら出し始めではない。
  // 読めるかを確かめている間や読めなかったときの枠は隠れているが、読み直さない。
  if (key === live.mockShownKey && !live.mockReloadAsked) {
    return;
  }
  live.mockShownKey = key;
  live.mockReloadAsked = false;
  live.mockUnreadable = "";
  const frame = /** @type {HTMLIFrameElement} */ (shell.refMockFrame.cloneNode(false));
  frame.hidden = true;
  shell.refMockFrame.replaceWith(frame);
  shell.refMockFrame = frame;
  // 枠は別のオリジンで中の状態を読めないので、出し始めるときに同じ URL を読めるか先に確かめる。
  void api.mockReadable(url).then((readable) => {
    // 確かめている間に別の条件で出し直した（枠を作り直した）か、モックをやめたなら、この結果は古い。
    if (shell?.refMockFrame !== frame || live.mockShownKey !== key) {
      return;
    }
    if (readable) {
      frame.hidden = false;
      frame.src = url;
    } else {
      live.mockUnreadable = key;
      renderReference();
    }
  });
}

/**
 * スナップショットの中身を、スクリプトを止めた枠に出す。消えた要素の印は中身に足して描く。
 * 枠を作り直すとその中のスクロールは先頭に戻るので、作り直すのは出すスナップショットか、印を付ける
 * 消えた要素の組が変わったときだけにする（動いているページの側の印だけが変わったときは作り直さない）。
 * @param {string} id
 */
async function showSnapshot(id) {
  if (!shell) {
    return;
  }
  const html = await loadSnapshot(id);
  const removed = removedMarksOf(id);
  const key = `${id} ${removed.join(",")}`;
  if (live.shownSnapshot !== id || live.shownFrame === key) {
    return;
  }
  live.shownFrame = key;
  // 同じ中身の srcdoc を入れ直しても読み込み直されないことがあるので、枠ごと作り直す。印が無くても通すのは、
  // ページが添えた要素の対応の <meta> を外すため（残すと head の先頭の子になり、head を前提にしたセレクタが変わる）。
  const frame = /** @type {HTMLIFrameElement} */ (shell.refFrame.cloneNode(false));
  const marked = markRemovedInSnapshot(html, removed);
  frame.srcdoc = marked.html;
  shell.refFrame.replaceWith(frame);
  shell.refFrame = frame;
  layoutFrames();
  if (live.mapped.get(id) !== marked.mapped) {
    // 一覧は対応の有無が分かる前に描いていることがあるので、印を付けられないことを出し直す。
    live.mapped.set(id, marked.mapped);
    renderChanges(shell.pageTree, changeHandlers, changesToList());
  }
}

/** 取り寄せ中のスナップショット（id → 取り寄せ）。同じものを 2 度取りに行かない。 */
/** @type {Map<string, Promise<string>>} */
const loadingSnapshots = new Map();

/**
 * スナップショットの中身と記述。手元に無ければ取り寄せて覚える。中身を返す。
 * @param {string} id
 * @returns {Promise<string>}
 */
function loadSnapshot(id) {
  const html = live.bodies.get(id);
  if (html !== undefined && live.descriptions.has(id)) {
    return Promise.resolve(html);
  }
  let loading = loadingSnapshots.get(id);
  if (!loading) {
    loading = api.getSnapshot(id).then(async (snapshot) => {
      const body = String(snapshot.html ?? "");
      const description = await readUploadedDescription(snapshot.description);
      live.bodies.set(id, body);
      live.descriptions.set(id, description);
      return body;
    });
    loading.finally(() => loadingSnapshots.delete(id)).catch(() => {});
    loadingSnapshots.set(id, loading);
  }
  return loading;
}

/**
 * 重ねて透かす間に、ページのスクロールで届かない分（文書の座標）。ページが見せたい位置までスクロールしきれなかった分で、
 * ページのスクロールが返るたびに変わる。
 * @returns {number}
 */
function overscrolled() {
  return live.overscroll === null ? 0 : Math.max(0, live.overscroll.y - live.scroll.y);
}

/**
 * 末尾より下へずらすのをやめる。ページの上の何かを見せる（ページにスクロールを頼む）前に呼ぶ。ずらしたままだと、ページが
 * 末尾までしかスクロールできないときにスクロールの知らせが来ず、見せたものがずらした分だけ上に描かれる。
 */
function endOverscroll() {
  if (live.overscroll !== null) {
    live.overscroll = null;
    layoutFrames();
  }
}

/** 選んだ表示幅で描き、枠に収まらなければ両方に同じ倍率をかけて縮める（R-PAGE-VIEW）。 */
function layoutFrames() {
  if (!shell || live.view !== "page") {
    return;
  }
  // 狭い画面では片方が隠れているので、見えている方の枠で測る。どちらも同じ大きさ。
  const viewport = shell.liveViewport.clientWidth > 0 ? shell.liveViewport : shell.refViewport;
  // 等倍では縮めず、枠より広い分は枠を横にスクロールして見る。縦はページの中でスクロールする。
  const scale = live.zoom === "full" ? 1 : fitScale(viewport.clientWidth, live.width);
  shell.liveFrame.style.width = `${live.width}px`;
  // 横のスクロールバーの分だけ低くなった枠の高さで描く（幅を決めてから読む）。
  const height = viewport.clientHeight / scale;
  const overlaid = live.compare === "overlay" && !state.narrow;
  if (live.overscroll !== null && (!overlaid || live.overscroll.key !== refShiftKey())) {
    live.overscroll = null;
  }
  // 末尾より下へずらしている分（重ねて透かすときだけ）。見る対象の枠も同じだけ上へずらし、比べる相手とそろえる。
  const beyond = overscrolled();
  shell.liveFrame.style.height = `${height}px`;
  shell.liveFrame.style.transform = beyond > 0 ? `translate(0px, ${-beyond * scale}px) scale(${scale})` : `scale(${scale})`;
  // 道具の層は、横にスクロールして出てくる所も含めてページの上を覆う。
  shell.capture.style.width = `${Math.max(viewport.clientWidth, live.width * scale)}px`;
  shell.capture.style.height = `${viewport.clientHeight}px`;
  // 重ねて透かすときは、比べる相手を中身の高さで描き、見る対象のスクロールの分だけずらす。狭い画面では
  // 重ねず 1 枚ずつ見る（R-PAGE-VIEW）。
  // 消えた要素の行を押した後は、比べる相手を中身の高さで描いて外側でずらす（比べる相手・見比べ方・ページ・表示幅の
  // どれかが変わるまで）。
  if (live.refShift !== null && (overlaid || live.refShift.key !== refShiftKey())) {
    live.refShift = null;
  }
  const reference = currentReference();
  const shownId = reference.type === "snapshot" ? reference.snapshot.id : "";
  /** @type {{ x: number, y: number } | null} 比べる相手の文書の左上が、枠の外側のどこにあるか（画面のピクセル） */
  let origin = null;
  let placement = { height, transform: `scale(${scale})` };
  if (overlaid) {
    // 比べる相手は、今のページより長くても末尾まで描く（消えた要素が末尾の下にあることがある）。
    placement = overlayPlacement({
      scale,
      viewportHeight: viewport.clientHeight,
      scrollX: live.scroll.x,
      scrollY: live.scroll.y + beyond,
      contentHeight: Math.max(live.scroll.height, live.descriptions.get(shownId)?.height ?? 0),
    });
    origin = { x: -live.scroll.x * scale, y: -(live.scroll.y + beyond) * scale };
  } else if (live.refShift !== null) {
    const contentHeight = Math.max(height, live.descriptions.get(shownId)?.height ?? 0);
    const y = Math.max(0, Math.min(live.refShift.y, contentHeight - height));
    live.refShift = { ...live.refShift, y };
    placement = { height: contentHeight, transform: `translate(0px, ${-y * scale}px) scale(${scale})` };
    origin = { x: 0, y: -y * scale };
  }
  for (const target of [shell.refFrame, shell.refMockFrame]) {
    target.style.width = `${live.width}px`;
    target.style.height = `${placement.height}px`;
    target.style.transform = placement.transform;
  }
  shell.refWheel.hidden = live.refShift === null;
  const glow = live.refGlow !== null && live.refGlow.snapshot === shownId && origin !== null ? live.refGlow.box : null;
  shell.refGlow.hidden = glow === null;
  if (glow !== null && origin !== null) {
    const [left, top, width, boxHeight] = glow;
    const gap = 4;
    shell.refGlow.style.left = `${origin.x + left * scale - gap}px`;
    shell.refGlow.style.top = `${origin.y + top * scale - gap}px`;
    shell.refGlow.style.width = `${width * scale + gap * 2}px`;
    shell.refGlow.style.height = `${boxHeight * scale + gap * 2}px`;
  }
  shell.stage.style.setProperty("--lv-scale", String(scale));
  if (live.scale !== scale) {
    live.scale = scale;
    renderBand();
  }
}
