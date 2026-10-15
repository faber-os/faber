<script lang="ts">
  import type { AppRole, PartInfo } from '../lib/state/types';
  import { candidates, takesMany, type RolePicks } from '../lib/state/appRoles';
  import { partList } from '../lib/desktop/packages';
  import Icon from './Icon.svelte';
  import RichText from './RichText.svelte';
  import { t } from '../lib/i18n';

  /**
   * Picks the parts for an app's roles (docs/apps.md §1): per role, only the parts that provide its
   * capability; one part, or several for a `count = "1.."` role. A role the app may drive says so
   * in words before anything is installed or saved.
   */
  let {
    app,
    roles,
    parts,
    picks = $bindable(),
    idPrefix,
  }: {
    /** The app's name, for "<app> will be able to drive <part>". */
    app: string;
    roles: AppRole[];
    parts: PartInfo[];
    picks: RolePicks;
    /** Unique per window, for the labels' ids. */
    idPrefix: string;
  } = $props();

  function pickOne(role: string, part: string) {
    picks = { ...picks, [role]: part ? [part] : [] };
  }

  function toggle(role: string, part: string, on: boolean) {
    const now = picks[role] ?? [];
    picks = {
      ...picks,
      [role]: on ? [...now.filter((p) => p !== part), part] : now.filter((p) => p !== part),
    };
  }
</script>

<ul class="roles" aria-label={t('roles.title')}>
  {#each roles as role (role.role)}
    {@const fit = candidates(role, parts)}
    {@const picked = picks[role.role] ?? []}
    {@const id = `${idPrefix}-role-${role.role}`}
    <li class="role" data-role={role.role}>
      <div class="head">
        {#if takesMany(role) || fit.length === 0}
          <span class="name">{role.role}</span>
        {:else}
          <label class="name" for={id}>{role.role}</label>
        {/if}
        <span class="cap"><code>{role.capability}</code></span>
        <span class="count">{takesMany(role) ? t('roles.count.many') : t('roles.count.one')}</span>
      </div>
      {#if fit.length === 0}
        <p class="none">
          <RichText text={t('roles.noPart', { capability: role.capability })} />
        </p>
      {:else if takesMany(role)}
        <fieldset {id} class="many">
          <legend class="sr">{role.role}</legend>
          {#each fit as part (part)}
            <label class="check">
              <input
                type="checkbox"
                checked={picked.includes(part)}
                onchange={(e) => toggle(role.role, part, e.currentTarget.checked)}
              />
              {part}
            </label>
          {/each}
        </fieldset>
      {:else}
        <select
          {id}
          value={picked[0] ?? ''}
          onchange={(e) => pickOne(role.role, e.currentTarget.value)}
        >
          <option value="">{t('roles.pickLater')}</option>
          {#each fit as part (part)}
            <option value={part}>{part}</option>
          {/each}
        </select>
      {/if}
      {#if role.command}
        <p class="drives" role="note">
          <Icon name="drive" size={16} />
          <span>
            {#if picked.length > 0}
              <RichText text={t('roles.willDrive', { app, parts: partList(picked) })} />
            {:else}
              <RichText text={t('roles.willDrivePicked', { app })} />
            {/if}
          </span>
        </p>
      {/if}
    </li>
  {/each}
</ul>

<style>
  .roles {
    display: grid;
    gap: var(--space-3);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .role {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-2);
  }
  .name {
    font-weight: 700;
  }
  .cap,
  .count {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  select {
    max-width: 100%;
  }
  .many {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-4);
    margin: 0;
    padding: 0;
    border: 0;
  }
  .check {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
  }
  .none {
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .drives {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--warn-soft);
    font-size: var(--text-sm);
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
</style>
