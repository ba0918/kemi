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

/**
 * 広い画面で会話パネルを開いているか（表示の好み。R-SERVE）。既定は畳んだ帯。
 * @returns {boolean}
 */
export function loadConversationOpen() {
  return localStorage.getItem("kemi-conversation-open") === "1";
}

/** @param {boolean} open */
export function saveConversationOpen(open) {
  localStorage.setItem("kemi-conversation-open", open ? "1" : "0");
}

/**
 * 会話パネルの幅（px）。覚えていなければ null。
 * @returns {number | null}
 */
export function loadConversationWidth() {
  const width = Number(localStorage.getItem("kemi-conversation-width"));
  return Number.isFinite(width) && width > 0 ? width : null;
}

/** @param {number} width */
export function saveConversationWidth(width) {
  localStorage.setItem("kemi-conversation-width", String(Math.round(width)));
}
