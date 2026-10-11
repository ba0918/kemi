// shoryo 46c6f764c1d373365a8902ffcc682245079363e6 のinline/canvas操作を限定取り込み。MIT: ./LICENSE
// @ts-check
/** @typedef {{x:number,y:number,k:number}} View */
const MIN_ZOOM = .2, MAX_ZOOM = 3, DRAG_PX = 4;
/** @param {number} value @param {number} low @param {number} high */
const clamp = (value, low, high) => Math.min(high, Math.max(low, value));
export class PanZoom {
  /** @param {HTMLElement} wrap @param {View} view @param {(view:View)=>void} onView @param {'inline'|'canvas'} mode */
  constructor(wrap, view, onView, mode) {
    this.wrap = wrap; this.view = { ...view }; this.onView = onView; this.mode = mode;
    const lifetime = new AbortController();
    /** @type {Map<number,{x:number,y:number}>} */ const pointers = new Map();
    /** @type {{view:View,start:{x:number,y:number}[]}|null} */ let gesture = null;
    let panning = false;
    /** @param {PointerEvent|WheelEvent} event */
    const local = event => { const box = wrap.getBoundingClientRect(); return { x: event.clientX - box.left, y: event.clientY - box.top }; };
    wrap.addEventListener('wheel', event => {
      if (wrap.classList.contains('failed') || (!event.ctrlKey && !event.metaKey)) return;
      event.preventDefault(); const p = local(event); this.zoomAt(Math.exp(-event.deltaY * .0015), p.x, p.y);
    }, { passive: false, signal: lifetime.signal });
    wrap.addEventListener('pointerdown', event => {
      if (event.button !== 0 || wrap.classList.contains('failed')) return;
      pointers.set(event.pointerId, mode === 'inline' && event.pointerType === 'touch' ? { x: event.clientX, y: event.clientY } : local(event));
      if (mode === 'inline' && event.pointerType === 'touch') wrap.setPointerCapture(event.pointerId);
      gesture = { view: { ...this.view }, start: [...pointers.values()].map(p => ({ ...p })) };
    }, { signal: lifetime.signal });
    wrap.addEventListener('pointermove', event => {
      if (!pointers.has(event.pointerId) || !gesture) return;
      const inlineTouch = mode === 'inline' && event.pointerType === 'touch';
      pointers.set(event.pointerId, inlineTouch ? { x: event.clientX, y: event.clientY } : local(event));
      const now = [...pointers.values()];
      if (inlineTouch && now.length === 1) {
        document.scrollingElement?.scrollBy(0, gesture.start[0].y - now[0].y);
        gesture = { view: { ...this.view }, start: now.map(p => ({ ...p })) }; return;
      }
      if (now.length === 1 && gesture.start.length === 1) {
        const dx = now[0].x - gesture.start[0].x, dy = now[0].y - gesture.start[0].y;
        if (!panning && Math.hypot(dx, dy) < DRAG_PX) return;
        if (!panning) wrap.setPointerCapture(event.pointerId); panning = true;
        this.set({ ...gesture.view, x: gesture.view.x + dx, y: gesture.view.y + dy });
      } else if (now.length === 2) {
        if (gesture.start.length !== 2) gesture = { view: { ...this.view }, start: now.map(p => ({ ...p })) };
        const [a, b] = gesture.start, [c, d] = now;
        const before = Math.hypot(a.x - b.x, a.y - b.y) || 1;
        const factor = Math.hypot(c.x - d.x, c.y - d.y) / before;
        const k = clamp(gesture.view.k * factor, Math.min(MIN_ZOOM, gesture.view.k), MAX_ZOOM);
        const mid = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 }, to = { x: (c.x + d.x) / 2, y: (c.y + d.y) / 2 };
        if (inlineTouch) { const box = wrap.getBoundingClientRect(); mid.x -= box.left; mid.y -= box.top; to.x -= box.left; to.y -= box.top; }
        const ratio = k / gesture.view.k; panning = true;
        this.set({ k, x: to.x - (mid.x - gesture.view.x) * ratio, y: to.y - (mid.y - gesture.view.y) * ratio });
      }
    }, { signal: lifetime.signal });
    const cancel = () => {
      const ids = [...pointers.keys()]; pointers.clear(); gesture = null; panning = false;
      for (const id of ids) if (wrap.hasPointerCapture(id)) wrap.releasePointerCapture(id);
    };
    /** @param {PointerEvent} event */
    const end = event => {
      if (!pointers.delete(event.pointerId)) return;
      if (pointers.size > 0) { gesture = { view: { ...this.view }, start: [...pointers.values()].map(p => ({ ...p })) }; return; }
      gesture = null; panning = false;
    };
    wrap.addEventListener('pointerup', end, { signal: lifetime.signal });
    wrap.addEventListener('pointercancel', mode === 'inline' ? cancel : end, { signal: lifetime.signal });
    if (mode === 'inline') wrap.addEventListener('lostpointercapture', event => { if (pointers.has(event.pointerId)) cancel(); }, { signal: lifetime.signal });
    this.cancelGesture = cancel;
    this.destroy = () => { cancel(); lifetime.abort(); };
  }
  /** @param {View} view */
  set(view) { this.view = view; this.onView(view); }
  /** @param {number} factor @param {number} cx @param {number} cy */
  zoomAt(factor, cx, cy) {
    const v = this.view, k = clamp(v.k * factor, Math.min(MIN_ZOOM, v.k), MAX_ZOOM), ratio = k / v.k;
    this.set({ k, x: cx - (cx - v.x) * ratio, y: cy - (cy - v.y) * ratio });
  }
  /** @param {number} factor */
  zoomBy(factor) { const box = this.wrap.getBoundingClientRect(); this.zoomAt(factor, box.width / 2, box.height / 2); }
}
