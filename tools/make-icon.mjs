// Generates src-tauri/icons/icon.png without pulling in an image library.
// The mark is a white connected graph on an orange rounded square.
// Re-run with `node tools/make-icon.mjs`.
import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";

const SIZE = 512;
const SS = 3; // supersampling factor, for cheap antialiasing

const BG = [255, 104, 54];
const MARK = [255, 255, 255];

const hi = SIZE * SS;
const cover = new Float32Array(hi * hi * 3);
const alpha = new Float32Array(hi * hi);

function paint(x, y, color, weight) {
  if (x < 0 || y < 0 || x >= hi || y >= hi || weight <= 0) return;
  const i = y * hi + x;
  const w = Math.min(1, weight);
  // Painter's algorithm: later strokes sit on top of earlier ones.
  cover[i * 3] = cover[i * 3] * (1 - w) + color[0] * w;
  cover[i * 3 + 1] = cover[i * 3 + 1] * (1 - w) + color[1] * w;
  cover[i * 3 + 2] = cover[i * 3 + 2] * (1 - w) + color[2] * w;
  alpha[i] = alpha[i] * (1 - w) + w;
}

function disc(cx, cy, radius, color) {
  const r = radius * SS;
  const x0 = Math.floor(cx * SS - r - 1);
  const x1 = Math.ceil(cx * SS + r + 1);
  const y0 = Math.floor(cy * SS - r - 1);
  const y1 = Math.ceil(cy * SS + r + 1);
  for (let y = y0; y <= y1; y++) {
    for (let x = x0; x <= x1; x++) {
      const dx = x + 0.5 - cx * SS;
      const dy = y + 0.5 - cy * SS;
      if (dx * dx + dy * dy <= r * r) paint(x, y, color, 1);
    }
  }
}

function line(ax, ay, bx, by, width, color) {
  const steps = Math.ceil(Math.hypot(bx - ax, by - ay) * SS * 2);
  for (let s = 0; s <= steps; s++) {
    const t = s / steps;
    disc(ax + (bx - ax) * t, ay + (by - ay) * t, width / 2, color);
  }
}

// Rounded-square background.
const pad = 36;
const radius = 118;
for (let y = 0; y < hi; y++) {
  for (let x = 0; x < hi; x++) {
    const px = x / SS;
    const py = y / SS;
    const dx = Math.max(pad + radius - px, px - (SIZE - pad - radius), 0);
    const dy = Math.max(pad + radius - py, py - (SIZE - pad - radius), 0);
    if (dx * dx + dy * dy <= radius * radius) paint(x, y, BG, 1);
  }
}

// Four-node graph from the orange reference artwork.
const nodes = [
  [252, 176],
  [358, 250],
  [157, 310],
  [266, 366],
];

for (const [from, to] of [
  [0, 1],
  [0, 2],
  [1, 3],
  [2, 3],
]) {
  line(...nodes[from], ...nodes[to], 32, MARK);
}

for (const [x, y] of nodes) disc(x, y, 42, MARK);

/** Box-downsamples the supersampled buffer to `size`, as PNG scanlines. */
function scanlines(size) {
  const block = hi / size;
  const raw = Buffer.alloc(size * (size * 4 + 1));
  for (let y = 0; y < size; y++) {
    raw[y * (size * 4 + 1)] = 0; // PNG filter type 0
    for (let x = 0; x < size; x++) {
      let r = 0;
      let g = 0;
      let b = 0;
      let a = 0;
      let n = 0;
      for (let sy = Math.floor(y * block); sy < Math.floor((y + 1) * block); sy++) {
        for (let sx = Math.floor(x * block); sx < Math.floor((x + 1) * block); sx++) {
          const i = sy * hi + sx;
          r += cover[i * 3];
          g += cover[i * 3 + 1];
          b += cover[i * 3 + 2];
          a += alpha[i];
          n++;
        }
      }
      const o = y * (size * 4 + 1) + 1 + x * 4;
      raw[o] = Math.round(r / n);
      raw[o + 1] = Math.round(g / n);
      raw[o + 2] = Math.round(b / n);
      raw[o + 3] = Math.round((a / n) * 255);
    }
  }
  return raw;
}

function crc32(buf) {
  let c = ~0;
  for (const byte of buf) {
    c ^= byte;
    for (let k = 0; k < 8; k++) c = (c >>> 1) ^ (0xedb88320 & -(c & 1));
  }
  return ~c >>> 0;
}

function chunk(type, data) {
  const head = Buffer.alloc(4);
  head.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const tail = Buffer.alloc(4);
  tail.writeUInt32BE(crc32(body));
  return Buffer.concat([head, body, tail]);
}

function png(size) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // colour type: RGBA

  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(scanlines(size), { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

// The sizes `tauri build` looks for when bundling Linux packages, plus the
// master used by `npm run tauri icon` to derive .ico/.icns for other platforms.
const outputs = [
  ["icon.png", SIZE],
  ["128x128@2x.png", 256],
  ["128x128.png", 128],
  ["32x32.png", 32],
];

for (const [name, size] of outputs) {
  const out = new URL(`../src-tauri/icons/${name}`, import.meta.url).pathname;
  mkdirSync(dirname(out), { recursive: true });
  const bytes = png(size);
  writeFileSync(out, bytes);
  console.log(`wrote ${out} (${bytes.length} bytes)`);
}
