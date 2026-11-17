import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** Conditional class names with Tailwind conflict resolution. */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** `3:07` for durations under an hour, `1:03:07` above. */
export function formatTime(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "0:00";
  const total = Math.floor(ms / 1000);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;

  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  }
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

/** Joins artist names the way Spotify shows them on a single line. */
export function joinArtists(artists: string[]): string {
  if (artists.length === 0) return "Unknown artist";
  if (artists.length === 1) return artists[0];
  if (artists.length === 2) return `${artists[0]}, ${artists[1]}`;
  return `${artists.slice(0, -1).join(", ")} & ${artists[artists.length - 1]}`;
}

/** Device-type glyphs, in words, for the picker. */
export function deviceKindLabel(kind: string): string {
  switch (kind.toLowerCase()) {
    case "computer":
      return "This computer";
    case "smartphone":
      return "Phone";
    case "tablet":
      return "Tablet";
    case "speaker":
      return "Speaker";
    case "tv":
      return "TV";
    case "tv_streaming_device":
    case "game_console":
      return "Console";
    case "automobile":
      return "Car";
    case "castaudio":
    case "castvideo":
      return "Cast";
    default:
      return kind ? kind.replace(/_/g, " ") : "Device";
  }
}

/** Clamps a number into an inclusive range. */
export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/** Maps a 0-100 "intensity" onto a px blur radius. */
export function blurToPx(blur: number): number {
  // 0 → 0px, 100 → 16px.
  //
  // Deliberately a short range. On a transparent Tauri window a large
  // backdrop-filter averages the desktop into a single flat tone, which reads as
  // grey plastic rather than glass. The default (72) lands on 12px.
  return Math.round((clamp(blur, 0, 100) / 100) * 16);
}

/** Maps a 0-100 "transparency" onto an alpha for the glass fill. */
export function transparencyToAlpha(transparency: number): number {
  // Higher transparency → lower alpha. The range is deliberately wide so the
  // slider is a real choice rather than a nudge: 0 is a near-solid sheet, 100 is
  // barely-there tint over the desktop blur.
  const t = clamp(transparency, 0, 100) / 100;
  // 0 -> 0.52 (near-solid), 100 -> 0.08 (barely-there tint).
  // Default (50) lands on 0.30: clearly translucent over the blurred desktop,
  // and white text still measures ~10:1 contrast (WCAG AAA needs 7:1).
  return 0.52 - t * 0.44;
}

/**
 * Bytes as a short human size.
 *
 * The shelf shows sizes next to filenames, and "1048576" is not a size anyone
 * reads. One decimal below 10 KB keeps "1.4 KB" honest without "1.40 KB".
 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"] as const;
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const rounded = unit === 0 ? value : value < 10 ? Math.round(value * 10) / 10 : Math.round(value);
  return `${rounded} ${units[unit]}`;
}

/** Unix seconds as a short relative-ish date: "Today", "Yesterday", "12 Mar". */
export function formatDay(unixSeconds: number): string {
  if (!Number.isFinite(unixSeconds) || unixSeconds <= 0) return "";
  const then = new Date(unixSeconds * 1000);
  const now = new Date();
  const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((startOfDay(now) - startOfDay(then)) / 86_400_000);
  if (days === 0) return "Today";
  if (days === 1) return "Yesterday";
  if (days > 1 && days < 7) return `${days} days ago`;
  return then.toLocaleDateString(undefined, { day: "numeric", month: "short" });
}