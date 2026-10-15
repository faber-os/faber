<script lang="ts">
  import { useCore } from '../lib/state/context';
  import { formatClockTime, formatLongDate, formatShortDate } from '../lib/desktop/wallClock';

  /** `withDate`: the date under the time (the large taskbar). */
  let { withDate = false }: { withDate?: boolean } = $props();
  const core = useCore();
  let now = $state(new Date());

  $effect(() => {
    // Tick on the minute boundary so the clock never lags behind the system clock.
    let timer: ReturnType<typeof setTimeout>;
    const tick = () => {
      // Read the time from a local, not from `now`, so this effect does not depend on what it sets.
      const d = new Date();
      now = d;
      timer = setTimeout(tick, 60_000 - (d.getSeconds() * 1000 + d.getMilliseconds()) + 20);
    };
    tick();
    return () => clearTimeout(timer);
  });

  // The machine's time, in every browser (D42).
  const time = $derived(formatClockTime(now.getTime(), core.wallClock));
  const date = $derived(formatLongDate(now.getTime(), core.wallClock));
</script>

<time class="clock num" class:stacked={withDate} datetime={now.toISOString()} title={date}
  >{time}{#if withDate}<span class="date">{formatShortDate(now.getTime(), core.wallClock)}</span
    >{/if}</time
>

<style>
  .clock {
    display: inline-flex;
    align-items: center;
    height: var(--touch);
    padding: 0 var(--space-3);
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text);
  }
  .clock.stacked {
    flex-direction: column;
    justify-content: center;
    align-items: flex-end;
    line-height: 1.25;
  }
  .date {
    font-size: var(--text-xs);
    font-weight: 500;
    color: var(--text-muted);
  }
</style>
