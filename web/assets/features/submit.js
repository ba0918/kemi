// @ts-check
// 送信の確認と、送信そのもの。

import * as api from "../api.js";
import { el, textEl } from "../dom.js";
import { state, unitLabel } from "../state.js";
import { submitSummary } from "../model.js";
import { renderDiff, renderFloating } from "./display.js";
import { renderFileHeader } from "../views/file-header.js";
import { renderSubmitButtons } from "../views/header.js";
import { openModal, showCompletion, showOverlay } from "../views/overlay.js";

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
  const list = el("dl", "sum");
  const seenLabel = state.unit ? `Seen files (${unitLabel(state.unit)})` : "Seen files";
  list.append(
    textEl("dt", "", "Comments"),
    textEl("dd", "", `${summary.comments} comments (${summary.suggestions} with a suggestion)`),
    textEl("dt", "", seenLabel),
    textEl("dd", "", `${summary.seen} / ${summary.total}`),
  );
  /** @type {HTMLElement[]} */
  const body = [list];
  if (summary.unseen > 0) {
    body.push(textEl("p", "warn", `${summary.unseen} file(s) not seen yet.`));
  }
  body.push(
    textEl(
      "p",
      "",
      `This ends the review and returns the ${approve ? "approve" : "request changes"} verdict and the comments to the terminal as JSON. This cannot be undone.`,
    ),
  );
  openModal({
    title: approve ? "Approve and finish?" : "Request changes and finish?",
    body,
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
    renderDiff();
    renderFloating();
    renderFileHeader();
    showCompletion(verdict, answer);
  } catch (error) {
    showOverlay("could not submit", String(error));
  }
}
