// @ts-check
// `--live` のスナップショット（live.md の R-PAGE-SNAPSHOT、R-PAGE-SESSION）: 開始時・渡す前・手で取るときに動いている
// ページの写しと要素の記述を取って預け、預けたものの一覧と中身を読む。比べる相手の側に出す写しには、消えた要素の
// 印を足す。

import * as api from "../../api.js";
import { markedPositions, unpackDescription } from "../../live-diff.js";
import { pageKey, startSnapshotDue } from "../../live-model.js";
import { ask } from "./channel.js";
import { whileLaidOut } from "./stage.js";
import { live, liveActions, shell } from "./state.js";

/**
 * @typedef {import("./state.js").Description} Description
 */

/** スナップショット 1 つの上限（R-PAGE-SNAPSHOT）。 */
const SNAPSHOT_LIMIT = 2 * 1024 * 1024;

/** ページが写しを返すまで待つ上限。 */
const CAPTURE_TIMEOUT = 15000;

/** 預けたスナップショットの一覧を読み直す。 */
export async function reloadSnapshots() {
  const list = await api.listSnapshots().catch(() => null);
  if (list === null) {
    return;
  }
  live.snapshots = list.snapshots ?? [];
  // 一覧から消えたもの（20 MB の規則で消したもの。R-PAGE-SESSION）の中身・記述・対応の有無は手放す。比べる相手には
  // 一覧のものしか選ばれず、二度と読まれないのに、画面を開いている間ずっと残る（中身は 1 つ 2 MB まで）。選んだ時点
  // （chosen）は残す: 選んだ時点が消えたら、記録されていないと出す。
  const listed = new Set(live.snapshots.map((snapshot) => snapshot.id));
  for (const held of [live.bodies, live.descriptions, live.mapped]) {
    for (const id of held.keys()) {
      if (!listed.has(id)) {
        held.delete(id);
      }
    }
  }
  liveActions.render();
}

/** 開始時のスナップショット。開始時につながらなければ、最初につながったとき（R-PAGE-SNAPSHOT）。 */
export function takeStartSnapshot() {
  const snapshots = live.snapshotsListed ? live.snapshots : null;
  if (!startSnapshotDue({ taken: live.startTaken, reachable: live.reachable, snapshots })) {
    return;
  }
  live.startTaken = true;
  void capture("start");
}

/**
 * エージェントに渡す前に、渡す画面で表示中のページのスナップショットを取る
 * （R-PAGE-SNAPSHOT）。渡した後に取ると、渡されたエージェントがもうページを変えていることが
 * ある。取れなくても渡すのは止めない（取れなかったことは画面に出る）。
 */
export async function captureBeforeHand() {
  await capture("handed");
}

/**
 * 表示中のページのスナップショットを取り、預ける。2 MB を超えるものは取らず、そのことを
 * 出す。比べる相手は 1 つ前のまま（R-PAGE-SNAPSHOT）。
 * @param {"start" | "handed" | "manual"} kind
 */
async function capture(kind) {
  const frame = shell?.liveFrame.contentWindow;
  if (!frame || !live.reachable) {
    // 開始時のものは、つながったときに取り直す。
    if (kind !== "start") {
      live.refNotice = "Not recorded: the page is not loaded";
      liveActions.render();
    }
    return;
  }
  const page = live.page;
  const width = live.width;
  if (kind === "manual") {
    live.recorded = null;
  }
  const answer = await whileLaidOut(() => ask(frame, "capture", CAPTURE_TIMEOUT));
  if (typeof answer.html !== "string") {
    live.refNotice = `Not recorded: ${answer.error ?? "the page could not be copied"}`;
    liveActions.render();
    return;
  }
  if (new Blob([answer.html]).size > SNAPSHOT_LIMIT) {
    live.refNotice = "Not recorded: the snapshot is larger than 2 MB";
    liveActions.render();
    return;
  }
  const description = await readUploadedDescription(answer.description);
  try {
    const takenPage = pageKey(String(answer.path ?? page));
    const taken = await api.takeSnapshot({
      page: takenPage,
      width,
      kind,
      html: answer.html,
      description: description === null ? null : answer.description,
    });
    // 取ったことで古いものが消えると、一覧の読み直しが先に届いて、もう入っていることがある。
    if (!live.snapshots.some((snapshot) => snapshot.id === taken.id)) {
      live.snapshots.push(taken);
    }
    live.bodies.set(taken.id, answer.html);
    live.descriptions.set(taken.id, description);
    live.refNotice = "";
    // 手で取ったら、そのページの比べる相手を取ったものに切り替え、取れたことを知らせる。見比べ方は変えない（R-PAGE-REF）。
    if (kind === "manual") {
      live.chosen.set(takenPage, taken.id);
      live.recorded = { page: takenPage, id: taken.id };
      // 取り消しを断った知らせは帯で取れたことの知らせより先に出るので、新しい知らせで置き換える。
      if (live.undoMockFailed?.page === takenPage) {
        live.undoMockFailed = null;
      }
    }
  } catch (error) {
    live.refNotice = `Not recorded: ${error instanceof Error ? error.message : String(error)}`;
  }
  liveActions.render();
}

export async function recordCurrent() {
  if (!live.reachable || live.recording) {
    return;
  }
  const fromGuide = document.activeElement instanceof HTMLElement && document.activeElement.matches(".lv-use-current");
  live.recording = true;
  try {
    liveActions.render();
    await capture("manual");
  } finally {
    const restoreFocus = fromGuide && document.activeElement === document.body;
    live.recording = false;
    liveActions.render();
    if (restoreFocus && shell) {
      const target = shell.pageTree.querySelector(".lv-use-current")
        ?? shell.pageTree.querySelector('.lv-page[data-current="true"] button');
      if (target instanceof HTMLElement) {
        target.focus();
      }
    }
  }
}

/**
 * スナップショットと一緒に預けた記述（gzip で縮めた base64 の文字列）を読む。読めなければ null（比べない）。
 * @param {unknown} value
 * @returns {Promise<Description | null>}
 */
async function readUploadedDescription(value) {
  if (typeof value !== "string" || value === "") {
    return null;
  }
  try {
    const binary = atob(value);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index++) {
      bytes[index] = binary.charCodeAt(index);
    }
    const text = await new Response(new Blob([bytes]).stream().pipeThrough(new DecompressionStream("gzip"))).text();
    return unpackDescription(JSON.parse(text));
  } catch {
    return null;
  }
}

/** 取り寄せ中のスナップショット（id → 取り寄せ）。同じものを 2 度取りに行かない。 */
/** @type {Map<string, Promise<string>>} */
const loadingSnapshots = new Map();

/**
 * スナップショットの中身と記述。手元に無ければ取り寄せて覚える。中身を返す。
 * @param {string} id
 * @returns {Promise<string>}
 */
export function loadSnapshot(id) {
  const html = live.bodies.get(id);
  if (html !== undefined && live.descriptions.has(id)) {
    return Promise.resolve(html);
  }
  let loading = loadingSnapshots.get(id);
  if (!loading) {
    loading = api.getSnapshot(id).then(async (snapshot) => {
      const body = String(snapshot.html ?? "");
      const description = await readUploadedDescription(snapshot.description);
      live.bodies.set(id, body);
      live.descriptions.set(id, description);
      return body;
    });
    loading.finally(() => loadingSnapshots.delete(id)).catch(() => {});
    loadingSnapshots.set(id, loading);
  }
  return loading;
}

/**
 * 消えた要素の印。スナップショットの枠の中は触れないので、写しの HTML に属性と <style> を足して描く。色と太さは
 * web/live/page.js の MARK_BORDERS の main と同じ値（変えるときは両方）。
 */
const REMOVED_MARK_STYLE = "[data-kemi-removed] { outline: 2px solid rgb(214, 69, 69) !important; outline-offset: -2px !important; }";

/** 記述の番号と HTML を読み直した文書での要素の位置の対応を入れる <meta> の name（web/live/page.js の ELEMENT_MAP）。 */
const ELEMENT_MAP = "kemi-elements";

/**
 * 比べる相手の側の印（消えた要素）を付けたスナップショットの HTML。HTML を読み直すと要素の並びが DOM と
 * 変わることがあるので、ページがスナップショットに添えた対応（記述の番号ごとの、読み直した文書での要素の
 * 位置。web/live/page.js の elementMap）で要素を探し、属性を付ける。位置は対応の <meta> を外してから、
 * 要素を文書の順に（<template> は中身を）数える。shadow root の中には文書の <style> が効かないので、
 * 印を付けた shadow root ごとにも <style> を足す。`mapped` は、ページが対応を添えていたか（無ければ印は付けられない）。
 * @param {string} html
 * @param {number[]} indices 印を付ける要素の記述の番号
 * @returns {{ html: string, mapped: boolean }}
 */
export function markRemovedInSnapshot(html, indices) {
  const parsed = new DOMParser().parseFromString(html, "text/html");
  const map = parsed.head.querySelector(`meta[name="${ELEMENT_MAP}"]`);
  const wanted = markedPositions(map?.getAttribute("content") ?? "", indices);
  map?.remove();
  /** @type {Element[]} */
  const marked = [];
  let position = 0;
  /** @param {Element} element */
  const visit = (element) => {
    if (wanted.has(position)) {
      marked.push(element);
    }
    position += 1;
    const children = element instanceof HTMLTemplateElement ? element.content.children : element.children;
    for (const child of [...children]) {
      visit(child);
    }
  };
  visit(parsed.documentElement);
  /** @type {Set<Node>} */
  const styled = new Set();
  for (const element of marked) {
    element.setAttribute("data-kemi-removed", "");
    const root = element.getRootNode();
    if (!styled.has(root)) {
      styled.add(root);
      const style = parsed.createElement("style");
      style.textContent = REMOVED_MARK_STYLE;
      (root instanceof DocumentFragment ? root : parsed.head).append(style);
    }
  }
  const doctype = parsed.doctype ? `<!doctype ${parsed.doctype.name}>` : "";
  return { html: doctype + parsed.documentElement.outerHTML, mapped: map !== null };
}
