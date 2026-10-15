<script lang="ts">
  /** `withDate`: the date under the time (the large taskbar). */
  let { withDate = false }: { withDate?: boolean } = $props();
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

  const time = $derived(now.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' }));
  const date = $derived(
    now.toLocaleDateString(undefined, {
      weekday: 'long',
      day: 'numeric',
      month: 'long',
      year: 'numeric',
    }),
  );
</script>

<time class="clock num" class:stacked={withDate} datetime={now.toISOString()} title={date}
  >{time}{#if withDate}<span class="date"
      >{now.toLocaleDateString(undefined, { day: 'numeric', month: 'short' })}</span
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
