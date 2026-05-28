/**
 * PR-8 — React state binding for `window.matchMedia`. Subscribes via the
 * modern `addEventListener('change', ...)` API and tears down on unmount.
 * Returns `false` in environments without `matchMedia` (jsdom default
 * without the setup.ts shim).
 */
import { useEffect, useState } from 'react';

export function useMediaQuery(query: string): boolean {
  const get = () =>
    typeof window !== 'undefined' && typeof window.matchMedia === 'function'
      ? window.matchMedia(query).matches
      : false;

  const [matches, setMatches] = useState<boolean>(get);

  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return;
    const mql = window.matchMedia(query);
    const handler = (ev: MediaQueryListEvent) => setMatches(ev.matches);
    // Modern API. addEventListener is well-supported in evergreen browsers
    // (and our test shim implements it). Falling back to addListener for
    // older browsers is out of scope.
    mql.addEventListener('change', handler);
    setMatches(mql.matches);
    return () => mql.removeEventListener('change', handler);
  }, [query]);

  return matches;
}
