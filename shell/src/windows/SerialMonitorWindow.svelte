<script lang="ts">
  import { useCore } from '../lib/state/context';
  import { labelNamesId, portAvailability } from '../lib/state/portHints';
  import { defaultRecordingName, validateRecordingName } from '../lib/state/recordingName';
  import { wallParts } from '../lib/desktop/wallClock';
  import {
    ByteLog,
    bytesToText,
    encodeLine,
    hexLines,
    type LineEnding,
  } from '../lib/state/serialFormat';
  import type { ErrorBody, SerialSession } from '../lib/state/types';
  import { t } from '../lib/i18n';
  import { useDesktop } from '../components/desktopController.svelte';
  import { openFolder } from '../components/fileOps';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import { formatBytes } from '../components/format';
  import Icon from '../components/Icon.svelte';
  import PortProblems from '../components/PortProblems.svelte';
  import RichText from '../components/RichText.svelte';
  import type { WindowProps } from '../components/windowDefs';

  let { win, visible }: WindowProps = $props();

  const BAUDS = [9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600];
  const KEEP_BYTES = 64 * 1024;
  const HEX_LINES_SHOWN = 2000;

  const core = useCore();
  // The serial monitor writes to a device: an operator's (docs/auth.md).
  const lock = $derived(core.auth.lock('operator'));
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
  /**
   * The part that holds the port this window tried to open, when the core said so: it can be
   * paused from here (a tool's claim, like the flasher's, can't).
   */
  const holder = $derived.by(() => {
    if (error?.code !== 'port-claimed' || !openedOn) return null;
    const by = ports.find((p) => p.id === openedOn?.port)?.claimedBy;
    return by == null || by.startsWith('tool:') ? null : by;
  });
  let pausing = $state(false);

  /** Pauses the part on the port (it frees the port), then opens the port here. */
  async function pauseAndOpen(part: string) {
    pausing = true;
    const r = await core.pausePart(part);
    pausing = false;
    if (!r.ok) {
      error = r.error;
      return;
    }
    if (openedOn) port = openedOn.port;
    open();
  }

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
    recName = defaultRecordingName(wallParts(Date.now(), core.wallClock));
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
      <span class="label">{t('serial.port.label')}</span>
      <select class="select" bind:value={port} disabled={status !== 'closed'}>
        {#if ports.length === 0}<option value="">{t('serial.port.none')}</option>{/if}
        {#each ports as p (p.id)}
          {@const a = portAvailability(p)}
          {@const name = labelNamesId(p)
            ? p.kind === 'bluetooth'
              ? t('serial.port.optionBluetoothNoId', { label: p.label })
              : p.label
            : t(p.kind === 'bluetooth' ? 'serial.port.optionBluetooth' : 'serial.port.option', {
                label: p.label,
                id: p.id,
              })}
          <option value={p.id}
            >{a.free ? name : t('serial.port.optionBusy', { option: name, reason: a.text })}</option
          >
        {/each}
      </select>
    </label>
    <label class="field baud">
      <span class="label">{t('serial.baud.label')}</span>
      <select class="select num" bind:value={baud} disabled={status !== 'closed'}>
        {#each BAUDS as b (b)}<option value={b}>{b}</option>{/each}
      </select>
    </label>
    {#if status !== 'closed'}
      <button class="btn" onclick={close}>{t('serial.close')}</button>
    {:else}
      <button
        class="btn primary"
        onclick={open}
        disabled={!port || core.connection !== 'connected' || lock !== null}
        title={lock ?? undefined}>{t('serial.open')}</button
      >
    {/if}
  </div>

  {#if core.portProblems.length && status === 'closed'}
    <div class="pad problems"><PortProblems /></div>
  {/if}

  {#if error}
    <div class="pad">
      <ErrorNotice {error}>
        {#snippet actions()}
          {#if holder}
            <button class="btn" disabled={pausing} onclick={() => pauseAndOpen(holder)}
              >{t('serial.pausePart', { part: holder })}</button
            >
          {/if}
        {/snippet}
      </ErrorNotice>
    </div>
  {/if}

  {#if status === 'open' && rec === 'naming'}
    <form class="record" onsubmit={startRecording} aria-label={t('serial.record.label')}>
      <label class="field grow">
        <span class="label">{t('serial.record.name')}</span>
        <input
          class="input mono"
          bind:value={recName}
          aria-invalid={recTouched && !!recProblem}
          aria-describedby="{win.id}-rec-help"
          autocomplete="off"
          spellcheck="false"
        />
      </label>
      <button class="btn primary" type="submit"
        ><Icon name="record" size={16} /> {t('serial.record.start')}</button
      >
      <button class="btn quiet" type="button" onclick={() => (rec = 'idle')}
        >{t('serial.record.cancel')}</button
      >
      <p id="{win.id}-rec-help" class="help" class:bad={recTouched && !!recProblem}>
        {#if recTouched && recProblem}{recProblem.message} {recProblem.hint}{:else}<RichText
            text={t('serial.record.help', { name: recName || '…' })}
          />{/if}
      </p>
    </form>
  {:else if rec === 'starting' || rec === 'recording'}
    <div class="record live-rec" role="status">
      <span class="rec-dot" aria-hidden="true"></span>
      {#if rec === 'starting'}
        <span>{t('serial.record.starting')}</span>
      {:else}
        <span class="num"
          ><RichText
            text={t('serial.record.recordingTo', {
              path: recPath ?? '',
              size: formatBytes(recBytes),
            })}
          /></span
        >
      {/if}
      <span class="grow"></span>
      <button class="btn" onclick={stopRecording} disabled={rec !== 'recording'}>
        <Icon name="stop" size={15} />
        {t('serial.record.stop')}
      </button>
    </div>
  {:else if lastRecording}
    <div class="record done" role="status">
      <span class="num"
        ><RichText
          text={t('serial.record.saved', {
            path: lastRecording.path,
            size: formatBytes(lastRecording.bytes),
          })}
        /></span
      >
      <span class="grow"></span>
      <button class="btn quiet" onclick={() => openFolder(desktop, 'home:/Recordings')}
        >{t('serial.record.showInFiles')}</button
      >
      <button
        class="btn quiet icon"
        aria-label={t('serial.record.dismiss')}
        onclick={() => (lastRecording = null)}><Icon name="close" /></button
      >
    </div>
  {/if}

  <div class="bar">
    <p class="state" role="status">
      {#if status === 'open' && openedOn}
        <span class="live" aria-hidden="true"></span>
        <RichText text={t('serial.state.open', { port: openedOn.port, baud: openedOn.baud })} />
      {:else if status === 'opening' && openedOn}
        <RichText text={t('serial.state.opening', { port: openedOn.port })} />
      {:else if closedReason}
        {t('serial.state.closedWithReason', { reason: closedReason })}
      {:else}
        {t('serial.state.closed')}
      {/if}
      <span class="count num">{t('serial.received', { size: formatBytes(received) })}</span>
    </p>
    {#if status === 'open' && rec === 'idle'}
      <button class="btn" onclick={startNaming}
        ><Icon name="record" size={16} /> {t('serial.record.button')}</button
      >
    {/if}
    <div class="segmented" role="radiogroup" aria-label={t('serial.mode.label')}>
      <label
        ><input type="radio" name="serial-mode" value="text" bind:group={mode} />{t(
          'serial.mode.text',
        )}</label
      >
      <label
        ><input type="radio" name="serial-mode" value="hex" bind:group={mode} />{t(
          'serial.mode.hex',
        )}</label
      >
    </div>
    <button
      class="btn quiet icon"
      aria-label={t('serial.clear')}
      title={t('serial.clear')}
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
    aria-label={t('serial.output.label')}
    tabindex="0">{output ||
      (status === 'open' ? t('serial.output.waiting') : t('serial.output.closed'))}</pre>

  <form class="send" onsubmit={send}>
    <label class="visually-hidden" for="serial-line">{t('serial.send.label')}</label>
    <input
      id="serial-line"
      class="input mono"
      bind:value={line}
      placeholder={status === 'open'
        ? t('serial.send.placeholder')
        : t('serial.send.placeholderClosed')}
      disabled={status !== 'open'}
      autocomplete="off"
      spellcheck="false"
    />
    <label class="visually-hidden" for="serial-ending">{t('serial.send.ending')}</label>
    <select id="serial-ending" class="select" bind:value={ending} disabled={status !== 'open'}>
      <option value="lf">{t('serial.send.ending.lf')}</option>
      <option value="crlf">{t('serial.send.ending.crlf')}</option>
      <option value="cr">{t('serial.send.ending.cr')}</option>
      <option value="none">{t('serial.send.ending.none')}</option>
    </select>
    <button class="btn" type="submit" disabled={status !== 'open'}
      ><Icon name="send" size={17} /> {t('serial.send.button')}</button
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
    color: var(--danger-text);
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
