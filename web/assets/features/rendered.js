// @ts-check
// 描画表示（R-RENDER）: 切り替え、取得、描き直し、ブロックからのコメントの入口。
// ファイルごとの切り替えはページを開いている間だけ覚える。

import * as api from "../api.js";
import {
  currentEntry,
  fileCacheKey,
  isShowingFile,
  renderToggleEnabled,
  renderedWanted,
  state,
} from "../state.js";
import { loadDraft } from "../storage.js";
import { collapseDefault, draftKey, placeRenderedComments, renderedStops } from "../model.js";
import { renderDiff } from "./display.js";
import { renderFileHeader, renderNotice } from "../views/file-header.js";
import { renderRendered } from "../views/rendered.js";

/** @typedef {import("../state.js").Entry} Entry */

/**
 * ファイルを表示し終えた後に、そのファイルの描画表示の状態を反映する。描画表示で見たい
 * ファイルは取得して出し、対象でない・描画できない・見たくないファイルはソース表示のまま。
 * @param {Entry} entry
 */
export async function applyRenderedView(entry) {
  state.renderFailure = null;
  const info = state.renderInfo;
  // 畳まれたノイズは開くまで描画表示も出さない。バイナリ（画像）は畳みの知らせを持たず
  // 開く操作も無いので、常に描画表示にする。
  const folded = !entry.file.binary && collapseDefault(entry.file, state.collapsedOverrides);
  if (!info || !info.target || info.reason || folded || !renderedWanted(entry)) {
    showSource();
    return;
  }
  const key = fileCacheKey(entry);
  const generation = state.selectGeneration;
  let data = state.renderCache.get(key);
  if (!data) {
    state.renderLoading = true;
    renderFileHeader();
    renderNotice();
    try {
      data = await api.getRender(entry.file.id, {
        dark: state.dark,
        highlight: state.highlightOverrides.get(entry.file.id),
      });
    } catch (error) {
      // 押してから描画に失敗したファイルは、ソース表示のままヘッダに理由を出す。
      data = { failed: String(error) };
    }
    if (generation !== state.selectGeneration || !isShowingFile(entry.file.id)) {
      // 取得中に別のファイルが選ばれた。古い応答で表示を上書きしない。
      return;
    }
    state.renderLoading = false;
    state.renderCache.set(key, data);
  }
  if (data.failed) {
    state.renderFailure = String(data.failed);
    showSource();
    return;
  }
  state.rendered = {
    kind: data.kind === "image" ? "image" : "document",
    html: String(data.html || ""),
    blocks: data.blocks || [],
    oldLines: data.old_lines || [],
    newLines: data.new_lines || [],
    image: data.kind === "image" ? { old: data.old || null, new: data.new || null, same: Boolean(data.same) } : null,
  };
  state.renderedActive = true;
  recomputeRenderedThreads();
  renderFileHeader();
  renderNotice();
  renderRendered();
  renderDiff();
}

function showSource() {
  state.renderedActive = false;
  state.rendered = null;
  renderFileHeader();
  renderNotice();
  renderRendered();
  renderDiff();
}

/** ブロックの列とコメントから、吹き出しの置き場と止まる場所を決め直す。 */
export function recomputeRenderedThreads() {
  const blocks = state.rendered ? state.rendered.blocks : [];
  state.renderedThreads = placeRenderedComments(blocks, state.comments);
  state.renderedStops = renderedStops(blocks, state.comments);
  state.rulerDirty = true;
}

/** コメントや入力欄が変わった後に描画表示を描き直す。描画表示でなければ何もしない。 */
export function refreshRendered() {
  if (!state.renderedActive) {
    return;
  }
  recomputeRenderedThreads();
  renderRendered();
  renderDiff();
}

/**
 * 描画表示とソース表示を切り替える（キーは r）。
 * @param {Entry} entry
 */
export function toggleRendered(entry) {
  if (!renderToggleEnabled() || !isShowingFile(entry.file.id)) {
    return;
  }
  setRendered(entry, !renderedWanted(entry));
}

/**
 * @param {Entry} entry
 * @param {boolean} rendered
 */
export function setRendered(entry, rendered) {
  if (!renderToggleEnabled() || !isShowingFile(entry.file.id)) {
    return;
  }
  state.renderedFiles.set(entry.file.id, rendered);
  // 入力欄は行かブロックに付くもので、切り替えた先に同じ置き場は無い。
  state.editor = null;
  state.selection = null;
  void applyRenderedView(entry);
}

/**
 * ブロックの `+`。そのブロックの元の行レンジへの行コメントの入力欄を開く（R-RENDER）。
 * side は新側にあるブロック（重ねたものを含む）が新側、消したブロックが旧側。
 * @param {number} index
 */
export function openBlockEditor(index) {
  const entry = currentEntry();
  const rendered = state.rendered;
  if (!entry || !rendered || state.submitted) {
    return;
  }
  const block = rendered.blocks[index];
  if (!block) {
    return;
  }
  const lines = block.side === "new" ? rendered.newLines : rendered.oldLines;
  const range = { side: block.side, start: block.start, end: block.end };
  state.selection = { fileId: entry.file.id, ...range, anchor: block.end };
  state.editor = {
    fileId: entry.file.id,
    ...range,
    anchor: block.end,
    wide: false,
    body: loadDraft(draftKey(entry.file.id, range)),
    suggestion: lines.slice(block.start - 1, block.end).join("\n"),
    suggestionOn: false,
    needsFocus: true,
  };
  refreshRendered();
}
