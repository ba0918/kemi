// @ts-check
// 内部 API の薄いラッパ（D7 の形はサーバが決める）。

/**
 * @param {string} path
 * @returns {Promise<any>}
 */
async function getJson(path) {
  const response = await fetch(path, { headers: { Accept: "application/json" } });
  if (!response.ok) {
    throw new Error(`${path}: HTTP ${response.status}`);
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
 * @param {"approved" | "changes_requested"} verdict
 */
export function submit(verdict) {
  return postJson("api/submit", { verdict });
}

/**
 * @param {() => void} onUpdate 新側の供給元が変わった（更新バッジ）
 * @param {() => void} onUnit もう片方のグループ単位の作成の状態が変わった
 * @returns {EventSource}
 */
export function subscribeEvents(onUpdate, onUnit) {
  const events = new EventSource("api/events");
  events.addEventListener("update", onUpdate);
  events.addEventListener("unit", onUnit);
  // つながる前（や切れていた間）に届かなかった状態の変化を、つながった時点で読み直す。
  events.addEventListener("open", onUnit);
  return events;
}
