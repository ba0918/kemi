// shoryo 46c6f764c1d373365a8902ffcc682245079363e6。MIT: ./LICENSE
// @ts-check
import { shapesOverlap, boxOverlapsShape, containsBox, segmentEntersShape, boxesOverlap } from './explanation-collision.js';
/** @param {import('./types.js').DrawingResult} result */
export function validateDrawing(result) {
  const { shapes, labels, arrows, frames, width, height, svg } = result;
  if (width > 8192 || height > 8192 || width < 64 || height < 64) throw new Error(`Canvas extent ${width}×${height} is outside 64–8192`);
  const bounds = svg.getBBox();
  if (bounds.x < -0.5 || bounds.y < -0.5 || bounds.x + bounds.width > width + 0.5 || bounds.y + bounds.height > height + 0.5) throw new Error('Drawing extends outside canvas');
  for (const shape of shapes) {
    if (shape.x - shape.width / 2 - 0.75 < -0.5 || shape.y - shape.height / 2 - 0.75 < -0.5 || shape.x + shape.width / 2 + 0.75 > width + 0.5 || shape.y + shape.height / 2 + 0.75 > height + 0.5) throw new Error(`Shape ${shape.id} stroke extends outside canvas`);
  }
  for (const [i, frame] of frames.entries()) {
    if (frame.x - 0.75 < -0.5 || frame.y - 0.75 < -0.5 || frame.x + frame.width + 0.75 > width + 0.5 || frame.y + frame.height + 0.75 > height + 0.5) throw new Error(`Frame ${i} stroke extends outside canvas`);
  }
  for (let i = 0; i < shapes.length; i++) for (let j = i + 1; j < shapes.length; j++) if (shapesOverlap(shapes[i], shapes[j])) throw new Error(`Shapes ${shapes[i].id} and ${shapes[j].id} overlap`);
  const measured = labels.map(label => {
    const box = label.element.getBBox();
    if (![box.x, box.y, box.width, box.height].every(Number.isFinite) || (label.block.original.trim() && (box.width <= 0 || box.height <= 0))) throw new Error('Label measurement unavailable');
    return { ...label, box };
  });
  for (const [i, label] of measured.entries()) {
    for (const shape of shapes) {
      if (shape.id === label.owner) { if (!containsBox(shape, label.box)) throw new Error(`Label outside owning shape ${shape.id}: containment failure`); }
      else if (boxOverlapsShape(label.box, shape)) throw new Error(`Label ${i} overlaps unrelated shape ${shape.id}`);
    }
    for (let j = i + 1; j < measured.length; j++) if (boxesOverlap(label.box, measured[j].box)) throw new Error(`Labels ${i} and ${j} overlap`);
  }
  for (const [i, arrow] of arrows.entries()) {
    for (const p of [...arrow.points, ...arrow.head]) if (p.x - 0.75 < -0.5 || p.y - 0.75 < -0.5 || p.x + 0.75 > width + 0.5 || p.y + 0.75 > height + 0.5) throw new Error(`Arrow ${i} extends outside canvas`);
    for (const shape of shapes) for (let j = 1; j < arrow.points.length; j++) if (segmentEntersShape(arrow.points[j - 1], arrow.points[j], shape)) throw new Error(`Arrow ${i} enters shape ${shape.id} interior`);
  }
  return result;
}
