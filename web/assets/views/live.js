// @ts-check
// `--live` のページの見方の要素（live.md の R-PAGE-VIEW）: 上部の帯、比べる相手と動いている
// ページを並べる舞台、左のページのツリー。状態は持たず、押されたときの処理は呼ぶ側が渡す。
// `--live` のレビューでだけ読み込む。

import { button, el, svgIcon, textEl } from "../dom.js";
import { WIDTH_CHOICES, WIDTH_MAX, WIDTH_MIN, placeSummary } from "../live-model.js";

/**
 * 表示中のページの変化の一覧の中身。`list` は要素ごとの変化、`reference` は比べている相手の名前、`unmarked` は、
 * 消えた要素をスナップショットの側に印で示せないこと。
 * @typedef {{
 *   list: import("../live-diff.js").ElementChanges[],
 *   reference: string,
 *   shiftedOpen: boolean,
 *   listed: { main: number, shifted: number },
 *   unmarked: boolean,
 * }} ChangeListState
 * 比べられないときに一覧の代わりに出す知らせ。
 * @typedef {{ notice: string }} ChangeNotice
 */

/**
 * @typedef {{
 *   band: HTMLElement,
 *   bandPage: HTMLElement,
 *   bandWidth: HTMLElement,
 *   bandAgent: HTMLElement,
 *   menu: HTMLElement,
 *   widthGroup: HTMLElement,
 *   mockGroup: HTMLElement,
 *   stageHead: HTMLElement,
 *   widthSeg: HTMLElement,
 *   widthInput: HTMLInputElement,
 *   widthError: HTMLElement,
 *   compareSlot: HTMLElement,
 *   compareSelect: HTMLSelectElement,
 *   recordButton: HTMLButtonElement,
 *   stageName: HTMLElement,
 *   zoomSeg: HTMLElement,
 *   reloadButton: HTMLButtonElement,
 *   modeSeg: HTMLElement,
 *   opacity: HTMLInputElement,
 *   mockInput: HTMLInputElement,
 *   mockAssign: HTMLButtonElement,
 *   mockRemove: HTMLButtonElement,
 *   mockReload: HTMLButtonElement,
 *   mockError: HTMLElement,
 *   sideSeg: HTMLElement,
 *   stage: HTMLElement,
 *   refPane: HTMLElement,
 *   refBar: HTMLElement,
 *   refLabel: HTMLElement,
 *   refNotice: HTMLElement,
 *   refNoticeText: HTMLElement,
 *   refNoticeAction: HTMLButtonElement,
 *   refViewport: HTMLElement,
 *   refFrame: HTMLIFrameElement,
 *   refMockFrame: HTMLIFrameElement,
 *   refEmpty: HTMLElement,
 *   refEmptyText: HTMLElement,
 *   refRecordButton: HTMLButtonElement,
 *   livePane: HTMLElement,
 *   liveBar: HTMLElement,
 *   liveLabel: HTMLElement,
 *   liveNotice: HTMLElement,
 *   liveViewport: HTMLElement,
 *   liveFrame: HTMLIFrameElement,
 *   noCode: HTMLElement,
 *   pageTree: HTMLElement,
 *   toolSeg: HTMLElement,
 *   hand: HTMLButtonElement,
 *   hint: HTMLElement,
 *   capture: HTMLElement,
 *   savedTip: HTMLElement,
 *   stroke: SVGPolylineElement,
 *   compose: ComposeShell,
 * }} LiveShell
 */

/**
 * 書いているページへのコメントの欄（R-PAGE-COMMENT）。
 * @typedef {{
 *   box: HTMLElement,
 *   list: HTMLElement,
 *   body: HTMLTextAreaElement,
 *   stray: HTMLElement,
 *   undo: HTMLButtonElement,
 *   cancel: HTMLButtonElement,
 *   save: HTMLButtonElement,
 *   error: HTMLElement,
 *   away: HTMLElement,
 *   awayText: HTMLElement,
 *   back: HTMLButtonElement,
 * }} ComposeShell
 */

/** コメントの場所を置く道具と、ページを普通に触る「操作」。名前とアイコン（画面モック docs/design/ui-mock-live-v2.html）。 */
export const TOOLS = /** @type {const} */ ([
  [
    "element",
    "Element",
    '<svg viewBox="0 0 16 16"><rect x="2" y="2" width="12" height="12" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-dasharray="2.5 2"/><path d="M7 7l6 2.5-2.6.9-.9 2.6z" fill="currentColor"/></svg>',
  ],
  [
    "arrow",
    "Arrow",
    '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M3 13L12.5 3.5M7 3.5h5.5V9"/></svg>',
  ],
  [
    "pen",
    "Pen",
    '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"><path d="M3 13l1-3.5 7-7 2.5 2.5-7 7z"/></svg>',
  ],
  [
    "interact",
    "Interact",
    '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"><path d="M4 2l8 6.5-3.6.6 2 4-1.6.8-2-4L4 12.5z"/></svg>',
  ],
]);

const SVG = "http://www.w3.org/2000/svg";

/**
 * 見方の切り替えの帯と、舞台と、ページのツリーの骨組み。
 * @returns {LiveShell}
 */
export function buildShell() {
  const band = el("div", "lv-band");
  band.id = "live-band";

  const widthGroup = el("div", "lv-widths lv-page-only");
  widthGroup.append(textEl("span", "lv-label", "Width"));
  const widthSeg = el("div", "lv-seg");
  widthSeg.setAttribute("role", "group");
  widthSeg.setAttribute("aria-label", "Width");
  for (const width of WIDTH_CHOICES) {
    const choice = button("");
    choice.textContent = String(width);
    choice.dataset.width = String(width);
    widthSeg.append(choice);
  }
  const widthInput = /** @type {HTMLInputElement} */ (el("input", "lv-width-input"));
  widthInput.type = "text";
  widthInput.inputMode = "numeric";
  widthInput.placeholder = `${WIDTH_MIN}–${WIDTH_MAX}`;
  widthInput.setAttribute("aria-label", `Width from ${WIDTH_MIN} to ${WIDTH_MAX}`);
  const widthError = el("span", "lv-width-error");
  widthError.setAttribute("role", "alert");
  widthError.hidden = true;
  widthGroup.append(widthSeg, widthInput, widthError);

  // 手で取る操作とモックの操作は、見比べ方によらず使えるよう帯に置く（R-PAGE-SNAPSHOT、R-PAGE-MOCK）。
  // 広い画面の帯ではアイコンだけ、狭い画面のメニューでは名前も出す。
  const recordButton = button("lv-record lv-record-now");
  recordButton.append(svgIcon(RECORD_ICON), textEl("span", "lv-record-label", "Record now"));
  recordButton.title = "Record now: take a snapshot of the page as it is now";
  recordButton.setAttribute("aria-label", "Record now");
  const mockGroup = el("div", "lv-mock");
  const mockInput = /** @type {HTMLInputElement} */ (el("input", "lv-mock-input"));
  mockInput.type = "text";
  mockInput.placeholder = "Mock path, e.g. docs/mock.html";
  mockInput.setAttribute("aria-label", "Path of the mock HTML file");
  const mockAssign = button("lv-record lv-mock-assign");
  mockAssign.textContent = "Assign mock";
  const mockRemove = button("lv-record lv-mock-remove");
  mockRemove.textContent = "Remove mock";
  const mockReload = button("lv-record lv-mock-reload");
  mockReload.textContent = "Reload mock";
  mockReload.title = "Read the mock file again";
  const mockError = el("span", "lv-width-error lv-mock-error");
  mockError.setAttribute("role", "alert");
  mockError.hidden = true;
  mockGroup.append(mockInput, mockAssign, mockRemove, mockReload, mockError);
  // 取れた・取れなかった・保存しなかったスナップショットの知らせは、比べる相手を出していなくても見える帯に出す。
  // 取れたときは、取ったものを並べて見る操作を添える（live-compare.md の R-PAGE-REF）。
  const refNotice = el("span", "lv-notice lv-band-notice");
  refNotice.setAttribute("role", "status");
  refNotice.hidden = true;
  const refNoticeText = el("span", "lv-notice-text");
  const refNoticeAction = button("lv-notice-action");
  refNoticeAction.hidden = true;
  refNotice.append(refNoticeText, refNoticeAction);

  // 舞台の見出し: 見比べ方の名前、倍率の切り替え、比べる相手の選択、読み込み直す操作、見比べ方の切り替え。
  const stageHead = el("div", "lv-stage-head");
  const stageName = el("span", "lv-stage-name");
  const zoomSeg = el("div", "lv-seg lv-zoom");
  zoomSeg.setAttribute("role", "group");
  zoomSeg.setAttribute("aria-label", "Scale");
  for (const [zoom, label, title] of [
    ["fit", "Fit", "Shrink the page to fit the frame"],
    ["full", "100%", "Show the page at its actual size and scroll in the frame"],
  ]) {
    const choice = button("");
    choice.textContent = label;
    choice.title = title;
    choice.dataset.zoom = zoom;
    zoomSeg.append(choice);
  }
  const compareSlot = el("div", "lv-compare");
  compareSlot.append(textEl("span", "lv-label", "Compare with"));
  const compareSelect = /** @type {HTMLSelectElement} */ (el("select", "lv-compare-select"));
  compareSelect.setAttribute("aria-label", "Compare with");
  const opacity = /** @type {HTMLInputElement} */ (el("input", "lv-opacity"));
  opacity.type = "range";
  opacity.min = "0";
  opacity.max = "100";
  opacity.value = "50";
  opacity.setAttribute("aria-label", "Opacity of the reference");
  opacity.title = "Opacity of the reference";
  compareSlot.append(compareSelect, opacity);
  const reloadButton = button("iconbtn lv-reload");
  reloadButton.title = "Reload page";
  reloadButton.setAttribute("aria-label", "Reload page");
  reloadButton.append(svgIcon(RELOAD_ICON));
  const modeSeg = el("div", "lv-seg lv-mode");
  modeSeg.setAttribute("role", "group");
  modeSeg.setAttribute("aria-label", "How to compare");
  for (const [mode, label, icon] of COMPARE_MODES) {
    const choice = button("");
    choice.title = label;
    choice.setAttribute("aria-label", label);
    choice.dataset.compare = mode;
    choice.append(svgIcon(icon));
    modeSeg.append(choice);
  }
  const stageLine = el("div", "lv-stage-line");
  stageLine.append(stageName, zoomSeg, el("span", "lv-spacer"), compareSlot, reloadButton, modeSeg);
  stageHead.append(stageLine);

  const sideSeg = el("div", "lv-seg lv-side lv-page-only");
  sideSeg.setAttribute("role", "group");
  sideSeg.setAttribute("aria-label", "Shown side");
  for (const [side, label] of [["live", "Now"], ["ref", "Before"]]) {
    const choice = button("");
    choice.textContent = label;
    choice.dataset.side = side;
    sideSeg.append(choice);
  }

  const toolSeg = el("div", "lv-seg lv-tools lv-page-only");
  toolSeg.setAttribute("role", "group");
  toolSeg.setAttribute("aria-label", "Comment tools");
  for (const [tool, label, icon] of TOOLS) {
    const choice = button("lv-tool");
    choice.append(svgIcon(icon), label);
    choice.dataset.tool = tool;
    choice.title =
      tool === "interact" ? "Use the page as it is" : `Put a place of a comment with the ${label.toLowerCase()} tool`;
    toolSeg.append(choice);
  }

  // 狭い画面では帯を 1 行にまとめ、ページの名前・表示幅・エージェントの状態と「…」のメニューを出す。手で取る操作・
  // 表示幅の選択・モック・比べる相手の選択・枠に合わせると等倍は、そのメニューに移す（features/live.js）。
  const bandPage = el("span", "lv-band-page");
  const bandWidth = el("span", "lv-band-width");
  const bandAgent = el("span", "agent-status lv-band-agent");
  bandAgent.setAttribute("role", "status");
  const menu = el("div", "lv-menu");
  menu.id = "live-menu";
  menu.setAttribute("popover", "");
  menu.setAttribute("aria-label", "Page view options");
  const menuButton = button("iconbtn lv-menu-button");
  menuButton.title = "Page view options";
  menuButton.setAttribute("aria-label", "Page view options");
  menuButton.setAttribute("popovertarget", menu.id);
  menuButton.append(svgIcon(MORE_ICON));
  band.append(
    widthGroup,
    recordButton,
    mockGroup,
    refNotice,
    el("span", "lv-spacer"),
    sideSeg,
    bandPage,
    bandWidth,
    bandAgent,
    menuButton,
    menu,
  );

  // 道具は見る対象の枠のすぐ上に、始め方の案内はその下に置く（R-PAGE-COMMENT）。
  const toolbar = el("div", "lv-toolbar");
  // 狭い画面では、道具のツールバーごと画面の下に浮かべ、「Hand to agent」も入れる（R-PAGE-VIEW）。広い画面では隠す。
  const hand = button("btn primary lv-hand");
  hand.title = "Hand everything written since the last hand-over to the agent";
  toolbar.append(toolSeg, hand);
  const hint = buildHint();
  stageHead.prepend(toolbar, hint);

  const stage = el("div", "lv-stage");
  stage.id = "live-stage";
  const ref = pane("ref", "Before");
  // スクリプトを止める（allow-scripts を付けない）。allow-same-origin も付けないので、
  // 中身はレビュー画面と別の不透明なオリジンになる（R-PAGE-SNAPSHOT）。
  const refFrame = /** @type {HTMLIFrameElement} */ (el("iframe", "lv-frame"));
  refFrame.title = "Snapshot";
  refFrame.setAttribute("sandbox", "");
  refFrame.referrerPolicy = "no-referrer";
  refFrame.hidden = true;
  const refEmpty = el("div", "lv-empty");
  const refEmptyText = textEl("p", "", "");
  const refRecordButton = button("btn lv-record");
  refRecordButton.textContent = "Record now";
  refEmpty.append(refEmptyText, refRecordButton);
  // モックのスクリプトは動かすが、allow-same-origin を付けないので不透明なオリジンで動き、
  // レビュー画面にも kemi の API にも届かない（R-PAGE-MOCK）。referrer も送らない。送ると
  // document.referrer からトークンの URL が読める。
  const refMockFrame = /** @type {HTMLIFrameElement} */ (el("iframe", "lv-frame"));
  refMockFrame.title = "Mock";
  refMockFrame.setAttribute("sandbox", "allow-scripts");
  refMockFrame.referrerPolicy = "no-referrer";
  refMockFrame.hidden = true;
  ref.box.append(refFrame, refMockFrame, refEmpty);
  const live = pane("live", "Now");
  const liveNotice = el("span", "lv-notice");
  liveNotice.hidden = true;
  live.bar.append(liveNotice);
  const liveFrame = /** @type {HTMLIFrameElement} */ (el("iframe", "lv-frame"));
  liveFrame.title = "Running page";
  // 道具を選んでいる間だけ枠の上に重ね、押す・描く操作をページより先に受ける（ページのスクリプトに
  // 横取りされないように）。描いている途中の線はここに描く。
  const capture = el("div", "lv-capture");
  capture.hidden = true;
  const strokeSvg = document.createElementNS(SVG, "svg");
  strokeSvg.setAttribute("class", "lv-stroke");
  const stroke = /** @type {SVGPolylineElement} */ (document.createElementNS(SVG, "polyline"));
  strokeSvg.append(stroke);
  capture.append(strokeSvg);
  // 保存したコメントの印に触れたとき、どのコメントかを出す（R-PAGE-COMMENT）。
  const savedTip = el("div", "lv-saved-tip");
  savedTip.setAttribute("role", "tooltip");
  savedTip.hidden = true;
  live.box.append(liveFrame, capture, savedTip);
  const compose = buildCompose();
  stage.append(stageHead, ref.pane, live.pane, compose.box);

  const noCode = textEl(
    "div",
    "lv-no-code",
    "This is not a git repository, so there is no code diff to show.",
  );
  noCode.id = "live-no-code";

  const pageTree = el("aside", "tree lv-tree");
  pageTree.id = "page-tree";
  pageTree.setAttribute("aria-label", "Page tree");

  return {
    band,
    bandPage,
    bandWidth,
    bandAgent,
    menu,
    widthGroup,
    mockGroup,
    stageHead,
    widthSeg,
    widthInput,
    widthError,
    compareSlot,
    compareSelect,
    recordButton,
    stageName,
    zoomSeg,
    reloadButton,
    modeSeg,
    opacity,
    mockInput,
    mockAssign,
    mockRemove,
    mockReload,
    mockError,
    sideSeg,
    stage,
    refPane: ref.pane,
    refBar: ref.bar,
    refLabel: ref.label,
    refNotice,
    refNoticeText,
    refNoticeAction,
    refViewport: ref.viewport,
    refFrame,
    refMockFrame,
    refEmpty,
    refEmptyText,
    refRecordButton,
    livePane: live.pane,
    liveBar: live.bar,
    liveLabel: live.label,
    liveNotice,
    liveViewport: live.viewport,
    liveFrame,
    noCode,
    pageTree,
    toolSeg,
    hand,
    hint,
    capture,
    savedTip,
    stroke,
    compose,
  };
}

/** ページの見方のアイコン（画面モック docs/design/ui-mock-live-v2.html に倣う）。 */
const RELOAD_ICON =
  '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M13 8a5 5 0 1 1-1.6-3.7M13 2.5v3.3H9.7"/></svg>';

const RECORD_ICON =
  '<svg viewBox="0 0 16 16"><circle cx="8" cy="8" r="5.5" fill="none" stroke="currentColor" stroke-width="1.6"/><circle cx="8" cy="8" r="2.6" fill="currentColor"/></svg>';

const MORE_ICON =
  '<svg viewBox="0 0 16 16" fill="currentColor"><circle cx="3.5" cy="8" r="1.5"/><circle cx="8" cy="8" r="1.5"/><circle cx="12.5" cy="8" r="1.5"/></svg>';

/** 見比べ方（live-compare.md の R-PAGE-REF）。値、名前、アイコン。 */
const COMPARE_MODES = /** @type {const} */ ([
  ["now", "Now only", '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><rect x="2.5" y="3" width="11" height="10" rx="1.5"/></svg>'],
  [
    "side",
    "Side by side",
    '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="1.5" y="3" width="5.6" height="10" rx="1.2"/><rect x="8.9" y="3" width="5.6" height="10" rx="1.2"/></svg>',
  ],
  [
    "overlay",
    "Overlay",
    '<svg viewBox="0 0 16 16" stroke="currentColor" stroke-width="1.5"><rect x="1.5" y="2" width="9" height="8" rx="1.2" fill="none"/><rect x="5.5" y="6" width="9" height="8" rx="1.2" fill="currentColor" fill-opacity=".3"/></svg>',
  ],
]);

const PAGE_ICON =
  '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><rect x="1.5" y="2" width="13" height="9.5" rx="1.5"/><path d="M5.5 14h5M8 11.5V14"/></svg>';
const CODE_TAB_ICON =
  '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5.5 4L1.8 8l3.7 4M10.5 4l3.7 4-3.7 4M9.2 2.5L6.8 13.5"/></svg>';

/**
 * ページの見方とコードの見方を切り替えるタブと、ページの見方の間に上部バーに出すもの（live.md の R-PAGE-MODE）。
 * @typedef {{
 *   tabs: HTMLElement,
 *   codeCount: HTMLElement,
 *   meta: HTMLElement,
 *   metaPage: HTMLElement,
 *   metaWidth: HTMLElement,
 *   agent: HTMLElement,
 * }} LiveTopbar
 * @returns {LiveTopbar}
 */
export function buildTopbar() {
  const tabs = el("div", "lv-modetabs lv-view");
  tabs.setAttribute("role", "group");
  tabs.setAttribute("aria-label", "View");
  const pageTab = button("lv-modetab");
  pageTab.dataset.view = "page";
  pageTab.title = "The running page";
  pageTab.append(svgIcon(PAGE_ICON), textEl("span", "lv-modetab-long", "Live page"), textEl("span", "lv-modetab-short", "Page"));
  const codeTab = button("lv-modetab");
  codeTab.dataset.view = "code";
  codeTab.title = "The changes in the working tree";
  const codeCount = textEl("span", "lv-modetab-count", "");
  codeTab.append(svgIcon(CODE_TAB_ICON), textEl("span", "lv-modetab-long", "Code changes"), textEl("span", "lv-modetab-short", "Code"), codeCount);
  tabs.append(pageTab, codeTab);
  const meta = el("span", "lv-topmeta");
  const metaPage = el("span", "lv-topmeta-page");
  const metaWidth = el("span", "lv-topmeta-width");
  meta.append(textEl("b", "", "Live review"), " · ", metaPage, " · ", metaWidth);
  const agent = el("span", "agent-status lv-top-agent");
  agent.setAttribute("role", "status");
  return { tabs, codeCount, meta, metaPage, metaWidth, agent };
}

/** 始め方の 3 つの手順（R-PAGE-COMMENT）。ページへのコメントを初めて保存するまで出す。 */
const HINT_STEPS = ["Point at a place on the page", "Write, then Comment", "Hand to agent"];

/** @returns {HTMLElement} */
function buildHint() {
  const hint = el("div", "lv-hint");
  hint.setAttribute("role", "note");
  hint.setAttribute("aria-label", "How to start");
  for (const [index, step] of HINT_STEPS.entries()) {
    if (index > 0) {
      hint.append(textEl("span", "lv-hint-arrow", "→"));
    }
    const item = el("span", "lv-hint-step");
    item.append(textEl("span", "lv-hint-n", String(index + 1)), step);
    hint.append(item);
  }
  hint.append(textEl("span", "lv-hint-end", "Shown until you save your first page comment"));
  return hint;
}

/** @returns {ComposeShell} */
function buildCompose() {
  const box = el("section", "lv-compose");
  box.id = "live-compose";
  box.setAttribute("aria-label", "Comment on the page");
  box.hidden = true;
  const head = textEl("div", "lv-compose-head", "Places of this comment");
  const list = el("ol", "lv-places");
  // 書きかけの場所と別の URL か表示幅を見ているときの知らせと、そこへ戻る操作（R-PAGE-COMMENT）。
  const away = el("div", "lv-compose-away");
  away.hidden = true;
  const awayText = el("span", "lv-compose-away-text");
  const back = button("lv-record lv-compose-back");
  away.append(awayText, back);
  const body = /** @type {HTMLTextAreaElement} */ (el("textarea", "lv-compose-body"));
  body.rows = 3;
  body.placeholder = "Refer to a place as #1, #2… (click a place number to insert it) · Cmd/Ctrl+Enter to save";
  body.setAttribute("aria-label", "Comment");
  // どの場所も指さない `#n` の箇所（R-PAGE-COMMENT）。直すまで保存できない。
  const stray = el("div", "lv-compose-stray");
  stray.setAttribute("role", "alert");
  stray.hidden = true;
  const error = el("span", "lv-width-error lv-compose-error");
  error.setAttribute("role", "alert");
  error.hidden = true;
  const actions = el("div", "lv-compose-actions");
  const undo = button("lv-record lv-compose-undo");
  undo.textContent = "Undo last place";
  const cancel = button("btn secondary lv-compose-cancel");
  cancel.textContent = "Discard";
  const save = button("btn primary lv-compose-save");
  save.textContent = "Comment";
  actions.append(undo, error, el("span", "lv-spacer"), cancel, save);
  box.append(head, away, list, body, stray, actions);
  return { box, list, body, stray, undo, cancel, save, error, away, awayText, back };
}

const PLACE_KINDS = { element: "Element", arrow: "Arrow", pen: "Pen" };

/**
 * 書いているコメントの場所の一覧。番号（押すと本文に `#n` を入れる）・種類・指している要素と、一覧から外す ×。
 * 行に乗せている間はページのその場所を光らせ、種類と要素を押すとその場所までスクロールして光らせる（R-PAGE-COMMENT）。
 * @param {ComposeShell} compose
 * @param {import("../live-model.js").Place[]} places
 * @param {{
 *   remove: (n: number) => void,
 *   insert: (n: number) => void,
 *   glow: (n: number, mode: "on" | "off" | "flash") => void,
 * }} handlers
 * @param {boolean} locked 保存している間は外せない
 */
export function renderPlaces(compose, places, handlers, locked) {
  compose.list.textContent = "";
  for (const place of places) {
    const row = el("li", "lv-place");
    row.dataset.n = String(place.n);
    const first = place.elements[0];
    if (first) {
      row.dataset.selector = first.selector;
    }
    const what = placeSummary(place);
    const remove = button("lv-place-remove");
    remove.textContent = "×";
    remove.title = `Remove place ${place.n}`;
    remove.setAttribute("aria-label", `Remove place ${place.n}`);
    remove.disabled = locked;
    remove.addEventListener("click", () => handlers.remove(place.n));
    const number = button("lv-place-n");
    number.textContent = String(place.n);
    number.title = `Insert #${place.n} into the text`;
    number.setAttribute("aria-label", `Insert #${place.n} into the text`);
    number.disabled = locked;
    number.addEventListener("click", () => handlers.insert(place.n));
    const show = button("lv-place-show");
    show.title = `Show place ${place.n} on the page`;
    show.append(textEl("span", "lv-place-kind", PLACE_KINDS[place.kind]), textEl("span", "lv-place-what", what));
    show.addEventListener("click", () => handlers.glow(place.n, "flash"));
    row.addEventListener("mouseenter", () => handlers.glow(place.n, "on"));
    row.addEventListener("mouseleave", () => handlers.glow(place.n, "off"));
    row.append(number, show, remove);
    compose.list.append(row);
  }
}

/**
 * どの場所も指さない本文の `#n` を示す。押すと本文のその箇所を選ぶ。
 * @param {ComposeShell} compose
 * @param {import("../live-model.js").StrayRef[]} stray
 * @param {(ref: import("../live-model.js").StrayRef) => void} onShow
 */
export function renderStrayRefs(compose, stray, onShow) {
  compose.stray.textContent = "";
  compose.stray.hidden = stray.length === 0;
  for (const ref of stray) {
    const item = button("lv-stray-ref");
    item.dataset.start = String(ref.start);
    item.append(textEl("span", "lv-ref-dead", `#${ref.n}`), ref.removed ? " points to a removed place" : " has no place");
    item.title = "Show it in the text";
    item.addEventListener("click", () => onShow(ref));
    compose.stray.append(item);
  }
  if (stray.length > 0) {
    compose.stray.append(textEl("span", "lv-stray-why", "Change or delete it to save."));
  }
}

/**
 * 舞台の 1 枚。上に何を出しているかの帯、下にページを縮めて収める枠。
 * @param {string} side
 * @param {string} title
 */
function pane(side, title) {
  const element = el("section", "lv-pane");
  element.dataset.side = side;
  const bar = el("div", "lv-bar");
  const label = el("span", "lv-bar-label");
  bar.append(textEl("b", "", title), label);
  const viewport = el("div", "lv-viewport");
  const box = el("div", "lv-box");
  viewport.append(box);
  element.append(bar, viewport);
  return { pane: element, bar, label, viewport, box };
}

/**
 * 比べる相手の選択の中身。先頭は自動で、そのすぐ下に自動の選び方の説明（選べない項目）を置く。
 * @param {HTMLSelectElement} select
 * @param {{ value: string, label: string }[]} options
 * @param {string} chosen
 * @param {string} rule 自動の選び方の説明
 */
export function renderCompareOptions(select, options, chosen, rule) {
  select.textContent = "";
  for (const [index, option] of options.entries()) {
    const element = /** @type {HTMLOptionElement} */ (el("option"));
    element.value = option.value;
    element.textContent = option.label;
    select.append(element);
    if (index === 0) {
      const note = /** @type {HTMLOptionElement} */ (el("option", "lv-compare-rule"));
      note.disabled = true;
      note.value = "";
      note.textContent = rule;
      select.append(note);
    }
  }
  select.value = chosen;
}

/**
 * ページのツリー（R-PAGE-VIEW）。ページを押すとそのページへ、表示幅の札を押すとその幅で
 * そのページへ移る。表示中のページの下には、比べた結果があれば変化の一覧を出す。
 * @param {HTMLElement} container
 * @param {import("../live-model.js").PageTreeItem[]} items
 * @param {{
 *   onPage: (page: string) => void,
 *   onWidth: (page: string, width: number) => void,
 * } & ChangeHandlers} handlers
 * @param {ChangeListState | ChangeNotice | null} changes
 * @param {boolean} locked ページと表示幅を変えられない間（コメントの保存中）。移る操作を使えないと出す
 */
export function renderPageTree(container, items, handlers, changes, locked) {
  container.textContent = "";
  container.append(textEl("div", "lv-tree-head", `Pages ${items.length}`));
  const list = el("ul", "lv-pages");
  for (const item of items) {
    const row = el("li", "lv-page");
    row.dataset.page = item.page;
    if (item.current) {
      row.dataset.current = "true";
    }
    const open = button("lv-page-open");
    open.title = item.page;
    open.disabled = locked;
    open.append(textEl("span", "lv-page-path", item.page));
    open.addEventListener("click", () => handlers.onPage(item.page));
    row.append(open);
    const tags = el("span", "lv-page-tags");
    for (const width of item.widths) {
      const tag = button("lv-width-tag");
      tag.textContent = String(width);
      tag.dataset.width = String(width);
      tag.title = `Show ${item.page} at ${width}px`;
    tag.disabled = locked;
      tag.addEventListener("click", () => handlers.onWidth(item.page, width));
      tags.append(tag);
    }
    if (item.mock) {
      tags.append(textEl("span", "lv-mock-tag", "mock"));
    }
    if (item.comments > 0) {
      const count = textEl("span", "lv-comment-count", String(item.comments));
      count.title = item.comments === 1 ? "1 comment on this page" : `${item.comments} comments on this page`;
      tags.append(count);
    }
    row.append(tags);
    if (item.current && changes !== null) {
      row.append(changeList(changes, handlers));
    }
    list.append(row);
  }
  container.append(list);
}

/**
 * 変化の一覧の操作。開いたずれただけと並べた数を覚えさせ、行を押したらその要素を見せる。
 * @typedef {{
 *   onShifted: (open: boolean) => void,
 *   onListed: (group: "main" | "shifted", count: number) => void,
 *   onShow: (element: import("../live-diff.js").ElementChanges) => void,
 * }} ChangeHandlers
 */

/** 変化の一覧の中の操作。描き直したときに同じ操作へフォーカスを移すために見分ける。 */
const FOCUSABLE_IN_CHANGES = [".lv-shifted > summary", ".lv-change-main .lv-change-show", ".lv-change-shifted .lv-change-show"];

/**
 * 表示中のページの下の変化の一覧だけを描き直す（R-PAGE-DIFF）。ページの行とその操作は作り直さない（動き続ける
 * ページで比べ直すたびに、押そうとしている操作やフォーカスが入れ替わらないように）。一覧の中の操作に
 * フォーカスがあれば、描き直した一覧の同じ操作に移す。表示中のページの行がまだ無ければ何もしない
 * （renderPageTree が描く）。
 * @param {HTMLElement} container renderPageTree で描いたツリー
 * @param {ChangeHandlers} handlers
 * @param {ChangeListState | ChangeNotice | null} changes
 */
export function renderChanges(container, handlers, changes) {
  const row = container.querySelector('.lv-page[data-current="true"]');
  if (!row) {
    return;
  }
  const old = row.querySelector(":scope > .lv-changes");
  const active = container.ownerDocument.activeElement;
  const focused = old && active && old.contains(active) ? FOCUSABLE_IN_CHANGES.find((selector) => active.matches(selector)) : undefined;
  if (changes === null) {
    old?.remove();
    return;
  }
  const box = changeList(changes, handlers);
  if (old) {
    old.replaceWith(box);
  } else {
    row.append(box);
  }
  if (focused) {
    /** @type {HTMLElement | null} */ (box.querySelector(focused))?.focus();
  }
}

/**
 * 一覧に一度に足す項目の数。大きなページで描き直しが重くならないよう、最初はこの数だけ並べ、
 * 残りは続きを出す操作で足す。
 */
const LISTED_CHANGES = 300;

/**
 * 変化の一覧（R-PAGE-DIFF）。要素ごとに 1 行で、主な変化を上に、ずれただけは畳んで下に、印の色の凡例をその下に。
 * 見出しには比べている相手の名前を出す（R-PAGE-VIEW）。比べられないときは、一覧の代わりにそのことを出す。
 * @param {ChangeListState | ChangeNotice} state
 * @param {ChangeHandlers} handlers
 */
function changeList(state, handlers) {
  if ("notice" in state) {
    const box = el("div", "lv-changes lv-changes-failed");
    box.append(textEl("p", "lv-changes-notice", state.notice));
    return box;
  }
  const changes = state.list;
  const main = changes.filter((change) => change.kind !== "shifted");
  const shifted = changes.filter((change) => change.kind === "shifted");
  const box = el("div", "lv-changes");
  box.dataset.main = String(main.length);
  box.dataset.shifted = String(shifted.length);
  const head = el("div", "lv-changes-head");
  const vs = textEl("span", "lv-changes-vs", state.reference);
  vs.title = `Compared with ${state.reference}`;
  head.append(textEl("span", "", `Changes ${main.length}`), vs);
  box.append(head);
  if (state.unmarked) {
    box.append(textEl("p", "lv-changes-unmarked", "Not marked: removed elements cannot be located in the snapshot"));
  }
  if (changes.length === 0) {
    box.append(textEl("p", "lv-changes-none", "No changes from the snapshot"));
    return box;
  }
  if (main.length > 0) {
    box.append(changeItems("lv-change-main", main, state.listed.main, handlers, (count) => handlers.onListed("main", count)));
  }
  if (shifted.length > 0) {
    const details = /** @type {HTMLDetailsElement} */ (el("details", "lv-shifted"));
    const shiftedItems = () =>
      changeItems("lv-change-shifted", shifted, state.listed.shifted, handlers, (count) => handlers.onListed("shifted", count));
    details.open = state.shiftedOpen;
    const summary = textEl("summary", "", `Shifted only ${shifted.length}`);
    summary.append(textEl("span", "lv-shifted-why", "Moved or resized; nothing else changed."));
    details.append(summary);
    if (state.shiftedOpen) {
      details.append(shiftedItems());
    }
    // 畳んでいる間は項目を作らない（ずれただけは数が多くなりやすい）。
    details.addEventListener("toggle", () => {
      handlers.onShifted(details.open);
      if (details.open && !details.querySelector(".lv-change-shifted")) {
        details.append(shiftedItems());
      }
    });
    box.append(details);
  }
  box.append(changeLegend());
  return box;
}

/** 印の色の意味（R-PAGE-DIFF）。点の色は一覧の行の点と、ページの上の印の色と同じ。 */
const CHANGE_LEGEND = /** @type {const} */ ([
  ["main", "changed"],
  ["added", "added"],
  ["removed", "removed (on the snapshot)"],
  ["shifted", "moved only"],
]);

/** @returns {HTMLElement} */
function changeLegend() {
  const legend = el("div", "lv-changes-legend");
  legend.setAttribute("aria-label", "What the marks mean");
  for (const [kind, label] of CHANGE_LEGEND) {
    const item = el("span", "lv-legend-item");
    item.dataset.kind = kind;
    item.append(el("i", "lv-change-dot"), label);
    legend.append(item);
  }
  return legend;
}

/**
 * 変化のあった要素を `listed` 個まで並べ、残りがあれば続きを出す操作を置く。押すと次の LISTED_CHANGES 個を足し、
 * 並べた数を `onListed` に知らせる（描き直しても同じ数まで並べるため）。行を押すとその要素を見せる。
 * @param {string} className
 * @param {import("../live-diff.js").ElementChanges[]} changes
 * @param {number} listed
 * @param {ChangeHandlers} handlers
 * @param {(count: number) => void} onListed
 */
function changeItems(className, changes, listed, handlers, onListed) {
  const list = el("ul", `lv-change-list ${className}`);
  let shown = 0;
  const more = el("li", "lv-change-more");
  const remaining = textEl("span", "", "");
  const showMore = button("lv-change-show");
  showMore.textContent = "Show more";
  more.append(remaining, showMore);
  /** @param {number} count */
  const showUpTo = (count) => {
    const items = changes.slice(shown, count).map((element) => {
      const item = el("li", "lv-change");
      item.dataset.kind = element.kind;
      item.dataset.tag = element.tag;
      const go = button("lv-change-go");
      go.title = element.side === "before" ? "Show it on the snapshot" : "Show it on the page";
      const what = el("span", "lv-change-what");
      what.append(textEl("span", "lv-change-el", element.excerpt === "" ? element.tag : `${element.tag} “${element.excerpt}”`));
      what.append(" — ", ...elementChanges(element));
      go.append(el("i", "lv-change-dot"), what, textEl("span", "lv-change-where", element.label));
      go.addEventListener("click", () => handlers.onShow(element));
      item.append(go);
      return item;
    });
    more.before(...items);
    shown = Math.min(count, changes.length);
    remaining.textContent = `${changes.length - shown} more `;
    more.hidden = shown >= changes.length;
  };
  list.append(more);
  showUpTo(Math.max(listed, LISTED_CHANGES));
  showMore.addEventListener("click", () => {
    showUpTo(shown + LISTED_CHANGES);
    onListed(shown);
  });
  return list;
}

/**
 * 1 つの要素で変わったものを並べた文。色の前後は色の見本で、ほかの見た目と文字は前後の値で出す。
 * @param {import("../live-diff.js").ElementChanges} element
 * @returns {(Node | string)[]}
 */
function elementChanges(element) {
  /** @type {(Node | string)[]} */
  const parts = [];
  element.changes.forEach((change, index) => {
    if (index > 0) {
      parts.push(", ");
    }
    parts.push(...changeWhat(change));
  });
  return parts;
}

/**
 * 変化 1 つの文。
 * @param {import("../live-diff.js").Change} change
 * @returns {(Node | string)[]}
 */
function changeWhat(change) {
  switch (change.kind) {
    case "visual":
      return [`${change.property} `, ...valueChange(change.was, change.is)];
    case "text":
      return ["text ", textEl("s", "", change.was), " → ", textEl("b", "", change.is)];
    case "added":
      return ["added"];
    case "removed":
      return ["removed"];
    case "shifted":
      return ["moved or resized"];
  }
}

/**
 * 見た目の値の前後。どちらも色なら色の見本を並べ（値は見本に乗せると出る）、そうでなければ値を並べる。
 * @param {string} was
 * @param {string} is
 * @returns {(Node | string)[]}
 */
function valueChange(was, is) {
  const isColor = (/** @type {string} */ value) => value !== "" && CSS.supports("color", value);
  if (isColor(was) && isColor(is)) {
    return [swatch(was), "→", swatch(is)];
  }
  return [textEl("s", "", was), " → ", textEl("b", "", is)];
}

/**
 * @param {string} color
 * @returns {HTMLElement}
 */
function swatch(color) {
  const item = el("span", "lv-swatch");
  item.style.background = color;
  item.title = color;
  item.setAttribute("role", "img");
  item.setAttribute("aria-label", color);
  return item;
}

/** 消えた要素の印。スナップショットの枠の中は触れないので、写しの HTML に属性と <style> を足して描く。 */
const REMOVED_MARK_STYLE = "[data-kemi-removed] { outline: 2px solid rgb(214, 69, 69) !important; outline-offset: -2px !important; }";

/** 記述の番号と HTML を読み直した文書での要素の位置の対応を入れる <meta> の name（web/live/page.js の ELEMENT_MAP）。 */
const ELEMENT_MAP = "kemi-elements";

/**
 * 比べる相手の側の印（消えた要素）を付けたスナップショットの HTML。HTML を読み直すと要素の並びが DOM と
 * 変わることがあるので、ページがスナップショットに添えた対応（記述の番号ごとの、読み直した文書での要素の
 * 位置。web/live/page.js の elementMap）で要素を探し、属性を付ける。位置は対応の <meta> を外してから、
 * 要素を文書の順に（<template> は中身を）数える。shadow root の中には文書の <style> が効かないので、
 * 印を付けた shadow root ごとにも <style> を足す。`mapped` は、ページが対応を添えていたか（無ければ印は付けられない）。
 * @param {string} html
 * @param {number[]} indices 印を付ける要素の記述の番号
 * @returns {{ html: string, mapped: boolean }}
 */
export function markRemovedInSnapshot(html, indices) {
  const parsed = new DOMParser().parseFromString(html, "text/html");
  const map = parsed.head.querySelector(`meta[name="${ELEMENT_MAP}"]`);
  const runs = (map?.getAttribute("content") ?? "")
    .split(" ")
    .filter((run) => run !== "")
    .map((run) => run.split(",").map(Number));
  map?.remove();
  /** @type {Set<number>} */
  const wanted = new Set();
  for (const index of indices) {
    const run = runs.find(([, first, count]) => index >= first && index < first + count);
    if (run) {
      wanted.add(run[0] + index - run[1]);
    }
  }
  /** @type {Element[]} */
  const marked = [];
  let position = 0;
  /** @param {Element} element */
  const visit = (element) => {
    if (wanted.has(position)) {
      marked.push(element);
    }
    position += 1;
    const children = element instanceof HTMLTemplateElement ? element.content.children : element.children;
    for (const child of [...children]) {
      visit(child);
    }
  };
  visit(parsed.documentElement);
  /** @type {Set<Node>} */
  const styled = new Set();
  for (const element of marked) {
    element.setAttribute("data-kemi-removed", "");
    const root = element.getRootNode();
    if (!styled.has(root)) {
      styled.add(root);
      const style = parsed.createElement("style");
      style.textContent = REMOVED_MARK_STYLE;
      (root instanceof DocumentFragment ? root : parsed.head).append(style);
    }
  }
  const doctype = parsed.doctype ? `<!doctype ${parsed.doctype.name}>` : "";
  return { html: doctype + parsed.documentElement.outerHTML, mapped: map !== null };
}
