<script lang="ts">
  /** The right panel: what is selected, the checks, and the build steps. */
  import { pinName } from '../lib/checks';
  import { splitKey } from '../lib/types';
  import { lab } from './lab.svelte';
  import { connectItems, openMenu, wireMenu } from './menu';

  const MM = 1.27;
  const part = $derived(lab.sel && 'part' in lab.sel ? lab.part(lab.sel.part) : undefined);
  const wire = $derived(lab.sel && 'wire' in lab.sel ? lab.doc.wires.find((w) => w.id === (lab.sel as { wire: string }).wire) : undefined);
  const errors = $derived(lab.issues.filter((i) => i.sev === 'error').length);
  const warns = $derived(lab.issues.length - errors);
  const total = $derived(lab.steps.reduce((n, g) => n + g.steps.length, 0));
  const done = $derived(lab.steps.reduce((n, g) => n + g.steps.filter((s) => lab.doc.done[s.id]).length, 0));

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
    {#if !lab.issues.length}
      <ul class="issues"><li class="issue ok"><span>Every wire checks out.<small>Power, ground, pin types and voltages all match.</small></span></li></ul>
    {:else}
      <ul class="issues">
        {#each lab.issues as it, i (i)}
          <li class="issue {it.sev}">
            <button onclick={() => showIssue(i)}><span>{it.text}<small>{it.more}</small></span></button>
          </li>
        {/each}
      </ul>
      <p class="note">Click a line to see it on the drawing. Faber runs the same checks before it writes code to a board.</p>
    {/if}
  {:else if lab.tab === 'build'}
    <div class="progress" role="progressbar" aria-valuenow={done} aria-valuemax={total}><div style:width="{total ? (done / total) * 100 : 0}%"></div></div>
    <p class="note"><b>{done} of {total} steps.</b> Unplug the battery and the USB cable before you start.</p>
    {#each lab.steps as g (g.title)}
      <h3>{g.title} <span>{g.sub}</span></h3>
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
  {:else if part}
    {@const d = lab.defs(part.def)!}
    <label class="field">Name<input value={part.name} onchange={(e) => lab.rename(part.id, e.currentTarget.value)} /></label>
    <dl class="kv">
      <dt>Part</dt><dd>{d.name}</dd>
      <dt>Covers</dt><dd>{d.covers}</dd>
      <dt>Size</dt><dd class="mono">{(d.w * MM).toFixed(1)} × {(d.h * MM).toFixed(1)} mm</dd>
    </dl>
    <div class="row">
      {#if d.kind !== 'breadboard'}
        <button class="btn primary" onclick={(e) => { const r = e.currentTarget.getBoundingClientRect(); openMenu(r.left, r.bottom + 4, connectItems(part.id)); }}>Connect to…</button>
        <button class="btn" onclick={() => lab.rotate(part.id)}>Rotate</button>
      {/if}
      <button class="btn danger" onclick={() => lab.removePart(part.id)}>Remove</button>
    </div>
    <div>
      <h2>{d.kind === 'part' ? 'Pins' : 'Pins in use'}</h2>
      <table class="pins">
        <tbody>
          {#each pinRows() as r (r.p.id)}
            <tr class:on={r.to.length}>
              <td>{r.p.label}</td>
              <td>{#if r.color}<i class="dot" style:background={r.color}></i>{/if}</td>
              <td class="to">{r.to.length ? r.to.join(', ') : r.p.optional ? 'free (optional)' : 'not connected'}</td>
            </tr>
          {:else}
            <tr><td colspan="3" class="to">No wires yet. Right-click it, or click a pin, to start one.</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else if wire}
    {@const net = lab.nets.of.get(wire.a)}
    <dl class="kv">
      <dt>From</dt><dd>{pinName(lab.snap, lab.defs, wire.a)}</dd>
      <dt>To</dt><dd>{pinName(lab.snap, lab.defs, wire.b)}</dd>
      <dt>Carries</dt><dd>{net?.kind === 'power' ? `${net.volts} V` : net?.kind === 'gnd' ? 'Ground' : net?.kind === 'motor' ? 'Motor current' : net?.kind === 'cable' ? 'A cable' : 'A signal'}</dd>
      <dt>On paper</dt><dd class="mono">{wireLength(wire.id)} mm</dd>
    </dl>
    <div class="row">
      <button class="btn" onclick={(e) => { const r = e.currentTarget.getBoundingClientRect(); openMenu(r.left, r.bottom + 4, wireMenu(wire.id)); }}>Colour…</button>
      <button class="btn danger" onclick={() => lab.removeWire(wire.id)}>Remove wire</button>
    </div>
    <p class="note">The colour you pick is the colour the build steps ask for, so it matches the wire in your hand.</p>
  {:else}
    <h2>How it works</h2>
    <p class="note"><b>Right-click</b> a part and pick <b>Connect to</b>: Faber finds free pins that fit and adds the wires. Hover an option to see them first.</p>
    <p class="note"><b>Wire by hand:</b> click a pin, then another. Pins that fit glow green, doubtful ones yellow; the rest fade.</p>
    <p class="note"><b>Breadboard:</b> drag a module onto it and its legs drop into the holes; the strips join them. Wire to any hole in the strip.</p>
    <p class="note"><b>Move:</b> drag a part. Wires find their own way, never run on top of each other, and hop where they cross. <b>Arrange</b> lays everything out.</p>
    <p class="note"><b>Keys:</b> <kbd>R</kbd> rotate · <kbd>Del</kbd> remove · <kbd>Ctrl Z</kbd> undo · <kbd>Esc</kbd> cancel</p>
  {/if}
</div>

<style>
  .tabs {
    display: flex;
    gap: 2px;
    padding: 8px 8px 0;
    border-bottom: 1px solid var(--line);
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
    overflow: auto;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
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
    justify-content: space-between;
    gap: 8px;
  }
  h3 span {
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
  kbd {
    font-family: var(--font-data);
    font-size: 11.5px;
    border: 1px solid var(--line-strong);
    border-radius: 4px;
    padding: 0 4px;
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
  .mono {
    font-family: var(--font-data);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12.5px;
    color: var(--muted);
  }
  .field input {
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    padding: 6px 9px;
    background: var(--surface-2);
    color: var(--text);
    font-size: 13.5px;
  }
  .row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .pins {
    width: 100%;
    border-collapse: collapse;
    font-size: 12.5px;
  }
  .pins td {
    padding: 5px 4px;
    border-top: 1px solid var(--line);
    vertical-align: top;
  }
  .pins td:first-child {
    font-family: var(--font-data);
    font-weight: 600;
    white-space: nowrap;
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
    display: grid;
    grid-template-columns: 4px minmax(0, 1fr);
    border-radius: var(--r-md);
    background: var(--surface-2);
    font-size: 13px;
    overflow: hidden;
  }
  .issue::before {
    content: '';
  }
  .issue.error::before {
    background: var(--danger);
  }
  .issue.warn::before {
    background: var(--warn);
  }
  .issue.ok::before {
    background: var(--ok);
  }
  .issue button,
  .issue > span {
    border: 0;
    background: none;
    text-align: left;
    padding: 8px 10px;
    cursor: pointer;
    color: inherit;
  }
  .issue button:hover {
    background: var(--surface-3);
  }
  .issue small {
    display: block;
    color: var(--muted);
    font-size: 12px;
    margin-top: 2px;
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
