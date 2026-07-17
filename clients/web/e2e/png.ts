/**
 * A tiny, dependency-free PNG reader — just enough to prove a canvas screenshot is
 * **non-blank** (issue #279). Playwright's `locator.screenshot()` returns an 8-bit
 * RGBA, non-interlaced PNG; rather than pull in an image library we decode it with
 * Node's built-in `zlib` and count how many distinct colors it contains. A blank
 * board is a single flat fill (one color); a board that actually painted its cards
 * has many. This is the pixel-level half of the "attached AND non-blank" assertion
 * that the StrictMode detach bug (#276) must fail.
 */
import { inflateSync } from 'node:zlib';

/** Reverse a single PNG scanline filter (CR of the PNG spec, §9). */
function unfilter(
  filter: number,
  line: Buffer,
  prev: Buffer | null,
  bpp: number,
  out: Buffer,
  offset: number,
): void {
  for (let i = 0; i < line.length; i += 1) {
    const raw = line[i];
    const a = i >= bpp ? out[offset + i - bpp] : 0; // left
    const b = prev ? prev[i] : 0; // up
    const c = prev && i >= bpp ? prev[i - bpp] : 0; // up-left
    let value: number;
    switch (filter) {
      case 0:
        value = raw;
        break;
      case 1:
        value = raw + a;
        break;
      case 2:
        value = raw + b;
        break;
      case 3:
        value = raw + ((a + b) >> 1);
        break;
      case 4: {
        const p = a + b - c;
        const pa = Math.abs(p - a);
        const pb = Math.abs(p - b);
        const pc = Math.abs(p - c);
        const pred = pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
        value = raw + pred;
        break;
      }
      default:
        throw new Error(`unsupported PNG filter ${filter}`);
    }
    out[offset + i] = value & 0xff;
  }
}

/**
 * Count the number of distinct RGBA colors in a PNG buffer. Used only to
 * distinguish a flat/blank canvas (≈1 color) from a real render (many).
 */
export function countDistinctColors(png: Buffer): number {
  // 8-byte signature, then length(4) + type(4) + data + crc(4) chunks.
  let pos = 8;
  let width = 0;
  let height = 0;
  let bitDepth = 0;
  let colorType = 0;
  const idat: Buffer[] = [];
  while (pos < png.length) {
    const len = png.readUInt32BE(pos);
    const type = png.toString('ascii', pos + 4, pos + 8);
    const data = png.subarray(pos + 8, pos + 8 + len);
    if (type === 'IHDR') {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      bitDepth = data[8];
      colorType = data[9];
    } else if (type === 'IDAT') {
      idat.push(data);
    } else if (type === 'IEND') {
      break;
    }
    pos += 12 + len;
  }
  if (bitDepth !== 8 || colorType !== 6) {
    throw new Error(`expected 8-bit RGBA PNG, got bitDepth=${bitDepth} colorType=${colorType}`);
  }

  const channels = 4;
  const stride = width * channels;
  const raw = inflateSync(Buffer.concat(idat));
  const pixels = Buffer.alloc(height * stride);
  let prev: Buffer | null = null;
  for (let y = 0; y < height; y += 1) {
    const filterByte = raw[y * (stride + 1)];
    const line = raw.subarray(y * (stride + 1) + 1, y * (stride + 1) + 1 + stride);
    unfilter(filterByte, line, prev, channels, pixels, y * stride);
    prev = pixels.subarray(y * stride, y * stride + stride);
  }

  const seen = new Set<number>();
  for (let i = 0; i < pixels.length; i += channels) {
    const key = (pixels[i] << 24) | (pixels[i + 1] << 16) | (pixels[i + 2] << 8) | pixels[i + 3];
    seen.add(key >>> 0);
    if (seen.size > 8) break; // early out: plenty of variance, definitely not blank
  }
  return seen.size;
}
