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

export function getReview(refresh = false) {
  return getJson(`api/review${refresh ? "?refresh=1" : ""}`);
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
 * @param {() => void} onUpdate
 * @returns {EventSource}
 */
export function subscribeEvents(onUpdate) {
  const events = new EventSource("api/events");
  events.addEventListener("update", onUpdate);
  return events;
}
