/**
 * Colour maths for checking that every style is readable: parse the `oklch()` colours the tokens
 * are written in, and compute the WCAG 2 contrast ratio between two of them. A translucent
 * colour is first laid over the background it sits on, as the browser would.
 */

export interface Rgba {
  /** Gamma-encoded sRGB, 0..1. */
  r: number;
  g: number;
  b: number;
  a: number;
}

/** `oklch(L C H)` or `oklch(L C H / A)`. Throws on anything else: tokens are always written so. */
export function parseOklch(css: string): Rgba {
  const m = /^oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)\s*(?:\/\s*([\d.]+)\s*)?\)$/.exec(css.trim());
  if (!m) throw new Error(`Not an oklch() colour: ${css}`);
  const [l, c, h] = [Number(m[1]), Number(m[2]), Number(m[3])];
  const a = m[4] === undefined ? 1 : Number(m[4]);
  return { ...oklchToSrgb(l, c, h), a };
}

export function oklchToSrgb(l: number, c: number, h: number): { r: number; g: number; b: number } {
  const hr = (h * Math.PI) / 180;
  const a = c * Math.cos(hr);
  const b = c * Math.sin(hr);
  const l_ = (l + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m_ = (l - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s_ = (l - 0.0894841775 * a - 1.291485548 * b) ** 3;
  const lr = 4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_;
  const lg = -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_;
  const lb = -0.0041960863 * l_ - 0.7034186147 * m_ + 1.707614701 * s_;
  return { r: encode(lr), g: encode(lg), b: encode(lb) };
}

function encode(x: number): number {
  const v = Math.min(1, Math.max(0, x));
  return v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055;
}

function decode(v: number): number {
  return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
}

/** `top` laid over the opaque `bottom`. */
export function over(top: Rgba, bottom: Rgba): Rgba {
  const a = top.a;
  return {
    r: top.r * a + bottom.r * (1 - a),
    g: top.g * a + bottom.g * (1 - a),
    b: top.b * a + bottom.b * (1 - a),
    a: 1,
  };
}

export function luminance(c: Rgba): number {
  return 0.2126 * decode(c.r) + 0.7152 * decode(c.g) + 0.0722 * decode(c.b);
}

/**
 * WCAG contrast of `fg` on `bg` (1..21). Each argument may be a list, laid over each other from
 * the bottom up: `['oklch(…surface)', 'oklch(… / 0.17)']` is a selection tint on a surface.
 */
export function contrast(fg: string | string[], bg: string | string[]): number {
  const b = stack(bg);
  const f = over(stack(fg, b), b);
  const [hi, lo] = [luminance(f), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

function stack(layers: string | string[], base?: Rgba): Rgba {
  const list = Array.isArray(layers) ? layers : [layers];
  let out = base ?? { r: 1, g: 1, b: 1, a: 1 };
  let first = base === undefined;
  for (const l of list) {
    const c = parseOklch(l);
    out = first ? over(c, { r: 1, g: 1, b: 1, a: 1 }) : over(c, out);
    first = false;
  }
  return out;
}
