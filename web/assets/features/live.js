// @ts-check
// `--live` のページの見方（live.md の R-PAGE-MODE・R-PAGE-VIEW・R-PAGE-SNAPSHOT、
// live-compare.md の R-PAGE-REF）。ページとコードの見方を切り替え、動いているページを選んだ
// 表示幅で描き、比べる相手（スナップショット）を同じ幅で並べ、ページのツリーを出す。
// app.js が `--live` のレビューでだけ動的に読み込む（R-VERIFY: ほかのレビューは読まない）。
// 中継したページとは postMessage だけで話す（R-PAGE-PROXY）。

import { actions } from "../actions.js";
import * as api from "../api.js";
import { dom } from "../dom.js";
import {
  buildPageTree,
  chooseReference,
  fitScale,
  liveOrigin,
  overlayPlacement,
  pageKey,
  parseWidth,
  snapshotLabel,
  snapshotOptions,
} from "../live-model.js";
import { diffDescriptions, marksOf, sameChanges, unpackDescription } from "../live-diff.js";
import { buildShell, markRemovedInSnapshot, renderChanges, renderCompareOptions, renderPageTree } from "../views/live.js";

/**
 * @typedef {{ port: number, start: string, page: string, code: boolean }} LiveInfo
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
  /** @type {SnapshotSummary[]} 取った順 */
  snapshots: [],
  /** ページごとに選んだ時点（スナップショットの id）。無ければ既定の順で選ぶ。 */
  /** @type {Map<string, string>} */
  chosen: new Map(),
  /** 開始時のスナップショットを取りに行ったか。 */
  startTaken: false,
  /** 取れなかったときの知らせ。次に描くまで出す。 */
  refNotice: "",
  /** @type {Map<string, string>} 中身の写し（id → HTML） */
  bodies: new Map(),
  /** 比べる相手の枠に今出しているスナップショット。 */
  shownSnapshot: "",
  /** 比べる相手の枠に今入れている中身（スナップショットの id と、印を付けた消えた要素の番号）。 */
  shownFrame: "",
  /** @type {Map<string, { path: string, url: string }>} ページごとのモックの割り当て */
  mocks: new Map(),
  /** モックを出し始めたときの条件。変わったら読み直す（R-PAGE-MOCK の読むきっかけ）。 */
  mockShownKey: "",
  /** 読み直す操作を押した。 */
  mockReloadAsked: false,
  /** 見比べ方。並べるか、重ねて透かすか（live-compare.md の R-PAGE-REF）。 */
  /** @type {"side" | "overlay"} */
  compare: "side",
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
  /** ページが読み込みを知らせてきた回数。 */
  pageLoads: 0,
  /** 表示中のページの変化の一覧。比べていなければ null（R-PAGE-DIFF）。 */
  /** @type {Change[] | null} */
  changes: null,
  /** 変化の一覧を計算したときの比べる相手と記述。変わらなければ計算し直さない。 */
  /** @type {{ snapshot: string, now: Description } | null} */
  changesFrom: null,
  /** ずれただけを開いているか。同じページの間だけ保ち、ページを移ったら畳む。 */
  shiftedOpen: false,
  /** 一覧に並べた項目の数（主な変化とずれただけ）。同じページの間だけ保ち、ページを移ったら戻す。 */
  listed: { main: 0, shifted: 0 },
};

/** @type {import("../views/live.js").LiveShell | null} */
let shell = null;

/** 写しか記述を頼んで返事を待っているもの。 */
/** @type {Map<number, (message: any) => void>} */
const pendingCaptures = new Map();
let nextCapture = 1;

/**
 * 中継したページに頼みごとをして、返事を待つ。時間内に返らなければ error を持つ返事にする。
 * @param {Window} frame
 * @param {"capture" | "describe"} type
 * @param {number} timeout
 * @returns {Promise<any>}
 */
async function ask(frame, type, timeout) {
  const id = nextCapture++;
  const answer = await new Promise((resolve) => {
    const timer = setTimeout(() => resolve({ error: "the page did not answer" }), timeout);
    pendingCaptures.set(id, (message) => {
      clearTimeout(timer);
      resolve(message);
    });
    frame.postMessage({ kemi: "live", type, id }, live.origin);
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

  shell.viewSeg.addEventListener("click", (event) => {
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
      live.side = side;
      render();
    }
  });
  shell.compareSelect.addEventListener("change", () => {
    if (!shell) {
      return;
    }
    live.chosen.set(live.page, shell.compareSelect.value);
    render();
  });
  shell.modeSeg.addEventListener("click", (event) => {
    const mode = /** @type {HTMLElement} */ (event.target).closest("button")?.dataset.compare;
    if (mode === "side" || mode === "overlay") {
      live.compare = mode;
      render();
    }
  });
  shell.opacity.addEventListener("input", () => {
    if (!shell) {
      return;
    }
    live.opacity = Number(shell.opacity.value);
    shell.stage.style.setProperty("--lv-opacity", String(live.opacity / 100));
  });
  shell.stage.style.setProperty("--lv-opacity", String(live.opacity / 100));
  shell.mockAssign.addEventListener("click", () => void assignMock());
  shell.mockInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      void assignMock();
    }
  });
  shell.mockRemove.addEventListener("click", () => void removeMock());
  shell.mockReload.addEventListener("click", () => {
    live.mockReloadAsked = true;
    render();
  });
  shell.recordButton.addEventListener("click", () => void capture("manual"));
  shell.refRecordButton.addEventListener("click", () => void capture("manual"));
  window.addEventListener("message", receive);
  new ResizeObserver(() => layoutFrames()).observe(shell.stage);

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
    // 読み込み直したページでは、開始時のスナップショットはもう取ってある。
    live.startTaken = live.startTaken || live.snapshots.some((snapshot) => snapshot.kind === "start");
    render();
    takeStartSnapshot();
  });
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
}

/**
 * @param {number} width
 */
function setWidth(width) {
  live.width = width;
  if (shell) {
    shell.widthInput.value = "";
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

/**
 * ページのツリーから、そのページへ移る。
 * @param {string} page
 */
function openPage(page) {
  if (!shell) {
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
  if (message.type === "captured" || message.type === "described") {
    pendingCaptures.get(Number(message.id))?.(message);
    return;
  }
  if (message.type === "changed") {
    live.pageChanges += 1;
    refreshChanges();
    return;
  }
  if (message.type === "scroll") {
    live.scroll = {
      x: Number(message.x) || 0,
      y: Number(message.y) || 0,
      height: Number(message.height) || 0,
    };
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
    live.shiftedOpen = false;
    live.listed = { main: 0, shifted: 0 };
  }
  live.page = page;
  live.pageChanges += 1;
  live.pageLoads += 1;
  live.describing = false;
  live.reachable = message.reachable !== false;
  live.rewrote = Array.isArray(message.rewrote) ? message.rewrote.map(String) : [];
  // 移った後の文書の記述が届くまで、前のページと比べた一覧と印は出さない（前のページの一覧が新しいページの
  // 下に、前の印が新しい比べる相手の上に残らないように）。同じページを読み込み直しただけなら、新しい記述が
  // 届くまで今の一覧と印を残す: 消すと消えた要素の印が一度外れて付き直し、比べる相手の枠が作り直されて
  // スクロールが先頭に戻る。比べる相手が別のスナップショットに変われば、refreshChanges が消す。
  live.now = null;
  if (moved) {
    setChanges(null, null);
  }
  render();
  takeStartSnapshot();
}

/** 入れたパスのモックを、表示中のページに割り当てる。断られたら理由を出す（R-PAGE-MOCK）。 */
async function assignMock() {
  if (!shell || shell.mockInput.value.trim() === "") {
    return;
  }
  const page = live.page;
  try {
    const mock = await api.assignMock(page, shell.mockInput.value.trim());
    live.mocks.set(page, { path: mock.path, url: mock.url });
    // 割り当てたページは既定でモックと比べる。
    live.chosen.delete(page);
    shell.mockInput.value = "";
    shell.mockError.hidden = true;
  } catch (error) {
    shell.mockError.textContent = error instanceof Error ? error.message : String(error);
    shell.mockError.hidden = false;
  }
  render();
}

/** 表示中のページのモックを外す。比べる相手はスナップショットに戻る。 */
async function removeMock() {
  const page = live.page;
  await api.assignMock(page, null);
  live.mocks.delete(page);
  if (live.chosen.get(page) === "mock") {
    live.chosen.delete(page);
  }
  render();
}

/** 開始時のスナップショット。開始時につながらなければ、最初につながったとき（R-PAGE-SNAPSHOT）。 */
function takeStartSnapshot() {
  if (live.startTaken || !live.reachable) {
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
  const answer = await ask(frame, "capture", CAPTURE_TIMEOUT);
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
    const taken = await api.takeSnapshot({
      page: pageKey(String(answer.path ?? page)),
      width,
      kind,
      html: answer.html,
      description: description === null ? null : answer.description,
    });
    live.snapshots.push(taken);
    live.bodies.set(taken.id, answer.html);
    live.descriptions.set(taken.id, description);
    live.refNotice = "";
  } catch (error) {
    live.refNotice = `Not recorded: ${error instanceof Error ? error.message : String(error)}`;
  }
  render();
}

function render() {
  renderBand();
  renderReference();
  refreshChanges();
  renderTree();
  layoutFrames();
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
  if (!live.descriptions.has(id)) {
    void loadSnapshot(id).then(
      () => refreshChanges(),
      () => live.descriptions.set(id, null),
    );
    return;
  }
  const before = live.descriptions.get(id) ?? null;
  if (before === null) {
    setChanges(null, null);
    return;
  }
  const now = live.now;
  if (now === null || live.describedAt !== live.pageChanges) {
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
  let changes = null;
  try {
    changes = diffDescriptions(before, now.description);
  } catch {
    // 形の崩れた記述は比べない。一覧は出さない。
  }
  setChanges(changes, { snapshot: id, now: now.description });
}

/**
 * @param {Change[] | null} changes
 * @param {{ snapshot: string, now: Description } | null} from
 */
function setChanges(changes, from) {
  const same = sameChanges(changes, live.changes);
  live.changesFrom = from;
  if (same && changes === null) {
    return;
  }
  // 結果が前と同じなら一覧は描き直さない。印は送り直す: ページは記述を作り直すたびに要素を数え直すので、
  // 要素が作り直されていれば同じ番号でも別の要素を指す（同じ要素のままなら、ページは印を作り直さない）。
  if (!same) {
    live.changes = changes;
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
 * 待つのをやめる（読み込み直す前の文書に頼んだものは返らないことがある）。
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
  // 動いているページの記述は postMessage で届くので縮めない（縮めて戻す手間のほうが大きい）。
  const description = unpackDescription(answer.description);
  if (description === null) {
    return;
  }
  live.now = { page: pageKey(String(answer.path ?? "/")), description };
  live.describedAt = asked;
  refreshChanges();
}

function renderBand() {
  if (!shell) {
    return;
  }
  for (const choice of shell.viewSeg.querySelectorAll("button")) {
    choice.setAttribute("aria-pressed", String(choice.dataset.view === live.view));
  }
  for (const choice of shell.widthSeg.querySelectorAll("button")) {
    choice.setAttribute("aria-pressed", String(Number(choice.dataset.width) === live.width));
  }
  for (const choice of shell.sideSeg.querySelectorAll("button")) {
    choice.setAttribute("aria-pressed", String(choice.dataset.side === live.side));
  }
  shell.stage.dataset.side = live.side;
  shell.stage.dataset.compare = live.compare;
  for (const choice of shell.modeSeg.querySelectorAll("button")) {
    choice.setAttribute("aria-pressed", String(choice.dataset.compare === live.compare));
  }
  shell.opacity.hidden = live.compare !== "overlay";
  const mock = live.mocks.get(live.page) ?? null;
  renderCompareOptions(
    shell.compareSelect,
    [
      ...(mock ? [{ value: "mock", label: `Mock: ${mock.path}` }] : []),
      { value: "latest", label: "Latest snapshot (handed, start, recorded)" },
      ...snapshotOptions(live.snapshots, live.page).map((option) => ({ value: option.id, label: option.label })),
    ],
    live.chosen.get(live.page) ?? (mock ? "mock" : "latest"),
  );
  shell.mockRemove.hidden = mock === null;
  shell.mockReload.hidden = mock === null;
  shell.recordButton.disabled = !live.reachable;
  shell.liveLabel.textContent = `${live.page} · ${live.width}${live.scale < 1 ? ` · ×${live.scale.toFixed(2)}` : ""}`;
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

/** 変化の一覧の操作。開いたずれただけと並べた数を、描き直しても保つために覚える。 */
const changeHandlers = {
  onShifted: (/** @type {boolean} */ open) => {
    live.shiftedOpen = open;
  },
  onListed: (/** @type {"main" | "shifted"} */ group, /** @type {number} */ count) => {
    live.listed[group] = count;
  },
};

/** ページのツリーに渡す、表示中のページの変化の一覧。 */
function changesToList() {
  return live.changes === null ? null : { list: live.changes, shiftedOpen: live.shiftedOpen, listed: live.listed };
}

function renderTree() {
  if (!shell) {
    return;
  }
  renderPageTree(
    shell.pageTree,
    buildPageTree({ current: live.page, snapshots: live.snapshots, mocks: new Set(live.mocks.keys()) }),
    {
      onPage: openPage,
      onWidth: (page, width) => {
        setWidth(width);
        openPage(page);
      },
      ...changeHandlers,
    },
    changesToList(),
  );
}

/** 比べる相手を出す。無ければ、記録されていない旨と取る操作を出す（R-PAGE-VIEW）。 */
function renderReference() {
  if (!shell) {
    return;
  }
  const mock = live.mocks.get(live.page) ?? null;
  const reference = currentReference();
  shell.refNotice.hidden = live.refNotice === "";
  shell.refNotice.textContent = live.refNotice;
  shell.refNotice.dataset.kind = "waiting";
  if (reference.type !== "mock") {
    shell.refMockFrame.hidden = true;
    live.mockShownKey = "";
  }
  if (reference.type === "mock") {
    shell.refPane.dataset.reference = "mock";
    delete shell.refPane.dataset.snapshot;
    shell.refLabel.textContent = `Mock · ${reference.path} · ${live.width}`;
    shell.refFrame.hidden = true;
    shell.refEmpty.hidden = true;
    live.shownSnapshot = "";
    showMock(mock?.url ?? "");
    return;
  }
  if (reference.type === "none") {
    shell.refPane.dataset.reference = "none";
    delete shell.refPane.dataset.snapshot;
    shell.refLabel.textContent = `${live.page} · ${live.width}`;
    shell.refFrame.hidden = true;
    shell.refEmpty.hidden = false;
    shell.refEmptyText.textContent = "This page at this width has not been recorded yet.";
    shell.refRecordButton.disabled = !live.reachable;
    live.shownSnapshot = "";
    return;
  }
  const snapshot = reference.snapshot;
  shell.refPane.dataset.reference = "snapshot";
  shell.refPane.dataset.snapshot = snapshot.id;
  shell.refLabel.textContent = `${snapshotLabel(live.snapshots, snapshot)} · ${snapshot.page} · ${snapshot.width}`;
  shell.refEmpty.hidden = true;
  shell.refFrame.hidden = false;
  if (live.shownSnapshot !== snapshot.id) {
    live.shownSnapshot = snapshot.id;
    void showSnapshot(snapshot.id);
  }
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
  if (key === live.mockShownKey && !live.mockReloadAsked && !shell.refMockFrame.hidden) {
    return;
  }
  live.mockShownKey = key;
  live.mockReloadAsked = false;
  const frame = /** @type {HTMLIFrameElement} */ (shell.refMockFrame.cloneNode(false));
  frame.hidden = false;
  frame.src = url;
  shell.refMockFrame.replaceWith(frame);
  shell.refMockFrame = frame;
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
  // 同じ中身の srcdoc を入れ直しても読み込み直されないことがあるので、枠ごと作り直す。
  const frame = /** @type {HTMLIFrameElement} */ (shell.refFrame.cloneNode(false));
  frame.srcdoc = removed.length > 0 ? markRemovedInSnapshot(html, removed) : html;
  shell.refFrame.replaceWith(frame);
  shell.refFrame = frame;
  layoutFrames();
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

/** 選んだ表示幅で描き、枠に収まらなければ両方に同じ倍率をかけて縮める（R-PAGE-VIEW）。 */
function layoutFrames() {
  if (!shell || live.view !== "page") {
    return;
  }
  // 狭い画面では片方が隠れているので、見えている方の枠で測る。どちらも同じ大きさ。
  const viewport = shell.liveViewport.clientWidth > 0 ? shell.liveViewport : shell.refViewport;
  const scale = fitScale(viewport.clientWidth, live.width);
  const height = viewport.clientHeight / scale;
  shell.liveFrame.style.width = `${live.width}px`;
  shell.liveFrame.style.height = `${height}px`;
  shell.liveFrame.style.transform = `scale(${scale})`;
  // 重ねて透かすときは、比べる相手を中身の高さで描き、見る対象のスクロールの分だけずらす。
  const placement =
    live.compare === "overlay"
      ? overlayPlacement({
          scale,
          viewportHeight: viewport.clientHeight,
          scrollX: live.scroll.x,
          scrollY: live.scroll.y,
          contentHeight: live.scroll.height,
        })
      : { height, transform: `scale(${scale})` };
  for (const target of [shell.refFrame, shell.refMockFrame]) {
    target.style.width = `${live.width}px`;
    target.style.height = `${placement.height}px`;
    target.style.transform = placement.transform;
  }
  shell.stage.style.setProperty("--lv-scale", String(scale));
  if (live.scale !== scale) {
    live.scale = scale;
    renderBand();
  }
}
