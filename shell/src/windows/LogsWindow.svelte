<script lang="ts">
  import { useCore } from '../lib/state/context';
  import { filterLogs, LOG_LEVELS } from '../lib/state/logFilter';
  import type { LogEntry, LogLevel } from '../lib/state/types';
  import { formatCoreTime } from '../components/format';
  import { formatLogTime } from '../lib/desktop/wallClock';
  import type { WindowProps } from '../components/windowDefs';

  let { visible }: WindowProps = $props();

  const core = useCore();
  const LABEL: Record<LogLevel, string> = {
    error: 'Errors',
    warn: 'Warnings',
    info: 'Info',
    debug: 'Debug',
  };

  let level = $state<LogLevel>('info');
  let text = $state('');
  let follow = $state(true);
  let list: HTMLElement | undefined = $state();

  const shown = $derived(filterLogs(core.logs, level, text));

  function time(e: LogEntry): string {
    const ms = core.coreTimeToEpochMs(e.tUs);
    return ms === null ? formatCoreTime(e.tUs) : formatLogTime(ms, core.wallClock);
  }

  $effect(() => {
    void shown;
    if (follow && list && visible) list.scrollTop = list.scrollHeight;
  });

  function onScroll() {
    if (!list) return;
    follow = list.scrollHeight - list.scrollTop - list.clientHeight < 24;
  }

  function jump() {
    follow = true;
    if (list) list.scrollTop = list.scrollHeight;
  }
</script>

<div class="logs">
  <div class="toolbar">
    <div class="segmented" role="radiogroup" aria-label="Show at least">
      {#each LOG_LEVELS as l (l)}
        <label><input type="radio" name="log-level" value={l} bind:group={level} />{LABEL[l]}</label
        >
      {/each}
    </div>
    <label class="visually-hidden" for="log-search">Filter by text</label>
    <input
      id="log-search"
      class="input"
      type="search"
      placeholder="Filter by source or text"
      bind:value={text}
    />
    <p class="count num">{shown.length} of {core.logs.length}</p>
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex (a scrollable region must be reachable by keyboard) -->
  <div
    class="list selectable"
    bind:this={list}
    onscroll={onScroll}
    role="log"
    aria-label="Log"
    tabindex="0"
  >
    {#if core.logs.length === 0}
      <p class="empty">
        {core.connection === 'connected'
          ? 'Nothing logged yet.'
          : 'The log appears once the shell is connected to the core.'}
      </p>
    {:else if shown.length === 0}
      <p class="empty">No entries match. Show more levels or clear the filter.</p>
    {:else}
      {#each shown as e, i (i)}
        <div class="row {e.level}">
          <span class="t num">{time(e)}</span>
          <span class="lvl">{e.level}</span>
          <span class="src">{e.source}</span>
          <span class="msg">{e.message}</span>
        </div>
      {/each}
    {/if}
  </div>

  {#if !follow}
    <button class="btn jump" onclick={jump}>Jump to newest</button>
  {/if}
</div>

<style>
  .logs {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    background: var(--surface);
  }
  .toolbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--line);
    background: var(--surface-2);
  }
  .toolbar .input {
    flex: 1;
    min-width: 10rem;
  }
  .count {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: var(--space-1) 0;
    font-family: var(--font-data);
    font-size: var(--text-sm);
    line-height: 1.5;
  }
  .row {
    display: grid;
    grid-template-columns: 7.6rem 3.4rem minmax(6rem, 8.5rem) 1fr;
    gap: var(--space-3);
    padding: 2px var(--space-4);
  }
  .row:hover {
    background: var(--hover);
  }
  .t,
  .src {
    color: var(--text-muted);
  }
  .src {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .lvl {
    font-weight: 600;
    color: var(--text-muted);
  }
  .msg {
    white-space: pre-wrap;
    word-break: break-word;
  }
  .error .lvl,
  .error .msg {
    color: var(--danger);
  }
  .error {
    background: var(--danger-soft);
  }
  .warn .lvl {
    color: var(--warn);
  }
  .debug .msg {
    color: var(--text-muted);
  }
  .empty {
    padding: var(--space-5) var(--space-4);
    font-family: var(--font-ui);
    color: var(--text-muted);
  }
  .jump {
    position: absolute;
    right: var(--space-4);
    bottom: var(--space-4);
    box-shadow: var(--shadow-pop);
  }
  @media (max-width: 640px) {
    .row {
      grid-template-columns: 6.6rem 1fr;
    }
    .lvl {
      display: none;
    }
    .msg {
      grid-column: 1 / -1;
    }
  }
</style>
