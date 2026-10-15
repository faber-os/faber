<script lang="ts" generics="T extends string">
  /** One setting with a few choices, as a segmented control: a name, a line about it, the choices. */
  let {
    id,
    label,
    description,
    options,
    value,
    disabled = false,
    onchange,
  }: {
    /** Unique in the page: names the radio group and labels it. */
    id: string;
    label: string;
    description: string;
    options: readonly (readonly [T, string])[];
    value: T;
    disabled?: boolean;
    onchange: (value: T) => void;
  } = $props();
</script>

<div class="row">
  <div class="text">
    <p class="name" id="{id}-label">{label}</p>
    <p class="desc">{description}</p>
  </div>
  <div class="segmented" role="radiogroup" aria-labelledby="{id}-label">
    {#each options as [v, text] (v)}
      <label
        ><input
          type="radio"
          name={id}
          value={v}
          checked={value === v}
          {disabled}
          onchange={() => onchange(v)}
        />{text}</label
      >
    {/each}
  </div>
</div>
