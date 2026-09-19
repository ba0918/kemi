// @ts-check
// 狭い画面（R-NARROW）: 幅がしきい値をまたいだときの切り替えと、ファイルツリーの引き出しの
// 開閉。幅そのものは app.js の matchMedia が見て、ここへは真偽値だけが届く。

import { dom } from "../dom.js";
import { state } from "../state.js";
import { captureAnchor, recomputeDisplay, renderDiff, renderFloating } from "./display.js";
import { refreshRendered } from "./rendered.js";
import { closeCommentList } from "./comment-list.js";
import { renderHeader } from "../views/header.js";
import { renderDrawer } from "../views/tree.js";

/**
 * 幅がしきい値をまたいだ。再読込なしにその場で切り替え、開いていた引き出しとシートは
 * 閉じ、選択中の範囲と入力中の下書きは保つ。表示行の作り直しは上端に見えていた行を残す。
 * @param {boolean} narrow
 */
export function applyNarrow(narrow) {
  if (state.narrow === narrow) {
    return;
  }
  const anchor = captureAnchor();
  state.narrow = narrow;
  closeDrawer();
  closeCommentList();
  dom.titleSheet.hidePopover();
  dom.viewMenu.hidePopover();
  dom.notes.hidePopover();
  recomputeDisplay(anchor);
  renderHeader();
  renderDiff();
  renderFloating();
  refreshRendered();
}

export function openDrawer() {
  state.drawerOpen = true;
  renderDrawer();
}

export function closeDrawer() {
  if (!state.drawerOpen) {
    return;
  }
  state.drawerOpen = false;
  renderDrawer();
}

export function toggleDrawer() {
  if (state.drawerOpen) {
    closeDrawer();
  } else {
    openDrawer();
  }
}
