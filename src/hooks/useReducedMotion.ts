import { useEffect, useRef, useState } from "react";

/**
 * Reads the OS "reduce motion" preference and keeps it live.
 *
 * Framer Motion has its own `useReducedMotion`, but the CSS side needs the answer
 * too (the pointer highlight and the equaliser are driven by CSS), so this hook is
 * the single source of truth for both.
 */
export function useReducedMotion(): boolean {
  const [reduced, setReduced] = useState(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
      return false;
    }
    return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  });

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;

    const query = window.matchMedia("(prefers-reduced-motion: reduce)");
    const onChange = (event: MediaQueryListEvent) => setReduced(event.matches);

    setReduced(query.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);

  return reduced;
}

/**
 * Fires a callback whenever `value` changes, but not on the first render.
 *
 * Used for the track-change animation: the player must animate when the *track*
 * changes, not on every 1 Hz playback snapshot, and not on mount.
 */
export function useOnChange<T>(value: T, onChange: (next: T, previous: T) => void) {
  const previous = useRef(value);
  const first = useRef(true);

  useEffect(() => {
    if (first.current) {
      first.current = false;
      previous.current = value;
      return;
    }
    if (previous.current === value) return;

    const before = previous.current;
    previous.current = value;
    onChange(value, before);
  }, [value, onChange]);
}