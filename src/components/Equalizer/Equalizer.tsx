import { cn } from "../../lib/format";

/**
 * Decorative "now playing" equaliser.
 *
 * IMPORTANT — this is not an audio spectrum analyser. The Spotify Web API exposes
 * no frequency or amplitude data for the current playback, so there is nothing
 * real to plot. These bars are a purely decorative animation whose only input is
 * whether playback is running; they are deliberately never labelled as an
 * analyser, and the code says so in as many words.
 *
 * Behaviour:
 *   - animates while playing
 *   - freezes when paused (the animation is removed, so the bars hold still)
 *   - unmounts entirely when disconnected
 *
 * When the OS asks for reduced motion the bars render as a static, level mark.
 */
export function Equalizer({
  playing,
  className,
  bars = 4,
  barClassName,
}: {
  playing: boolean;
  className?: string;
  bars?: number;
  barClassName?: string;
}) {
  // A fixed set of periods keeps the motion organic without any JS timer: each bar
  // is a CSS animation with its own duration and delay.
  const periods = [900, 1180, 760, 1040, 880];
  const delays = [0, 140, 70, 210, 110];

  return (
    <span
      aria-hidden="true"
      className={cn("inline-flex items-end", className)}
      style={{ gap: 2, height: 10 }}
    >
      {Array.from({ length: bars }, (_, index) => (
        <span
          key={index}
          className={cn("eq-bar", barClassName)}
          style={{
            width: 2,
            height: "100%",
            borderRadius: 1,
            background: "currentColor",
            // Paused: hold a pleasant mid-level rather than a flat line, so the
            // mark still reads as a deliberate glyph.
            animationPlayState: playing ? "running" : "paused",
            animationDuration: `${periods[index % periods.length]}ms`,
            animationDelay: `${delays[index % delays.length]}ms`,
            transform: playing ? undefined : "scaleY(0.5)",
            opacity: playing ? 1 : 0.55,
            transition: "opacity 220ms cubic-bezier(0.32,0.72,0,1)",
          }}
        />
      ))}
    </span>
  );
}