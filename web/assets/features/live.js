// @ts-check
// `--live` のページの見方（live.md の R-PAGE-MODE・R-PAGE-VIEW）。ページとコードの見方を
// 切り替え、動いているページを選んだ表示幅で描き、比べる相手を並べ、ページのツリーを出す。
// app.js が `--live` のレビューでだけ動的に読み込む（R-VERIFY: ほかのレビューは読まない）。
// 中継したページとは postMessage だけで話す（R-PAGE-PROXY）。

import { actions } from "../actions.js";
import { dom } from "../dom.js";
import { buildPageTree, fitScale, liveOrigin, pageKey, parseWidth } from "../live-model.js";
import { buildShell, renderPageTree } from "../views/live.js";

/**
 * @typedef {{ port: number, start: string, page: string, code: boolean }} LiveInfo
 */

/** 表示幅の既定。 */
const DEFAULT_WIDTH = 1280;

const live = {
  /** @type {LiveInfo | null} */
  info: null,
  origin: "",
  /** @type {"page" | "code"} */
  view: "page",
  width: DEFAULT_WIDTH,
  /** 表示中のページ（パスとクエリ）。 */
  page: "/",
  reachable: true,
  /** @type {string[]} */
  rewrote: [],
  /** 狭い画面で見ている側。 */
  /** @type {"live" | "ref"} */
  side: "live",
  /** 枠に収めるためにかけている倍率。 */
  scale: 1,
};

/** @type {import("../views/live.js").LiveShell | null} */
let shell = null;

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
      renderBand();
      layoutFrames();
    }
  });
  window.addEventListener("message", receive);
  new ResizeObserver(() => layoutFrames()).observe(shell.stage);

  shell.liveFrame.src = live.origin + live.page;
  setView("page");
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
  if (view === "code" && live.info?.code) {
    // 隠れていた間に測れなかった差分の枠を測り直させる。
    window.dispatchEvent(new Event("resize"));
  }
  document.body.dataset.liveCode = String(Boolean(live.info?.code));
  renderBand();
  renderTree();
  layoutFrames();
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
  renderBand();
  layoutFrames();
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
  live.page = page;
  shell.liveFrame.src = live.origin + page;
  renderTree();
  renderBand();
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
  if (!message || message.kemi !== "live" || message.type !== "page") {
    return;
  }
  live.page = pageKey(String(message.path ?? "/"));
  live.reachable = message.reachable !== false;
  live.rewrote = Array.isArray(message.rewrote) ? message.rewrote.map(String) : [];
  renderBand();
  renderTree();
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

function renderTree() {
  if (!shell) {
    return;
  }
  renderPageTree(
    shell.pageTree,
    buildPageTree({ current: live.page, snapshots: [], mocks: new Set() }),
    {
      onPage: openPage,
      onWidth: (page, width) => {
        setWidth(width);
        openPage(page);
      },
    },
  );
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
  for (const target of [shell.liveFrame]) {
    target.style.width = `${live.width}px`;
    target.style.height = `${height}px`;
    target.style.transform = `scale(${scale})`;
  }
  shell.stage.style.setProperty("--lv-scale", String(scale));
  live.scale = scale;
  renderBand();
}
