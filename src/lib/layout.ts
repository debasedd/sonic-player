/**
 * Per-mode geometry.
 *
 * These numbers must match `WindowMode::size()` in `src-tauri/src/config.rs`,
 * otherwise the window will clip or letterbox its contents.
 *
 * The window is sized in *logical* pixels; Windows applies the DPI scale.
 */

export const WINDOW = {
  pill: { width: 416, height: 108, radius: 28 },
  // 548 px is the measured natural height of the expanded content (artwork,
  // identity, scrubber, large transport, secondary row) — verified via the DOM.
  expanded: { width: 416, height: 548, radius: 26 },
  mini: { width: 248, height: 60, radius: 30 },
  // Pocket is a PAGE, not a taller window: it occupies the pill's exact rect and
  // the island slides sideways between the player and Pocket. Keeping the same
  // height as the pill is what removes the resize fight (see `set_pocket_open`
  // in `src-tauri/src/commands.rs`).
  pocket: { width: 416, height: 108, radius: 28 },
  // The drop target the window grows into while a native file drag hovers it.
  drop: { width: 416, height: 176, radius: 26 },
} as const;

export type ModeKey = keyof typeof WINDOW;

/** CSS custom properties applied to the root before paint. */
export function modeVars(mode: ModeKey): Record<string, string> {
  const { radius } = WINDOW[mode];
  return {
    "--shell-radius": `${radius}px`,
  };
}