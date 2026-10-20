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
  bandNotice,
  commentShortName,
  consumeOverscroll,
  buildPageTree,
  compareHeading,
  liveOrigin,
  pageKey,
  placeSets,
  draftElsewhere,
  draftSavable,
  editBody,
  editedSpan,
  emptyDraft,
  imageUnsavedNotice,
  parseWidth,
  referenceName,
  referenceOptions,
  removePlace,
  strayRefs,
  snapshotLabel,
  undoPlace,
} from "../live-model.js";
import { changesByElement, diffDescriptions, marksOf, sameChanges, unpackDescription } from "../live-diff.js";
import {
  buildShell,
  buildTopbar,
  renderCompareOptions,
  renderPlaces,
  renderStrayRefs,
} from "../views/live.js";
import { renderChanges, renderPageTree } from "../views/live-changes.js";
import { handToAgent } from "./agent.js";
import { closeSheet } from "./conversation.js";
import { refresh } from "./files.js";
import { renderConversation } from "../views/conversation.js";
import { renderHeader } from "../views/header.js";
import { refreshCommentBadges } from "../views/tree.js";
import { bindLiveActions, currentReference, live, pageComments, setShell, shell } from "./live/state.js";
import { ask, deliverAnswer } from "./live/channel.js";
import {
  glowAndReveal,
  laidOutHeight,
  layoutFrames,
  liveFrameHidden,
  overscrolled,
  showChangedElement,
  showRemovedElement,
  startShiftedReferenceMoves,
  syncLaidOutInert,
  whileLaidOut,
  wheelUnit,
} from "./live/stage.js";
import { loadSnapshot, markRemovedInSnapshot, recordCurrent, reloadSnapshots, takeStartSnapshot } from "./live/snapshots.js";
import { openMockMenu, openMockPanel, showMock, startMockControls, undoRemovedMock } from "./live/mock.js";

export { captureBeforeHand } from "./live/snapshots.js";

/**
 * @typedef {import("./live/state.js").LiveInfo} LiveInfo
 * @typedef {import("../live-diff.js").Description} Description
 * @typedef {import("../live-diff.js").Change} Change
 */

/** ページが記述を返すまで待つ上限。 */
const DESCRIBE_TIMEOUT = 5000;

/** ページが置いた場所を決めて返すまで待つ上限。ペンは要素をすべて測るので長めに。 */
const PLACE_TIMEOUT = 5000;

/** ページがコメントの画像を返すまで待つ上限。過ぎたら画像なしで保存する。 */
const IMAGE_TIMEOUT = 15000;

/** これより短い矢印とペンの線は、押しただけとみなして場所にしない（画面のピクセル）。 */
const MIN_STROKE = 4;

/** 描いている途中の線。点は枠の中の画面の座標（ページの CSS ピクセル）と、重ねた層の中の座標。 */
/** @type {{ pointer: number, points: { x: number, y: number }[], drawn: { x: number, y: number }[] } | null} */
let drawing = null;

/** @type {import("../views/live.js").LiveTopbar | null} */
let topbar = null;

/**
 * ページの見方を始める。
 * @param {LiveInfo} info
 */
export function startLive(info) {
  bindLiveActions({ render, renderBand, renderReference, setSide });
  live.info = info;
  live.origin = liveOrigin(location.protocol, location.hostname, info.port);
  live.page = pageKey(info.start);
  void api.registerExplanationPage(live.page);
  live.pageCommentSaved = info.page_comment_saved === true;
  const stylesheet = document.createElement("link");
  stylesheet.rel = "stylesheet";
  stylesheet.href = "assets/live.css";
  document.head.append(stylesheet);

  const shell = buildShell();
  setShell(shell);
  if (info.code_unavailable) {
    shell.noCode.textContent = `Code unavailable: ${info.code_unavailable}`;
    const retry = document.createElement("button");
    retry.type = "button";
    retry.textContent = "Retry code";
    retry.addEventListener("click", async () => {
      await refresh();
      const updated = state.review?.live;
      if (updated?.code) {
        live.info = updated;
        setView("code");
      }
    });
    shell.noCode.append(retry);
  }
  const main = dom.viewport.closest("main");
  main?.prepend(shell.band);
  main?.append(shell.stage, shell.noCode);
  dom.tree.before(shell.pageTree);
  mirrorDrawer(shell.pageTree);

  topbar = buildTopbar();
  dom.viewSwitch.classList.add('lv-view','lv-modetabs');
  for (const tab of topbar.tabs) {
    const view = tab.dataset.view;
    dom.viewSwitch.querySelector(`[data-review-view="${view}"]`)?.remove();
    tab.dataset.reviewView = view;
    dom.viewSwitch.append(tab);
  }
  dom.titleBlock.before(topbar.meta, topbar.agent);
  mirrorAgentState(topbar.agent);
  mirrorAgentState(shell.bandAgent);
  mirrorHand(shell.hand);
  const menu = shell.menu;
  // 狭い画面の帯のメニューは、帯のすぐ下に開く。
  menu.addEventListener("beforetoggle", () => {
    menu.style.top = `${Math.round(shell?.band.getBoundingClientRect().bottom ?? 0) + 4}px`;
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
  shell.marksButton.addEventListener("click", () => {
    live.marks = !live.marks;
    renderBand();
    renderMarks();
  });
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
  shell.recordButton.addEventListener("click", () => void recordCurrent());
  startComposing(shell);
  shell.refRecordButton.addEventListener("click", () => void recordCurrent());
  startShiftedReferenceMoves(shell.refWheel);
  const noticeAction = shell.refNoticeAction;
  noticeAction.addEventListener("click", () => runBandAction(noticeAction.dataset.action));
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
  actions.syncReviewView(view);
}

/** @param {'code'|'page'} view @param {string} [page] */
export function setLiveView(view,page) {
  if (page !== undefined) openPage(page);
  setView(view);
}

/** 更新バッジを押した。ページの見方の間なら、コードの見方に切り替えてから読み直す（R-PAGE-MODE）。 */
export async function refreshFromBadge() {
  if (live.view !== "code") {
    setView("code");
  }
  await refresh();
  renderTopbar();
}

/**
 * 上部バーのタブの変更ファイルの数と、ページの見方の間に出すページと表示幅。タブの押した状態は、見方の切り替えの
 * ほかのタブと一緒に views/explanation.js の renderExplanationStatus が付ける。
 */
function renderTopbar() {
  if (!topbar) {
    return;
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
  if (state.liveSaving) {
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
  if (!shell || state.liveSaving) {
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
  if (!shell || state.liveSaving) {
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
  if (deliverAnswer(message)) {
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
    const ended = live.overscroll !== null && live.scroll.y < before - 1;
    if (ended) {
      live.overscroll = null;
    }
    // 枠の置き方がスクロールの位置で変わるのは重ねて透かす間だけ。ほかの見比べ方では置き直さない（スクロールの知らせは
    // 1 フレームに 1 回来る）。重ねて透かすへ切り替えたときは、描き直しがここで覚えた位置で置く。
    if (ended || (live.compare === "overlay" && !state.narrow)) {
      layoutFrames();
    }
    return;
  }
  if (message.type !== "page") {
    return;
  }
  const page = pageKey(String(message.path ?? "/"));
  void api.registerExplanationPage(page);
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
      const unit = wheelUnit(event, layer);
      // 等倍で枠より広いページは、枠のほうを横にスクロールする。枠が動ききった残りだけをページに送る。
      const viewport = shell.liveViewport;
      const before = viewport.scrollLeft;
      viewport.scrollLeft += event.deltaX * unit;
      const x = event.deltaX * unit - (viewport.scrollLeft - before);
      let y = event.deltaY * unit;
      // 末尾より下へずらしている間は、上へのホイールでまずその分を戻す。
      if (live.overscroll !== null) {
        const consumed = consumeOverscroll({ deltaY: y, beyond: overscrolled(), y: live.overscroll.y });
        if (consumed !== null) {
          live.overscroll = consumed.y === null ? null : { ...live.overscroll, y: consumed.y };
          y = consumed.deltaY;
          layoutFrames();
        }
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
  if (!stroke || !frame || live.tool === "interact" || state.liveSaving || draftElsewhere(live.draft, live.page, live.width)) {
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
  if (state.liveSaving) {
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
  if (!body || state.liveSaving) {
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

/** 最後にページへ送った描き込み。同じなら送り直さない（会話が描き直されるたびに呼ばれる）。 */
let sentPlaces = "";

/** ページの上に場所を描かせる（描かせる組は live-model.js の placeSets）。 */
function sendPlaces() {
  const frame = shell?.liveFrame.contentWindow;
  if (!frame) {
    return;
  }
  const sets = placeSets({
    comments: pageComments(),
    draft: live.draft,
    page: live.page,
    width: live.width,
    thread: state.conversation.thread,
  });
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
  if (!comment.page || state.liveSaving) {
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
  await glowAndReveal(frame, "glow-places", { comment, n, mode, scroll: true });
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
  if (state.liveSaving) {
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
    state.liveSaving,
  );
  const stray = strayRefs(live.draft);
  renderStrayRefs(compose, stray, (ref) => {
    compose.body.focus();
    compose.body.setSelectionRange(ref.start, ref.end);
  });
  compose.undo.disabled = places.length === 0 || state.liveSaving;
  compose.cancel.disabled = state.liveSaving;
  compose.body.readOnly = state.liveSaving;
  compose.save.disabled = !draftSavable(live.draft, live.page, live.width) || state.liveSaving || state.submitted;
  compose.error.hidden = live.composeError === "";
  compose.error.textContent = live.composeError;
}

/**
 * 書いたコメントを保存する。ページに場所の周りの画像を作らせ、コメントと 1 回で送る。画像を作れなければ
 * 画像なしで保存する（R-PAGE-COMMENT の画像は作れないこともある）。保存した後は場所を変えない。
 */
async function savePageComment() {
  if (!shell || state.liveSaving || state.submitted) {
    return;
  }
  const draft = live.draft;
  if (!draftSavable(draft, live.page, live.width)) {
    return;
  }
  const body = draft.body.trim();
  setSaving(true);
  const frame = shell.liveFrame.contentWindow;
  let image = null;
  let imageError = "";
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
    if (image === null && typeof answer.error === "string") imageError = answer.error;
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
    const notice = imageUnsavedNotice(comment) || (imageError ? `The comment was saved without its image: ${imageError}` : "");
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
  state.liveSaving = saving;
  renderBand();
  renderCompose();
  renderTree();
  renderConversation();
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
 * 印を隠しているときは、どちらの側の印も外す。
 */
function renderMarks() {
  if (!shell) {
    return;
  }
  const marks = live.changes === null || !live.marks ? { now: [], before: [] } : marksOf(live.changes);
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
  if (live.changes === null || !live.marks || live.changesFrom?.snapshot !== id) {
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
    choice.disabled = state.liveSaving;
  }
  shell.widthInput.disabled = state.liveSaving;
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
  shell.reloadButton.disabled = state.liveSaving;
  shell.marksButton.title = live.marks ? "Hide change marks" : "Show change marks";
  shell.marksButton.setAttribute("aria-pressed", String(live.marks));
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
  shell.recordButton.disabled = !live.reachable || live.recording;
  for (const choice of shell.toolSeg.querySelectorAll("button")) {
    choice.setAttribute("aria-pressed", String(choice.dataset.tool === live.tool));
    choice.disabled = state.submitted && choice.dataset.tool !== "interact";
  }
  shell.capture.hidden = live.tool === "interact" || state.submitted;
  shell.hint.hidden = live.pageCommentSaved;
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
 * @type {import("../views/live-changes.js").ChangeHandlers}
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
  onRecord: () => void recordCurrent(),
};

/**
 * ページのツリーに渡す、表示中のページの変化の一覧。比べられなければその知らせ。モックと比べるときは、一覧の代わりに
 * モックの名前の見出し（R-PAGE-VIEW）。
 * @returns {import("../views/live-changes.js").ChangeListState | import("../views/live-changes.js").ChangeNotice | import("../views/live-changes.js").ChangeMock | null}
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
  return {
    list: changesByElement(live.changes), reference, shiftedOpen: live.shiftedOpen, listed: live.listed, unmarked,
    fromStart: compared?.kind === "start", canRecord: live.reachable && !live.recording, recording: live.recording,
  };
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
    state.liveSaving,
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
    shell.refRecordButton.disabled = !live.reachable || live.recording;
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
 * 帯の知らせ（live-model.js の bandNotice）。添える操作は、押したときに読めるよう操作の要素に書いておく。
 * @param {import("../live-model.js").Reference} reference
 */
function renderBandNotice(reference) {
  if (!shell) {
    return;
  }
  const notice = bandNotice({
    refNotice: live.refNotice,
    undoMockFailed: live.undoMockFailed,
    removedMock: live.removedMock,
    recorded: live.recorded,
    chosen: live.chosen.get(live.page),
    reference,
    snapshots: live.snapshots,
    page: live.page,
    narrow: state.narrow,
    side: live.side,
    compare: live.compare,
  });
  shell.refNotice.hidden = notice.text === "";
  shell.refNotice.dataset.kind = notice.kind;
  shell.refNoticeText.textContent = notice.text;
  shell.refNoticeAction.hidden = notice.action === null;
  shell.refNoticeAction.dataset.action = notice.action ?? "";
  shell.refNoticeAction.textContent = notice.label;
  shell.refNoticeAction.title = notice.title;
}

/**
 * 帯の知らせの操作を押した。押す時点の状態で決め直さず、描いたときに決めた操作をする（見えている操作の名前と、
 * 押してすることを食い違わせない）。
 * @param {string | undefined} action
 */
function runBandAction(action) {
  if (action === "undo-mock") {
    void undoRemovedMock();
  } else if (action === "show-recorded") {
    showRecorded();
  }
}

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
