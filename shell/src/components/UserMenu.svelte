<script lang="ts">
  import { t } from '../lib/i18n';
  import { item, separator } from '../lib/desktop/contextMenu';
  import { useCore } from '../lib/state/context';
  import { roleName } from '../lib/state/session.svelte';
  import { useDesktop } from './desktopController.svelte';
  import Icon from './Icon.svelte';
  import { useMenu } from './menuController.svelte';

  /**
   * Who is using this shell, on the taskbar (docs/auth.md): their name and role, Settings →
   * Users, and Sign out. The attached screen, which works without signing in, offers Sign in
   * (and Create the admin on a fresh machine) instead.
   */
  let { large = false }: { large?: boolean } = $props();

  const core = useCore();
  const desktop = useDesktop();
  const menu = useMenu();

  const identity = $derived(core.auth.info?.identity ?? null);
  const who = $derived(
    identity?.user ?? (identity?.via === 'screen' ? t('auth.screen') : t('auth.developer')),
  );
  const role = $derived(identity ? roleName(identity.role) : '');

  function open(e: MouseEvent) {
    if (!identity) return;
    const button = e.currentTarget as HTMLElement;
    const box = button.getBoundingClientRect();
    const session = identity.via === 'session';
    const screen = identity.via === 'screen';
    menu.show(
      {
        label: t('auth.menu.label'),
        header: { title: who, detail: role, icon: 'user' },
        entries: [
          item('account', t('auth.menu.account'), { icon: 'key' }),
          separator(),
          screen && item('sign-in', t('auth.menu.signIn'), { icon: 'user' }),
          screen &&
            core.auth.info?.needsSetup === true &&
            item('setup', t('auth.menu.setup'), { icon: 'plus' }),
          session && item('sign-out', t('auth.menu.signOut'), { icon: 'power' }),
        ],
        onSelect: (id) => {
          if (id === 'account') desktop.open('settings', { params: { section: 'users' } });
          else if (id === 'sign-in') core.auth.prompt = 'sign-in';
          else if (id === 'setup') core.auth.prompt = 'setup';
          else if (id === 'sign-out') void core.auth.signOutAndForget();
        },
      },
      { x: box.right, y: box.top },
      button,
      e.detail === 0,
    );
  }
</script>

{#if identity}
  <button
    class="user"
    class:large
    aria-haspopup="menu"
    aria-label={t('auth.menu.button', { name: who, role })}
    title={t('auth.menu.button', { name: who, role })}
    onclick={open}
  >
    <Icon name="user" size={large ? 22 : 18} />
    {#if large}<span class="name">{who}</span>{/if}
  </button>
{/if}

<style>
  .user {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    height: 36px;
    min-width: 36px;
    justify-content: center;
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text);
  }
  .user:hover {
    background: var(--hover);
  }
  .user.large {
    height: 48px;
    padding: 0 var(--space-3);
  }
  .name {
    max-width: 10ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 650;
  }
</style>
