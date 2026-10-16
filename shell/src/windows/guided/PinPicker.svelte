<script lang="ts">
  import { tick } from 'svelte';
  import { t } from '../../lib/i18n';
  import { layoutBoard, stepPin, type DrawnPosition } from '../../lib/desktop/boardDrawing';
  import { pickable, pinLabel, pinLook, type PinKind } from '../../lib/state/guided';
  import type { PinMap, RoleOptions } from '../../lib/state/types';

  /**
   * The pin picker (docs/wiring.md §3, step 3): a drawing of the board's headers that Faber
   * makes from the pin map, every pin coloured for the role being filled, and the part's roles
   * beside it. Clicking a pin (or Enter on it) puts it on the role; the arrow keys move along the
   * headers. Every pin is a button with its state in its name ("D7, suitable for Echo").
   */
  let {
    map,
    roles,
    pins,
    firstPick,
    help = {},
    current = $bindable(),
    busy = false,
    onpick,
    onclear,
  }: {
    map: PinMap;
    roles: RoleOptions[];
    /** This part's pins now, role → pin. */
    pins: Record<string, string>;
    /** Faber's first suggestion per role. */
    firstPick: Record<string, string>;
    /** The roles' help, from the template. */
    help?: Record<string, string>;
    /** The role being filled. */
    current: string;
    busy?: boolean;
    onpick: (role: string, pin: string) => void;
    onclear: (role: string) => void;
  } = $props();

  const pinNames = $derived(new Set(map.pins.map((p) => p.name)));
  const drawing = $derived(layoutBoard(map.headers, (name) => pinNames.has(name)));
  const role = $derived(roles.find((r) => r.role === current) ?? roles[0]);
  /** The pin with the keyboard's focus (the drawing's one tab stop). */
  let focusPin = $state('');
  /** What the pin under the pointer or the keyboard is, written out. */
  let info = $state('');
  const refs: Record<string, SVGGElement | undefined> = $state({});

  const rolesOn = $derived(
    Object.fromEntries(
      Object.entries(pins).map(([r, pin]) => [pin, roles.find((x) => x.role === r)?.name ?? r]),
    ) as Record<string, string>,
  );

  function look(pin: string) {
    return role
      ? pinLook(role, pin, pins, firstPick, roles)
      : { kind: 'unsuitable' as PinKind, reason: '' };
  }

  // The tab stop follows the role: its pin, else Faber's pick, else the first pin.
  $effect(() => {
    if (!role) return;
    const want = pins[role.role] ?? role.suggested ?? drawing.order[0] ?? '';
    focusPin = want;
  });

  function label(pin: string): string {
    return role ? pinLabel(pin, look(pin), role.name) : pin;
  }

  function choose(pin: string) {
    if (!role || busy) return;
    const l = look(pin);
    if (!pickable(l.kind)) {
      info = label(pin);
      return;
    }
    focusPin = pin;
    if (l.kind !== 'chosen') onpick(role.role, pin);
  }

  async function keydown(e: KeyboardEvent, pin: string) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      choose(pin);
      return;
    }
    const next = stepPin(drawing.order, pin, e.key);
    if (next === null) return;
    e.preventDefault();
    focusPin = next;
    await tick();
    refs[next]?.focus();
  }

  function labelAt(p: DrawnPosition) {
    const gap = 0.62;
    switch (p.side) {
      case 'below':
        return { x: p.x, y: p.y + gap, rotate: true, anchor: 'end' };
      case 'above':
        return { x: p.x, y: p.y - gap, rotate: true, anchor: 'start' };
      case 'left':
        return { x: p.x - gap, y: p.y, rotate: false, anchor: 'end' };
      case 'right':
        return { x: p.x + gap, y: p.y, rotate: false, anchor: 'start' };
    }
  }

  const LEGEND: PinKind[] = [
    'chosen',
    'suggested',
    'suitable',
    'other-role',
    'in-use',
    'reserved',
    'unsuitable',
  ];
  const shownKinds = $derived(new Set(drawing.order.map((pin) => look(pin).kind)) as Set<PinKind>);
</script>

{#snippet glyph(kind: PinKind | 'label', x: number, y: number)}
  {#if kind === 'chosen'}
    <circle class="g-chosen-ring" cx={x} cy={y} r="0.44" />
    <circle class="g-chosen" cx={x} cy={y} r="0.3" />
  {:else if kind === 'other-role'}
    <circle class="g-other" cx={x} cy={y} r="0.32" />
  {:else if kind === 'suggested'}
    <circle class="g-suggested" cx={x} cy={y} r="0.34" />
    <circle class="g-chosen" cx={x} cy={y} r="0.12" />
  {:else if kind === 'suitable'}
    <circle class="g-suitable" cx={x} cy={y} r="0.22" />
  {:else if kind === 'in-use'}
    <rect class="g-in-use" x={x - 0.24} y={y - 0.24} width="0.48" height="0.48" rx="0.06" />
  {:else if kind === 'reserved'}
    <path class="g-reserved" d="M{x - 0.2} {y - 0.2}l0.4 0.4M{x + 0.2} {y - 0.2}l-0.4 0.4" />
  {:else if kind === 'unsuitable'}
    <circle class="g-unsuitable" cx={x} cy={y} r="0.14" />
  {:else}
    <rect class="g-hole" x={x - 0.13} y={y - 0.13} width="0.26" height="0.26" />
  {/if}
{/snippet}

<div class="picker">
  <div class="board-side">
    {#if role}
      <p class="prompt" aria-live="polite">
        {#if pins[role.role]}
          {t('guided.pins.prompt.chosen', { role: role.name, pin: pins[role.role] ?? '' })}
        {:else}
          {t('guided.pins.prompt.pick', { role: role.name })}
        {/if}
      </p>
    {/if}
    <svg
      class="board"
      viewBox="{drawing.view.x} {drawing.view.y} {drawing.view.width} {drawing.view.height}"
      role="group"
      aria-label={t('guided.pins.drawing', { board: map.name })}
      class:busy
    >
      <rect class="pcb" x="0" y="0" width={drawing.width} height={drawing.height} rx="0.6" />
      <rect
        class="usb"
        x={drawing.usb.x}
        y={drawing.usb.y}
        width={drawing.usb.width}
        height={drawing.usb.height}
        rx="0.2"
      />
      <text
        class="usb-text"
        x={drawing.usb.x + drawing.usb.width / 2}
        y={drawing.usb.y + drawing.usb.height / 2}
        aria-hidden="true">{t('guided.pins.usb')}</text
      >
      <text
        class="board-name"
        x={drawing.middle.x + drawing.middle.width / 2}
        y={drawing.middle.y + drawing.middle.height / 2}
        aria-hidden="true">{map.name}</text
      >
      {#each drawing.positions as p, i (i)}
        <rect class="socket" x={p.x - 0.5} y={p.y - 0.5} width="1" height="1" />
      {/each}
      {#each drawing.positions as p, i (i)}
        {@const at = labelAt(p)}
        {@const on = p.pin ? rolesOn[p.name] : undefined}
        <text
          class="pin-name"
          class:on={on !== undefined}
          class:label={!p.pin}
          x={at.x}
          y={at.y}
          text-anchor={at.anchor}
          transform={at.rotate ? `rotate(-90 ${at.x} ${at.y})` : undefined}
          aria-hidden="true">{on ? `${p.name} ${on}` : p.name}</text
        >
      {/each}
      {#each drawing.positions as p, i (i)}
        {#if p.pin}
          {@const l = look(p.name)}
          <g
            bind:this={refs[p.name]}
            class="pin"
            data-pin={p.name}
            data-kind={l.kind}
            role="button"
            tabindex={focusPin === p.name ? 0 : -1}
            aria-label={label(p.name)}
            aria-disabled={!pickable(l.kind) || busy}
            aria-pressed={l.kind === 'chosen'}
            onclick={() => choose(p.name)}
            onkeydown={(e) => keydown(e, p.name)}
            onfocus={() => {
              focusPin = p.name;
              info = label(p.name);
            }}
            onmouseenter={() => (info = label(p.name))}
          >
            <title>{label(p.name)}</title>
            <rect class="hit" x={p.x - 0.5} y={p.y - 0.5} width="1" height="1" />
            {@render glyph(l.kind, p.x, p.y)}
            <circle class="focus" cx={p.x} cy={p.y} r="0.56" />
          </g>
        {:else}
          <g aria-hidden="true">{@render glyph('label', p.x, p.y)}</g>
        {/if}
      {/each}
    </svg>
  </div>

  <div class="side">
    <div class="roles" role="group" aria-label={t('guided.pins.roles')}>
      {#each roles as r (r.role)}
        {@const pin = pins[r.role]}
        <div class="role" class:current={r.role === role?.role}>
          <button
            class="role-btn"
            aria-pressed={r.role === role?.role}
            onclick={() => (current = r.role)}
          >
            <span class="role-top">
              <span class="role-name">{r.name}</span>
              {#if r.optional}<span class="tag">{t('guided.pins.optional')}</span>{/if}
              <span class="role-pin" class:none={!pin}
                >{pin ?? (r.optional ? t('guided.pins.unconnected') : t('guided.pins.none'))}</span
              >
            </span>
            {#if help[r.role]}<span class="role-help">{help[r.role]}</span>{/if}
          </button>
          {#if r.optional && pin}
            <button
              class="btn quiet small"
              onclick={() => onclear(r.role)}
              aria-label={t('guided.pins.clearNamed', { role: r.name })}
              >{t('guided.pins.clear')}</button
            >
          {/if}
        </div>
      {/each}
    </div>
    <p class="info" class:empty={!info}>{info || t('guided.pins.hover')}</p>
    <ul class="legend" aria-label={t('guided.pins.legend')}>
      {#each LEGEND.filter((k) => shownKinds.has(k)) as k (k)}
        <li>
          <svg viewBox="-0.6 -0.6 1.2 1.2" aria-hidden="true" class="swatch">
            <rect class="socket" x="-0.5" y="-0.5" width="1" height="1" />
            {@render glyph(k, 0, 0)}
          </svg>
          {t(`guided.pins.kind.${k}`)}
        </li>
      {/each}
    </ul>
  </div>
</div>

<style>
  .picker {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-4);
    align-items: flex-start;
  }
  .board-side {
    flex: 3 1 22rem;
    min-width: 0;
    display: grid;
    gap: var(--space-2);
  }
  .prompt {
    font-weight: 650;
  }
  .board {
    width: 100%;
    height: auto;
    max-height: 62vh;
    display: block;
    font-family: var(--font-data);
    user-select: none;
  }
  .board.busy {
    opacity: 0.75;
  }
  .pcb {
    fill: var(--surface-sunken);
    stroke: var(--line-strong);
    stroke-width: 0.08;
  }
  .usb {
    fill: var(--surface-3);
    stroke: var(--line-strong);
    stroke-width: 0.08;
  }
  .usb-text {
    font-size: 0.55px;
    fill: var(--text-muted);
    text-anchor: middle;
    dominant-baseline: central;
  }
  .board-name {
    font-family: var(--font-ui);
    font-size: 0.85px;
    font-weight: 650;
    fill: var(--text-faint);
    text-anchor: middle;
    dominant-baseline: central;
  }
  .socket {
    fill: var(--surface-3);
    stroke: var(--line-strong);
    stroke-width: 0.05;
  }
  .pin-name {
    font-size: 0.5px;
    fill: var(--text-muted);
    dominant-baseline: central;
  }
  .pin-name.label {
    fill: var(--text-faint);
  }
  .pin-name.on {
    fill: var(--accent-ink);
    font-weight: 700;
  }
  .pin {
    cursor: pointer;
    outline: none;
  }
  .pin[aria-disabled='true'] {
    cursor: not-allowed;
  }
  .hit {
    fill: transparent;
  }
  .pin:hover .hit {
    fill: var(--hover);
  }
  .focus {
    fill: none;
    stroke: var(--focus-ring);
    stroke-width: 0.12;
    opacity: 0;
  }
  .pin:focus-visible .focus {
    opacity: 1;
  }
  .g-chosen {
    fill: var(--accent);
  }
  .g-chosen-ring {
    fill: none;
    stroke: var(--accent);
    stroke-width: 0.09;
  }
  .g-other {
    fill: var(--accent-soft);
    stroke: var(--accent);
    stroke-width: 0.09;
  }
  .g-suggested {
    fill: none;
    stroke: var(--accent);
    stroke-width: 0.08;
    stroke-dasharray: 0.16 0.1;
  }
  .g-suitable {
    fill: var(--ok);
  }
  .g-in-use {
    fill: var(--warn);
  }
  .g-reserved {
    stroke: var(--danger);
    stroke-width: 0.1;
    stroke-linecap: round;
  }
  .g-unsuitable {
    fill: none;
    stroke: var(--text-faint);
    stroke-width: 0.06;
  }
  .g-hole {
    fill: var(--text-faint);
  }
  .info {
    min-height: 3.2em;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    background: var(--surface-2);
    font-size: var(--text-sm);
    user-select: text;
  }
  .info.empty {
    color: var(--text-muted);
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-4);
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .legend li {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }
  .swatch {
    width: 16px;
    height: 16px;
  }
  .side {
    flex: 1 1 13rem;
    min-width: 0;
    display: grid;
    gap: var(--space-3);
  }
  .roles {
    display: grid;
    gap: var(--space-2);
  }
  .role {
    display: grid;
    gap: var(--space-1);
    justify-items: start;
  }
  .role-btn {
    width: 100%;
    display: grid;
    gap: 2px;
    text-align: left;
    padding: var(--space-2) var(--space-3);
    min-height: var(--touch);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--surface);
  }
  .role-btn:hover {
    background: var(--surface-2);
  }
  .role.current .role-btn {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .role-top {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
  }
  .role-name {
    font-weight: 650;
  }
  .role-pin {
    margin-left: auto;
    font-family: var(--font-data);
    font-weight: 650;
    color: var(--accent-ink);
  }
  .role-pin.none {
    font-family: var(--font-ui);
    font-weight: 500;
    color: var(--text-muted);
  }
  .role-help {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .tag {
    padding: 0 6px;
    border-radius: var(--radius-sm);
    background: var(--neutral-soft);
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--text-muted);
  }
  .btn.small {
    min-height: 0;
    padding: 2px var(--space-2);
    font-size: var(--text-sm);
  }
</style>
