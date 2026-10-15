<script lang="ts">
  import Icon, { type IconName } from './Icon.svelte';

  /**
   * A program's icon: its glyph in the program's own colour on a plate (white in light mode, dark
   * in dark mode). Used on the desktop, in the start menu and in the taskbar.
   */
  let {
    icon,
    colour,
    size = 52,
    flat = false,
  }: {
    icon: IconName;
    /** `--app-<colour>`, e.g. `files`. */
    colour: string;
    /** The plate, in px; the glyph is about half of it. */
    size?: number;
    /** No plate, just the coloured glyph (the small taskbar). */
    flat?: boolean;
  } = $props();
</script>

<span
  class="app-icon"
  class:flat
  style:--app="var(--app-{colour})"
  style:width="{size}px"
  style:height="{size}px"
  style:border-radius="{Math.round(size * 0.27)}px"
>
  <Icon name={icon} size={flat ? size : Math.round(size * 0.52)} />
</span>

<style>
  .app-icon {
    display: inline-grid;
    place-items: center;
    flex: none;
    color: var(--app);
    background: var(--plate);
    box-shadow:
      0 0 0 1px oklch(0.3 0.01 250 / 0.13),
      0 2px 5px -2px oklch(0.2 0.01 250 / 0.28);
  }
  :global([data-theme='dark']) .app-icon {
    box-shadow:
      0 0 0 1px oklch(1 0 0 / 0.07),
      0 2px 6px -2px oklch(0 0 0 / 0.6);
  }
  .app-icon.flat {
    background: none;
    box-shadow: none !important;
  }
</style>
