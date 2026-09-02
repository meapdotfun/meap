/**
 * Pixel passes.
 *
 * These run on a 2D canvas over the rendered frame rather than as GL shaders.
 * For treatments that resample the image into something else entirely, reading
 * a small buffer and drawing text or dots is simpler than a shader and gives
 * exact control over the result.
 */

const RAMP = ' .:-=+*#%@';
const RAMP_LONG = ' .`\'",:;I!l><~+_-?][}{1)(|\\/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$';

let buf = null;
function readback(src) {
  if (!buf || buf.canvas.width !== src.width || buf.canvas.height !== src.height) {
    const c = document.createElement('canvas');
    c.width = src.width; c.height = src.height;
    buf = c.getContext('2d', { willReadFrequently: true });
  }
  buf.clearRect(0, 0, src.width, src.height);
  buf.drawImage(src, 0, 0);
  return buf.getImageData(0, 0, src.width, src.height);
}

/** Luminance to characters, coloured by the source pixel. */
export function ascii(src, ctx, w, h, opts = {}) {
  const img = readback(src);
  const cols = src.width, rows = src.height;
  const cw = w / cols, ch = h / rows;
  ctx.fillStyle = opts.bg || '#05070A';
  ctx.fillRect(0, 0, w, h);
  const size = Math.max(4, ch * 1.02);
  ctx.font = `${size}px ui-monospace, "JetBrains Mono", monospace`;
  ctx.textBaseline = 'top';
  const ramp = opts.long ? RAMP_LONG : RAMP;
  const d = img.data;
  for (let y = 0; y < rows; y++) {
    for (let x = 0; x < cols; x++) {
      const i = (y * cols + x) * 4;
      const l = (d[i] * 0.299 + d[i + 1] * 0.587 + d[i + 2] * 0.114) / 255;
      if (l < 0.03) continue;
      const ci = Math.min(ramp.length - 1, Math.floor(l * ramp.length));
      // Tint toward the source hue so structure survives, but keep it terminal.
      ctx.fillStyle = opts.mono
        ? `rgba(${opts.ink[0]},${opts.ink[1]},${opts.ink[2]},${0.25 + l * 0.75})`
        : `rgb(${Math.round(d[i] * 0.5 + 120 * l)},${Math.round(d[i + 1] * 0.55 + 190 * l)},${Math.round(d[i + 2] * 0.5 + 130 * l)})`;
      ctx.fillText(ramp[ci], x * cw, y * ch);
    }
  }
}

/** Nearest-neighbour upscale with a posterised palette and ordered dither. */
const BAYER = [
  [0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5],
];
export function pixel(src, ctx, w, h, opts = {}) {
  const img = readback(src);
  const cols = src.width, rows = src.height;
  const levels = opts.levels || 6;
  const d = img.data;
  const step = 255 / (levels - 1);
  for (let y = 0; y < rows; y++) {
    for (let x = 0; x < cols; x++) {
      const i = (y * cols + x) * 4;
      const t = (BAYER[y & 3][x & 3] / 16 - 0.5) * (step * 0.7);
      for (let k = 0; k < 3; k++) {
        d[i + k] = Math.max(0, Math.min(255, Math.round((d[i + k] + t) / step) * step));
      }
    }
  }
  buf.putImageData(img, 0, 0);
  ctx.imageSmoothingEnabled = false;
  ctx.clearRect(0, 0, w, h);
  ctx.drawImage(buf.canvas, 0, 0, cols, rows, 0, 0, w, h);
  ctx.imageSmoothingEnabled = true;
}

/**
 * Two ink plates on paper, each screened at its own angle and offset by a
 * pixel or two. The misregistration is the point.
 */
export function riso(src, ctx, w, h, opts = {}) {
  const img = readback(src);
  const cols = src.width, rows = src.height;
  const d = img.data;
  const paper = opts.paper || '#F3F0E6';
  ctx.fillStyle = paper;
  ctx.fillRect(0, 0, w, h);

  const plates = opts.inks || [
    { color: [204, 255, 0], angle: 0.26, offset: [1.6, -1.1], cell: 4.4, gamma: 1.35 },
    { color: [24, 24, 30], angle: 1.02, offset: [-1.2, 1.4], cell: 3.6, gamma: 1.9 },
  ];
  const sx = w / cols, sy = h / rows;

  for (const p of plates) {
    ctx.save();
    ctx.translate(p.offset[0], p.offset[1]);
    ctx.fillStyle = `rgb(${p.color[0]},${p.color[1]},${p.color[2]})`;
    const ca = Math.cos(p.angle), sa = Math.sin(p.angle);
    const cell = p.cell;
    const R = Math.hypot(w, h);
    for (let v = -R; v < R; v += cell) {
      for (let u = -R; u < R; u += cell) {
        const x = u * ca - v * sa + w / 2;
        const y = u * sa + v * ca + h / 2;
        if (x < -cell || y < -cell || x > w + cell || y > h + cell) continue;
        const px = Math.min(cols - 1, Math.max(0, Math.floor(x / sx)));
        const py = Math.min(rows - 1, Math.max(0, Math.floor(y / sy)));
        const i = (py * cols + px) * 4;
        const l = (d[i] * 0.299 + d[i + 1] * 0.587 + d[i + 2] * 0.114) / 255;
        const ink = Math.pow(1 - l, p.gamma);
        if (ink < 0.04) continue;
        const r = cell * 0.62 * Math.sqrt(ink);
        ctx.beginPath();
        ctx.arc(x, y, r, 0, 6.2832);
        ctx.fill();
      }
    }
    ctx.restore();
  }
}

/** Straight blit plus horizontal scanlines and a soft vignette. */
export function scanlines(src, ctx, w, h) {
  ctx.clearRect(0, 0, w, h);
  ctx.drawImage(src, 0, 0, w, h);
  ctx.save();
  ctx.globalAlpha = 0.16;
  ctx.fillStyle = '#000';
  for (let y = 0; y < h; y += 3) ctx.fillRect(0, y, w, 1.4);
  ctx.restore();
  const g = ctx.createRadialGradient(w / 2, h / 2, Math.min(w, h) * 0.28, w / 2, h / 2, Math.max(w, h) * 0.72);
  g.addColorStop(0, 'rgba(0,0,0,0)');
  g.addColorStop(1, 'rgba(0,0,0,0.55)');
  ctx.fillStyle = g;
  ctx.fillRect(0, 0, w, h);
}

/** Cheap bloom: blur the bright parts back over the frame with screen blend. */
let bloomBuf = null;
export function bloom(src, ctx, w, h, opts = {}) {
  ctx.clearRect(0, 0, w, h);
  ctx.drawImage(src, 0, 0, w, h);
  const bw = Math.max(1, Math.round(w / 5)), bh = Math.max(1, Math.round(h / 5));
  if (!bloomBuf || bloomBuf.canvas.width !== bw || bloomBuf.canvas.height !== bh) {
    const c = document.createElement('canvas');
    c.width = bw; c.height = bh;
    bloomBuf = c.getContext('2d');
  }
  bloomBuf.clearRect(0, 0, bw, bh);
  bloomBuf.filter = `brightness(${opts.brightness ?? 1.2}) contrast(${opts.contrast ?? 3.2}) blur(3px)`;
  bloomBuf.drawImage(src, 0, 0, bw, bh);
  bloomBuf.filter = 'none';
  ctx.save();
  ctx.globalCompositeOperation = 'screen';
  ctx.globalAlpha = opts.amount ?? 0.28;
  ctx.imageSmoothingEnabled = true;
  ctx.drawImage(bloomBuf.canvas, 0, 0, w, h);
  ctx.restore();
}

export const PASSES = { ascii, pixel, riso, scanlines, bloom };
