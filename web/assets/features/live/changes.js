// @ts-check
// `--live` の変化の一覧と印（live.md の R-PAGE-DIFF・R-PAGE-VIEW）: 比べる相手のスナップショットと動いているページの
// 記述を比べ、ページのツリーの下に一覧を出し、見る対象の側と比べる相手の側（消えた要素）に印を付ける。

import { changesByElement, diffDescriptions, marksOf, sameChanges, unpackDescription } from "../../live-diff.js";
import { buildPageTree, pageKey, referenceName } from "../../live-model.js";
import { state } from "../../state.js";
import { renderChanges, renderPageTree } from "../../views/live-changes.js";
import { ask } from "./channel.js";
import { openMockMenu } from "./mock.js";
import { loadSnapshot, markRemovedInSnapshot, recordCurrent } from "./snapshots.js";
import { layoutFrames, liveFrameHidden, showChangedElement, showRemovedElement } from "./stage.js";
import { currentReference, live, liveActions, pageComments, shell } from "./state.js";

/**
 * @typedef {import("./state.js").Description} Description
 * @typedef {import("./state.js").Change} Change
 */

/** ページが記述を返すまで待つ上限。 */
const DESCRIBE_TIMEOUT = 5000;

/**
 * 変化の一覧を今に合わせる（R-PAGE-DIFF）。比べる相手がスナップショットのときだけ比べる。
 * 動いているページの記述が古い（ページが変わった、移った、表示幅が違う）ときは頼み直し、
 * 届いてから比べる。どちらの記述も変わっていなければ計算し直さない。
 */
export function refreshChanges() {
  const reference = currentReference();
  if (reference.type !== "snapshot" || !live.reachable) {
    setChanges(null, null);
    return;
  }
  const id = reference.snapshot.id;
  // 比べる相手が別のスナップショットに変わったら、前のものと比べた一覧は新しい結果が出るまで出さない。
  if (live.changesFrom !== null && live.changesFrom.snapshot !== id) {
    setChanges(null, null);
  }
  if (live.unloadable.has(id)) {
    setChanges(null, null, "Not compared: the snapshot could not be loaded");
    return;
  }
  if (!live.descriptions.has(id)) {
    void loadSnapshot(id).then(
      () => refreshChanges(),
      () => {
        live.unloadable.add(id);
        refreshChanges();
      },
    );
    return;
  }
  const before = live.descriptions.get(id) ?? null;
  if (before === null) {
    setChanges(null, null, "Not compared: the snapshot has no element description");
    return;
  }
  const now = live.now;
  if (now === null || live.describedAt !== live.pageChanges) {
    // 返らなかった記述は描くたびには頼み直さない（大きなページでは、記述を作るたびに全要素をたどる）。
    if (live.describeFailure !== "") {
      setChanges(null, null, live.describeFailure);
      return;
    }
    // 隠れている間は頼まず、今の一覧を残す。また見えるようにしたときの描き直しか、ページの変化の知らせで頼む。
    if (liveFrameHidden()) {
      return;
    }
    void describeNow();
    return;
  }
  // 移る途中や表示幅を変えた直後の記述。ページが落ち着くと知らせが来るので、それまで待つ
  // （ここで頼み直すと、移り終えるまで頼みごとが続く）。
  if (now.page !== live.page || now.description.width !== live.width) {
    setChanges(null, null);
    return;
  }
  if (live.changesFrom?.snapshot === id && live.changesFrom.now === now.description) {
    return;
  }
  const from = { snapshot: id, now: now.description };
  let changes;
  try {
    changes = diffDescriptions(before, now.description);
  } catch {
    // 形の崩れた記述は比べない。一覧の代わりにそのことを出す。
    setChanges(null, from, "Not compared: the element descriptions could not be read");
    return;
  }
  setChanges(changes, from);
}

/** 記述やスナップショットを得られなかったことを忘れ、次に描くときに頼み直させる。 */
export function forgetFailures() {
  live.describeFailure = "";
  live.unloadable.clear();
}

/**
 * @param {Change[] | null} changes
 * @param {{ snapshot: string, now: Description } | null} from
 * @param {string} [notice] 比べられないとき、一覧の代わりに出す知らせ
 */
export function setChanges(changes, from, notice = "") {
  const same = sameChanges(changes, live.changes) && notice === live.changesNotice;
  live.changesFrom = from;
  if (same && changes === null) {
    return;
  }
  // 結果が前と同じなら一覧は描き直さない。印は送り直す: ページは記述を作り直すたびに要素を数え直すので、
  // 要素が作り直されていれば同じ番号でも別の要素を指す（同じ要素のままなら、ページは印を作り直さない）。
  if (!same) {
    live.changes = changes;
    live.changesNotice = notice;
    if (shell) {
      renderChanges(shell.pageTree, changeHandlers, changesToList());
    }
  }
  renderMarks();
}

/**
 * 変わったところに印を付ける（R-PAGE-VIEW）。今のページにある変化は動いているページの側に、
 * 差し込んだスクリプトが付ける。消えた要素は比べる相手の側に、スナップショットの中身に足して付ける。
 * 印を隠しているときは、どちらの側の印も外す。
 */
export function renderMarks() {
  if (!shell) {
    return;
  }
  const marks = live.changes === null || !live.marks ? { now: [], before: [] } : marksOf(live.changes);
  shell.liveFrame.contentWindow?.postMessage({ kemi: "live", type: "marks", marks: marks.now }, live.origin);
  if (live.shownSnapshot !== "") {
    void showSnapshot(live.shownSnapshot);
  }
}

/**
 * 比べる相手の枠に出すスナップショットの、印を付ける消えた要素の番号。今の変化の一覧がそのスナップショットと
 * 比べたものでなければ無し（前に比べた別のスナップショットの印を付けない）。
 * @param {string} id
 * @returns {number[]}
 */
function removedMarksOf(id) {
  if (live.changes === null || !live.marks || live.changesFrom?.snapshot !== id) {
    return [];
  }
  return marksOf(live.changes)
    .before.map((mark) => mark.index)
    .sort((a, b) => a - b);
}

/**
 * 動いているページに今の記述を頼む。返事を待つ間に頼み直しはしない。ページが読み込まれ直したら
 * 待つのをやめる（読み込み直す前の文書に頼んだものは返らないことがある）。返らない・読めないときは
 * 比べられないことを出し、次のきっかけまで頼み直さない。
 */
async function describeNow() {
  const frame = shell?.liveFrame.contentWindow;
  if (!frame || live.describing) {
    return;
  }
  live.describing = true;
  const asked = live.pageChanges;
  const loads = live.pageLoads;
  const answer = await ask(frame, "describe", DESCRIBE_TIMEOUT);
  if (loads !== live.pageLoads) {
    return;
  }
  live.describing = false;
  // 待つ間に隠れた枠の記述は使わない。また見えるようにしたときに頼み直す。
  if (liveFrameHidden()) {
    return;
  }
  // 動いているページの記述は postMessage で届くので縮めない（縮めて戻す手間のほうが大きい）。
  const description = unpackDescription(answer.description);
  if (description === null) {
    live.now = null;
    live.describeFailure = `Not compared: ${answer.error ?? "the page did not describe its elements"}`;
    refreshChanges();
    return;
  }
  live.now = { page: pageKey(String(answer.path ?? "/")), description };
  live.describedAt = asked;
  refreshChanges();
}

/**
 * 変化の一覧の操作。開いたずれただけと並べた数を、描き直しても保つために覚える。行を押したらその要素を見せる。
 * @type {import("../../views/live-changes.js").ChangeHandlers}
 */
const changeHandlers = {
  onShifted: (open) => {
    live.shiftedOpen = open;
  },
  onListed: (group, count) => {
    live.listed[group] = count;
  },
  onShow: (element) => {
    if (element.side === "now") {
      void showChangedElement(element.index);
    } else {
      showRemovedElement(element.index);
    }
  },
  onMockMenu: (anchor) => openMockMenu(anchor),
  onRecord: () => void recordCurrent(),
};

/**
 * ページのツリーに渡す、表示中のページの変化の一覧。比べられなければその知らせ。モックと比べるときは、一覧の代わりに
 * モックの名前の見出し（R-PAGE-VIEW）。
 * @returns {import("../../views/live-changes.js").ChangeListState | import("../../views/live-changes.js").ChangeNotice | import("../../views/live-changes.js").ChangeMock | null}
 */
function changesToList() {
  const shown = currentReference();
  if (shown.type === "mock") {
    return { mock: referenceName(live.snapshots, shown) };
  }
  if (live.changes === null) {
    return live.changesNotice === "" ? null : { notice: live.changesNotice };
  }
  const snapshot = live.changesFrom?.snapshot ?? "";
  const unmarked = live.mapped.get(snapshot) === false && live.changes.some((change) => change.kind === "removed");
  // 見出しには、見比べ方によらず比べている実物の名前を出す（自動のときも、自動が選んだ実物。R-PAGE-VIEW）。
  const compared = live.snapshots.find((item) => item.id === snapshot);
  const reference = compared ? referenceName(live.snapshots, { type: "snapshot", snapshot: compared }) : "";
  return {
    list: changesByElement(live.changes), reference, shiftedOpen: live.shiftedOpen, listed: live.listed, unmarked,
    fromStart: compared?.kind === "start", canRecord: live.reachable && !live.recording, recording: live.recording,
  };
}

export function renderTree() {
  if (!shell) {
    return;
  }
  renderPageTree(
    shell.pageTree,
    buildPageTree({
      current: live.page,
      snapshots: live.snapshots,
      mocks: new Set(live.mocks.keys()),
      comments: pageComments().map((comment) => ({ page: comment.page.url, width: comment.page.width })),
    }),
    {
      onPage: liveActions.openPage,
      onWidth: (page, width) => {
        liveActions.setWidth(width);
        liveActions.openPage(page);
      },
      ...changeHandlers,
    },
    changesToList(),
    state.liveSaving,
  );
}

/**
 * スナップショットの中身を、スクリプトを止めた枠に出す。消えた要素の印は中身に足して描く。
 * 枠を作り直すとその中のスクロールは先頭に戻るので、作り直すのは出すスナップショットか、印を付ける
 * 消えた要素の組が変わったときだけにする（動いているページの側の印だけが変わったときは作り直さない）。
 * @param {string} id
 */
export async function showSnapshot(id) {
  if (!shell) {
    return;
  }
  const html = await loadSnapshot(id);
  const removed = removedMarksOf(id);
  const key = `${id} ${removed.join(",")}`;
  if (live.shownSnapshot !== id || live.shownFrame === key) {
    return;
  }
  live.shownFrame = key;
  // 同じ中身の srcdoc を入れ直しても読み込み直されないことがあるので、枠ごと作り直す。印が無くても通すのは、
  // ページが添えた要素の対応の <meta> を外すため（残すと head の先頭の子になり、head を前提にしたセレクタが変わる）。
  const frame = /** @type {HTMLIFrameElement} */ (shell.refFrame.cloneNode(false));
  const marked = markRemovedInSnapshot(html, removed);
  frame.srcdoc = marked.html;
  shell.refFrame.replaceWith(frame);
  shell.refFrame = frame;
  layoutFrames();
  if (live.mapped.get(id) !== marked.mapped) {
    // 一覧は対応の有無が分かる前に描いていることがあるので、印を付けられないことを出し直す。
    live.mapped.set(id, marked.mapped);
    renderChanges(shell.pageTree, changeHandlers, changesToList());
  }
}
