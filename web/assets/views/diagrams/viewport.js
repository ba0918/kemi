// shoryo固定公開版のviewport操作と実寸再検証をkemiのviewsへ接続。MIT: ../../diagrams/LICENSE
// @ts-check
import { button, el, textEl } from '../../dom.js';
import { drawing, svgElement, textMetrics } from '../../diagrams/explanation-svg.js';
import { sequenceDrawing } from '../../diagrams/explanation-sequence.js';
import { flowDrawing } from '../../diagrams/explanation-flow.js';
import { validateDrawing } from '../../diagrams/explanation-layout-validation.js';
import { PanZoom } from '../../diagrams/pan-zoom.js';

/** @param {import('../../diagrams/types.js').Sequence|import('../../diagrams/types.js').Flow} part @param {boolean} [modal] */
export function diagramViewport(part, modal = false) {
  const host = el('section', 'part-diagram');
  host.append(textEl('header', 'part-metadata', `${part.title} · ${part.role}`));
  const surface = el('div', 'part-diagram-surface');
  surface.tabIndex = 0; surface.setAttribute('role', 'group'); surface.setAttribute('aria-label', `${part.title} diagram controls`);
  const controls = el('div', 'part-diagram-controls');
  const svg = svgElement('svg', { class: 'explanation-diagram', role: 'img', 'aria-label': part.title });
  let ready = false, fitted = false, signature = '', queued = false;
  const zoom = new PanZoom(surface, { x: 0, y: 0, k: 1 }, view => {
    fitted = false; svg.style.transform = `translate(${view.x}px, ${view.y}px) scale(${view.k})`;
  }, modal ? 'canvas' : 'inline');
  const whole = () => {
    if (!ready) return;
    const box = surface.getBoundingClientRect(), width = Number(svg.getAttribute('width')), height = Number(svg.getAttribute('height'));
    if (box.width <= 0 || box.height <= 0) return;
    const k = Math.min(1, (box.width - 24) / width, (box.height - 24) / height);
    zoom.set({ k, x: (box.width - width * k) / 2, y: (box.height - height * k) / 2 }); fitted = true;
  };
  /** @param {string} label @param {()=>void} action */
  const control = (label, action) => { const btn = button(''); btn.textContent = label; btn.addEventListener('click', action); controls.append(btn); return btn; };
  control('Zoom out', () => zoom.zoomBy(1 / 1.2)); control('Zoom in', () => zoom.zoomBy(1.2)); control('Show whole diagram', whole);
  control('Reset diagram', () => { fitted = false; zoom.set({ x: 0, y: 0, k: 1 }); });
  /** @type {Record<string,[number,number]>} */ const moves = { ArrowLeft: [-40, 0], ArrowRight: [40, 0], ArrowUp: [0, -40], ArrowDown: [0, 40] };
  for (const [key, [dx, dy]] of Object.entries(moves)) control(`Move ${key.slice(5).toLowerCase()}`, () => zoom.set({ ...zoom.view, x: zoom.view.x + dx, y: zoom.view.y + dy }));
  if (!modal) {
    const enlarge = control('Open diagram separately', () => {
      const dialog = document.createElement('dialog'); dialog.className = 'part-diagram-dialog';
      dialog.setAttribute('aria-label', part.title);
      dialog.addEventListener('keydown', event => event.stopPropagation());
      const close = button(''); close.textContent = 'Close diagram';
      const finish = () => { dialog.close(); dialog.remove(); enlarge.focus(); host.scrollIntoView({ block: 'nearest' }); };
      close.addEventListener('click', finish); dialog.addEventListener('cancel', event => { event.preventDefault(); finish(); });
      dialog.append(close, diagramViewport(part, true)); document.body.append(dialog); dialog.showModal(); close.focus();
    });
  }
  surface.addEventListener('keydown', event => {
    if (event.target !== surface || event.ctrlKey || event.metaKey || event.altKey || !ready) return;
    const move = moves[event.key];
    if (move) { event.preventDefault(); event.stopPropagation(); zoom.set({ ...zoom.view, x: zoom.view.x + move[0], y: zoom.view.y + move[1] }); }
    else if (event.key === '+' || event.key === '-') { event.preventDefault(); event.stopPropagation(); zoom.zoomBy(event.key === '+' ? 1.2 : 1 / 1.2); }
    else if (event.key === '0') { event.preventDefault(); event.stopPropagation(); whole(); }
  });
  function conditions() {
    const probe = svgElement('text');
    const sample = svg.isConnected ? svg : svgElement('svg', { class: 'explanation-diagram' });
    if (sample !== svg) surface.append(sample); sample.append(probe);
    const styles = [sample, ...sample.querySelectorAll('text')].map(node => {
      const style = getComputedStyle(node); return [style.fontFamily, style.fontSize, style.fontWeight, style.fontStyle, style.letterSpacing, style.wordSpacing, style.lineHeight].join('|');
    });
    probe.remove(); if (sample !== svg) sample.remove();
    const zooms = [];
    for (let node = surface.parentElement; node; node = node.parentElement) zooms.push(getComputedStyle(node).zoom);
    return [surface.getClientRects().length > 0, ...new Set(styles), ...zooms, window.devicePixelRatio, window.visualViewport?.scale].join('|');
  }
  async function render() {
    await document.fonts.ready;
    if (!host.isConnected) return;
    surface.replaceChildren(svg); svg.replaceChildren();
    svg.style.transform = '';
    try {
      const draw = drawing(svg, textMetrics(svg));
      const result = part.type === 'flow' ? flowDrawing(part, draw, key => key.endsWith('start') ? 'Start' : 'End') : sequenceDrawing(part, draw);
      validateDrawing(result); ready = true; surface.classList.remove('failed'); controls.hidden = false;
      surface.style.height = modal ? '65vh' : `min(${result.height}px, 22.5rem, 50vh)`;
      svg.style.transform = `translate(${zoom.view.x}px, ${zoom.view.y}px) scale(${zoom.view.k})`;
      if (fitted || modal) whole();
    } catch (error) {
      ready = false; controls.hidden = true; surface.classList.add('failed'); surface.style.height = '';
      zoom.cancelGesture();
      const status = textEl('p', 'part-failure', `Diagram unavailable: ${error}`); status.setAttribute('role', 'status');
      surface.replaceChildren(status, textEl('pre', 'part-original', JSON.stringify(part, null, 2)));
    }
    signature = conditions();
  }
  function schedule() { if (queued) return; queued = true; requestAnimationFrame(() => { queued = false; void render(); }); }
  const resize = new ResizeObserver(() => { if (signature !== conditions()) schedule(); if (fitted) whole(); });
  const observer = new MutationObserver(records => {
    if (!host.isConnected) { observer.disconnect(); resize.disconnect(); document.fonts.removeEventListener('loadingdone', schedule); window.removeEventListener('resize', schedule); zoom.destroy(); return; }
    if (records.some(record => (record.type === 'attributes' && !(record.target instanceof SVGElement)) || document.head.contains(record.target)) && signature !== conditions()) schedule();
  });
  requestAnimationFrame(() => {
    if (!host.isConnected) return;
    resize.observe(surface); observer.observe(document.documentElement, { attributes: true });
    observer.observe(document.head, { childList: true, subtree: true, characterData: true, attributes: true });
    observer.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ['class', 'hidden', 'style'] });
    document.fonts.addEventListener('loadingdone', schedule); window.addEventListener('resize', schedule); void render();
  });
  host.append(controls, surface); controls.hidden = true; return host;
}
