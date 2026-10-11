// shoryo 46c6f764c1d373365a8902ffcc682245079363e6。MIT: ./LICENSE
// @ts-check
const NS = "http://www.w3.org/2000/svg";
/** @template {keyof SVGElementTagNameMap} T @param {T} tag @param {Record<string,string|number|boolean>} [attrs] @param {...(SVGElement|string|(SVGElement|string)[])} children @returns {SVGElementTagNameMap[T]} */
export function svgElement(tag, attrs = {}, ...children) {
  const node = document.createElementNS(NS, tag);
  for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, String(value));
  for (const child of children.flat()) node.append(typeof child === "string" ? document.createTextNode(child) : child);
  return node;
}
/** @param {SVGSVGElement} svg @returns {import('./types.js').Metrics} */
export function textMetrics(svg) {
  const probe = svgElement("text", { "xml:space": "preserve", "dominant-baseline": "text-before-edge" });
  svg.append(probe);
  const size = Number.parseFloat(getComputedStyle(probe).fontSize);
  if (!Number.isFinite(size) || size <= 0) throw new Error("Text measurement unavailable");
  const scale = size / 14;
  /** @type {Map<string,number>} */
  const cache = new Map();
  /** @param {string} value */
  function width(value) {
    if (!cache.has(value)) {
      probe.textContent = value;
      const measured = probe.getComputedTextLength();
      if (!Number.isFinite(measured) || (value.trim() && measured <= 0)) throw new Error("Text measurement unavailable");
      cache.set(value, measured);
    }
    return cache.get(value) ?? 0;
  }
  /** @param {string} value @param {number} limit */
  function wrap(value, limit) {
    const lines = [];
    for (const run of value.split("\n")) {
      let rest = [...run];
      if (!rest.length) lines.push("");
      while (rest.length) {
        let lo = 0, hi = rest.length;
        while (lo < hi) {
          const mid = Math.ceil((lo + hi) / 2);
          if (width(rest.slice(0, mid).join("")) <= limit) lo = mid;
          else hi = mid - 1;
        }
        if (lo === 0) throw new Error("A character cannot fit its text region");
        let count = lo;
        if (lo < rest.length) {
          for (let i = lo - 1; i >= 0; i--) if (/\s/u.test(rest[i])) { count = i + 1; break; }
        }
        lines.push(rest.slice(0, count).join(""));
        rest = rest.slice(count);
      }
    }
    return { lines, width: Math.max(0, ...lines.map(width)), height: lines.length * 18 * scale, original: value };
  }
  return { scale, width, wrap, dispose: () => probe.remove() };
}
/** @param {SVGSVGElement} svg @param {import('./types.js').Metrics} metrics @returns {import('./types.js').Drawing} */
export function drawing(svg, metrics) {
  /** @type {import('./types.js').Shape[]} */ const shapes = [];
  /** @type {import('./types.js').Label[]} */ const labels = [];
  /** @type {import('./types.js').Arrow[]} */ const arrows = [];
  /** @type {import('./types.js').Box[]} */ const frames = [];
  /** @type {import('./types.js').Drawing['text']} */
  function text(block, x, y, attrs = {}, owner = null) {
    const element = svgElement("text", { x, y, "dominant-baseline": "text-before-edge", "xml:space": "preserve", "data-original": block.original, ...attrs }, block.lines.map((line, index) => svgElement("tspan", { x, y: y + index * 18 * metrics.scale }, line)));
    svg.append(element); labels.push({ element, block, x, y, owner }); return element;
  }
  /** @type {import('./types.js').Drawing['arrow']} */
  function arrow(points, attrs = {}, endpoints = []) {
    const path = svgElement("path", { d: points.map((p, i) => `${i ? "L" : "M"} ${p.x} ${p.y}`).join(" "), fill: "none", "stroke-width": 1.5, ...attrs });
    svg.append(path);
    const a = points.at(-2), b = points.at(-1);
    if (!a || !b) throw new Error('Arrow requires two points');
    const length = Math.hypot(b.x - a.x, b.y - a.y);
    if (!length) throw new Error("Arrow has no visible final segment");
    const ux = (b.x - a.x) / length, uy = (b.y - a.y) / length;
    const head = [{ x: b.x, y: b.y }, { x: b.x - 8 * ux + 3 * uy, y: b.y - 8 * uy - 3 * ux }, { x: b.x - 8 * ux - 3 * uy, y: b.y - 8 * uy + 3 * ux }];
    svg.append(svgElement("polygon", { points: head.map(p => `${p.x},${p.y}`).join(" "), class: "explanation-arrowhead" }));
    arrows.push({ points, head, endpoints });
  }
  /** @type {import('./types.js').Drawing['shape']} */
  function shape(value, element) { shapes.push(value); svg.append(element); }
  /** @type {import('./types.js').Drawing['finish']} */
  function finish(canvas) {
    metrics.dispose();
    const bounds = svg.getBBox();
    const width = canvas?.width ?? Math.max(64, Math.ceil(bounds.x + bounds.width + 24));
    const height = canvas?.height ?? Math.max(64, Math.ceil(bounds.y + bounds.height + 24));
    svg.setAttribute("width", String(width)); svg.setAttribute("height", String(height)); svg.setAttribute("viewBox", `0 0 ${width} ${height}`);
    return { svg, shapes, labels, arrows, frames, width, height };
  }
  return { svg, metrics, shapes, labels, arrows, frames, text, arrow, shape, finish };
}
