// shoryo固定公開版 diagram.js/measure.js のgrid配置・三行省略・全文操作を限定取り込み。MIT: ../../diagrams/LICENSE
// @ts-check
import { el, textEl } from '../../dom.js';
import { svgElement, textMetrics } from '../../diagrams/explanation-svg.js';

/** @param {{type:'diagram',title:string,role:string,source:string}} part */
export function gridView(part) {
  const host = el('section', 'part-grid');
  host.append(textEl('header', 'part-metadata', `${part.title} · ${part.role}`));
  requestAnimationFrame(async () => {
    await document.fonts.ready;
    if (!host.isConnected) return;
    const svg = svgElement('svg', { class: 'diagram', role: 'img', 'aria-label': part.title });
    host.append(svg);
    try {
      const metrics = textMetrics(svg);
      /** @type {Map<string,{label:string,slot:boolean}>} */ const nodes = new Map();
      /** @type {string[][]} */ const rows = [];
      /** @type {{from:string,to:string,label:string}[]} */ const edges = [];
      for (const raw of part.source.split('\n')) {
        const line = raw.trim(); if (!line) continue;
        if (line.startsWith('|')) { rows.push(line.replace(/^\|/, '').replace(/\|$/, '').split('|').map(cell => cell.trim())); continue; }
        const edge = line.match(/^(\S+)\s*->\s*(\S+?)\s*(?::\s*(.*))?$/);
        if (edge) { edges.push({ from: edge[1], to: edge[2], label: edge[3] ?? '' }); continue; }
        const node = line.match(/^(\S+)\s*=\s*(\?)?\s*(.*)$/);
        if (node) nodes.set(node[1], { label: node[3], slot: node[2] === '?' });
      }
      /** @type {Map<string,{x:number,y:number}>} */ const at = new Map();
      rows.forEach((row, y) => row.forEach((id, x) => { if (id !== '.' && id && !at.has(id)) at.set(id, { x, y }); }));
      let extra = 0; for (const id of nodes.keys()) if (!at.has(id)) at.set(id, { x: extra++, y: rows.length });
      const prepared = [...at].map(([id, point]) => {
        const node = nodes.get(id); if (!node) throw new Error('Unknown grid node');
        const whole = metrics.wrap(node.label, 134).lines;
        const cut = whole.slice(0, 3);
        if (whole.length > 3) {
          let tail = cut[2]; while (tail && metrics.width(`${tail}…`) > 134) tail = [...tail].slice(0, -1).join(''); cut[2] = `${tail}…`;
        }
        return { id, point, node, cut, whole };
      });
      metrics.dispose();
      /** @type {Map<number,number>} */ const heights = new Map();
      for (const node of prepared) heights.set(node.point.y, Math.max(heights.get(node.point.y) ?? 0, node.cut.length * 15 + 20, 44));
      const content = svgElement('g', { transform: 'translate(16 16)' }); svg.append(content);
      /** @param {string[]} lines @param {number} x @param {number} y */
      const label = (lines, x, y) => svgElement('text', { x, y: y - (lines.length - 1) * 7.5, 'text-anchor': 'middle', 'dominant-baseline': 'middle' }, lines.map((line, i) => svgElement('tspan', { x, dy: i === 0 ? 0 : 15 }, line)));
      /** @param {{x:number,y:number}} point */ const center = point => ({ x: point.x * 250 + 125, y: point.y * 130 + 65 });
      for (const edge of edges) {
        const from = at.get(edge.from), to = at.get(edge.to); if (!from || !to || edge.from === edge.to) continue;
        const a = center(from), b = center(to), length = Math.hypot(b.x - a.x, b.y - a.y), dx = (b.x - a.x) / length, dy = (b.y - a.y) / length;
        const offset = edges.some(e => e.from === edge.to && e.to === edge.from) ? 8 : 0;
        /** @param {{x:number,y:number}} p @param {number} height @param {number} vx @param {number} vy */
        const border = (p, height, vx, vy) => { const t = Math.min(vx === 0 ? Infinity : 78 / Math.abs(vx), vy === 0 ? Infinity : (height / 2 + 3) / Math.abs(vy)); return { x: p.x + vx * t - dy * offset, y: p.y + vy * t + dx * offset }; };
        const start = border(a, heights.get(from.y) ?? 44, dx, dy), end = border(b, heights.get(to.y) ?? 44, -dx, -dy);
        content.append(svgElement('line', { x1: start.x, y1: start.y, x2: end.x, y2: end.y }));
        content.append(svgElement('polygon', { points: `${end.x},${end.y} ${end.x - dx * 8 + dy * 3},${end.y - dy * 8 - dx * 3} ${end.x - dx * 8 - dy * 3},${end.y - dy * 8 + dx * 3}` }));
        if (edge.label) content.append(label([edge.label], (start.x + end.x) / 2 - dy * 10, (start.y + end.y) / 2 + dx * 10 - 8));
      }
      const frame = () => {
        const box = content.getBBox(); const width = box.width + 32, height = box.height + 32;
        svg.setAttribute('viewBox', `${box.x} ${box.y} ${width} ${height}`); svg.setAttribute('width', String(width)); svg.setAttribute('height', String(height));
      };
      for (const { point, node, cut, whole } of prepared) {
        const c = center(point), height = heights.get(point.y) ?? 44;
        const rect = svgElement('rect', { x: c.x - 75, y: c.y - height / 2, width: 150, height, rx: 6, ...(node.slot ? { 'stroke-dasharray': '6 4' } : {}) });
        const group = svgElement('g', {}, rect, label(cut, c.x, c.y)); content.append(group);
        if (whole.length > cut.length) {
          group.prepend(svgElement('title', {}, node.label));
          group.setAttribute('tabindex', '0'); group.setAttribute('role', 'button'); group.setAttribute('aria-label', node.label); group.setAttribute('aria-expanded', 'false');
          let showing = false;
          const toggle = () => {
            showing = !showing; group.setAttribute('aria-expanded', String(showing)); const grown = showing ? Math.max(44, whole.length * 15 + 20) : height;
            rect.setAttribute('y', String(c.y - grown / 2)); rect.setAttribute('height', String(grown)); group.replaceChildren(rect, label(showing ? whole : cut, c.x, c.y)); frame();
          };
          group.addEventListener('click', toggle); group.addEventListener('keydown', event => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); toggle(); } });
        }
      }
      frame();
    } catch (error) {
      svg.remove(); host.append(textEl('p', 'part-failure', `Diagram unavailable: ${error}`), textEl('pre', 'part-original', JSON.stringify(part, null, 2)));
    }
  });
  return host;
}
