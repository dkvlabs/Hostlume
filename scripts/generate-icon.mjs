// Generates the original placeholder app icon (1024x1024 RGBA PNG):
// a dark rounded tile with a teal pulse line, drawn procedurally from the
// design tokens in docs/design.md §2. No downloaded or third-party assets.
// Run: node scripts/generate-icon.mjs
import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const outPath = join(
  here,
  "..",
  "apps",
  "desktop",
  "src-tauri",
  "icons",
  "source",
  "app-icon-1024.png"
);

const BASE = 1024;
const SS = 2; // supersample factor for anti-aliasing
const N = BASE * SS;

// --- PNG encoding -----------------------------------------------------------
const CRC_TABLE = new Int32Array(256);
for (let n = 0; n < 256; n++) {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  CRC_TABLE[n] = c;
}
function crc32(buf) {
  let c = 0xffffffff;
  for (const b of buf) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body), 0);
  return Buffer.concat([len, body, crc]);
}
function encodePng(width, height, rgba) {
  const signature = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // color type RGBA
  const stride = width * 4;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y++) {
    rgba.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
  }
  return Buffer.concat([
    signature,
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

// --- Drawing helpers ---------------------------------------------------------
const clamp01 = (v) => Math.min(1, Math.max(0, v));

// Rounded-rect signed distance coverage (exact anti-aliasing).
function roundedRectCoverage(px, py, cx, cy, half, radius) {
  const qx = Math.abs(px - cx) - (half - radius);
  const qy = Math.abs(py - cy) - (half - radius);
  const outside = Math.hypot(Math.max(qx, 0), Math.max(qy, 0));
  const inside = Math.min(Math.max(qx, qy), 0);
  const sd = outside + inside - radius;
  return clamp01(0.5 - sd);
}

function segmentDistance(px, py, ax, ay, bx, by) {
  const abx = bx - ax;
  const aby = by - ay;
  const lenSq = abx * abx + aby * aby;
  const t = lenSq === 0 ? 0 : clamp01(((px - ax) * abx + (py - ay) * aby) / lenSq);
  return Math.hypot(px - (ax + t * abx), py - (ay + t * aby));
}

function pulseCoverage(px, py, points, halfWidth) {
  let best = Infinity;
  for (let i = 0; i < points.length - 1; i++) {
    const [ax, ay] = points[i];
    const [bx, by] = points[i + 1];
    best = Math.min(best, segmentDistance(px, py, ax, ay, bx, by));
  }
  return clamp01(halfWidth + 0.5 - best);
}

// Design tokens: --bg #101318, --accent #67D4D0 (docs/design.md §2).
const BG = [16, 19, 24];
const ACCENT = [103, 212, 208];
const scale = N / BASE;
const half = N / 2;
const radius = 0.2237 * N; // macOS-style squircle-ish corner share
const pulsePoints = [
  [176, 544],
  [336, 544],
  [416, 352],
  [512, 688],
  [600, 448],
  [660, 544],
  [848, 544],
].map(([x, y]) => [x * scale, y * scale]);
const pulseHalfWidth = 28 * scale;

const hi = new Float32Array(N * N); // tile alpha
const hp = new Float32Array(N * N); // pulse alpha
for (let y = 0; y < N; y++) {
  for (let x = 0; x < N; x++) {
    const px = x + 0.5;
    const py = y + 0.5;
    hi[y * N + x] = roundedRectCoverage(px, py, half, half, half, radius);
    hp[y * N + x] = pulseCoverage(px, py, pulsePoints, pulseHalfWidth);
  }
}

// Box downsample SS×SS and composite.
const out = Buffer.alloc(BASE * BASE * 4);
for (let y = 0; y < BASE; y++) {
  for (let x = 0; x < BASE; x++) {
    let t = 0;
    let p = 0;
    for (let sy = 0; sy < SS; sy++) {
      for (let sx = 0; sx < SS; sx++) {
        t += hi[(y * SS + sy) * N + (x * SS + sx)];
        p += hp[(y * SS + sy) * N + (x * SS + sx)];
      }
    }
    const tile = t / (SS * SS);
    const pulse = p / (SS * SS);
    const idx = (y * BASE + x) * 4;
    const r = BG[0] + (ACCENT[0] - BG[0]) * pulse;
    const g = BG[1] + (ACCENT[1] - BG[1]) * pulse;
    const b = BG[2] + (ACCENT[2] - BG[2]) * pulse;
    out[idx] = Math.round(r);
    out[idx + 1] = Math.round(g);
    out[idx + 2] = Math.round(b);
    out[idx + 3] = Math.round(tile * 255);
  }
}

mkdirSync(dirname(outPath), { recursive: true });
writeFileSync(outPath, encodePng(BASE, BASE, out));
console.log(`wrote ${outPath}`);
