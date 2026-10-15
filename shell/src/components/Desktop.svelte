<script lang="ts">
  import { useCore } from '../lib/state/context';
  import { t } from '../lib/i18n';
  import { resolveWallpaper } from '../lib/desktop/wallpapers';
  import ContextMenu from './ContextMenu.svelte';
  import DesktopIcons from './DesktopIcons.svelte';
  import { useDesktop } from './desktopController.svelte';
  import ErrorNotice from './ErrorNotice.svelte';
  import NoticeToasts from './NoticeToasts.svelte';
  import Taskbar from './Taskbar.svelte';
  import type { WindowDef } from './windowDefs';
  import WindowFrame from './WindowFrame.svelte';
  import { windowIcon } from './appWindow';

  let { defs }: { defs: WindowDef[] } = $props();

  const core = useCore();
  const desktop = useDesktop();
  const byKind = $derived(new Map(defs.map((d) => [d.kind, d])));
  const wallpaper = $derived(resolveWallpaper(core.settings?.appearance.wallpaper));
</script>

<div class="desktop">
  <div class="wall wallpaper" data-wallpaper={wallpaper} aria-hidden="true"></div>

  <DesktopIcons {defs} />

  <main
    class="windows"
    aria-label={t('desktop.windowsArea')}
    style:height="{desktop.area.height}px"
  >
    {#each desktop.state.windows as win (win.id)}
      {@const def = byKind.get(win.kind)}
      {#if def}
        {@const Content = def.component}
        <WindowFrame {win} icon={def.icon} iconSrc={windowIcon(core, win)}>
          <Content {win} visible={!win.minimised} />
        </WindowFrame>
      {/if}
    {/each}
  </main>

  <div class="toasts">
    <NoticeToasts />
    {#if core.lastError}
      <div class="toast">
        <ErrorNotice error={core.lastError}>
          {#snippet actions()}
            <button class="btn quiet" onclick={() => (core.lastError = null)}
              >{t('desktop.toast.dismiss')}</button
            >
          {/snippet}
        </ErrorNotice>
      </div>
    {/if}
  </div>

  <Taskbar {defs} />
  <ContextMenu />
</div>

<style>
  .desktop {
    position: relative;
    height: 100%;
    overflow: hidden;
  }
  .wall {
    position: absolute;
    inset: 0;
  }
  /* The windows layer spans the desktop but lets clicks through to the icons between windows. */
  .windows {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    pointer-events: none;
  }
  .windows > :global(*) {
    pointer-events: auto;
  }
  /* Notices and the last error, stacked above the taskbar; clicks pass between them. */
  .toasts {
    position: absolute;
    right: var(--space-3);
    bottom: calc(var(--taskbar-h) + var(--space-3));
    width: min(420px, calc(100vw - 24px));
    z-index: 99998;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    pointer-events: none;
  }
  .toasts > :global(*) {
    pointer-events: auto;
  }
  .toast {
    border-radius: var(--radius-md);
    background: var(--surface);
    box-shadow: var(--shadow-pop);
  }
</style>
