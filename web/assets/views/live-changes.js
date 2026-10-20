// @ts-check
// `--live` のページの見方の左の列: ページのツリーと、表示中のページの下の変化の一覧（live.md の R-PAGE-VIEW・
// R-PAGE-DIFF）。状態は持たず、押されたときの処理は呼ぶ側が渡す。動き続けるページで比べ直すたびに描き直すので、
// 押している途中の操作とフォーカスを入れ替えない。`--live` のレビューでだけ読み込む。

import { button, el, textEl } from "../dom.js";
import { mockMenuToggle } from "./live.js";

/**
 * 表示中のページの変化の一覧の中身。`list` は要素ごとの変化、`reference` は比べている相手の名前、`unmarked` は、
 * 消えた要素をスナップショットの側に印で示せないこと。
 * @typedef {{
 *   list: import("../live-diff.js").ElementChanges[],
 *   reference: string,
 *   shiftedOpen: boolean,
 *   listed: { main: number, shifted: number },
 *   unmarked: boolean,
 *   fromStart: boolean,
 *   canRecord: boolean,
 *   recording: boolean,
 * }} ChangeListState
 * 比べられないときに一覧の代わりに出す知らせ。
 * @typedef {{ notice: string }} ChangeNotice
 * 比べる相手がモックのとき、一覧の代わりに出す見出し（モックの名前）。
 * @typedef {{ mock: string }} ChangeMock
 */

/**
 * ページのツリー（R-PAGE-VIEW）。ページを押すとそのページへ、表示幅の札を押すとその幅で
 * そのページへ移る。表示中のページの下には、比べた結果があれば変化の一覧を出す。
 * @param {HTMLElement} container
 * @param {import("../live-model.js").PageTreeItem[]} items
 * @param {{
 *   onPage: (page: string) => void,
 *   onWidth: (page: string, width: number) => void,
 * } & ChangeHandlers} handlers
 * @param {ChangeListState | ChangeNotice | ChangeMock | null} changes
 * @param {boolean} locked ページと表示幅を変えられない間（コメントの保存中）。移る操作を使えないと出す
 */
export function renderPageTree(container, items, handlers, changes, locked) {
  // 変化の一覧の中の操作にフォーカスがあれば、描き直した一覧の同じ操作に戻す（行を押して描き直したときも）。
  const previous = container.querySelector('.lv-page[data-current="true"] > .lv-changes');
  const focused = focusInChanges(previous, container.ownerDocument);
  const head = textEl("div", "lv-tree-head", `Pages ${items.length}`);
  const list = el("ul", "lv-pages");
  for (const item of items) {
    const row = el("li", "lv-page");
    row.dataset.page = item.page;
    if (item.current) {
      row.dataset.current = "true";
    }
    const open = button("lv-page-open");
    open.title = item.page;
    open.disabled = locked;
    open.append(textEl("span", "lv-page-path", item.page));
    open.addEventListener("click", () => handlers.onPage(item.page));
    row.append(open);
    const tags = el("span", "lv-page-tags");
    for (const width of item.widths) {
      const tag = button("lv-width-tag");
      tag.textContent = String(width);
      tag.dataset.width = String(width);
      tag.title = `Show ${item.page} at ${width}px`;
      tag.disabled = locked;
      tag.addEventListener("click", () => handlers.onWidth(item.page, width));
      tags.append(tag);
    }
    if (item.mock) {
      tags.append(textEl("span", "lv-mock-tag", "mock"));
    }
    if (item.comments > 0) {
      const count = textEl("span", "lv-comment-count", String(item.comments));
      count.title = item.comments === 1 ? "1 comment on this page" : `${item.comments} comments on this page`;
      tags.append(count);
    }
    row.append(tags);
    if (item.current && changes !== null) {
      row.append(changeList(changes, handlers));
    }
    list.append(row);
  }
  const fragment = document.createDocumentFragment();
  fragment.append(head, list);
  const previousRow = previous?.parentElement;
  const previousList = previousRow?.parentElement;
  const nextRow = list.querySelector('.lv-page[data-current="true"]');
  const nextBox = nextRow?.querySelector(":scope > .lv-changes");
  if (previous && previousRow && previousList && nextRow && nextBox
      && previousRow.dataset.page === nextRow.getAttribute("data-page")
      && updateStartGuide(previous, nextBox)) {
    replaceAround(previousRow, nextRow, previous, nextBox);
    replaceAround(previousList, list, previousRow, nextRow);
    replaceAround(container, fragment, previousList, list);
  } else {
    container.replaceChildren(fragment);
  }
  // 文書に入ってからでないとフォーカスできない。
  const box = container.querySelector('.lv-page[data-current="true"] > .lv-changes');
  if (box instanceof HTMLElement) {
    refocus(box, focused);
  }
}

/**
 * 変化の一覧の操作。開いたずれただけと並べた数を覚えさせ、行を押したらその要素を見せる。モックの見出しの「…」で
 * モックのメニューを開く。
 * @typedef {{
 *   onShifted: (open: boolean) => void,
 *   onListed: (group: "main" | "shifted", count: number) => void,
 *   onShow: (element: import("../live-diff.js").ElementChanges) => void,
 *   onMockMenu: (anchor: HTMLElement) => void,
 *   onRecord: () => void,
 * }} ChangeHandlers
 */

/**
 * 変化の一覧の中の操作。描き直したときに同じ操作へフォーカスを移すために見分ける。要素の行は、同じ要素（側と番号）の行に戻す。
 */
const FOCUSABLE_IN_CHANGES = [
  ".lv-shifted > summary",
  ".lv-change-main .lv-change-show",
  ".lv-change-shifted .lv-change-show",
  ".lv-changes-mock .lv-mock-menu-button",
  ".lv-use-current",
];

/**
 * 変化の一覧の中でフォーカスのある操作を、描き直した一覧で探すためのセレクタ。一覧の外にフォーカスがあれば undefined。
 * @param {Element | null} box 描き直す前の一覧
 * @param {Document} document
 * @returns {string | undefined}
 */
function focusInChanges(box, document) {
  const active = document.activeElement;
  if (!box || !active || !box.contains(active)) {
    return undefined;
  }
  if (active instanceof HTMLElement && active.matches(".lv-change-go")) {
    return `.lv-change-go[data-side="${active.dataset.side}"][data-index="${active.dataset.index}"]`;
  }
  return FOCUSABLE_IN_CHANGES.find((selector) => active.matches(selector));
}

/**
 * 描き直した一覧の、前にフォーカスのあった操作にフォーカスを戻す。
 * @param {Element} box
 * @param {string | undefined} selector focusInChanges の返り値
 */
function refocus(box, selector) {
  if (selector) {
    /** @type {HTMLElement | null} */ (box.querySelector(selector))?.focus();
  }
}

/**
 * @param {Element | null} previous
 * @param {Element} box
 */
function updateStartGuide(previous, box) {
  const guide = previous?.querySelector(".lv-start-comparison");
  const next = box.querySelector(".lv-start-comparison");
  const record = guide?.querySelector("button");
  const nextRecord = next?.querySelector("button");
  if (previous && guide && next && record instanceof HTMLButtonElement && nextRecord instanceof HTMLButtonElement) {
    // 初期化中に一覧が変わっても、押しているボタンを入れ替えるとpointerupのクリックが失われる。
    record.disabled = nextRecord.disabled;
    record.textContent = nextRecord.textContent;
    replaceAround(previous, box, guide, next);
    previous.setAttribute("data-main", box.getAttribute("data-main") ?? "0");
    previous.setAttribute("data-shifted", box.getAttribute("data-shifted") ?? "0");
    return true;
  }
  return false;
}

/**
 * @param {Element} parent
 * @param {Node} replacement
 * @param {Node} kept
 * @param {Node} slot
 */
function replaceAround(parent, replacement, kept, slot) {
  // ボタンだけを別の木へ移しても押下は途切れるため、その祖先も文書につないだまま周りを替える。
  const children = [...replacement.childNodes];
  for (const child of [...parent.childNodes]) {
    if (child !== kept) parent.removeChild(child);
  }
  let before = true;
  for (const child of children) {
    if (child === slot) before = false;
    else if (before) parent.insertBefore(child, kept);
    else parent.appendChild(child);
  }
}

/**
 * 表示中のページの下の変化の一覧だけを描き直す（R-PAGE-DIFF）。ページの行とその操作は作り直さない（動き続ける
 * ページで比べ直すたびに、押そうとしている操作やフォーカスが入れ替わらないように）。一覧の中の操作に
 * フォーカスがあれば、描き直した一覧の同じ操作に移す。表示中のページの行がまだ無ければ何もしない
 * （renderPageTree が描く）。
 * @param {HTMLElement} container renderPageTree で描いたツリー
 * @param {ChangeHandlers} handlers
 * @param {ChangeListState | ChangeNotice | ChangeMock | null} changes
 */
export function renderChanges(container, handlers, changes) {
  const row = container.querySelector('.lv-page[data-current="true"]');
  if (!row) {
    return;
  }
  const old = row.querySelector(":scope > .lv-changes");
  const focused = focusInChanges(old, container.ownerDocument);
  if (changes === null) {
    old?.remove();
    return;
  }
  const box = changeList(changes, handlers);
  if (old && updateStartGuide(old, box)) {
    refocus(old, focused);
    return;
  }
  if (old) {
    old.replaceWith(box);
  } else {
    row.append(box);
  }
  refocus(box, focused);
}

/**
 * 一覧に一度に足す項目の数。大きなページで描き直しが重くならないよう、最初はこの数だけ並べ、
 * 残りは続きを出す操作で足す。
 */
const LISTED_CHANGES = 300;

/**
 * 変化の一覧（R-PAGE-DIFF）。要素ごとに 1 行で、主な変化を上に、ずれただけは畳んで下に、印の色の凡例をその下に。
 * 見出しには比べている相手の名前を出す（R-PAGE-VIEW）。比べられないときは、一覧の代わりにそのことを出す。
 * @param {ChangeListState | ChangeNotice | ChangeMock} state
 * @param {ChangeHandlers} handlers
 */
function changeList(state, handlers) {
  if ("notice" in state) {
    const box = el("div", "lv-changes lv-changes-failed");
    box.append(textEl("p", "lv-changes-notice", state.notice));
    return box;
  }
  if ("mock" in state) {
    // モックと比べるときは一覧を出さず、見出しにモックの名前と、選択の横と同じメニューを出す（R-PAGE-VIEW）。
    const box = el("div", "lv-changes lv-changes-mock");
    const head = el("div", "lv-changes-head");
    const vs = textEl("span", "lv-changes-vs", state.mock);
    vs.title = `Compared with ${state.mock}`;
    const toggle = mockMenuToggle();
    toggle.addEventListener("click", () => handlers.onMockMenu(toggle));
    const end = el("span", "lv-changes-mock-end");
    end.append(vs, toggle);
    head.append(textEl("span", "", "Changes"), end);
    box.append(head);
    return box;
  }
  const changes = state.list;
  const main = changes.filter((change) => change.kind !== "shifted");
  const shifted = changes.filter((change) => change.kind === "shifted");
  const box = el("div", "lv-changes");
  box.dataset.main = String(main.length);
  box.dataset.shifted = String(shifted.length);
  const head = el("div", "lv-changes-head");
  const vs = textEl("span", "lv-changes-vs", state.reference);
  vs.title = `Compared with ${state.reference}`;
  head.append(textEl("span", "", `Changes ${main.length}`), vs);
  box.append(head);
  if (state.fromStart && changes.length > 0) {
    const guide = el("div", "lv-start-comparison");
    guide.setAttribute("role", "note");
    guide.append(textEl("p", "", "Compared with the page at load time. Changes during initialization are included."));
    const record = button("lv-record lv-use-current");
    record.textContent = state.recording ? "Recording…" : "Use current as baseline";
    record.disabled = !state.canRecord;
    record.addEventListener("click", handlers.onRecord);
    guide.append(record);
    box.append(guide);
  }
  if (state.unmarked) {
    box.append(textEl("p", "lv-changes-unmarked", "Not marked: removed elements cannot be located in the snapshot"));
  }
  if (changes.length === 0) {
    box.append(textEl("p", "lv-changes-none", "No changes from the snapshot"));
    return box;
  }
  if (main.length > 0) {
    box.append(changeItems("lv-change-main", main, state.listed.main, handlers, (count) => handlers.onListed("main", count)));
  }
  if (shifted.length > 0) {
    const details = /** @type {HTMLDetailsElement} */ (el("details", "lv-shifted"));
    const shiftedItems = () =>
      changeItems("lv-change-shifted", shifted, state.listed.shifted, handlers, (count) => handlers.onListed("shifted", count));
    details.open = state.shiftedOpen;
    const summary = textEl("summary", "", `Shifted only ${shifted.length}`);
    summary.append(textEl("span", "lv-shifted-why", "Moved or resized; nothing else changed."));
    details.append(summary);
    if (state.shiftedOpen) {
      details.append(shiftedItems());
    }
    // 畳んでいる間は項目を作らない（ずれただけは数が多くなりやすい）。
    details.addEventListener("toggle", () => {
      handlers.onShifted(details.open);
      if (details.open && !details.querySelector(".lv-change-shifted")) {
        details.append(shiftedItems());
      }
    });
    box.append(details);
  }
  box.append(changeLegend());
  return box;
}

/** 印の色の意味（R-PAGE-DIFF）。点の色は一覧の行の点と、ページの上の印の色と同じ。 */
const CHANGE_LEGEND = /** @type {const} */ ([
  ["main", "changed"],
  ["added", "added"],
  ["removed", "removed (on the snapshot)"],
  ["shifted", "moved only"],
]);

/** @returns {HTMLElement} */
function changeLegend() {
  const legend = el("div", "lv-changes-legend");
  legend.setAttribute("aria-label", "What the marks mean");
  for (const [kind, label] of CHANGE_LEGEND) {
    const item = el("span", "lv-legend-item");
    item.dataset.kind = kind;
    item.append(el("i", "lv-change-dot"), label);
    legend.append(item);
  }
  return legend;
}

/**
 * 変化のあった要素を `listed` 個まで並べ、残りがあれば続きを出す操作を置く。押すと次の LISTED_CHANGES 個を足し、
 * 並べた数を `onListed` に知らせる（描き直しても同じ数まで並べるため）。行を押すとその要素を見せる。
 * @param {string} className
 * @param {import("../live-diff.js").ElementChanges[]} changes
 * @param {number} listed
 * @param {ChangeHandlers} handlers
 * @param {(count: number) => void} onListed
 */
function changeItems(className, changes, listed, handlers, onListed) {
  const list = el("ul", `lv-change-list ${className}`);
  let shown = 0;
  const more = el("li", "lv-change-more");
  const remaining = textEl("span", "", "");
  const showMore = button("lv-change-show");
  showMore.textContent = "Show more";
  more.append(remaining, showMore);
  /** @param {number} count */
  const showUpTo = (count) => {
    const items = changes.slice(shown, count).map((element) => {
      const item = el("li", "lv-change");
      item.dataset.kind = element.kind;
      item.dataset.tag = element.tag;
      const go = button("lv-change-go");
      go.dataset.side = element.side;
      go.dataset.index = String(element.index);
      go.title = element.side === "before" ? "Show it on the snapshot" : "Show it on the page";
      const what = el("span", "lv-change-what");
      what.append(textEl("span", "lv-change-el", element.excerpt === "" ? element.tag : `${element.tag} “${element.excerpt}”`));
      what.append(" — ", ...elementChanges(element));
      go.append(el("i", "lv-change-dot"), what, textEl("span", "lv-change-where", element.label));
      go.addEventListener("click", () => handlers.onShow(element));
      item.append(go);
      return item;
    });
    more.before(...items);
    shown = Math.min(count, changes.length);
    remaining.textContent = `${changes.length - shown} more `;
    more.hidden = shown >= changes.length;
  };
  list.append(more);
  showUpTo(Math.max(listed, LISTED_CHANGES));
  showMore.addEventListener("click", () => {
    showUpTo(shown + LISTED_CHANGES);
    onListed(shown);
  });
  return list;
}

/**
 * 1 つの要素で変わったものを並べた文。色の前後は色の見本で、ほかの見た目と文字は前後の値で出す。
 * @param {import("../live-diff.js").ElementChanges} element
 * @returns {(Node | string)[]}
 */
function elementChanges(element) {
  /** @type {(Node | string)[]} */
  const parts = [];
  element.changes.forEach((change, index) => {
    if (index > 0) {
      parts.push(", ");
    }
    parts.push(...changeWhat(change));
  });
  return parts;
}

/**
 * 変化 1 つの文。
 * @param {import("../live-diff.js").Change} change
 * @returns {(Node | string)[]}
 */
function changeWhat(change) {
  switch (change.kind) {
    case "visual":
      return [`${change.property} `, ...valueChange(change.was, change.is)];
    case "text":
      return ["text ", textEl("s", "", change.was), " → ", textEl("b", "", change.is)];
    case "added":
      return ["added"];
    case "removed":
      return ["removed"];
    case "shifted":
      return ["moved or resized"];
  }
}

/**
 * 見た目の値の前後。どちらも色なら色の見本を並べ（値は見本に乗せると出る）、そうでなければ値を並べる。
 * @param {string} was
 * @param {string} is
 * @returns {(Node | string)[]}
 */
function valueChange(was, is) {
  const isColor = (/** @type {string} */ value) => value !== "" && CSS.supports("color", value);
  if (isColor(was) && isColor(is)) {
    return [swatch(was), "→", swatch(is)];
  }
  return [textEl("s", "", was), " → ", textEl("b", "", is)];
}

/**
 * @param {string} color
 * @returns {HTMLElement}
 */
function swatch(color) {
  const item = el("span", "lv-swatch");
  item.style.background = color;
  item.title = color;
  item.setAttribute("role", "img");
  item.setAttribute("aria-label", color);
  return item;
}
