<script lang="ts">
  import { useCore } from '../lib/state/context';
  import { portAvailability } from '../lib/state/portHints';
  import { defaultRecordingName, validateRecordingName } from '../lib/state/recordingName';
  import {
    ByteLog,
    bytesToText,
    encodeLine,
    hexLines,
    type LineEnding,
  } from '../lib/state/serialFormat';
  import type { ErrorBody, SerialSession } from '../lib/state/types';
  import { useDesktop } from '../components/desktopController.svelte';
  import { openFolder } from '../components/fileOps';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import { formatBytes } from '../components/format';
  import Icon from '../components/Icon.svelte';
  import PortProblems from '../components/PortProblems.svelte';
  import type { WindowProps } from '../components/windowDefs';

  let { win, visible }: WindowProps = $props();

  const BAUDS = [9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600];
  const KEEP_BYTES = 64 * 1024;
  const HEX_LINES_SHOWN = 2000;

  const core = useCore();
  const desktop = useDesktop();

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

  let session: SerialSession | null = null;

  // Recording (docs/api.md, "Recordings"): idle → naming → starting → recording → idle.
  let rec = $state<'idle' | 'naming' | 'starting' | 'recording'>('idle');
  let recName = $state('');
  let recPath = $state<string | null>(null);
  let recBytes = $state(0);
  let recTouched = $state(false);
  let lastRecording = $state<{ path: string; bytes: number } | null>(null);
  const recProblem = $derived(validateRecordingName(recName));
  const log = new ByteLog(KEEP_BYTES);
  let renderQueued = false;
  let pre: HTMLPreElement | undefined = $state();

  const ports = $derived(core.ports ?? []);

  // Opened for one port (Parts → "Open port in serial monitor"): pick it, if nothing is open.
  $effect(() => {
    const wanted = win.params.port;
    if (!wanted) return;
    if (status === 'closed') port = wanted;
    desktop.update(win.id, { params: {} });
  });

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
        if (rec === 'recording') recBytes += data.length;
        queueRender();
      },
      onClosed: (reason) => {
        session = null;
        status = 'closed';
        if (rec !== 'idle') rec = 'idle';
        if (reason) closedReason = reason;
      },
      onError: (e) => {
        error = e;
        // The core answers a record request with either serial-recording or an error.
        if (rec === 'starting') rec = 'naming';
      },
      onRecording: (path) => {
        rec = 'recording';
        recPath = path;
        recBytes = 0;
        lastRecording = null;
      },
      onRecordingStopped: (path, bytes) => {
        rec = 'idle';
        recPath = null;
        lastRecording = { path, bytes };
      },
    });
  }

  function startNaming() {
    recName = defaultRecordingName(new Date());
    recTouched = false;
    rec = 'naming';
  }

  function startRecording(e: SubmitEvent) {
    e.preventDefault();
    recTouched = true;
    if (recProblem || !session) return;
    error = null;
    rec = 'starting';
    session.startRecording(recName);
  }

  function stopRecording() {
    session?.stopRecording();
  }

  function close() {
    // Closing ends a recording too. Say what was saved from what this window counted; the core's
    // own count (serial-recording-stopped, which still arrives after the close) replaces it.
    if (rec === 'recording' && recPath) lastRecording = { path: recPath, bytes: recBytes };
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
          <option value={p.id}
            >{p.kind === 'bluetooth' ? 'Bluetooth: ' : ''}{p.label} ({p.id}){a.free
              ? ''
              : ` · ${a.text}`}</option
          >
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

  {#if core.portProblems.length && status === 'closed'}
    <div class="pad problems"><PortProblems /></div>
  {/if}

  {#if error}
    <div class="pad"><ErrorNotice {error} /></div>
  {/if}

  {#if status === 'open' && rec === 'naming'}
    <form class="record" onsubmit={startRecording} aria-label="Record">
      <label class="field grow">
        <span class="label">Recording name</span>
        <input
          class="input mono"
          bind:value={recName}
          aria-invalid={recTouched && !!recProblem}
          aria-describedby="{win.id}-rec-help"
          autocomplete="off"
          spellcheck="false"
        />
      </label>
      <button class="btn primary" type="submit"><Icon name="record" size={16} /> Start</button>
      <button class="btn quiet" type="button" onclick={() => (rec = 'idle')}>Cancel</button>
      <p id="{win.id}-rec-help" class="help" class:bad={recTouched && !!recProblem}>
        {#if recTouched && recProblem}{recProblem.message} {recProblem.hint}{:else}Saved in
          Recordings as <code>{recName || '…'}.fbcap</code>, and usable as a replay port at once.{/if}
      </p>
    </form>
  {:else if rec === 'starting' || rec === 'recording'}
    <div class="record live-rec" role="status">
      <span class="rec-dot" aria-hidden="true"></span>
      {#if rec === 'starting'}
        <span>Starting the recording…</span>
      {:else}
        <span
          >Recording to <code>{recPath}</code> (<span class="num">{formatBytes(recBytes)}</span
          >)</span
        >
      {/if}
      <span class="grow"></span>
      <button class="btn" onclick={stopRecording} disabled={rec !== 'recording'}>
        <Icon name="stop" size={15} /> Stop
      </button>
    </div>
  {:else if lastRecording}
    <div class="record done" role="status">
      <span
        >Saved <code>{lastRecording.path}</code> (<span class="num"
          >{formatBytes(lastRecording.bytes)}</span
        >).</span
      >
      <span class="grow"></span>
      <button class="btn quiet" onclick={() => openFolder(desktop, 'home:/Recordings')}
        >Show in Files</button
      >
      <button class="btn quiet icon" aria-label="Dismiss" onclick={() => (lastRecording = null)}
        ><Icon name="close" /></button
      >
    </div>
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
    {#if status === 'open' && rec === 'idle'}
      <button class="btn" onclick={startNaming}><Icon name="record" size={16} /> Record</button>
    {/if}
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
  .problems {
    display: grid;
    gap: var(--space-2);
  }
  .record {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: var(--space-2) var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--line);
    font-size: var(--text-sm);
  }
  .record .grow {
    flex: 1;
    min-width: 0;
  }
  .record .help {
    flex-basis: 100%;
    color: var(--text-muted);
  }
  .record .help.bad {
    color: var(--danger);
    font-weight: 600;
  }
  .live-rec,
  .done {
    align-items: center;
  }
  .live-rec {
    background: var(--danger-soft);
  }
  .rec-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--danger);
    animation: rec 1.2s ease-in-out infinite;
  }
  @keyframes rec {
    50% {
      opacity: 0.35;
    }
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
