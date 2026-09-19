// @ts-check
// テーマの適用と切り替え。

import { invalidateHorizontalWidth } from "./horizontal-scroll.js";
import { dom, svgIcon } from "../dom.js";
import { THEME_LABELS, currentEntry, state } from "../state.js";
import { saveTheme } from "../storage.js";
import { isDarkTheme, nextTheme, resolveTheme } from "../model.js";

const THEME_ICON =
  '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M13.5 9.5A5.5 5.5 0 1 1 6.5 2.5a4.5 4.5 0 0 0 7 7z"/></svg>';
import { scheduleRender } from "./display.js";
import { selectEntry } from "./files.js";

export function applyTheme() {
  invalidateHorizontalWidth();
  const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const resolved = resolveTheme(state.theme, prefersDark);
  const dark = isDarkTheme(resolved);
  const changed = state.dark !== dark;
  state.dark = dark;
  state.rulerDirty = true;
  document.documentElement.dataset.theme = resolved;
  const current =
    THEME_LABELS[state.theme] || THEME_LABELS.auto;
  const next = THEME_LABELS[nextTheme(state.theme)] || "";
  dom.btnTheme.title = `Theme: ${current} (click for ${next})`;
  dom.menuTheme.textContent = "";
  dom.menuTheme.append(svgIcon(THEME_ICON), document.createTextNode(`Theme: ${current}`));
  dom.menuTheme.title = `click for ${next}`;
  if (changed && !state.submitted) {
    state.cache.clear();
    const entry = currentEntry();
    if (entry) {
      // テーマ切替は表示色の再取得だけ。入力中のエディタは閉じない。
      void selectEntry(entry, { scrollTop: false, keepEditor: true });
    }
  } else if (!changed) {
    // 明暗が同じでもプリセットが変われば追加・削除の色が変わる。位置の帯を描き直す。
    scheduleRender();
  }
}

export function stepTheme() {
  state.theme = nextTheme(state.theme);
  saveTheme(state.theme);
  applyTheme();
}

export function onSystemThemeChange() {
  if (state.theme === "auto") {
    applyTheme();
  }
}
