<script lang="ts">
  // The viewer of a digital-in@1 or digital-out@1 signal: on or off now, and the latest changes.
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import type { ErrorBody } from '../lib/state/types';
  import { formatCoreTime } from '../components/format';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import RichText from '../components/RichText.svelte';
  import StateBadge from '../components/StateBadge.svelte';
  import type { WindowProps } from '../components/windowDefs';

  let { win }: WindowProps = $props();

  const KEEP = 12;

  const core = useCore();
  const address = $derived(win.params.address ?? '');
  const partName = $derived(win.params.part ?? address.split('/')[0] ?? '');
  const part = $derived(core.parts?.find((p) => p.name === partName) ?? null);

  let level = $state<boolean | null>(null);
  let output = $state(false);
  let changes = $state<{ tUs: number; level: boolean }[]>([]);
  let count = $state(0);
  let error = $state<ErrorBody | null>(null);

  $effect(() =>
    core.watchSignal(address, 50, {
      onReading: (r) => {
        if (r.capability !== 'digital-in@1' && r.capability !== 'digital-out@1') return;
        output = r.capability === 'digital-out@1';
        if (level !== null && r.reading.level !== level) count++;
        level = r.reading.level;
        changes = [{ tUs: r.reading.tUs, level: r.reading.level }, ...changes].slice(0, KEEP);
        error = null;
      },
      onError: (e) => (error = e),
    }),
  );
</script>

<div class="viewer">
  {#if error}
    <div class="pad"><ErrorNotice {error} /></div>
  {:else if level === null}
    <p class="pad quiet" role="status"><RichText text={t('signal.waiting', { address })} /></p>
  {:else}
    <div class="now">
      <span class="lamp" class:on={level} aria-hidden="true"></span>
      <p class="value" data-testid="level-value">{level ? t('level.on') : t('level.off')}</p>
      <p class="sub">{output ? t('level.output') : t('level.changes', { count })}</p>
    </div>
    <h4>{t('level.latest')}</h4>
    <ol class="changes">
      {#each changes as c, i (i)}
        <li>
          <span>{c.level ? t('level.on') : t('level.off')}</span>
          <span class="time">{formatCoreTime(c.tUs)}</span>
        </li>
      {/each}
    </ol>
  {/if}
  <div class="stats">
    <code>{address}</code>
    {#if part}<StateBadge kind={part.state.kind} />{/if}
  </div>
</div>

<style>
  .viewer {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    background: var(--surface);
  }
  .pad {
    padding: var(--space-4);
  }
  .quiet {
    font-size: var(--text-sm);
  }
  .now {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-4);
  }
  .lamp {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    border: 2px solid var(--line-strong, var(--line));
    background: var(--surface-3);
  }
  .lamp.on {
    background: var(--accent);
    border-color: var(--accent);
  }
  .value {
    font-size: var(--text-xl);
    font-weight: 700;
  }
  .sub {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }
  h4 {
    margin: 0 var(--space-4);
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .changes {
    flex: 1;
    overflow-y: auto;
    margin: var(--space-1) var(--space-4);
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
  }
  .changes li {
    display: flex;
    justify-content: space-between;
    padding: 2px 0;
    border-bottom: 1px solid var(--line);
  }
  .time {
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .stats {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin-top: auto;
    padding: var(--space-2) var(--space-4);
    border-top: 1px solid var(--line);
    background: var(--surface-2);
    font-size: var(--text-sm);
  }
</style>
