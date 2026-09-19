// @ts-check
// 上部バー（題、meta、グループ単位、進捗、表示の切り替えの押された状態）、更新バッジ、
// 送信のボタン。

import { actions } from "../actions.js";
import { button, dom, el, focusKeyWithin, restoreFocusKey, textEl } from "../dom.js";
import { displayMode, displayWrap, state, unitLabel } from "../state.js";
import { metaItems, seenProgress, unitSwitchOrder } from "../model.js";

export function renderHeader() {
  const review = state.review;
  const title = review ? review.title : "kemi";
  dom.title.textContent = title;
  dom.btnTitle.textContent = title;
  dom.sheetTitle.textContent = title;
  const subtitle = review ? review.subtitle : "";
  dom.subtitle.textContent = subtitle || "";
  dom.subtitle.hidden = !subtitle;
  dom.sheetSubtitle.textContent = subtitle || "";
  dom.sheetSubtitle.hidden = !subtitle;
  dom.meta.textContent = "";
  dom.notes.textContent = "";
  // 狭い画面では subtitle と meta を title から開くシートに入れる（R-NARROW）。同じ中身を
  // シートにも描き、どちらを見せるかは CSS の狭い画面の規則が決める。
  dom.sheetMeta.textContent = "";
  dom.sheetNotes.textContent = "";
  if (review) {
    for (const item of metaItems(review)) {
      if (item.stat) {
        dom.meta.append(metaStat(item));
        dom.sheetMeta.append(metaStat(item));
      } else {
        dom.notes.append(textEl("dt", "", item.label), textEl("dd", "", item.value));
        dom.sheetNotes.append(textEl("dt", "", item.label), textEl("dd", "", item.value));
      }
    }
  }
  dom.btnNotes.hidden = dom.notes.childElementCount === 0;
  renderUnitSwitch();
  renderProgress();
  dom.commentCount.textContent = String(state.allComments.length);
  dom.btnComments.setAttribute("aria-label", `Comment list (${state.allComments.length})`);
  dom.btnUnified.setAttribute("aria-pressed", String(displayMode() === "unified"));
  dom.btnSplit.setAttribute("aria-pressed", String(displayMode() === "split"));
  dom.btnWrap.setAttribute("aria-pressed", String(displayWrap()));
  dom.menuWrap.setAttribute("aria-pressed", String(displayWrap()));
  dom.chipFocus.setAttribute("aria-pressed", String(state.focusOnly));
  dom.menuFocus.setAttribute("aria-pressed", String(state.focusOnly));
  dom.chipSort.setAttribute("aria-pressed", String(state.sortBySize));
  dom.menuSort.setAttribute("aria-pressed", String(state.sortBySize));
  dom.menuComments.setAttribute("aria-pressed", String(state.narrowComments));
}

/**
 * @param {{ label: string, value: string }} item
 * @returns {HTMLElement}
 */
function metaStat(item) {
  const span = el("span");
  span.append(textEl("b", "", item.label), document.createTextNode(item.value));
  return span;
}

/** コミット範囲だけに出す「最終形 | コミットごと」の切り替え（R-UNIT）。 */
export function renderUnitSwitch() {
  const focusKey = focusKeyWithin(dom.unitSwitch);
  dom.unitSwitch.textContent = "";
  dom.unitSwitch.hidden = state.units.length === 0;
  for (const status of unitSwitchOrder(state.units)) {
    const unit = String(status.unit);
    const item = button("unit-button");
    const pending = state.pendingUnit && state.pendingUnit.unit === unit;
    let label = unitLabel(unit);
    if (status.state === "failed") {
      label = `${label} (failed)`;
      item.classList.add("failed");
      item.title = `could not create: ${status.error || ""} (click to see the reason and retry)`;
    } else if (status.state === "building" && pending) {
      label = `${label} (loading…)`;
    }
    item.textContent = label;
    item.dataset.focusKey = `unit:${unit}`;
    item.setAttribute("aria-pressed", String(unit === state.unit));
    item.addEventListener("click", () => actions.switchUnit(unit, null));
    dom.unitSwitch.append(item);
  }
  restoreFocusKey(dom.unitSwitch, focusKey);
}

/** 上部の、表示中のグループ単位の見たの進捗（R-SEEN）。 */
export function renderProgress() {
  const progress = seenProgress(state.entries.map((entry) => entry.file));
  dom.progress.hidden = !state.review || progress.total === 0;
  dom.progressBar.style.width = `${progress.total ? (progress.seen / progress.total) * 100 : 0}%`;
  dom.progressText.textContent = `Seen ${progress.seen} / ${progress.total}`;
  fitProgress();
}

/**
 * 狭い画面の 2 段目に進捗と送信のボタンが入りきらないときは、進捗を数字だけにする
 * （R-NARROW）。幅を測らず、バー付きで並べてみて送信のボタンが下の段へ折れたかで決める。
 * 広い画面の上部バーは折れないので、ここで印が付くことは無い。
 */
export function fitProgress() {
  delete dom.progress.dataset.compact;
  if (dom.progress.hidden) {
    return;
  }
  const progress = dom.progress.getBoundingClientRect();
  const submit = dom.submitChanges.getBoundingClientRect();
  if (submit.top >= progress.bottom) {
    dom.progress.dataset.compact = "";
  }
}

export function renderUpdateBadge() {
  dom.updateBadge.hidden = !state.updateAvailable;
}

/** 送信後は承認と変更要求のボタンを押せなくする（R-SUBMIT）。 */
export function renderSubmitButtons() {
  dom.submitApproved.disabled = state.submitted;
  dom.submitChanges.disabled = state.submitted;
}

/** 開いた meta の箱を、題の左端に揃えて題のブロックの下へ置く。 */
export function placeNotes() {
  const anchor = dom.titleBlock.getBoundingClientRect();
  dom.notes.style.top = `${Math.round(anchor.bottom + 8)}px`;
  dom.notes.style.left = `${Math.round(anchor.left)}px`;
}
