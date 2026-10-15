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
  import { DisplayPrefs } from './components/display.svelte';
  import { WINDOW_DEFS, WINDOW_KINDS } from './windows/registry';
  import { taskbarHeight } from './lib/desktop/taskbar';
  import { pageTitle } from './lib/desktop/machineName';

  const core = createCore();
  const storage = browserStorage();
  const desktop = new DesktopController(WINDOW_KINDS, storage, viewport());
  const media = (query: string) => window.matchMedia?.(query) ?? null;
  const theme = new ThemeController(
    storage,
    media('(prefers-color-scheme: dark)'),
    media('(prefers-contrast: more)'),
  );
  const display = new DisplayPrefs(media('(prefers-reduced-motion: reduce)'));
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
    if (!core.settings) return;
    theme.setPreference(core.settings.appearance.theme);
    theme.setStyle(core.settings.appearance.style);
    theme.setContrast(core.settings.accessibility.higherContrast);
    display.set(core.settings.accessibility);
    desktop.setTaskbarHeight(taskbarHeight(core.settings.taskbar.size));
  });

  $effect(() => {
    document.documentElement.dataset.theme = theme.resolved;
  });

  $effect(() => {
    document.title = pageTitle(core.settings?.machine.name);
  });

  // Accessibility (D40): base.css turns animations off under data-motion="reduce", and every
  // size is in rem, so the root font size scales all text.
  $effect(() => {
    const root = document.documentElement;
    root.dataset.motion = display.reduceMotion ? 'reduce' : 'full';
    root.style.fontSize = display.fontSize;
  });

  // The style's colours, set on the page root. Until the settings arrive, index.html's copy of
  // the last style (or tokens.css) paints, so a non-default style doesn't flash.
  $effect(() => {
    if (!theme.style) return;
    const root = document.documentElement.style;
    for (const [name, value] of Object.entries(theme.tokens)) root.setProperty(name, value);
  });

  $effect(() => {
    document.documentElement.style.setProperty('--taskbar-h', `${desktop.taskbarHeight}px`);
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
{#if core.apiMismatch}
  <Boot status={core.connection} mismatch={{ shell: FABER_API, core: core.system?.api }} />
{:else if !ready}
  <Boot status={core.connection} />
{/if}
