<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { useCore } from '../lib/state/context';
  import { portAvailability } from '../lib/state/portHints';
  import {
    boardGuessText,
    flashablePorts,
    guessedBoard,
    overallPercent,
    stageText,
  } from '../lib/state/flasher';
  import { codeView, identityCaveat, toolsStepText } from '../lib/state/arduino';
  import type { ToolsProgress } from '../lib/state/core.svelte';
  import type {
    GuidedBoard,
    ArduinoInfo,
    BoardCodeInfo,
    CompileResult,
    ErrorBody,
    FlashStage,
    SketchDiagnostic,
    UploadResult,
  } from '../lib/state/types';
  import {
    SKETCHBOOK,
    diagnosticPlace,
    diagnosticsByTab,
    isSketchFile,
    sizeLines,
    sketchNameProblem,
    sketchOf,
    tabOrder,
    type SketchPaths,
  } from '../lib/desktop/arduino';
  import { baseName, childPath } from '../lib/desktop/filePaths';
  import { formatDateTime } from '../lib/desktop/wallClock';
  import { t } from '../lib/i18n';
  import { useDesktop } from '../components/desktopController.svelte';
  import { openInArduino } from '../components/arduinoWindow';
  import CodeEditor from '../components/CodeEditor.svelte';
  import Dialog from '../components/Dialog.svelte';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import FilePicker from '../components/FilePicker.svelte';
  import Icon from '../components/Icon.svelte';
  import RichText from '../components/RichText.svelte';
  import type { WindowProps } from '../components/windowDefs';

  /**
   * The Arduino editor (docs/arduino-editor.md): one sketch per window, its files as tabs.
   * Verify compiles, Upload compiles and flashes with Faber's flasher (after a confirm), and
   * "On the board" says which code runs on the board, opening it only once that is proved.
   */
  let { win, visible }: WindowProps = $props();

  const core = useCore();
  /** Saving, verifying and new sketches need an operator; uploading, checking a board (it
   *  resets it) and installing the tools need an admin (docs/auth.md). */
  const editLock = $derived(core.auth.lock('operator'));
  const flashLock = $derived(core.auth.lock('admin'));
  const desktop = useDesktop();
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();

  let info = $state.raw<ArduinoInfo | null>(null);
  let infoError = $state.raw<ErrorBody | null>(null);

  interface Tab {
    name: string;
    path: string;
    text: string;
    saved: string;
  }

  let sketch = $state.raw<SketchPaths | null>(null);
  let tabs = $state.raw<Tab[]>([]);
  let active = $state('');
  let loading = $state(false);
  /** Counts loads, so a reloaded sketch gets fresh editors even for tabs of the same name. */
  let generation = $state(0);
  const editors: Record<string, ReturnType<typeof CodeEditor> | undefined> = $state({});

  /** The sketch is a guided board's: Faber writes its `.ino` from the plan (docs/wiring.md §5). */
  let guided = $state.raw<GuidedBoard | null>(null);
  let takingOver = $state<{ error: ErrorBody | null; busy: boolean } | null>(null);
  let board = $state('');
  let boardTouched = $state(false);
  let port = $state('');

  // What the window is doing: one thing at a time.
  let phase = $state<
    | 'idle'
    | 'saving'
    | 'compiling'
    | 'confirm-upload'
    | 'uploading'
    | 'confirm-install'
    | 'installing'
  >('idle');
  let result = $state.raw<CompileResult | null>(null);
  let uploaded = $state.raw<UploadResult | null>(null);
  let error = $state.raw<ErrorBody | null>(null);
  let progress = $state<{ stage: FlashStage; done: number; total: number }>({
    stage: 'connecting',
    done: 0,
    total: 0,
  });
  let toolsStep = $state.raw<ToolsProgress | null>(null);

  // Dialogs.
  let picking = $state(false);
  let creating = $state<{ name: string; template: string; error: ErrorBody | null } | null>(null);
  let addingTab = $state<{ name: string } | null>(null);
  let closing = $state(false);
  let code = $state.raw<{
    port: string;
    info: BoardCodeInfo | null;
    error: ErrorBody | null;
    busy: boolean;
  } | null>(null);

  const busy = $derived(phase !== 'idle');
  const ports = $derived(flashablePorts(core.ports ?? []));
  const chosenPort = $derived(ports.find((p) => p.id === port));
  const boards = $derived(info?.boards ?? []);
  const chosenBoard = $derived(boards.find((b) => b.id === board));
  const ready = $derived(info?.tools.state === 'ready');
  const dirty = $derived(tabs.filter((tab) => tab.text !== tab.saved));
  const errors = $derived(result?.diagnostics.filter((d) => d.severity === 'error') ?? []);
  // The compiler's messages per tab, for the editors to underline. Made again only when the
  // result or the tabs' names change (a string, so typing doesn't), so an underline an edit took
  // away stays away until the next Verify.
  const tabNames = $derived(tabs.map((tab) => tab.name).join('/'));
  const tabDiagnostics = $derived(diagnosticsByTab(result?.diagnostics ?? [], tabNames.split('/')));
  const NO_DIAGNOSTICS: SketchDiagnostic[] = [];

  async function loadInfo() {
    const r = await core.arduino();
    if (r.ok) {
      info = r.value;
      infoError = null;
    } else infoError = r.error;
  }
  $effect(() => {
    void loadInfo();
  });

  // The board its USB ids name, unless the user picked one; else the first (the Uno).
  $effect(() => {
    if (boardTouched || boards.length === 0) return;
    // A guided board's sketch compiles only for the board its plan is for.
    const planned = guided?.plan?.board;
    if (planned && boards.some((b) => b.id === planned)) {
      const guess = guessedBoard(chosenPort, boards)?.id;
      const family = (id: string | undefined) => (id === 'nano-old' ? 'nano' : id);
      board = guess && family(guess) === family(planned) ? guess : planned;
      return;
    }
    board = guessedBoard(chosenPort, boards)?.id ?? (board || boards[0]!.id);
  });

  // A free port with an official Arduino on it, else any free one.
  $effect(() => {
    if (port !== '' || ports.length === 0 || boards.length === 0) return;
    const free = ports.filter((p) => portAvailability(p).free);
    port = (free.find((p) => guessedBoard(p, boards)) ?? free[0] ?? ports[0]!).id;
  });

  // Opened (or asked again) for a file: its sketch, at that tab.
  $effect(() => {
    const path = win.params.path;
    if (!path) return;
    untrack(() => {
      desktop.update(win.id, { params: {} });
      const s = sketchOf(path);
      if (s && sketch && s.folder === sketch.folder) active = baseName(path);
      else void openSketch(path);
    });
  });

  async function openSketch(path: string) {
    const s = sketchOf(path);
    error = null;
    if (!s) {
      error = {
        code: 'not-a-sketch',
        message: t('arduino.open.notSketch', { name: baseName(path) }),
        hint: '',
      };
      return;
    }
    loading = true;
    const list = await core.listFolder(s.folder);
    if (!list.ok) {
      loading = false;
      error = list.error;
      return;
    }
    const main = `${s.name}.ino`;
    const names = list.value.entries
      .filter((e) => e.kind === 'file' && isSketchFile(e.name))
      .map((e) => e.name);
    if (!names.includes(main)) {
      loading = false;
      error = {
        code: 'not-a-sketch',
        message: t('arduino.open.notSketch', { name: baseName(path) }),
        hint: '',
      };
      return;
    }
    const loaded: Tab[] = [];
    for (const name of tabOrder(names, main)) {
      const file = childPath(s.folder, name);
      const r = await core.readFile(file);
      if (!r.ok) {
        loading = false;
        error = r.error;
        return;
      }
      const text = decoder.decode(r.value);
      loaded.push({ name, path: file, text, saved: text });
    }
    // A folder with a plan is a guided board: is its sketch still Faber's to write?
    const plan = await core.plan(s.name);
    guided = plan.ok && plan.value.plan && !plan.value.plan.takenOver ? plan.value : null;
    generation += 1;
    tabs = loaded;
    sketch = s;
    active = names.includes(baseName(path)) ? baseName(path) : main;
    result = null;
    uploaded = null;
    loading = false;
    // Keyed by its sketch from now on, so opening the sketch again (from Files) comes here.
    desktop.update(win.id, {
      title: t('arduino.window.title', { name: s.name }),
      key: `arduino:${s.folder}`,
    });
  }

  function edit(name: string, text: string) {
    tabs = tabs.map((tab) => (tab.name === name ? { ...tab, text } : tab));
  }

  /** Saves every changed tab. False (with `error` set) if one couldn't be saved. */
  async function saveAll(): Promise<boolean> {
    for (const tab of dirty) {
      const text = tab.text;
      const r = await core.writeFile(tab.path, encoder.encode(text), 'replace');
      if (!r.ok) {
        error = r.error;
        return false;
      }
      tabs = tabs.map((x) => (x.name === tab.name ? { ...x, saved: text } : x));
    }
    return true;
  }

  async function save() {
    if (busy) return;
    phase = 'saving';
    error = null;
    await saveAll();
    phase = 'idle';
  }

  async function verify() {
    if (!sketch || !chosenBoard || busy) return;
    error = null;
    uploaded = null;
    phase = 'saving';
    if (!(await saveAll())) {
      phase = 'idle';
      return;
    }
    phase = 'compiling';
    result = null;
    const r = await core.compileSketch(sketch.main, chosenBoard.id);
    phase = 'idle';
    if (r.ok) result = r.value;
    else error = r.error;
  }

  async function upload(stopPart = false) {
    if (!sketch || !chosenBoard || !chosenPort) return;
    error = null;
    uploaded = null;
    result = null;
    phase = 'saving';
    if (!(await saveAll())) {
      phase = 'idle';
      return;
    }
    phase = 'uploading';
    progress = { stage: 'connecting', done: 0, total: 0 };
    const target = chosenPort.id;
    const stop = core.watchFlash((p) => {
      if (p.port === target) progress = { stage: p.stage, done: p.done, total: p.total };
    });
    const r = await core.uploadSketch(sketch.main, chosenBoard.id, target, stopPart);
    stop();
    phase = 'idle';
    if (r.ok) {
      result = r.value.compile;
      uploaded = r.value.flashed ? r.value : null;
    } else error = r.error;
  }

  async function install() {
    phase = 'installing';
    error = null;
    toolsStep = null;
    const stop = core.watchArduinoTools((p) => (toolsStep = p));
    const r = await core.installArduinoTools();
    stop();
    phase = 'idle';
    if (!r.ok) error = r.error;
    await loadInfo();
  }

  async function goto(d: SketchDiagnostic) {
    if (!d.inSketch || !d.file || !tabs.some((tab) => tab.name === d.file)) return;
    active = d.file;
    await tick();
    editors[d.file]?.goto(d.line ?? 1, d.column ?? 1);
  }

  async function createSketch() {
    if (!creating) return;
    const name = creating.name.trim();
    const problem = sketchNameProblem(name);
    if (problem) {
      creating = {
        ...creating,
        error: { code: 'invalid-sketch-name', message: problem, hint: '' },
      };
      return;
    }
    const r = await core.newSketch(name, creating.template);
    if (!r.ok) {
      creating = { ...creating, error: r.error };
      return;
    }
    creating = null;
    if (sketch) openInArduino(desktop, r.value.path);
    else await openSketch(r.value.path);
  }

  const NEW_TAB = /^[A-Za-z0-9_][A-Za-z0-9_.-]*\.(h|hpp|c|cpp|ino)$/;
  const newTabProblem = $derived.by(() => {
    if (!addingTab) return null;
    const name = addingTab.name.trim();
    if (!NEW_TAB.test(name)) return t('arduino.newTab.bad');
    if (tabs.some((tab) => tab.name === name)) return t('arduino.newTab.exists', { name });
    return null;
  });

  async function addTab() {
    if (!sketch || !addingTab || newTabProblem) return;
    const name = addingTab.name.trim();
    const path = childPath(sketch.folder, name);
    const r = await core.writeFile(path, new Uint8Array(), 'create');
    if (!r.ok) {
      error = r.error;
      return;
    }
    addingTab = null;
    tabs = [...tabs, { name, path, text: '', saved: '' }];
    active = name;
  }

  function pickedFile(path: string) {
    picking = false;
    if (sketch && sketchOf(path)?.folder !== sketch.folder) openInArduino(desktop, path);
    else void openSketch(path);
  }

  async function lookAtBoard() {
    if (!chosenPort) return;
    code = { port: chosenPort.id, info: null, error: null, busy: true };
    const r = await core.boardCode(chosenPort.id);
    code = { ...code, busy: false, ...(r.ok ? { info: r.value } : { error: r.error }) };
  }

  async function checkCode(stopPart = false) {
    if (!code) return;
    code = { ...code, busy: true, error: null };
    const r = await core.checkBoardCode(code.port, stopPart);
    code = { ...code, busy: false, ...(r.ok ? { info: r.value } : { error: r.error }) };
  }

  async function openRecord(id: string, copy: boolean) {
    if (!code) return;
    code = { ...code, busy: true, error: null };
    const r = await core.openFlashed(id, copy);
    if (!r.ok) {
      code = { ...code, busy: false, error: r.error };
      return;
    }
    code = null;
    if (sketch && sketchOf(r.value.path)?.folder === sketch.folder) await openSketch(r.value.path);
    else if (sketch) openInArduino(desktop, r.value.path);
    else await openSketch(r.value.path);
  }

  const view = $derived(
    code?.info
      ? codeView(code.info, (iso) => formatDateTime(Date.parse(iso), core.wallClock))
      : null,
  );

  // Closing with unsaved tabs asks first.
  $effect(() =>
    desktop.setCloseGuard(win.id, () => {
      if (dirty.length === 0) return true;
      closing = true;
      return false;
    }),
  );

  async function saveAndClose() {
    closing = false;
    if (await saveAll()) desktop.close(win.id);
  }

  function keys(e: KeyboardEvent) {
    if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
    const k = e.key.toLowerCase();
    if (k === 's') {
      e.preventDefault();
      void save();
    } else if (k === 'r') {
      e.preventDefault();
      void verify();
    } else if (k === 'u') {
      e.preventDefault();
      if (sketch && ready && chosenPort && !busy && !flashLock) phase = 'confirm-upload';
    }
  }

  /** "Edit the plan": the plan opens in Parts. */
  function editPlan() {
    if (!sketch) return;
    const id = desktop.open('parts', { params: { editPlan: sketch.name } });
    desktop.update(id, { params: { editPlan: sketch.name } });
    desktop.focus(id);
  }

  /** "Take over the code": one way; the sketch is yours from then on (docs/wiring.md §8). */
  async function takeOver() {
    if (!sketch || !takingOver) return;
    takingOver = { error: null, busy: true };
    const r = await core.takeOverPlan(sketch.name);
    if (!r.ok) {
      takingOver = { error: r.error, busy: false };
      return;
    }
    takingOver = null;
    await openSketch(childPath(sketch.folder, active || `${sketch.name}.ino`));
  }

  function startNew() {
    creating = { name: '', template: info?.templates[0]?.id ?? 'blank', error: null };
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions (Ctrl+S, Ctrl+R and Ctrl+U) -->
<div class="arduino" onkeydown={keys}>
  {#if infoError}
    <ErrorNotice error={infoError} />
  {/if}

  {#if info && info.tools.state !== 'ready'}
    <section class="tools" aria-live="polite">
      <Icon name="warning" />
      <div class="tools-text">
        {#if info.tools.state === 'missing'}
          <p class="title">{t('arduino.tools.missing.title')}</p>
          <p>{t('arduino.tools.missing.body')}</p>
        {:else if info.tools.state === 'no-avr-core'}
          <p class="title">{t('arduino.tools.noCore.title')}</p>
          <p>{t('arduino.tools.noCore.body')}</p>
        {:else}
          <p class="title">{t('arduino.tools.broken.title')}</p>
          <p>{info.tools.problem}</p>
        {/if}
        {#if phase === 'installing'}
          <div
            class="bar"
            role="progressbar"
            aria-label={t('arduino.tools.progress.label')}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={toolsStep?.step === 'downloading' && toolsStep.total
              ? Math.round(((toolsStep.done ?? 0) / toolsStep.total) * 100)
              : undefined}
          >
            <div
              class="fill"
              class:indeterminate={!(toolsStep?.step === 'downloading' && toolsStep.total)}
              style:width={toolsStep?.step === 'downloading' && toolsStep.total
                ? `${Math.round(((toolsStep.done ?? 0) / toolsStep.total) * 100)}%`
                : undefined}
            ></div>
          </div>
          <p class="help" role="status">
            {toolsStep ? toolsStepText(toolsStep) : t('arduino.tools.step.downloading')}
          </p>
        {:else if info.tools.install || info.tools.state === 'no-avr-core'}
          <div class="actions">
            <button
              class="btn primary"
              onclick={() => (phase = 'confirm-install')}
              disabled={busy || flashLock !== null}
              title={flashLock ?? undefined}
              ><Icon name="download" size={16} /> {t('arduino.tools.install')}</button
            >
          </div>
        {:else}
          <p class="help">{t('arduino.tools.unavailable')}</p>
        {/if}
      </div>
    </section>
  {/if}

  {#if !sketch}
    <section class="start">
      {#if loading}
        <p class="help" role="status">{t('arduino.loading')}</p>
      {:else}
        <p class="title">{t('arduino.start.title')}</p>
        <p class="help">{t('arduino.start.body')}</p>
        <div class="actions">
          <button class="btn primary" onclick={startNew}
            ><Icon name="plus" size={16} /> {t('arduino.new.title')}</button
          >
          <button class="btn" onclick={() => (picking = true)}
            ><Icon name="open" size={16} /> {t('arduino.open.sketch')}</button
          >
          <button class="btn" onclick={lookAtBoard} disabled={!chosenPort}
            ><Icon name="chip" size={16} /> {t('arduino.open.board')}</button
          >
        </div>
        {#if ports.length > 0}
          <label class="field inline">
            <span class="label">{t('arduino.port.label')}</span>
            <select class="select" bind:value={port}>
              {#each ports as p (p.id)}
                <option value={p.id}>{p.label}</option>
              {/each}
            </select>
          </label>
        {/if}
        {#if error}<ErrorNotice {error} />{/if}
      {/if}
    </section>
  {:else}
    <div class="toolbar" role="toolbar" aria-label={t('arduino.toolbar.label')}>
      <button
        class="btn"
        onclick={verify}
        disabled={!ready || busy || !chosenBoard || editLock !== null}
        title={editLock ?? t('arduino.verify.title')}
        ><Icon name="check" size={16} /> {t('arduino.verify')}</button
      >
      <button
        class="btn primary"
        onclick={() => (phase = 'confirm-upload')}
        disabled={!ready || busy || !chosenBoard || !chosenPort || flashLock !== null}
        title={flashLock ?? t('arduino.upload.title')}
        ><Icon name="upload" size={16} /> {t('arduino.upload')}</button
      >
      <label class="field">
        <span class="label">{t('arduino.board.label')}</span>
        <select
          class="select"
          bind:value={board}
          disabled={busy}
          onchange={() => (boardTouched = true)}
        >
          {#each boards as b (b.id)}
            <option value={b.id}>{t('arduino.board.option', { name: b.name })}</option>
          {/each}
        </select>
      </label>
      <label class="field">
        <span class="label">{t('arduino.port.label')}</span>
        <select class="select" bind:value={port} disabled={busy}>
          {#if ports.length === 0}<option value="">{t('arduino.port.none')}</option>{/if}
          {#each ports as p (p.id)}
            {@const a = portAvailability(p)}
            {@const g = boardGuessText(p, boards)}
            {@const name = t('flasher.port.option', { label: p.label, id: p.id })}
            {@const shown = g ? t('flasher.port.optionGuess', { option: name, guess: g }) : name}
            <option value={p.id}
              >{a.free
                ? shown
                : t('flasher.port.optionBusy', { option: shown, reason: a.text })}</option
            >
          {/each}
        </select>
      </label>
      <span class="spacer"></span>
      <button
        class="btn"
        onclick={save}
        disabled={busy || dirty.length === 0 || editLock !== null}
        title={editLock ?? t('arduino.save.title')}
        ><Icon name="save" size={16} /> {t('arduino.save')}</button
      >
      <button
        class="btn"
        onclick={startNew}
        disabled={busy || editLock !== null}
        title={editLock ?? undefined}
        ><Icon name="plus" size={16} /> {t('arduino.newSketch')}</button
      >
      <button class="btn" onclick={() => (picking = true)} disabled={busy}
        ><Icon name="open" size={16} /> {t('arduino.openSketch')}</button
      >
      <button
        class="btn"
        onclick={lookAtBoard}
        disabled={busy || !chosenPort}
        title={t('arduino.fromBoard.title')}
        ><Icon name="chip" size={16} /> {t('arduino.fromBoard')}</button
      >
    </div>

    <div class="tabs" role="tablist" aria-label={t('arduino.tabs.label')}>
      {#each tabs as tab (tab.name)}
        {@const changed = tab.text !== tab.saved}
        <button
          class="tab"
          role="tab"
          aria-selected={tab.name === active}
          aria-label={changed ? t('arduino.tabs.unsaved', { name: tab.name }) : tab.name}
          onclick={() => (active = tab.name)}
          >{tab.name}{#if changed}<span class="dot" aria-hidden="true"></span>{/if}</button
        >
      {/each}
      <button
        class="tab add"
        onclick={() => (addingTab = { name: '' })}
        aria-label={t('arduino.tabs.add')}
        title={t('arduino.tabs.add')}
        disabled={busy}><Icon name="plus" size={14} /></button
      >
    </div>

    {#if guided && active === `${sketch.name}.ino`}
      <div class="guided-bar" role="note">
        <Icon name="info" size={18} />
        <p>
          <b>{t('arduino.guided.title')}</b>
          {t('arduino.guided.body')}
        </p>
        <button
          class="btn"
          onclick={editPlan}
          disabled={editLock !== null}
          title={editLock ?? undefined}>{t('arduino.guided.edit')}</button
        >
        <button
          class="btn quiet"
          onclick={() => (takingOver = { error: null, busy: false })}
          disabled={editLock !== null || busy}
          title={editLock ?? undefined}>{t('arduino.guided.takeOver')}</button
        >
      </div>
    {/if}

    <div class="editors">
      {#each tabs as tab (`${generation}:${tab.name}`)}
        <CodeEditor
          readOnly={guided !== null && tab.name === `${sketch.name}.ino`}
          bind:this={editors[tab.name]}
          value={tab.saved}
          label={t('arduino.editor.label', { name: tab.name })}
          file={tab.name}
          others={() =>
            tabs
              .filter((other) => other.name !== tab.name)
              .map((other) => ({ name: other.name, text: other.text }))}
          diagnostics={tabDiagnostics.get(tab.name) ?? NO_DIAGNOSTICS}
          visible={visible && tab.name === active}
          onchange={(text) => edit(tab.name, text)}
        />
      {/each}
    </div>

    <section class="output" aria-label={t('arduino.output.label')}>
      {#if phase === 'uploading'}
        <div class="progress">
          <div
            class="bar"
            role="progressbar"
            aria-label={t('arduino.progress.label')}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={overallPercent(progress.stage, progress.done, progress.total)}
          >
            <div
              class="fill"
              style:width="{overallPercent(progress.stage, progress.done, progress.total)}%"
            ></div>
          </div>
          <p class="status" role="status">
            {t('arduino.output.uploading', { board: chosenBoard?.name ?? board })}
            {stageText(progress.stage, progress.done, progress.total)}
          </p>
        </div>
      {:else if phase === 'compiling' || phase === 'saving'}
        <p class="status" role="status">
          {phase === 'saving'
            ? t('arduino.output.saving')
            : t('arduino.output.compiling', { board: chosenBoard?.name ?? board })}
        </p>
      {:else if error}
        <ErrorNotice {error}>
          {#snippet actions()}
            {#if error?.code === 'port-used-by-bridge'}
              <button class="btn danger" onclick={() => upload(true)}
                >{t('arduino.bridge.stop')}</button
              >
            {/if}
          {/snippet}
        </ErrorNotice>
      {:else if result}
        {#if result.ok}
          <p class="status ok" role="status">
            <Icon name="check" size={16} />
            {uploaded?.flashed
              ? t('arduino.output.uploaded', {
                  board: uploaded.flashed.boardName,
                  sketch: result.sketchName,
                })
              : t('arduino.output.compiled', { board: chosenBoard?.name ?? result.board })}
          </p>
          {#if uploaded?.flashed?.stoppedPart}
            <p class="help">
              {t('arduino.output.restarted', { part: uploaded.flashed.stoppedPart })}
            </p>
          {/if}
          {#if uploaded && !uploaded.record}
            <p class="help">{t('arduino.output.notRecorded')}</p>
          {/if}
        {:else}
          <p class="status bad" role="status">
            {errors.length > 0
              ? t('arduino.output.failed', { count: errors.length })
              : t('arduino.output.failedOther')}
          </p>
          {#if result.message}<p class="help mono selectable">{result.message}</p>{/if}
        {/if}
        {#if result.size}
          <p class="size">
            {#each sizeLines(result.size) as line (line)}<span>{line}</span>{/each}
          </p>
        {/if}
        {#if result.diagnostics.length > 0}
          <ul class="diagnostics" aria-label={t('arduino.diag.list')}>
            {#each result.diagnostics as d, i (i)}
              {@const place = diagnosticPlace(d)}
              <li class={d.severity}>
                {#if d.inSketch && d.file && d.line !== undefined}
                  <button
                    class="place"
                    onclick={() => goto(d)}
                    aria-label={t('arduino.diag.goto', { place })}>{place}</button
                  >
                {:else if place}
                  <span class="place">{place}</span>
                {/if}
                <span class="kind"
                  >{d.severity === 'error'
                    ? t('arduino.diag.error')
                    : t('arduino.diag.warning')}</span
                >
                <span class="msg mono selectable">{d.message}</span>
              </li>
            {/each}
          </ul>
        {/if}
      {:else}
        <p class="help">{t('arduino.output.idle')}</p>
      {/if}
    </section>
  {/if}
</div>

{#if picking}
  <FilePicker
    mode="open"
    start={sketch?.folder ?? SKETCHBOOK}
    oncancel={() => (picking = false)}
    onpick={pickedFile}
  />
{/if}

{#if creating}
  <Dialog title={t('arduino.new.title')} oncancel={() => (creating = null)} wide>
    <div class="form">
      <label class="field">
        <span class="label">{t('arduino.new.name')}</span>
        <input
          class="input"
          bind:value={creating.name}
          spellcheck="false"
          autocomplete="off"
          onkeydown={(e) => e.key === 'Enter' && createSketch()}
        />
      </label>
      <p class="help">{t('arduino.new.where', { folder: SKETCHBOOK })}</p>
      <fieldset class="templates">
        <legend class="label">{t('arduino.new.template')}</legend>
        {#each info?.templates ?? [] as tpl (tpl.id)}
          <label class="choice">
            <input
              type="radio"
              name="{win.id}-template"
              value={tpl.id}
              bind:group={creating.template}
            />
            <span>
              <span class="name">{tpl.name}</span>
              <span class="help">{tpl.description}</span>
              {#if tpl.faberlink}<span class="badge">{t('arduino.new.faberlink')}</span>{/if}
            </span>
          </label>
        {/each}
      </fieldset>
      {#if creating.error}<ErrorNotice error={creating.error} />{/if}
    </div>
    {#snippet actions()}
      <button class="btn" onclick={() => (creating = null)}>{t('arduino.new.cancel')}</button>
      <button class="btn primary" onclick={createSketch}>{t('arduino.new.create')}</button>
    {/snippet}
  </Dialog>
{/if}

{#if addingTab}
  <Dialog title={t('arduino.newTab.title')} oncancel={() => (addingTab = null)}>
    <label class="field">
      <span class="label">{t('arduino.newTab.name')}</span>
      <input
        class="input mono"
        bind:value={addingTab.name}
        spellcheck="false"
        autocomplete="off"
        onkeydown={(e) => e.key === 'Enter' && addTab()}
      />
    </label>
    <p class="help">
      {addingTab.name.trim() && newTabProblem ? newTabProblem : t('arduino.newTab.help')}
    </p>
    {#snippet actions()}
      <button class="btn" onclick={() => (addingTab = null)}>{t('arduino.newTab.cancel')}</button>
      <button class="btn primary" onclick={addTab} disabled={!!newTabProblem}
        >{t('arduino.newTab.create')}</button
      >
    {/snippet}
  </Dialog>
{/if}

{#if phase === 'confirm-upload' && sketch && chosenBoard && chosenPort}
  <Dialog title={t('arduino.confirm.title')} oncancel={() => (phase = 'idle')} tone="danger">
    <p>
      <RichText
        text={t('arduino.confirm.body', {
          sketch: sketch.name,
          board: chosenBoard.name,
          port: chosenPort.label,
        })}
      />
    </p>
    {#snippet actions()}
      <button class="btn" onclick={() => (phase = 'idle')}>{t('arduino.confirm.cancel')}</button>
      <button class="btn danger solid" onclick={() => upload()}
        >{t('arduino.confirm.upload')}</button
      >
    {/snippet}
  </Dialog>
{/if}

{#if phase === 'confirm-install' && info}
  <Dialog title={t('arduino.tools.confirm.title')} oncancel={() => (phase = 'idle')} wide>
    <p>{t('arduino.tools.confirm.intro', { mb: info.tools.install?.approxMb ?? 150 })}</p>
    <ul class="downloads">
      {#if info.tools.state === 'missing' && info.tools.install}
        <li>
          <RichText
            text={t('arduino.tools.confirm.cli', {
              version: info.tools.install.cliVersion,
              url: info.tools.install.url,
            })}
          />
        </li>
      {:else}
        <li>{t('arduino.tools.confirm.cliThere')}</li>
      {/if}
      <li><RichText text={t('arduino.tools.confirm.core')} /></li>
    </ul>
    <p class="help">{t('arduino.tools.confirm.internet')}</p>
    {#snippet actions()}
      <button class="btn" onclick={() => (phase = 'idle')}
        >{t('arduino.tools.confirm.cancel')}</button
      >
      <button class="btn primary" onclick={install}>{t('arduino.tools.confirm.install')}</button>
    {/snippet}
  </Dialog>
{/if}

{#if code}
  <Dialog title={t('arduino.code.title')} oncancel={() => (code = null)} wide>
    <p class="help">{t('arduino.code.port', { port: chosenPort?.label ?? code.port })}</p>
    {#if code.busy && !code.info}
      <p role="status">{t('arduino.code.looking')}</p>
    {:else if code.busy}
      <p role="status">{t('arduino.code.checking')}</p>
    {:else if view}
      <p class="verdict" data-kind={view.kind} role="status">{view.text}</p>
      {#if view.kind === 'same' || view.kind === 'different'}<p class="help">{view.proof}</p>{/if}
      {#if code.info && identityCaveat(code.info)}
        <p class="help caveat">{identityCaveat(code.info)}</p>
      {/if}
      <p class="help">{t('arduino.code.why')}</p>
    {/if}
    {#if code.error}
      <ErrorNotice error={code.error}>
        {#snippet actions()}
          {#if code?.error?.code === 'port-used-by-bridge'}
            <button class="btn danger" onclick={() => checkCode(true)}
              >{t('arduino.code.stopAndCheck')}</button
            >
          {/if}
        {/snippet}
      </ErrorNotice>
    {/if}
    {#snippet actions()}
      <button class="btn" onclick={() => (code = null)}>{t('arduino.code.close')}</button>
      {#if view?.kind === 'same'}
        {@const id = view.record.id}
        <button class="btn primary" disabled={code?.busy} onclick={() => openRecord(id, false)}
          >{t('arduino.code.open')}</button
        >
      {:else if view?.kind === 'different'}
        {@const id = view.record.id}
        <button class="btn" disabled={code?.busy} onclick={() => openRecord(id, true)}
          >{t('arduino.code.openCopy')}</button
        >
      {:else if view?.kind === 'needs-check'}
        <button
          class="btn primary"
          disabled={code?.busy || flashLock !== null}
          title={flashLock ?? undefined}
          onclick={() => checkCode()}>{t('arduino.code.check')}</button
        >
      {/if}
    {/snippet}
  </Dialog>
{/if}

{#if takingOver && sketch}
  <Dialog
    title={t('arduino.guided.confirm.title')}
    oncancel={() => (takingOver = null)}
    tone="danger"
  >
    <p><RichText text={t('arduino.guided.confirm.body', { sketch: sketch.name })} /></p>
    <p class="help">{t('arduino.guided.confirm.after')}</p>
    {#if takingOver.error}<ErrorNotice error={takingOver.error} />{/if}
    {#snippet actions()}
      <button class="btn" onclick={() => (takingOver = null)}
        >{t('arduino.guided.confirm.cancel')}</button
      >
      <button class="btn danger solid" disabled={takingOver?.busy} onclick={takeOver}
        >{t('arduino.guided.confirm.ok')}</button
      >
    {/snippet}
  </Dialog>
{/if}

{#if closing && sketch}
  <Dialog
    title={t('arduino.close.title', { sketch: sketch.name })}
    oncancel={() => (closing = false)}
  >
    <p>{t('arduino.close.body', { files: dirty.map((tab) => tab.name).join(', ') })}</p>
    {#snippet actions()}
      <button class="btn" onclick={() => (closing = false)}>{t('arduino.close.cancel')}</button>
      <button class="btn danger" onclick={() => desktop.close(win.id)}
        >{t('arduino.close.discard')}</button
      >
      <button class="btn primary" onclick={saveAndClose}>{t('arduino.close.save')}</button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .arduino {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    background: var(--surface);
  }
  .tools {
    display: flex;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    background: var(--warn-soft);
    color: var(--text);
  }
  .tools-text {
    display: grid;
    gap: var(--space-1);
    flex: 1;
    min-width: 0;
  }
  .tools p,
  .start p,
  .output p {
    margin: 0;
  }
  .title {
    font-weight: 600;
  }
  .start {
    display: grid;
    align-content: start;
    gap: var(--space-2);
    padding: var(--space-4);
    max-width: 40rem;
    overflow-y: auto;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .help {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--line);
    background: var(--surface-2);
  }
  .toolbar .field {
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .toolbar .label {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .toolbar .select {
    max-width: 16rem;
  }
  .field.inline {
    display: grid;
    gap: 2px;
    max-width: 24rem;
  }
  .spacer {
    flex: 1;
  }
  .tabs {
    display: flex;
    gap: 2px;
    padding: var(--space-1) var(--space-2) 0;
    overflow-x: auto;
    border-bottom: 1px solid var(--line);
    background: var(--surface-2);
    flex-shrink: 0;
  }
  .tab {
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: var(--radius-sm) var(--radius-sm) 0 0;
    background: transparent;
    color: var(--text-muted);
    padding: var(--space-1) var(--space-3);
    min-height: 32px;
    font: inherit;
    font-size: var(--text-sm);
    white-space: nowrap;
    cursor: pointer;
  }
  .tab[aria-selected='true'] {
    background: var(--surface);
    border-color: var(--line);
    color: var(--text);
  }
  .tab:hover {
    color: var(--text);
  }
  .tab.add {
    display: inline-flex;
    align-items: center;
  }
  .dot::after {
    content: ' •';
    color: var(--accent);
  }
  .guided-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2) var(--space-3);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--line);
    background: var(--info-soft);
    font-size: var(--text-sm);
  }
  .guided-bar p {
    flex: 1 1 16rem;
    margin: 0;
  }
  .guided-bar :global(svg) {
    color: var(--info-text);
    flex: none;
  }
  .editors {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .output {
    flex-shrink: 0;
    max-height: 35%;
    overflow-y: auto;
    display: grid;
    gap: var(--space-1);
    padding: var(--space-2) var(--space-3);
    border-top: 1px solid var(--line);
    background: var(--surface-2);
    font-size: var(--text-sm);
  }
  .status {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .status.ok {
    color: var(--ok-text);
  }
  .status.bad {
    color: var(--danger-text);
    font-weight: 600;
  }
  .size {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-4);
    color: var(--text-muted);
  }
  .diagnostics {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 2px;
  }
  .diagnostics li {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: baseline;
  }
  .diagnostics .place {
    font-family: var(--font-data);
    color: var(--text);
    background: none;
    border: none;
    padding: 2px 0;
    font-size: var(--text-sm);
  }
  .diagnostics button.place {
    color: var(--accent-ink);
    text-decoration: underline;
    cursor: pointer;
    min-height: 28px;
  }
  .diagnostics .kind {
    font-weight: 600;
  }
  .diagnostics .error .kind {
    color: var(--danger-text);
  }
  .diagnostics .warning .kind {
    color: var(--warn-text);
  }
  .progress {
    display: grid;
    gap: var(--space-1);
  }
  .bar {
    height: 10px;
    border-radius: 5px;
    background: var(--surface-sunken);
    border: 1px solid var(--line);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.15s linear;
  }
  .fill.indeterminate {
    width: 30%;
    animation: slide 1.4s ease-in-out infinite;
  }
  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .fill.indeterminate {
      animation: none;
      width: 100%;
      opacity: 0.4;
    }
  }
  .form {
    display: grid;
    gap: var(--space-2);
  }
  .form .field {
    display: grid;
    gap: 2px;
  }
  .templates {
    display: grid;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    border: 0;
    min-width: 0;
  }
  .choice {
    display: flex;
    align-items: start;
    gap: var(--space-2);
  }
  .choice > span {
    display: grid;
  }
  .choice .name {
    font-weight: 600;
  }
  .badge {
    justify-self: start;
    font-size: var(--text-xs);
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    color: var(--accent-ink);
  }
  .downloads {
    margin: 0;
    padding-left: var(--space-5);
    display: grid;
    gap: var(--space-1);
  }
  .verdict {
    font-weight: 600;
  }
  .verdict[data-kind='different'] {
    color: var(--danger-text);
  }
  .verdict[data-kind='same'] {
    color: var(--ok-text);
  }
  .caveat {
    color: var(--warn-text);
  }
</style>
