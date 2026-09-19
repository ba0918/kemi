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
  renderToggleEnabled,
  state,
} from "../state.js";
import { collapseDefault, countLabel, formatBytes, statusLetter } from "../model.js";
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
  line.append(textEl("span", "gh-st", `Seen ${progress.seen} / ${progress.total}`));
  if (entry.group.why) {
    const toggle = button("gh-toggle");
    const label = () => (dom.groupHeader.dataset.open === "true" ? "Collapse explanation" : "Expand explanation");
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
      textEl("b", "", "Watch"),
      document.createTextNode(entry.group.watch),
    );
    dom.groupHeader.append(watch);
  }
}

function highlightTitle() {
  if (state.highlightEnabled) {
    return "Turn off highlighting";
  }
  if (state.highlightCapable) {
    return "Turn on highlighting";
  }
  return "Turn on highlighting for this file";
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
    badge.title = `${countLabel(comments, "comment")} on this file (including file-wide comments)`;
    dom.fileHeader.append(badge);
  }
  dom.fileHeader.append(fileStatsEl(entry.file));
  if (entry.file.focus) {
    dom.fileHeader.append(textEl("span", "badge focus", "Important"));
  }
  if (entry.file.note) {
    dom.fileHeader.append(textEl("span", "note", entry.file.note));
  }
  dom.fileHeader.append(el("span", "spacer"));

  // 取得が終わるまでは、前のファイルに操作が届かないよう、ヘッダの操作を無効にする。
  const busy = state.loading;
  const comment = iconButton("Comment on the whole file", COMMENT_ICON);
  comment.disabled = state.submitted || busy;
  comment.dataset.focusKey = "file-comment";
  comment.addEventListener("click", () => actions.openFileWideEditor());
  dom.fileHeader.append(comment);

  const copy = iconButton("Copy path", COPY_ICON);
  copy.dataset.focusKey = "file-copy";
  copy.addEventListener("click", () => actions.copyPath(entry.file.path, copy));
  dom.fileHeader.append(copy);

  const fullyExpanded = allLinesExpanded();
  const expand = iconButton(
    fullyExpanded ? "Collapse all" : "Expand all lines",
    EXPAND_ICON,
  );
  expand.disabled = state.submitted || state.binary || busy || state.renderedActive;
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

  renderViewSwitch(entry);

  if (originAvailable() && !state.binary && !state.highlightCapable) {
    const forced = state.originForced.has(entry.file.id);
    const origin = iconButton(
      forced ? "Hide origin for this file" : "Find origin for this file",
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
  seen.title = entry.file.seen ? "Mark as not seen (v)" : "Mark as seen (v)";
  seen.disabled = busy;
  seen.dataset.focusKey = "file-seen";
  seen.setAttribute("aria-pressed", String(entry.file.seen));
  seen.append(el("span", "chk"), document.createTextNode("Seen"), textEl("kbd", "", "v"));
  seen.addEventListener("click", () => actions.toggleSeen(entry.file));
  dom.fileHeader.append(seen);
  restoreFocusKey(dom.fileHeader, focusKey);
}

/**
 * 描画表示とソース表示の切り替え（R-RENDER）。対象のファイルにだけ出し、取得中と、事前に
 * 分かる描画不可では押せず、理由をツールチップに出す。押してから描画に失敗したときは
 * ソース表示のまま、理由をヘッダに出す。
 * @param {import("../state.js").Entry} entry
 */
function renderViewSwitch(entry) {
  const info = state.renderInfo;
  if (!info || !info.toggle) {
    return;
  }
  const group = el("div", "seg view-switch");
  group.setAttribute("role", "group");
  group.setAttribute("aria-label", "View");
  const enabled = renderToggleEnabled();
  const rendered = button("iconbtn view-rendered");
  rendered.textContent = "Rendered";
  rendered.title = info.reason ? `Cannot render: ${info.reason}` : "Rendered view (r)";
  rendered.disabled = !enabled;
  rendered.dataset.focusKey = "file-rendered";
  rendered.setAttribute("aria-pressed", String(state.renderedActive));
  rendered.addEventListener("click", () => actions.setRendered(entry, true));
  const source = button("iconbtn view-source");
  source.textContent = "Source";
  source.title = "Source view (r)";
  source.disabled = !enabled;
  source.dataset.focusKey = "file-source";
  source.setAttribute("aria-pressed", String(!state.renderedActive));
  source.addEventListener("click", () => actions.setRendered(entry, false));
  group.append(rendered, source);
  dom.fileHeader.append(group);
  if (state.renderFailure) {
    dom.fileHeader.append(textEl("span", "render-failed", `Cannot render: ${state.renderFailure}`));
  }
}

export function renderNotice() {
  dom.notice.hidden = true;
  dom.notice.textContent = "";
  const entry = currentEntry();
  if (!entry) {
    if (state.review) {
      dom.notice.hidden = false;
      dom.notice.textContent = "No files to show";
    }
    return;
  }
  if (state.loading || state.renderLoading) {
    dom.notice.hidden = false;
    dom.notice.append(textEl("span", "notice-text", state.loading ? "Loading…" : "Rendering…"));
    return;
  }
  if (state.binary) {
    dom.notice.hidden = false;
    dom.notice.append(
      textEl(
        "span",
        "notice-text",
        `Binary: ${formatBytes(entry.file.old_size)} → ${formatBytes(entry.file.new_size)}`,
      ),
    );
    return;
  }
  if (collapseDefault(entry.file, state.collapsedOverrides)) {
    dom.notice.hidden = false;
    const label = entry.file.noise ? "Noise" : "Fold";
    dom.notice.append(textEl("span", "notice-text", `${label}: content folded`));
    const open = button("btn");
    open.textContent = "Show";
    open.addEventListener("click", () => actions.showCollapsed(entry));
    dom.notice.append(open);
  }
}
