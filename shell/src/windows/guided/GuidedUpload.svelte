<script lang="ts">
  import { untrack } from 'svelte';
  import { t } from '../../lib/i18n';
  import { useCore } from '../../lib/state/context';
  import { portAvailability } from '../../lib/state/portHints';
  import { flashablePorts, guessedBoard, overallPercent, stageText } from '../../lib/state/flasher';
  import { BRIDGE_CONNECTOR, isBridge, nameFromBoard } from '../../lib/state/bridges';
  import { sizeLines } from '../../lib/desktop/arduino';
  import type {
    ArduinoInfo,
    CompileResult,
    ErrorBody,
    FlashStage,
    GuidedBoard,
    UploadResult,
  } from '../../lib/state/types';
  import { useDesktop } from '../../components/desktopController.svelte';
  import { openInArduino } from '../../components/arduinoWindow';
  import Dialog from '../../components/Dialog.svelte';
  import ErrorNotice from '../../components/ErrorNotice.svelte';
  import Icon from '../../components/Icon.svelte';
  import RichText from '../../components/RichText.svelte';

  /**
   * Verify and upload a guided board's sketch (docs/wiring.md §3, step 5), with the editor's
   * compile and upload calls and its confirm. Verifying needs an operator, uploading an admin,
   * as in the editor. After the upload it points at where the parts appear in Parts.
   */
  let {
    board,
    onshow,
  }: {
    board: GuidedBoard;
    /** Shows a part in Parts (closing the wizard). */
    onshow: (part: string) => void;
  } = $props();

  const core = useCore();
  const desktop = useDesktop();
  const editLock = $derived(core.auth.lock('operator'));
  const flashLock = $derived(core.auth.lock('admin'));

  let info = $state.raw<ArduinoInfo | null>(null);
  let infoError = $state.raw<ErrorBody | null>(null);
  let port = $state('');
  let phase = $state<'idle' | 'compiling' | 'confirm' | 'uploading' | 'adding'>('idle');
  let result = $state.raw<CompileResult | null>(null);
  let uploaded = $state.raw<UploadResult | null>(null);
  let error = $state.raw<ErrorBody | null>(null);
  let progress = $state<{ stage: FlashStage; done: number; total: number }>({
    stage: 'connecting',
    done: 0,
    total: 0,
  });

  $effect(() => {
    untrack(async () => {
      const r = await core.arduino();
      if (r.ok) info = r.value;
      else infoError = r.error;
    });
  });

  const type = $derived(board.plan?.board ?? '');
  /** The Nano's two bootloaders are one board to a plan: either can be uploaded to. */
  const family = (id: string) => (id === 'nano-old' ? 'nano' : id);
  const boards = $derived((info?.boards ?? []).filter((b) => family(b.id) === family(type)));
  let boardId = $state('');
  const ports = $derived(flashablePorts(core.ports ?? []));
  const chosenPort = $derived(ports.find((p) => p.id === port));
  const chosenBoard = $derived(boards.find((b) => b.id === boardId) ?? boards[0]);
  const ready = $derived(info?.tools.state === 'ready');
  const busy = $derived(phase !== 'idle' && phase !== 'confirm');
  const errors = $derived(result?.diagnostics.filter((d) => d.severity === 'error') ?? []);

  // A free port whose USB ids name this kind of board, else any free one.
  $effect(() => {
    if (port !== '' || ports.length === 0) return;
    const free = ports.filter((p) => portAvailability(p).free);
    port = (free.find((p) => guessedBoard(p, boards)) ?? free[0] ?? ports[0]!).id;
  });
  $effect(() => {
    if (boardId !== '' || boards.length === 0) return;
    boardId = (guessedBoard(chosenPort, boards) ?? boards.find((b) => b.id === type) ?? boards[0]!)
      .id;
  });

  /** The board's own part in Parts, on the port it was uploaded to. */
  const bridge = $derived(
    uploaded?.flashed ? (core.parts ?? []).find((p) => isBridge(p) && p.port === port) : undefined,
  );
  const firstPart = $derived(Object.keys(board.plan?.parts ?? {})[0]);

  async function verify() {
    if (!chosenBoard) return;
    phase = 'compiling';
    error = null;
    uploaded = null;
    result = null;
    const r = await core.compileSketch(board.sketch, chosenBoard.id);
    phase = 'idle';
    if (r.ok) result = r.value;
    else error = r.error;
  }

  async function upload(stopPart = false) {
    if (!chosenBoard || !chosenPort) return;
    phase = 'uploading';
    error = null;
    result = null;
    uploaded = null;
    progress = { stage: 'connecting', done: 0, total: 0 };
    const target = chosenPort.id;
    const stop = core.watchFlash((p) => {
      if (p.port === target) progress = { stage: p.stage, done: p.done, total: p.total };
    });
    const r = await core.uploadSketch(board.sketch, chosenBoard.id, target, stopPart);
    stop();
    phase = 'idle';
    if (r.ok) {
      result = r.value.compile;
      uploaded = r.value.flashed ? r.value : null;
    } else error = r.error;
  }

  /** The board isn't in Parts yet: add it, so Faber talks to it and its parts appear. */
  async function addBoard() {
    if (!chosenPort) return;
    phase = 'adding';
    error = null;
    const name = nameFromBoard(board.name) || 'board';
    const r = await core.addPart({ name, connector: BRIDGE_CONNECTOR, port: chosenPort.id });
    phase = 'idle';
    if (!r.ok) {
      error = r.error;
      return;
    }
    onshow(firstPart ? `${r.value.name}.${firstPart}` : r.value.name);
  }
</script>

<div class="upload">
  {#if infoError}<ErrorNotice error={infoError} />{/if}
  {#if info && !ready}
    <ErrorNotice
      tone="warn"
      error={{ message: t('guided.upload.noTools'), hint: t('guided.upload.noTools.hint') }}
    >
      {#snippet actions()}
        <button class="btn" onclick={() => openInArduino(desktop, board.sketch)}
          >{t('guided.upload.openEditor')}</button
        >
      {/snippet}
    </ErrorNotice>
  {/if}

  <p class="help">
    {t('guided.upload.what', { sketch: board.sketch, board: chosenBoard?.name ?? type })}
  </p>
  <div class="fields">
    {#if boards.length > 1}
      <label class="field">
        <span class="label">{t('arduino.board.label')}</span>
        <select class="select" bind:value={boardId} disabled={busy}>
          {#each boards as b (b.id)}<option value={b.id}>{b.name}</option>{/each}
        </select>
      </label>
    {/if}
    <label class="field">
      <span class="label">{t('arduino.port.label')}</span>
      <select class="select" bind:value={port} disabled={busy}>
        {#if ports.length === 0}<option value="">{t('arduino.port.none')}</option>{/if}
        {#each ports as p (p.id)}
          {@const a = portAvailability(p)}
          {@const name = t('flasher.port.option', { label: p.label, id: p.id })}
          <option value={p.id}
            >{a.free
              ? name
              : t('flasher.port.optionBusy', { option: name, reason: a.text })}</option
          >
        {/each}
      </select>
    </label>
  </div>

  <div class="buttons">
    <button
      class="btn"
      onclick={verify}
      disabled={!ready || busy || !chosenBoard || editLock !== null}
      title={editLock ?? t('arduino.verify.title')}
      ><Icon name="check" size={16} /> {t('arduino.verify')}</button
    >
    <button
      class="btn primary"
      onclick={() => (phase = 'confirm')}
      disabled={!ready || busy || !chosenBoard || !chosenPort || flashLock !== null}
      title={flashLock ?? t('arduino.upload.title')}
      ><Icon name="upload" size={16} /> {t('arduino.upload')}</button
    >
    <button class="btn quiet" onclick={() => openInArduino(desktop, board.sketch)}
      ><Icon name="code" size={16} /> {t('guided.upload.openSketch')}</button
    >
  </div>
  {#if flashLock}<p class="help">{t('guided.upload.needsAdmin', { reason: flashLock })}</p>{/if}

  <div class="output" aria-live="polite">
    {#if phase === 'uploading'}
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
      <p role="status">
        {t('arduino.output.uploading', { board: chosenBoard?.name ?? type })}
        {stageText(progress.stage, progress.done, progress.total)}
      </p>
    {:else if phase === 'compiling'}
      <p role="status">{t('arduino.output.compiling', { board: chosenBoard?.name ?? type })}</p>
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
        <p class="ok" role="status">
          <Icon name="check" size={16} />
          {uploaded?.flashed
            ? t('arduino.output.uploaded', {
                board: uploaded.flashed.boardName,
                sketch: result.sketchName,
              })
            : t('arduino.output.compiled', { board: chosenBoard?.name ?? result.board })}
        </p>
      {:else}
        <p class="bad" role="status">
          {errors.length > 0
            ? t('arduino.output.failed', { count: errors.length })
            : t('arduino.output.failedOther')}
        </p>
        <ul class="diagnostics">
          {#each result.diagnostics as d, i (i)}
            <li><span class="mono selectable">{d.message}</span></li>
          {/each}
        </ul>
      {/if}
      {#if result.size}
        <p class="size">
          {#each sizeLines(result.size) as line (line)}<span>{line}</span>{/each}
        </p>
      {/if}
    {/if}
  </div>

  {#if uploaded?.flashed}
    <div class="next">
      {#if bridge}
        <p>
          <RichText
            text={t('guided.upload.appears', { board: bridge.name, part: firstPart ?? '' })}
          />
        </p>
        <button
          class="btn primary"
          onclick={() => onshow(firstPart ? `${bridge.name}.${firstPart}` : bridge.name)}
          >{t('guided.upload.show')}</button
        >
      {:else}
        <p>{t('guided.upload.addBoard.body')}</p>
        <button
          class="btn primary"
          onclick={addBoard}
          disabled={phase === 'adding' || flashLock !== null}
          title={flashLock ?? undefined}>{t('guided.upload.addBoard')}</button
        >
      {/if}
    </div>
  {/if}
</div>

{#if phase === 'confirm' && chosenBoard && chosenPort}
  <Dialog title={t('arduino.confirm.title')} oncancel={() => (phase = 'idle')} tone="danger">
    <p>
      <RichText
        text={t('arduino.confirm.body', {
          sketch: board.name,
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

<style>
  .upload {
    display: grid;
    gap: var(--space-3);
  }
  .fields {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-3);
  }
  .fields .field {
    flex: 1 1 14rem;
    max-width: 24rem;
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .help {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .output {
    display: grid;
    gap: var(--space-1);
    font-size: var(--text-sm);
  }
  .ok {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--ok-text);
    font-weight: 600;
  }
  .bad {
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
    margin: 0;
    padding-left: var(--space-4);
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
  .next {
    display: grid;
    gap: var(--space-2);
    justify-items: start;
    padding: var(--space-3);
    border-radius: var(--radius-md);
    background: var(--ok-soft);
  }
</style>
