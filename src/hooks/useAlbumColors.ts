import { useEffect, useState } from "react";

/**
 * Dominant colours for the current artwork, used for the ambient glow.
 *
 * Artwork is sampled on a tiny canvas (16x16) rather than at full size: the result
 * is a handful of averages, so the source resolution is irrelevant and a small
 * canvas keeps the cost negligible. Results are cached by URL, so polling the same
 * track every second does no work at all.
 *
 * The palette is deliberately muted. Spotify covers are often extremely saturated,
 * and a saturated glow behind dark glass reads as a neon sign rather than a
 * premium material, so each swatch is pulled toward a neutral mid-grey.
 */

export interface AlbumPalette {
  /** A representative colour, `r g b` for use inside `rgb(var(--x) / a)`. */
  base: string;
  /** A slightly brighter companion for the rim of the glow. */
  lift: string;
}

const CACHE = new Map<string, AlbumPalette | null>();
const NEUTRAL: AlbumPalette = { base: "148 148 158", lift: "196 196 206" };

/** Pulls a colour toward neutral so it tints the glass instead of shouting. */
function mute(r: number, g: number, b: number, amount: number): [number, number, number] {
  const grey = (r + g + b) / 3;
  return [
    Math.round(r + (grey - r) * amount),
    Math.round(g + (grey - g) * amount),
    Math.round(b + (grey - b) * amount),
  ];
}

function extract(image: HTMLImageElement): AlbumPalette | null {
  const size = 16;
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;

  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) return null;

  try {
    context.drawImage(image, 0, 0, size, size);
    const { data } = context.getImageData(0, 0, size, size);

    // Bucket pixels by hue so a small, unusually bright element (a white logo in
    // the corner of a cover) cannot dominate the average. We score buckets by
    // saturation-weighted population and then average only the winning bucket.
    const buckets = new Map<number, { n: number; r: number; g: number; b: number }>();

    for (let i = 0; i < data.length; i += 4) {
      const alpha = data[i + 3];
      if (alpha < 200) continue;

      const r = data[i];
      const g = data[i + 1];
      const b = data[i + 2];

      const max = Math.max(r, g, b);
      const min = Math.min(r, g, b);
      const saturation = max === 0 ? 0 : (max - min) / max;

      // Ignore near-black and near-white pixels: they carry no hue information and
      // would drag the average toward grey.
      if (max < 26 || min > 232) continue;

      const key = Math.round(saturation * 4) * 1000 + Math.round((max / 255) * 4) * 100;
      const bucket = buckets.get(key) ?? { n: 0, r: 0, g: 0, b: 0 };
      bucket.n += 1;
      bucket.r += r;
      bucket.g += g;
      bucket.b += b;
      buckets.set(key, bucket);
    }

    let best: { n: number; r: number; g: number; b: number } | null = null;
    for (const bucket of buckets.values()) {
      if (!best || bucket.n > best.n) best = bucket;
    }
    if (!best || best.n === 0) return NEUTRAL;

    const r = best.r / best.n;
    const g = best.g / best.n;
    const b = best.b / best.n;

    // Mute hard: 0.55 toward grey keeps a hint of the album's character without
    // letting it tint the whole surface.
    const [mr, mg, mb] = mute(r, g, b, 0.55);
    const [lr, lg, lb] = mute(r, g, b, 0.35);

    return {
      base: `${mr} ${mg} ${mb}`,
      lift: `${Math.min(255, Math.round(lr * 1.18))} ${Math.min(255, Math.round(lg * 1.18))} ${Math.min(255, Math.round(lb * 1.18))}`,
    };
  } catch {
    // A tainted canvas (missing CORS headers) throws on getImageData. The artwork
    // still renders; we simply fall back to the neutral palette.
    return null;
  }
}

/**
 * Returns the palette for an artwork URL.
 *
 * `null` means "not resolved yet or unavailable" — callers should fall back to the
 * neutral glass rather than rendering a coloured glow.
 */
export function useAlbumColors(src: string | undefined): AlbumPalette | null {
  const [palette, setPalette] = useState<AlbumPalette | null>(() =>
    src ? CACHE.get(src) ?? null : null,
  );

  useEffect(() => {
    if (!src) {
      setPalette(null);
      return;
    }

    const cached = CACHE.get(src);
    if (cached !== undefined) {
      setPalette(cached);
      return;
    }

    let cancelled = false;
    const image = new Image();
    // Required: without this the canvas is tainted and getImageData throws.
    image.crossOrigin = "anonymous";
    image.decoding = "async";

    image.onload = () => {
      if (cancelled) return;
      const result = extract(image);
      CACHE.set(src, result);
      setPalette(result);
    };
    image.onerror = () => {
      if (cancelled) return;
      // Cache the failure too, so a broken URL is not re-fetched on every poll.
      CACHE.set(src, null);
      setPalette(null);
    };

    image.src = src;

    return () => {
      cancelled = true;
    };
  }, [src]);

  return palette;
}