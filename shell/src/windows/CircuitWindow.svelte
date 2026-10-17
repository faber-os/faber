<script lang="ts" module>
  let untitled = 0;
</script>

<script lang="ts">
  import { t } from '../lib/i18n';
  import { baseName, parentPath } from '../lib/desktop/filePaths';
  import { useCore } from '../lib/state/context';
  import type { ErrorBody } from '../lib/state/types';
  import Dialog from '../components/Dialog.svelte';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import FilePicker from '../components/FilePicker.svelte';
  import { useDesktop } from '../components/desktopController.svelte';
  import type { WindowProps } from '../components/windowDefs';
  import { lookup } from '../lib/circuit/catalog';
  import { diagramFromParts } from '../lib/circuit/fromParts';
  import { arrange } from '../lib/circuit/layout';
  import {
    blankDiagram,
    CircuitStore,
    example,
    parseCircuit,
    provideCircuit,
  } from './circuit/circuit.svelte';
  import CircuitEditor from './circuit/CircuitEditor.svelte';
  import type { MenuItem } from './circuit/menu';
  import { faberParts } from './circuit/faberParts';

  /**
   * A Circuits window (docs/circuits.md): one drawing, kept as a `.fbcircuit` file in Files.
   * Opened from Files with a path, or empty from the start menu.
   */
  let { win }: WindowProps = $props();

  const core = useCore();
  const desktop = useDesktop();
  // svelte-ignore state_referenced_locally (the window's first file; later ones come through `open`)
  const first = win.params.path ?? null;
  let path = $state<string | null>(first);
  const n = first ? 0 : ++untitled;
  const lab = new CircuitStore();
  lab.load({
    faberCircuit: 1,
    doc: blankDiagram(n > 1 ? t('circuit.window.untitledN', { n }) : t('circuit.window.untitled')),
    custom: [],
  });
  provideCircuit(lab);

  let error = $state.raw<ErrorBody | null>(null);
  let picker = $state<'open' | 'save' | null>(null);
  let closing = $state(false);
  let afterSave: (() => void) | null = null;

  const name = $derived(path ? baseName(path).replace(/\.fbcircuit$/i, '') : lab.doc.name);

  async function load(p: string) {
    error = null;
    const r = await core.readFile(p);
    if (!r.ok) {
      error = r.error;
      return;
    }
    const file = parseCircuit(new TextDecoder().decode(r.value));
    if (!file) {
      error = {
        code: 'not-a-circuit',
        message: t('circuit.window.notACircuit', { name: baseName(p) }),
        hint: t('circuit.window.notACircuitHint'),
      };
      return;
    }
    lab.load(file);
    path = p;
    desktop.noteRecent(p);
    requestAnimationFrame(() => lab.fit());
  }
  if (first) void load(first);

  // Another file opened into this window (Files → Open with, while it is open).
  $effect(() => {
    const want = win.params.open;
    if (!want) return;
    desktop.update(win.id, { params: {} });
    void load(want);
  });

  $effect(() => {
    desktop.update(win.id, {
      title: t('circuit.window.title', { name: lab.dirty ? `${name} •` : name }),
      key: path ?? `circuit:new:${n}`,
    });
  });

  $effect(() =>
    desktop.setCloseGuard(win.id, () => {
      if (!lab.dirty || !lab.doc.parts.length) return true;
      closing = true;
      return false;
    }),
  );

  async function save(then?: () => void) {
    if (!path) {
      afterSave = then ?? null;
      picker = 'save';
      return;
    }
    await writeTo(path, then);
  }

  async function writeTo(p: string, then?: () => void) {
    const file = lab.toFile();
    file.doc.name = baseName(p).replace(/\.fbcircuit$/i, '');
    const r = await core.writeFile(p, new TextEncoder().encode(JSON.stringify(file, null, 1)));
    if (!r.ok) {
      error = r.error;
      lab.saved = '';
      return;
    }
    path = p;
    lab.doc.name = file.doc.name;
    lab.saved = JSON.stringify($state.snapshot(lab.doc));
    desktop.noteRecent(p);
    lab.say(t('circuit.window.saved', { name: baseName(p) }));
    then?.();
  }

  function replaceWith(doc: ReturnType<typeof example>) {
    lab.replace(doc);
    requestAnimationFrame(() => lab.fit());
  }

  async function fromFaber() {
    const parts = await faberParts(core);
    if (!parts.ok) {
      error = parts.error;
      return;
    }
    if (!parts.value.parts.length) {
      lab.say(t('circuit.window.noFaberParts'));
      return;
    }
    const doc = diagramFromParts(parts.value.parts, parts.value.host, name);
    arrange(doc, lookup(lab.custom));
    replaceWith(doc);
    lab.say(
      doc.skipped?.length
        ? t('circuit.window.fromFaberSkipped', {
            count: doc.parts.length,
            skipped: doc.skipped.join(', '),
          })
        : t('circuit.window.fromFaber', { count: doc.parts.length }),
    );
  }

  const menus = {
    file: (): MenuItem[] => [
      {
        label: t('circuit.file.new'),
        action: () => desktop.open('circuit', { key: `circuit:new:${Date.now()}` }),
      },
      { label: t('circuit.file.open'), action: () => (picker = 'open') },
      { separator: true, label: '' },
      { label: t('circuit.file.save'), shortcut: 'Ctrl+S', action: () => void save() },
      { label: t('circuit.file.saveAs'), action: () => (picker = 'save') },
    ],
    add: (): MenuItem[] => [
      {
        label: t('circuit.add.faber'),
        hint: t('circuit.add.faberHint'),
        action: () => void fromFaber(),
      },
      { separator: true, label: '' },
      {
        label: t('circuit.add.sennep'),
        hint: t('circuit.add.sennepHint'),
        action: () => replaceWith(example('machine')),
      },
      {
        label: t('circuit.add.corner'),
        hint: t('circuit.add.cornerHint'),
        action: () => replaceWith(example('corner')),
      },
      { separator: true, label: '' },
      { label: t('circuit.add.newPart'), action: () => (lab.newPartOpen = true) },
    ],
    code: (): MenuItem[] => codeMenu(),
  };

  function codeMenu(): MenuItem[] {
    return [{ label: t('circuit.code.soon'), disabled: true }];
  }
</script>

<div class="circuit-window">
  {#if error}<div class="err"><ErrorNotice {error} /></div>{/if}
  <CircuitEditor {menus} onsave={() => void save()} />
</div>

{#if picker}
  <FilePicker
    mode={picker}
    start={(path && parentPath(path)) || 'home:/Documents'}
    suggestedName={`${name}.fbcircuit`}
    oncancel={() => {
      picker = null;
      afterSave = null;
    }}
    onpick={(p) => {
      const mode = picker;
      picker = null;
      if (mode === 'open') {
        if (!lab.doc.parts.length || !lab.dirty) void load(p);
        else desktop.open('circuit', { key: p, params: { path: p } });
      } else {
        const then = afterSave ?? undefined;
        afterSave = null;
        void writeTo(p.toLowerCase().endsWith('.fbcircuit') ? p : `${p}.fbcircuit`, then);
      }
    }}
  />
{/if}

{#if closing}
  <Dialog title={t('circuit.close.title', { name })} oncancel={() => (closing = false)}>
    <p>{t('circuit.close.body')}</p>
    {#snippet actions()}
      <button class="btn" onclick={() => (closing = false)}>{t('circuit.close.cancel')}</button>
      <button class="btn danger" onclick={() => desktop.close(win.id)}
        >{t('circuit.close.discard')}</button
      >
      <button
        class="btn primary"
        onclick={() => ((closing = false), void save(() => desktop.close(win.id)))}
        >{t('circuit.close.save')}</button
      >
    {/snippet}
  </Dialog>
{/if}

<style>
  .circuit-window {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    background: var(--surface-2);
  }
  .err {
    padding: var(--space-2) var(--space-3) 0;
  }
</style>
