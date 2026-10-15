import { getContext, setContext } from 'svelte';
import {
  isMenuTrigger,
  normaliseMenu,
  type MenuEntry,
  type Point,
} from '../lib/desktop/contextMenu';
import { LongPress } from '../lib/desktop/longPress';

export interface MenuRequest {
  entries: readonly (MenuEntry | false)[];
  onSelect: (id: string) => void;
  /** Read by screen readers as the menu's name, e.g. "notes.txt". */
  label?: string;
}

export interface OpenMenu {
  entries: MenuEntry[];
  onSelect: (id: string) => void;
  label: string;
  at: Point;
  /** Gets focus back when the menu closes without choosing (as Windows does). */
  opener: HTMLElement | null;
  /** True when opened from the keyboard: the first item gets focus. */
  fromKeyboard: boolean;
}

/** The one context menu on the desktop: at most one is open at a time. */
export class MenuController {
  current = $state.raw<OpenMenu | null>(null);

  show(req: MenuRequest, at: Point, opener: HTMLElement | null, fromKeyboard = false): void {
    const entries = normaliseMenu(req.entries);
    if (entries.length === 0) return;
    this.current = {
      entries,
      onSelect: req.onSelect,
      label: req.label ?? 'Menu',
      at,
      opener,
      fromKeyboard,
    };
  }

  close(returnFocus: boolean): void {
    const m = this.current;
    this.current = null;
    if (returnFocus && m?.opener?.isConnected) m.opener.focus({ preventScroll: true });
  }

  /** Choose an item: the menu closes first, so whatever the item opens can take focus. */
  choose(id: string): void {
    const m = this.current;
    if (!m) return;
    this.current = null;
    m.onSelect(id);
  }
}

const KEY = Symbol('faber-menu');

export function provideMenu(m: MenuController): void {
  setContext(KEY, m);
}

export function useMenu(): MenuController {
  const m = getContext<MenuController | undefined>(KEY);
  if (!m) throw new Error('useMenu() needs provideMenu() in a parent component.');
  return m;
}

export const MENU_CONTEXT_KEY = KEY;

const claimed = new WeakSet<Event>();

export interface MenuTriggerParams {
  menu: MenuController;
  /** Builds the menu when it is asked for; null means no menu here (let it bubble). */
  build: (target: HTMLElement) => MenuRequest | null;
}

/**
 * `use:menuTrigger`: opens a context menu on right-click, Shift+F10, the ContextMenu key, or a
 * 500 ms long-press on touch. The innermost element with a menu wins.
 */
export function menuTrigger(node: HTMLElement, params: MenuTriggerParams) {
  let p = params;
  const press = new LongPress();
  let swallowClickUntil = 0;
  let keyboardOpenedAt = -Infinity;

  function open(target: HTMLElement, at: Point, fromKeyboard: boolean): boolean {
    const req = p.build(target);
    if (!req) return false;
    p.menu.show(req, at, document.activeElement as HTMLElement | null, fromKeyboard);
    return true;
  }

  function oncontextmenu(e: MouseEvent) {
    // A touch long-press may also make the browser send its own contextmenu: ours already opened.
    if (press.fired && performance.now() < swallowClickUntil) {
      e.preventDefault();
      return;
    }
    // The ContextMenu key sends keydown and then the browser's own contextmenu event.
    if (performance.now() - keyboardOpenedAt < 400) {
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    if (open(e.target as HTMLElement, { x: e.clientX, y: e.clientY }, false)) {
      e.preventDefault();
      e.stopPropagation();
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (!isMenuTrigger(e)) return;
    const target = e.target as HTMLElement;
    const r = target.getBoundingClientRect();
    if (open(target, { x: r.left + Math.min(24, r.width / 2), y: r.top + r.height / 2 }, true)) {
      keyboardOpenedAt = performance.now();
      e.preventDefault();
      e.stopPropagation();
    }
  }

  function onpointerdown(e: PointerEvent) {
    if (e.pointerType !== 'touch') return;
    // Only the innermost trigger listens to this finger; outer ones see it is taken.
    if (claimed.has(e)) return;
    claimed.add(e);
    const target = e.target as HTMLElement;
    press.start(e.clientX, e.clientY, (x, y) => {
      if (open(target, { x, y }, false)) swallowClickUntil = performance.now() + 800;
    });
  }

  const onpointermove = (e: PointerEvent) => press.move(e.clientX, e.clientY);
  const end = () => press.cancel();
  function onclick(e: MouseEvent) {
    // The finger lifting after a long-press must not also "click" what was under it.
    if (press.fired && performance.now() < swallowClickUntil) {
      e.preventDefault();
      e.stopPropagation();
    }
  }

  node.addEventListener('contextmenu', oncontextmenu);
  node.addEventListener('keydown', onkeydown);
  node.addEventListener('pointerdown', onpointerdown);
  node.addEventListener('pointermove', onpointermove);
  node.addEventListener('pointerup', end);
  node.addEventListener('pointercancel', end);
  node.addEventListener('click', onclick, true);
  return {
    update(next: MenuTriggerParams) {
      p = next;
    },
    destroy() {
      press.cancel();
      node.removeEventListener('contextmenu', oncontextmenu);
      node.removeEventListener('keydown', onkeydown);
      node.removeEventListener('pointerdown', onpointerdown);
      node.removeEventListener('pointermove', onpointermove);
      node.removeEventListener('pointerup', end);
      node.removeEventListener('pointercancel', end);
      node.removeEventListener('click', onclick, true);
    },
  };
}
