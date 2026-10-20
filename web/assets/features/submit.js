// @ts-check
// 送信の確認と、送信そのもの。

import * as api from "../api.js";
import { currentUnit, state } from "../state.js";
import { submitSummary, unhandedNotice } from "../model.js";
import { renderDiff, renderFloating } from "./display.js";
import { renderFileHeader } from "../views/file-header.js";
import { renderConversation } from "../views/conversation.js";
import { renderSubmitButtons } from "../views/header.js";
import { openModal, showCompletion, showOverlay, submitConfirmBody } from "../views/overlay.js";

/**
 * 送信の前の確認。取り消せないことと verdict に加えて、送るコメントの件数（両方の
 * グループ単位の合計）と、表示中の単位の見たファイル数を出す（R-SUBMIT）。
 * @param {"approved" | "changes_requested"} verdict
 */
export function openConfirm(verdict) {
  if (state.submitted) {
    return;
  }
  const approve = verdict === "approved";
  const summary = submitSummary(
    state.allComments,
    state.entries.map((entry) => entry.file),
  );
  // まだエージェントに渡していないものが残っていても submit は止めない（R-AGENT-HAND）。
  const unhanded = unhandedNotice(state.agent);
  openModal({
    title: approve ? "Approve and finish?" : "Request changes and finish?",
    body: submitConfirmBody({ summary, unit: currentUnit(), unhanded, approve }),
    okLabel: approve ? "Approve and finish" : "Request changes and finish",
    okClass: approve ? "btn primary" : "btn secondary",
    cancelLabel: "Back",
    action: () => void submitReview(verdict),
  });
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
    renderConversation();
    renderDiff();
    renderFloating();
    renderFileHeader();
    showCompletion(verdict, answer);
  } catch (error) {
    showOverlay("could not submit", String(error));
  }
}
