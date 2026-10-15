<script lang="ts">
  import { useCore } from '../../lib/state/context';
  import { diagnosticsText, formatUptime, logsText } from '../../lib/state/diagnostics';
  import type { AboutInfo, ErrorBody, LogEntry } from '../../lib/state/types';
  import { formatLogTime, wallParts } from '../../lib/desktop/wallClock';
  import Dialog from '../../components/Dialog.svelte';
  import ErrorNotice from '../../components/ErrorNotice.svelte';
  import { formatCoreTime } from '../../components/format';

  let { winId }: { winId: string } = $props();

  const core = useCore();

  let about = $state<AboutInfo | null>(null);
  let error = $state<ErrorBody | null>(null);
  let copied = $state(false);
  /** Diagnostics to copy by hand, when the browser won't let the page use the clipboard. */
  let manual = $state<string | null>(null);
  let confirmReset = $state(false);
  let resetting = $state(false);

  $effect(() => {
    void core.loadAbout().then((r) => {
      if (r.ok) about = r.value;
      else error = r.error;
    });
  });

  function logTime(e: LogEntry): string {
    const ms = core.coreTimeToEpochMs(e.tUs);
    return ms === null ? formatCoreTime(e.tUs) : formatLogTime(ms, core.wallClock);
  }

  function diagnostics(): string {
    return diagnosticsText({
      about,
      ports: core.ports ?? [],
      parts: core.parts ?? [],
      logs: core.logs,
      logTime,
    });
  }

  async function copy() {
    copied = false;
    const text = diagnostics();
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
    } catch {
      // The clipboard needs a secure page (https or localhost); show the text to copy instead.
      manual = text;
    }
  }

  function download() {
    const p = wallParts(Date.now(), core.wallClock);
    const n = (x: number) => String(x).padStart(2, '0');
    const name = `faber-logs-${p.year}-${n(p.month)}-${n(p.day)}-${n(p.hour)}${n(p.minute)}.txt`;
    const blob = new Blob([logsText(core.logs, logTime, about?.dataFolder ?? null)], {
      type: 'text/plain;charset=utf-8',
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = name;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  async function reset() {
    resetting = true;
    error = null;
    const r = await core.resetSettings();
    resetting = false;
    confirmReset = false;
    if (!r.ok) error = r.error;
  }
</script>

<section id="{winId}-about" aria-labelledby="{winId}-about-h">
  <h3 id="{winId}-about-h">About and diagnostics</h3>
  {#if error}<ErrorNotice {error} />{/if}
  <dl class="facts">
    <div>
      <dt>Version</dt>
      <dd class="num">{about?.version ?? '…'}</dd>
    </div>
    <div>
      <dt>API</dt>
      <dd><code class="selectable">{about?.api ?? '…'}</code></dd>
    </div>
    <div>
      <dt>Data folder</dt>
      <dd><code class="selectable path">{about?.dataFolder ?? '…'}</code></dd>
    </div>
    <div>
      <dt>Running for</dt>
      <dd>{about ? formatUptime(about.uptimeS) : '…'}</dd>
    </div>
    <div>
      <dt>Operating system</dt>
      <dd>{about?.os ?? '…'}</dd>
    </div>
    <div>
      <dt>Time zone</dt>
      <dd>{about?.timeZone ?? '…'}</dd>
    </div>
  </dl>

  <div class="row">
    <div class="text">
      <p class="name">Diagnostics</p>
      <p class="desc">
        For a bug report: version, operating system, ports, part states and the last 200 log lines.
        No names, folders or personal data.
      </p>
      {#if copied}<p class="done" role="status">Copied.</p>{/if}
    </div>
    <div class="buttons">
      <button class="btn" onclick={copy}>Copy diagnostics</button>
      <button class="btn" onclick={download}>Download logs</button>
    </div>
  </div>

  <div class="row">
    <div class="text">
      <p class="name">Reset settings to defaults</p>
      <p class="desc">Every setting goes back to how a new machine has it. Parts and files stay.</p>
    </div>
    <button class="btn danger" onclick={() => (confirmReset = true)}>Reset settings…</button>
  </div>
</section>

{#if manual !== null}
  <Dialog title="Copy diagnostics" oncancel={() => (manual = null)} wide>
    <p>This browser won't let Faber use the clipboard here. Select the text and copy it.</p>
    <textarea class="input manual" readonly rows="10" value={manual}></textarea>
    {#snippet actions()}
      <button class="btn" onclick={() => (manual = null)}>Done</button>
    {/snippet}
  </Dialog>
{/if}

{#if confirmReset}
  <Dialog title="Reset every setting?" tone="danger" oncancel={() => (confirmReset = false)}>
    <p>
      Appearance, accessibility, units, date and time, the taskbar and where desktop icons sit go
      back to how a new machine has them, and whole-machine file access is turned off.
    </p>
    <p>Parts and files are not touched.</p>
    {#snippet actions()}
      <button class="btn" onclick={() => (confirmReset = false)}>Cancel</button>
      <button class="btn danger solid" disabled={resetting} onclick={reset}>Reset settings</button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .facts {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(14rem, 1fr));
    gap: var(--space-3) var(--space-4);
    margin: 0;
    padding-bottom: var(--space-4);
    border-bottom: 1px solid var(--line);
  }
  .facts div {
    min-width: 0;
  }
  dt {
    font-size: var(--text-xs);
    font-weight: 700;
    color: var(--text-muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .path {
    font-size: var(--text-sm);
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .done {
    margin-top: var(--space-1);
    font-size: var(--text-sm);
    color: var(--ok);
  }
  .manual {
    width: 100%;
    font-family: var(--font-data);
    font-size: var(--text-xs);
    resize: vertical;
  }
</style>
