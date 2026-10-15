<script lang="ts">
  import Logo from './Logo.svelte';

  /** `size` is the cap height area in px; the nut is drawn a little taller than the letters. */
  let { size = 18, mark = true }: { size?: number; mark?: boolean } = $props();
</script>

<span class="wordmark" style:font-size="{size}px" role="img" aria-label="Faber">
  {#if mark}<span class="nut"><Logo size={Math.round(size * 1.25)} /></span>{/if}
  <span class="name" aria-hidden="true">Faber</span>
</span>

<style>
  .wordmark {
    display: inline-flex;
    align-items: center;
    gap: 0.38em;
    line-height: 1;
    color: var(--text);
  }
  .nut {
    color: var(--accent);
  }
  .name {
    font-weight: 700;
    letter-spacing: -0.01em;
    /* Optical alignment: the cap height, not the em box, should sit centred on the nut. */
    padding-top: 0.06em;
  }
</style>
