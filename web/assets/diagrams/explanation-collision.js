// shoryo 46c6f764c1d373365a8902ffcc682245079363e6。MIT: ./LICENSE
// @ts-check
/** @typedef {import('./types.js').Point} Point */
/** @typedef {import('./types.js').Box} Box */
/** @typedef {import('./types.js').Shape} Shape */
/** @typedef {{polygon:Point[]}|{circle:Point & {radius:number}}} Piece */
const EPS = 0.5;
/** @param {Box} box */
export function corners(box) { return [{ x: box.x, y: box.y }, { x: box.x + box.width, y: box.y }, { x: box.x + box.width, y: box.y + box.height }, { x: box.x, y: box.y + box.height }]; }
/** @param {Shape} shape */
function polygon(shape) {
  const { x, y, width: w, height: h } = shape;
  if (shape.kind === 'decision') return [{ x, y: y - h / 2 }, { x: x + w / 2, y }, { x, y: y + h / 2 }, { x: x - w / 2, y }];
  return corners({ x: x - w / 2, y: y - h / 2, width: w, height: h });
}
/** @param {Shape} shape @returns {Piece[]} */
function pieces(shape) {
  if (shape.kind !== 'start' && shape.kind !== 'end') return [{ polygon: polygon(shape) }];
  const { x, y, width: w, height: h } = shape;
  const radius = Math.min(w, h) / 2;
  const horizontal = w >= h;
  const offset = Math.abs(w - h) / 2;
  const core = { x: x - (horizontal ? offset : radius), y: y - (horizontal ? radius : offset), width: horizontal ? offset * 2 : w, height: horizontal ? h : offset * 2 };
  if (offset === 0) return [{ circle: { x, y, radius } }];
  return [{ polygon: corners(core) }, ...[-1, 1].map(sign => ({ circle: { x: x + (horizontal ? sign * offset : 0), y: y + (horizontal ? 0 : sign * offset), radius } }))];
}
/** @param {Point} p @param {Point} a @param {Point} b */
function distanceToSegment(p, a, b) {
  const dx = b.x - a.x, dy = b.y - a.y;
  const t = Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / (dx * dx + dy * dy || 1)));
  return Math.hypot(p.x - a.x - t * dx, p.y - a.y - t * dy);
}
/** @param {Point} p @param {Point[]} poly @param {number} [margin] */
function insidePolygon(p, poly, margin = 0) {
  return poly.every((a, i) => { const b = poly[(i + 1) % poly.length]; const cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x); return cross >= margin * Math.hypot(b.x - a.x, b.y - a.y); });
}
/** @param {Point[]} a @param {Point[]} b */
function polygonOverlap(a, b) {
  for (const poly of [a, b]) for (const [i, p] of poly.entries()) {
    const q = poly[(i + 1) % poly.length]; const length = Math.hypot(q.x - p.x, q.y - p.y); if (!length) continue;
    const nx = -(q.y - p.y) / length, ny = (q.x - p.x) / length;
    const aa = a.map(v => v.x * nx + v.y * ny), bb = b.map(v => v.x * nx + v.y * ny);
    if (Math.min(Math.max(...aa), Math.max(...bb)) - Math.max(Math.min(...aa), Math.min(...bb)) <= EPS) return false;
  }
  return true;
}
/** @param {Point & {radius:number}} circle @param {Point[]} poly */
function circlePolygon(circle, poly) { if (insidePolygon(circle, poly)) return true; return poly.some((a, i) => distanceToSegment(circle, a, poly[(i + 1) % poly.length]) < circle.radius - EPS); }
/** @param {Piece} a @param {Piece} b */
function pieceOverlap(a, b) {
  if ('polygon' in a && 'polygon' in b) return polygonOverlap(a.polygon, b.polygon);
  if ('circle' in a && 'circle' in b) return Math.hypot(a.circle.x - b.circle.x, a.circle.y - b.circle.y) < a.circle.radius + b.circle.radius - EPS;
  if ('circle' in a && 'polygon' in b) return circlePolygon(a.circle, b.polygon);
  if ('polygon' in a && 'circle' in b) return circlePolygon(b.circle, a.polygon);
  return false;
}
/** @param {Shape} a @param {Shape} b */
export function shapesOverlap(a, b) { return pieces(a).some(pa => pieces(b).some(pb => pieceOverlap(pa, pb))); }
/** @param {Box} box @param {Shape} shape */
export function boxOverlapsShape(box, shape) { return pieces(shape).some(piece => pieceOverlap({ polygon: corners(box) }, piece)); }
/** @param {Shape} shape @param {Box} box */
export function containsBox(shape, box) {
  const { x, y, width: w, height: h } = shape;
  return corners(box).every(p => {
    if (shape.kind === 'start' || shape.kind === 'end') { const radius = Math.min(w, h) / 2; const dx = Math.max(0, Math.abs(p.x - x) - (w / 2 - radius)); const dy = Math.max(0, Math.abs(p.y - y) - (h / 2 - radius)); return Math.hypot(dx, dy) <= radius + EPS; }
    return insidePolygon(p, polygon(shape), -EPS);
  });
}
/**
 * 箱が形の中に収まるまで高さを 1 ずつ足す。始点・終点の丸い端は、幅いっぱいに折り返した文字の角を削るため。
 * 640 を超えても収まらなければ、超えた高さを返す（呼び出し側が上限の超過として扱う）。
 * @param {Shape} shape @param {Box[]} boxes
 */
export function fitHeight(shape, boxes) {
  let height = shape.height;
  while (height <= 640 && !boxes.every(box => containsBox({ ...shape, height }, box))) height += 1;
  return height;
}
/**
 * 辺の中点から法線の向きへ、ラベルの箱が線から gap 離れる所に中心を置く。
 * @param {Point} a @param {Point} b @param {{width:number,height:number}} size @param {number} gap
 */
export function labelBesideSegment(a, b, size, gap) {
  const length = Math.hypot(b.x - a.x, b.y - a.y) || 1;
  const nx = -(b.y - a.y) / length, ny = (b.x - a.x) / length;
  const offset = size.width / 2 * Math.abs(nx) + size.height / 2 * Math.abs(ny) + gap;
  return { x: (a.x + b.x) / 2 + nx * offset, y: (a.y + b.y) / 2 + ny * offset };
}
/** @param {Point} a @param {Point} b @param {Point[]} poly */
function segmentPolygon(a, b, poly) {
  let low = 0, high = 1;
  for (const [i, p] of poly.entries()) {
    const q = poly[(i + 1) % poly.length]; const nx = -(q.y - p.y), ny = q.x - p.x;
    const threshold = EPS * Math.hypot(nx, ny); const base = nx * (a.x - p.x) + ny * (a.y - p.y); const delta = nx * (b.x - a.x) + ny * (b.y - a.y);
    if (Math.abs(delta) < 1e-9) { if (base <= threshold) return false; }
    else if (delta > 0) low = Math.max(low, (threshold - base) / delta); else high = Math.min(high, (threshold - base) / delta);
    if (low >= high) return false;
  }
  return high > 0 && low < 1 && low < high;
}
/** @param {Point} a @param {Point} b @param {Shape} shape */
export function segmentEntersShape(a, b, shape) { return pieces(shape).some(piece => 'circle' in piece ? distanceToSegment(piece.circle, a, b) < piece.circle.radius - EPS : segmentPolygon(a, b, piece.polygon)); }
/** @param {Box} a @param {Box} b */
export function boxesOverlap(a, b) { return Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x) > EPS && Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y) > EPS; }
