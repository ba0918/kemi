// @ts-check
// 内部 API の薄いラッパ（D7 の形はサーバが決める）。

/**
 * @param {string} path
 * @returns {Promise<any>}
 */
async function getJson(path) {
  const response = await fetch(path, { headers: { Accept: "application/json" } });
  if (!response.ok) {
    let message = `${path}: HTTP ${response.status}`;
    try {
      const payload = await response.json();
      if (payload && typeof payload.error === "string") {
        message = payload.error;
      }
    } catch {
      // JSON でないエラーは status のまま
    }
    throw new Error(message);
  }
  return response.json();
}

/**
 * @param {string} path
 * @param {unknown} body
 * @returns {Promise<any>}
 */
async function postJson(path, body) {
  const response = await fetch(path, {
    method: "POST",
    headers: { "Content-Type": "application/json", Accept: "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    let message = `HTTP ${response.status}`;
    try {
      const payload = await response.json();
      if (payload && typeof payload.error === "string") {
        message = payload.error;
      }
    } catch {
      // JSON でないエラーは status のまま
    }
    throw new Error(message);
  }
  return response.json();
}

/**
 * @param {boolean} [refresh]
 * @param {string | null} [unit] コミット範囲のグループ単位（`file` / `commit`）。省略時は起動時の単位。
 */
export function getReview(refresh = false, unit = null) {
  const params = new URLSearchParams();
  if (refresh) {
    params.set("refresh", "1");
  }
  if (unit) {
    params.set("unit", unit);
  }
  const query = params.toString();
  return getJson(`api/review${query ? `?${query}` : ""}`);
}

/**
 * 作成に失敗したグループ単位を作り直す。
 * @param {string} unit
 */
export function retryUnit(unit) {
  return postJson("api/unit", { op: "retry", unit });
}

/**
 * @param {string} id
 * @param {{from?: number, to?: number}|null} [range]
 * @param {{dark?: boolean, highlight?: string}} [options]
 */
export function getFile(id, range, options = {}) {
  const params = new URLSearchParams();
  if (range && range.from !== undefined && range.to !== undefined) {
    params.set("from", String(range.from));
    params.set("to", String(range.to));
  }
  if (options.dark) {
    params.set("dark", "1");
  }
  if (options.highlight) {
    params.set("highlight", options.highlight);
  }
  const query = params.toString();
  return getJson(`api/file/${encodeURIComponent(id)}${query ? `?${query}` : ""}`);
}

/**
 * 描画表示の HTML とブロックの一覧（R-RENDER）。表示時に初めて計算される。
 * @param {string} id
 * @param {{dark?: boolean, highlight?: string}} [options]
 */
export function getRender(id, options = {}) {
  const params = new URLSearchParams();
  if (options.dark) {
    params.set("dark", "1");
  }
  if (options.highlight) {
    params.set("highlight", options.highlight);
  }
  const query = params.toString();
  return getJson(`api/render/${encodeURIComponent(id)}${query ? `?${query}` : ""}`);
}

/**
 * 最終形のファイルの由来。`force` で上限を超えるファイルでも求める。
 * @param {string} id
 * @param {boolean} force
 */
export function getOrigin(id, force) {
  return getJson(`api/origin/${encodeURIComponent(id)}${force ? "?force=1" : ""}`);
}

/**
 * @param {{file_id: string, seen?: boolean, collapsed?: boolean}} body
 */
export function postState(body) {
  return postJson("api/state", body);
}

/**
 * @param {object} body
 */
export function postComment(body) {
  return postJson("api/comment", body);
}

/**
 * レビュー全体への発言を書く。
 * @param {string} body
 */
export function postMessage(body) {
  return postJson("api/message", { body });
}

/** 前に渡した後の変化を、まとめてエージェントに渡す（R-AGENT-HAND）。 */
export function hand() {
  return postJson("api/hand", {});
}

/**
 * @param {"approved" | "changes_requested"} verdict
 */
export function submit(verdict) {
  return postJson("api/submit", { verdict });
}

/**
 * @typedef {{
 *   onThread: (comment: any) => void,
 *   onMessage: (message: any) => void,
 *   onAgent: (agent: any) => void,
 *   onMissed: () => void,
 * }} AgentHandlers
 */

/**
 * @param {() => void} onUpdate 新側の供給元が変わった（更新バッジ）
 * @param {() => void} onUnit もう片方のグループ単位の作成の状態が変わった
 * @param {AgentHandlers} agent 返信・発言・エージェントの状態の通知と、届かなかった通知の取り直し
 * @returns {Promise<void>} 最初につながった（サーバが通知を送り始めた）とき、またはつながらなかったときに解決する
 */
export function subscribeEvents(onUpdate, onUnit, agent) {
  const events = new EventSource("api/events");
  events.addEventListener("update", onUpdate);
  events.addEventListener("unit", onUnit);
  /**
   * @param {string} name
   * @param {(payload: any) => void} handler
   */
  const json = (name, handler) =>
    events.addEventListener(name, (event) => {
      handler(JSON.parse(/** @type {MessageEvent} */ (event).data));
    });
  json("thread", agent.onThread);
  json("message", agent.onMessage);
  json("agent", agent.onAgent);
  // どの通知を取りこぼしたかは分からない。更新があったものとして扱い、往復の中身も取り直す。
  events.addEventListener("lagged", () => {
    onUpdate();
    agent.onMissed();
  });
  // つながる前（や切れていた間）に届かなかった単位の状態を、つながった時点で読み直す。
  // 返信・発言・エージェントの状態は、最初の接続では読み直さない（起動はつながってから
  // 中身を読むので、取りこぼしは無い。読み直すとコメントをすべて差し替える）。つなぎ直した
  // ときだけ読み直す。
  let connected = false;
  events.addEventListener("open", () => {
    onUnit();
    if (connected) {
      agent.onMissed();
    }
    connected = true;
  });
  // サーバは通知の購読を始めてから応答のヘッダを返すので、open の後に起きたことは届く。
  return new Promise((resolve) => {
    events.addEventListener("open", () => resolve(), { once: true });
    events.addEventListener("error", () => resolve(), { once: true });
  });
}
