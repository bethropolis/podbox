// Coordination between the global Cmd+K palette (docs) and pages that need
// that shortcut themselves.
//
// The palette is rendered by the site layout on every page, so a page with
// its own palette (Studio's settings search) must be able to claim the
// shortcut. Pages mark themselves with `data-page-palette`; the global
// palette stands down whenever that marker is on screen. No imports, no
// ordering assumptions between the two window listeners.

declare global {
  interface Window {
    /** Set by the global palette so a page can ask who owns Cmd+K. */
    __podboxPaletteOwner?: 'global';
  }
}

/** True when a page with its own palette is mounted. */
export function isPagePaletteActive(): boolean {
  if (typeof document === 'undefined') return false;
  return document.querySelector('[data-page-palette]') !== null;
}
