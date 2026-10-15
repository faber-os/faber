<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import { t } from '../lib/i18n';
  import { useCore } from '../lib/state/context';
  import { AppHost } from '../lib/state/appHost';
  import { useTheme } from '../components/theme.svelte';
  import { themeForApps } from '../components/appWindow';
  import Dialog from '../components/Dialog.svelte';
  import Icon from '../components/Icon.svelte';
  import type { WindowProps } from '../components/windowDefs';

  /**
   * An app's window (docs/apps.md §9): its UI in an `<iframe sandbox="allow-scripts">`, never
   * with allow-same-origin, so the page has an opaque origin and can't reach the core, the shell
   * or the network. It talks to the shell only through `postMessage`, answered by an AppHost that
   * checks every message and every request against the app's roles. Nothing the app sends is ever
   * shown as HTML here; its title is text.
   */
  let { win }: WindowProps = $props();

  const core = useCore();
  const theme = useTheme();

  const appId = $derived(win.params.app ?? '');
  const app = $derived(core.apps?.find((a) => a.id === appId) ?? null);
  const machine = $derived(core.settings?.machine.name ?? 'faber');

  let frame: HTMLIFrameElement | undefined = $state();
  /** The parts this window drives now, shown above the frame with a Stop button. */
  let driving = $state<string[]>([]);
  /** D46: the user is asked once per session before an app's window drives anything. */
  let asking = $state<{ part: string; answer: (yes: boolean) => void } | null>(null);

  // svelte-ignore state_referenced_locally
  const host = new AppHost({
    appId: untrack(() => appId),
    app: () => core.apps?.find((a) => a.id === appId) ?? null,
    parts: () => core.parts ?? [],
    frame: () => frame?.contentWindow ?? null,
    post: (m) => frame?.contentWindow?.postMessage(m, '*'),
    readSignal: (address) => core.readSignal(address),
    watchSignal: (address, hz, handlers) => core.watchSignal(address, hz, handlers),
    leases: core,
    watchLeaseEnds: (fn) => core.watchLeaseEnds(fn),
    confirmDriving: (part) =>
      core.drivingConfirmed
        ? Promise.resolve(true)
        : new Promise<boolean>((answer) => {
            asking?.answer(false);
            asking = { part, answer };
          }),
    theme: () => themeForApps(theme.resolved),
    onDriving: (parts) => (driving = parts),
  });

  function onMessage(e: MessageEvent) {
    host.receive({ origin: e.origin, source: e.source, data: e.data });
  }

  $effect(() => {
    window.addEventListener('message', onMessage);
    return () => window.removeEventListener('message', onMessage);
  });

  // The app turned on or off, or got other parts: the page hears it, and loses what it may no
  // longer have.
  $effect(() => {
    void app;
    void core.parts;
    untrack(() => host.appChanged());
  });

  $effect(() => {
    void theme.resolved;
    void theme.tokens;
    untrack(() => host.themeChanged());
  });

  onDestroy(() => {
    asking?.answer(false);
    void host.dispose();
  });

  function answer(yes: boolean) {
    if (!asking) return;
    if (yes) core.confirmDriving();
    asking.answer(yes);
    asking = null;
  }
</script>

<div class="app-window" data-testid="app-window" data-app={appId}>
  {#if core.apps !== null && !app}
    <p class="missing">{t('appWindow.missing')}</p>
  {:else if app && !app.ui}
    <p class="missing">{t('appWindow.noUi', { app: app.name })}</p>
  {:else if app?.ui}
    {#if driving.length > 0}
      <div class="driving" role="status">
        <Icon name="drive" size={16} />
        <span>{t('appWindow.driving', { app: app.name, parts: driving.join(', ') })}</span>
        <button class="btn danger" onclick={() => void host.stopDriving()}
          >{t('appWindow.stop')}</button
        >
      </div>
    {/if}
    <iframe
      bind:this={frame}
      src={app.ui}
      title={t('appWindow.frameTitle', { app: app.name })}
      sandbox="allow-scripts"
      referrerpolicy="no-referrer"
      onload={() => host.reset()}
    ></iframe>
  {:else}
    <p class="missing">{t('appWindow.loading')}</p>
  {/if}
</div>

{#if asking && app}
  <Dialog title={t('drive.confirm.title', { machine })} oncancel={() => answer(false)}>
    <p>{t('appWindow.confirm.body', { app: app.name, part: asking.part })}</p>
    {#snippet actions()}
      <button class="btn" onclick={() => answer(false)}>{t('drive.confirm.cancel')}</button>
      <button class="btn primary" onclick={() => answer(true)}>{t('drive.confirm.start')}</button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .app-window {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--surface);
  }
  iframe {
    flex: 1;
    width: 100%;
    min-height: 0;
    border: 0;
    background: var(--surface);
  }
  .missing {
    padding: var(--space-5);
    color: var(--text-muted);
  }
  .driving {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    background: var(--warn-soft);
    font-size: var(--text-sm);
  }
  .driving span {
    flex: 1;
  }
</style>
