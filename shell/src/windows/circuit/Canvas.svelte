<script lang="ts">
  /**
   * The cutting mat. Drag a part to move it (Shift keeps it on one axis), drag the empty mat to
   * select with a box, Space+drag or the middle button to pan, wheel to zoom. Click a pin then
   * another to wire them; right-click anything for its menu. In power mode, click a switch to
   * flick it.
   */
  import { capName, pairVerdict, pinName } from '../../lib/circuit/checks';
  import { geom, type PartGeo, type PinGeo } from '../../lib/circuit/geometry';
  import type { Part, PartDef, PinDef } from '../../lib/circuit/types';
  import { splitKey } from '../../lib/circuit/types';
  import { t } from '../../lib/i18n';
  import { even, useCircuit } from './circuit.svelte';
  const lab = useCircuit();
  // Pattern ids are per window: two Circuits windows share one page.
  const uid = Math.random().toString(36).slice(2, 8);
  import { G, art, transformOf } from './looks';
  import { matMenu, openMenu, partMenu, pinMenu, wireMenu } from './menu';

  let svg: SVGSVGElement;
  let rubber = $state<[number, number] | null>(null);
  let tip = $state<{ x: number; y: number; title: string; lines: string[] } | null>(null);
  let box = $state<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  let panning = $state(false);
  let space = $state(false);

  type Drag =
    | { kind: 'pan'; x: number; y: number; tx: number; ty: number }
    | { kind: 'box'; x: number; y: number; add: boolean; moved: boolean }
    | {
        kind: 'parts';
        start: [number, number];
        origin: Map<Part, [number, number]>;
        moved: boolean;
        before: string;
        clicked: Part;
        toggle: boolean;
      };
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
    for (const { part, d } of items)
      for (const p of d.pins) {
        const key = `${part.id}.${p.id}`;
        m.set(key, key === from ? 'self' : pairVerdict(lab.snap, lab.defs, from, key));
      }
    return m;
  });
  const lit = (part: string) => !lab.hl || lab.hl.parts.includes(part);
  const litWire = (id: string) => !lab.hl || lab.hl.wires.includes(id);
  const netColor = (id: number) => lab.nets.nets.find((n) => n.id === id)?.color ?? '#fff';
  const selected = $derived(new Set(lab.selParts));
  const shortWires = $derived.by(() => {
    const sim = lab.sim;
    if (!sim?.shorts.size) return new Set<string>();
    return new Set(
      lab.doc.wires.filter((w) => sim.shorts.has(sim.nets.of.get(w.a)?.id ?? -1)).map((w) => w.id),
    );
  });

  // Wires a hovered menu item or fix would add (and remove).
  const ghost = $derived.by(() => {
    const opt =
      lab.preview ??
      (lab.fixPreview
        ? { wires: lab.fixPreview.add ?? [], addParts: lab.fixPreview.addParts ?? [] }
        : null);
    if (!opt) return null;
    const extra = new Map(opt.addParts.map((p) => [p.id, p]));
    const at = (key: string): PinGeo | undefined => {
      const [pid, pn] = splitKey(key);
      const part = lab.part(pid) ?? extra.get(pid);
      return part ? geom(part, lab.defs).pins[pn] : undefined;
    };
    const moves = (lab.fixPreview?.move ?? []).flatMap((m) => {
      const w = lab.doc.wires.find((x) => x.id === m.wire);
      if (!w) return [];
      const keep = w.a === m.from ? w.b : w.a;
      return [[keep, m.to] as [string, string]];
    });
    return {
      parts: opt.addParts.map((p) => geom(p, lab.defs)),
      lines: [...opt.wires, ...moves].flatMap(([a, b]) => {
        const pa = at(a);
        const pb = at(b);
        return pa && pb ? [{ x1: pa.x * G, y1: pa.y * G, x2: pb.x * G, y2: pb.y * G }] : [];
      }),
      gone: new Set([
        ...(lab.fixPreview?.remove ?? []),
        ...(lab.fixPreview?.move ?? []).map((m) => m.wire),
      ]),
    };
  });

  function toWorld(e: { clientX: number; clientY: number }): [number, number] {
    const r = svg.getBoundingClientRect();
    return [
      (e.clientX - r.left - lab.view.tx) / lab.view.k,
      (e.clientY - r.top - lab.view.ty) / lab.view.k,
    ];
  }

  function target(e: Event) {
    const t = e.target as Element;
    return {
      pin: t.closest('[data-pin]')?.getAttribute('data-pin') ?? undefined,
      part: t.closest('[data-part]')?.getAttribute('data-part') ?? undefined,
      wire: t.closest('[data-wire]')?.getAttribute('data-wire') ?? undefined,
    };
  }

  function onpointerdown(e: PointerEvent) {
    lab.menu = null;
    tip = null;
    if (e.button === 1 || (e.button === 0 && space)) {
      e.preventDefault();
      drag = { kind: 'pan', x: e.clientX, y: e.clientY, tx: lab.view.tx, ty: lab.view.ty };
      panning = true;
      svg.setPointerCapture(e.pointerId);
      return;
    }
    if (e.button !== 0) return;
    const { pin, part, wire } = target(e);
    if (pin) {
      if (!lab.connecting) {
        lab.connecting = pin;
        lab.hint = t('circuit.canvas.connectHint', { pin: pinName(lab.snap, lab.defs, pin) });
        rubber = toWorld(e);
        return;
      }
      if (pin === lab.connecting) return stopConnecting();
      if (pairVerdict(lab.snap, lab.defs, lab.connecting, pin) === 'bad') {
        lab.say(t('circuit.canvas.badPin'));
        return;
      }
      const from = lab.connecting;
      stopConnecting();
      lab.connectPins(from, pin);
      return;
    }
    if (lab.connecting) return stopConnecting();
    if (part) {
      const p = lab.part(part)!;
      const toggle = e.ctrlKey || e.metaKey;
      if (!toggle && !selected.has(p.id)) lab.select([p.id]);
      if (toggle) lab.toggle(p.id);
      // Everything selected moves, and what is plugged into a selected breadboard rides along.
      const ids = new Set(lab.selParts.length ? lab.selParts : [p.id]);
      for (const id of [...ids]) for (const r of lab.riders(id)) ids.add(r.id);
      const origin = new Map<Part, [number, number]>();
      for (const id of ids) {
        const q = lab.part(id);
        if (q) origin.set(q, [q.x, q.y]);
      }
      drag = {
        kind: 'parts',
        start: toWorld(e),
        origin,
        moved: false,
        before: JSON.stringify($state.snapshot(lab.doc)),
        clicked: p,
        toggle,
      };
      lab.dragging = new Set(ids);
      svg.setPointerCapture(e.pointerId);
      return;
    }
    if (wire) {
      lab.selectWire(wire);
      return;
    }
    const [wx, wy] = toWorld(e);
    drag = { kind: 'box', x: wx, y: wy, add: e.ctrlKey || e.metaKey || e.shiftKey, moved: false };
    svg.setPointerCapture(e.pointerId);
  }

  function onpointermove(e: PointerEvent) {
    if (lab.connecting) rubber = toWorld(e);
    if (!drag) return hover(e);
    if (drag.kind === 'pan') {
      lab.view = {
        ...lab.view,
        tx: drag.tx + e.clientX - drag.x,
        ty: drag.ty + e.clientY - drag.y,
      };
      return;
    }
    const [wx, wy] = toWorld(e);
    if (drag.kind === 'box') {
      if (Math.abs(wx - drag.x) + Math.abs(wy - drag.y) > 4 / lab.view.k) drag.moved = true;
      box = {
        x0: Math.min(drag.x, wx),
        y0: Math.min(drag.y, wy),
        x1: Math.max(drag.x, wx),
        y1: Math.max(drag.y, wy),
      };
      return;
    }
    let dx = (wx - drag.start[0]) / G;
    let dy = (wy - drag.start[1]) / G;
    // Shift: along one axis only, whichever the pointer has gone further on.
    if (e.shiftKey) {
      if (Math.abs(dx) > Math.abs(dy)) dy = 0;
      else dx = 0;
    }
    const sx = even(dx);
    const sy = even(dy);
    if (sx || sy) drag.moved = true;
    for (const [p, [ox, oy]] of drag.origin) {
      p.x = ox + sx;
      p.y = oy + sy;
    }
  }

  function onpointerup(e: PointerEvent) {
    const d = drag;
    drag = null;
    panning = false;
    lab.dragging = null;
    if (!d) return;
    if (d.kind === 'box') {
      if (d.moved && box) {
        const b = box;
        const inside = items
          .filter(
            ({ g }) => g.x0 * G >= b.x0 && g.x1 * G <= b.x1 && g.y0 * G >= b.y0 && g.y1 * G <= b.y1,
          )
          .map(({ part }) => part.id);
        lab.select(inside, d.add);
        if (inside.length) lab.say(t('circuit.say.selected', { count: lab.selParts.length }));
      } else if (!d.add) lab.clearSelection();
      box = null;
      return;
    }
    if (d.kind !== 'parts') return;
    if (d.moved) {
      lab.rememberState(d.before);
      lab.dropped(
        [...d.origin.keys()].filter((p) => lab.selParts.includes(p.id) || p === d.clicked),
      );
      return;
    }
    // A click without a move.
    if (lab.power && lab.defs(d.clicked.def)?.switch) {
      lab.flick(d.clicked.id);
      return;
    }
    if (!d.toggle && !e.shiftKey) lab.select([d.clicked.id]);
  }

  function oncontextmenu(e: MouseEvent) {
    e.preventDefault();
    tip = null;
    if (lab.connecting) stopConnecting();
    const { pin, part, wire } = target(e);
    if (pin) openMenu(lab, e.clientX, e.clientY, pinMenu(lab, pin));
    else if (part) {
      if (!selected.has(part)) lab.select([part]);
      openMenu(lab, e.clientX, e.clientY, partMenu(lab, part));
    } else if (wire) {
      lab.selectWire(wire);
      openMenu(lab, e.clientX, e.clientY, wireMenu(lab, wire));
    } else {
      const [wx, wy] = toWorld(e);
      openMenu(lab, e.clientX, e.clientY, matMenu(lab, [wx / G, wy / G]));
    }
  }

  function onwheel(e: WheelEvent) {
    e.preventDefault();
    const r = svg.getBoundingClientRect();
    // A trackpad's two-finger scroll pans; a wheel (or a pinch, which sends ctrlKey) zooms.
    if (!e.ctrlKey && Math.abs(e.deltaX) > 0 && e.deltaMode === 0 && Math.abs(e.deltaY) < 40) {
      lab.view = { ...lab.view, tx: lab.view.tx - e.deltaX, ty: lab.view.ty - e.deltaY };
      return;
    }
    lab.zoomAt(
      e.clientX - r.left,
      e.clientY - r.top,
      Math.exp(-e.deltaY * (e.ctrlKey ? 0.01 : 0.0015)),
    );
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
    const lines = describe(p);
    const sim = lab.sim;
    if (sim) {
      const v = sim.volts.get(pin);
      lines.unshift(
        v !== undefined && Math.abs(v) > 0.02
          ? t('circuit.pin.now', { volts: v.toFixed(2) })
          : t('circuit.pin.noVoltage'),
      );
    }
    tip = {
      x: Math.min(e.clientX - r.left + 14, r.width - 270),
      y: e.clientY - r.top + 14,
      title: `${part.name} ${p.label}`,
      lines,
    };
  }

  function describe(p: PinDef): string[] {
    const out: string[] = [];
    const extra = (...parts: (string | false | undefined)[]) => parts.filter(Boolean).join(' · ');
    if (p.kind === 'io')
      out.push(
        t('circuit.pin.io', { caps: (p.caps ?? []).map(capName).join(', '), logic: p.logic ?? 5 }),
      );
    else if (p.kind === 'supply')
      out.push(
        extra(
          t('circuit.pin.supply', { volts: p.volts ?? 0 }),
          !!p.maxMa && t('circuit.pin.upTo', { ma: p.maxMa }),
        ),
      );
    else if (p.kind === 'gnd') out.push(t('circuit.pin.gnd'));
    else if (p.kind === 'vcc' || p.kind === 'vin' || p.kind === 'power-in')
      out.push(
        extra(
          t('circuit.pin.powerIn', { min: p.min ?? 0, max: p.max ?? 0 }),
          !!p.ma && t('circuit.pin.about', { ma: p.ma }),
        ),
      );
    else if (p.kind === 'sig')
      out.push(
        extra(
          t('circuit.pin.needs', { caps: (p.needs ?? ['io']).map(capName).join(', ') }),
          !!p.drives && t('circuit.pin.outputs', { volts: p.drives }),
          p.optional && t('circuit.pin.optional'),
        ),
      );
    else if (p.kind === 'motor-out') out.push(t('circuit.pin.motorOut'));
    else if (p.kind === 'motor-in') out.push(t('circuit.pin.motorIn'));
    else if (p.kind === 'speaker-out') out.push(t('circuit.pin.speakerOut'));
    else if (p.kind === 'speaker-in') out.push(t('circuit.pin.speakerIn'));
    else if (p.kind === 'port') {
      const kind =
        p.port === 'usb'
          ? t('circuit.port.usb')
          : p.port === 'usbc-power'
            ? t('circuit.port.usbcPower')
            : p.port === 'hdmi'
              ? t('circuit.port.hdmi')
              : t('circuit.port.csi');
      out.push(
        extra(
          p.host ? t('circuit.pin.socket', { kind }) : t('circuit.pin.plug', { kind }),
          !!p.volts && t('circuit.pin.volts', { volts: p.volts ?? 0 }),
        ),
      );
    } else if (p.kind === 'pass') out.push(t('circuit.pin.pass'));
    else if (p.kind === 'hole') out.push(t('circuit.pin.hole'));
    else out.push(t('circuit.pin.other'));
    if (p.place) out.push(p.place);
    if (p.caution) out.push(p.caution);
    return out;
  }

  // Pin labels sit inside the part, beside the pin, reading away from the edge.
  function label(p: PinDef, gp: PinGeo, board: boolean) {
    if (p.hideLabel || p.kind === 'hole') return null;
    const text = p.pairLabel ?? p.label;
    const off = 5.5;
    const x = gp.x * G;
    const y = gp.y * G;
    const size = board ? 6 : 6.5;
    if (gp.d === 0) return { text, x, y: y + off, anchor: 'end', rotate: true, size };
    if (gp.d === 2) return { text, x, y: y - off, anchor: 'start', rotate: true, size };
    if (gp.d === 1) return { text, x: x - off, y, anchor: 'end', rotate: false, size };
    return { text, x: x + off, y, anchor: 'start', rotate: false, size };
  }

  // Small parts carry their name outside, away from their pins.
  function title(d: PartDef, g: PartGeo) {
    let x = ((g.x0 + g.x1) / 2) * G;
    let y = ((g.y0 + g.y1) / 2) * G;
    let anchor: 'middle' | 'start' | 'end' = 'middle';
    if (Math.min(g.w, g.h) < 16) {
      const ds = d.pins.map((p) => g.pins[p.id]!.d);
      const dom = [0, 1, 2, 3].sort(
        (a, b) => ds.filter((v) => v === b).length - ds.filter((v) => v === a).length,
      )[0]!;
      const away = (dom + 2) & 3;
      if (away === 0) y = g.y0 * G - 17;
      if (away === 2) y = g.y1 * G + 17;
      if (away === 1) [x, anchor] = [g.x1 * G + 6, 'start'];
      if (away === 3) [x, anchor] = [g.x0 * G - 6, 'end'];
    }
    return { x, y, anchor };
  }

  const BADGE: Record<string, { fill: string; text: () => string }> = {
    on: { fill: '#16a34a', text: () => t('circuit.badge.on') },
    source: { fill: '#16a34a', text: () => t('circuit.badge.source') },
    off: { fill: '#6b7280', text: () => t('circuit.badge.off') },
    over: { fill: '#dc2626', text: () => t('circuit.badge.over') },
    under: { fill: '#d97706', text: () => t('circuit.badge.under') },
    hot: { fill: '#ea580c', text: () => t('circuit.badge.hot') },
    tripped: { fill: '#dc2626', text: () => t('circuit.badge.tripped') },
    overload: { fill: '#dc2626', text: () => t('circuit.badge.overload') },
    burnt: { fill: '#7f1d1d', text: () => t('circuit.badge.burnt') },
  };
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.code === 'Space' && !(e.target as Element).matches('input, textarea, select')) {
      space = true;
      e.preventDefault();
    }
  }}
  onkeyup={(e) => {
    if (e.code === 'Space') space = false;
  }}
/>

<svg
  bind:this={svg}
  {@attach (el: Element) => {
    lab.el = el as unknown as HTMLElement;
    return () => (lab.el = null);
  }}
  class:panning
  class:space
  class:connecting={!!lab.connecting}
  role="application"
  aria-label={t('circuit.canvas.label')}
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
    <pattern id="mat-{uid}" width={20 * G} height={20 * G} patternUnits="userSpaceOnUse">
      <path
        d="M {20 * G} 0 L 0 0 0 {20 * G}"
        fill="none"
        stroke="var(--mat-major)"
        stroke-width="1.2"
      />
      {#each [4, 8, 12, 16] as u (u)}
        <path
          d="M {u * G} 0 V {20 * G} M 0 {u * G} H {20 * G}"
          stroke="var(--mat-minor)"
          stroke-width="1"
        />
      {/each}
    </pattern>
    <pattern id="mesh-{uid}" width="4" height="4" patternUnits="userSpaceOnUse"
      ><circle cx="2" cy="2" r="0.9" fill="#6f757c" /></pattern
    >
  </defs>
  <g transform="translate({lab.view.tx} {lab.view.ty}) scale({lab.view.k})">
    <rect x="-60000" y="-60000" width="120000" height="120000" fill="url(#mat-{uid})" />

    <!-- Breadboards and their holes first, so parts sit on them. -->
    {#each items as { part, d, g } (part.id)}
      {#if d.kind === 'breadboard'}
        <g data-part={part.id} class="part" opacity={lit(part.id) ? 1 : 0.18}>
          <g transform="translate({part.x * G} {part.y * G}) {transformOf(d, part.rot)}"
            >{@html art(d).replaceAll('url(#mesh)', `url(#mesh-${uid})`)}</g
          >
          {#if selected.has(part.id)}
            <rect
              x={g.x0 * G - 5}
              y={g.y0 * G - 5}
              width={g.w * G + 10}
              height={g.h * G + 10}
              rx="12"
              fill="none"
              stroke="var(--select)"
              stroke-width="2.5"
              stroke-dasharray="7 5"
            />
          {/if}
        </g>
        {#each d.pins as p (p.id)}
          {@const gp = g.pins[p.id]!}
          {@const key = `${part.id}.${p.id}`}
          <g data-pin={key} opacity={lit(part.id) ? 1 : 0.18}>
            <rect
              x={gp.x * G - 2.6}
              y={gp.y * G - 2.6}
              width="5.2"
              height="5.2"
              rx="1"
              fill={used.has(key) ? '#5d6066' : '#b9b4a8'}
            />
            <circle cx={gp.x * G} cy={gp.y * G} r="4.5" fill="transparent" />
          </g>
        {/each}
      {/if}
    {/each}
    {#each items as { part, d, g } (part.id)}
      {#if d.kind !== 'breadboard'}
        {@const st = lab.sim?.parts.get(part.id)}
        <g
          data-part={part.id}
          class="part"
          class:switch={!!lab.sim && !!d.switch}
          opacity={lit(part.id) ? 1 : 0.18}
        >
          {#if st === 'on' || st === 'source'}
            <rect
              x={g.x0 * G - 4}
              y={g.y0 * G - 4}
              width={g.w * G + 8}
              height={g.h * G + 8}
              rx="10"
              fill="#22c55e"
              opacity="0.2"
            />
          {:else if st === 'over' || st === 'tripped'}
            <rect
              x={g.x0 * G - 4}
              y={g.y0 * G - 4}
              width={g.w * G + 8}
              height={g.h * G + 8}
              rx="10"
              fill="#ef4444"
              opacity="0.3"
              class="pulse"
            />
          {/if}
          <g transform="translate({part.x * G} {part.y * G}) {transformOf(d, part.rot)}"
            >{@html art(d).replaceAll('url(#mesh)', `url(#mesh-${uid})`)}</g
          >
          {#if selected.has(part.id)}
            <rect
              x={g.x0 * G - 5}
              y={g.y0 * G - 5}
              width={g.w * G + 10}
              height={g.h * G + 10}
              rx="12"
              fill="none"
              stroke="var(--select)"
              stroke-width="2.5"
              stroke-dasharray="7 5"
            />
          {/if}
          <rect x={g.x0 * G} y={g.y0 * G} width={g.w * G} height={g.h * G} fill="transparent" />
        </g>
      {/if}
    {/each}

    {#each lab.paths as w (w.id)}
      {@const flow = lab.sim?.flow.get(w.id)}
      <g data-wire={w.id} opacity={litWire(w.id) ? (ghost?.gone.has(w.id) ? 0.35 : 1) : 0.15}>
        {#if w.failed}
          <path d={w.d} fill="none" stroke="#ef4444" stroke-width="2.5" stroke-dasharray="6 5" />
        {:else}
          <path
            d={w.d}
            fill="none"
            stroke="var(--mat)"
            stroke-width={w.width + 4}
            stroke-linecap="round"
            stroke-linejoin="round"
          />
          {#if w.kind === 'gnd' && w.color === '#202328'}
            <path
              d={w.d}
              fill="none"
              stroke="#9aa3ad"
              stroke-width={w.width + 1.6}
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          {/if}
          {#if lab.selWire === w.id}
            <path
              d={w.d}
              fill="none"
              stroke="var(--select)"
              stroke-width={w.width + 5}
              stroke-linecap="round"
              stroke-linejoin="round"
              opacity="0.6"
            />
          {/if}
          <path
            d={w.d}
            fill="none"
            stroke={shortWires.has(w.id) ? '#ef4444' : w.color}
            stroke-width={w.width}
            stroke-linecap="round"
            stroke-linejoin="round"
            class:pulse={shortWires.has(w.id)}
          />
          {#if w.kind === 'cable'}
            <path
              d={w.d}
              fill="none"
              stroke="#5f666e"
              stroke-width="1.2"
              stroke-dasharray="1 5"
              stroke-linecap="round"
            />
          {/if}
          {#if flow}
            <path
              d={w.d}
              fill="none"
              stroke="#fffbe6"
              stroke-width={Math.max(1.6, w.width - 1.2)}
              stroke-dasharray="2 9"
              stroke-linecap="round"
              class="flow"
              class:back={flow === -1}
            />
          {/if}
        {/if}
        <path d={w.d} fill="none" stroke="transparent" stroke-width="12" />
      </g>
    {/each}

    {#each lab.routing.junctions as j (`${j.x},${j.y},${j.net}`)}
      <circle
        cx={j.x * G}
        cy={j.y * G}
        r="4.6"
        fill={netColor(j.net)}
        stroke="var(--mat)"
        stroke-width="1.6"
        opacity={lab.hl ? 0.3 : 1}
      />
    {/each}

    <!-- Pins, their labels and part names, on top. -->
    {#each items as { part, d, g } (part.id)}
      {#if d.kind !== 'breadboard'}
        {@const on = lit(part.id)}
        {@const st = lab.sim?.parts.get(part.id)}
        {#each d.pins as p (p.id)}
          {@const gp = g.pins[p.id]!}
          {@const key = `${part.id}.${p.id}`}
          {@const v = verdicts?.get(key)}
          {@const l = label(p, gp, d.kind === 'board')}
          <g data-pin={key} opacity={v === 'bad' ? 0.3 : on ? 1 : 0.18}>
            {#if p.terminal}
              <circle
                cx={gp.x * G}
                cy={gp.y * G}
                r="6"
                fill="#d9dde2"
                stroke="#4a5058"
                stroke-width="1.5"
              />
              <line
                x1={gp.x * G - 3}
                y1={gp.y * G}
                x2={gp.x * G + 3}
                y2={gp.y * G}
                stroke="#4a5058"
                stroke-width="1.5"
              />
            {:else if p.kind === 'port'}
              <rect
                x={gp.x * G - 6}
                y={gp.y * G - 4}
                width="12"
                height="8"
                rx="1.5"
                fill={p.port === 'hdmi' ? '#3f4650' : p.port === 'csi' ? '#e9d8a6' : '#d9dde2'}
                stroke="#4a5058"
              />
            {:else if d.kind === 'board'}
              <rect x={gp.x * G - 4.2} y={gp.y * G - 4.2} width="8.4" height="8.4" fill="#0b0c0d" />
              <rect
                x={gp.x * G - 1.8}
                y={gp.y * G - 1.8}
                width="3.6"
                height="3.6"
                fill={used.has(key) ? '#e6c35c' : '#3a3d42'}
              />
            {:else}
              <rect
                x={gp.x * G - 3.4}
                y={gp.y * G - 3.4}
                width="6.8"
                height="6.8"
                rx="1"
                fill="#e2bd52"
                stroke="#8f6f17"
              />
            {/if}
            {#if v === 'ok'}<circle
                cx={gp.x * G}
                cy={gp.y * G}
                r="7"
                fill="none"
                stroke="#14b8a6"
                stroke-width="2.4"
              />{/if}
            {#if v === 'warn'}<circle
                cx={gp.x * G}
                cy={gp.y * G}
                r="7"
                fill="none"
                stroke="#eab308"
                stroke-width="2"
              />{/if}
            {#if v === 'self'}<circle
                cx={gp.x * G}
                cy={gp.y * G}
                r="8"
                fill="none"
                stroke="#fff"
                stroke-width="2.4"
              />{/if}
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
          {@const tt = title(d, g)}
          {@const w = Math.max(part.name.length * 6.6, d.name.length * 5.1) + 14}
          <g pointer-events="none" opacity={on ? 1 : 0.18}>
            <rect
              x={tt.anchor === 'middle'
                ? tt.x - w / 2
                : tt.anchor === 'start'
                  ? tt.x - 4
                  : tt.x - w + 4}
              y={tt.y - 15}
              width={w}
              height="30"
              rx="6"
              fill="#0d1014"
              fill-opacity="0.62"
            />
            <text
              x={tt.anchor === 'middle' ? tt.x : tt.anchor === 'start' ? tt.x + 3 : tt.x - 3}
              y={tt.y - 2}
              text-anchor={tt.anchor}
              class="pname">{part.name}</text
            >
            <text
              x={tt.anchor === 'middle' ? tt.x : tt.anchor === 'start' ? tt.x + 3 : tt.x - 3}
              y={tt.y + 10}
              text-anchor={tt.anchor}
              class="psub">{d.switch && part.open ? t('circuit.badge.open') : d.name}</text
            >
          </g>
        {/if}
        {@const real = lab.live.get(part.name)}
        {#if real}
          {@const ok = real === 'running'}
          {@const lt = t('circuit.live.badge', { state: real })}
          <g pointer-events="none">
            <title>{t('circuit.live.title', { state: real })}</title>
            <rect
              x={g.x0 * G - 6}
              y={g.y0 * G - 9}
              width={lt.length * 6.2 + 12}
              height="16"
              rx="8"
              fill={ok ? '#0e7490' : real === 'failed' ? '#dc2626' : '#6b7280'}
            />
            <text x={g.x0 * G + lt.length * 3.1} y={g.y0 * G + 3} text-anchor="middle" class="badge"
              >{lt}</text
            >
          </g>
        {/if}
        {#if st && BADGE[st]}
          {@const b = BADGE[st]!}
          {@const bt = b.text()}
          <g pointer-events="none">
            <rect
              x={g.x1 * G - bt.length * 6.2 - 6}
              y={g.y0 * G - 9}
              width={bt.length * 6.2 + 12}
              height="16"
              rx="8"
              fill={b.fill}
            />
            <text x={g.x1 * G - bt.length * 3.1} y={g.y0 * G + 3} text-anchor="middle" class="badge"
              >{bt}</text
            >
          </g>
        {/if}
      {/if}
    {/each}

    {#if ghost}
      {#each ghost.parts as g, i (i)}
        <rect
          x={g.x0 * G}
          y={g.y0 * G}
          width={g.w * G}
          height={g.h * G}
          rx="5"
          fill="var(--select)"
          fill-opacity="0.15"
          stroke="var(--select)"
          stroke-dasharray="6 4"
          stroke-width="2"
        />
      {/each}
      {#each ghost.lines as l, i (i)}
        <line
          x1={l.x1}
          y1={l.y1}
          x2={l.x2}
          y2={l.y2}
          stroke="var(--select)"
          stroke-width="2.4"
          stroke-dasharray="6 5"
        />
        <circle cx={l.x1} cy={l.y1} r="5" fill="var(--select)" /><circle
          cx={l.x2}
          cy={l.y2}
          r="5"
          fill="var(--select)"
        />
      {/each}
    {/if}

    {#if lab.connecting && rubber}
      {@const [pid, pn] = splitKey(lab.connecting)}
      {@const from = lab.part(pid) ? geom(lab.part(pid)!, lab.defs).pins[pn] : undefined}
      {#if from}
        <line
          x1={from.x * G}
          y1={from.y * G}
          x2={rubber[0]}
          y2={rubber[1]}
          stroke="#fff"
          stroke-width="2"
          stroke-dasharray="5 4"
          pointer-events="none"
        />
      {/if}
    {/if}
    {#if box}
      <rect
        x={box.x0}
        y={box.y0}
        width={box.x1 - box.x0}
        height={box.y1 - box.y0}
        fill="var(--select)"
        fill-opacity="0.12"
        stroke="var(--select)"
        stroke-width={1.5 / lab.view.k}
        stroke-dasharray="{6 / lab.view.k} {4 / lab.view.k}"
        pointer-events="none"
      />
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
    cursor: default;
    user-select: none;
    touch-action: none;
  }
  svg.space {
    cursor: grab;
  }
  svg.panning {
    cursor: grabbing;
  }
  svg.connecting {
    cursor: crosshair;
  }
  .part {
    cursor: move;
  }
  .part.switch {
    cursor: pointer;
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
  .badge {
    font-family: var(--font-ui);
    font-size: 9px;
    font-weight: 700;
    fill: #fff;
    letter-spacing: 0.04em;
  }
  .flow {
    animation: flow 0.9s linear infinite;
  }
  .flow.back {
    animation-direction: reverse;
  }
  @keyframes flow {
    to {
      stroke-dashoffset: -22;
    }
  }
  .pulse {
    animation: pulse 0.8s ease-in-out infinite alternate;
  }
  @keyframes pulse {
    from {
      opacity: 1;
    }
    to {
      opacity: 0.35;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .flow,
    .pulse {
      animation: none;
    }
  }
  .tip {
    position: absolute;
    pointer-events: none;
    z-index: 5;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-md);
    padding: 6px 9px;
    font-size: 12px;
    box-shadow: var(--shadow-pop);
    max-width: 260px;
    display: flex;
    flex-direction: column;
  }
  .tip b {
    font-family: var(--font-data);
  }
  .tip span {
    color: var(--text-muted);
  }
</style>
