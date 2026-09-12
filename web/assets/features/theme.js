// @ts-check
// テーマの適用と切り替え。

import { dom } from "../dom.js";
import { THEME_LABELS, currentEntry, state } from "../state.js";
import { saveTheme } from "../storage.js";
import { isDarkTheme, nextTheme, resolveTheme } from "../model.js";
import { scheduleRender } from "./display.js";
import { selectEntry } from "./files.js";

export function applyTheme() {
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
  dom.btnTheme.title = `テーマ: ${current}（クリックで ${next}）`;
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

/** テーマのボタン。決めた順にテーマを 1 つ進める。 */
export function stepTheme() {
  state.theme = nextTheme(state.theme);
  saveTheme(state.theme);
  applyTheme();
}

/** OS の明暗が変わった。自動のときだけ、いまのテーマを引き直す。 */
export function onSystemThemeChange() {
  if (state.theme === "auto") {
    applyTheme();
  }
}
