// @ts-check
// 狭い画面（R-NARROW）: 幅がしきい値をまたいだときの切り替え、ファイルツリーの引き出しの
// 開閉、札の出し入れ。幅そのものは app.js の matchMedia が見て、ここへは真偽値だけが
// 届く。

import { dom } from "../dom.js";
import { state } from "../state.js";
import {
  captureAnchor,
  recomputeDisplay,
  remeasureAndRender,
  renderDiff,
  renderFloating,
} from "./display.js";
import { refreshRendered } from "./rendered.js";
import { closeSheet } from "./conversation.js";
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
  // 狭い画面のシートは閉じ、広い画面では覚えている会話パネルの開閉に戻る。
  closeSheet();
  dom.titleSheet.hidePopover();
  dom.viewMenu.hidePopover();
  dom.notes.hidePopover();
  recomputeDisplay(anchor);
  renderHeader();
  renderDiff();
  renderFloating();
  refreshRendered();
}

/**
 * 狭い画面で画面の下に浮かぶ「Hand to agent」とその案内（R-AGENT-STATE）の高さを `--hand-float-height` に入れる。
 * 差分と描画表示の下をその分空け、終わりの行を浮かぶものの上まで送れるようにする（R-NARROW）。案内の行数は
 * 状態で変わるので測る。
 */
export function reserveHandFloatSpace() {
  new ResizeObserver(() => {
    document.body.style.setProperty("--hand-float-height", `${dom.handFloatBar.getBoundingClientRect().height}px`);
  }).observe(dom.handFloatBar);
}

/**
 * 狭い画面の札を隠す・戻す切り替え。行の高さが変わるので測り直させる。
 */
export function toggleComments() {
  state.narrowComments = !state.narrowComments;
  renderHeader();
  remeasureAndRender();
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
