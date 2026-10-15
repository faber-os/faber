<script lang="ts">
  import { untrack } from 'svelte';
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import {
    patternFor,
    patternKinds,
    patternValue,
    sensorRange,
    type PatternKind,
  } from '../lib/state/simulation';
  import type { ErrorBody, PartInfo, Pattern } from '../lib/state/types';
  import { formatLength, formatVolts } from '../lib/viewers/units';
  import ErrorNotice from '../components/ErrorNotice.svelte';

  /** A manual simulated sensor's value: a slider and a pattern menu (docs/simulation.md, 9). */
  let { part }: { part: PartInfo } = $props();

  const core = useCore();
  const name = $derived(part.name);
  const range = $derived(sensorRange(part));
  const kinds = $derived(patternKinds(part));
  const unit = $derived(core.settings?.units.length ?? 'metric');
  const show = (v: number) =>
    range.quantity === 'voltage' ? formatVolts(v) : formatLength(v, unit);

  let value = $state<number | null>(null);
  let kind = $state<PatternKind>('steady');
  let error = $state<ErrorBody | null>(null);

  const LABELS: Record<PatternKind, () => string> = {
    steady: () => t('parts.manual.pattern.steady'),
    sine: () => t('parts.manual.pattern.sine'),
    noise: () => t('parts.manual.pattern.noise'),
    step: () => t('parts.manual.pattern.step'),
    drain: () => t('parts.manual.pattern.drain'),
  };

  // What the part does now: the core keeps it, and another screen may have changed it.
  $effect(() => {
    const part = name;
    untrack(() => {
      void core.getSimulation(part).then((r) => {
        if (part !== name) return;
        if (r.ok) {
          kind = r.value.pattern.kind;
          value = patternValue(r.value.pattern);
          error = null;
        } else error = r.error;
      });
    });
  });

  // A slider moves faster than the core answers: send only the newest value, one at a time.
  let sending = false;
  let pending: Pattern | null = null;
  async function send(p: Pattern) {
    pending = p;
    if (sending) return;
    sending = true;
    while (pending) {
      const next = pending;
      pending = null;
      const r = await core.putSimulation(name, next);
      error = r.ok ? null : r.error;
    }
    sending = false;
  }

  function change(v: number, k: PatternKind) {
    value = v;
    kind = k;
    void send(patternFor(k, v, range));
  }
</script>

<div class="manual">
  <input
    type="range"
    min={range.min}
    max={range.max}
    step={(range.max - range.min) / 200}
    value={value ?? range.min}
    disabled={value === null}
    aria-label={t('parts.manual.value', { name })}
    aria-valuetext={value === null ? undefined : show(value)}
    oninput={(e) => change(Number(e.currentTarget.value), kind)}
  />
  <output class="num">{value === null ? '–' : show(value)}</output>
  <select
    class="input"
    value={kind}
    disabled={value === null}
    aria-label={t('parts.manual.pattern', { name })}
    onchange={(e) => change(value ?? range.min, e.currentTarget.value as PatternKind)}
  >
    {#each kinds as k (k)}
      <option value={k}>{LABELS[k]()}</option>
    {/each}
  </select>
</div>
{#if error}<ErrorNotice {error} />{/if}

<style>
  .manual {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-3);
    margin-top: var(--space-1);
  }
  input[type='range'] {
    flex: 1 1 10rem;
    min-width: 8rem;
    max-width: 22rem;
    accent-color: var(--accent);
  }
  .num {
    min-width: 4.5rem;
    font-variant-numeric: tabular-nums;
    font-weight: 650;
    font-size: var(--text-sm);
  }
  select {
    width: auto;
  }
</style>
