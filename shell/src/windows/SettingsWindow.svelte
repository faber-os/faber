<script lang="ts">
  import { useCore } from '../lib/state/context';
  import type { ErrorBody, Settings, Theme } from '../lib/state/types';
  import { WALLPAPERS, resolveWallpaper } from '../lib/desktop/wallpapers';
  import Dialog from '../components/Dialog.svelte';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import type { WindowProps } from '../components/windowDefs';

  let props: WindowProps = $props();

  const core = useCore();
  const CONFIRM_WORD = 'machine';

  let error = $state<ErrorBody | null>(null);
  let saving = $state(false);
  let warning = $state(false);
  let understood = $state(false);
  let typed = $state('');

  const settings = $derived(core.settings);
  const wallpaper = $derived(resolveWallpaper(settings?.appearance.wallpaper));
  const fullMachine = $derived(settings?.developer.fullMachineFiles ?? false);
  const confirmed = $derived(understood && typed.trim().toLowerCase() === CONFIRM_WORD);

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
  {#if !settings}
    <p class="loading">Loading settings…</p>
  {:else}
    <div class="scroll">
      {#if error}<ErrorNotice {error} />{/if}

      <section aria-labelledby="appearance">
        <h3 id="appearance">Appearance</h3>
        <div class="row">
          <div class="text">
            <p class="name" id="theme-label">Theme</p>
            <p class="desc">Applies to every screen showing this machine.</p>
          </div>
          <div class="segmented" role="radiogroup" aria-labelledby="theme-label">
            {#each [['system', 'System'], ['light', 'Light'], ['dark', 'Dark']] as const as [value, label] (value)}
              <label
                ><input
                  type="radio"
                  name="theme"
                  {value}
                  checked={settings.appearance.theme === value}
                  disabled={saving}
                  onchange={() => setTheme(value)}
                />{label}</label
              >
            {/each}
          </div>
        </div>

        <fieldset class="walls">
          <legend class="name">Wallpaper</legend>
          <div class="wall-grid">
            {#each WALLPAPERS as w (w.id)}
              <label class="wall-choice">
                <input
                  type="radio"
                  name="wallpaper"
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
      </section>

      <section aria-labelledby="developer">
        <h3 id="developer">Developer</h3>
        <p class="intro">
          For people building Faber or connectors. Leave these off unless you know why you need
          them.
        </p>
        <div class="row">
          <div class="text">
            <label class="name" for="full-machine">Allow Files to see the whole machine</label>
            <p class="desc" id="full-machine-desc">
              {#if fullMachine}
                On. Files shows “This computer” with every drive and system folder.
              {:else}
                Off. Files only shows Faber's home folder.
              {/if}
            </p>
          </div>
          <input
            id="full-machine"
            class="switch"
            type="checkbox"
            role="switch"
            aria-describedby="full-machine-desc"
            checked={fullMachine}
            disabled={saving}
            onchange={toggleMachine}
          />
        </div>
      </section>
    </div>
  {/if}

  {#if warning}
    <Dialog title="Let Files see the whole machine?" tone="danger" oncancel={closeWarning} wide>
      <p>
        Files will show every folder on this computer, not just Faber's home folder. Read this
        first:
      </p>
      <ul class="risks">
        <li>
          Deleting or changing system files can stop this computer from starting, and Faber with it.
        </li>
        <li>
          There is <strong>no Trash outside the home folder</strong>: anything you delete there is
          gone for good, at once.
        </li>
        <li>Anyone using this shell can then read and change those files too.</li>
      </ul>
      <label class="ack">
        <input type="checkbox" class="check" bind:checked={understood} />
        I understand that I can break this computer and lose files for good.
      </label>
      <label class="field">
        <span class="label">Type <code>{CONFIRM_WORD}</code> to confirm</span>
        <input class="input" bind:value={typed} autocomplete="off" spellcheck="false" />
      </label>
      {#snippet actions()}
        <button class="btn" onclick={closeWarning}>Cancel</button>
        <button
          class="btn danger solid"
          disabled={!confirmed || saving}
          onclick={() => setMachine(true)}>Allow whole-machine access</button
        >
      {/snippet}
    </Dialog>
  {/if}
</div>

<style>
  .settings {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    background: var(--surface);
  }
  .loading {
    padding: var(--space-5);
    color: var(--text-muted);
  }
  .scroll {
    flex: 1;
    overflow-y: auto;
    display: grid;
    align-content: start;
    gap: var(--space-6);
    padding: var(--space-5);
  }
  section {
    display: grid;
    gap: var(--space-4);
  }
  h3 {
    font-size: var(--text-xs);
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-muted);
    padding-bottom: var(--space-2);
    border-bottom: 1px solid var(--line);
  }
  .intro {
    margin-top: calc(var(--space-2) * -1);
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }
  .text {
    min-width: 0;
  }
  .name {
    font-weight: 650;
  }
  .desc {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .walls {
    margin: 0;
    padding: 0;
    border: 0;
    min-width: 0;
  }
  .walls legend {
    padding: 0;
    margin-bottom: var(--space-2);
  }
  .wall-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(124px, 1fr));
    gap: var(--space-3);
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
  .wall-choice input {
    position: absolute;
    opacity: 0;
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
  .wall-choice:has(input:focus-visible) {
    outline: 2px solid var(--focus-ring);
  }
  .wall-name {
    font-size: var(--text-sm);
    font-weight: 650;
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
