// @ts-check
// 本文側の上の帯: グループの帯（題・why・watch）、ファイルの帯（パス・操作）、
// 読み込み中や畳んだ内容の知らせ。

import { actions } from "../actions.js";
import {
  CODE_ICON,
  COMMENT_ICON,
  COPY_ICON,
  EXPAND_ICON,
  ORIGIN_ICON,
  button,
  dom,
  el,
  focusKeyWithin,
  iconButton,
  restoreFocusKey,
  textEl,
} from "../dom.js";
import {
  allLinesExpanded,
  commentsOf,
  currentEntry,
  groupProgressFor,
  originAvailable,
  state,
} from "../state.js";
import { collapseDefault, formatBytes, statusLetter } from "../model.js";
import { appendGroupTitle, fileStatsEl } from "./tree.js";

/** 本文側のグループ帯。why は既定で畳み（開閉はページを開いている間だけ覚える）、watch は常に出す。 */
export function renderGroupHeader() {
  const entry = currentEntry();
  dom.groupHeader.textContent = "";
  if (!entry) {
    return;
  }
  const open = state.groupHeaderOpen.get(entry.group.id) === true;
  dom.groupHeader.dataset.open = open ? "true" : "false";
  const line = el("div", "gh-line");
  appendGroupTitle(line, entry.group, "gh-title");
  const progress = groupProgressFor(entry.group.id);
  line.append(textEl("span", "gh-st", `${progress.seen} / ${progress.total} 見た`));
  if (entry.group.why) {
    const toggle = button("gh-toggle");
    const label = () => (dom.groupHeader.dataset.open === "true" ? "説明を畳む" : "説明を開く");
    toggle.textContent = label();
    toggle.setAttribute("aria-expanded", String(open));
    toggle.addEventListener("click", () => {
      const nextOpen = dom.groupHeader.dataset.open !== "true";
      dom.groupHeader.dataset.open = nextOpen ? "true" : "false";
      state.groupHeaderOpen.set(entry.group.id, nextOpen);
      toggle.setAttribute("aria-expanded", String(nextOpen));
      toggle.textContent = label();
    });
    line.append(toggle);
  }
  dom.groupHeader.append(line);
  if (entry.group.why) {
    dom.groupHeader.append(textEl("p", "gh-why", entry.group.why));
  }
  if (entry.group.watch) {
    const watch = el("div", "gh-watch");
    watch.append(
      textEl("b", "", "見てほしい点"),
      document.createTextNode(entry.group.watch),
    );
    dom.groupHeader.append(watch);
  }
}

function highlightTitle() {
  if (state.highlightEnabled) {
    return "ハイライトを切る";
  }
  if (state.highlightCapable) {
    return "ハイライトを有効にする";
  }
  return "このファイルでハイライトを有効にする";
}

export function renderFileHeader() {
  const focusKey = focusKeyWithin(dom.fileHeader);
  dom.fileHeader.textContent = "";
  const entry = currentEntry();
  if (!entry) {
    return;
  }
  dom.fileHeader.append(textEl("span", "path", entry.file.path));
  if (entry.file.old_path) {
    dom.fileHeader.append(
      textEl("span", "file-old", `← ${entry.file.old_path}`),
    );
  }
  const letter = statusLetter(entry.file.status);
  dom.fileHeader.append(textEl("span", `sl ${letter}`, letter));
  const comments = commentsOf(entry).length;
  if (comments > 0) {
    const badge = textEl("span", "cbadge", `💬 ${comments}`);
    badge.title = `このファイルのコメント ${comments} 件（ファイル全体へのコメントを含む）`;
    dom.fileHeader.append(badge);
  }
  dom.fileHeader.append(fileStatsEl(entry.file));
  if (entry.file.focus) {
    dom.fileHeader.append(textEl("span", "badge focus", "重要"));
  }
  if (entry.file.note) {
    dom.fileHeader.append(textEl("span", "note", entry.file.note));
  }
  dom.fileHeader.append(el("span", "spacer"));

  // 取得が終わるまでは、前のファイルに操作が届かないよう、ヘッダの操作を無効にする。
  const busy = state.loading;
  const comment = iconButton("ファイル全体にコメント", COMMENT_ICON);
  comment.disabled = state.submitted || busy;
  comment.dataset.focusKey = "file-comment";
  comment.addEventListener("click", () => actions.openFileWideEditor());
  dom.fileHeader.append(comment);

  const copy = iconButton("パスをコピー", COPY_ICON);
  copy.dataset.focusKey = "file-copy";
  copy.addEventListener("click", () => actions.copyPath(entry.file.path, copy));
  dom.fileHeader.append(copy);

  const fullyExpanded = allLinesExpanded();
  const expand = iconButton(
    fullyExpanded ? "すべて折りたたむ" : "すべての行を展開",
    EXPAND_ICON,
  );
  expand.disabled = state.submitted || state.binary || busy;
  expand.dataset.focusKey = "file-expand";
  expand.classList.toggle("active", fullyExpanded);
  expand.setAttribute("aria-pressed", String(fullyExpanded));
  expand.addEventListener("click", () => {
    if (fullyExpanded) {
      actions.collapseAll();
    } else {
      actions.expandAll();
    }
  });
  dom.fileHeader.append(expand);

  const highlight = iconButton(highlightTitle(), CODE_ICON);
  highlight.disabled = busy;
  highlight.dataset.focusKey = "file-highlight";
  highlight.classList.toggle("active", state.highlightEnabled);
  highlight.setAttribute("aria-pressed", String(state.highlightEnabled));
  highlight.addEventListener("click", () => actions.toggleHighlight(entry));
  dom.fileHeader.append(highlight);

  if (originAvailable() && !state.binary && !state.highlightCapable) {
    const forced = state.originForced.has(entry.file.id);
    const origin = iconButton(
      forced ? "このファイルの由来を隠す" : "このファイルで由来を求める",
      ORIGIN_ICON,
    );
    origin.disabled = busy;
    origin.dataset.focusKey = "file-origin";
    origin.setAttribute("aria-pressed", String(forced));
    origin.addEventListener("click", () => actions.toggleOrigin(entry));
    dom.fileHeader.append(origin);
  }

  // 見たは文字付きのチェック。キー v でも付け外しできる（R-SEEN）。
  const seen = button(`seen-toggle${entry.file.seen ? " on" : ""}`);
  seen.title = entry.file.seen ? "見たを取り消す（v）" : "見たにする（v）";
  seen.disabled = busy;
  seen.dataset.focusKey = "file-seen";
  seen.setAttribute("aria-pressed", String(entry.file.seen));
  seen.append(el("span", "chk"), document.createTextNode("見た"), textEl("kbd", "", "v"));
  seen.addEventListener("click", () => actions.toggleSeen(entry.file));
  dom.fileHeader.append(seen);
  restoreFocusKey(dom.fileHeader, focusKey);
}

export function renderNotice() {
  dom.notice.hidden = true;
  dom.notice.textContent = "";
  const entry = currentEntry();
  if (!entry) {
    if (state.review) {
      dom.notice.hidden = false;
      dom.notice.textContent = "表示するファイルがありません";
    }
    return;
  }
  if (state.loading) {
    dom.notice.hidden = false;
    dom.notice.append(textEl("span", "notice-text", "読み込み中…"));
    return;
  }
  if (state.binary) {
    dom.notice.hidden = false;
    dom.notice.append(
      textEl(
        "span",
        "notice-text",
        `バイナリ: ${formatBytes(entry.file.old_size)} → ${formatBytes(entry.file.new_size)}`,
      ),
    );
    return;
  }
  if (collapseDefault(entry.file, state.collapsedOverrides)) {
    dom.notice.hidden = false;
    const label = entry.file.noise ? "ノイズ" : "折りたたみ";
    dom.notice.append(textEl("span", "notice-text", `${label}: 内容を畳んでいます`));
    const open = button("btn");
    open.textContent = "表示する";
    open.addEventListener("click", () => actions.showCollapsed(entry));
    dom.notice.append(open);
  }
}
