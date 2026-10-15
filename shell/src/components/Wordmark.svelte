<script lang="ts">
  import Logo from './Logo.svelte';

  /**
   * The Faber lockup: the nut in the accent colour, "Faber", and (with `os`) a small "OS" tag. Its
   * colours are tokens, so the same lockup works in light and dark mode. `size` is the letter
   * size in px; the nut is drawn a little taller than the letters.
   */
  let {
    size = 18,
    mark = true,
    os = false,
  }: { size?: number; mark?: boolean; os?: boolean } = $props();
</script>

<span class="wordmark" style:font-size="{size}px" role="img" aria-label={os ? 'Faber OS' : 'Faber'}>
  {#if mark}<span class="nut"><Logo size={Math.round(size * 1.15)} /></span>{/if}
  <span class="name" aria-hidden="true">Faber</span>
  {#if os}<span class="os" aria-hidden="true">OS</span>{/if}
</span>

<style>
  .wordmark {
    display: inline-flex;
    align-items: center;
    gap: 0.3em;
    line-height: 1;
    color: var(--text);
  }
  .nut {
    display: inline-flex;
    color: var(--accent);
  }
  .name {
    font-weight: 800;
    letter-spacing: -0.02em;
    /* Optical alignment: the cap height, not the em box, should sit centred on the nut. */
    padding-top: 0.06em;
  }
  .os {
    align-self: center;
    margin-left: 0.1em;
    padding: 0.14em 0.4em 0.1em;
    border: 1px solid var(--line-strong);
    border-radius: 0.38em;
    font-size: 0.34em;
    font-weight: 600;
    letter-spacing: 0.04em;
    color: var(--text-muted);
  }
</style>
