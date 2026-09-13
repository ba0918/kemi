// @ts-check
// 要素そのものと、要素を作る道具。import したときに document を引く（index.html が
// type="module" で読むので、このとき DOM は出来ている）。ほかのモジュールは import しない。

/**
 * @param {string} selector
 * @returns {HTMLElement}
 */
function must(selector) {
  return /** @type {HTMLElement} */ (document.querySelector(selector));
}

export const dom = {
  titleBlock: must(".title-block"),
  title: must("#review-title"),
  subtitle: must("#review-subtitle"),
  unitSwitch: must("#unit-switch"),
  meta: must("#review-meta"),
  notes: must("#review-notes"),
  btnNotes: /** @type {HTMLButtonElement} */ (must("#btn-notes")),
  progress: must("#progress"),
  progressBar: must("#progress-bar"),
  progressText: must("#progress-text"),
  ruler: must("#ruler"),
  rulerCanvas: /** @type {HTMLCanvasElement} */ (must("#ruler-canvas")),
  rulerView: must("#ruler-view"),
  nav: must("#nav"),
  navPrev: /** @type {HTMLButtonElement} */ (must("#nav-prev")),
  navNext: /** @type {HTMLButtonElement} */ (must("#nav-next")),
  navPos: must("#nav-pos"),
  toast: must("#toast"),
  tree: must("#tree"),
  groupHeader: must("#group-header"),
  fileHeader: must("#file-header"),
  floating: must("#floating-threads"),
  notice: must("#notice"),
  viewport: must("#diff-viewport"),
  content: must("#diff-content"),
  horizontal: must("#horizontal-scroll"),
  horizontalWidth: must("#horizontal-scroll-width"),
  btnUnified: /** @type {HTMLButtonElement} */ (must("#btn-unified")),
  btnSplit: /** @type {HTMLButtonElement} */ (must("#btn-split")),
  btnWrap: /** @type {HTMLButtonElement} */ (must("#btn-wrap")),
  chipFocus: /** @type {HTMLButtonElement} */ (must("#chip-focus")),
  chipSort: /** @type {HTMLButtonElement} */ (must("#chip-sort")),
  btnTheme: /** @type {HTMLButtonElement} */ (must("#btn-theme")),
  btnComments: /** @type {HTMLButtonElement} */ (must("#btn-comments")),
  commentCount: must("#comment-count"),
  commentList: must("#comment-list"),
  updateBadge: /** @type {HTMLButtonElement} */ (must("#update-badge")),
  submitApproved: /** @type {HTMLButtonElement} */ (must("#btn-approve")),
  submitChanges: /** @type {HTMLButtonElement} */ (must("#btn-changes")),
  modal: must("#modal"),
  modalTitle: must("#modal-title"),
  modalBody: must("#modal-body"),
  modalOk: /** @type {HTMLButtonElement} */ (must("#modal-ok")),
  modalCancel: /** @type {HTMLButtonElement} */ (must("#modal-cancel")),
  overlay: must("#overlay"),
  overlayCard: must("#overlay-card"),
};

/**
 * @param {string} tag
 * @param {string} [className]
 * @returns {HTMLElement}
 */
export function el(tag, className) {
  const element = document.createElement(tag);
  if (className) {
    element.className = className;
  }
  return element;
}

/**
 * @param {string} className
 * @returns {HTMLButtonElement}
 */
export function button(className) {
  const element = /** @type {HTMLButtonElement} */ (el("button", className));
  element.type = "button";
  return element;
}

/**
 * @param {string} tag
 * @param {string} className
 * @param {string} text
 * @returns {HTMLElement}
 */
export function textEl(tag, className, text) {
  const element = el(tag, className);
  element.textContent = text;
  return element;
}

export const FILE_ICON =
  '<svg class="fi" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M4 1.5h5l3 3v10H4z"/><path d="M9 1.5v3h3"/></svg>';

export const DIR_ICON =
  '<svg class="fi" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M1.5 3.5h4l1.5 2h7.5v7h-13z"/></svg>';

export const COMMENT_ICON =
  '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M2.5 3.5h11v7h-6l-3 3v-3h-2z"/></svg>';

export const COPY_ICON =
  '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><rect x="5.5" y="5.5" width="8" height="9" rx="1.5"/><path d="M10.5 5.5V3a1.5 1.5 0 0 0-1.5-1.5H4A1.5 1.5 0 0 0 2.5 3v7A1.5 1.5 0 0 0 4 11.5h1.5"/></svg>';

export const EXPAND_ICON =
  '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M3 5.5 8 10l5-4.5"/><path d="M3 2.5h10"/></svg>';

export const CODE_ICON =
  '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M4.5 4 2 8l2.5 4M11.5 4 14 8l-2.5 4M9.5 2.5l-3 11"/></svg>';

export const ORIGIN_ICON =
  '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><circle cx="4" cy="4" r="1.8"/><circle cx="4" cy="12" r="1.8"/><circle cx="12" cy="8" r="1.8"/><path d="M4 5.8v4.4M5.6 4.8 10.4 7.2"/></svg>';

/**
 * 解析済みのアイコン。ツリーの項目ごとに markup を解析し直さず、複製して使う。
 * @type {Map<string, SVGElement>}
 */
const parsedIcons = new Map();

/**
 * @param {string} markup
 * @returns {SVGElement}
 */
export function svgIcon(markup) {
  let icon = parsedIcons.get(markup);
  if (!icon) {
    const template = document.createElement("template");
    template.innerHTML = markup;
    icon = /** @type {SVGElement} */ (template.content.firstElementChild);
    parsedIcons.set(markup, icon);
  }
  return /** @type {SVGElement} */ (icon.cloneNode(true));
}

/**
 * @param {string} title
 * @param {string} markup
 * @returns {HTMLButtonElement}
 */
export function iconButton(title, markup) {
  const element = button("iconbtn");
  element.title = title;
  element.setAttribute("aria-label", title);
  element.append(svgIcon(markup));
  return element;
}

/**
 * @param {HTMLElement} parent
 * @param {import("./model.js").Segment[]|null|undefined} segments
 * @param {string|null|undefined} fallback
 */
export function appendSegments(parent, segments, fallback) {
  if (!segments || segments.length === 0) {
    parent.append(document.createTextNode(fallback ?? ""));
    return;
  }
  for (const segment of segments) {
    if (segment.changed) {
      const mark = el("span", "changed");
      mark.textContent = segment.text;
      parent.append(mark);
    } else {
      parent.append(document.createTextNode(segment.text));
    }
  }
}

/**
 * 描き直しで作り直す操作にフォーカスがあれば、その操作の鍵（data-focus-key）。
 * 作り直した後に同じ鍵の要素へフォーカスを戻し、キーボードの操作を続けられるようにする。
 * @param {HTMLElement} container
 * @returns {string | null}
 */
export function focusKeyWithin(container) {
  const active = document.activeElement;
  if (!(active instanceof HTMLElement) || !container.contains(active)) {
    return null;
  }
  return active.dataset.focusKey ?? null;
}

/**
 * @param {HTMLElement} container
 * @param {string | null} key
 */
export function restoreFocusKey(container, key) {
  if (!key) {
    return;
  }
  const next = container.querySelector(`[data-focus-key="${CSS.escape(key)}"]`);
  if (next instanceof HTMLElement && next !== document.activeElement) {
    next.focus({ preventScroll: true });
  }
}
