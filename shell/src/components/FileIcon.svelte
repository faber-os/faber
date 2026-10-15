<script lang="ts">
  import type { FileType } from '../lib/desktop/fileTypes';
  import { iconPath, type IconName } from './Icon.svelte';

  /**
   * A file or folder's picture, drawn on a 48-unit grid in the plate style: a sheet of paper with
   * a mark saying what is on it, or an amber folder (with an emblem for Faber's fixed folders).
   * Colours are `--file-*` tokens, so both themes work.
   */
  let {
    type,
    size = 48,
    emblem,
  }: { type: FileType | 'disk'; size?: number; emblem?: IconName | undefined } = $props();

  const PAPER = 'M12 5h17l9 9v28a1 1 0 0 1-1 1H13a1 1 0 0 1-1-1z';
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 48 48"
  aria-hidden="true"
  focusable="false"
  class="file-icon"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  {#if type === 'folder'}
    <path
      d="M5 11a2 2 0 0 1 2-2h11l4 4h19a2 2 0 0 1 2 2v23a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2z"
      fill="var(--file-folder-back)"
      stroke="var(--file-folder-edge)"
      stroke-width="1.5"
    />
    <path
      d="M5 18.5h38V38a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2z"
      fill="var(--file-folder)"
      stroke="var(--file-folder-edge)"
      stroke-width="1.5"
    />
    {#if emblem}
      <svg x="17" y="22.5" width="14" height="14" viewBox="0 0 24 24">
        <path
          d={iconPath(emblem)}
          fill="none"
          stroke="var(--file-folder-edge)"
          stroke-width="2.4"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
    {/if}
  {:else if type === 'image'}
    <rect
      x="6"
      y="9"
      width="36"
      height="30"
      rx="2.5"
      fill="var(--file-paper)"
      stroke="var(--file-edge)"
      stroke-width="1.5"
    />
    <path d="M9 36l9.5-11 7 7.5 4.5-4.5 9 8z" fill="var(--file-image)" />
    <circle cx="32" cy="17" r="3.2" fill="var(--file-sun)" />
  {:else if type === 'disk'}
    <path
      d="M11 11h26l5 15H6z"
      fill="var(--file-paper)"
      stroke="var(--file-disk)"
      stroke-width="1.5"
    />
    <path
      d="M6 26h36v10a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2z"
      fill="var(--file-paper)"
      stroke="var(--file-disk)"
      stroke-width="1.5"
    />
    <circle cx="35" cy="32" r="1.8" fill="var(--file-recording)" />
    <path d="M12 32h10" stroke="var(--file-ink)" stroke-width="2" fill="none" />
  {:else}
    <path d={PAPER} fill="var(--file-paper)" stroke="var(--file-edge)" stroke-width="1.5" />
    <path d="M29 5v8a1 1 0 0 0 1 1h8" fill="none" stroke="var(--file-edge)" stroke-width="1.5" />
    {#if type === 'text'}
      <path d="M17 21h16M17 26.5h16M17 32h11" stroke="var(--file-ink)" stroke-width="2" />
    {:else if type === 'markdown'}
      <path d="M17 20.5h9" stroke="var(--file-config)" stroke-width="3" />
      <path d="M17 27h16M17 32.5h12" stroke="var(--file-ink)" stroke-width="2" />
    {:else if type === 'config'}
      <path d="M17 22h15M17 31h15" stroke="var(--file-ink)" stroke-width="2" />
      <circle
        cx="21.5"
        cy="22"
        r="3"
        fill="var(--file-paper)"
        stroke="var(--file-config)"
        stroke-width="2.2"
      />
      <circle
        cx="28.5"
        cy="31"
        r="3"
        fill="var(--file-paper)"
        stroke="var(--file-config)"
        stroke-width="2.2"
      />
    {:else if type === 'code'}
      <path
        d="M21 22l-4.5 4.5L21 31M28 22l4.5 4.5L28 31M26 20.5l-3 12"
        fill="none"
        stroke="var(--file-code)"
        stroke-width="2.2"
      />
    {:else if type === 'package'}
      <path
        d="M24 19l7.5 4v8.5L24 35.5l-7.5-4V23zM16.5 23l7.5 4 7.5-4M24 27v8.5"
        fill="none"
        stroke="var(--file-config)"
        stroke-width="2.2"
      />
    {:else if type === 'recording'}
      <path
        d="M15.5 28h3l2.2-6 3 12 3-15 3 11.5 2-2.5h3"
        fill="none"
        stroke="var(--file-recording)"
        stroke-width="2.2"
      />
    {/if}
  {/if}
</svg>

<style>
  .file-icon {
    display: block;
    flex: none;
  }
</style>
