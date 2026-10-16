<script lang="ts">
  import { useCore } from '../../lib/state/context';
  import { diagnosticsText, logsText } from '../../lib/state/diagnostics';
  import type { AboutInfo, ErrorBody, LogEntry } from '../../lib/state/types';
  import { formatLogTime, wallParts } from '../../lib/desktop/wallClock';
  import Dialog from '../../components/Dialog.svelte';
  import ErrorNotice from '../../components/ErrorNotice.svelte';
  import { formatCoreTime } from '../../components/format';
  import { t } from '../../lib/i18n';

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

  /** "1 h 2 min" or "4 min 5 s". */
  function uptime(s: number): string {
    const hours = Math.floor(s / 3600);
    const minutes = Math.floor((s % 3600) / 60);
    return hours > 0
      ? t('settings.about.uptime.hours', { hours, minutes })
      : t('settings.about.uptime.minutes', { minutes, seconds: s % 60 });
  }

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

  // Every setting, the machine's own too: an admin's (docs/auth.md).
  const resetLock = $derived(core.auth.lock('admin'));

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
  <h3 id="{winId}-about-h">{t('settings.about.title')}</h3>
  {#if error}<ErrorNotice {error} />{/if}
  <dl class="facts">
    <div>
      <dt>{t('settings.about.version')}</dt>
      <dd class="num">{about?.version ?? '…'}</dd>
    </div>
    <div>
      <dt>{t('settings.about.api')}</dt>
      <dd><code class="selectable">{about?.api ?? '…'}</code></dd>
    </div>
    <div>
      <dt>{t('settings.about.dataFolder')}</dt>
      <dd><code class="selectable path">{about?.dataFolder ?? '…'}</code></dd>
    </div>
    <div>
      <dt>{t('settings.about.uptime')}</dt>
      <dd>{about ? uptime(about.uptimeS) : '…'}</dd>
    </div>
    <div>
      <dt>{t('settings.about.os')}</dt>
      <dd>{about?.os ?? '…'}</dd>
    </div>
    <div>
      <dt>{t('settings.about.timeZone')}</dt>
      <dd>{about?.timeZone ?? '…'}</dd>
    </div>
  </dl>

  <div class="row">
    <div class="text">
      <p class="name">{t('settings.about.diagnostics.name')}</p>
      <p class="desc">{t('settings.about.diagnostics.desc')}</p>
      {#if copied}<p class="done" role="status">{t('settings.about.diagnostics.copied')}</p>{/if}
    </div>
    <div class="buttons">
      <button class="btn" onclick={copy}>{t('settings.about.diagnostics.copy')}</button>
      <button class="btn" onclick={download}>{t('settings.about.diagnostics.download')}</button>
    </div>
  </div>

  <div class="row">
    <div class="text">
      <p class="name">{t('settings.about.reset.name')}</p>
      <p class="desc">{t('settings.about.reset.desc')}</p>
    </div>
    <button
      class="btn danger"
      disabled={resetLock !== null}
      title={resetLock ?? undefined}
      onclick={() => (confirmReset = true)}>{t('settings.about.reset.open')}</button
    >
  </div>
</section>

{#if manual !== null}
  <Dialog title={t('settings.about.diagnostics.manualTitle')} oncancel={() => (manual = null)} wide>
    <p>{t('settings.about.diagnostics.manual')}</p>
    <textarea class="input manual" readonly rows="10" value={manual}></textarea>
    {#snippet actions()}
      <button class="btn" onclick={() => (manual = null)}
        >{t('settings.about.diagnostics.done')}</button
      >
    {/snippet}
  </Dialog>
{/if}

{#if confirmReset}
  <Dialog
    title={t('settings.about.reset.title')}
    tone="danger"
    oncancel={() => (confirmReset = false)}
  >
    <p>{t('settings.about.reset.body')}</p>
    <p>{t('settings.about.reset.untouched')}</p>
    {#snippet actions()}
      <button class="btn" onclick={() => (confirmReset = false)}
        >{t('settings.about.reset.cancel')}</button
      >
      <button class="btn danger solid" disabled={resetting} onclick={reset}
        >{t('settings.about.reset.confirm')}</button
      >
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
