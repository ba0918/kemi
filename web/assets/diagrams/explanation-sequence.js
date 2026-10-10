// shoryo 46c6f764c1d373365a8902ffcc682245079363e6。MIT: ./LICENSE
// @ts-check
import { svgElement } from './explanation-svg.js';
/** @param {import('./types.js').Sequence} part @param {import('./types.js').Drawing} draw */
export function sequenceDrawing(part, draw) {
  const { metrics: m, svg } = draw;
  const s = m.scale;
  const layout = { participant_gap: 220, event_gap: 72, self_loop_width: 48, ...part.layout };
  const participants = part.participants.map((p, i) => ({ ...p, x: p.x ?? 124 + i * layout.participant_gap, block: m.wrap(p.label, 136) }));
  const headerHeight = Math.max(48, ...participants.map(p => p.block.height + 24 * s));
  const headerBottom = 24 + headerHeight;
  const byId = new Map(participants.map(p => [p.id, p]));
  for (const p of participants) {
    draw.shape({ id: p.id, kind: 'process', x: p.x, y: 24 + headerHeight / 2, width: 160, height: headerHeight }, svgElement('rect', { x: p.x - 80, y: 24, width: 160, height: headerHeight, 'data-header': p.id }));
    draw.text(p.block, p.x - p.block.width / 2, 24 + (headerHeight - p.block.height) / 2, { 'data-header-label': p.id }, p.id);
  }
  /** @param {import('./types.js').Message} event @param {number} top */
  function message(event, top) {
    const from = byId.get(event.from), to = byId.get(event.to);
    if (!from || !to) throw new Error('Unknown participant');
    const self = from.id === to.id;
    const wrapWidth = self ? Math.max(1, layout.self_loop_width - 16) : Math.max(1, Math.min(280, Math.abs(to.x - from.x) - 24));
    const block = m.wrap(event.label, wrapWidth);
    const x = self ? from.x + layout.self_loop_width / 2 : (from.x + to.x) / 2;
    draw.text(block, x - block.width / 2, top, { 'data-message-label': true });
    const arrowY = top + block.height + 12 * s;
    const points = self ? [{ x: from.x, y: arrowY }, { x: from.x + layout.self_loop_width, y: arrowY }, { x: from.x + layout.self_loop_width, y: arrowY + 32 }, { x: from.x, y: arrowY + 32 }] : [{ x: from.x, y: arrowY }, { x: to.x, y: arrowY }];
    draw.arrow(points, { 'data-message': true, ...(event.kind === 'return' ? { 'stroke-dasharray': '6 4' } : {}) });
    return points[points.length - 1].y;
  }
  let top = headerBottom + 48;
  let occupiedBottom = top;
  for (const event of part.events) {
    if (event.type === 'message') { occupiedBottom = message(event, top); top = occupiedBottom + (event.gap_after ?? layout.event_gap); continue; }
    const branches = event.type === 'alt' ? event.branches : [{ condition: event.condition, messages: event.messages }];
    const firstLabel = draw.labels.length, firstArrow = draw.arrows.length;
    const bands = branches.map(b => ({ ...b, block: m.wrap(b.condition, 280), marker: m.wrap(event.type, 280) }));
    const frameTop = top;
    const placements = [];
    for (const [i, band] of bands.entries()) {
      let divider = null;
      if (i) { divider = top + 16 * s; top = divider + 8 * s; }
      const bandTop = top;
      const bandHeight = 16 * s + band.marker.height + 8 * s + band.block.height + 16 * s;
      placements.push({ band, top: bandTop, height: bandHeight, divider }); top += bandHeight + 16 * s;
      for (const item of band.messages) { occupiedBottom = message(item, top); top = occupiedBottom + (item.gap_after ?? layout.event_gap); }
    }
    const contentLabels = draw.labels.slice(firstLabel);
    const points = draw.arrows.slice(firstArrow).flatMap(a => a.points);
    const xs = [...points.map(p => p.x), ...contentLabels.flatMap(l => [l.x, l.x + l.block.width])];
    if (Math.max(...xs) === Math.min(...xs)) xs.push(...participants.map(p => p.x));
    const left = Math.min(...xs) - 16 * s;
    const conditionWidth = Math.max(...bands.flatMap(b => [b.block.width, b.marker.width]));
    const right = Math.max(Math.max(...xs) + 16 * s, left + 16 * s + conditionWidth + 16 * s);
    const bottom = top + 16 * s;
    svg.prepend(svgElement('rect', { x: left, y: frameTop, width: right - left, height: bottom - frameTop, 'data-frame': event.type, class: 'explanation-frame' }));
    draw.frames.push({ x: left, y: frameTop, width: right - left, height: bottom - frameTop });
    for (const item of placements) {
      if (item.divider !== null) svg.append(svgElement('line', { x1: left, x2: right, y1: item.divider, y2: item.divider }));
      draw.text(item.band.marker, left + 16 * s, item.top + 16 * s, { 'data-frame-marker': true });
      draw.text(item.band.block, left + 16 * s, item.top + 16 * s + item.band.marker.height + 8 * s, { 'data-condition': true });
    }
    occupiedBottom = bottom; top = bottom + (event.gap_after ?? layout.event_gap);
  }
  for (const p of participants) svg.prepend(svgElement('line', { x1: p.x, x2: p.x, y1: headerBottom, y2: occupiedBottom + 24, class: 'explanation-lifeline' }));
  return draw.finish(part.canvas);
}
