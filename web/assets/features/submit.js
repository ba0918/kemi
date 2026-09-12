// @ts-check
// 送信の確認と、送信そのもの。

import * as api from "../api.js";
import { dom, el, textEl } from "../dom.js";
import { UNIT_LABELS, state } from "../state.js";
import { submitSummary } from "../model.js";
import { renderDiff, renderFloating } from "./display.js";
import { renderFileHeader } from "../views/file-header.js";
import { renderSubmitButtons } from "../views/header.js";
import { showCompletion, showOverlay } from "../views/overlay.js";

/**
 * 送信の前の確認。取り消せないことと verdict に加えて、送るコメントの件数（両方の
 * グループ単位の合計）と、表示中の単位の見たファイル数を出す（R-SUBMIT）。
 * @param {"approved" | "changes_requested"} verdict
 */
export function openConfirm(verdict) {
  if (state.submitted) {
    return;
  }
  state.modalAction = () => void submitReview(verdict);
  dom.modalCancel.textContent = "戻る";
  const approve = verdict === "approved";
  const summary = submitSummary(
    state.allComments,
    state.entries.map((entry) => entry.file),
  );
  dom.modalTitle.textContent = approve ? "承認して終了しますか？" : "変更要求で終了しますか？";
  dom.modalBody.textContent = "";
  const list = el("dl", "sum");
  const seenLabel = state.unit ? `見たファイル（${UNIT_LABELS[state.unit] || state.unit}）` : "見たファイル";
  list.append(
    textEl("dt", "", "コメント"),
    textEl("dd", "", `${summary.comments} 件（うち suggestion 付き ${summary.suggestions} 件）`),
    textEl("dt", "", seenLabel),
    textEl("dd", "", `${summary.seen} / ${summary.total}`),
  );
  dom.modalBody.append(list);
  if (summary.unseen > 0) {
    dom.modalBody.append(
      textEl("p", "warn", `まだ見ていないファイルが ${summary.unseen} あります。`),
    );
  }
  dom.modalBody.append(
    textEl(
      "p",
      "",
      `レビューを終了して、${approve ? "承認" : "変更要求"}の verdict とコメントを実行ターミナルへ JSON で返します。この操作は取り消せません。`,
    ),
  );
  dom.modalOk.textContent = approve ? "承認して終了" : "変更要求で終了";
  dom.modalOk.className = approve ? "btn primary" : "btn secondary";
  dom.modal.hidden = false;
}

/**
 * @param {"approved" | "changes_requested"} verdict
 */
async function submitReview(verdict) {
  if (state.submitted) {
    return;
  }
  try {
    const answer = await api.submit(verdict);
    state.submitted = true;
    state.selection = null;
    state.editor = null;
    renderSubmitButtons();
    renderDiff();
    renderFloating();
    renderFileHeader();
    showCompletion(verdict, answer);
  } catch (error) {
    showOverlay("送信できませんでした", String(error));
  }
}
