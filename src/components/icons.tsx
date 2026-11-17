import type { SVGProps } from "react";

/**
 * Inline SVG icons.
 *
 * Lucide is the primary icon set; this module only holds glyphs Lucide does not
 * ship (the Spotify mark and a couple of one-off shapes). Everything is stroke
 * or fill on `currentColor` so it inherits the glass text colours, and every
 * path is a hand-written vector — no raster or 3D assets anywhere in Sonic.
 */

type IconProps = SVGProps<SVGSVGElement>;

export function MusicNote(props: IconProps) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...props}
    >
      <circle cx="6.5" cy="17.5" r="2.5" />
      <circle cx="17.5" cy="15.5" r="2.5" />
      <path d="M9 17.5V7.2a1 1 0 0 1 .76-.97l8-1.85a1 1 0 0 1 1.24.97v10.15" />
      <path d="M9 9.6l9.99-2.3" />
    </svg>
  );
}

/**
 * The Spotify mark.
 *
 * Drawn as three concentric arcs cut by the circle edge — the official mark is a
 * trademark, so this is a deliberately simplified rendition used only to label
 * the "Open in Spotify" affordance.
 */
export function SpotifyMark(props: IconProps) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden="true"
      {...props}
    >
      <path d="M12 1.5a10.5 10.5 0 1 0 0 21 10.5 10.5 0 0 0 0-21Zm4.85 15.16a.79.79 0 0 1-1.08.26c-2.96-1.81-6.69-2.22-11.08-1.22a.79.79 0 1 1-.35-1.54c4.8-1.09 8.91-.62 12.24 1.42.37.23.49.71.27 1.08Zm1.3-2.9a.98.98 0 0 1-1.35.32c-3.39-2.08-8.56-2.69-12.57-1.47a.98.98 0 1 1-.57-1.88c4.58-1.39 10.27-.71 14.17 1.68.46.29.6.89.32 1.35Zm.11-3.02C14.2 8.35 7.7 8.13 3.8 9.3a1.18 1.18 0 1 1-.68-2.26C7.6 5.75 14.78 6 19.4 8.74a1.18 1.18 0 0 1-1.2 2.03l.06.07Z" />
    </svg>
  );
}

/** Windows glyph used on the platform-specific hint. */
export function WindowsMark(props: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" {...props}>
      <path d="M3 5.6 10.4 4.6v6.9H3V5.6Zm0 12.8 7.4 1v-6.8H3v5.8Zm8.6 1.2L21 21V12.5h-9.4v7.1Zm0-15.2v7h9.4V3l-9.4 1.4Z" />
    </svg>
  );
}

/** A small waveform used as a "now playing" indicator. */
export function Waveform({ className }: { className?: string }) {
  return (
    <span
      className={className}
      style={{ display: "inline-flex", alignItems: "flex-end", gap: 2, height: 10 }}
      aria-hidden="true"
    >
      {[3, 6, 9, 5].map((h, i) => (
        <span
          key={i}
          style={{
            width: 2,
            height: h,
            borderRadius: 1,
            background: "currentColor",
            animation: `wave 900ms ease-in-out ${i * 110}ms infinite alternate`,
          }}
        />
      ))}
      <style>{`
        @keyframes wave {
          from { transform: scaleY(0.45); }
          to   { transform: scaleY(1); }
        }
      `}</style>
    </span>
  );
}