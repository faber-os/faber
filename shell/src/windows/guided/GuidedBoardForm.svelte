<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { t } from '../../lib/i18n';
  import { useCore } from '../../lib/state/context';
  import {
    boardNameFor,
    defaultSettings,
    freeName,
    partGroup,
    partNameProblem,
    readSetting,
    roleNameIn,
    settingText,
    templateGroups,
    wireChanges,
    type PartGroup,
    type WireChange,
  } from '../../lib/state/guided';
  import { sketchNameProblem } from '../../lib/desktop/arduino';
  import type {
    ArduinoBoard,
    BoardPlan,
    ErrorBody,
    GuidedBoard,
    GuidedBoardSummary,
    PartSuggestion,
    PartTemplate,
    PinMap,
    PlanProblem,
    SettingValue,
  } from '../../lib/state/types';
  import { useDesktop } from '../../components/desktopController.svelte';
  import { openInArduino } from '../../components/arduinoWindow';
  import AddPartMode from '../../components/AddPartMode.svelte';
  import ErrorNotice from '../../components/ErrorNotice.svelte';
  import Icon from '../../components/Icon.svelte';
  import type { IconName } from '../../components/Icon.svelte';
  import GuidedUpload from './GuidedUpload.svelte';
  import PartSettingsForm from './PartSettingsForm.svelte';
  import PinPicker from './PinPicker.svelte';
  import WiringChecklist from './WiringChecklist.svelte';

  /**
   * Parts → Add part → On a board's pins (docs/wiring.md §3), and editing a guided board's plan
   * later: pick the board (or start one), the part, its name, shape and settings, its pins on a
   * drawing of the board, then the wiring checklist and the upload. Works for any template the
   * core serves.
   */
  let {
    initialPlan = null,
    oncancel,
    onshow,
    onport,
  }: {
    /** Open this guided board's plan ("Edit plan"), instead of starting at the board. */
    initialPlan?: string | null;
    oncancel: () => void;
    /** Shows a part in Parts (the wizard closes). */
    onshow: (part: string) => void;
    /** Switches Add part back to a part on a port; absent when editing a plan. */
    onport?: () => void;
  } = $props();

  const core = useCore();
  const desktop = useDesktop();
  /** Starting, saving and taking over a plan is an operator's; uploading an admin's. */
  const editLock = $derived(core.auth.lock('operator'));

  type Step = 'board' | 'plan' | 'part' | 'pins' | 'wiring' | 'upload';
  // svelte-ignore state_referenced_locally (the form is mounted afresh for each plan)
  let step = $state<Step>(initialPlan ? 'plan' : 'board');
  // svelte-ignore state_referenced_locally
  const editFlow = initialPlan !== null;

  let plans = $state.raw<GuidedBoardSummary[] | null>(null);
  let templates = $state.raw<PartTemplate[] | null>(null);
  let boardTypes = $state.raw<ArduinoBoard[]>([]);
  let loadError = $state.raw<ErrorBody | null>(null);

  const NEW = '\u0000new';
  let choice = $state('');
  let newType = $state('uno');
  let newName = $state('');
  let newNameTouched = $state(false);
  let board = $state.raw<GuidedBoard | null>(null);
  let map = $state.raw<PinMap | null>(null);
  let working = $state(false);
  let error = $state.raw<ErrorBody | null>(null);

  // The part being added or changed.
  let templateId = $state('');
  let shapeId = $state('');
  let partName = $state('');
  let partNameTouched = $state(false);
  let settings = $state<Record<string, string>>({});
  let editing = $state<string | null>(null);
  let suggestion = $state.raw<PartSuggestion | null>(null);
  let pins = $state<Record<string, string>>({});
  let firstPick = $state<Record<string, string>>({});
  let currentRole = $state('');
  let suggesting = $state(false);
  let nameError = $state<string | null>(null);
  /** The part's shape, name and settings, scrolled to once a part is picked. */
  let details = $state<HTMLElement | undefined>();
  let scroller = $state<HTMLElement | undefined>();
  // Each step starts at its top.
  $effect(() => {
    void step;
    if (scroller) scroller.scrollTop = 0;
  });

  // Saving.
  let saveError = $state.raw<ErrorBody | null>(null);
  let changes = $state.raw<WireChange[]>([]);
  let removing = $state<string | null>(null);

  const plan = $derived(board?.plan ?? null);
  const template = $derived(templates?.find((x) => x.id === templateId) ?? null);
  const shape = $derived(template?.shapes.find((s) => s.id === shapeId) ?? template?.shapes[0]);
  const groups = $derived(templateGroups(templates ?? []));
  const typeName = (id: string) => boardTypes.find((b) => b.id === id)?.name ?? id;
  const takenOver = $derived(plan?.takenOver !== undefined);

  const newNameProblem = $derived.by(() => {
    const n = newName.trim();
    const basic = sketchNameProblem(n);
    if (basic) return basic;
    if (plans?.some((p) => p.name.toLowerCase() === n.toLowerCase()))
      return t('guided.board.nameTaken', { name: n });
    return null;
  });
  const partProblem = $derived(
    plan ? (partNameProblem(partName, plan, editing ?? undefined) ?? nameError) : null,
  );
  const settingsOk = $derived(
    template?.settings.every((s) => readSetting(s, settings[s.id] ?? '').ok) ?? false,
  );
  const missingRoles = $derived(
    suggestion?.roles.filter((r) => !r.optional && !pins[r.role]).map((r) => r.name) ?? [],
  );
  const pinProblems = $derived(suggestion?.problems ?? []);

  $effect(() => {
    untrack(() => void load());
  });

  async function load() {
    const [p, tpl, info] = await Promise.all([core.plans(), core.partTemplates(), core.arduino()]);
    if (p.ok) plans = p.value;
    else loadError = p.error;
    if (tpl.ok) templates = tpl.value;
    else loadError = tpl.error;
    if (info.ok) boardTypes = info.value.boards;
    if (p.ok) {
      const usable = p.value.filter((x) => !x.takenOver);
      choice = usable[0]?.name ?? NEW;
    }
    if (!newName)
      newName = freeName(
        boardNameFor(newType),
        (plans ?? []).map((x) => x.name),
      );
    if (initialPlan) await openBoard(initialPlan, 'plan');
  }

  /** Loads a plan and its board's pins, then goes to `next`. */
  async function openBoard(name: string, next: Step) {
    working = true;
    error = null;
    const r = await core.plan(name);
    if (!r.ok) {
      working = false;
      error = r.error;
      return;
    }
    await useBoard(r.value, next);
  }

  async function useBoard(g: GuidedBoard, next: Step) {
    board = g;
    if (g.plan && map?.id !== g.plan.board) {
      const m = await core.pinMap(g.plan.board);
      if (!m.ok) {
        working = false;
        error = m.error;
        return;
      }
      map = m.value;
    }
    working = false;
    step = next;
  }

  async function boardNext() {
    if (choice !== NEW) {
      await openBoard(choice, 'part');
      startNewPart();
      return;
    }
    newNameTouched = true;
    if (newNameProblem) return;
    working = true;
    error = null;
    const r = await core.newPlan(newName.trim(), newType);
    if (!r.ok) {
      working = false;
      error = r.error;
      return;
    }
    plans = [...(plans ?? []), { name: r.value.name, board: newType, parts: 0, takenOver: false }];
    await useBoard(r.value, 'part');
    startNewPart();
  }

  function startNewPart() {
    editing = null;
    templateId = '';
    shapeId = '';
    partName = '';
    partNameTouched = false;
    nameError = null;
    settings = {};
    suggestion = null;
    saveError = null;
  }

  function pickTemplate(id: string) {
    const tpl = templates?.find((x) => x.id === id);
    if (!tpl || !plan) return;
    templateId = id;
    shapeId = tpl.shapes[0]?.id ?? '';
    if (!partNameTouched) partName = freeName(tpl.defaultName, Object.keys(plan.parts));
    settings = Object.fromEntries(
      Object.entries(defaultSettings(tpl)).map(([k, v]) => [k, settingText(v)]),
    );
    // On a small screen the rest of the step is below the catalogue: bring it into view.
    void tick().then(() => details?.scrollIntoView?.({ block: 'start', behavior: 'smooth' }));
  }

  /** Changes a part already on the board: its settings and shape, then its pins. */
  function editPart(name: string, to: 'part' | 'pins') {
    const p = plan?.parts[name];
    const tpl = templates?.find((x) => x.id === p?.template);
    if (!p || !tpl) return;
    startNewPart();
    editing = name;
    templateId = tpl.id;
    shapeId = p.shape ?? tpl.shapes[0]?.id ?? '';
    partName = name;
    settings = Object.fromEntries(
      tpl.settings.map((s) => [s.id, settingText(p.settings[s.id] ?? s.default)]),
    );
    pins = { ...p.pins };
    if (to === 'pins') void toPins();
    else step = 'part';
  }

  function settingValues(): Record<string, SettingValue> {
    const out: Record<string, SettingValue> = {};
    for (const s of template?.settings ?? []) {
      const r = readSetting(s, settings[s.id] ?? '');
      if (r.ok) out[s.id] = r.value;
    }
    return out;
  }

  /** Asks the core where the part could go; `chosen` are pins already decided. */
  async function suggest(chosen: Record<string, string>): Promise<PartSuggestion | null> {
    if (!board || !template) return null;
    suggesting = true;
    const r = await core.suggestPart(board.name, {
      template: template.id,
      ...(shape ? { shape: shape.id } : {}),
      name: partName,
      pins: chosen,
      settings: settingValues(),
    });
    suggesting = false;
    if (!r.ok) {
      if (r.error.code === 'invalid-part' && step === 'part') nameError = r.error.message;
      else error = r.error;
      return null;
    }
    return r.value;
  }

  async function toPins() {
    partNameTouched = true;
    if (!template || partProblem || !settingsOk) return;
    error = null;
    // Moving a part's pins starts from where they are, unless its shape (and so its roles) changed.
    const was = editing ? plan?.parts[editing] : undefined;
    const keep = was && (was.shape ?? template.shapes[0]?.id) === shape?.id ? pins : {};
    const s = await suggest(editing ? keep : {});
    if (!s) return;
    suggestion = s;
    pins = { ...s.part.pins };
    firstPick = Object.fromEntries(
      s.roles.filter((r) => r.suggested).map((r) => [r.role, r.suggested!]),
    );
    currentRole =
      (s.roles.find((r) => !r.optional && !s.part.pins[r.role]) ?? s.roles[0])?.role ?? '';
    saveError = null;
    step = 'pins';
  }

  async function pick(role: string, pin: string) {
    const next = { ...pins };
    // A pin another role of this part has: the two swap.
    const other = Object.keys(next).find((r) => r !== role && next[r] === pin);
    const old = next[role];
    if (other) {
      if (old) next[other] = old;
      else delete next[other];
    }
    next[role] = pin;
    pins = next;
    const roles = suggestion?.roles ?? [];
    const after = roles.slice(roles.findIndex((r) => r.role === role) + 1);
    const empty = after.find((r) => !r.optional && !next[r.role]);
    if (empty) currentRole = empty.role;
    await refresh(next);
  }

  async function clear(role: string) {
    const next = { ...pins };
    delete next[role];
    pins = next;
    await refresh(next);
  }

  async function refresh(chosen: Record<string, string>) {
    const s = await suggest(chosen);
    if (!s) return;
    suggestion = s;
    pins = { ...s.part.pins };
  }

  /** Saves `next` as the board's plan, then shows the wiring. */
  async function savePlan(next: BoardPlan, overwrite = false) {
    if (!board?.plan) return;
    const before = board.plan;
    working = true;
    saveError = null;
    const r = await core.savePlan(next, overwrite);
    working = false;
    if (!r.ok) {
      saveError = r.error;
      return;
    }
    const roleName = (p: Parameters<typeof roleNameIn>[1], role: string) =>
      roleNameIn(templates ?? [], p, role);
    changes = r.value.plan ? wireChanges(before, r.value.plan, roleName) : [];
    board = r.value;
    plans = (plans ?? []).map((p) =>
      p.name === r.value.name ? { ...p, parts: Object.keys(r.value.plan?.parts ?? {}).length } : p,
    );
    removing = null;
    step = 'wiring';
  }

  function savePart(overwrite = false) {
    if (!plan || !template || !suggestion) return;
    const parts = { ...plan.parts };
    parts[partName] = {
      template: template.id,
      ...(suggestion.shape ? { shape: suggestion.shape } : {}),
      pins: { ...pins },
      settings: settingValues(),
    };
    void savePlan({ ...plan, parts }, overwrite);
  }

  function removePart(name: string) {
    if (!plan) return;
    const parts = { ...plan.parts };
    delete parts[name];
    void savePlan({ ...plan, parts });
  }

  function popout() {
    if (!board) return;
    const name = board.name;
    desktop.open('wiring', {
      key: `wiring:${name}`,
      title: t('guided.wiring.windowTitle', { board: name }),
      params: { plan: name },
    });
  }

  function back() {
    error = null;
    saveError = null;
    switch (step) {
      case 'board':
      case 'plan':
        oncancel();
        return;
      case 'part':
        step = editFlow || editing ? 'plan' : 'board';
        return;
      case 'pins':
        step = 'part';
        return;
      case 'wiring':
        step = 'plan';
        return;
      case 'upload':
        step = 'wiring';
        return;
    }
  }

  const ICONS: Record<PartGroup, IconName> = {
    distance: 'distance',
    motion: 'drive',
    inputs: 'digital',
    outputs: 'power',
    power: 'battery',
    sensors: 'gauge',
    other: 'chip',
  };

  const STEPS = $derived<{ id: Step; title: string }[]>([
    editFlow
      ? { id: 'plan', title: t('guided.step.plan') }
      : { id: 'board', title: t('guided.step.board') },
    { id: 'part', title: t('guided.step.part') },
    { id: 'pins', title: t('guided.step.pins') },
    { id: 'wiring', title: t('guided.step.wiring') },
    { id: 'upload', title: t('guided.step.upload') },
  ]);
  const stepIndex = $derived(
    Math.max(
      0,
      STEPS.findIndex((s) => s.id === (step === 'plan' || step === 'board' ? STEPS[0]!.id : step)),
    ),
  );

  function problemWhere(p: PlanProblem): string {
    // The part being added isn't in the saved plan yet.
    const part =
      p.part && p.part === partName && template
        ? { template: template.id, shape: shape?.id, pins: {}, settings: {} }
        : p.part && plan?.parts[p.part];
    const role = p.role && part ? roleNameIn(templates ?? [], part, p.role) : p.role;
    return [p.part, role, p.pin].filter(Boolean).join(' · ');
  }
</script>

{#snippet problemList(list: PlanProblem[], label: string)}
  <ul class="problems" aria-label={label}>
    {#each list as p, i (i)}
      {@const where = problemWhere(p)}
      <li class={p.severity}>
        <Icon name="warning" size={16} />
        <span>
          {#if where}<span class="where">{where}</span>{/if}
          <span class="msg">{p.message}</span>
          {#if p.hint}<span class="hint">{p.hint}</span>{/if}
        </span>
      </li>
    {/each}
  </ul>
{/snippet}

<div class="guided" aria-label={t('guided.form.label')} role="form">
  <div class="toolbar">
    <button type="button" class="btn quiet" onclick={back}>
      <Icon name="back" size={17} />
      {step === 'board' || step === 'plan' ? t('addPart.back') : t('guided.back')}
    </button>
    <h3>
      {initialPlan ? t('guided.title.edit', { board: initialPlan }) : t('addPart.title')}
    </h3>
    <ol class="steps" aria-label={t('guided.steps')}>
      {#each STEPS as s, i (s.id)}
        <li
          class:done={i < stepIndex}
          class:now={i === stepIndex}
          aria-current={i === stepIndex ? 'step' : undefined}
        >
          <span class="n">{i + 1}</span><span class="step-title">{s.title}</span>
        </li>
      {/each}
    </ol>
    <span class="spacer"></span>
    {#if onport}
      <AddPartMode mode="pins" onchange={(m) => m === 'port' && onport?.()} />
    {/if}
  </div>

  <div class="scroll" bind:this={scroller}>
    {#if loadError}<ErrorNotice error={loadError} />{/if}

    {#if step === 'board'}
      <fieldset class="step">
        <legend>{t('guided.board.legend')}</legend>
        <p class="help">{t('guided.board.help')}</p>
        {#if plans === null && !loadError}
          <p class="muted">{t('guided.loading')}</p>
        {:else}
          <div class="choices">
            {#each plans ?? [] as p (p.name)}
              <label class="choice" class:disabled={p.takenOver}>
                <input
                  type="radio"
                  name="guided-board"
                  value={p.name}
                  checked={choice === p.name}
                  disabled={p.takenOver}
                  onchange={() => (choice = p.name)}
                />
                <span class="choice-body">
                  <span class="choice-title">{p.name}</span>
                  <span class="choice-meta">
                    <span>{p.board ? typeName(p.board) : t('guided.board.unknownType')}</span>
                    <span>{t('guided.board.parts', { count: p.parts })}</span>
                  </span>
                  {#if p.takenOver}
                    <span class="help">{t('guided.board.takenOver')}</span>
                  {:else if p.problem}
                    <span class="help bad">{p.problem}</span>
                  {/if}
                </span>
              </label>
            {/each}
            <label class="choice">
              <input
                type="radio"
                name="guided-board"
                value={NEW}
                checked={choice === NEW}
                onchange={() => (choice = NEW)}
              />
              <span class="choice-body">
                <span class="choice-title">{t('guided.board.new')}</span>
                <span class="help">{t('guided.board.new.help')}</span>
              </span>
            </label>
          </div>
          {#if choice === NEW}
            <div class="new-board">
              <label class="field">
                <span class="label">{t('guided.board.type')}</span>
                <select
                  class="select"
                  bind:value={newType}
                  onchange={() => {
                    if (!newNameTouched)
                      newName = freeName(
                        boardNameFor(newType),
                        (plans ?? []).map((x) => x.name),
                      );
                  }}
                >
                  {#each boardTypes as b (b.id)}<option value={b.id}>{b.name}</option>{/each}
                </select>
              </label>
              <label class="field">
                <span class="label">{t('guided.board.name')}</span>
                <input
                  class="input mono"
                  bind:value={newName}
                  oninput={() => (newNameTouched = true)}
                  aria-invalid={newNameProblem !== null}
                  aria-describedby="guided-board-name-help"
                  spellcheck="false"
                  autocomplete="off"
                />
              </label>
              <p id="guided-board-name-help" class="help wide" class:bad={newNameProblem !== null}>
                {newNameProblem ?? t('guided.board.name.help')}
              </p>
            </div>
          {/if}
        {/if}
      </fieldset>
    {:else if step === 'plan' && board}
      <section class="step">
        <p class="board-head">
          <b>{board.name}</b>
          {#if plan}<span class="muted">{typeName(plan.board)}</span>{/if}
        </p>
        {#if board.fileProblem}
          <ErrorNotice
            error={{ message: board.fileProblem.message, hint: board.fileProblem.hint }}
          />
        {:else if takenOver}
          <ErrorNotice tone="warn" error={{ message: t('guided.plan.takenOver'), hint: '' }} />
        {:else if board.edited}
          <ErrorNotice
            tone="warn"
            error={{ message: t('guided.plan.edited'), hint: t('guided.plan.edited.hint') }}
          />
        {/if}
        {#if board.problems.length > 0}
          {@render problemList(board.problems, t('guided.plan.problems'))}
        {/if}
        {#if plan && Object.keys(plan.parts).length === 0}
          <p class="muted">{t('guided.plan.empty')}</p>
        {/if}
        {#if plan}
          <ul class="planned">
            {#each Object.entries(plan.parts) as [name, part] (name)}
              {@const tpl = templates?.find((x) => x.id === part.template)}
              <li>
                <div class="planned-main">
                  <span class="planned-name"><code>{name}</code> {tpl?.name ?? part.template}</span>
                  <span class="planned-pins">
                    {Object.entries(part.pins)
                      .map(([r, pin]) =>
                        t('parts.pins.pin', { role: roleNameIn(templates ?? [], part, r), pin }),
                      )
                      .join(' · ')}
                  </span>
                </div>
                {#if removing === name}
                  <span class="confirm">{t('guided.plan.removeConfirm')}</span>
                  <button
                    class="btn danger solid"
                    disabled={working || editLock !== null}
                    onclick={() => removePart(name)}>{t('guided.plan.remove')}</button
                  >
                  <button class="btn quiet" onclick={() => (removing = null)}
                    >{t('guided.plan.keep')}</button
                  >
                {:else if !takenOver}
                  <button
                    class="btn"
                    disabled={editLock !== null || !tpl}
                    title={editLock ?? undefined}
                    aria-label={t('guided.plan.pinsNamed', { name })}
                    onclick={() => editPart(name, 'pins')}>{t('guided.plan.pins')}</button
                  >
                  <button
                    class="btn"
                    disabled={editLock !== null || !tpl}
                    title={editLock ?? undefined}
                    aria-label={t('guided.plan.settingsNamed', { name })}
                    onclick={() => editPart(name, 'part')}>{t('guided.plan.settings')}</button
                  >
                  <button
                    class="btn quiet icon"
                    disabled={editLock !== null}
                    title={editLock ?? t('guided.plan.removeNamed', { name })}
                    aria-label={t('guided.plan.removeNamed', { name })}
                    onclick={() => (removing = name)}><Icon name="trash" size={18} /></button
                  >
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
        {#if saveError}<ErrorNotice error={saveError} />{/if}
        <div class="row">
          {#if !takenOver && plan}
            <button
              class="btn primary"
              disabled={editLock !== null}
              title={editLock ?? undefined}
              onclick={() => {
                startNewPart();
                step = 'part';
              }}><Icon name="plus" size={16} /> {t('guided.plan.add')}</button
            >
            <button class="btn" onclick={() => ((changes = []), (step = 'wiring'))}
              >{t('guided.plan.wiring')}</button
            >
            <button class="btn" onclick={() => (step = 'upload')}>{t('guided.plan.upload')}</button>
          {/if}
          <button class="btn quiet" onclick={() => board && openInArduino(desktop, board.sketch)}
            ><Icon name="code" size={16} /> {t('guided.upload.openSketch')}</button
          >
        </div>
      </section>
    {:else if step === 'part' && plan}
      {#if !editing}
        <fieldset class="step">
          <legend>{t('guided.part.legend', { board: plan.name })}</legend>
          {#if templates === null}
            <p class="muted">{t('guided.loading')}</p>
          {/if}
          {#each groups as g (g.group)}
            <div class="group">
              <p class="group-title">{g.title}</p>
              <div class="cards">
                {#each g.templates as tpl (tpl.id)}
                  <label class="card" class:on={templateId === tpl.id}>
                    <input
                      type="radio"
                      name="guided-template"
                      value={tpl.id}
                      checked={templateId === tpl.id}
                      onchange={() => pickTemplate(tpl.id)}
                    />
                    <span class="card-icon" aria-hidden="true"
                      ><Icon name={ICONS[partGroup(tpl.capability)]} size={22} /></span
                    >
                    <span class="card-body">
                      <span class="choice-title">{tpl.name}</span>
                      <span class="help">{tpl.description}</span>
                      {#if tpl.covers.length > 0}
                        <span class="covers">{tpl.covers.join(', ')}</span>
                      {/if}
                    </span>
                  </label>
                {/each}
              </div>
            </div>
          {/each}
        </fieldset>
      {:else if template}
        <p class="board-head">
          <b><code>{editing}</code></b> <span class="muted">{template.name}</span>
        </p>
      {/if}

      {#if template}
        <div class="details" bind:this={details}></div>
        {#if template.shapes.length > 1}
          <fieldset class="step">
            <legend>{t('guided.shape.legend')}</legend>
            <div class="choices">
              {#each template.shapes as s (s.id)}
                <label class="choice">
                  <input
                    type="radio"
                    name="guided-shape"
                    value={s.id}
                    checked={shape?.id === s.id}
                    onchange={() => (shapeId = s.id)}
                  />
                  <span class="choice-body">
                    <span class="choice-title">{s.name}</span>
                    {#if s.description}<span class="help">{s.description}</span>{/if}
                    <span class="choice-meta"
                      >{s.roles
                        .map((r) =>
                          r.optional ? t('guided.shape.optionalRole', { role: r.name }) : r.name,
                        )
                        .join(' · ')}</span
                    >
                  </span>
                </label>
              {/each}
            </div>
          </fieldset>
        {/if}

        {#if !editing}
          <fieldset class="step">
            <legend>{t('guided.name.legend')}</legend>
            <div class="field">
              <label class="label" for="guided-part-name">{t('addPart.name.label')}</label>
              <input
                id="guided-part-name"
                class="input mono name"
                bind:value={partName}
                oninput={() => {
                  partNameTouched = true;
                  nameError = null;
                }}
                aria-invalid={partProblem !== null}
                aria-describedby="guided-part-name-help"
                autocomplete="off"
                autocapitalize="off"
                spellcheck="false"
              />
              <p id="guided-part-name-help" class="help" class:bad={partProblem !== null}>
                {partProblem ?? t('guided.name.help', { board: plan.name, name: partName || '…' })}
              </p>
            </div>
          </fieldset>
        {/if}

        {#if template.settings.length > 0}
          <fieldset class="step">
            <legend>{t('guided.settings.legend')}</legend>
            <PartSettingsForm {template} bind:values={settings} />
          </fieldset>
        {/if}
        {#if template.warnings.length > 0}
          <ul class="notes">
            {#each template.warnings as w (w)}<li>{w}</li>{/each}
          </ul>
        {/if}
      {/if}
    {:else if step === 'pins' && suggestion && map}
      <PinPicker
        {map}
        roles={suggestion.roles}
        {pins}
        {firstPick}
        help={Object.fromEntries((shape?.roles ?? []).map((r) => [r.id, r.help]))}
        bind:current={currentRole}
        busy={suggesting || working}
        onpick={pick}
        onclear={clear}
      />
      {#if pinProblems.length > 0}
        {@render problemList(pinProblems, t('guided.pins.problems'))}
      {/if}
    {:else if step === 'wiring' && board}
      <WiringChecklist {board} templates={templates ?? []} {changes} onpopout={popout} />
    {:else if step === 'upload' && board}
      <GuidedUpload {board} {onshow} />
    {/if}

    {#if error}<ErrorNotice {error} />{/if}
  </div>

  {#if saveError && step === 'pins'}
    <!-- Next to the button that caused it: what the core refused, problem by problem. -->
    <div class="answer">
      {#if saveError.problems && saveError.problems.length > 0}
        <p class="refused">{saveError.message}</p>
        {@render problemList(saveError.problems, t('guided.save.problems'))}
      {:else}
        <ErrorNotice error={saveError}>
          {#snippet actions()}
            {#if saveError?.code === 'sketch-edited'}
              <button class="btn danger" onclick={() => savePart(true)}
                >{t('guided.save.overwrite')}</button
              >
              <button class="btn" onclick={() => board && openInArduino(desktop, board.sketch)}
                >{t('guided.upload.openSketch')}</button
              >
            {/if}
          {/snippet}
        </ErrorNotice>
      {/if}
    </div>
  {/if}

  {#if step !== 'plan'}
    <div class="footer">
      <p class="muted small">
        {#if step === 'part' && !template}{t(
            'guided.footer.pickPart',
          )}{:else if step === 'pins' && missingRoles.length > 0}{t('guided.footer.missing', {
            roles: missingRoles.join(', '),
          })}{:else if step === 'pins'}{t('guided.footer.save')}{/if}
      </p>
      {#if step === 'board'}
        <button
          class="btn primary"
          disabled={working ||
            plans === null ||
            (choice === NEW && editLock !== null) ||
            (choice === NEW && newNameTouched && newNameProblem !== null)}
          title={choice === NEW ? (editLock ?? undefined) : undefined}
          onclick={boardNext}>{t('guided.next')}</button
        >
      {:else if step === 'part'}
        <button
          class="btn primary"
          disabled={!template || suggesting || partProblem !== null || !settingsOk}
          onclick={toPins}>{t('guided.part.next')}</button
        >
      {:else if step === 'pins'}
        <button
          class="btn primary"
          disabled={working || suggesting || missingRoles.length > 0 || editLock !== null}
          title={editLock ?? undefined}
          onclick={() => savePart()}>{working ? t('guided.saving') : t('guided.pins.next')}</button
        >
      {:else if step === 'wiring'}
        <button class="btn primary" onclick={() => (step = 'upload')}
          >{t('guided.wiring.next')}</button
        >
      {:else if step === 'upload' && editFlow}
        <button class="btn" onclick={() => (step = 'plan')}>{t('guided.upload.done')}</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .guided {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    background: var(--surface);
  }
  .toolbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--line);
    background: var(--surface-2);
  }
  .toolbar h3 {
    font-size: var(--text-md);
    font-weight: 650;
  }
  .spacer {
    flex: 1;
  }
  .steps {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-3);
    margin: 0 0 0 var(--space-3);
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .steps li {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
  }
  .steps .n {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--surface-3);
    font-size: var(--text-xs);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .steps .done .n {
    background: var(--accent-soft);
    color: var(--accent-ink);
  }
  .steps .now {
    color: var(--text);
    font-weight: 650;
  }
  .steps .now .n {
    background: var(--accent);
    color: var(--accent-text);
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: grid;
    align-content: start;
    gap: var(--space-5);
    padding: var(--space-4) var(--space-4) var(--space-5);
  }
  .step {
    margin: 0;
    padding: 0;
    border: 0;
    min-width: 0;
    display: grid;
    gap: var(--space-3);
  }
  legend {
    margin-bottom: var(--space-3);
    padding: 0;
    font-weight: 650;
  }
  .choices {
    display: grid;
    gap: var(--space-2);
  }
  .choice,
  .card {
    position: relative;
    display: flex;
    gap: var(--space-3);
    align-items: flex-start;
    padding: var(--space-3);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--surface);
    cursor: pointer;
  }
  .choice:hover:not(.disabled),
  .card:hover {
    background: var(--surface-2);
  }
  .choice:has(input:checked),
  .card:has(input:checked) {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .choice:has(input:focus-visible),
  .card:has(input:focus-visible) {
    outline: 2px solid var(--focus-ring);
    outline-offset: 1px;
  }
  .choice.disabled {
    cursor: not-allowed;
    background: var(--surface-2);
  }
  .choice input {
    margin: 3px 0 0;
    accent-color: var(--accent);
    width: 18px;
    height: 18px;
    flex: none;
  }
  .card input {
    position: absolute;
    opacity: 0;
    inset: 0;
    margin: 0;
    cursor: pointer;
  }
  .choice-body,
  .card-body {
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .choice-title {
    font-weight: 640;
  }
  .choice-meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-3);
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  /* A scroll anchor: no room of its own in the step's grid. */
  .details {
    margin-bottom: calc(-1 * var(--space-5));
  }
  .group {
    display: grid;
    gap: var(--space-2);
  }
  .group + .group {
    margin-top: var(--space-3);
  }
  .group-title {
    font-size: var(--text-xs);
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(16rem, 100%), 1fr));
    gap: var(--space-2);
  }
  .card-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-md);
    background: var(--surface-3);
    color: var(--text-muted);
  }
  .card.on .card-icon {
    background: var(--accent);
    color: var(--accent-text);
  }
  .covers {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .new-board {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(14rem, 100%), 1fr));
    gap: var(--space-2) var(--space-3);
    margin-top: var(--space-2);
  }
  .new-board .wide {
    grid-column: 1 / -1;
  }
  .input.name {
    max-width: 22rem;
  }
  .help {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .help.bad {
    color: var(--danger-text);
    font-weight: 560;
  }
  .muted {
    color: var(--text-muted);
  }
  .small {
    font-size: var(--text-sm);
  }
  .board-head {
    display: flex;
    gap: var(--space-2);
    align-items: baseline;
    font-size: var(--text-lg);
  }
  .planned {
    margin: 0;
    padding: 0;
    list-style: none;
    border-top: 1px solid var(--line);
  }
  .planned li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--line);
  }
  .planned-main {
    flex: 1 1 14rem;
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .planned-name {
    font-weight: 620;
  }
  .planned-pins {
    font-family: var(--font-data);
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .confirm {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .notes {
    margin: 0;
    padding-left: var(--space-5);
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .problems {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .problems li {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    font-size: var(--text-sm);
  }
  .problems li.error {
    background: var(--danger-soft);
  }
  .problems li.error :global(svg) {
    color: var(--danger-text);
    flex: none;
  }
  .problems li.warning {
    background: var(--warn-soft);
  }
  .problems li.warning :global(svg) {
    color: var(--warn-text);
    flex: none;
  }
  .where {
    font-family: var(--font-data);
    font-weight: 700;
    margin-right: var(--space-2);
  }
  .msg {
    font-weight: 560;
  }
  .hint {
    display: block;
    color: var(--text-muted);
  }
  .refused {
    font-weight: 650;
    color: var(--danger-text);
    margin-bottom: var(--space-2);
  }
  .answer {
    padding: var(--space-3) var(--space-4) 0;
    border-top: 1px solid var(--line);
    max-height: 40%;
    overflow-y: auto;
  }
  .footer {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-top: 1px solid var(--line);
    background: var(--surface-2);
  }
  .footer p {
    flex: 1;
    min-width: 0;
  }
</style>
