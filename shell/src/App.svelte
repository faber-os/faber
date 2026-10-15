<script lang="ts">
  import { onMount } from 'svelte';
  import { createCore, provideCore } from './lib/state/context';
  import Boot from './components/Boot.svelte';
  import Desktop from './components/Desktop.svelte';
  import {
    browserStorage,
    DesktopController,
    provideDesktop,
  } from './components/desktopController.svelte';
  import { MenuController, provideMenu } from './components/menuController.svelte';
  import { provideTheme, ThemeController } from './components/theme.svelte';
  import { WINDOW_DEFS, WINDOW_KINDS } from './windows/registry';

  const core = createCore();
  const storage = browserStorage();
  const desktop = new DesktopController(WINDOW_KINDS, storage, viewport());
  const theme = new ThemeController(
    storage,
    window.matchMedia?.('(prefers-color-scheme: dark)') ?? null,
  );
  provideCore(core);
  provideDesktop(desktop);
  provideTheme(theme);
  provideMenu(new MenuController());

  /** Until the core has answered once, the loading screen shows instead of an empty desktop. */
  let ready = $state(false);

  function viewport() {
    return { width: window.innerWidth, height: window.innerHeight };
  }

  $effect(() => {
    if (core.settings) theme.setPreference(core.settings.appearance.theme);
  });

  $effect(() => {
    document.documentElement.dataset.theme = theme.resolved;
  });

  // Ready once connected with settings; a core that never sends settings still gets in after 3 s.
  $effect(() => {
    if (ready || core.connection !== 'connected') return;
    if (core.settings) {
      ready = true;
      return;
    }
    const t = setTimeout(() => (ready = true), 3000);
    return () => clearTimeout(t);
  });

  onMount(() => {
    core.start();
    // Step 1 of the ladder is "add a part and watch it": start where that happens.
    desktop.open('parts');
    return () => {
      desktop.flush();
      core.stop();
    };
  });
</script>

<svelte:window
  onresize={() => desktop.setViewport(viewport())}
  onbeforeunload={() => desktop.flush()}
/>

<Desktop defs={WINDOW_DEFS} />
{#if !ready}
  <Boot status={core.connection} />
{/if}
