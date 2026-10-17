/**
 * The drawings of parts, as SVG markup in the part's own frame (before rotation and mirroring),
 * in px. Faber will ship these as SVG files beside the part files (D60); here they are code.
 */
import type { PartDef } from '../lib/types';

export const G = 8; // px per grid unit at zoom 1

const esc = (s: string) => s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);

const strips = (d: PartDef) =>
  (d.strips ?? []).map(([x0, y0, x1, y1]) => `<rect x="${x0 * G}" y="${y0 * G}" width="${(x1 - x0) * G}" height="${(y1 - y0) * G}" rx="2" fill="#121315"/>`).join('');

const UI = "font-family=\"'Atkinson Hyperlegible Next Variable', sans-serif\"";
const MONO = "font-family=\"'Atkinson Hyperlegible Mono Variable', monospace\"";

const board = (W: number, H: number, fill: string, stroke: string, rx = 5) => `<rect width="${W}" height="${H}" rx="${rx}" fill="${fill}" stroke="${stroke}" stroke-width="2"/>`;
const text = (x: number, y: number, t: string, size = 9, fill = '#e8eef6', anchor = 'middle', weight = 600) =>
  `<text x="${x}" y="${y}" text-anchor="${anchor}" ${UI} font-weight="${weight}" font-size="${size}" fill="${fill}">${esc(t)}</text>`;
const chip = (x: number, y: number, w: number, h: number) => `<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="1.5" fill="#141518"/>`;
const terminal = (x: number, y: number, w = G * 4, h = G * 4) => `<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="1" fill="#2f8f4e" stroke="#1d5e33"/>`;
const hole = (cx: number, cy: number, r = 5) => `<circle cx="${cx}" cy="${cy}" r="${r}" fill="#d8dde2" stroke="#0e1726" stroke-opacity=".4"/>`;

type Look = (d: PartDef, W: number, H: number) => string;

export const LOOKS: Record<string, Look> = {
  arduino: (d, W, H) => `
    ${board(W, H, '#0b7d84', '#065a5f', 10)}
    <rect x="2" y="${G * 6}" width="${G * 9}" height="${G * 8}" rx="2" fill="#c3c8ce" stroke="#8a9097"/>
    <rect x="2" y="${H * 0.64}" width="${G * 7}" height="${G * 5}" rx="3" fill="#17191c"/>
    ${[[0.07, 0.1], [0.07, 0.9], [0.68, 0.88], [0.62, 0.12]].map(([a, b]) => hole(W * a!, H * b!, 6)).join('')}
    <rect x="${W * 0.5 - 30}" y="${H * 0.5 - 30}" width="60" height="60" fill="#1a1c1f" transform="rotate(45 ${W * 0.5} ${H * 0.5})"/>
    ${text(W * 0.3, H * 0.5 + 7, d.title ?? '', d.w > 60 ? 22 : 18, '#e9fbfb', 'middle', 700)}
    ${strips(d)}`,
  nano: (_d, W, H) => `
    ${board(W, H, '#0b7d84', '#065a5f', 4)}
    <rect x="0" y="${H / 2 - G * 2.5}" width="${G * 4}" height="${G * 5}" rx="1.5" fill="#c3c8ce" stroke="#8a9097"/>
    ${chip(W * 0.45, H / 2 - G * 2.2, G * 4.4, G * 4.4)}
    ${text(W * 0.75, H / 2 + 4, 'NANO', 10, '#e9fbfb')}`,
  pi: (d, W, H) => `
    ${board(W, H, '#1d7a3c', '#145a2c', 12)}
    ${[[0.06, 0.08], [0.06, 0.92], [0.76, 0.08], [0.76, 0.92]].map(([a, b]) => `<circle cx="${W * a!}" cy="${H * b!}" r="7" fill="#e7c76a" stroke="#145a2c" stroke-width="2"/>`).join('')}
    <rect x="${W - G * 14}" y="${G * 2}" width="${G * 15}" height="${G * 14}" fill="#c9ced4" stroke="#8c939a"/>
    <rect x="${W - G * 14}" y="${G * 18}" width="${G * 15}" height="${G * 14}" fill="#c9ced4" stroke="#8c939a"/>
    <rect x="${W - G * 14}" y="${G * 34}" width="${G * 15}" height="${G * 9}" fill="#b4bac1" stroke="#8c939a"/>
    <rect x="${W * 0.36}" y="${H * 0.36}" width="${G * 12}" height="${G * 12}" rx="3" fill="#b9bec4"/>
    ${text(W * 0.08, H * 0.6, d.title ?? '', 17, '#f2fff5', 'start', 700)}
    ${strips(d)}`,
  breadboard: (_d, W, H) => {
    let s = `<rect width="${W}" height="${H}" rx="6" fill="#f3f1ec" stroke="#cfcac0" stroke-width="2"/>
      <rect x="0" y="${G * 22}" width="${W}" height="${G * 2}" fill="#dedad1"/>`;
    for (const [y, c] of [[3, '#d94141'], [7, '#3b6fd4'], [39, '#d94141'], [43, '#3b6fd4']] as const) {
      s += `<line x1="${G * 2}" y1="${y * G}" x2="${W - G * 2}" y2="${y * G}" stroke="${c}" stroke-width="1.5" opacity=".75"/>`;
    }
    const rows = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j'];
    const ys = [12, 14, 16, 18, 20, 26, 28, 30, 32, 34];
    rows.forEach((r, i) => {
      s += `<text x="${G * 1.6}" y="${ys[i]! * G}" ${MONO} font-size="7" fill="#8f897c" text-anchor="middle" dominant-baseline="central">${r}</text>`;
      s += `<text x="${W - G * 1.6}" y="${ys[i]! * G}" ${MONO} font-size="7" fill="#8f897c" text-anchor="middle" dominant-baseline="central">${r}</text>`;
    });
    for (let c = 0; c < 30; c += 1) {
      if (c === 0 || (c + 1) % 5 === 0) s += `<text x="${(4 + 2 * c) * G}" y="${G * 9.6}" ${MONO} font-size="7" fill="#8f897c" text-anchor="middle">${c + 1}</text>`;
    }
    return s;
  },
  sonar: (_d, W, H) => {
    const can = (cx: number) =>
      `<circle cx="${cx}" cy="${H * 0.46}" r="${G * 7.4}" fill="#c9ced4" stroke="#7e858c" stroke-width="2"/><circle cx="${cx}" cy="${H * 0.46}" r="${G * 5.6}" fill="#3b3f45"/><circle cx="${cx}" cy="${H * 0.46}" r="${G * 5.6}" fill="url(#mesh)"/>`;
    return `${board(W, H, '#1d4f9a', '#13386f')}${can(W * 0.24)}${can(W * 0.76)}<rect x="${W / 2 - G * 3}" y="${G * 2}" width="${G * 6}" height="${G * 3}" rx="2" fill="#d9dde2"/>`;
  },
  tof: (_d, W, H) => `${board(W, H, '#4b2f86', '#341f61')}${chip(W / 2 - G * 2.5, G * 2.5, G * 5, G * 3)}<circle cx="${W / 2 - G}" cy="${G * 4}" r="3" fill="#7a6aa8"/><circle cx="${W / 2 + G}" cy="${G * 4}" r="3" fill="#5b4b89"/>`,
  imu: (_d, W, H) => `${board(W, H, '#22408f', '#162b63')}${chip(W / 2 - G * 2.2, G * 2.4, G * 4.4, G * 4.4)}${hole(G * 2, G * 2, 4)}${hole(W - G * 2, G * 2, 4)}`,
  bts: (_d, W, H) => {
    let fins = '';
    for (let i = 0; i < 9; i++) fins += `<rect x="${G * 7 + i * G * 2.6}" y="${G * 4}" width="${G * 1.4}" height="${H - G * 8}" rx="2" fill="#b8bfc6"/>`;
    return `${board(W, H, '#26292e', '#16181b')}<rect x="${G * 6}" y="${G * 3}" width="${W - G * 11}" height="${H - G * 6}" rx="4" fill="#9aa2aa"/>${fins}
      ${[6, 12, 24, 30].map((y) => terminal(W - G * 4, y * G)).join('')}<rect x="0" y="${G * 11}" width="${G * 2.4}" height="${G * 16}" fill="#121315"/>`;
  },
  l298n: (_d, W, H) => {
    let fins = '';
    for (let i = 0; i < 7; i++) fins += `<rect x="${G * 9 + i * G * 2.4}" y="${G * 3}" width="${G * 1.2}" height="${G * 12}" rx="1" fill="#1c1d20"/>`;
    return `${board(W, H, '#a3322b', '#6e1f1a')}<rect x="${G * 8}" y="${G * 2}" width="${G * 18}" height="${G * 14}" rx="2" fill="#2a2c30"/>${fins}
      ${terminal(0, G * 8, G * 4, G * 10)}${terminal(W - G * 4, G * 8, G * 4, G * 10)}${terminal(G * 6, H - G * 4, G * 12, G * 4)}`;
  },
  motor: (_d, W, H) => `<rect x="0" y="${H * 0.18}" width="${W * 0.42}" height="${H * 0.64}" rx="6" fill="#b8bec5" stroke="#7e858c" stroke-width="2"/>
    <rect x="${W * 0.42}" y="${H * 0.08}" width="${W * 0.46}" height="${H * 0.84}" rx="4" fill="#c9a227" stroke="#8a6d12" stroke-width="2"/>
    <rect x="${W * 0.88}" y="${H * 0.44}" width="${W * 0.12}" height="${H * 0.12}" fill="#d9dde2" stroke="#7e858c"/>
    <rect x="0" y="${G * 6}" width="${G * 2}" height="${G * 10}" fill="#d0d4d9"/>`,
  servo: (_d, W, H) => `<rect x="${W * 0.14}" y="0" width="${W * 0.72}" height="${H}" rx="3" fill="#2563c9" stroke="#183f80" stroke-width="2"/>
    <rect x="${W * 0.04}" y="${H * 0.3}" width="${W * 0.92}" height="${H * 0.4}" rx="2" fill="#2563c9" stroke="#183f80"/>
    <circle cx="${W * 0.66}" cy="${H * 0.5}" r="${H * 0.32}" fill="#f2f3f4" stroke="#b8bec5"/>`,
  button: (_d, W, H) => `<circle cx="${W / 2}" cy="${H / 2}" r="${W / 2 - 1}" fill="#2b2f35" stroke="#16181b" stroke-width="2"/><circle cx="${W / 2}" cy="${H / 2}" r="${W / 2 - 9}" fill="#d43a3a" stroke="#8f2020" stroke-width="2"/>`,
  led: (_d, W, H) => `${board(W, H, '#26292e', '#16181b', 4)}<circle cx="${W / 2}" cy="${H * 0.38}" r="${W * 0.32}" fill="#f2c037" stroke="#a37a0f" stroke-width="2"/><circle cx="${W / 2 - 4}" cy="${H * 0.33}" r="3" fill="#fff6d6"/>`,
  shifter: (_d, W, H) => `${board(W, H, '#1d4f9a', '#13386f', 3)}${[0.22, 0.42, 0.62, 0.82].map((x) => chip(W * x - 5, H / 2 - 4, 10, 8)).join('')}
    ${text(G, H / 2 - 7, 'LV', 6.5, '#dbe7ff', 'start')}${text(G, H / 2 + 12, 'HV', 6.5, '#dbe7ff', 'start')}`,
  buck: (_d, W, H) => `${board(W, H, '#1d4f9a', '#13386f', 4)}<circle cx="${W * 0.42}" cy="${H / 2}" r="${G * 5}" fill="#3a3d42" stroke="#1d1f22" stroke-width="2"/><circle cx="${W * 0.42}" cy="${H / 2}" r="${G * 2.4}" fill="#b07a43"/>
    <rect x="${W * 0.66}" y="${G * 3}" width="${G * 4}" height="${G * 4}" rx="1" fill="#2a59b5" stroke="#d9dde2"/>
    ${[4, 10].map((y) => terminal(0, y * G) + terminal(W - G * 4, y * G)).join('')}`,
  buckusb: (_d, W, H) => `${board(W, H, '#24272c', '#111')}<circle cx="${W * 0.45}" cy="${H / 2}" r="${G * 5}" fill="#3a3d42" stroke="#1d1f22" stroke-width="2"/>
    ${terminal(0, G * 4, G * 4, G * 10)}<rect x="${W - G * 3}" y="${H / 2 - G * 2}" width="${G * 3}" height="${G * 4}" rx="2" fill="#c9ced4" stroke="#8c939a"/>${text(W * 0.45, H / 2 + 3, '5.1 V', 8)}`,
  battery: (d, W, H) => {
    const s = Number(d.title ?? 3);
    let cells = '';
    const cw = (W - G * 8) / s;
    for (let i = 0; i < s; i++) {
      cells += `<rect x="${G * 3 + i * cw}" y="${G * 3}" width="${cw - G}" height="${H - G * 6}" rx="${Math.min(G * 2, (cw - G) / 2)}" fill="#2a5db5" stroke="#173a75" stroke-width="2"/>${text(G * 3 + i * cw + (cw - G) / 2, H / 2 + 5, `G${i + 1}`, 12, '#dbe7ff')}`;
    }
    return `${board(W, H, '#1b2a44', '#0f1a2c', 8)}${cells}${terminal(W - G * 4, H / 2 - G * 5, G * 4, G * 10)}`;
  },
  bms: (_d, W, H) => `${board(W, H, '#1d4f9a', '#13386f')}${[0.3, 0.5, 0.7].map((x) => chip(W * x - G * 2, G * 3, G * 4, G * 3)).join('')}${text(W / 2, H - G * 3, 'BMS 3S', 9)}`,
  adapter: (_d, W, H) => `${board(W, H, '#2b2f35', '#16181b', 6)}<rect x="${G * 2}" y="${H / 2 - G * 3}" width="${G * 8}" height="${G * 6}" rx="2" fill="#16181b"/>${text(W * 0.62, H / 2 + 3, 'DC', 9)}`,
  to220: (_d, W, H) => `<rect x="${G}" y="0" width="${W - G * 2}" height="${H * 0.45}" rx="2" fill="#b8bfc6" stroke="#7e858c"/><circle cx="${W / 2}" cy="${H * 0.22}" r="${G * 1.4}" fill="#e8ebee"/>
    <rect x="${G}" y="${H * 0.45}" width="${W - G * 2}" height="${H * 0.4}" rx="1" fill="#1d1f22"/>`,
  relay: (_d, W, H) => `${board(W, H, '#1d1f22', '#0d0e10', 4)}${text(W / 2, H / 2 + 4, 'RELAY', 9)}`,
  fuse: (_d, W, H) => `<rect x="${G}" y="${H / 2 - G * 2}" width="${W - G * 2}" height="${G * 4}" rx="${G * 2}" fill="#2b2f35"/><rect x="${W * 0.3}" y="${H / 2 - G}" width="${W * 0.4}" height="${G * 2}" rx="${G}" fill="#e8f0f6" stroke="#9aa3ad"/>`,
  xt60: (_d, W, H) => `<rect x="0" y="0" width="${W / 2}" height="${H}" rx="3" fill="#e2b33a" stroke="#9a7616"/><rect x="${W / 2}" y="0" width="${W / 2}" height="${H}" rx="3" fill="#d6a527" stroke="#9a7616"/>`,
  estop: (_d, W, H) => `<rect width="${W}" height="${H}" rx="4" fill="#f3c623" stroke="#8a6d12" stroke-width="2"/><circle cx="${W / 2}" cy="${H / 2}" r="${W * 0.36}" fill="#c81e1e" stroke="#7f1111" stroke-width="2"/>`,
  switch: (_d, W, H) => `<rect width="${W}" height="${H}" rx="3" fill="#2b2f35" stroke="#16181b" stroke-width="2"/><rect x="${W / 2 - 2}" y="${G}" width="4" height="${H / 2}" rx="2" fill="#c9ced4"/>`,
  e18: (_d, W, H) => `<rect width="${W}" height="${H}" rx="${H / 2}" fill="#3a3d42" stroke="#1d1f22" stroke-width="2"/><circle cx="${W - G * 3}" cy="${H / 2}" r="${G * 2}" fill="#8b1e1e"/>`,
  gps: (_d, W, H) => `${board(W, H, '#1d4f9a', '#13386f')}<rect x="${W * 0.2}" y="${G * 2}" width="${W * 0.6}" height="${H * 0.6}" rx="2" fill="#d8b46a" stroke="#8a6d12"/>${chip(W / 2 - G * 2, G * 4, G * 4, G * 4)}`,
  rtc: (_d, W, H) => `${board(W, H, '#1d4f9a', '#13386f')}<circle cx="${W * 0.62}" cy="${H / 2}" r="${G * 6}" fill="#c9ced4" stroke="#7e858c" stroke-width="2"/>${chip(W * 0.2, H / 2 - G * 2, G * 4, G * 4)}`,
  mic: (_d, W, H) => `${board(W, H, '#a3322b', '#6e1f1a')}<circle cx="${W - G * 4}" cy="${H / 2}" r="${G * 3}" fill="#2b2f35"/><circle cx="${W * 0.5}" cy="${H / 2}" r="${G * 1.6}" fill="#2a59b5"/>`,
  irrx: (_d, W, H) => `<rect x="${G}" y="0" width="${W - G * 2}" height="${H * 0.7}" rx="3" fill="#1d1f22"/><circle cx="${W / 2}" cy="${H * 0.35}" r="${G * 1.8}" fill="#3a3d42"/>`,
  lidar: (_d, W, H) => `<rect width="${W}" height="${H}" rx="${G * 3}" fill="#2b2f35" stroke="#16181b" stroke-width="2"/><circle cx="${W / 2}" cy="${H * 0.45}" r="${W * 0.32}" fill="#1d1f22" stroke="#4b5058" stroke-width="2"/><rect x="${W / 2 - G * 3}" y="${H * 0.45 - G}" width="${G * 6}" height="${G * 2}" rx="1" fill="#8b1e1e"/>${text(W / 2, H - G * 3, 'LD19', 9)}`,
  cp2102: (_d, W, H) => `${board(W, H, '#a3322b', '#6e1f1a', 3)}<rect x="0" y="${H / 2 - G * 2.5}" width="${G * 6}" height="${G * 5}" rx="1" fill="#c9ced4" stroke="#8c939a"/>${chip(W * 0.45, H / 2 - G * 2, G * 4, G * 4)}`,
  camera: (_d, W, H) => `${board(W, H, '#1d7a3c', '#145a2c', 3)}<circle cx="${W / 2}" cy="${H * 0.42}" r="${G * 4}" fill="#1d1f22"/><circle cx="${W / 2}" cy="${H * 0.42}" r="${G * 2}" fill="#3a4a7a"/>`,
  nrf: (_d, W, H) => `${board(W, H, '#1d4f9a', '#13386f', 3)}<path d="M ${W * 0.5} ${G * 2} h ${W * 0.4} v ${G * 2} h -${W * 0.3} v ${G * 2} h ${W * 0.3}" fill="none" stroke="#e2bd52" stroke-width="2"/>${chip(W * 0.25, H / 2 - G * 2, G * 4, G * 4)}`,
  esp01: (_d, W, H) => `${board(W, H, '#1d1f22', '#0d0e10', 3)}<path d="M ${W * 0.55} ${G * 2} h ${W * 0.35} v ${G * 2} h -${W * 0.25} v ${G * 2} h ${W * 0.25}" fill="none" stroke="#e2bd52" stroke-width="2"/>${chip(W * 0.25, H / 2 - G * 2, G * 4, G * 4)}`,
  mux: (_d, W, H) => `${board(W, H, '#1d4f9a', '#13386f', 3)}${chip(W / 2 - G * 3, H / 2 - G * 3, G * 6, G * 6)}${text(W / 2, H - G * 2, 'I²C ×8', 8)}`,
  screen: (_d, W, H) => `<rect width="${W}" height="${H}" rx="6" fill="#16181b" stroke="#0a0b0c" stroke-width="2"/><rect x="${G * 3}" y="${G * 3}" width="${W - G * 6}" height="${H - G * 8}" rx="2" fill="#24435f"/>${text(W / 2, H / 2, '1024 × 600', 14, '#9fc0dd')}`,
  lcd: (_d, W, H) => `${board(W, H, '#1d4f9a', '#13386f', 3)}<rect x="${G * 4}" y="${G * 3}" width="${W - G * 8}" height="${H - G * 6}" rx="2" fill="#8fd14f" stroke="#2b2f35" stroke-width="2"/>`,
  keypad: (_d, W, H) => {
    let keys = '';
    for (let r = 0; r < 4; r++) for (let c = 0; c < 3; c++) keys += `<rect x="${G * 2 + c * (W - G * 4) / 3}" y="${G * 2 + r * (H - G * 8) / 4}" width="${(W - G * 4) / 3 - G}" height="${(H - G * 8) / 4 - G}" rx="2" fill="#e8ebee"/>`;
    return `<rect width="${W}" height="${H}" rx="4" fill="#2b2f35"/>${keys}`;
  },
  voltmeter: (_d, W, H) => `<rect width="${W}" height="${H}" rx="2" fill="#16181b"/>${text(W / 2, H / 2 + 5, '12.6', 14, '#ff4d4d')}`,
  dfplayer: (_d, W, H) => `${board(W, H, '#1d4f9a', '#13386f', 3)}<rect x="${W * 0.25}" y="${G * 3}" width="${W * 0.5}" height="${G * 6}" rx="1" fill="#c9ced4"/>${chip(W / 2 - G * 2, H - G * 8, G * 4, G * 4)}`,
  speaker: (_d, W, H) => `<circle cx="${W / 2}" cy="${H / 2}" r="${W / 2 - 1}" fill="#2b2f35" stroke="#16181b" stroke-width="2"/><circle cx="${W / 2}" cy="${H / 2}" r="${W / 4}" fill="#4b5058"/>`,
  amp: (_d, W, H) => `${board(W, H, '#a3322b', '#6e1f1a', 3)}${chip(W / 2 - G * 2, H / 2 - G * 2, G * 4, G * 4)}`,
  led3w: (_d, W, H) => `<polygon points="${W / 2},0 ${W},${H * 0.3} ${W},${H * 0.7} ${W / 2},${H} 0,${H * 0.7} 0,${H * 0.3}" fill="#c9ced4" stroke="#7e858c"/><circle cx="${W / 2}" cy="${H / 2}" r="${W * 0.22}" fill="#fff6d6" stroke="#e2bd52"/>`,
  irled: (_d, W, H) => `<rect x="${G}" y="0" width="${W - G * 2}" height="${H * 0.7}" rx="${(W - G * 2) / 2}" fill="#7a6aa8" opacity=".85"/>`,
  joystick: (_d, W, H) => `${board(W, H, '#1d1f22', '#0d0e10', 3)}<circle cx="${W * 0.6}" cy="${H / 2}" r="${G * 6}" fill="#3a3d42"/><circle cx="${W * 0.6}" cy="${H / 2}" r="${G * 3}" fill="#16181b"/>`,
  pot: (_d, W, H) => `<rect x="0" y="0" width="${W}" height="${H * 0.75}" rx="3" fill="#2a59b5"/><circle cx="${W / 2}" cy="${H * 0.38}" r="${G * 2.4}" fill="#e8ebee"/>`,
  strip: (_d, W, H) => {
    let leds = '';
    for (let i = 0; i < 5; i++) leds += `<rect x="${G * 5 + i * G * 7}" y="${H / 2 - G * 1.5}" width="${G * 3}" height="${G * 3}" fill="#f2f3f4" stroke="#b8bec5"/>`;
    return `<rect width="${W}" height="${H}" rx="2" fill="#1d1f22"/>${leds}`;
  },
  mb102: (_d, W, H) => `${board(W, H, '#1d4f9a', '#13386f', 4)}<rect x="${W - G * 6}" y="${G * 13}" width="${G * 6}" height="${G * 8}" rx="2" fill="#16181b"/><rect x="${W - G * 4}" y="${G * 27}" width="${G * 4}" height="${G * 6}" rx="1" fill="#c9ced4"/>${text(W / 2, H / 2 + 4, 'PSU', 9)}`,
  generic: (d, W, H) => {
    const img = d.art ? `<image href="${esc(d.art)}" x="0" y="0" width="${W}" height="${H}" preserveAspectRatio="none"/>` : '';
    return `${board(W, H, esc(d.color ?? '#1d4f9a'), '#0e1726')}${img}`;
  },
};

export function art(d: PartDef): string {
  return (LOOKS[d.look] ?? LOOKS.generic!)(d, d.w * G, d.h * G);
}

/** The transform that mirrors (in the part's own frame) and then turns the part. */
export function transformOf(d: PartDef, rot: number, flip?: boolean): string {
  let t = '';
  if (rot === 1) t = `translate(${d.h * G} 0) rotate(90)`;
  if (rot === 2) t = `translate(${d.w * G} ${d.h * G}) rotate(180)`;
  if (rot === 3) t = `translate(0 ${d.w * G}) rotate(270)`;
  if (flip) t += ` translate(${d.w * G} 0) scale(-1 1)`;
  return t;
}
