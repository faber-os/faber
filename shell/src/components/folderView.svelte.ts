import { untrack } from 'svelte';
import {
  back,
  canGoBack,
  canGoForward,
  forward,
  navigate,
  startHistory,
  type History,
} from '../lib/desktop/history';
import { isAffected, parentPath } from '../lib/desktop/filePaths';
import type { CoreState } from '../lib/state/core.svelte';
import type { ErrorBody, FolderListing } from '../lib/state/types';

/**
 * One folder being browsed, with back/forward history, kept fresh by the core's `files` topic.
 * Used by the Files window and by Notes' open and save dialogs.
 */
export class FolderView {
  history = $state.raw<History>(startHistory('home:/'));
  listing = $state.raw<FolderListing | null>(null);
  error = $state.raw<ErrorBody | null>(null);
  loading = $state(false);

  path = $derived(this.history.current);
  canBack = $derived(canGoBack(this.history));
  canForward = $derived(canGoForward(this.history));
  canUp = $derived(parentPath(this.history.current) !== null);

  private readonly core: CoreState;
  private request = 0;

  constructor(core: CoreState, start: string) {
    this.core = core;
    this.history = startHistory(start);
  }

  /** Start listing and following changes. Returns the function that stops following them. */
  start(): () => void {
    // Called from an effect: the first listing must not make that effect depend on it.
    untrack(() => void this.refresh());
    return this.core.watchFiles((folders) => {
      if (isAffected(this.path, folders)) void this.refresh(true);
    });
  }

  go(path: string): void {
    this.history = navigate(this.history, path);
    void this.refresh();
  }

  back(): void {
    this.history = back(this.history);
    void this.refresh();
  }

  forward(): void {
    this.history = forward(this.history);
    void this.refresh();
  }

  up(): void {
    const p = parentPath(this.path);
    if (p) this.go(p);
  }

  /** `quiet`: a background refresh keeps showing the old listing until the new one is here. */
  async refresh(quiet = false): Promise<void> {
    const id = ++this.request;
    const path = this.path;
    if (!quiet) {
      this.loading = true;
      if (this.listing?.path !== path) this.listing = null;
    }
    const r = await this.core.listFolder(path);
    if (id !== this.request) return; // An older answer, for a folder no longer shown.
    this.loading = false;
    if (r.ok) {
      this.listing = r.value;
      this.error = null;
    } else {
      this.error = r.error;
      if (!quiet) this.listing = null;
    }
  }
}
