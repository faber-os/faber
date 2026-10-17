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
  import { SketchRunner } from './circuit/runner.svelte';
  import { emulatedBoard } from '../lib/circuit/avr';
  import { SKETCHBOOK } from '../lib/desktop/arduino';
  import { openInArduino } from '../components/arduinoWindow';

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
  let picker = $state<'open' | 'save' | 'sketch' | null>(null);
  const runner = new SketchRunner(lab);
  /** The board a sketch is being picked for, and the sketch that runs. */
  let runOn = $state<string | null>(null);
  let sketchPath = $state<string | null>(null);
  let compiling = $state(false);
  let showSerial = $state(true);
  let toSend = $state('');
  $effect(() => () => runner.stop());
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

  // What the real parts are doing: a part with the same name on the drawing shows it.
  $effect(() => {
    lab.live = new Map((core.parts ?? []).map((p) => [p.name, p.state.kind]));
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

  const COMPILE_BOARD: Record<string, string> = { uno: 'uno', nano: 'nano', mega: 'mega2560' };

  function codeMenu(): MenuItem[] {
    const boards = lab.doc.parts.filter((p) => emulatedBoard(lab.defs(p.def)?.id ?? ''));
    if (!boards.length)
      return [
        { label: t('circuit.code.noBoard'), hint: t('circuit.code.noBoardHint'), disabled: true },
      ];
    const items: MenuItem[] = boards.map((b) => ({
      label: t('circuit.code.run', { board: b.name }),
      hint: t('circuit.code.runHint'),
      action: () => {
        runOn = b.id;
        picker = 'sketch';
      },
    }));
    if (runner.running) {
      items.push({ separator: true, label: '' });
      items.push({ label: t('circuit.code.stop'), action: () => runner.stop() });
      if (sketchPath)
        items.push({
          label: t('circuit.code.edit'),
          action: () => openInArduino(desktop, sketchPath!),
        });
    }
    return items;
  }

  async function runSketch(path: string) {
    const part = lab.part(runOn ?? '');
    const board = part ? emulatedBoard(lab.defs(part.def)?.id ?? '') : null;
    if (!part || !board) return;
    error = null;
    compiling = true;
    const r = await core.compileSketch(path, COMPILE_BOARD[board]!, true);
    compiling = false;
    if (!r.ok) {
      error = r.error;
      return;
    }
    if (!r.value.ok || !r.value.hex) {
      const first = r.value.diagnostics.find((d) => d.severity === 'error');
      error = {
        code: 'compile-failed',
        message: t('circuit.code.failed', { sketch: r.value.sketchName }),
        hint: first
          ? `${first.file ?? ''}${first.line ? `:${first.line}` : ''} ${first.message}`.trim()
          : (r.value.message ?? ''),
      };
      return;
    }
    sketchPath = path;
    runner.start(part.id, board, r.value.hex, r.value.sketchName);
    lab.say(t('circuit.code.started', { sketch: r.value.sketchName, board: part.name }));
  }
</script>

<div class="circuit-window">
  {#if error}<div class="err"><ErrorNotice {error} /></div>{/if}
  {#if runner.part}
    <div class="run" class:stopped={!runner.running}>
      <span class="dot" aria-hidden="true"></span>
      <b
        >{runner.running
          ? t('circuit.run.running', {
              sketch: runner.sketch,
              board: lab.part(runner.part)?.name ?? '',
            })
          : t('circuit.run.stopped', { sketch: runner.sketch })}</b
      >
      <span class="muted"
        >{t('circuit.run.time', {
          seconds: runner.seconds.toFixed(1),
          speed: Math.round(runner.speed * 100),
        })}</span
      >
      <span class="spacer"></span>
      <button class="btn quiet" onclick={() => (showSerial = !showSerial)}
        >{showSerial ? t('circuit.run.hideSerial') : t('circuit.run.showSerial')}</button
      >
      {#if runner.running}
        <button class="btn" onclick={() => runner.stop()}>{t('circuit.code.stop')}</button>
      {:else}
        <button class="btn" onclick={() => (runner.part = null)}>{t('circuit.run.close')}</button>
      {/if}
    </div>
    {#if showSerial}
      <div class="serial">
        <pre aria-label={t('circuit.run.serial')}>{runner.serial || t('circuit.run.noSerial')}</pre>
        <form
          onsubmit={(e) => {
            e.preventDefault();
            runner.write(`${toSend}\n`);
            toSend = '';
          }}
        >
          <input
            bind:value={toSend}
            placeholder={t('circuit.run.send')}
            aria-label={t('circuit.run.send')}
            disabled={!runner.running}
          />
        </form>
      </div>
    {/if}
  {/if}
  {#if compiling}<div class="run">
      <span class="muted">{t('circuit.code.compilingShort')}</span>
    </div>{/if}
  <CircuitEditor {menus} onsave={() => void save()} />
</div>

{#if picker === 'sketch'}
  <FilePicker
    mode="open"
    start={(sketchPath && parentPath(sketchPath)) || SKETCHBOOK}
    oncancel={() => (picker = null)}
    onpick={(p) => {
      picker = null;
      void runSketch(p);
    }}
  />
{:else if picker}
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
  .run {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--line);
    background: var(--surface);
    font-size: var(--text-sm);
  }
  .run .dot {
    width: 9px;
    height: 9px;
    border-radius: 99px;
    background: var(--ok);
  }
  .run.stopped .dot {
    background: var(--text-faint);
  }
  .muted {
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .spacer {
    flex: 1;
  }
  .serial {
    display: flex;
    flex-direction: column;
    border-bottom: 1px solid var(--line);
    background: var(--surface);
  }
  .serial pre {
    margin: 0;
    max-height: 120px;
    overflow: auto;
    padding: var(--space-2) var(--space-3);
    font-family: var(--font-data);
    font-size: var(--text-xs);
    white-space: pre-wrap;
  }
  .serial input {
    border: 0;
    border-top: 1px solid var(--line);
    padding: var(--space-1) var(--space-3);
    background: var(--surface-2);
    width: 100%;
    font-family: var(--font-data);
  }
</style>
