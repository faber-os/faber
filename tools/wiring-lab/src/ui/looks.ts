/**
 * The drawings of parts, as SVG markup in the part's own frame (before rotation), in px.
 * Faber will ship these as SVG files beside the part files (D60); here they are code.
 */
import type { PartDef } from '../lib/types';

export const G = 8; // px per grid unit at zoom 1

const esc = (s: string) => s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);

const strips = (d: PartDef) =>
  (d.strips ?? []).map(([x0, y0, x1, y1]) => `<rect x="${x0 * G}" y="${y0 * G}" width="${(x1 - x0) * G}" height="${(y1 - y0) * G}" rx="2" fill="#121315"/>`).join('');

const UI_FONT = "font-family=\"'Atkinson Hyperlegible Next Variable', sans-serif\"";
const DATA_FONT = "font-family=\"'Atkinson Hyperlegible Mono Variable', monospace\"";

type Look = (d: PartDef, W: number, H: number) => string;

export const LOOKS: Record<string, Look> = {
  arduino: (d, W, H) => `
    <rect width="${W}" height="${H}" rx="10" fill="#0b7d84" stroke="#065a5f" stroke-width="2"/>
    <rect x="2" y="${G * 6}" width="${G * 9}" height="${G * 8}" rx="2" fill="#c3c8ce" stroke="#8a9097"/>
    <rect x="2" y="${H * 0.64}" width="${G * 7}" height="${G * 5}" rx="3" fill="#17191c"/>
    ${[[0.07, 0.1], [0.07, 0.9], [0.68, 0.88], [0.62, 0.12]].map(([a, b]) => `<circle cx="${W * a!}" cy="${H * b!}" r="6" fill="#d8dde2" stroke="#065a5f" stroke-width="2"/>`).join('')}
    <rect x="${W * 0.5 - 30}" y="${H * 0.5 - 30}" width="60" height="60" fill="#1a1c1f" transform="rotate(45 ${W * 0.5} ${H * 0.5})"/>
    <text x="${W * 0.3}" y="${H * 0.5 + 7}" text-anchor="middle" ${UI_FONT} font-weight="700" font-size="${d.w > 60 ? 22 : 18}" fill="#e9fbfb" letter-spacing="1">${esc(d.title ?? '')}</text>
    ${strips(d)}`,
  pi: (d, W, H) => `
    <rect width="${W}" height="${H}" rx="12" fill="#1d7a3c" stroke="#145a2c" stroke-width="2"/>
    ${[[0.06, 0.08], [0.06, 0.92], [0.76, 0.08], [0.76, 0.92]].map(([a, b]) => `<circle cx="${W * a!}" cy="${H * b!}" r="7" fill="#e7c76a" stroke="#145a2c" stroke-width="2"/>`).join('')}
    <rect x="${W - G * 14}" y="${G * 2}" width="${G * 15}" height="${G * 14}" fill="#c9ced4" stroke="#8c939a"/>
    <rect x="${W - G * 14}" y="${G * 18}" width="${G * 15}" height="${G * 14}" fill="#c9ced4" stroke="#8c939a"/>
    <rect x="${W - G * 14}" y="${G * 34}" width="${G * 15}" height="${G * 9}" fill="#b4bac1" stroke="#8c939a"/>
    <rect x="${G * 10}" y="${H - G * 4}" width="${G * 10}" height="${G * 4}" rx="4" fill="#c9ced4" stroke="#8c939a"/>
    <rect x="${W * 0.36}" y="${H * 0.36}" width="${G * 12}" height="${G * 12}" rx="3" fill="#b9bec4"/>
    <text x="${W * 0.08}" y="${H * 0.6}" ${UI_FONT} font-weight="700" font-size="17" fill="#f2fff5">${esc(d.title ?? '')}</text>
    ${strips(d)}`,
  breadboard: (d, W, H) => {
    let s = `<rect width="${W}" height="${H}" rx="6" fill="#eceae4" stroke="#c9c5bb" stroke-width="2"/>
      <rect x="0" y="${G * 21}" width="${W}" height="${G * 2}" fill="#d9d5cc"/>`;
    for (const [y, c] of [[3, '#d94141'], [7, '#3b6fd4'], [37, '#d94141'], [41, '#3b6fd4']] as const) {
      s += `<line x1="${G * 2}" y1="${y * G}" x2="${W - G * 2}" y2="${y * G}" stroke="${c}" stroke-width="1.5" opacity=".75"/>`;
    }
    const rows = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j'];
    const ys = [12, 14, 16, 18, 20, 24, 26, 28, 30, 32];
    rows.forEach((r, i) => {
      s += `<text x="${G * 1.6}" y="${ys[i]! * G}" ${DATA_FONT} font-size="7" fill="#8f897c" text-anchor="middle" dominant-baseline="central">${r}</text>`;
      s += `<text x="${W - G * 1.6}" y="${ys[i]! * G}" ${DATA_FONT} font-size="7" fill="#8f897c" text-anchor="middle" dominant-baseline="central">${r}</text>`;
    });
    for (let c = 0; c < 30; c += 1) {
      if (c === 0 || (c + 1) % 5 === 0) s += `<text x="${(4 + 2 * c) * G}" y="${G * 9.6}" ${DATA_FONT} font-size="7" fill="#8f897c" text-anchor="middle">${c + 1}</text>`;
    }
    return s;
  },
  sonar: (_d, W, H) => {
    const can = (cx: number) =>
      `<circle cx="${cx}" cy="${H * 0.46}" r="${G * 7.4}" fill="#c9ced4" stroke="#7e858c" stroke-width="2"/><circle cx="${cx}" cy="${H * 0.46}" r="${G * 5.6}" fill="#3b3f45"/><circle cx="${cx}" cy="${H * 0.46}" r="${G * 5.6}" fill="url(#mesh)"/>`;
    return `<rect width="${W}" height="${H}" rx="5" fill="#1d4f9a" stroke="#13386f" stroke-width="2"/>${can(W * 0.24)}${can(W * 0.76)}
      <rect x="${W / 2 - G * 3}" y="${G * 2}" width="${G * 6}" height="${G * 3}" rx="2" fill="#d9dde2"/>`;
  },
  tof: (_d, W) => `<rect width="${W}" height="${14 * G}" rx="5" fill="#4b2f86" stroke="#341f61" stroke-width="2"/>
    <rect x="${W / 2 - G * 2.5}" y="${G * 2.5}" width="${G * 5}" height="${G * 3}" rx="2" fill="#141518"/><circle cx="${W / 2 - G}" cy="${G * 4}" r="3" fill="#7a6aa8"/><circle cx="${W / 2 + G}" cy="${G * 4}" r="3" fill="#5b4b89"/>`,
  imu: (_d, W, H) => `<rect width="${W}" height="${H}" rx="5" fill="#22408f" stroke="#162b63" stroke-width="2"/>
    <rect x="${W / 2 - G * 2.2}" y="${G * 2.4}" width="${G * 4.4}" height="${G * 4.4}" fill="#141518"/>
    <circle cx="${G * 2}" cy="${G * 2}" r="4" fill="#d9dde2"/><circle cx="${W - G * 2}" cy="${G * 2}" r="4" fill="#d9dde2"/>`,
  bts: (_d, W, H) => {
    let fins = '';
    for (let i = 0; i < 9; i++) fins += `<rect x="${G * 7 + i * G * 2.6}" y="${G * 4}" width="${G * 1.4}" height="${H - G * 8}" rx="2" fill="#b8bfc6"/>`;
    return `<rect width="${W}" height="${H}" rx="5" fill="#26292e" stroke="#16181b" stroke-width="2"/>
      <rect x="${G * 6}" y="${G * 3}" width="${W - G * 11}" height="${H - G * 6}" rx="4" fill="#9aa2aa"/>${fins}
      ${[6, 12, 24, 30].map((y) => `<rect x="${W - G * 4}" y="${y * G}" width="${G * 4}" height="${G * 4}" rx="1" fill="#2f8f4e"/>`).join('')}
      <rect x="0" y="${G * 11}" width="${G * 2.4}" height="${G * 16}" fill="#121315"/>`;
  },
  motor: (_d, W, H) => `<rect x="0" y="${H * 0.18}" width="${W * 0.42}" height="${H * 0.64}" rx="6" fill="#b8bec5" stroke="#7e858c" stroke-width="2"/>
    <rect x="${W * 0.42}" y="${H * 0.08}" width="${W * 0.46}" height="${H * 0.84}" rx="4" fill="#c9a227" stroke="#8a6d12" stroke-width="2"/>
    <rect x="${W * 0.88}" y="${H * 0.44}" width="${W * 0.12}" height="${H * 0.12}" fill="#d9dde2" stroke="#7e858c"/>
    <rect x="0" y="${G * 6}" width="${G * 2}" height="${G * 10}" fill="#d0d4d9"/>`,
  servo: (_d, W, H) => `<rect x="${W * 0.14}" y="0" width="${W * 0.72}" height="${H}" rx="3" fill="#2563c9" stroke="#183f80" stroke-width="2"/>
    <rect x="${W * 0.04}" y="${H * 0.3}" width="${W * 0.92}" height="${H * 0.4}" rx="2" fill="#2563c9" stroke="#183f80"/>
    <circle cx="${W * 0.66}" cy="${H * 0.5}" r="${H * 0.32}" fill="#f2f3f4" stroke="#b8bec5"/>
    <rect x="0" y="${G * 3}" width="${G * 2}" height="${G * 6}" fill="#121315"/>`,
  button: (_d, W, H) => `<circle cx="${W / 2}" cy="${H / 2}" r="${W / 2 - 1}" fill="#2b2f35" stroke="#16181b" stroke-width="2"/><circle cx="${W / 2}" cy="${H / 2}" r="${W / 2 - 9}" fill="#d43a3a" stroke="#8f2020" stroke-width="2"/>`,
  led: (_d, W, H) => `<rect width="${W}" height="${H}" rx="4" fill="#26292e" stroke="#16181b" stroke-width="2"/><circle cx="${W / 2}" cy="${H * 0.38}" r="${W * 0.32}" fill="#f2c037" stroke="#a37a0f" stroke-width="2"/><circle cx="${W / 2 - 4}" cy="${H * 0.33}" r="3" fill="#fff6d6"/>`,
  shifter: (_d, W, H) => `<rect width="${W}" height="${H}" rx="3" fill="#1d4f9a" stroke="#13386f" stroke-width="2" fill-opacity=".92"/>
    ${[0.22, 0.42, 0.62, 0.82].map((x) => `<rect x="${W * x - 5}" y="${H / 2 - 4}" width="10" height="8" rx="1" fill="#141518"/>`).join('')}
    <text x="${G}" y="${H / 2 - 7}" ${DATA_FONT} font-size="6.5" fill="#dbe7ff">LV</text><text x="${G}" y="${H / 2 + 12}" ${DATA_FONT} font-size="6.5" fill="#dbe7ff">HV</text>`,
  buck: (_d, W, H) => `<rect width="${W}" height="${H}" rx="4" fill="#1d4f9a" stroke="#13386f" stroke-width="2"/>
    <circle cx="${W * 0.42}" cy="${H / 2}" r="${G * 5}" fill="#3a3d42" stroke="#1d1f22" stroke-width="2"/><circle cx="${W * 0.42}" cy="${H / 2}" r="${G * 2.4}" fill="#b07a43"/>
    <rect x="${W * 0.66}" y="${G * 3}" width="${G * 4}" height="${G * 4}" rx="1" fill="#2a59b5" stroke="#d9dde2"/>
    ${[4, 10].map((y) => `<rect x="0" y="${y * G}" width="${G * 4}" height="${G * 4}" rx="1" fill="#2f8f4e"/><rect x="${W - G * 4}" y="${y * G}" width="${G * 4}" height="${G * 4}" rx="1" fill="#2f8f4e"/>`).join('')}`,
  battery: (_d, W, H) => {
    let cells = '';
    const cw = (W - G * 8) / 3;
    for (let i = 0; i < 3; i++) {
      cells += `<rect x="${G * 3 + i * cw}" y="${G * 3}" width="${cw - G}" height="${H - G * 6}" rx="${G * 2}" fill="#2a5db5" stroke="#173a75" stroke-width="2"/><text x="${G * 3 + i * cw + (cw - G) / 2}" y="${H / 2 + 5}" text-anchor="middle" ${DATA_FONT} font-size="13" fill="#dbe7ff">G${i + 1}</text>`;
    }
    return `<rect width="${W}" height="${H}" rx="8" fill="#1b2a44" stroke="#0f1a2c" stroke-width="2"/>${cells}<rect x="${W - G * 4}" y="${G * 8}" width="${G * 4}" height="${G * 10}" rx="1" fill="#2f8f4e"/>`;
  },
  generic: (d, W, H) => {
    const img = d.art ? `<image href="${esc(d.art)}" x="0" y="0" width="${W}" height="${H}" preserveAspectRatio="none"/>` : '';
    return `<rect width="${W}" height="${H}" rx="5" fill="${esc(d.color ?? '#1d4f9a')}" stroke="#0e1726" stroke-opacity=".5" stroke-width="2"/>${img}`;
  },
};

export function art(d: PartDef): string {
  return (LOOKS[d.look] ?? LOOKS.generic!)(d, d.w * G, d.h * G);
}

/** The transform that turns the part's own frame by its rotation. */
export function rotation(d: PartDef, rot: number): string {
  if (rot === 1) return `translate(${d.h * G} 0) rotate(90)`;
  if (rot === 2) return `translate(${d.w * G} ${d.h * G}) rotate(180)`;
  if (rot === 3) return `translate(0 ${d.w * G}) rotate(270)`;
  return '';
}
