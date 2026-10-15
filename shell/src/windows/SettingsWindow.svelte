<script lang="ts">
  import { useCore } from '../lib/state/context';
  import type { ErrorBody, Settings, Theme } from '../lib/state/types';
  import { WALLPAPERS, resolveWallpaper } from '../lib/desktop/wallpapers';
  import { STYLES, resolveStyle, styleSwatch } from '../lib/desktop/styles';
  import { isPinned } from '../lib/desktop/taskbar';
  import AppIcon from '../components/AppIcon.svelte';
  import Dialog from '../components/Dialog.svelte';
  import { useDesktop } from '../components/desktopController.svelte';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import Icon from '../components/Icon.svelte';
  import type { WindowDef, WindowProps } from '../components/windowDefs';
  import AboutSection from './settings/AboutSection.svelte';
  import AccessibilitySection from './settings/AccessibilitySection.svelte';
  import ConnectorsSection from './settings/ConnectorsSection.svelte';
  import StorageSection from './settings/StorageSection.svelte';
  import TimeSection from './settings/TimeSection.svelte';
  import MachineSection from './settings/MachineSection.svelte';
  import UnitsSection from './settings/UnitsSection.svelte';
  import LanguageRow from './settings/LanguageRow.svelte';
  import RichText from '../components/RichText.svelte';
  import { t } from '../lib/i18n';

  let { win }: WindowProps = $props();

  const core = useCore();
  const desktop = useDesktop();
  const confirmWord = $derived(t('settings.developer.warning.word'));

  /**
   * The sidebar, in groups (D40); each item scrolls to its section. Only what is built is
   * listed: no empty "coming later" sections. Group and item labels are catalogue keys.
   */
  const NAV = [
    {
      group: 'settings.nav.group.you',
      items: [
        { id: 'appearance', label: 'settings.nav.appearance', icon: 'sun' },
        { id: 'accessibility', label: 'settings.nav.accessibility', icon: 'access' },
        { id: 'units', label: 'settings.nav.units', icon: 'ruler' },
        { id: 'desktop', label: 'settings.nav.desktop', icon: 'taskbar' },
      ],
    },
    {
      group: 'settings.nav.group.machine',
      items: [
        { id: 'machine', label: 'settings.nav.machine', icon: 'nut' },
        { id: 'time', label: 'settings.nav.time', icon: 'clock' },
        { id: 'storage', label: 'settings.nav.storage', icon: 'archive' },
      ],
    },
    {
      group: 'settings.nav.group.advanced',
      items: [
        { id: 'connectors', label: 'settings.nav.connectors', icon: 'package' },
        { id: 'about', label: 'settings.nav.about', icon: 'info' },
        { id: 'developer', label: 'settings.nav.developer', icon: 'warning' },
      ],
    },
  ] as const;
  type SectionId = (typeof NAV)[number]['items'][number]['id'];

  let current = $state<SectionId>('appearance');
  let scroller: HTMLElement | undefined = $state();
  // The desktop is built from the window registry, so its kinds are the full definitions.
  const programs = (Object.values(desktop.kinds) as WindowDef[]).filter((d) => d.program);

  let error = $state<ErrorBody | null>(null);
  let saving = $state(false);
  let warning = $state(false);
  let understood = $state(false);
  let typed = $state('');

  const settings = $derived(core.settings);
  const wallpaper = $derived(resolveWallpaper(settings?.appearance.wallpaper));
  const style = $derived(resolveStyle(settings?.appearance.style).id);
  const pinned = $derived(settings?.taskbar.pinned ?? []);
  const fullMachine = $derived(settings?.developer.fullMachineFiles ?? false);
  const confirmed = $derived(
    understood && typed.trim().toLowerCase() === confirmWord.toLowerCase(),
  );

  async function change(patch: (s: Settings) => Settings) {
    if (!settings) return;
    saving = true;
    error = null;
    const r = await core.saveSettings(patch(settings));
    saving = false;
    if (!r.ok) error = r.error;
  }

  function setTheme(theme: Theme) {
    void change((s) => ({ ...s, appearance: { ...s.appearance, theme } }));
  }

  function setStyle(id: string) {
    void change((s) => ({ ...s, appearance: { ...s.appearance, style: id } }));
  }

  function setTaskbarSize(size: 'small' | 'large') {
    void change((s) => ({ ...s, taskbar: { ...s.taskbar, size } }));
  }

  function setPin(key: string, on: boolean) {
    void change((s) => ({
      ...s,
      taskbar: {
        ...s.taskbar,
        pinned: on
          ? [...s.taskbar.pinned.filter((k) => k !== key), key]
          : s.taskbar.pinned.filter((k) => k !== key),
      },
    }));
  }

  function go(id: SectionId) {
    current = id;
    const el = scroller?.querySelector(`#${win.id}-${id}`);
    // Not every browser engine has it (the test DOM doesn't); the sidebar still marks the section.
    const reduce = document.documentElement.dataset.motion === 'reduce';
    el?.scrollIntoView?.({ block: 'start', behavior: reduce ? 'auto' : 'smooth' });
  }

  // Opened at one section (the taskbar's "Taskbar settings…"): go there once the page is drawn.
  $effect(() => {
    const want = win.params.section as SectionId | undefined;
    if (!want || !scroller || !settings) return;
    go(want);
    desktop.update(win.id, { params: {} });
  });

  /** Follow the scrolling: the section nearest the top is the one marked in the sidebar. */
  function onScroll() {
    if (!scroller) return;
    const top = scroller.getBoundingClientRect().top;
    let best: SectionId = 'appearance';
    for (const g of NAV) {
      for (const it of g.items) {
        const el = scroller.querySelector(`#${win.id}-${it.id}`);
        if (el && el.getBoundingClientRect().top - top < 80) best = it.id;
      }
    }
    current = best;
  }

  function setWallpaper(id: string) {
    void change((s) => ({ ...s, appearance: { ...s.appearance, wallpaper: id } }));
  }

  async function setMachine(on: boolean) {
    saving = true;
    error = null;
    const r = await core.setDeveloper({ fullMachineFiles: on, acknowledgedRisk: on });
    saving = false;
    if (!r.ok) error = r.error;
    closeWarning();
  }

  function toggleMachine(e: Event) {
    const box = e.currentTarget as HTMLInputElement;
    if (box.checked) {
      // Not on yet: only the warning dialog can turn it on.
      box.checked = false;
      warning = true;
    } else void setMachine(false);
  }

  function closeWarning() {
    warning = false;
    understood = false;
    typed = '';
  }
</script>

<div class="settings">
  <nav class="side" aria-label={t('settings.nav.label')}>
    {#each NAV as g (g.group)}
      <p class="group">{t(g.group)}</p>
      {#each g.items as it (it.id)}
        <button
          class="nav-item"
          class:on={current === it.id}
          aria-current={current === it.id ? 'true' : undefined}
          onclick={() => go(it.id)}
        >
          <Icon name={it.icon} size={17} /><span>{t(it.label)}</span>
        </button>
      {/each}
    {/each}
  </nav>

  {#if !settings}
    <p class="loading">{t('settings.loading')}</p>
  {:else}
    <div class="scroll" bind:this={scroller} onscroll={onScroll}>
      {#if error}<ErrorNotice {error} />{/if}

      <section id="{win.id}-appearance" aria-labelledby="{win.id}-appearance-h">
        <h3 id="{win.id}-appearance-h">{t('settings.appearance.title')}</h3>
        <div class="row">
          <div class="text">
            <p class="name" id="{win.id}-theme-label">{t('settings.appearance.mode.name')}</p>
            <p class="desc">{t('settings.appearance.mode.desc')}</p>
          </div>
          <div class="segmented" role="radiogroup" aria-labelledby="{win.id}-theme-label">
            {#each [['system', t('settings.appearance.mode.system')], ['light', t('settings.appearance.mode.light')], ['dark', t('settings.appearance.mode.dark')]] as const as [value, label] (value)}
              <label
                ><input
                  type="radio"
                  name="{win.id}-theme"
                  {value}
                  checked={settings.appearance.theme === value}
                  disabled={saving}
                  onchange={() => setTheme(value)}
                />{label}</label
              >
            {/each}
          </div>
        </div>

        <fieldset class="choices">
          <legend class="name">{t('settings.appearance.style.name')}</legend>
          <p class="desc legend-desc">{t('settings.appearance.style.desc')}</p>
          <div class="style-grid">
            {#each STYLES as st (st.id)}
              {@const sw = styleSwatch(st)}
              <label class="style-choice" title={st.description}>
                <input
                  type="radio"
                  name="{win.id}-style"
                  value={st.id}
                  checked={style === st.id}
                  disabled={saving}
                  onchange={() => setStyle(st.id)}
                />
                <span class="style-swatch" aria-hidden="true">
                  <span class="half" style:background={sw.light}
                    ><span class="dot" style:background={sw.accentLight}></span><span
                      class="aa light">Aa</span
                    ></span
                  >
                  <span class="half" style:background={sw.dark}
                    ><span class="dot" style:background={sw.accentDark}></span><span class="aa dark"
                      >Aa</span
                    ></span
                  >
                </span>
                <span class="style-name">{st.label}</span>
              </label>
            {/each}
          </div>
        </fieldset>

        <fieldset class="choices">
          <legend class="name">{t('settings.appearance.wallpaper.name')}</legend>
          <div class="wall-grid">
            {#each WALLPAPERS as w (w.id)}
              <label class="wall-choice">
                <input
                  type="radio"
                  name="{win.id}-wallpaper"
                  value={w.id}
                  checked={wallpaper === w.id}
                  disabled={saving}
                  onchange={() => setWallpaper(w.id)}
                />
                <span class="swatch wallpaper" data-wallpaper={w.id} aria-hidden="true"></span>
                <span class="wall-name">{w.label}</span>
                <span class="desc">{w.description}</span>
              </label>
            {/each}
          </div>
        </fieldset>
        <LanguageRow winId={win.id} {settings} {saving} change={(p) => void change(p)} />
      </section>

      <AccessibilitySection winId={win.id} {settings} {saving} change={(p) => void change(p)} />
      <UnitsSection winId={win.id} {settings} {saving} change={(p) => void change(p)} />

      <section id="{win.id}-desktop" aria-labelledby="{win.id}-desktop-h">
        <h3 id="{win.id}-desktop-h">{t('settings.desktop.title')}</h3>
        <div class="row">
          <div class="text">
            <p class="name" id="{win.id}-size-label">{t('settings.desktop.size.name')}</p>
            <p class="desc">{t('settings.desktop.size.desc')}</p>
          </div>
          <div class="segmented" role="radiogroup" aria-labelledby="{win.id}-size-label">
            {#each [['small', t('settings.desktop.size.small')], ['large', t('settings.desktop.size.large')]] as const as [value, label] (value)}
              <label
                ><input
                  type="radio"
                  name="{win.id}-size"
                  {value}
                  checked={settings.taskbar.size === value}
                  disabled={saving}
                  onchange={() => setTaskbarSize(value)}
                />{label}</label
              >
            {/each}
          </div>
        </div>
        <fieldset class="choices">
          <legend class="name">{t('settings.desktop.pinned.name')}</legend>
          <p class="desc legend-desc">{t('settings.desktop.pinned.desc')}</p>
          <ul class="pins">
            {#each programs as d (d.kind)}
              <li>
                <label class="pin-row">
                  <AppIcon icon={d.icon} colour={d.colour} size={30} />
                  <span class="pin-name">{d.title}</span>
                  <input
                    class="switch"
                    type="checkbox"
                    role="switch"
                    aria-label={t('settings.desktop.pinned.pin', { program: d.title })}
                    checked={isPinned(pinned, d.program!)}
                    disabled={saving}
                    onchange={(e) => setPin(d.program!, e.currentTarget.checked)}
                  />
                </label>
              </li>
            {/each}
          </ul>
        </fieldset>
      </section>

      <MachineSection winId={win.id} {settings} {saving} change={(p) => void change(p)} />
      <TimeSection winId={win.id} {settings} {saving} change={(p) => void change(p)} />
      <StorageSection winId={win.id} />
      <ConnectorsSection winId={win.id} />
      <AboutSection winId={win.id} />

      <section id="{win.id}-developer" aria-labelledby="{win.id}-developer-h">
        <h3 id="{win.id}-developer-h">{t('settings.developer.title')}</h3>
        <p class="intro">{t('settings.developer.intro')}</p>
        <div class="row danger-card">
          <div class="text">
            <label class="name" for="{win.id}-full-machine"
              >{t('settings.developer.machine.name')}</label
            >
            <p class="desc" id="{win.id}-full-machine-desc">
              {fullMachine
                ? t('settings.developer.machine.on')
                : t('settings.developer.machine.off')}
            </p>
          </div>
          <input
            id="{win.id}-full-machine"
            class="switch"
            type="checkbox"
            role="switch"
            aria-describedby="{win.id}-full-machine-desc"
            checked={fullMachine}
            disabled={saving}
            onchange={toggleMachine}
          />
        </div>
      </section>
    </div>
  {/if}

  {#if warning}
    <Dialog
      title={t('settings.developer.warning.title')}
      tone="danger"
      oncancel={closeWarning}
      wide
    >
      <p>{t('settings.developer.warning.intro')}</p>
      <ul class="risks">
        <li>{t('settings.developer.warning.system')}</li>
        <li><RichText text={t('settings.developer.warning.noTrash')} /></li>
        <li>{t('settings.developer.warning.others')}</li>
      </ul>
      <label class="ack">
        <input type="checkbox" class="check" bind:checked={understood} />
        {t('settings.developer.warning.understood')}
      </label>
      <label class="field">
        <span class="label"
          ><RichText text={t('settings.developer.warning.type', { word: confirmWord })} /></span
        >
        <input class="input" bind:value={typed} autocomplete="off" spellcheck="false" />
      </label>
      {#snippet actions()}
        <button class="btn" onclick={closeWarning}>{t('settings.developer.warning.cancel')}</button>
        <button
          class="btn danger solid"
          disabled={!confirmed || saving}
          onclick={() => setMachine(true)}>{t('settings.developer.warning.confirm')}</button
        >
      {/snippet}
    </Dialog>
  {/if}
</div>

<style>
  .settings {
    position: relative;
    display: flex;
    flex: 1;
    min-height: 0;
    background: var(--surface);
    container-type: inline-size;
  }
  .side {
    flex: none;
    /* In rem, so it grows with Settings → Accessibility → Text size. */
    width: 12.5rem;
    padding: var(--space-4) var(--space-2);
    border-right: 1px solid var(--line);
    background: var(--surface-2);
    overflow-y: auto;
    display: grid;
    align-content: start;
    gap: 2px;
  }
  .group {
    padding: var(--space-3) var(--space-3) var(--space-1);
    font-size: var(--text-xs);
    font-weight: 700;
    color: var(--text-muted);
  }
  .group:first-child {
    padding-top: 0;
  }
  .nav-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-height: var(--touch);
    padding: 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-md);
    background: none;
    color: var(--text-muted);
    font-size: var(--text-sm);
    text-align: left;
  }
  .nav-item:hover {
    background: var(--hover);
    color: var(--text);
  }
  .nav-item.on {
    background: var(--selected);
    color: var(--text);
    font-weight: 700;
  }
  .nav-item.on :global(svg) {
    color: var(--accent-ink);
  }
  @container (max-width: 560px) {
    .settings {
      flex-direction: column;
    }
    .side {
      width: auto;
      display: flex;
      padding: var(--space-1) var(--space-2);
      border-right: 0;
      border-bottom: 1px solid var(--line);
      overflow-x: auto;
    }
    .group {
      display: none;
    }
    .nav-item {
      width: auto;
      white-space: nowrap;
    }
  }
  .loading {
    padding: var(--space-5);
    color: var(--text-muted);
  }
  .scroll {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    display: grid;
    align-content: start;
    gap: var(--space-6);
    padding: var(--space-5);
  }
  /* Shared with the section components in ./settings/, hence :global inside the scroller. */
  .scroll :global(section) {
    display: grid;
    gap: var(--space-4);
    scroll-margin-top: var(--space-4);
    min-width: 0;
  }
  .scroll :global(section > h3) {
    font-size: var(--text-xl);
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .scroll :global(.intro) {
    margin-top: calc(var(--space-2) * -1);
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .scroll :global(.row) {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3) var(--space-4);
    padding-bottom: var(--space-4);
    border-bottom: 1px solid var(--line);
  }
  .scroll :global(.row > .text) {
    flex: 1 1 16rem;
    min-width: 0;
  }
  .scroll :global(.name) {
    font-weight: 650;
  }
  .scroll :global(.desc) {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .choices {
    margin: 0;
    padding: 0 0 var(--space-4);
    border: 0;
    border-bottom: 1px solid var(--line);
    min-width: 0;
  }
  .choices legend {
    padding: 0;
    margin-bottom: var(--space-1);
  }
  .legend-desc {
    margin-bottom: var(--space-3);
  }
  .style-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: var(--space-3);
  }
  .style-choice {
    position: relative;
    display: grid;
    gap: var(--space-2);
    padding: 6px;
    border-radius: var(--radius-lg);
    box-shadow: inset 0 0 0 1px var(--line);
    cursor: pointer;
  }
  .style-choice:hover {
    background: var(--hover);
  }
  .style-choice input,
  .wall-choice input {
    position: absolute;
    opacity: 0;
  }
  .style-choice:has(input:checked) {
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .style-choice:has(input:focus-visible),
  .wall-choice:has(input:focus-visible) {
    outline: 2px solid var(--focus-ring);
  }
  .style-swatch {
    display: flex;
    height: 60px;
    border-radius: var(--radius-md);
    overflow: hidden;
    box-shadow: inset 0 0 0 1px oklch(0.3 0.01 250 / 0.12);
  }
  .half {
    flex: 1;
    display: flex;
    align-items: flex-end;
    gap: 4px;
    padding: 6px;
  }
  .dot {
    width: 11px;
    height: 11px;
    border-radius: 50%;
  }
  .aa {
    font-size: var(--text-xs);
    font-weight: 650;
    line-height: 1;
  }
  .aa.light {
    color: oklch(0.235 0.008 250);
  }
  .aa.dark {
    color: oklch(0.94 0.003 250);
  }
  .style-name {
    padding: 0 4px 2px;
    font-size: var(--text-sm);
    font-weight: 650;
  }
  .wall-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(124px, 1fr));
    gap: var(--space-3);
    margin-top: var(--space-2);
  }
  .wall-choice {
    position: relative;
    display: grid;
    gap: 2px;
    padding: var(--space-2);
    border-radius: var(--radius-md);
    cursor: pointer;
  }
  .wall-choice:hover {
    background: var(--hover);
  }
  .swatch {
    display: block;
    height: 64px;
    margin-bottom: var(--space-1);
    border-radius: var(--radius-sm);
    box-shadow: inset 0 0 0 1px oklch(0.3 0.01 250 / 0.15);
  }
  .wall-choice:has(input:checked) .swatch {
    box-shadow:
      0 0 0 2px var(--surface),
      0 0 0 4px var(--accent);
  }
  .wall-name {
    font-size: var(--text-sm);
    font-weight: 650;
  }
  .pins {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 2px;
  }
  .pin-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 48px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-md);
  }
  .pin-row:hover {
    background: var(--hover);
  }
  .pin-name {
    flex: 1;
    font-weight: 600;
  }
  .danger-card {
    padding: var(--space-4);
    border: 1px solid oklch(0.62 0.19 27 / 0.35);
    border-radius: var(--radius-lg);
    background: var(--danger-soft);
  }
  .danger-card .name {
    color: var(--danger);
  }
  .risks {
    margin: 0;
    padding-left: 1.2em;
    display: grid;
    gap: var(--space-2);
  }
  .ack {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    font-weight: 600;
  }
  .ack .check {
    margin-top: 2px;
  }
</style>
