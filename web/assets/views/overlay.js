// @ts-check
// 画面に重ねて出すもの: 知らせ、完了画面、確認ダイアログ、短い通知。

import { dom, button, copyText, el, textEl } from "../dom.js";
import { state } from "../state.js";
import { countLabel, unitLabel } from "../model.js";

/** 短い通知を消すタイマー。 */
let toastTimer = 0;

/**
 * 確認ダイアログの OK で行う処理。開いていなければ null。
 * @type {(() => void) | null}
 */
let modalAction = null;

/**
 * 確認ダイアログの本文を、改行を保つ段落 1 つにする。
 * @param {string} text
 */
function setModalText(text) {
  dom.modalBody.textContent = "";
  dom.modalBody.append(textEl("p", "", text));
}

/**
 * 確認ダイアログを出す。題・本文・OK の文言と見た目・戻るの文言・OK で行う処理は、
 * 呼ぶ側がすべて決める（既定は置かない）。
 * @param {{
 *   title: string,
 *   body: string | HTMLElement[],
 *   okLabel: string,
 *   okClass: string,
 *   cancelLabel: string,
 *   action: () => void,
 * }} options 本文は、改行を保つ段落 1 つなら文字列で、組んだ要素ならその並びで渡す
 */
export function openModal(options) {
  dom.modalTitle.textContent = options.title;
  if (typeof options.body === "string") {
    setModalText(options.body);
  } else {
    dom.modalBody.textContent = "";
    dom.modalBody.append(...options.body);
  }
  dom.modalOk.textContent = options.okLabel;
  dom.modalOk.className = options.okClass;
  dom.modalCancel.textContent = options.cancelLabel;
  modalAction = options.action;
  dom.modal.hidden = false;
}

/**
 * 送信の確認の本文。送るコメントの件数（両方のグループ単位の合計）と表示中の単位の見たファイル数、
 * 見ていないファイルとまだ渡していないものの注意、取り消せないことを並べる（R-SUBMIT）。
 * @param {{
 *   summary: { comments: number, suggestions: number, seen: number, total: number, unseen: number },
 *   unit: string | null,
 *   unhanded: string | null,
 *   approve: boolean,
 * }} options unit は表示中のグループ単位、unhanded はまだ渡していないものの注意（無ければ null）
 * @returns {HTMLElement[]}
 */
export function submitConfirmBody({ summary, unit, unhanded, approve }) {
  const list = el("dl", "sum");
  const seenLabel = unit ? `Seen files (${unitLabel(unit)})` : "Seen files";
  list.append(
    textEl("dt", "", "Comments"),
    textEl("dd", "", `${countLabel(summary.comments, "comment")} (${summary.suggestions} with a suggestion)`),
    textEl("dt", "", seenLabel),
    textEl("dd", "", `${summary.seen} / ${summary.total}`),
  );
  /** @type {HTMLElement[]} */
  const body = [list];
  if (summary.unseen > 0) {
    body.push(textEl("p", "warn", `${countLabel(summary.unseen, "file")} not seen yet.`));
  }
  if (unhanded) {
    const notice = textEl("p", "warn", unhanded);
    notice.id = "unhanded-notice";
    body.push(notice);
  }
  body.push(
    textEl(
      "p",
      "",
      `This ends the review and returns the ${approve ? "approve" : "request changes"} verdict and the comments to the terminal as JSON. This cannot be undone.`,
    ),
  );
  return body;
}

export function closeModal() {
  dom.modal.hidden = true;
  modalAction = null;
}

/**
 * @param {string} title
 * @param {string|null} detail
 */
export function showOverlay(title, detail) {
  if (state.submitted) {
    // 送信後はサーバが止まっているので、失敗を出しても直せない。完了画面（結果の JSON と
    // その写し）を失敗の知らせで置き換えない（R-SUBMIT）。
    return;
  }
  dom.overlay.hidden = false;
  dom.overlayCard.textContent = "";
  dom.overlayCard.className = "";
  dom.overlayCard.append(textEl("h2", "overlay-title", title));
  if (detail) {
    dom.overlayCard.append(textEl("p", "overlay-detail", detail));
  }
  const close = button("btn overlay-close");
  close.textContent = "Close";
  close.addEventListener("click", () => {
    dom.overlay.hidden = true;
  });
  dom.overlayCard.append(close);
}

/**
 * 送信後の完了画面。結果の JSON（stdout と同じ）と、写す操作、結果ファイルの保存先を出す。
 * 承認のときは「閲」の印を押す（R-SUBMIT, R-RESULT）。
 * @param {"approved" | "changes_requested"} verdict
 * @param {any} answer submit の応答（result と saved）
 */
export function showCompletion(verdict, answer) {
  const result = answer && answer.result ? answer.result : answer;
  const saved = (answer && answer.saved) || {};
  const json = JSON.stringify(result);
  const comments = Array.isArray(result && result.comments) ? result.comments : [];
  dom.overlay.hidden = false;
  dom.overlayCard.textContent = "";
  dom.overlayCard.className = "finish";
  if (verdict === "approved") {
    const seal = textEl("span", "seal big", "閲");
    seal.setAttribute("aria-hidden", "true");
    dom.overlayCard.append(seal);
  }
  const body = el("div", "finish-body");
  body.append(
    textEl("h2", "overlay-title", verdict === "approved" ? "Approval sent" : "Change request sent"),
    textEl(
      "p",
      "overlay-detail",
      "kemi wrote the result to standard output and exited. If the agent does not respond, copy this JSON into the conversation.",
    ),
  );
  const row = el("div", "finish-row");
  const copy = button("btn primary");
  copy.textContent = "Copy JSON";
  copy.addEventListener("click", async () => {
    const failure = await copyText(json);
    copy.textContent = failure === null ? "Copied" : `Could not copy: ${failure}`;
  });
  const suggestions = comments.filter((/** @type {any} */ comment) => comment.suggestion).length;
  row.append(copy, textEl("span", "finish-meta", `${countLabel(comments.length, "comment")} / ${countLabel(suggestions, "suggestion")}`));
  body.append(row);
  if (saved.error) {
    body.append(textEl("p", "finish-save failed", `could not save: ${saved.error}`));
  } else if (saved.path) {
    body.append(textEl("p", "finish-save", `Result file: ${saved.path} (read it with kemi --result)`));
  } else if (saved.dir) {
    body.append(textEl("p", "finish-save", `Result file directory: ${saved.dir}`));
  }
  const pre = el("pre", "finish-json");
  pre.textContent = json;
  body.append(pre);
  dom.overlayCard.append(body);
}

/**
 * @param {string} message
 */
export function showToast(message) {
  dom.toast.textContent = message;
  dom.toast.hidden = false;
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    dom.toast.hidden = true;
  }, 2_000);
}

export function runModalAction() {
  const action = modalAction;
  closeModal();
  if (action) {
    action();
  }
}
