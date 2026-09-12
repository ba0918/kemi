// @ts-check
// 画面に重ねて出すもの: 知らせ、完了画面、確認ダイアログ、短い通知。

import { dom, button, el, textEl } from "../dom.js";
import { state } from "../state.js";

/**
 * 確認ダイアログの本文を、改行を保つ段落 1 つにする。
 * @param {string} text
 */
export function setModalText(text) {
  dom.modalBody.textContent = "";
  dom.modalBody.append(textEl("p", "", text));
}

export function closeModal() {
  dom.modal.hidden = true;
  state.modalAction = null;
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
  close.textContent = "閉じる";
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
    textEl("h2", "overlay-title", verdict === "approved" ? "承認を送りました" : "変更要求を送りました"),
    textEl(
      "p",
      "overlay-detail",
      "kemi は結果を標準出力に書いて終了しました。エージェントが反応しないときは、この JSON をコピーして会話に貼れば済みます。",
    ),
  );
  const row = el("div", "finish-row");
  const copy = button("btn primary");
  copy.textContent = "JSON をコピー";
  copy.addEventListener("click", () => {
    navigator.clipboard
      .writeText(json)
      .then(() => {
        copy.textContent = "コピーしました";
      })
      .catch((error) => {
        copy.textContent = `コピーできません: ${error}`;
      });
  });
  const suggestions = comments.filter((/** @type {any} */ comment) => comment.suggestion).length;
  row.append(copy, textEl("span", "finish-meta", `コメント ${comments.length} 件 / suggestion ${suggestions} 件`));
  body.append(row);
  if (saved.error) {
    body.append(textEl("p", "finish-save failed", `保存できませんでした: ${saved.error}`));
  } else if (saved.path) {
    body.append(textEl("p", "finish-save", `結果ファイル: ${saved.path}（kemi --result で読めます）`));
  } else if (saved.dir) {
    body.append(textEl("p", "finish-save", `結果ファイルの保存先: ${saved.dir}`));
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
  window.clearTimeout(state.toastTimer);
  state.toastTimer = window.setTimeout(() => {
    dom.toast.hidden = true;
  }, 2_000);
}

/** 確認ダイアログの OK。閉じてから、覚えておいた処理を実行する。 */
export function runModalAction() {
  const action = state.modalAction;
  closeModal();
  if (action) {
    action();
  }
}
