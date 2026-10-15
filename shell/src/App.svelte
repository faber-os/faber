<script lang="ts">
  import { onMount } from 'svelte';
  import { createCore, provideCore } from './lib/state/context';
  import Desktop from './components/Desktop.svelte';
  import {
    browserStorage,
    DesktopController,
    provideDesktop,
  } from './components/desktopController.svelte';
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

  function viewport() {
    return { width: window.innerWidth, height: window.innerHeight };
  }

  $effect(() => {
    document.documentElement.dataset.theme = theme.resolved;
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
