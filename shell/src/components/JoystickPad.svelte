<script lang="ts">
  import { t } from '../lib/i18n';
  import { padToStick, STILL, type Stick } from '../lib/desktop/driveInput';

  /**
   * The on-screen joystick: drag from the middle to drive, let go to stop. Works with a mouse, a
   * pen and touch (pointer events, one pointer at a time). Screen up is forward.
   */
  let {
    holonomic,
    disabled = false,
    onchange,
  }: {
    holonomic: boolean;
    disabled?: boolean;
    onchange: (stick: Stick) => void;
  } = $props();

  let pad: HTMLDivElement | undefined = $state();
  const helpId = `drive-pad-help-${Math.random().toString(36).slice(2)}`;
  let pointer: number | null = null;
  /** The knob, as a fraction of the radius from the middle. */
  let knob = $state({ x: 0, y: 0 });

  function at(e: PointerEvent) {
    if (!pad) return;
    const r = pad.getBoundingClientRect();
    const radius = Math.min(r.width, r.height) / 2 || 1;
    let x = (e.clientX - (r.left + r.width / 2)) / radius;
    let y = (e.clientY - (r.top + r.height / 2)) / radius;
    const len = Math.hypot(x, y);
    if (len > 1) {
      x /= len;
      y /= len;
    }
    knob = { x, y };
    onchange(padToStick(x, y, holonomic));
  }

  function down(e: PointerEvent) {
    if (disabled || pointer !== null) return;
    pointer = e.pointerId;
    pad?.setPointerCapture?.(e.pointerId);
    e.preventDefault();
    at(e);
  }

  function move(e: PointerEvent) {
    if (e.pointerId === pointer) at(e);
  }

  function up(e: PointerEvent) {
    if (e.pointerId !== pointer) return;
    pointer = null;
    knob = { x: 0, y: 0 };
    onchange(STILL);
  }

  // Disabled while held (the e-stop, the window losing focus): let go at once.
  $effect(() => {
    if (!disabled || pointer === null) return;
    pointer = null;
    knob = { x: 0, y: 0 };
    onchange(STILL);
  });
</script>

<div
  bind:this={pad}
  class="pad"
  class:disabled
  role="group"
  aria-label={t('drive.pad.label')}
  aria-describedby={helpId}
  aria-disabled={disabled}
  data-testid="drive-pad"
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  onlostpointercapture={up}
>
  <span id={helpId} class="visually-hidden">{t('drive.pad.help')}</span>
  <span class="ring" aria-hidden="true"></span>
  <span class="cross" aria-hidden="true"></span>
  <span
    class="knob"
    aria-hidden="true"
    style:left="{50 + knob.x * 50}%"
    style:top="{50 + knob.y * 50}%"
  ></span>
</div>

<style>
  .pad {
    position: relative;
    width: 100%;
    max-width: 220px;
    aspect-ratio: 1;
    border-radius: 50%;
    background: var(--surface-2);
    box-shadow: inset 0 0 0 1px var(--line);
    touch-action: none;
    user-select: none;
    cursor: grab;
  }
  .pad:active {
    cursor: grabbing;
  }
  .pad.disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .ring {
    position: absolute;
    inset: 25%;
    border-radius: 50%;
    border: 1px dashed var(--line-strong);
  }
  .cross {
    position: absolute;
    inset: 0;
    background:
      linear-gradient(var(--line), var(--line)) center / 1px 100% no-repeat,
      linear-gradient(var(--line), var(--line)) center / 100% 1px no-repeat;
    border-radius: 50%;
  }
  .knob {
    position: absolute;
    width: 26%;
    aspect-ratio: 1;
    border-radius: 50%;
    transform: translate(-50%, -50%);
    background: var(--accent);
    box-shadow: var(--shadow-pop);
    pointer-events: none;
  }
</style>
