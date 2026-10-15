<script lang="ts" module>
  /*
   * The Faber mark (D31): a hex nut seen from above, with an F cut out where the bore would be.
   * Drawn on a 16-unit grid so the F's strokes land on whole pixels at 16 px (taskbar, favicon).
   * From 48 px up it also shows the nut's chamfer ring, which is too fine to read smaller.
   */
  let count = 0;
</script>

<script lang="ts">
  let { size = 24, title }: { size?: number; title?: string } = $props();
  const id = `faber-logo-${++count}`;
  const detail = $derived(size >= 48);
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 16 16"
  role={title ? 'img' : undefined}
  aria-label={title}
  aria-hidden={title ? undefined : 'true'}
  focusable="false"
  class="logo"
>
  <mask {id} maskUnits="userSpaceOnUse" x="0" y="0" width="16" height="16">
    <path
      d="M8 1.2 14 4.6v6.8L8 14.8 2 11.4V4.6Z"
      fill="#fff"
      stroke="#fff"
      stroke-width="1.2"
      stroke-linejoin="round"
    />
    {#if detail}
      <circle
        cx="8"
        cy="8"
        r="5.65"
        fill="none"
        stroke="#000"
        stroke-opacity="0.4"
        stroke-width="0.24"
      />
    {/if}
    <path d="M5 4h6v2H7v2h3v2H7v2H5Z" fill="#000" />
  </mask>
  <rect width="16" height="16" fill="currentColor" mask="url(#{id})" />
</svg>

<style>
  .logo {
    display: block;
    flex: none;
  }
</style>
