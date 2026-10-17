<script lang="ts">
  /**
   * The cutting mat: parts, wires, pins. Drag a part to move it (wires follow), drag the mat to
   * pan, wheel to zoom, click a pin then another to wire them, right-click anything for its menu.
   * Drop a part from the parts list to place it.
   */
  import { capName, pairVerdict, pinName } from '../lib/checks';
  import { geom, type PinGeo } from '../lib/geometry';
  import type { Part, PartDef, PinDef } from '../lib/types';
  import { splitKey } from '../lib/types';
  import { lab, even } from './lab.svelte';
  import { G, art, rotation } from './looks';
  import { matMenu, openMenu, partMenu, pinMenu, wireMenu } from './menu';

  let svg: SVGSVGElement;
  let rubber = $state<[number, number] | null>(null);
  let tip = $state<{ x: number; y: number; title: string; lines: string[] } | null>(null);
  let panning = $state(false);

  type Drag =
    | { kind: 'pan'; x: number; y: number; tx: number; ty: number; moved: boolean }
    | { kind: 'part'; part: Part; ox: number; oy: number; riders: { p: Part; dx: number; dy: number }[]; moved: boolean; before: string };
  let drag: Drag | null = null;

  const items = $derived(
    lab.doc.parts.flatMap((part) => {
      const d = lab.defs(part.def);
      return d ? [{ part, d, g: geom(part, lab.defs) }] : [];
    }),
  );
  const used = $derived.by(() => {
    const s = new Set<string>();
    for (const w of lab.doc.wires) {
      s.add(w.a);
      s.add(w.b);
    }
    for (const [p, h] of lab.nets.plugs) {
      s.add(p);
      s.add(h);
    }
    return s;
  });
  const verdicts = $derived.by(() => {
    const from = lab.connecting;
    if (!from) return null;
    const m = new Map<string, string>();
    for (const { part, d } of items) for (const p of d.pins) {
      const key = `${part.id}.${p.id}`;
      m.set(key, key === from ? 'self' : pairVerdict(lab.snap, lab.defs, from, key));
    }
    return m;
  });
  const lit = (part: string) => !lab.hl || lab.hl.parts.includes(part);
  const litWire = (id: string) => !lab.hl || lab.hl.wires.includes(id);
  const netColor = (id: number) => lab.nets.nets.find((n) => n.id === id)?.color ?? '#fff';
  const selPart = $derived(lab.sel && 'part' in lab.sel ? lab.sel.part : null);
  const selWire = $derived(lab.sel && 'wire' in lab.sel ? lab.sel.wire : null);

  // Wires a hovered menu item would add.
  const ghost = $derived.by(() => {
    const opt = lab.preview;
    if (!opt) return null;
    const extra = new Map(opt.addParts.map((p) => [p.id, p]));
    const at = (key: string): PinGeo | undefined => {
      const [pid, pn] = splitKey(key);
      const part = lab.part(pid) ?? extra.get(pid);
      return part ? geom(part, lab.defs).pins[pn] : undefined;
    };
    return {
      parts: opt.addParts.map((p) => geom(p, lab.defs)),
      lines: opt.wires.flatMap(([a, b]) => {
        const pa = at(a);
        const pb = at(b);
        return pa && pb ? [{ x1: pa.x * G, y1: pa.y * G, x2: pb.x * G, y2: pb.y * G }] : [];
      }),
    };
  });

  function toWorld(e: { clientX: number; clientY: number }): [number, number] {
    const r = svg.getBoundingClientRect();
    return [(e.clientX - r.left - lab.view.tx) / lab.view.k, (e.clientY - r.top - lab.view.ty) / lab.view.k];
  }

  function target(e: Event) {
    const t = e.target as Element;
    const pin = t.closest('[data-pin]')?.getAttribute('data-pin');
    const part = t.closest('[data-part]')?.getAttribute('data-part');
    const wire = t.closest('[data-wire]')?.getAttribute('data-wire');
    return { pin, part, wire };
  }

  function onpointerdown(e: PointerEvent) {
    if (e.button !== 0) return;
    lab.menu = null;
    tip = null;
    const { pin, part, wire } = target(e);
    if (pin) {
      if (!lab.connecting) {
        lab.connecting = pin;
        lab.hint = `Click the pin to connect ${pinName(lab.snap, lab.defs, pin)} to. Esc cancels.`;
        rubber = toWorld(e);
        return;
      }
      if (pin === lab.connecting) {
        stopConnecting();
        return;
      }
      if (pairVerdict(lab.snap, lab.defs, lab.connecting, pin) === 'bad') {
        lab.say('That would harm a part, or can\'t work. Pick a glowing pin.');
        return;
      }
      const from = lab.connecting;
      stopConnecting();
      lab.connectPins(from, pin);
      return;
    }
    if (lab.connecting) {
      stopConnecting();
      return;
    }
    if (part) {
      const p = lab.part(part)!;
      const [wx, wy] = toWorld(e);
      const riders = lab.defs(p.def)?.kind === 'breadboard' ? lab.riders(p.id).map((r) => ({ p: r, dx: r.x - p.x, dy: r.y - p.y })) : [];
      drag = { kind: 'part', part: p, ox: wx / G - p.x, oy: wy / G - p.y, riders, moved: false, before: JSON.stringify($state.snapshot(lab.doc)) };
      svg.setPointerCapture(e.pointerId);
      return;
    }
    if (wire) {
      lab.sel = { wire };
      lab.hl = null;
      lab.tab = 'sel';
      return;
    }
    drag = { kind: 'pan', x: e.clientX, y: e.clientY, tx: lab.view.tx, ty: lab.view.ty, moved: false };
    panning = true;
    svg.setPointerCapture(e.pointerId);
  }

  function onpointermove(e: PointerEvent) {
    if (lab.connecting) rubber = toWorld(e);
    if (!drag) {
      hover(e);
      return;
    }
    if (drag.kind === 'pan') {
      const dx = e.clientX - drag.x;
      const dy = e.clientY - drag.y;
      if (Math.abs(dx) + Math.abs(dy) > 3) drag.moved = true;
      lab.view = { ...lab.view, tx: drag.tx + dx, ty: drag.ty + dy };
      return;
    }
    const [wx, wy] = toWorld(e);
    const nx = even(wx / G - drag.ox);
    const ny = even(wy / G - drag.oy);
    if (nx !== drag.part.x || ny !== drag.part.y) {
      drag.moved = true;
      drag.part.x = nx;
      drag.part.y = ny;
      for (const r of drag.riders) {
        r.p.x = nx + r.dx;
        r.p.y = ny + r.dy;
      }
    }
  }

  function onpointerup() {
    const d = drag;
    drag = null;
    panning = false;
    if (!d) return;
    if (d.kind === 'pan') {
      if (!d.moved) {
        lab.sel = null;
        lab.hl = null;
      }
      return;
    }
    if (d.moved) {
      lab.rememberState(d.before);
      lab.dropped(d.part);
    } else {
      lab.sel = { part: d.part.id };
      lab.hl = null;
      lab.tab = 'sel';
    }
  }

  function oncontextmenu(e: MouseEvent) {
    e.preventDefault();
    tip = null;
    if (lab.connecting) stopConnecting();
    const { pin, part, wire } = target(e);
    if (pin) {
      openMenu(e.clientX, e.clientY, pinMenu(pin));
    } else if (part) {
      lab.sel = { part };
      openMenu(e.clientX, e.clientY, partMenu(part));
    } else if (wire) {
      lab.sel = { wire };
      openMenu(e.clientX, e.clientY, wireMenu(wire));
    } else {
      const [wx, wy] = toWorld(e);
      openMenu(e.clientX, e.clientY, matMenu([wx / G, wy / G]));
    }
  }

  function onwheel(e: WheelEvent) {
    e.preventDefault();
    const r = svg.getBoundingClientRect();
    lab.zoomAt(e.clientX - r.left, e.clientY - r.top, Math.exp(-e.deltaY * 0.0015));
  }

  function ondragover(e: DragEvent) {
    if (e.dataTransfer?.types.includes('text/x-faber-part')) e.preventDefault();
  }

  function ondrop(e: DragEvent) {
    const id = e.dataTransfer?.getData('text/x-faber-part');
    if (!id) return;
    e.preventDefault();
    const [wx, wy] = toWorld(e);
    lab.addPart(id, [wx / G, wy / G]);
  }

  function stopConnecting() {
    lab.connecting = null;
    rubber = null;
    lab.hint = '';
  }

  function hover(e: PointerEvent) {
    const { pin } = target(e);
    if (!pin) {
      tip = null;
      return;
    }
    const [pid, pn] = splitKey(pin);
    const part = lab.part(pid);
    const d = part ? lab.defs(part.def) : undefined;
    const p = d?.pins.find((x) => x.id === pn);
    if (!part || !p) return;
    const r = svg.getBoundingClientRect();
    tip = { x: e.clientX - r.left + 14, y: e.clientY - r.top + 14, title: `${part.name} ${p.label}`, lines: describe(p) };
  }

  function describe(p: PinDef): string[] {
    const out: string[] = [];
    if (p.kind === 'io') out.push(`${(p.caps ?? []).map(capName).join(', ')} · ${p.logic} V logic`);
    else if (p.kind === 'supply') out.push(`${p.volts} V out`);
    else if (p.kind === 'gnd') out.push('Ground');
    else if (p.kind === 'vcc' || p.kind === 'vin' || p.kind === 'power-in') out.push(`Power in, ${p.min}–${p.max} V`);
    else if (p.kind === 'sig') out.push(`Needs ${(p.needs ?? ['io']).map(capName).join(', ')}${p.drives ? ` · outputs ${p.drives} V` : ''}${p.optional ? ' · optional' : ''}`);
    else if (p.kind === 'motor-out') out.push('To the motor');
    else if (p.kind === 'motor-in') out.push('From a motor driver');
    else if (p.kind === 'usb-host') out.push('USB port');
    else if (p.kind === 'usb-device') out.push('USB, to a computer or the Pi');
    else if (p.kind === 'hole') out.push('Breadboard hole');
    else out.push('Not for wiring');
    if (p.place) out.push(p.place);
    return out;
  }

  // Pin labels sit inside the part, beside the pin, reading away from the edge.
  function label(p: PinDef, gp: PinGeo, board: boolean) {
    if (p.hideLabel || p.kind === 'hole') return null;
    const text = p.pairLabel ?? p.label;
    const off = 5.5;
    const x = gp.x * G;
    const y = gp.y * G;
    const dir = p.jog && p.pairLabel ? 1 : gp.d;
    const size = board ? 6 : 6.5;
    if (dir === 0) return { text, x, y: y + off, anchor: 'end', rotate: true, size };
    if (dir === 2) return { text, x, y: y - off, anchor: 'start', rotate: true, size };
    if (dir === 1) return { text, x: x - off, y, anchor: 'end', rotate: false, size };
    return { text, x: x + off, y, anchor: 'start', rotate: false, size };
  }

  // Small parts carry their name outside, away from their pins.
  function title(d: PartDef, g: ReturnType<typeof geom>) {
    let x = ((g.x0 + g.x1) / 2) * G;
    let y = ((g.y0 + g.y1) / 2) * G;
    let anchor: 'middle' | 'start' | 'end' = 'middle';
    if (Math.min(g.w, g.h) < 16) {
      const ds = d.pins.map((p) => g.pins[p.id]!.d);
      const dom = [0, 1, 2, 3].sort((a, b) => ds.filter((v) => v === b).length - ds.filter((v) => v === a).length)[0]!;
      const away = (dom + 2) & 3;
      if (away === 0) y = g.y0 * G - 17;
      if (away === 2) y = g.y1 * G + 17;
      if (away === 1) [x, anchor] = [g.x1 * G + 6, 'start'];
      if (away === 3) [x, anchor] = [g.x0 * G - 6, 'end'];
    }
    return { x, y, anchor };
  }
</script>

<svg
  id="canvas"
  bind:this={svg}
  class:panning
  class:connecting={!!lab.connecting}
  role="application"
  aria-label="Wiring diagram"
  {onpointerdown}
  {onpointermove}
  {onpointerup}
  {oncontextmenu}
  {onwheel}
  {ondragover}
  {ondrop}
  onpointerleave={() => (tip = null)}
>
  <defs>
    <pattern id="mat" width={20 * G} height={20 * G} patternUnits="userSpaceOnUse">
      <path d="M {20 * G} 0 L 0 0 0 {20 * G}" fill="none" stroke="var(--mat-major)" stroke-width="1.2" />
      {#each [4, 8, 12, 16] as u (u)}
        <path d="M {u * G} 0 V {20 * G} M 0 {u * G} H {20 * G}" stroke="var(--mat-minor)" stroke-width="1" />
      {/each}
    </pattern>
    <pattern id="mesh" width="4" height="4" patternUnits="userSpaceOnUse"><circle cx="2" cy="2" r="0.9" fill="#6f757c" /></pattern>
  </defs>
  <g transform="translate({lab.view.tx} {lab.view.ty}) scale({lab.view.k})">
    <rect x="-60000" y="-60000" width="120000" height="120000" fill="url(#mat)" />

    <!-- Breadboards and their holes first, so parts sit on them. -->
    {#each items as { part, d, g } (part.id)}
      {#if d.kind === 'breadboard'}
        <g data-part={part.id} class="part" opacity={lit(part.id) ? 1 : 0.18}>
          <g transform="translate({part.x * G} {part.y * G}) {rotation(d, part.rot)}">{@html art(d)}</g>
          {#if selPart === part.id}
            <rect x={g.x0 * G - 5} y={g.y0 * G - 5} width={g.w * G + 10} height={g.h * G + 10} rx="12" fill="none" stroke="var(--select)" stroke-width="2.5" stroke-dasharray="7 5" />
          {/if}
        </g>
        {#each d.pins as p (p.id)}
          {@const gp = g.pins[p.id]!}
          {@const key = `${part.id}.${p.id}`}
          <g data-pin={key} opacity={lit(part.id) ? 1 : 0.18}>
            <rect x={gp.x * G - 2.6} y={gp.y * G - 2.6} width="5.2" height="5.2" rx="1" fill={used.has(key) ? '#5d6066' : '#b9b4a8'} />
            <circle cx={gp.x * G} cy={gp.y * G} r="4.5" fill="transparent" />
          </g>
        {/each}
      {/if}
    {/each}
    {#each items as { part, d, g } (part.id)}
      {#if d.kind !== 'breadboard'}
        <g data-part={part.id} class="part" opacity={lit(part.id) ? 1 : 0.18}>
          <g transform="translate({part.x * G} {part.y * G}) {rotation(d, part.rot)}">{@html art(d)}</g>
          {#if selPart === part.id}
            <rect x={g.x0 * G - 5} y={g.y0 * G - 5} width={g.w * G + 10} height={g.h * G + 10} rx="12" fill="none" stroke="var(--select)" stroke-width="2.5" stroke-dasharray="7 5" />
          {/if}
          <rect x={g.x0 * G} y={g.y0 * G} width={g.w * G} height={g.h * G} fill="transparent" />
        </g>
      {/if}
    {/each}

    {#each lab.paths as w (w.id)}
      <g data-wire={w.id} opacity={litWire(w.id) ? 1 : 0.15}>
        {#if w.failed}
          <path d={w.d} fill="none" stroke="#ef4444" stroke-width="2.5" stroke-dasharray="6 5" />
        {:else}
          <path d={w.d} fill="none" stroke="var(--mat)" stroke-width={w.width + 4} stroke-linecap="round" stroke-linejoin="round" />
          {#if w.kind === 'gnd' && w.color === '#202328'}
            <path d={w.d} fill="none" stroke="#9aa3ad" stroke-width={w.width + 1.6} stroke-linecap="round" stroke-linejoin="round" />
          {/if}
          {#if selWire === w.id}
            <path d={w.d} fill="none" stroke="var(--select)" stroke-width={w.width + 5} stroke-linecap="round" stroke-linejoin="round" opacity="0.6" />
          {/if}
          <path d={w.d} fill="none" stroke={w.color} stroke-width={w.width} stroke-linecap="round" stroke-linejoin="round" />
          {#if w.kind === 'cable'}
            <path d={w.d} fill="none" stroke="#5f666e" stroke-width="1.2" stroke-dasharray="1 5" stroke-linecap="round" />
          {/if}
        {/if}
        <path d={w.d} fill="none" stroke="transparent" stroke-width="12" />
      </g>
    {/each}

    {#each lab.routing.junctions as j (`${j.x},${j.y},${j.net}`)}
      <circle cx={j.x * G} cy={j.y * G} r="4.6" fill={netColor(j.net)} stroke="var(--mat)" stroke-width="1.6" opacity={lab.hl ? 0.3 : 1} />
    {/each}

    <!-- Pins, their labels and part names, on top. -->
    {#each items.filter((i) => i.d.kind !== 'breadboard') as { part, d, g } (part.id)}
      {@const on = lit(part.id)}
      {#each d.pins as p (p.id)}
        {@const gp = g.pins[p.id]!}
        {@const key = `${part.id}.${p.id}`}
        {@const v = verdicts?.get(key)}
        {@const l = label(p, gp, d.kind === 'board')}
        <g data-pin={key} opacity={v === 'bad' ? 0.3 : on ? 1 : 0.18}>
          {#if p.terminal}
            <circle cx={gp.x * G} cy={gp.y * G} r="6" fill="#d9dde2" stroke="#4a5058" stroke-width="1.5" />
            <line x1={gp.x * G - 3} y1={gp.y * G} x2={gp.x * G + 3} y2={gp.y * G} stroke="#4a5058" stroke-width="1.5" />
          {:else if p.kind === 'usb-host' || p.kind === 'usb-device'}
            <rect x={gp.x * G - 6} y={gp.y * G - 4} width="12" height="8" rx="1.5" fill="#d9dde2" stroke="#4a5058" />
          {:else if d.kind === 'board'}
            <rect x={gp.x * G - 4.2} y={gp.y * G - 4.2} width="8.4" height="8.4" fill="#0b0c0d" />
            <rect x={gp.x * G - 1.8} y={gp.y * G - 1.8} width="3.6" height="3.6" fill={used.has(key) ? '#e6c35c' : '#3a3d42'} />
          {:else}
            <rect x={gp.x * G - 3.4} y={gp.y * G - 3.4} width="6.8" height="6.8" rx="1" fill="#e2bd52" stroke="#8f6f17" />
          {/if}
          {#if v === 'ok'}<circle cx={gp.x * G} cy={gp.y * G} r="7" fill="none" stroke="#5eead4" stroke-width="2.4" />{/if}
          {#if v === 'warn'}<circle cx={gp.x * G} cy={gp.y * G} r="7" fill="none" stroke="#facc15" stroke-width="2" />{/if}
          {#if v === 'self'}<circle cx={gp.x * G} cy={gp.y * G} r="8" fill="none" stroke="#fff" stroke-width="2.4" />{/if}
          <circle cx={gp.x * G} cy={gp.y * G} r="7" fill="transparent" />
        </g>
        {#if l}
          <text
            x={l.x}
            y={l.y}
            class="pinlabel"
            font-size={l.size}
            text-anchor={l.anchor}
            dominant-baseline="central"
            transform={l.rotate ? `rotate(-90 ${l.x} ${l.y})` : undefined}
            opacity={on ? 0.92 : 0.18}>{l.text}</text
          >
        {/if}
      {/each}
      {#if d.kind === 'part'}
        {@const t = title(d, g)}
        {@const w = Math.max(part.name.length * 6.6, d.name.length * 5.1) + 14}
        <g pointer-events="none" opacity={on ? 1 : 0.18}>
          <rect x={t.anchor === 'middle' ? t.x - w / 2 : t.anchor === 'start' ? t.x - 4 : t.x - w + 4} y={t.y - 15} width={w} height="30" rx="6" fill="#0d1014" fill-opacity="0.62" />
          <text x={t.anchor === 'middle' ? t.x : t.anchor === 'start' ? t.x + 3 : t.x - 3} y={t.y - 2} text-anchor={t.anchor} class="pname">{part.name}</text>
          <text x={t.anchor === 'middle' ? t.x : t.anchor === 'start' ? t.x + 3 : t.x - 3} y={t.y + 10} text-anchor={t.anchor} class="psub">{d.name}</text>
        </g>
      {/if}
    {/each}

    {#if ghost}
      {#each ghost.parts as g, i (i)}
        <rect x={g.x0 * G} y={g.y0 * G} width={g.w * G} height={g.h * G} rx="5" fill="var(--select)" fill-opacity="0.15" stroke="var(--select)" stroke-dasharray="6 4" stroke-width="2" />
      {/each}
      {#each ghost.lines as l, i (i)}
        <line x1={l.x1} y1={l.y1} x2={l.x2} y2={l.y2} stroke="var(--select)" stroke-width="2.4" stroke-dasharray="6 5" />
        <circle cx={l.x1} cy={l.y1} r="5" fill="var(--select)" /><circle cx={l.x2} cy={l.y2} r="5" fill="var(--select)" />
      {/each}
    {/if}

    {#if lab.connecting && rubber}
      {@const [pid, pn] = splitKey(lab.connecting)}
      {@const from = lab.part(pid) ? geom(lab.part(pid)!, lab.defs).pins[pn] : undefined}
      {#if from}
        <line x1={from.x * G} y1={from.y * G} x2={rubber[0]} y2={rubber[1]} stroke="#fff" stroke-width="2" stroke-dasharray="5 4" pointer-events="none" />
      {/if}
    {/if}
  </g>
</svg>

{#if tip}
  <div class="tip" style:left="{tip.x}px" style:top="{tip.y}px">
    <b>{tip.title}</b>
    {#each tip.lines as line, i (i)}<span>{line}</span>{/each}
  </div>
{/if}

<style>
  svg {
    width: 100%;
    height: 100%;
    display: block;
    cursor: grab;
    user-select: none;
    touch-action: none;
  }
  svg.panning {
    cursor: grabbing;
  }
  svg.connecting {
    cursor: crosshair;
  }
  .pinlabel {
    font-family: var(--font-data);
    fill: #f4f7f8;
    pointer-events: none;
  }
  .pname {
    font-family: var(--font-ui);
    font-size: 11.5px;
    font-weight: 700;
    fill: #fff;
  }
  .psub {
    font-family: var(--font-ui);
    font-size: 9px;
    fill: #d5dde3;
  }
  .tip {
    position: absolute;
    pointer-events: none;
    z-index: 5;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    padding: 6px 9px;
    font-size: 12px;
    box-shadow: var(--shadow);
    max-width: 260px;
    display: flex;
    flex-direction: column;
  }
  .tip b {
    font-family: var(--font-data);
  }
  .tip span {
    color: var(--muted);
  }
</style>
