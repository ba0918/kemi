import test from 'node:test';
import assert from 'node:assert/strict';
import { containsBox, fitHeight, labelBesideSegment } from './explanation-collision.js';

/** 中心 (0,0) の箱の中で、上下に並べた幅 width・高さ height の文字の箱。 @param {number[][]} sizes */
function stacked(sizes) {
  const total = sizes.reduce((sum, [, height]) => sum + height, 0);
  let top = -total / 2;
  return sizes.map(([width, height]) => { const box = { x: -width / 2, y: top, width, height }; top += height; return box; });
}

test('a start or end node grows until a wrapped label reaching the rounded ends fits inside', () => {
  const boxes = stacked([[30, 18], [0, 8], [156, 72]]);
  const shape = { id: 'n', kind: 'end', x: 0, y: 0, width: 180, height: 122 };
  assert.equal(boxes.every(box => containsBox(shape, box)), false, 'the height derived from the text alone cuts the corners');
  const height = fitHeight(shape, boxes);
  assert.ok(boxes.every(box => containsBox({ ...shape, height }, box)));
});

test('a node whose text already fits keeps the height derived from the text', () => {
  const boxes = stacked([[30, 18], [0, 8], [60, 18]]);
  assert.equal(fitHeight({ id: 'n', kind: 'start', x: 0, y: 0, width: 180, height: 68 }, boxes), 68);
});

test('an edge label sits beside its segment, clear of the line, whichever way the edge runs', () => {
  const size = { width: 48, height: 15 };
  for (const [a, b] of [[{ x: 0, y: 0 }, { x: 100, y: 0 }], [{ x: 100, y: 0 }, { x: 0, y: 0 }], [{ x: 0, y: 0 }, { x: 0, y: 100 }], [{ x: 0, y: 0 }, { x: 80, y: 60 }]]) {
    const centre = labelBesideSegment(a, b, size, 4);
    const length = Math.hypot(b.x - a.x, b.y - a.y), nx = -(b.y - a.y) / length, ny = (b.x - a.x) / length;
    const distances = [-1, 1].flatMap(sx => [-1, 1].map(sy => (centre.x + sx * size.width / 2 - a.x) * nx + (centre.y + sy * size.height / 2 - a.y) * ny));
    assert.ok(Math.min(...distances) >= 4 - 1e-9, `${JSON.stringify([a, b])}: ${distances}`);
  }
});
