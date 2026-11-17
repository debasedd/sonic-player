import { useEffect } from "react";
import { AnimatePresence, LayoutGroup, motion } from "framer-motion";
import { usePlayer } from "./store/playerStore";
import { useSonicEvents } from "./hooks/useSonicEvents";
import { useHotkeys } from "./hooks/useHotkeys";
import { usePointerClaim } from "./hooks/usePointerClaim";
import { SettingsPanel } from "./components/Settings/SettingsPanel";
import { PocketStage } from "./components/Pocket/PocketStage";
import { DropTarget } from "./components/Pocket/DropTarget";
import { GlassSurface } from "./components/GlassSurface/GlassSurface";
import { api } from "./services/spotify/api";
import { modeVars } from "./lib/layout";
import { blurToPx, transparencyToAlpha } from "./lib/format";
import { T_LAYOUT } from "./lib/motion";
import type { WindowMode } from "./types/spotify";
import "./styles/globals.css";

/**
 * App root.
 *
 * The window is transparent, so the body and this container must both be
 * transparent for the OS acrylic to show through.
 */
export default function App() {
  const hydrated = usePlayer((s) => s.hydrated);
  const hydrate = usePlayer((s) => s.hydrate);
  const settings = usePlayer((s) => s.settings);
  const effectiveMode = usePlayer((s) => s.effectiveMode);
  const panel = usePlayer((s) => s.panel);
  const closePanel = usePlayer((s) => s.closePanel);
  const dropActive = usePlayer((s) => s.dropActive);

  // Boot: pull the full snapshot from Rust.
  useEffect(() => {
    void hydrate();
  }, [hydrate]);

  useSonicEvents();
  useHotkeys();
  usePointerClaim();
  // File drag-and-drop is handled in Rust (`drag_drop.rs`), attached to the
  // webview. It used to be a JS `listen()` on `tauri://drag-*`, which does not
  // fire on a frameless, transparent, always-on-top window — dropping a file
  // did nothing. The webview now only draws the target the backend flag asks
  // for; there is no JS gesture handling left to get out of sync.

  // Rust is authoritative for geometry now, including hover collapse. This is the
  // mode it says the window currently is — reading `settings.mode` here would draw
  // a pill inside a window that just shrank to mini.
  const mode: WindowMode = effectiveMode;
  // Settings is a panel with its own taller rect, so it reports `expanded` for
  // radius purposes. Pocket is NOT: it is a page at the pill's exact rect, and
  // the island slides sideways into it. Reporting `expanded` for Pocket would
  // give the shell the 26px expanded radius inside a 108px-tall window.
  const layoutMode = panel === "settings" ? "expanded" : mode;

  // Panels are not player modes, so their height is requested explicitly. Doing
  // it here (rather than in the webview) keeps the window and the panel driven by
  // one side — the old webview-driven resize is what let the panel open clipped.
  useEffect(() => {
    void api.setSettingsOpen(panel === "settings");
    void api.setPocketOpen(panel === "pocket");
  }, [panel]);

  // Glass tokens derived from the user's settings. These drive the CSS custom
  // properties in globals.css, so the sliders are live rather than cosmetic.
  //
  // `solidSurface` swaps the material rather than adjusting it: the surface goes
  // fully opaque, the backdrop blur is dropped to 0, and `data-solid` on the root
  // lets CSS strip the sheen, ambient tint, album wash and cursor light. The
  // sliders are left untouched underneath, so turning Solid off restores exactly
  // what the user had.
  const solid = settings?.solidSurface ?? false;

  const vars = {
    ...modeVars(layoutMode),
    "--glass-alpha": solid
      ? "1"
      : transparencyToAlpha(settings?.transparency ?? 50).toFixed(3),
    "--glass-blur": solid ? "0px" : `${blurToPx(settings?.blur ?? 72)}px`,
    "--glass-blur-strong": solid
      ? "0px"
      : `${Math.round(blurToPx(settings?.blur ?? 72) * 1.75)}px`,
  } as React.CSSProperties;

  // The root fills the window rather than declaring its own size. Rust animates
  // the window between mode geometries; a fixed width/height here would be a
  // second, competing geometry that clips the content for the length of every
  // transition.
  return (
    <div
      className="relative h-full w-full overflow-hidden bg-transparent"
      style={vars}
      data-mode={layoutMode}
      data-solid={solid ? "true" : undefined}
      data-drop={dropActive ? "true" : undefined}
    >
      {/* Do NOT use mode="wait" here. `wait` finishes the exit animation before
          starting the enter, which leaves one or more frames with no content on
          screen at all — the "broken frame" a mode change used to show. Letting
          enter and exit overlap cross-draws the two layouts under the growing
          window, which is what makes the transition read as continuous. */}
      <LayoutGroup id="sonic-player">
        <AnimatePresence initial={false}>
          {panel === "settings" ? (
          <motion.div
            key="settings"
            initial={{ opacity: 0, scale: 0.985 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.99 }}
            transition={T_LAYOUT}
            className="absolute inset-0"
          >
            <GlassSurface
              interactive={false}
              className="flex h-full w-full flex-col overflow-hidden"
              style={{ borderRadius: "var(--shell-radius, 26px)" }}
            >
              <SettingsPanel />
            </GlassSurface>
          </motion.div>
        ) : dropActive ? (
          /* The native file drag is over the player. Rust has already grown the
             window to the drop rect, so this surface only has to draw the target.
             It replaces the player rather than stacking on it: two glass surfaces
             in one 176 px window would read as a rendering fault. */
          <motion.div
            key="drop"
            initial={{ opacity: 0, scale: 0.99 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.99 }}
            transition={T_LAYOUT}
            className="absolute inset-0"
          >
            <DropTarget />
          </motion.div>
        ) : (
          <motion.div
            key={`stage-${mode}`}
            initial={{ opacity: 0.99 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0.99 }}
            transition={T_LAYOUT}
            className="absolute inset-0"
            onClick={() => {
              // A click on the island's empty surface should not open anything,
              // but closing an open popover via backdrop click is expected.
              if (panel !== "none") closePanel();
            }}
          >
            {/* The stage always renders BOTH pages side by side and slides
                between them. That is what makes the gesture reversible and
                draggable; a mount/unmount swap can only crossfade. */}
            <PocketStage mode={mode} />
          </motion.div>
          )}
        </AnimatePresence>
      </LayoutGroup>

      {/* First-paint veil: hides the very first frame so the window never flashes
          an unstyled white box before the CSS lands. */}
      <AnimatePresence>
        {!hydrated && (
          <motion.div
            className="pointer-events-none absolute inset-0 bg-transparent"
            initial={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.3, ease: "easeOut" }}
          />
        )}
      </AnimatePresence>
    </div>
  );
}