<script lang="ts">
  import { useCore } from '../lib/state/context';
  import { portAvailability } from '../lib/state/portHints';
  import {
    ByteLog,
    bytesToText,
    encodeLine,
    hexLines,
    type LineEnding,
  } from '../lib/state/serialFormat';
  import type { ErrorBody } from '../lib/state/types';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import { formatBytes } from '../components/format';
  import Icon from '../components/Icon.svelte';
  import type { WindowProps } from '../components/windowDefs';

  let { visible }: WindowProps = $props();

  const BAUDS = [9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600];
  const KEEP_BYTES = 64 * 1024;
  const HEX_LINES_SHOWN = 2000;

  const core = useCore();

  let port = $state('');
  let baud = $state(115200);
  let mode = $state<'text' | 'hex'>('text');
  let ending = $state<LineEnding>('lf');
  let line = $state('');
  // `opening` until the core confirms with `serial-opened` (docs/api.md).
  let status = $state<'closed' | 'opening' | 'open'>('closed');
  let openedOn = $state<{ port: string; baud: number } | null>(null);
  let closedReason = $state<string | null>(null);
  let error = $state<ErrorBody | null>(null);
  let output = $state('');
  let received = $state(0);
  let follow = $state(true);

  let session: { send(d: Uint8Array): void; close(): void } | null = null;
  const log = new ByteLog(KEEP_BYTES);
  let renderQueued = false;
  let pre: HTMLPreElement | undefined = $state();

  const ports = $derived(core.ports ?? []);

  // Pick the first free port once the list arrives, so Open is one click away.
  $effect(() => {
    if (port === '' && ports.length > 0)
      port = (ports.find((p) => portAvailability(p).free) ?? ports[0]!).id;
  });

  $effect(() => () => session?.close());

  function open() {
    error = null;
    closedReason = null;
    log.clear();
    render();
    const target = { port, baud };
    status = 'opening';
    openedOn = target;
    session = core.openSerial(port, baud, {
      onOpened: () => (status = 'open'),
      onData: (data) => {
        log.append(data);
        queueRender();
      },
      onClosed: (reason) => {
        session = null;
        status = 'closed';
        if (reason) closedReason = reason;
      },
      onError: (e) => (error = e),
    });
  }

  function close() {
    session?.close();
    session = null;
    status = 'closed';
  }

  function send(e: SubmitEvent) {
    e.preventDefault();
    if (!session) return;
    session.send(encodeLine(line, ending));
    line = '';
  }

  function queueRender() {
    if (renderQueued) return;
    renderQueued = true;
    requestAnimationFrame(() => {
      renderQueued = false;
      render();
    });
  }

  function render() {
    const bytes = log.bytes();
    received = log.total;
    if (mode === 'text') output = bytesToText(bytes);
    else {
      const lines = hexLines(bytes, 16, log.firstOffset);
      output = lines.slice(-HEX_LINES_SHOWN).join('\n');
    }
  }

  $effect(() => {
    void mode;
    render();
  });

  $effect(() => {
    void output;
    if (follow && pre && visible) pre.scrollTop = pre.scrollHeight;
  });

  function onScroll() {
    if (!pre) return;
    follow = pre.scrollHeight - pre.scrollTop - pre.clientHeight < 24;
  }
</script>

<div class="serial">
  <div class="controls">
    <label class="field">
      <span class="label">Port</span>
      <select class="select" bind:value={port} disabled={status !== 'closed'}>
        {#if ports.length === 0}<option value="">No ports found</option>{/if}
        {#each ports as p (p.id)}
          {@const a = portAvailability(p)}
          <option value={p.id}>{p.label} ({p.id}){a.free ? '' : ` · ${a.text}`}</option>
        {/each}
      </select>
    </label>
    <label class="field baud">
      <span class="label">Baud</span>
      <select class="select num" bind:value={baud} disabled={status !== 'closed'}>
        {#each BAUDS as b (b)}<option value={b}>{b}</option>{/each}
      </select>
    </label>
    {#if status !== 'closed'}
      <button class="btn" onclick={close}>Close port</button>
    {:else}
      <button class="btn primary" onclick={open} disabled={!port || core.connection !== 'connected'}
        >Open port</button
      >
    {/if}
  </div>

  {#if error}
    <div class="pad"><ErrorNotice {error} /></div>
  {/if}

  <div class="bar">
    <p class="state" role="status">
      {#if status === 'open' && openedOn}
        <span class="live" aria-hidden="true"></span> <code>{openedOn.port}</code> at
        <span class="num">{openedOn.baud}</span> baud
      {:else if status === 'opening' && openedOn}
        Opening <code>{openedOn.port}</code>…
      {:else if closedReason}
        Closed: {closedReason}
      {:else}
        Closed
      {/if}
      <span class="count num">{formatBytes(received)} received</span>
    </p>
    <div class="segmented" role="radiogroup" aria-label="Show as">
      <label><input type="radio" name="serial-mode" value="text" bind:group={mode} />Text</label>
      <label><input type="radio" name="serial-mode" value="hex" bind:group={mode} />Hex</label>
    </div>
    <button
      class="btn quiet icon"
      aria-label="Clear the output"
      title="Clear the output"
      onclick={() => {
        log.clear();
        render();
      }}
    >
      <Icon name="clear" />
    </button>
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex (a scrollable region must be reachable by keyboard) -->
  <pre
    class="out selectable"
    bind:this={pre}
    onscroll={onScroll}
    aria-label="Received data"
    tabindex="0">{output ||
      (status === 'open' ? 'Waiting for data…' : 'Open a port to see what it sends.')}</pre>

  <form class="send" onsubmit={send}>
    <label class="visually-hidden" for="serial-line">Line to send</label>
    <input
      id="serial-line"
      class="input mono"
      bind:value={line}
      placeholder={status === 'open' ? 'Type a line and press Enter' : 'Open a port to send'}
      disabled={status !== 'open'}
      autocomplete="off"
      spellcheck="false"
    />
    <label class="visually-hidden" for="serial-ending">Line ending</label>
    <select id="serial-ending" class="select" bind:value={ending} disabled={status !== 'open'}>
      <option value="lf">LF</option>
      <option value="crlf">CR LF</option>
      <option value="cr">CR</option>
      <option value="none">No ending</option>
    </select>
    <button class="btn" type="submit" disabled={status !== 'open'}
      ><Icon name="send" size={17} /> Send</button
    >
  </form>
</div>

<style>
  .serial {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    background: var(--surface);
  }
  .controls {
    display: flex;
    align-items: flex-end;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--line);
    background: var(--surface-2);
  }
  .controls .field:first-child {
    flex: 1;
    min-width: 0;
  }
  .controls .select {
    width: 100%;
  }
  .baud {
    width: 8.5rem;
  }
  .pad {
    padding: var(--space-3) var(--space-4) 0;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4);
  }
  .state {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    flex-wrap: wrap;
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .live {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--ok);
    box-shadow: 0 0 0 3px var(--ok-soft);
  }
  .count {
    margin-left: auto;
    padding-left: var(--space-3);
  }
  .out {
    flex: 1;
    min-height: 0;
    margin: 0 var(--space-4);
    padding: var(--space-3);
    overflow: auto;
    border-radius: var(--radius-md);
    background: var(--surface-sunken);
    border: 1px solid var(--line);
    font-family: var(--font-data);
    font-size: var(--text-sm);
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-all;
  }
  .send {
    display: flex;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
  }
  .send .input {
    flex: 1;
  }
</style>
