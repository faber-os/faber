<script lang="ts">
  import { useCore } from '../lib/state/context';
  import { portAvailability } from '../lib/state/portHints';
  import { boardCheckText } from '../lib/state/bridges';
  import {
    boardGuessText,
    fileName,
    flashablePorts,
    guessedBoard,
    overallPercent,
    packagesFor,
    stageText,
  } from '../lib/state/flasher';
  import type {
    BoardCheck,
    ErrorBody,
    FirmwareRef,
    FlashBoard,
    FlashResult,
    FlashStage,
  } from '../lib/state/types';
  import { t } from '../lib/i18n';
  import { useDesktop } from '../components/desktopController.svelte';
  import Dialog from '../components/Dialog.svelte';
  import ErrorNotice from '../components/ErrorNotice.svelte';
  import FilePicker from '../components/FilePicker.svelte';
  import { formatBytes } from '../components/format';
  import Icon from '../components/Icon.svelte';
  import PortProblems from '../components/PortProblems.svelte';
  import RichText from '../components/RichText.svelte';
  import type { WindowProps } from '../components/windowDefs';

  let { win }: WindowProps = $props();

  const core = useCore();
  const desktop = useDesktop();

  let boards = $state<FlashBoard[]>([]);
  let boardsError = $state<ErrorBody | null>(null);
  let port = $state('');
  let board = $state('');
  // A board the user picked stays picked when the port list changes.
  let boardTouched = $state(false);
  let source = $state<'file' | 'package'>('file');
  let file = $state<string | null>(null);
  let pkg = $state('');
  let picking = $state(false);

  // idle → confirming → flashing → done (or back to idle with an error).
  let phase = $state<'idle' | 'confirming' | 'flashing' | 'done'>('idle');
  let progress = $state<{ stage: FlashStage; done: number; total: number }>({
    stage: 'connecting',
    done: 0,
    total: 0,
  });
  let result = $state<FlashResult | null>(null);
  let resultFromPackage = $state(false);
  let error = $state<ErrorBody | null>(null);
  let check = $state<{ result: BoardCheck } | { error: ErrorBody } | null>(null);
  let checking = $state(false);

  const ports = $derived(flashablePorts(core.ports ?? []));
  const chosenPort = $derived(ports.find((p) => p.id === port));
  const guess = $derived(guessedBoard(chosenPort, boards));
  const chosenBoard = $derived(boards.find((b) => b.id === board));
  const offered = $derived(board ? packagesFor(core.packages ?? [], board) : []);
  const firmware = $derived<FirmwareRef | null>(
    source === 'file'
      ? file
        ? { kind: 'file', path: file }
        : null
      : pkg && offered.some((p) => p.id === pkg)
        ? { kind: 'package', package: pkg }
        : null,
  );
  const firmwareName = $derived(
    firmware?.kind === 'file'
      ? fileName(firmware.path)
      : firmware
        ? t('flasher.firmware.packageFile', { package: firmware.package, board })
        : '',
  );
  const ready = $derived(
    !!chosenPort && !!chosenBoard && !!firmware && core.connection === 'connected',
  );

  $effect(() => {
    void core.flasherBoards().then((r) => {
      if (r.ok) boards = r.value;
      else boardsError = r.error;
    });
  });

  // Opened for one port (e.g. from Parts): pick it.
  $effect(() => {
    const wanted = win.params.port;
    if (!wanted) return;
    if (phase === 'idle') port = wanted;
    desktop.update(win.id, { params: {} });
  });

  // Pick a port once the list arrives: a free one an official Arduino is on, else any free one.
  $effect(() => {
    if (port !== '' || ports.length === 0 || boards.length === 0) return;
    const free = ports.filter((p) => portAvailability(p).free);
    port = (free.find((p) => guessedBoard(p, boards)) ?? free[0] ?? ports[0]!).id;
  });

  // The board its USB ids name, unless the user picked one.
  $effect(() => {
    if (!boardTouched) board = guess?.id ?? '';
  });

  // The only package with firmware for this board is picked by itself.
  $effect(() => {
    if (!offered.some((p) => p.id === pkg)) pkg = offered.length === 1 ? offered[0]!.id : '';
  });

  async function flash(stopPart = false) {
    if (!chosenPort || !chosenBoard || !firmware) return;
    phase = 'flashing';
    error = null;
    check = null;
    progress = { stage: 'connecting', done: 0, total: 0 };
    const target = chosenPort.id;
    const stop = core.watchFlash((p) => {
      if (p.port === target) progress = { stage: p.stage, done: p.done, total: p.total };
    });
    const r = await core.flash(target, {
      board: chosenBoard.id,
      firmware,
      ...(stopPart ? { stopPart: true } : {}),
    });
    stop();
    if (r.ok) {
      result = r.value;
      resultFromPackage = firmware.kind === 'package';
      phase = 'done';
    } else {
      error = r.error;
      phase = 'idle';
    }
  }

  async function checkBoard() {
    if (!result) return;
    checking = true;
    const r = await core.checkBoard(result.port);
    checking = false;
    check = r.ok ? { result: r.value } : { error: r.error };
  }

  function again() {
    phase = 'idle';
    result = null;
    check = null;
  }
</script>

<div class="flasher">
  {#if boardsError}
    <ErrorNotice error={boardsError} />
  {/if}

  {#if phase === 'done' && result}
    <section class="done" aria-live="polite">
      <p class="title">
        <Icon name="check" />
        {t('flasher.done.title', { board: result.boardName })}
      </p>
      <p>{t('flasher.done.body', { size: formatBytes(result.bytes) })}</p>
      {#if result.stoppedPart}
        <p>{t('flasher.done.restarted', { part: result.stoppedPart })}</p>
      {/if}
      <div class="actions">
        {#if resultFromPackage && !result.stoppedPart}
          <button class="btn primary" onclick={checkBoard} disabled={checking}
            >{checking ? t('flasher.done.checking') : t('flasher.done.check')}</button
          >
        {/if}
        <button class="btn" onclick={again}>{t('flasher.done.again')}</button>
      </div>
      {#if check && 'result' in check}
        <p class="check" role="status">{boardCheckText(check.result)}</p>
      {:else if check}
        <ErrorNotice error={check.error} />
      {/if}
    </section>
  {:else}
    <div class="form">
      <label class="field">
        <span class="label">{t('flasher.port.label')}</span>
        <select class="select" bind:value={port} disabled={phase !== 'idle'}>
          {#if ports.length === 0}<option value="">{t('flasher.port.none')}</option>{/if}
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
      {#if core.portProblems.length && ports.length === 0}
        <PortProblems />
      {/if}

      <label class="field">
        <span class="label">{t('flasher.board.label')}</span>
        <select
          class="select"
          bind:value={board}
          disabled={phase !== 'idle'}
          onchange={() => (boardTouched = true)}
        >
          <option value="" disabled>{t('flasher.board.pick')}</option>
          {#each boards as b (b.id)}
            <option value={b.id}>{t('flasher.board.option', { name: b.name, chip: b.chip })}</option
            >
          {/each}
        </select>
      </label>
      {#if guess}
        <p class="help">{t('flasher.board.guessed', { board: guess.name })}</p>
      {:else if chosenPort?.board?.usbChip}
        <p class="help">{t('flasher.board.clone')}</p>
      {/if}

      <fieldset class="field program" disabled={phase !== 'idle'}>
        <legend class="label">{t('flasher.firmware.label')}</legend>
        <label class="choice">
          <input type="radio" name="{win.id}-source" value="file" bind:group={source} />
          {t('flasher.firmware.file')}
        </label>
        {#if source === 'file'}
          <div class="file">
            <span class="path mono" class:none={!file}>{file ?? t('flasher.firmware.noFile')}</span>
            <button class="btn" onclick={() => (picking = true)}
              >{t('flasher.firmware.choose')}</button
            >
          </div>
          <p class="help">{t('flasher.firmware.help')}</p>
        {/if}
        <label class="choice">
          <input type="radio" name="{win.id}-source" value="package" bind:group={source} />
          {t('flasher.firmware.package')}
        </label>
        {#if source === 'package'}
          {#if offered.length === 0}
            <p class="help">
              {t('flasher.firmware.noPackages', { board: chosenBoard?.name ?? board })}
            </p>
          {:else}
            <label class="field">
              <span class="label">{t('flasher.firmware.packageLabel')}</span>
              <select class="select" bind:value={pkg}>
                {#each offered as p (p.id)}
                  <option value={p.id}
                    >{t('flasher.firmware.packageOption', {
                      name: p.name,
                      version: p.version,
                    })}</option
                  >
                {/each}
              </select>
            </label>
          {/if}
        {/if}
      </fieldset>

      <div class="warning" role="note">
        <Icon name="warning" />
        <div>
          <p class="title">{t('flasher.warning.title')}</p>
          <p>{t('flasher.warning.body')}</p>
        </div>
      </div>

      {#if error}
        <ErrorNotice {error}>
          {#snippet actions()}
            {#if error?.code === 'port-used-by-bridge'}
              <button class="btn danger" onclick={() => flash(true)}
                >{t('flasher.bridge.stop')}</button
              >
            {/if}
          {/snippet}
        </ErrorNotice>
      {/if}

      {#if phase === 'flashing'}
        <div class="progress">
          <div
            class="bar"
            role="progressbar"
            aria-label={t('flasher.progress.label')}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={overallPercent(progress.stage, progress.done, progress.total)}
          >
            <div
              class="fill"
              style:width="{overallPercent(progress.stage, progress.done, progress.total)}%"
            ></div>
          </div>
          <p class="stage" role="status">
            {stageText(progress.stage, progress.done, progress.total)}
          </p>
          <p class="help">{t('flasher.progress.keepPlugged')}</p>
        </div>
      {:else}
        <div class="actions">
          <button
            class="btn primary"
            disabled={!ready || phase !== 'idle'}
            onclick={() => (phase = 'confirming')}
            ><Icon name="upload" size={16} /> {t('flasher.flash')}</button
          >
        </div>
      {/if}
    </div>
  {/if}
</div>

{#if picking}
  <FilePicker
    mode="open"
    start={file ? file.slice(0, file.lastIndexOf('/')) : 'home:/Documents'}
    oncancel={() => (picking = false)}
    onpick={(path) => {
      picking = false;
      file = path;
    }}
  />
{/if}

{#if phase === 'confirming' && chosenBoard && chosenPort}
  <Dialog title={t('flasher.confirm.title')} oncancel={() => (phase = 'idle')} tone="danger">
    <p>
      <RichText
        text={t('flasher.confirm.body', {
          firmware: firmwareName,
          board: chosenBoard.name,
          port: chosenPort.label,
        })}
      />
    </p>
    {#snippet actions()}
      <button class="btn" onclick={() => (phase = 'idle')}>{t('flasher.confirm.cancel')}</button>
      <button class="btn danger solid" onclick={() => flash()}>{t('flasher.confirm.flash')}</button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .flasher {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-3) var(--space-4) var(--space-4);
    background: var(--surface);
  }
  .form {
    display: grid;
    gap: var(--space-3);
    max-width: 40rem;
  }
  .select {
    width: 100%;
  }
  .help {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .program {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    border: 0;
    min-width: 0;
  }
  .choice {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .file {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding-left: var(--space-5);
    min-width: 0;
  }
  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-sm);
  }
  .path.none {
    color: var(--text-muted);
  }
  .warning {
    display: flex;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-md);
    background: var(--warn-soft);
    color: var(--text);
  }
  .warning p,
  .done p {
    margin: 0;
  }
  .title {
    font-weight: 600;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
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
  .stage {
    margin: 0;
    font-size: var(--text-sm);
  }
  .done {
    display: grid;
    gap: var(--space-2);
    max-width: 40rem;
  }
  .done .title {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .check {
    margin: 0;
    font-size: var(--text-sm);
  }
</style>
