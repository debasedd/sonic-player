import { useEffect } from "react";
import { usePlayer } from "../store/playerStore";

/**
 * Tells Rust when the pointer is busy.
 *
 * The hover watcher in Rust polls the OS cursor and collapses the pill to mini
 * when the pointer rests on it. That is wrong while a drag is in progress: the
 * window would shrink out from under the user's hand and the drag would lose its
 * grip. Only the webview knows a drag started, because only the webview sees the
 * `mousedown` that Tauri turns into a window drag.
 *
 * The claim is reference-counted rather than a boolean. Two things can hold the
 * pointer at once (a popover opening while a drag ends, for example), and a plain
 * flag would release the claim on the first one to finish.
 */
export function usePointerClaim() {
  const setPointerClaimed = usePlayer((s) => s.setPointerClaimed);

  useEffect(() => {
    let depth = 0;

    const claim = () => {
      depth += 1;
      if (depth === 1) void setPointerClaimed(true);
    };

    const release = () => {
      if (depth === 0) return;
      depth -= 1;
      if (depth === 0) void setPointerClaimed(false);
    };

    // A drag region is any element Tauri should translate into a window drag.
    // Claiming on `pointerdown` (not `mousedown`) also covers pen and touch, and
    // the claim is released on the matching `pointerup` wherever it lands — a drag
    // that ends outside the window still fires `pointerup` on the captured target.
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as HTMLElement | null;
      if (!target?.closest("[data-tauri-drag-region]")) return;
      if (event.button !== 0) return;
      claim();
    };

    const onPointerUp = () => release();

    // A drag can also be aborted by the window losing focus (alt-tab, a Win32
    // modal). Without this, a lost `pointerup` would leave the claim held forever
    // and hover would silently stop working for the rest of the session.
    //
    // Releasing here is a blunt `setPointerClaimed(false)`, which reaches Rust's
    // `apply_effective_mode` and resizes the window. That is correct for a drag
    // that died on alt-tab, but wrong while a panel is open: the panel's height is
    // not the drag's to release. The claim is a depth counter, so a panel that
    // never claimed is simply not decremented here — Rust now refuses to resize
    // under an open panel as a second line of defence.
    const onBlur = () => {
      depth = 0;
      void setPointerClaimed(false);
    };

    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("pointerup", onPointerUp, true);
    document.addEventListener("pointercancel", onPointerUp, true);
    window.addEventListener("blur", onBlur);

    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("pointerup", onPointerUp, true);
      document.removeEventListener("pointercancel", onPointerUp, true);
      window.removeEventListener("blur", onBlur);
      if (depth > 0) {
        depth = 0;
        void setPointerClaimed(false);
      }
    };
  }, [setPointerClaimed]);
}