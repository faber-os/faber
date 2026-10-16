<script lang="ts">
  // The Signal plotter: any number in any signal, scrolling, with pause and CSV export.
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import { History } from '../lib/viewers/signals';
  import {
    numericFields,
    polyline,
    seriesId,
    toCsv,
    valueRange,
    type CsvRow,
    type SeriesKey,
  } from '../lib/viewers/plotter';
  import Icon from '../components/Icon.svelte';
  import type { WindowProps } from '../components/windowDefs';

  let { win, visible }: WindowProps = $props();

  const WINDOW_S = 30;
  const W = 400;
  const H = 160;
  /** Every recorded sample, for the CSV: at most this many. */
  const KEEP = 20_000;

  const core = useCore();

  /** Every signal there is, by address. */
  const signals = $derived(
    (core.parts ?? []).flatMap((p) =>
      p.signals.map((s) => ({ address: s.address, capability: s.capability })),
    ),
  );

  /** Fields seen per signal so far, for the field picker. */
  let fieldsOf = $state<Record<string, string[]>>({});
  // svelte-ignore state_referenced_locally
  let address = $state<string>(win.params.address ?? '');
  let field = $state<string>('');
  let series = $state<SeriesKey[]>([]);
  let paused = $state(false);
  let lines = $state<Record<string, string>>({});
  let range = $state({ min: 0, max: 1 });
  let now = 0;
  const histories = new Map<string, History>();
  let rows: CsvRow[] = [];

  /** Watch every signal a series uses, plus the one being picked (to learn its fields). */
  const watched = $derived([
    ...new Set([...series.map((s) => s.address), address].filter(Boolean)),
  ]);
  $effect(() => {
    const stops = watched.map((a) =>
      core.watchSignal(a, 20, {
        onReading: (r) => {
          const fields = numericFields(r.reading);
          const names = fields.map((f) => f.path);
          if ((fieldsOf[a] ?? []).join() !== names.join()) fieldsOf = { ...fieldsOf, [a]: names };
          if (paused) return;
          const tUs = r.reading.tUs;
          now = Math.max(now, tUs);
          for (const s of series) {
            if (s.address !== a) continue;
            const f = fields.find((x) => x.path === s.field);
            if (!f) continue;
            const id = seriesId(s);
            let h = histories.get(id);
            if (!h) histories.set(id, (h = new History(WINDOW_S * 1_000_000)));
            h.push({ tUs, value: f.value, ok: true });
            rows.push({ tUs, address: a, field: s.field, value: f.value });
            if (rows.length > KEEP) rows = rows.slice(-KEEP);
          }
          if (visible) redraw();
        },
        onError: () => {},
      }),
    );
    return () => stops.forEach((s) => s());
  });

  function redraw() {
    const all = series.map((s) => histories.get(seriesId(s))?.samples ?? []);
    range = valueRange(all);
    const next: Record<string, string> = {};
    for (const s of series) {
      const h = histories.get(seriesId(s));
      if (h) next[seriesId(s)] = polyline(h.samples, now, h.windowUs, range, W, H);
    }
    lines = next;
  }

  const fieldChoices = $derived(fieldsOf[address] ?? []);
  $effect(() => {
    if (fieldChoices.length > 0 && !fieldChoices.includes(field)) field = fieldChoices[0]!;
  });

  function add() {
    if (!address || !field) return;
    const k = { address, field };
    if (!series.some((s) => seriesId(s) === seriesId(k))) series = [...series, k];
  }

  function remove(k: SeriesKey) {
    series = series.filter((s) => seriesId(s) !== seriesId(k));
    histories.delete(seriesId(k));
    redraw();
  }

  function exportCsv() {
    const blob = new Blob([toCsv(rows)], { type: 'text/csv;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'faber-signals.csv';
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  const COLOURS = [
    'var(--accent)',
    'var(--warn)',
    'var(--app-logs)',
    'var(--app-world)',
    'var(--app-files)',
  ];
  const fmt = (v: number) => (Math.abs(v) >= 100 ? v.toFixed(0) : v.toPrecision(3));
</script>

<div class="plotter">
  <div class="toolbar">
    <label>
      {t('plotter.signal')}
      <select bind:value={address} aria-label={t('plotter.signal')}>
        <option value="">{t('plotter.pick')}</option>
        {#each signals as s (s.address)}
          <option value={s.address}>{s.address}</option>
        {/each}
      </select>
    </label>
    <label>
      {t('plotter.field')}
      <select
        bind:value={field}
        aria-label={t('plotter.field')}
        disabled={fieldChoices.length === 0}
      >
        {#each fieldChoices as f (f)}
          <option value={f}>{f}</option>
        {/each}
      </select>
    </label>
    <button class="btn" onclick={add} disabled={!address || !field}>
      <Icon name="plus" size={15} />
      {t('plotter.add')}
    </button>
  </div>
  {#if signals.length === 0}
    <p class="note">{t('plotter.noSignals')}</p>
  {:else if address && fieldChoices.length === 0}
    <p class="note">{t('plotter.waiting')}</p>
  {/if}

  <figure class="plot">
    <svg
      viewBox="0 0 {W} {H}"
      preserveAspectRatio="none"
      data-testid="plotter-plot"
      aria-hidden="true"
    >
      {#each series as s, i (seriesId(s))}
        <polyline
          points={lines[seriesId(s)] ?? ''}
          stroke={COLOURS[i % COLOURS.length]}
          vector-effect="non-scaling-stroke"
        />
      {/each}
    </svg>
    <figcaption>
      {t('plotter.range', { seconds: WINDOW_S, min: fmt(range.min), max: fmt(range.max) })}
    </figcaption>
  </figure>

  <ul class="legend" aria-label={t('plotter.series')}>
    {#each series as s, i (seriesId(s))}
      <li>
        <span class="swatch" style:background={COLOURS[i % COLOURS.length]}></span>
        <code>{s.address}</code>
        <span>{s.field}</span>
        <button
          class="btn quiet"
          onclick={() => remove(s)}
          aria-label={t('plotter.remove', { series: seriesId(s) })}
        >
          <Icon name="close" size={14} />
        </button>
      </li>
    {/each}
  </ul>

  <div class="actions">
    <button class="btn" aria-pressed={paused} onclick={() => (paused = !paused)}>
      {paused ? t('plotter.resume') : t('plotter.pause')}
    </button>
    <button class="btn" onclick={exportCsv} disabled={series.length === 0}>
      <Icon name="download" size={15} />
      {t('plotter.csv')}
    </button>
  </div>
</div>

<style>
  .plotter {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    flex: 1;
    min-height: 0;
    padding: var(--space-3);
    overflow-y: auto;
    background: var(--surface);
  }
  .toolbar,
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }
  label {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    font-size: var(--text-sm);
  }
  .note {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .plot {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: 0;
    min-height: 140px;
  }
  .plot svg {
    flex: 1;
    width: 100%;
    min-height: 100px;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }
  polyline {
    fill: none;
    stroke-width: 2;
  }
  figcaption {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .legend {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: var(--space-1);
    font-size: var(--text-sm);
  }
  .legend li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .swatch {
    width: 14px;
    height: 4px;
    border-radius: 2px;
  }
</style>
