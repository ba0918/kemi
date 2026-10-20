// @ts-check
// `--live` のページの見方（live.md の R-PAGE-MODE・R-PAGE-VIEW・R-PAGE-SNAPSHOT、
// live-compare.md の R-PAGE-REF）の入口。骨組みを作って結線し、ページとコードの見方・狭い画面で見る側・
// 表示幅・ページを切り替え、中継したページからの知らせを振り分け、帯と比べる相手を描く。往復・枠の置き方・
// スナップショット・モック・変化の一覧と印・ページへのコメントは features/live/ の下の部品。
// app.js が `--live` のレビューでだけ動的に読み込む（R-VERIFY: ほかのレビューは読まない）。
// 中継したページとは postMessage だけで話す（R-PAGE-PROXY）。

import { actions } from "../actions.js";
import * as api from "../api.js";
import { dom } from "../dom.js";
import {
  AUTO_RULE,
  MOCK_FILE_CHOICE,
  WIDTH_CHOICES,
  bandNotice,
  compareHeading,
  liveOrigin,
  pageKey,
  parseWidth,
  referenceName,
  referenceOptions,
  snapshotLabel,
} from "../live-model.js";
import { state } from "../state.js";
import { renderConversation } from "../views/conversation.js";
import { renderHeader } from "../views/header.js";
import { buildShell, buildTopbar, renderCompareOptions } from "../views/live.js";
import { handToAgent } from "./agent.js";
import { refresh } from "./files.js";
import {
  forgetFailures,
  refreshChanges,
  refreshChangesAndTree,
  renderMarks,
  setChanges,
  showSnapshot,
} from "./live/changes.js";
import { deliverAnswer } from "./live/channel.js";
import { placesAfterLoad, renderCompose, sendPlaces, showSavedTip, startComposing } from "./live/compose.js";
import { openMockPanel, showMock, startMockControls, undoRemovedMock } from "./live/mock.js";
import { reloadSnapshots, recordCurrent, takeStartSnapshot } from "./live/snapshots.js";
import { layoutFrames, startShiftedReferenceMoves, syncLaidOutInert } from "./live/stage.js";
import { bindLiveActions, currentReference, live, setShell, shell } from "./live/state.js";

export { captureBeforeHand } from "./live/snapshots.js";
export { refreshPageComments, showPageComment } from "./live/compose.js";

/**
 * @typedef {import("./live/state.js").LiveInfo} LiveInfo
 */

/** @type {import("../views/live.js").LiveTopbar | null} */
let topbar = null;

/**
 * ページの見方を始める。
 * @param {LiveInfo} info
 */
export function startLive(info) {
  bindLiveActions({ render, renderBand, renderReference, setView, setSide, setWidth, openPage });
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
  placesAfterLoad();
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
  refreshChangesAndTree();
  layoutFrames();
  // 並べている間に見方や側が変わると、並べたまま隠れるものも変わる。
  syncLaidOutInert();
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
