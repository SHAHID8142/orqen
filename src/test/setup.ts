/**
 * Vitest (jsdom) global test setup.
 *
 * jsdom does not implement window.matchMedia; the shell uses it for the
 * narrow-width breakpoint, so provide a minimal always-non-matching stub.
 * Individual tests can override it with vi.spyOn(window, "matchMedia").
 */
if (typeof window !== "undefined" && typeof window.matchMedia !== "function") {
  window.matchMedia = ((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    addListener: () => undefined,
    removeListener: () => undefined,
    dispatchEvent: () => false,
  })) as unknown as typeof window.matchMedia;
}
