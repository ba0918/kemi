// @ts-check
// 左のファイルツリー（グループ見出し、ディレクトリ、ファイルの項目）と、その部分描き。

import { actions } from "../actions.js";
import { DIR_ICON, FILE_ICON, button, dom, el, svgIcon, textEl } from "../dom.js";
import { commentsOf, currentEntry, groupProgressFor, state } from "../state.js";
import {
  buildTree,
  commentCountChanges,
  commitTypeBox,
  formatBytes,
  statusLetter,
} from "../model.js";

/** @typedef {import("../model.js").FileEntry} FileEntry */
/** @typedef {import("../state.js").Entry} Entry */

/**
 * @param {FileEntry} file
 * @returns {HTMLElement}
 */
export function fileStatsEl(file) {
  const stats = el("span", "st");
  if (file.binary) {
    stats.append(
      document.createTextNode(
        `${formatBytes(file.old_size)} → ${formatBytes(file.new_size)}`,
      ),
    );
  } else {
    stats.append(
      textEl("span", "p", `+${file.add}`),
      textEl("span", "m", `−${file.del}`),
    );
  }
  return stats;
}

/**
 * グループの見出しの題。コミットごとでは件名の種類を枠で示し、件名から外す。
 * @param {HTMLElement} parent
 * @param {any} group
 * @param {string} className
 */
export function appendGroupTitle(parent, group, className) {
  const title = el("span", className);
  const box = state.unit === "commit" ? commitTypeBox(group.title || "") : null;
  if (box) {
    title.append(textEl("span", "ctype", box.type), document.createTextNode(box.title));
  } else {
    title.append(document.createTextNode(group.title || group.id));
  }
  parent.append(title);
}

/**
 * ツリーのグループ見出しの進捗。全部見たら数の代わりに「閲」の印を出す。
 * @param {string} groupId
 */
export function updateGroupHead(groupId) {
  const head = state.groupHeads.get(groupId);
  if (!head) {
    return;
  }
  const progress = groupProgressFor(groupId);
  head.root.classList.toggle("done", progress.done);
  head.count.textContent = "";
  if (progress.done) {
    const seal = textEl("span", "seal", "閲");
    seal.title = "All seen";
    head.count.append(seal);
  } else {
    head.count.append(textEl("span", "g-count", `${progress.seen} / ${progress.total}`));
  }
  head.bar.style.width = `${progress.total ? (progress.seen / progress.total) * 100 : 0}%`;
}

/**
 * コメントの件数が変わったファイルの、ツリーの項目だけを描き直す。ツリー全体は
 * 作り直さない（コミットごとで数万項目あると、1 回のコメントで固まる）。
 * @param {any[]} before 変わる前のすべてのコメント
 */
export function refreshCommentBadges(before) {
  for (const changed of commentCountChanges(before, state.allComments)) {
    for (const entry of state.visible) {
      if (
        entry.group.id === changed.group_id &&
        entry.file.path === changed.path &&
        state.treeItems.has(entry.file.id)
      ) {
        treeItem(entry, entry.file.path.split("/").pop() ?? entry.file.path, state.treeItems);
      }
    }
  }
}

export function renderTree() {
  if (state.treeVersion !== state.treeRenderedVersion) {
    rebuildTree();
    state.treeRenderedVersion = state.treeVersion;
  }
  updateTreeActive();
}

/**
 * 狭い画面の引き出し（R-NARROW）。開閉は属性で示し、重ね方と隠し方は CSS の狭い画面の
 * 規則が決める。広い画面ではツリーは常に見えていて、この属性は何も変えない。
 */
export function renderDrawer() {
  if (state.drawerOpen) {
    dom.tree.dataset.drawer = "open";
  } else {
    delete dom.tree.dataset.drawer;
  }
  dom.btnTree.setAttribute("aria-expanded", String(state.drawerOpen));
}

/**
 * 畳める見出しの、いまの開閉と、押したときの切り替え。開閉はページを開いている間だけ
 * `remembered` に覚え、ツリーは作り直さずに見た目だけを変える。
 * @param {HTMLElement} container `data-open` を持つ入れ物
 * @param {HTMLButtonElement} head 押す見出し
 * @param {Map<string, boolean>} remembered 開閉の記憶
 * @param {string} key
 * @returns {HTMLElement} 矢印。見出しのどこに置くかは呼ぶ側が決める
 */
function foldToggle(container, head, remembered, key) {
  const open = remembered.get(key) !== false;
  container.dataset.open = open ? "true" : "false";
  head.setAttribute("aria-expanded", String(open));
  const caret = textEl("span", "caret", open ? "▾" : "▸");
  head.addEventListener("click", () => {
    const nextOpen = container.dataset.open === "false";
    container.dataset.open = nextOpen ? "true" : "false";
    remembered.set(key, nextOpen);
    head.setAttribute("aria-expanded", String(nextOpen));
    caret.textContent = nextOpen ? "▾" : "▸";
  });
  return caret;
}

function rebuildTree() {
  const groups = buildTree(state.visible);
  /** 前回のボタンを使い回す。同じファイルの項目は作り直さない。 */
  const items = new Map(state.treeItems);
  dom.tree.textContent = "";
  state.groupHeads = new Map();
  const fragment = document.createDocumentFragment();
  for (const { group, nodes } of groups) {
    const groupEl = el("div", "group");
    groupEl.dataset.group = group.id;
    const head = button("group-head");
    head.title = group.title || group.id;
    const caret = foldToggle(groupEl, head, state.groupOpen, group.id);
    const count = el("span", "g-progress");
    head.append(caret);
    appendGroupTitle(head, group, "gtitle");
    head.append(count);
    const barTrack = el("div", "g-bar");
    const bar = el("i");
    barTrack.append(bar);
    groupEl.append(head, barTrack);
    // ツリーのグループ見出しには why と watch を出さない（グループ帯に出す）。
    const body = el("div", "group-body");
    const list = el("ul", "files");
    appendNodes(list, nodes, group, items);
    body.append(list);
    groupEl.append(body);
    fragment.append(groupEl);
    state.groupHeads.set(group.id, { root: groupEl, count, bar });
    updateGroupHead(group.id);
  }
  dom.tree.append(fragment);
  // 使い回したボタンに前の選択の印が残ると、2 つのファイルが選ばれて見える。印を外してから
  // 選び直させる。
  if (state.treeActiveId) {
    items.get(state.treeActiveId)?.classList.remove("active");
  }
  state.treeItems = items;
  state.treeActiveId = null;
}

/**
 * @param {HTMLElement} parent
 * @param {import("../model.js").TreeNode[]} nodes
 * @param {any} group
 * @param {Map<string, HTMLButtonElement>} items
 */
function appendNodes(parent, nodes, group, items) {
  for (const node of nodes) {
    if (node.type === "dir") {
      const li = el("li", "dir");
      const key = `${group.id}:${node.path}`;
      const head = button("dir-head");
      const caret = foldToggle(li, head, state.dirOpen, key);
      head.append(caret, svgIcon(DIR_ICON), document.createTextNode(node.name));
      const ul = el("ul");
      appendNodes(ul, node.children, group, items);
      li.append(head, ul);
      parent.append(li);
    } else {
      const li = el("li");
      li.append(treeItem({ file: node.file, group }, node.name, items));
      parent.append(li);
    }
  }
}

/**
 * @param {Entry} entry
 * @param {string} label
 * @param {Map<string, HTMLButtonElement>} items
 * @returns {HTMLButtonElement}
 */
function treeItem(entry, label, items) {
  let item = items.get(entry.file.id);
  if (!item) {
    item = button("file");
    item.dataset.fileId = entry.file.id;
    item.addEventListener("click", () => {
      const index = state.visible.findIndex(
        (candidate) => candidate.file.id === item?.dataset.fileId,
      );
      void actions.selectIndex(index, { scrollTop: true });
    });
    items.set(entry.file.id, item);
  }
  item.textContent = "";
  item.title = entry.file.path;
  // 見たは左端のチェックで示す。取り消し線は削除と見分けがつかないので使わない。
  item.classList.toggle("seen", entry.file.seen);
  const letter = statusLetter(entry.file.status);
  item.append(
    el("span", "chk"),
    svgIcon(FILE_ICON),
    textEl("span", "fname", label),
  );
  const comments = commentsOf(entry).length;
  if (comments > 0) {
    item.append(textEl("span", "cbadge", `💬 ${comments}`));
  }
  if (entry.file.focus) {
    item.append(textEl("span", "badge-focus", "Important"));
  }
  if (entry.file.noise) {
    item.append(textEl("span", "badge-noise", "Noise"));
  }
  item.append(textEl("span", `sl ${letter}`, letter), fileStatsEl(entry.file));
  return item;
}

function updateTreeActive() {
  const entry = currentEntry();
  const nextId = entry ? entry.file.id : null;
  if (nextId === state.treeActiveId) {
    return;
  }
  if (state.treeActiveId) {
    state.treeItems.get(state.treeActiveId)?.classList.remove("active");
  }
  if (nextId) {
    state.treeItems.get(nextId)?.classList.add("active");
  }
  state.treeActiveId = nextId;
}
