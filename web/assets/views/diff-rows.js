// @ts-check
// 差分の 1 行分（折りたたみ行・由来の行・コードの行）と、その行に付く吹き出しや入力欄。

import { actions } from "../actions.js";
import { appendSegments, button, el, textEl } from "../dom.js";
import {
  currentEntry,
  currentOrigin,
  displayMode,
  selectionContains,
  selectionEndsAt,
  shownComments,
  state,
} from "../state.js";
import { countLabel, lineHasAnchor, originJumpTarget, plusSides, sideTone } from "../model.js";
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
    for (const comment of shownComments(threads)) {
      // 吹き出しは範囲の最後の行の直下で、コードの列の位置から始める。
      const row = el("div", `bal-row mode-${displayMode()} side-${comment.side}`);
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
  const mode = displayMode();
  const tone = mode === "split" ? "" : sideTone(line.kind, null);
  const row = el("div", `row kind-${line.kind}${tone ? ` tone-${tone}` : ""}`);
  if (line.kind === "skip") {
    const skip = line.skip;
    const expand = button("expand-button");
    expand.textContent = `↕ Show ${countLabel(skip && skip.count ? skip.count : 0, "line")}`;
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
  const sides = plusSides(line);
  const canComment = sides.length > 0 && !state.submitted;
  if (mode === "split") {
    row.classList.add("split");
    row.append(
      sideCell(
        "old",
        line.oldLine,
        line.oldSegments,
        canComment && plusHere("old", line.oldLine, sides),
        sideTone(line.kind, "old"),
      ),
      sideCell(
        "new",
        line.newLine,
        line.newSegments,
        canComment && plusHere("new", line.newLine, sides),
        sideTone(line.kind, "new"),
      ),
    );
    return row;
  }
  row.append(
    numberCell("old", line.oldLine, canComment && plusHere("old", line.oldLine, sides)),
    numberCell("new", line.newLine, canComment && plusHere("new", line.newLine, sides)),
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
    parts.push(`old ${text(range.old)}`);
  }
  if (range.new) {
    parts.push(`new ${text(range.new)}`);
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
  row.append(textEl("span", "origin-label", "Origin"));
  const origin = currentOrigin();
  if (!origin || origin === "pending") {
    row.append(textEl("span", "origin-pending", "Computing…"));
    return row;
  }
  if (origin.failed) {
    row.append(textEl("span", "origin-unknown", "Cannot determine"));
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
    item.textContent = entry.merge ? `Merge ${short}` : `${short} ${commit.subject}`;
    item.title = commit.subject || short;
    const open = state.originOpen.get(openKey) === entry.sha;
    item.setAttribute("aria-expanded", String(open));
    item.addEventListener("click", () => actions.toggleOriginReason(openKey, entry.sha));
    row.append(item);
  }
  if (!block || block.unknown === "all") {
    row.append(textEl("span", "origin-unknown", "Cannot determine"));
  } else if (block.unknown === "some") {
    row.append(textEl("span", "origin-unknown", "Some cannot be determined"));
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
    textEl("p", "origin-body", commit.body || "(no body)"),
  );
  const block = origin.blocks.get(Number(line.block));
  const target = block ? block.entries.find((/** @type {any} */ item) => item.sha === sha) : null;
  // マージの由来は理由を開くだけで、移り先を持たない。
  if (target && !target.merge && target.target) {
    const jump = button("btn origin-jump");
    jump.textContent = "View in this commit";
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
  const text = el("span", "code-text");
  fillCode(text, line, segments);
  code.append(text);
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
 * この側の行番号の欄に `+` を作るか。既定はその行が `+` を出す側。狭い画面では
 * 選択中の範囲の最後の行にも作り、見せるのはその 1 つだけにする（R-NARROW）。
 * @param {"old" | "new"} side
 * @param {import("../model.js").Line|null} line
 * @param {("old" | "new")[]} sides
 * @returns {boolean}
 */
function plusHere(side, line, sides) {
  if (sides.includes(side)) {
    return true;
  }
  return state.narrow && line !== null && selectionEndsAt(side, Number(line.number));
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
    number.addEventListener("click", () => actions.tapLine(side, value));
    if (selectionContains(side, value)) {
      number.classList.add("selected");
    }
    if (state.narrow && selectionEndsAt(side, value)) {
      cell.classList.add("selection-end");
    }
  }
  cell.append(number);
  if (line && withPlus) {
    const plus = button("line-add-btn");
    plus.textContent = "+";
    plus.title = "Comment on this line";
    plus.setAttribute("aria-label", "Comment on this line");
    plus.addEventListener("click", (event) => {
      event.stopPropagation();
      actions.openEditorAt(side, Number(line.number));
    });
    cell.append(plus);
  }
  return cell;
}
