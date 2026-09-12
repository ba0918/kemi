// @ts-check
// localStorage の読み書きをここへ集める。下書きの鍵の計算は持たず、呼ぶ側が
// model.js の draftKey で作った鍵を渡す。

/**
 * @param {string} key
 * @returns {string}
 */
export function loadDraft(key) {
  return localStorage.getItem(key) || "";
}

/**
 * 下書きは入力のたびに残す（CONTEXT.md の下書き）。
 * @param {string} key
 * @param {string} value
 */
export function saveDraft(key, value) {
  if (value === "") {
    localStorage.removeItem(key);
  } else {
    localStorage.setItem(key, value);
  }
}

/**
 * コメントとして送った下書きを消す。
 * @param {string} key
 */
export function clearDraft(key) {
  localStorage.removeItem(key);
}

/** @returns {"unified" | "split"} */
export function loadMode() {
  return localStorage.getItem("kemi-mode") === "split" ? "split" : "unified";
}

/** @param {"unified" | "split"} mode */
export function saveMode(mode) {
  localStorage.setItem("kemi-mode", mode);
}

/** @returns {string} */
export function loadTheme() {
  return localStorage.getItem("kemi-theme") || "auto";
}

/** @param {string} theme */
export function saveTheme(theme) {
  localStorage.setItem("kemi-theme", theme);
}
