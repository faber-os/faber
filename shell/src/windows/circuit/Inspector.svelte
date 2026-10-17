<script lang="ts">
  /** The right panel: what is selected (and its settings), the checks with their fixes, the build steps. */
  import { t } from '../../lib/i18n';
  import RichText from '../../components/RichText.svelte';
  import { pinName } from '../../lib/circuit/checks';
  import { GAUGE_ORDER, GAUGES, gaugeOf } from '../../lib/circuit/circuit';
  import type { Fix } from '../../lib/circuit/fixes';
  import { paramValues } from '../../lib/circuit/library';
  import type { Gauge } from '../../lib/circuit/types';
  import { splitKey } from '../../lib/circuit/types';
  import { useCircuit } from './circuit.svelte';
  import { connectItems, openMenu, wireMenu, type MenuItem } from './menu';

  const lab = useCircuit();
  const MM = 1.27;
  const part = $derived(lab.selParts.length === 1 ? lab.part(lab.selParts[0]!) : undefined);
  const wire = $derived(lab.selWire ? lab.doc.wires.find((w) => w.id === lab.selWire) : undefined);
  const errors = $derived(lab.issues.filter((i) => i.sev === 'error').length);
  const warns = $derived(lab.issues.length - errors);
  const total = $derived(lab.steps.reduce((n, g) => n + g.steps.length, 0));
  const done = $derived(
    lab.steps.reduce((n, g) => n + g.steps.filter((s) => lab.doc.done[s.id]).length, 0),
  );
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
        const to = [
          ...ws.map((w) => pinName(lab.snap, lab.defs, w.a === key ? w.b : w.a)),
          ...(plug ? [t('circuit.inspector.hole', { hole: splitKey(plug[1])[1] })] : []),
        ];
        const color = ws[0]
          ? (ws[0].color ?? lab.nets.of.get(key)?.color)
          : plug
            ? '#e2bd52'
            : undefined;
        return { p, to, color };
      })
      .filter((r) => d.kind === 'part' || r.to.length);
  }

  function wireLength(id: string): number {
    const r = lab.routing.routes.get(id);
    if (!r) return 0;
    let n = 0;
    for (let i = 0; i < r.pts.length - 1; i++)
      n += Math.abs(r.pts[i + 1]![0] - r.pts[i]![0]) + Math.abs(r.pts[i + 1]![1] - r.pts[i]![1]);
    return Math.round(n * MM);
  }

  function showIssue(i: number) {
    const it = lab.issues[i]!;
    lab.hl = { parts: it.parts, wires: it.wires };
  }

  function chooseFix(e: MouseEvent, list: Fix[]) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const items: MenuItem[] = list.map((f) => ({
      label: f.label,
      hint: f.hint,
      action: () => lab.fix(f),
    }));
    openMenu(lab, r.left, r.bottom + 4, items);
  }

  const POWER_TEXT: Record<string, () => string> = {
    on: () => t('circuit.power.on'),
    source: () => t('circuit.power.source'),
    off: () => t('circuit.power.off'),
    over: () => t('circuit.power.over'),
    under: () => t('circuit.power.under'),
    hot: () => t('circuit.power.hot'),
    tripped: () => t('circuit.power.tripped'),
    overload: () => t('circuit.power.overload'),
    burnt: () => t('circuit.power.burnt'),
  };

  const amps = (a: number | undefined) =>
    a === undefined
      ? '–'
      : Math.abs(a) < 1
        ? t('circuit.units.ma', { value: Math.round(Math.abs(a) * 1000) })
        : t('circuit.units.a', { value: Math.abs(a).toFixed(2) });
  const volts = (key: string) => {
    const v = lab.sim?.volts.get(key);
    return v === undefined ? '' : ` · ${t('circuit.pin.volts', { volts: v.toFixed(2) })}`;
  };
  function carries(kind: string | undefined, v: number | undefined): string {
    if (kind === 'power') return t('circuit.pin.volts', { volts: v ?? 0 });
    if (kind === 'gnd') return t('circuit.pin.gnd');
    if (kind === 'motor') return t('circuit.inspector.motorCurrent');
    if (kind === 'cable') return t('circuit.inspector.cable');
    return t('circuit.inspector.signal');
  }
  function showFinding(parts: string[], wires: string[]) {
    lab.hl = { parts, wires };
  }
</script>

<div class="tabs" role="tablist">
  <button
    role="tab"
    aria-selected={lab.tab === 'sel'}
    onclick={() => ((lab.tab = 'sel'), (lab.hl = null))}>{t('circuit.tab.selected')}</button
  >
  <button role="tab" aria-selected={lab.tab === 'checks'} onclick={() => (lab.tab = 'checks')}>
    {t('circuit.tab.checks')}
    {#if errors}<span class="chip err">{errors}</span>{:else if warns}<span class="chip warn"
        >{warns}</span
      >{:else}<span class="chip ok">✓</span>{/if}
  </button>
  <button role="tab" aria-selected={lab.tab === 'build'} onclick={() => (lab.tab = 'build')}
    >{t('circuit.tab.build')} <span class="count">{done}/{total}</span></button
  >
</div>

<div class="body">
  {#if lab.tab === 'checks'}
    {#if lab.sim}
      <h2>{t('circuit.checks.powerOn')}</h2>
      <ul class="issues">
        {#each lab.sim.findings as f, i (i)}
          <li class="issue {f.sev}">
            <button class="txt" onclick={() => showFinding(f.parts, f.wires)}
              >{f.text}{#if f.why}<small>{f.why}</small>{/if}</button
            >
          </li>
        {:else}
          <li class="issue ok">
            <div class="txt">
              {t('circuit.checks.powerOk')}<small>{t('circuit.checks.powerOkMore')}</small>
            </div>
          </li>
        {/each}
      </ul>
      <h2>{t('circuit.checks.wiring')}</h2>
    {/if}
    {#if !lab.issues.length}
      <ul class="issues">
        <li class="issue ok">
          <div class="txt">{t('circuit.checks.ok')}<small>{t('circuit.checks.okMore')}</small></div>
        </li>
      </ul>
    {:else}
      {#if fixes.some((f) => f.length === 1)}
        <button class="btn primary wide" onclick={() => lab.fixAll()}
          >{t('circuit.checks.fixAll')}</button
        >
      {/if}
      <ul class="issues">
        {#each lab.issues as it, i (it.code + it.text)}
          {@const list = fixes[i] ?? []}
          <li class="issue {it.sev}">
            <button class="txt" onclick={() => showIssue(i)}
              >{it.text}<small>{it.more}</small></button
            >
            {#if list.length === 1}
              <button
                class="fix"
                onpointerenter={() => (lab.fixPreview = list[0]!)}
                onpointerleave={() => (lab.fixPreview = null)}
                onfocus={() => (lab.fixPreview = list[0]!)}
                onblur={() => (lab.fixPreview = null)}
                onclick={() => lab.fix(list[0]!)}
                >{t('circuit.checks.fixOne', { what: list[0]!.label })}</button
              >
            {:else if list.length > 1}
              <button class="fix" onclick={(e) => chooseFix(e, list)}
                >{t('circuit.checks.ways', { count: list.length })}</button
              >
            {/if}
          </li>
        {/each}
      </ul>
      <p class="note">{t('circuit.checks.help')}</p>
    {/if}
  {:else if lab.tab === 'build'}
    <div class="progress" role="progressbar" aria-valuenow={done} aria-valuemax={total}>
      <div style:width="{total ? (done / total) * 100 : 0}%"></div>
    </div>
    <p class="note"><RichText text={t('circuit.build.progress', { done, count: total })} /></p>
    {#each lab.steps as g (g.title)}
      <h3><span class="t">{g.title}</span> <span class="s">{g.sub}</span></h3>
      <ul class="steps">
        {#each g.steps as s (s.id)}
          <li class:done={lab.doc.done[s.id]}>
            <input
              type="checkbox"
              checked={!!lab.doc.done[s.id]}
              aria-label={t('circuit.build.done')}
              onchange={(e) => lab.tick(s.id, e.currentTarget.checked)}
            />
            <button class="step" onclick={() => (lab.hl = { parts: s.parts, wires: s.wires })}>
              <i class="sw" style:background={s.color}></i>
              <span>{s.text}<small>{s.detail}</small></span>
            </button>
          </li>
        {/each}
      </ul>
    {/each}
    <p class="note">{t('circuit.build.then')}</p>
  {:else if lab.selParts.length > 1}
    <h2>{t('circuit.inspector.manySelected', { count: lab.selParts.length })}</h2>
    <p class="note">{lab.selParts.map((id) => lab.part(id)?.name).join(', ')}</p>
    <div class="row">
      <button class="btn" onclick={() => lab.rotate()}>{t('circuit.menu.rotate')}</button>
      <button class="btn" onclick={() => lab.mirror('h')}>{t('circuit.inspector.mirrorH')}</button>
      <button class="btn" onclick={() => lab.mirror('v')}>{t('circuit.inspector.mirrorV')}</button>
      <button class="btn" onclick={() => lab.duplicate()}>{t('circuit.menu.duplicate')}</button>
      <button class="btn danger" onclick={() => lab.removeParts(lab.selParts)}
        >{t('circuit.inspector.remove')}</button
      >
    </div>
    <p class="note">{t('circuit.inspector.dragTogether')}</p>
  {:else if part}
    {@const d = lab.defs(part.def)!}
    {@const values = paramValues(part.def)}
    {@const st = lab.sim?.parts.get(part.id)}
    <label class="field"
      >{t('circuit.newPart.name')}<input
        value={part.name}
        onchange={(e) => lab.rename(part.id, e.currentTarget.value)}
      /></label
    >
    <dl class="kv">
      <dt>{t('circuit.inspector.part')}</dt>
      <dd>{d.name}</dd>
      <dt>{t('circuit.inspector.covers')}</dt>
      <dd>{d.covers}</dd>
      <dt>{t('circuit.inspector.size')}</dt>
      <dd class="mono">
        {t('circuit.inspector.sizeMm', { w: (d.w * MM).toFixed(1), h: (d.h * MM).toFixed(1) })}
      </dd>
      {#if st}<dt>{t('circuit.inspector.power')}</dt>
        <dd class="power {st}">{POWER_TEXT[st]?.()}</dd>{/if}
      {#if lab.sim && lab.sim.partAmps.has(part.id)}<dt>{t('circuit.inspector.current')}</dt>
        <dd class="mono">{amps(lab.sim.partAmps.get(part.id))}</dd>{/if}
    </dl>
    {#if d.params}
      <div class="params">
        <h2>{t('circuit.inspector.settings')}</h2>
        {#each d.params as p (p.id)}
          <label class="field"
            >{p.label}{p.unit ? ` (${p.unit})` : ''}
            <input
              type="number"
              min={p.min}
              max={p.max}
              step={p.step}
              value={values[p.id] ?? p.default}
              onchange={(e) => lab.setParam(part.id, p.id, Number(e.currentTarget.value))}
            />
          </label>
        {/each}
      </div>
    {/if}
    <div class="row">
      {#if d.kind !== 'breadboard'}
        <button
          class="btn primary"
          onclick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            openMenu(lab, r.left, r.bottom + 4, connectItems(lab, part.id));
          }}>{t('circuit.inspector.connectTo')}</button
        >
        <button class="btn" onclick={() => lab.rotate([part.id])}>{t('circuit.menu.rotate')}</button
        >
      {/if}
      {#if lab.power && d.switch}<button class="btn" onclick={() => lab.flick(part.id)}
          >{part.open
            ? t('circuit.inspector.close')
            : d.switch === 'push'
              ? t('circuit.inspector.press')
              : t('circuit.inspector.open')}</button
        >{/if}
      <button class="btn danger" onclick={() => lab.removeParts([part.id])}
        >{t('circuit.inspector.remove')}</button
      >
    </div>
    {#if d.notes?.length}
      <div class="notes">
        {#each d.notes as n, i (i)}<p>{n}</p>{/each}
      </div>
    {/if}
    <div>
      <h2>{d.kind === 'part' ? t('circuit.inspector.pins') : t('circuit.inspector.pinsInUse')}</h2>
      <table class="pins">
        <tbody>
          {#each pinRows() as r (r.p.id)}
            <tr class:on={r.to.length}>
              <td class="pin">{r.p.label}</td>
              <td class="dotcell"
                >{#if r.color}<i class="dot" style:background={r.color}></i>{/if}</td
              >
              <td class="to"
                >{r.to.length
                  ? r.to.join(', ')
                  : r.p.optional
                    ? t('circuit.inspector.freeOptional')
                    : t('circuit.inspector.notConnected')}{volts(`${part.id}.${r.p.id}`)}</td
              >
            </tr>
          {:else}
            <tr><td colspan="3" class="to">{t('circuit.inspector.noWires')}</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
    {#if d.kind === 'part'}<p class="note">{t('circuit.inspector.pinOrder')}</p>{/if}
  {:else if wire}
    {@const net = lab.nets.of.get(wire.a)}
    {@const flow = lab.sim?.flow.get(wire.id)}
    <dl class="kv">
      <dt>{t('circuit.inspector.from')}</dt>
      <dd>{pinName(lab.snap, lab.defs, wire.a)}</dd>
      <dt>{t('circuit.inspector.to')}</dt>
      <dd>{pinName(lab.snap, lab.defs, wire.b)}</dd>
      <dt>{t('circuit.inspector.carries')}</dt>
      <dd>{carries(net?.kind, net?.volts)}</dd>
      {#if flow}<dt>{t('circuit.inspector.current')}</dt>
        <dd>
          {t('circuit.inspector.toward', {
            amps: amps(lab.sim?.amps.get(wire.id)),
            pin: pinName(lab.snap, lab.defs, flow === 1 ? wire.b : wire.a),
          })}
        </dd>{/if}
      {#if net?.kind !== 'cable'}
        {@const g = gaugeOf(wire, lab.snap, lab.defs)}
        <dt>{t('circuit.inspector.wire')}</dt>
        <dd>{t('circuit.inspector.gauge', { name: GAUGES[g].name, amps: GAUGES[g].amps })}</dd>
        {#if lab.sim}
          {@const a = Math.abs(lab.sim.amps.get(wire.id) ?? 0)}
          <dt>{t('circuit.inspector.loses')}</dt>
          <dd class="mono">
            {t('circuit.pin.volts', {
              volts: (
                a *
                (GAUGES[g].ohmPerM * (wire.len ?? 0.25) + (g === 'dupont' ? 0.02 : 0))
              ).toFixed(3),
            })}
          </dd>
        {/if}
      {/if}
      <dt>{t('circuit.inspector.onPaper')}</dt>
      <dd class="mono">{t('circuit.inspector.mm', { value: wireLength(wire.id) })}</dd>
    </dl>
    {#if net?.kind !== 'cable'}
      <div class="params">
        <label class="field"
          >{t('circuit.inspector.wire')}
          <select
            value={gaugeOf(wire, lab.snap, lab.defs)}
            onchange={(e) => lab.setWire(wire.id, { gauge: e.currentTarget.value as Gauge })}
          >
            {#each GAUGE_ORDER as g (g)}<option value={g}
                >{t('circuit.inspector.gaugeOption', {
                  name: GAUGES[g].name,
                  amps: GAUGES[g].amps,
                })}</option
              >{/each}
          </select>
        </label>
        <label class="field"
          >{t('circuit.inspector.length')}
          <input
            type="number"
            min="0.02"
            max="20"
            step="0.05"
            value={wire.len ?? 0.25}
            onchange={(e) => lab.setWire(wire.id, { len: Number(e.currentTarget.value) })}
          />
        </label>
      </div>
    {/if}
    <div class="row">
      <button
        class="btn"
        onclick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          openMenu(lab, r.left, r.bottom + 4, wireMenu(lab, wire.id));
        }}>{t('circuit.inspector.colour')}</button
      >
      <button class="btn danger" onclick={() => lab.removeWire(wire.id)}
        >{t('circuit.menu.removeWire')}</button
      >
    </div>
    <p class="note">{t('circuit.inspector.colourNote')}</p>
  {:else}
    <h2>{t('circuit.help.title')}</h2>
    <p class="note"><RichText text={t('circuit.help.connect')} /></p>
    <p class="note"><RichText text={t('circuit.help.byHand')} /></p>
    <p class="note"><RichText text={t('circuit.help.breadboard')} /></p>
    <p class="note"><RichText text={t('circuit.help.power')} /></p>
    <p class="note"><RichText text={t('circuit.help.checks')} /></p>
    <h2>{t('circuit.help.keys')}</h2>
    <dl class="keys">
      <dt>{t('circuit.keys.k.clipboard')}</dt>
      <dd>{t('circuit.keys.clipboard')}</dd>
      <dt>{t('circuit.keys.k.undo')}</dt>
      <dd>{t('circuit.keys.undo')}</dd>
      <dt>{t('circuit.keys.k.rotate')}</dt>
      <dd>{t('circuit.keys.rotate')}</dd>
      <dt>{t('circuit.keys.k.mirror')}</dt>
      <dd>{t('circuit.keys.mirror')}</dd>
      <dt>{t('circuit.keys.shiftDrag')}</dt>
      <dd>{t('circuit.keys.shiftDragDo')}</dd>
      <dt>{t('circuit.keys.dragMat')}</dt>
      <dd>{t('circuit.keys.dragMatDo')}</dd>
      <dt>{t('circuit.keys.spaceDrag')}</dt>
      <dd>{t('circuit.keys.spaceDragDo')}</dd>
      <dt>{t('circuit.keys.arrows')}</dt>
      <dd>{t('circuit.keys.arrowsDo')}</dd>
      <dt>{t('circuit.keys.k.other')}</dt>
      <dd>{t('circuit.keys.other')}</dd>
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
    color: var(--text-muted);
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
    color: var(--text-muted);
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
    color: var(--text-muted);
    font-weight: 500;
    font-family: var(--font-data);
  }
  .note {
    font-size: 12.5px;
    color: var(--text-muted);
    margin: 0;
  }
  .note :global(b) {
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
    color: var(--text-muted);
  }
  .kv {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 4px 12px;
    font-size: 13px;
    margin: 0;
  }
  .kv dt {
    color: var(--text-muted);
  }
  .kv dd {
    margin: 0;
    min-width: 0;
  }
  .power.on,
  .power.source {
    color: var(--ok-text);
    font-weight: 600;
  }
  .power.over,
  .power.tripped {
    color: var(--danger-text);
    font-weight: 600;
  }
  .power.under,
  .power.hot,
  .power.off {
    color: var(--warn-text);
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
    color: var(--text-muted);
    min-width: 0;
  }
  .field select,
  .field input {
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-md);
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
    border-radius: 0 var(--radius-md) var(--radius-md) 0;
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
    color: var(--text-muted);
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
    border-radius: var(--radius-md);
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
    color: var(--text-muted);
    font-size: 12px;
    margin-top: 2px;
  }
  .fix {
    margin: 0 10px 8px;
    align-self: flex-start;
    border: 1px solid var(--accent);
    color: var(--accent);
    background: var(--surface);
    border-radius: var(--radius-md);
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
    border-radius: var(--radius-md);
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
    color: var(--text-faint);
    text-decoration: line-through;
  }
  .step small {
    display: block;
    color: var(--text-muted);
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
