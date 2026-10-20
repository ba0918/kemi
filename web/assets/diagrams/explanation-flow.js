// shoryo 46c6f764c1d373365a8902ffcc682245079363e6。文字の外形で高さを合わせる直しは shoryo f3f163a077cc5172f8428d10240d5c1b3813adeb（v0.1.3）に合わせた。MIT: ./LICENSE
// @ts-check
import { svgElement } from './explanation-svg.js';
import { fitHeight } from './explanation-collision.js';
/** @param {import('./types.js').Flow} part @param {import('./types.js').Drawing} draw @param {(key:string)=>string} t */
export function flowDrawing(part, draw, t) {
  const { metrics: m } = draw;
  const s = m.scale;
  const nodes = part.nodes.map((node, i) => {
    const width = node.width ?? (node.kind === 'decision' ? 220 : 180);
    const terminal = node.kind === 'start' || node.kind === 'end';
    const block = m.wrap(node.label, (node.kind === 'decision' ? width / 2 : width) - 24 * s);
    const marker = terminal ? m.wrap(t(`part.node.${node.kind}`), width - 24 * s) : null;
    const contentHeight = block.height + (marker ? marker.height + 8 * s : 0);
    const derived = terminal ? Math.max(64, contentHeight + 24 * s) : node.kind === 'decision' ? Math.max(96, 2 * (contentHeight + 24 * s)) : Math.max(64, contentHeight + 24 * s);
    const markerTop = -contentHeight / 2;
    const blockTop = markerTop + (marker ? marker.height + 8 * s : 0);
    // 行数 × 18 ではなく、描いた文字の外形に合わせる。検証が測るのは外形で、その高さは行の送りではなく
    // フォントの ascent と descent で決まるため。
    /** @param {import('./types.js').TextBlock} text @param {number} top */
    const drawn = (text, top) => { const b = m.box(text); return { ...b, x: b.x - text.width / 2, y: b.y + top }; };
    const boxes = [drawn(block, blockTop), ...(marker ? [drawn(marker, markerTop)] : [])];
    const height = node.height ?? fitHeight({ id: node.id, kind: node.kind, x: 0, y: 0, width, height: derived }, boxes);
    if (height > 640) throw new Error(`Node ${node.id} derived height exceeds 640`);
    return { ...node, x: node.position?.x ?? 160, y: node.position?.y ?? 100 + i * 160, width, height, block, marker, markerTop, blockTop };
  });
  const byId = new Map(nodes.map(n => [n.id, n]));
  for (const node of nodes) {
    const { x, y, width: w, height: h } = node;
    const attrs = { 'data-node': node.id, 'data-kind': node.kind };
    const element = node.kind === 'decision'
      ? svgElement('polygon', { ...attrs, points: `${x},${y - h / 2} ${x + w / 2},${y} ${x},${y + h / 2} ${x - w / 2},${y}`, class: 'explanation-node' })
      : svgElement('rect', { ...attrs, x: x - w / 2, y: y - h / 2, width: w, height: h, ...(node.marker ? { rx: Math.min(w, h) / 2 } : {}) });
    draw.shape(node, element);
    if (node.marker) draw.text(node.marker, x - node.marker.width / 2, y + node.markerTop, { 'data-terminal-marker': node.id }, node.id);
    draw.text(node.block, x - node.block.width / 2, y + node.blockTop, { 'data-node-label': node.id }, node.id);
  }
  /** @param {import('./types.js').Shape} node @param {string} name */
  function port(node, name) {
    switch (name) {
      case 'north': return { x: node.x, y: node.y - node.height / 2 };
      case 'south': return { x: node.x, y: node.y + node.height / 2 };
      case 'east': return { x: node.x + node.width / 2, y: node.y };
      case 'west': return { x: node.x - node.width / 2, y: node.y };
      default: throw new Error(`Unknown port ${name}`);
    }
  }
  for (const [i, edge] of part.edges.entries()) {
    const from = byId.get(edge.from), to = byId.get(edge.to);
    if (!from || !to) throw new Error('Unknown node');
    const a = port(from, edge.from_port ?? 'south'), b = port(to, edge.to_port ?? 'north');
    const via = edge.via ?? [{ x: a.x, y: (a.y + b.y) / 2 }, { x: b.x, y: (a.y + b.y) / 2 }];
    const points = [a, ...via, b].filter((p, j, all) => j === 0 || p.x !== all[j - 1].x || p.y !== all[j - 1].y);
    if (points.length < 2) throw new Error(`Edge ${edge.from}→${edge.to} has no visible route`);
    draw.arrow(points, { 'data-edge': i }, [from.id, to.id]);
    if (edge.label) {
      const block = m.wrap(edge.label.text, 160);
      let center = edge.label.position;
      let left, top;
      if (center) { left = center.x - block.width / 2; top = center.y - block.height / 2; }
      else {
        const segments = points.slice(1).map((p, j) => ({ a: points[j], b: p, length: Math.hypot(p.x - points[j].x, p.y - points[j].y) }));
        const longest = segments.reduce((best, segment) => segment.length > best.length ? segment : best);
        center = { x: (longest.a.x + longest.b.x) / 2, y: (longest.a.y + longest.b.y) / 2 };
        if (Math.abs(longest.b.y - longest.a.y) > Math.abs(longest.b.x - longest.a.x)) { left = center.x + 8 * s; top = center.y - block.height / 2; }
        else { left = center.x - block.width / 2; top = center.y - 8 * s - block.height; }
      }
      draw.text(block, left, top, { 'data-edge-label': i });
    }
  }
  return draw.finish(part.canvas);
}
