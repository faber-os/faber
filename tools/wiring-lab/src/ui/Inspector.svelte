<script lang="ts">
  /** The right panel: what is selected (and its settings), the checks with their fixes, the build steps. */
  import { pinName } from '../lib/checks';
  import { GAUGE_ORDER, GAUGES, gaugeOf } from '../lib/circuit';
  import type { Gauge } from '../lib/types';
  import type { Fix } from '../lib/fixes';
  import { paramValues } from '../lib/library';
  import { splitKey } from '../lib/types';
  import { lab } from './lab.svelte';
  import { connectItems, openMenu, wireMenu, type MenuItem } from './menu';

  const MM = 1.27;
  const part = $derived(lab.selParts.length === 1 ? lab.part(lab.selParts[0]!) : undefined);
  const wire = $derived(lab.selWire ? lab.doc.wires.find((w) => w.id === lab.selWire) : undefined);
  const errors = $derived(lab.issues.filter((i) => i.sev === 'error').length);
  const warns = $derived(lab.issues.length - errors);
  const total = $derived(lab.steps.reduce((n, g) => n + g.steps.length, 0));
  const done = $derived(lab.steps.reduce((n, g) => n + g.steps.filter((s) => lab.doc.done[s.id]).length, 0));
  // Fixes are worked out when the Checks tab shows, one list per issue.
  const fixes = $derived(lab.tab === 'checks' ? lab.issues.map((_, i) => lab.fixes(i)) : []);

  function pinRows() {
    if (!part) return [];
    const d = lab.defs(part.def)!;
    return d.pins
      .map((p) => {
        const key = `${part.id}.${p.id}`;
        const ws = lab.doc.wires.filter((w) => w.a === key || w.b === key);
        const plug = lab.nets.plugs.find(([k]) => k === key);
        const to = [...ws.map((w) => pinName(lab.snap, lab.defs, w.a === key ? w.b : w.a)), ...(plug ? [`hole ${splitKey(plug[1])[1]}`] : [])];
        const color = ws[0] ? (ws[0].color ?? lab.nets.of.get(key)?.color) : plug ? '#e2bd52' : undefined;
        return { p, to, color };
      })
      .filter((r) => d.kind === 'part' || r.to.length);
  }

  function wireLength(id: string): number {
    const r = lab.routing.routes.get(id);
    if (!r) return 0;
    let n = 0;
    for (let i = 0; i < r.pts.length - 1; i++) n += Math.abs(r.pts[i + 1]![0] - r.pts[i]![0]) + Math.abs(r.pts[i + 1]![1] - r.pts[i]![1]);
    return Math.round(n * MM);
  }

  function showIssue(i: number) {
    const it = lab.issues[i]!;
    lab.hl = { parts: it.parts, wires: it.wires };
  }

  function chooseFix(e: MouseEvent, list: Fix[]) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const items: MenuItem[] = list.map((f) => ({ label: f.label, hint: f.hint, action: () => lab.fix(f) }));
    openMenu(r.left, r.bottom + 4, items);
  }

  const POWER_TEXT: Record<string, string> = { on: 'On', source: 'Power source', off: 'No power', over: 'Too much: it gets damaged', under: 'Too little voltage', hot: 'On, but hot', tripped: 'Cut the power', overload: 'Asked for more than it gives', burnt: 'Burns out' };

  const amps = (a: number | undefined) => (a === undefined ? '–' : Math.abs(a) < 1 ? `${Math.round(Math.abs(a) * 1000)} mA` : `${Math.abs(a).toFixed(2)} A`);
  const volts = (key: string) => {
    const v = lab.sim?.volts.get(key);
    return v === undefined ? '' : ` · ${v.toFixed(2)} V`;
  };
  function showFinding(parts: string[], wires: string[]) {
    lab.hl = { parts, wires };
  }
</script>

<div class="tabs" role="tablist">
  <button role="tab" aria-selected={lab.tab === 'sel'} onclick={() => ((lab.tab = 'sel'), (lab.hl = null))}>Selected</button>
  <button role="tab" aria-selected={lab.tab === 'checks'} onclick={() => (lab.tab = 'checks')}>
    Checks
    {#if errors}<span class="chip err">{errors}</span>{:else if warns}<span class="chip warn">{warns}</span>{:else}<span class="chip ok">✓</span>{/if}
  </button>
  <button role="tab" aria-selected={lab.tab === 'build'} onclick={() => (lab.tab = 'build')}>Build <span class="count">{done}/{total}</span></button>
</div>

<div class="body">
  {#if lab.tab === 'checks'}
    {#if lab.sim}
      <h2>With the power on</h2>
      <ul class="issues">
        {#each lab.sim.findings as f, i (i)}
          <li class="issue {f.sev}">
            <button class="txt" onclick={() => showFinding(f.parts, f.wires)}>{f.text}{#if f.why}<small>{f.why}</small>{/if}</button>
          </li>
        {:else}
          <li class="issue ok"><div class="txt">Every part gets what it needs.<small>Volts in range, no wire or pin over its current.</small></div></li>
        {/each}
      </ul>
      <h2>The wiring</h2>
    {/if}
    {#if !lab.issues.length}
      <ul class="issues"><li class="issue ok"><div class="txt">Every wire checks out.<small>Power, ground, pin types and voltages all match.</small></div></li></ul>
    {:else}
      {#if fixes.some((f) => f.length === 1)}
        <button class="btn primary wide" onclick={() => lab.fixAll()}>Fix everything with one clear fix</button>
      {/if}
      <ul class="issues">
        {#each lab.issues as it, i (it.code + it.text)}
          {@const list = fixes[i] ?? []}
          <li class="issue {it.sev}">
            <button class="txt" onclick={() => showIssue(i)}>{it.text}<small>{it.more}</small></button>
            {#if list.length === 1}
              <button
                class="fix"
                onpointerenter={() => (lab.fixPreview = list[0]!)}
                onpointerleave={() => (lab.fixPreview = null)}
                onfocus={() => (lab.fixPreview = list[0]!)}
                onblur={() => (lab.fixPreview = null)}
                onclick={() => lab.fix(list[0]!)}>Fix: {list[0]!.label.charAt(0).toLowerCase() + list[0]!.label.slice(1)}</button
              >
            {:else if list.length > 1}
              <button class="fix" onclick={(e) => chooseFix(e, list)}>{list.length} ways to fix it ▾</button>
            {/if}
          </li>
        {/each}
      </ul>
      <p class="note">Click a line to see it on the drawing; hover a fix to see what it changes. Every fix is tried before it is offered, and Undo takes it back.</p>
    {/if}
  {:else if lab.tab === 'build'}
    <div class="progress" role="progressbar" aria-valuenow={done} aria-valuemax={total}><div style:width="{total ? (done / total) * 100 : 0}%"></div></div>
    <p class="note"><b>{done} of {total} steps.</b> Unplug the battery and the USB cable before you start.</p>
    {#each lab.steps as g (g.title)}
      <h3><span class="t">{g.title}</span> <span class="s">{g.sub}</span></h3>
      <ul class="steps">
        {#each g.steps as s (s.id)}
          <li class:done={lab.doc.done[s.id]}>
            <input type="checkbox" checked={!!lab.doc.done[s.id]} aria-label="Done" onchange={(e) => lab.tick(s.id, e.currentTarget.checked)} />
            <button class="step" onclick={() => (lab.hl = { parts: s.parts, wires: s.wires })}>
              <i class="sw" style:background={s.color}></i>
              <span>{s.text}<small>{s.detail}</small></span>
            </button>
          </li>
        {/each}
      </ul>
    {/each}
    <p class="note">Then plug in USB. In Faber, each part is tested as soon as its steps are ticked: the sonar reads a distance, the motor turns a little (wheels off the ground).</p>
  {:else if lab.selParts.length > 1}
    <h2>{lab.selParts.length} parts selected</h2>
    <p class="note">{lab.selParts.map((id) => lab.part(id)?.name).join(', ')}</p>
    <div class="row">
      <button class="btn" onclick={() => lab.rotate()}>Rotate</button>
      <button class="btn" onclick={() => lab.mirror('h')}>Mirror ↔</button>
      <button class="btn" onclick={() => lab.mirror('v')}>Mirror ↕</button>
      <button class="btn" onclick={() => lab.duplicate()}>Duplicate</button>
      <button class="btn danger" onclick={() => lab.removeParts(lab.selParts)}>Remove</button>
    </div>
    <p class="note">Drag any of them to move them together; hold Shift to stay on one axis.</p>
  {:else if part}
    {@const d = lab.defs(part.def)!}
    {@const values = paramValues(part.def)}
    {@const st = lab.sim?.parts.get(part.id)}
    <label class="field">Name<input value={part.name} onchange={(e) => lab.rename(part.id, e.currentTarget.value)} /></label>
    <dl class="kv">
      <dt>Part</dt><dd>{d.name}</dd>
      <dt>Covers</dt><dd>{d.covers}</dd>
      <dt>Size</dt><dd class="mono">{(d.w * MM).toFixed(1)} × {(d.h * MM).toFixed(1)} mm</dd>
      {#if st}<dt>Power</dt><dd class="power {st}">{POWER_TEXT[st]}</dd>{/if}
      {#if lab.sim && lab.sim.partAmps.has(part.id)}<dt>Current</dt><dd class="mono">{amps(lab.sim.partAmps.get(part.id))}</dd>{/if}
    </dl>
    {#if d.params}
      <div class="params">
        <h2>Settings</h2>
        {#each d.params as p (p.id)}
          <label class="field"
            >{p.label}{p.unit ? ` (${p.unit})` : ''}
            <input type="number" min={p.min} max={p.max} step={p.step} value={values[p.id] ?? p.default} onchange={(e) => lab.setParam(part.id, p.id, Number(e.currentTarget.value))} />
          </label>
        {/each}
      </div>
    {/if}
    <div class="row">
      {#if d.kind !== 'breadboard'}
        <button class="btn primary" onclick={(e) => { const r = e.currentTarget.getBoundingClientRect(); openMenu(r.left, r.bottom + 4, connectItems(part.id)); }}>Connect to…</button>
        <button class="btn" onclick={() => lab.rotate([part.id])}>Rotate</button>
      {/if}
      {#if lab.power && d.switch}<button class="btn" onclick={() => lab.flick(part.id)}>{part.open ? 'Close' : d.switch === 'push' ? 'Press' : 'Open'}</button>{/if}
      <button class="btn danger" onclick={() => lab.removeParts([part.id])}>Remove</button>
    </div>
    {#if d.notes?.length}
      <div class="notes">{#each d.notes as n, i (i)}<p>{n}</p>{/each}</div>
    {/if}
    <div>
      <h2>{d.kind === 'part' ? 'Pins' : 'Pins in use'}</h2>
      <table class="pins">
        <tbody>
          {#each pinRows() as r (r.p.id)}
            <tr class:on={r.to.length}>
              <td class="pin">{r.p.label}</td>
              <td class="dotcell">{#if r.color}<i class="dot" style:background={r.color}></i>{/if}</td>
              <td class="to">{r.to.length ? r.to.join(', ') : r.p.optional ? 'free (optional)' : 'not connected'}{volts(`${part.id}.${r.p.id}`)}</td>
            </tr>
          {:else}
            <tr><td colspan="3" class="to">No wires yet. Right-click it, or click a pin, to start one.</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
    {#if d.kind === 'part'}<p class="note">Pin order is the module's own print as usually sold; a clone can differ, so check yours.</p>{/if}
  {:else if wire}
    {@const net = lab.nets.of.get(wire.a)}
    {@const flow = lab.sim?.flow.get(wire.id)}
    <dl class="kv">
      <dt>From</dt><dd>{pinName(lab.snap, lab.defs, wire.a)}</dd>
      <dt>To</dt><dd>{pinName(lab.snap, lab.defs, wire.b)}</dd>
      <dt>Carries</dt><dd>{net?.kind === 'power' ? `${net.volts} V` : net?.kind === 'gnd' ? 'Ground' : net?.kind === 'motor' ? 'Motor current' : net?.kind === 'cable' ? 'A cable' : 'A signal'}</dd>
      {#if flow}<dt>Current</dt><dd>{amps(lab.sim?.amps.get(wire.id))}, toward {pinName(lab.snap, lab.defs, flow === 1 ? wire.b : wire.a)}</dd>{/if}
      {#if net?.kind !== 'cable'}
        {@const g = gaugeOf(wire, lab.snap, lab.defs)}
        <dt>Wire</dt><dd>{GAUGES[g].name}, takes {GAUGES[g].amps} A</dd>
        {#if lab.sim}
          {@const a = Math.abs(lab.sim.amps.get(wire.id) ?? 0)}
          <dt>Loses</dt><dd class="mono">{(a * (GAUGES[g].ohmPerM * (wire.len ?? 0.25) + (g === 'dupont' ? 0.02 : 0))).toFixed(3)} V</dd>
        {/if}
      {/if}
      <dt>On paper</dt><dd class="mono">{wireLength(wire.id)} mm</dd>
    </dl>
    {#if net?.kind !== 'cable'}
      <div class="params">
        <label class="field"
          >Wire
          <select value={gaugeOf(wire, lab.snap, lab.defs)} onchange={(e) => lab.setWire(wire.id, { gauge: e.currentTarget.value as Gauge })}>
            {#each GAUGE_ORDER as g (g)}<option value={g}>{GAUGES[g].name} (up to {GAUGES[g].amps} A)</option>{/each}
          </select>
        </label>
        <label class="field"
          >Real length (m)
          <input type="number" min="0.02" max="20" step="0.05" value={wire.len ?? 0.25} onchange={(e) => lab.setWire(wire.id, { len: Number(e.currentTarget.value) })} />
        </label>
      </div>
    {/if}
    <div class="row">
      <button class="btn" onclick={(e) => { const r = e.currentTarget.getBoundingClientRect(); openMenu(r.left, r.bottom + 4, wireMenu(wire.id)); }}>Colour…</button>
      <button class="btn danger" onclick={() => lab.removeWire(wire.id)}>Remove wire</button>
    </div>
    <p class="note">The colour you pick is the colour the build steps ask for, so it matches the wire in your hand.</p>
  {:else}
    <h2>How it works</h2>
    <p class="note"><b>Right-click</b> a part, pick <b>Connect to</b>: free pins that fit get the wires. Hover an option to see them first.</p>
    <p class="note"><b>Wire by hand:</b> click a pin, then another. Pins that fit glow; the rest fade.</p>
    <p class="note"><b>Breadboard:</b> drop a module near it and its legs snap into the holes; the strips join them.</p>
    <p class="note"><b>Power</b> (P) switches the drawing on: what runs glows, current moves along the wires, a short trips the BMS or fuse.</p>
    <p class="note"><b>Checks</b> offers fixes: one click, or a choice when there are several.</p>
    <h2>Keys</h2>
    <dl class="keys">
      <dt>Ctrl A · C · X · V · D</dt><dd>select all, copy, cut, paste, duplicate</dd>
      <dt>Ctrl Z · Ctrl Y</dt><dd>undo, redo</dd>
      <dt>R · Shift R</dt><dd>rotate, rotate back</dd>
      <dt>H · V</dt><dd>mirror the layout of the selection (the parts keep their pins)</dd>
      <dt>Shift + drag</dt><dd>move along one axis</dd>
      <dt>Drag the mat</dt><dd>select with a box (Ctrl adds)</dd>
      <dt>Space + drag</dt><dd>pan (or the middle button)</dd>
      <dt>Arrows</dt><dd>nudge (Shift: further)</dd>
      <dt>Del · Esc · F · P</dt><dd>remove, cancel, fit, power</dd>
    </dl>
  {/if}
</div>

<style>
  .tabs {
    display: flex;
    gap: 2px;
    padding: 8px 8px 0;
    border-bottom: 1px solid var(--line);
    flex-wrap: wrap;
  }
  .tabs button {
    border: 0;
    background: none;
    padding: 7px 10px 9px;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    color: var(--muted);
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .tabs button[aria-selected='true'] {
    color: var(--text);
    border-color: var(--accent);
  }
  .count {
    font-family: var(--font-data);
    font-weight: 400;
    font-size: 12px;
  }
  .body {
    overflow-y: auto;
    overflow-x: hidden;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .body > * {
    min-width: 0;
    max-width: 100%;
  }
  h2 {
    font-size: 11.5px;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--muted);
    margin: 0 0 4px;
  }
  h3 {
    font-size: 12.5px;
    margin: 6px 0 0;
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 2px 8px;
  }
  h3 .s {
    color: var(--muted);
    font-weight: 500;
    font-family: var(--font-data);
  }
  .note {
    font-size: 12.5px;
    color: var(--muted);
    margin: 0;
  }
  .note b {
    color: var(--text);
    font-weight: 600;
  }
  .keys {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 4px 10px;
    font-size: 12px;
    margin: 0;
  }
  .keys dt {
    font-family: var(--font-data);
    color: var(--text);
    white-space: nowrap;
  }
  .keys dd {
    margin: 0;
    color: var(--muted);
  }
  .kv {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 4px 12px;
    font-size: 13px;
    margin: 0;
  }
  .kv dt {
    color: var(--muted);
  }
  .kv dd {
    margin: 0;
    min-width: 0;
  }
  .power.on,
  .power.source {
    color: var(--ok);
    font-weight: 600;
  }
  .power.over,
  .power.tripped {
    color: var(--danger);
    font-weight: 600;
  }
  .power.under,
  .power.hot,
  .power.off {
    color: var(--warn);
    font-weight: 600;
  }
  .mono {
    font-family: var(--font-data);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12.5px;
    color: var(--muted);
    min-width: 0;
  }
  .field select,
  .field input {
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    padding: 6px 9px;
    background: var(--surface-2);
    color: var(--text);
    font-size: 13.5px;
    width: 100%;
    min-width: 0;
  }
  .params {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
    gap: 8px;
  }
  .params h2 {
    grid-column: 1 / -1;
  }
  .row {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .wide {
    width: 100%;
    justify-content: center;
    white-space: normal;
  }
  .notes {
    background: var(--surface-2);
    border-left: 3px solid var(--accent);
    border-radius: 0 var(--r-md) var(--r-md) 0;
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12.5px;
  }
  .notes p {
    margin: 0;
  }
  .pins {
    width: 100%;
    table-layout: fixed;
    border-collapse: collapse;
    font-size: 12.5px;
  }
  .pins td {
    padding: 5px 4px;
    border-top: 1px solid var(--line);
    vertical-align: top;
  }
  .pins .pin {
    font-family: var(--font-data);
    font-weight: 600;
    width: 30%;
  }
  .pins .dotcell {
    width: 14px;
  }
  .to {
    color: var(--muted);
  }
  tr.on .to {
    color: var(--text);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 99px;
    display: inline-block;
  }
  .issues {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .issue {
    border-radius: var(--r-md);
    background: var(--surface-2);
    font-size: 13px;
    overflow: hidden;
    border-left: 4px solid var(--line-strong);
    display: flex;
    flex-direction: column;
  }
  .issue.error {
    border-left-color: var(--danger);
  }
  .issue.warn {
    border-left-color: var(--warn);
  }
  .issue.ok {
    border-left-color: var(--ok);
  }
  .issue .txt {
    border: 0;
    background: none;
    text-align: left;
    padding: 8px 10px;
    cursor: pointer;
    color: inherit;
    width: 100%;
  }
  .issue .txt:hover {
    background: var(--surface-3);
  }
  .issue small {
    display: block;
    color: var(--muted);
    font-size: 12px;
    margin-top: 2px;
  }
  .fix {
    margin: 0 10px 8px;
    align-self: flex-start;
    border: 1px solid var(--accent);
    color: var(--accent);
    background: var(--surface);
    border-radius: var(--r-md);
    padding: 4px 9px;
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
    text-align: left;
    max-width: calc(100% - 20px);
  }
  .fix:hover {
    background: var(--accent-soft);
  }
  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .steps li {
    display: grid;
    grid-template-columns: 20px minmax(0, 1fr);
    gap: 6px;
    align-items: start;
  }
  .steps input {
    margin: 9px 0 0;
    accent-color: var(--accent);
  }
  .step {
    display: grid;
    grid-template-columns: 14px minmax(0, 1fr);
    gap: 8px;
    border: 0;
    background: none;
    text-align: left;
    padding: 6px;
    border-radius: var(--r-md);
    cursor: pointer;
    font-size: 13px;
    color: inherit;
    min-width: 0;
  }
  .step:hover {
    background: var(--surface-3);
  }
  .sw {
    width: 14px;
    height: 5px;
    border-radius: 3px;
    margin-top: 7px;
  }
  .done .step span {
    color: var(--faint);
    text-decoration: line-through;
  }
  .step small {
    display: block;
    color: var(--muted);
    font-size: 12px;
  }
  .progress {
    height: 6px;
    background: var(--surface-3);
    border-radius: 99px;
    overflow: hidden;
    flex: none;
  }
  .progress div {
    height: 100%;
    background: var(--accent);
    border-radius: 99px;
    transition: width 0.2s;
  }
  @media (prefers-reduced-motion: reduce) {
    .progress div {
      transition: none;
    }
  }
</style>
