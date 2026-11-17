import { useEffect, useRef } from "react";
import { usePlayer } from "../store/playerStore";

/**
 * Global keyboard shortcuts.
 *
 * The window is frameless and never focused by default, so we bind on `window`
 * and skip when a form field owns the keyboard.
 */
export function useHotkeys() {
  const transport = usePlayer((s) => s.transport);
  const hide = usePlayer((s) => s.hide);
  const openSettings = usePlayer((s) => s.openSettings);
  const panel = usePlayer((s) => s.panel);
  const closePanel = usePlayer((s) => s.closePanel);
  const setMode = usePlayer((s) => s.setMode);
  const modeRef = useRef(usePlayer.getState().settings?.mode ?? "pill");

  useEffect(() => {
    const unsubscribe = usePlayer.subscribe((state) => {
      modeRef.current = state.settings?.mode ?? "pill";
    });
    return unsubscribe;
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      // Never steal keys from a form field.
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable)
      ) {
        return;
      }

      // Space and Enter activate a focused button natively. Binding Space to
      // play/pause as well meant a keyboard user who tabbed to "Next track" and
      // pressed Space got both actions — the button fired and the transport
      // toggled, so the visible result depended on which handler won.
      if (
        target?.tagName === "BUTTON" &&
        (event.key === " " || event.key === "Enter")
      ) {
        return;
      }

      switch (event.key) {
        case " ":
          event.preventDefault();
          void transport("toggle");
          break;
        case "ArrowRight":
          if (event.shiftKey) {
            event.preventDefault();
            void transport("next");
          }
          break;
        case "ArrowLeft":
          if (event.shiftKey) {
            event.preventDefault();
            void transport("previous");
          }
          break;
        case "Escape":
          if (panel !== "none") {
            event.preventDefault();
            closePanel();
          }
          break;
        case ",":
          if (event.ctrlKey) {
            event.preventDefault();
            openSettings();
          }
          break;
        case "m":
        case "M":
          event.preventDefault();
          void setMode(modeRef.current === "mini" ? "pill" : "mini");
          break;
        case "e":
        case "E":
          event.preventDefault();
          void setMode(modeRef.current === "expanded" ? "pill" : "expanded");
          break;
        case "h":
        case "H":
          event.preventDefault();
          void hide();
          break;
        default:
          break;
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [transport, hide, openSettings, closePanel, setMode, panel]);
}