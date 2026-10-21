// ブラウザ検査の PNG の読み書きと画素の比べ方。ブラウザに依らない。

import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { crc32, deflateSync, inflateSync } from 'node:zlib';

/** PNG を読む（8 ビットの RGB か RGBA、インターレース無し）。 */
export function decodePng(bytes) {
  let offset = 8;
  let width = 0;
  let height = 0;
  let channels = 4;
  const data = [];
  while (offset < bytes.length) {
    const length = bytes.readUInt32BE(offset);
    const type = bytes.toString('latin1', offset + 4, offset + 8);
    const body = bytes.subarray(offset + 8, offset + 8 + length);
    if (type === 'IHDR') {
      width = body.readUInt32BE(0);
      height = body.readUInt32BE(4);
      assert.equal(body[8], 8, 'only 8-bit PNG');
      channels = body[9] === 6 ? 4 : body[9] === 2 ? 3 : assert.fail(`color type ${body[9]}`);
    } else if (type === 'IDAT') {
      data.push(body);
    }
    offset += 12 + length;
  }
  const raw = inflateSync(Buffer.concat(data));
  const stride = width * channels;
  const pixels = Buffer.alloc(width * height * 4);
  let previous = Buffer.alloc(stride);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const line = Buffer.from(raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1)));
    for (let x = 0; x < stride; x++) {
      const left = x >= channels ? line[x - channels] : 0;
      const up = previous[x];
      const corner = x >= channels ? previous[x - channels] : 0;
      const paeth = () => {
        const p = left + up - corner;
        const pa = Math.abs(p - left);
        const pb = Math.abs(p - up);
        const pc = Math.abs(p - corner);
        return pa <= pb && pa <= pc ? left : pb <= pc ? up : corner;
      };
      const add = [0, left, up, (left + up) >> 1, paeth()][filter];
      line[x] = (line[x] + add) & 0xff;
    }
    for (let x = 0; x < width; x++) {
      for (let c = 0; c < 4; c++) {
        pixels[(y * width + x) * 4 + c] = c < channels ? line[x * channels + c] : 255;
      }
    }
    previous = line;
  }
  return { width, height, pixels };
}

/** PNG のチャンク（長さ・種類・中身・CRC）。 */
function chunk(type, body) {
  const header = Buffer.alloc(8);
  header.writeUInt32BE(body.length, 0);
  header.write(type, 4, 'latin1');
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([Buffer.from(type, 'latin1'), body])), 0);
  return Buffer.concat([header, body, crc]);
}

/**
 * 8 ビットでインターレース無しの PNG を組み立てる。
 * @param {number} width
 * @param {number} height
 * @param {number} colorType 2 は RGB、6 は RGBA
 * @param {Buffer} raw 各行の先頭にフィルタの種類（0）を付けた画素
 */
function assemble(width, height, colorType, raw) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8;
  header[9] = colorType;
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', header), chunk('IDAT', deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}

/** RGBA の画素を PNG にする（差の画像を残すため）。 */
export function encodePng(width, height, pixels) {
  const raw = Buffer.alloc(height * (width * 4 + 1));
  for (let y = 0; y < height; y++) {
    pixels.copy(raw, y * (width * 4 + 1) + 1, y * width * 4, (y + 1) * width * 4);
  }
  return assemble(width, height, 6, raw);
}

/**
 * 1×1 の RGB の PNG。色ごとに違うバイト列になる。
 * @param {number} r
 * @param {number} g
 * @param {number} b
 */
export function png(r, g, b) {
  return assemble(1, 1, 2, Buffer.from([0, r, g, b]));
}

/** 画像の (x, y) の色。 */
export function pixelAt(image, x, y) {
  const at = (y * image.width + x) * 4;
  return [image.pixels[at], image.pixels[at + 1], image.pixels[at + 2]];
}

/** 画像の範囲 [左, 上, 右, 下) の中で、条件に合う画素の数。 */
export function countPixels(image, [left, top, right, bottom], test) {
  let count = 0;
  for (let y = top; y < Math.min(bottom, image.height); y++) {
    for (let x = left; x < Math.min(right, image.width); x++) {
      if (test(pixelAt(image, x, y))) count += 1;
    }
  }
  return count;
}

/** 画像の左上から width × height を切り出す。 */
export function crop(image, width, height) {
  const pixels = Buffer.alloc(width * height * 4);
  for (let y = 0; y < height; y++) image.pixels.copy(pixels, y * width * 4, y * image.width * 4, (y * image.width + width) * 4);
  return { width, height, pixels };
}

/** 2 枚の重なる範囲を比べ、違う画素の数と割合を返す。違えば差の画像を書く。 */
export async function comparePixels(left, right, diffPath) {
  const width = Math.min(left.width, right.width);
  const height = Math.min(left.height, right.height);
  const diff = Buffer.alloc(width * height * 4);
  let different = 0;
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const a = (y * left.width + x) * 4;
      const b = (y * right.width + x) * 4;
      const same = [0, 1, 2].every((c) => left.pixels[a + c] === right.pixels[b + c]);
      const d = (y * width + x) * 4;
      if (same) {
        diff[d] = diff[d + 1] = diff[d + 2] = Math.round(left.pixels[a] * 0.3 + 178);
      } else {
        different += 1;
        diff[d] = 255;
      }
      diff[d + 3] = 255;
    }
  }
  if (different > 0) {
    await writeFile(diffPath, encodePng(width, height, diff));
  }
  return { different, total: width * height, ratio: different / (width * height) };
}
