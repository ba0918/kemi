// @ts-check
// 差分の 1 行分（折りたたみ行・由来の行・コードの行）と、その行に付く吹き出しや入力欄。

import { actions } from "../actions.js";
import { appendSegments, button, el, textEl } from "../dom.js";
import { currentEntry, currentOrigin, selectionContains, state } from "../state.js";
import { lineAnchor, lineHasAnchor, originJumpTarget, sideTone } from "../model.js";
import { renderCommentOrEditor, renderEditor } from "./comment.js";
import { renderFileHeader } from "./file-header.js";
import { showToast } from "./overlay.js";

/** @typedef {import("../model.js").DisplayLine} DisplayLine */

/**
 * @param {DisplayLine} line
 * @param {number} index
 * @returns {HTMLDivElement}
 */
export function renderBlock(line, index) {
  const block = /** @type {HTMLDivElement} */ (el("div", "row-block"));
  block.dataset.kemiRow = "1";
  const rendered = renderLine(line);
  if (state.commented.has(index)) {
    rendered.classList.add("commented");
  }
  block.append(rendered);
  if (line.kind === "origin") {
    const reason = renderOriginReason(line);
    if (reason) {
      block.append(reason);
    }
  }
  const threads = state.threads.byLine.get(index);
  if (threads) {
    for (const comment of threads) {
      // 吹き出しは範囲の最後の行の直下で、コードの列の位置から始める。
      const row = el("div", `bal-row mode-${state.mode} side-${comment.side}`);
      row.append(renderCommentOrEditor(comment));
      block.append(row);
    }
  }
  const editorState = state.editor;
  const entry = currentEntry();
  if (
    editorState &&
    !editorState.wide &&
    !editorState.editId &&
    editorState.fileId === (entry ? entry.file.id : "") &&
    lineHasAnchor(line, editorState.side, editorState.anchor)
  ) {
    block.append(renderEditor(editorState));
  }
  return block;
}

/**
 * @param {DisplayLine} line
 * @returns {HTMLElement}
 */
function renderLine(line) {
  if (line.kind === "origin") {
    return renderOriginLine(line);
  }
  const tone = state.mode === "split" ? "" : sideTone(line.kind, null);
  const row = el("div", `row kind-${line.kind}${tone ? ` tone-${tone}` : ""}`);
  if (line.kind === "skip") {
    const skip = line.skip;
    const expand = button("expand-button");
    expand.textContent = `↕ ${skip && skip.count ? skip.count : 0} 行を表示`;
    expand.disabled = state.loading;
    expand.addEventListener("click", () => {
      void actions.expandSkipAt(line.logicalIndex).then(() => renderFileHeader());
    });
    row.append(expand);
    const range = state.skipRanges.get(line.logicalIndex);
    if (range) {
      row.append(textEl("span", "skip-range", rangeLabel(range)));
    }
    return row;
  }
  const anchor = lineAnchor(line);
  const canComment = Boolean(anchor) && !state.submitted;
  const plusSide = anchor ? anchor.side : null;
  if (state.mode === "split") {
    row.classList.add("split");
    row.append(
      sideCell(
        "old",
        line.oldLine,
        line.oldSegments,
        canComment && plusSide === "old",
        sideTone(line.kind, "old"),
      ),
      sideCell(
        "new",
        line.newLine,
        line.newSegments,
        canComment && plusSide === "new",
        sideTone(line.kind, "new"),
      ),
    );
    return row;
  }
  row.append(
    numberCell("old", line.oldLine, canComment && plusSide === "old"),
    numberCell("new", line.newLine, canComment && plusSide === "new"),
    textEl("span", "mk", signFor(line.kind)),
  );
  const code = el("span", "code");
  if (line.newLine) {
    fillCode(code, line.newLine, line.newSegments);
  } else if (line.oldLine) {
    fillCode(code, line.oldLine, line.oldSegments);
  }
  row.append(code);
  return row;
}

/**
 * 折りたたみ行に出す、下に続く範囲の旧・新の行番号。
 * @param {{ old: { start: number, end: number } | null, new: { start: number, end: number } | null }} range
 * @returns {string}
 */
function rangeLabel(range) {
  /** @param {{ start: number, end: number }} span */
  const text = (span) => (span.start === span.end ? `${span.start}` : `${span.start}–${span.end}`);
  const parts = [];
  if (range.old) {
    parts.push(`旧 ${text(range.old)}`);
  }
  if (range.new) {
    parts.push(`新 ${text(range.new)}`);
  }
  return parts.join(" → ");
}

/**
 * 変更ブロックのすぐ上の由来の行（R-ORIGIN）。計算中は行だけ先に出して印を置く。
 * @param {DisplayLine} line
 * @returns {HTMLElement}
 */
function renderOriginLine(line) {
  const row = el("div", "row origin-row");
  row.append(textEl("span", "origin-label", "由来"));
  const origin = currentOrigin();
  if (!origin || origin === "pending") {
    row.append(textEl("span", "origin-pending", "計算中…"));
    return row;
  }
  if (origin.failed) {
    row.append(textEl("span", "origin-unknown", "特定できない"));
    return row;
  }
  const block = origin.blocks.get(Number(line.block));
  const entries = block ? block.entries : [];
  const openKey = `${currentEntry()?.file.id}:${line.block}`;
  for (const entry of entries) {
    const commit = (origin.commits || {})[entry.sha] || { subject: "", body: "" };
    const short = String(entry.sha).slice(0, 7);
    const item = button("origin-entry");
    item.dataset.focusKey = `origin:${openKey}:${entry.sha}`;
    item.textContent = entry.merge ? `マージ ${short}` : `${short} ${commit.subject}`;
    item.title = commit.subject || short;
    const open = state.originOpen.get(openKey) === entry.sha;
    item.setAttribute("aria-expanded", String(open));
    item.addEventListener("click", () => actions.toggleOriginReason(openKey, entry.sha));
    row.append(item);
  }
  if (!block || block.unknown === "all") {
    row.append(textEl("span", "origin-unknown", "特定できない"));
  } else if (block.unknown === "some") {
    row.append(textEl("span", "origin-unknown", "一部特定できない"));
  }
  return row;
}

/**
 * 由来を押したときに開く、そのコミットの理由（本文）。
 * @param {DisplayLine} line
 * @returns {HTMLElement | null}
 */
function renderOriginReason(line) {
  const entry = currentEntry();
  const origin = currentOrigin();
  if (!entry || !origin || origin === "pending" || origin.failed) {
    return null;
  }
  const sha = state.originOpen.get(`${entry.file.id}:${line.block}`);
  if (!sha) {
    return null;
  }
  const commit = (origin.commits || {})[sha] || { subject: "", body: "", merge: false };
  const panel = el("div", "origin-reason");
  const head = el("div", "origin-reason-head");
  head.append(
    textEl("span", "origin-sha", String(sha).slice(0, 7)),
    textEl("span", "origin-subject", commit.subject),
  );
  panel.append(head);
  panel.append(
    textEl("p", "origin-body", commit.body || "（本文はありません）"),
  );
  const block = origin.blocks.get(Number(line.block));
  const target = block ? block.entries.find((/** @type {any} */ item) => item.sha === sha) : null;
  // マージの由来は理由を開くだけで、移り先を持たない。
  if (target && !target.merge && target.target) {
    const jump = button("btn origin-jump");
    jump.textContent = "このコミットで見る";
    jump.addEventListener("click", () => {
      const to = target.target;
      actions.switchUnit("commit", {
        find: (entries) => originJumpTarget(entries, sha, to),
        side: to.side,
        line: to.line,
        missing: () => showToast("could not find the target file"),
      });
    });
    panel.append(jump);
  }
  return panel;
}

/**
 * @param {string} kind
 * @returns {string}
 */
function signFor(kind) {
  switch (kind) {
    case "insert":
    case "replace-new":
      return "+";
    case "delete":
    case "replace-old":
      return "−";
    default:
      return "";
  }
}

/**
 * @param {"old" | "new"} side
 * @param {import("../model.js").Line|null} line
 * @param {import("../model.js").Segment[]} segments
 * @param {boolean} withPlus
 * @param {string} tone
 * @returns {HTMLElement}
 */
function sideCell(side, line, segments, withPlus, tone) {
  const cell = el("span", `cell${tone ? ` side-${tone}` : ""}`);
  const code = el("span", "code");
  fillCode(code, line, segments);
  const sign = tone === "del" ? "−" : tone === "add" ? "+" : "";
  cell.append(numberCell(side, line, withPlus), textEl("span", "mk", sign), code);
  return cell;
}

/**
 * @param {HTMLElement} code
 * @param {import("../model.js").Line|null} line
 * @param {import("../model.js").Segment[]} segments
 */
function fillCode(code, line, segments) {
  if (line && line.html) {
    code.innerHTML = line.html;
    return;
  }
  appendSegments(code, segments, line ? line.text : "");
}

/**
 * @param {"old" | "new"} side
 * @param {import("../model.js").Line|null} line
 * @param {boolean} withPlus
 * @returns {HTMLElement}
 */
function numberCell(side, line, withPlus) {
  const cell = el("span", "no-cell");
  const number = textEl("span", "num", line ? String(line.number) : "");
  if (line && !state.submitted) {
    const value = Number(line.number);
    number.addEventListener("mousedown", (event) => {
      event.preventDefault();
      actions.startSelection(side, value);
    });
    number.addEventListener("mouseenter", () => actions.extendSelection(side, value));
    if (selectionContains(side, value)) {
      number.classList.add("selected");
    }
  }
  cell.append(number);
  if (line && withPlus) {
    const plus = button("line-add-btn");
    plus.textContent = "+";
    plus.title = "この行にコメント";
    plus.setAttribute("aria-label", "この行にコメント");
    plus.addEventListener("click", (event) => {
      event.stopPropagation();
      actions.openEditorAt(side, Number(line.number));
    });
    cell.append(plus);
  }
  return cell;
}
