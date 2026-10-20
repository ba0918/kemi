// @ts-check
// `--live` のモック（live-compare.md の R-PAGE-MOCK）: パネル、選択の横のメニュー、外したときの取り消しと、
// 比べる相手の側にモックを出すこと。

import * as api from "../../api.js";
import { renderMockFiles } from "../../views/live.js";
import { live, liveActions, shell } from "./state.js";

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
 * @param {import("../../views/live.js").LiveShell} shell
 */
export function startMockControls(shell) {
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
    liveActions.render();
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
export function openMockPanel(anchor) {
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
export function openMockMenu(anchor) {
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
  liveActions.render();
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
      liveActions.renderReference();
    }, UNDO_MOCK);
  }
  liveActions.render();
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
export async function undoRemovedMock() {
  const removed = live.removedMock;
  forgetRemovedMock();
  if (!removed) {
    return;
  }
  const refused = await assignMock(removed.path, removed.page);
  if (refused !== "") {
    live.undoMockFailed = { page: removed.page, text: `Mock not assigned again · ${removed.path}: ${refused}` };
    liveActions.renderReference();
  }
}

/**
 * モックを出す。読むのは出し始めるときだけ: モックに切り替えたとき、ページを移ったとき、
 * 表示幅を切り替えたとき、読み直す操作を押したとき。出している間にファイルが変わっても
 * 勝手には描き直さない（R-PAGE-MOCK）。読み直すときは枠ごと作り直す。
 * @param {string} url
 */
export function showMock(url) {
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
      liveActions.renderReference();
    }
  });
}
