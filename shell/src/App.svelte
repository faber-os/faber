<script lang="ts">
  import { onMount } from 'svelte';
  import { createCore, provideCore } from './lib/state/context';
  import Boot from './components/Boot.svelte';
  import SignInScreen from './components/SignInScreen.svelte';
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
  import { locale, t } from './lib/i18n';

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
  /**
   * Who is at the shell (docs/auth.md): `checking` until the core says, `out` while nobody is
   * signed in (the sign-in screen, or "Create the admin" on a fresh machine), `in` once they are.
   * The desktop connects to the core only once someone is in.
   */
  let gate = $state<'checking' | 'out' | 'in'>('checking');
  let started = false;
  let retry: ReturnType<typeof setTimeout> | undefined;

  async function checkSession() {
    const r = await core.auth.load();
    if (!r.ok) {
      // The core isn't there yet: the loading screen says so; try again.
      retry = setTimeout(() => void checkSession(), 2000);
      return;
    }
    if (r.value.identity) enter();
    else gate = 'out';
  }

  function enter() {
    gate = 'in';
    if (started) return;
    started = true;
    core.start();
  }

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

  // The shell's language (D44): a translation built in or on the machine, else English. Before
  // signing in, the session says which, so the sign-in screen speaks it too.
  $effect(() => {
    void core.useLanguage(core.settings?.appearance.language ?? core.auth.info?.language ?? 'en');
  });

  // Signed out while in (here, in another tab, a password change, a user removed): start over
  // at the sign-in screen, with nothing of the last user's left in the page.
  $effect(() => {
    if (gate === 'in' && core.auth.info && !core.auth.info.identity) window.location.reload();
  });

  // The connection dropped: it may be the session that ended (the core closes it then).
  $effect(() => {
    if (gate === 'in' && core.connection === 'reconnecting') void core.auth.load();
  });

  $effect(() => {
    document.documentElement.lang = locale.language;
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
    void checkSession();
    // Step 1 of the ladder is "add a part and watch it": start where that happens.
    desktop.open('parts');
    return () => {
      clearTimeout(retry);
      desktop.flush();
      if (started) core.stop();
    };
  });
</script>

<svelte:window
  onresize={() => desktop.setViewport(viewport())}
  onbeforeunload={() => desktop.flush()}
/>

<Desktop defs={WINDOW_DEFS} />
{#if core.auth.info?.signInOff}
  <p class="no-auth" role="status">{t('auth.off.banner')}</p>
{/if}
{#if gate === 'out'}
  <SignInScreen mode={core.auth.info?.needsSetup ? 'setup' : 'sign-in'} ondone={enter} />
{:else if core.auth.prompt}
  <!-- The attached screen signing in as someone: the page starts again as them. -->
  <SignInScreen
    mode={core.auth.prompt}
    ondone={() => window.location.reload()}
    oncancel={() => (core.auth.prompt = null)}
  />
{:else if core.apiMismatch}
  <Boot status={core.connection} mismatch={{ shell: FABER_API, core: core.system?.api }} />
{:else if !ready}
  <Boot status={gate === 'checking' ? 'connecting' : core.connection} />
{/if}

<style>
  .no-auth {
    position: fixed;
    top: 0;
    left: 50%;
    transform: translateX(-50%);
    z-index: 200000;
    margin: 0;
    padding: var(--space-1) var(--space-3);
    border-radius: 0 0 var(--radius-md) var(--radius-md);
    background: var(--warn);
    color: var(--surface);
    font-size: var(--text-sm);
    font-weight: 700;
    pointer-events: none;
  }
</style>
