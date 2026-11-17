import { useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowDownUp,
  Check,
  Clipboard,
  FileText,
  FolderOpen,
  Image as ImageIcon,
  Music,
  Pin,
  PinOff,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import { GlassSurface } from "../GlassSurface/GlassSurface";
import { usePlayer } from "../../store/playerStore";
import type { ClipboardEntry, ShelfEntry } from "../../types/spotify";
import { cn, formatBytes } from "../../lib/format";

/**
 * Pocket — a discreet utility page inside the same window as the player.
 *
 * ## Why it is not a second window
 *
 * Sonic's whole identity is one floating pill. A second window would add a second
 * always-on-top surface, a second lifecycle, and a second thing for the user to
 * lose behind other apps. Pocket takes over the *existing* window instead: same
 * glass, same radius, same pointer light, no extra process or webview.
 *
 * ## Compact by default
 *
 * The page is the pill's own rect (416x108), so it cannot show a long list. It
 * shows one row at a time and the list itself scrolls. That is the point: drag-out
 * from the shelf must not require expanding anything first.
 */
export function PocketPanel() {
  const tab = usePlayer((s) => s.pocketTab);
  const setTab = usePlayer((s) => s.setPocketTab);
  const close = usePlayer((s) => s.closePanel);
  const refreshShelf = usePlayer((s) => s.refreshShelf);
  const refreshVault = usePlayer((s) => s.refreshVault);
  const shelf = usePlayer((s) => s.shelf);
  const vault = usePlayer((s) => s.vault);
  // Usage comes from the snapshot's settings block rather than a separate
  // command: `ui_state` already carries the counters, so reading them here costs
  // nothing and keeps one source of truth.
  const shelfBytes = usePlayer((s) => s.settings?.shelfBytes ?? 0);

  // The panel owns its own data: Rust is authoritative, but the list is only
  // needed while the page is visible. Both pages stay mounted inside the sliding
  // stage, so a plain on-mount fetch would run on every app start even when the
  // user never opens Pocket. Keying on `panel` keeps it lazy.
  const panel = usePlayer((s) => s.panel);
  const active = panel === "pocket";

  useEffect(() => {
    if (!active) return;
    void refreshShelf();
    void refreshVault();
  }, [active, refreshShelf, refreshVault]);

  return (
    <GlassSurface
      interactive={false}
      artwork={undefined}
      className="flex h-full w-full flex-col overflow-hidden"
      style={{ borderRadius: "var(--shell-radius, 28px)" }}
    >
      {/* One compact row. The page is the pill's own 108 px, so the header, the
          tabs and one list row have to share it — there is no room for a second
          line, which is why the usage figure sits on the same line as the title. */}
      <div className="flex items-center gap-2 px-3 pt-2.5">
        <p className="shrink-0 text-[11.5px] font-semibold leading-none text-white">Pocket</p>
        <p className="min-w-0 flex-1 truncate text-[9.5px] font-medium leading-none text-ink-faint">
          {formatBytes(shelfBytes)} · {shelf.length} file{shelf.length === 1 ? "" : "s"}
          {tab === "clipboard" && vault.length > 0
            ? ` · ${vault.length} clip${vault.length === 1 ? "" : "s"}`
            : ""}
        </p>
        <TabBar tab={tab} setTab={setTab} />
        <button
          type="button"
          onClick={close}
          aria-label="Close Pocket"
          title="Close"
          className="ctl h-[22px] w-[22px] shrink-0"
        >
          <X size={12.5} strokeWidth={2.3} />
        </button>
      </div>

      <div className="mt-1.5 flex min-h-0 flex-1 flex-col">
        {tab === "files" ? <FilesTab /> : <ClipboardTab />}
      </div>
    </GlassSurface>
  );
}

function TabBar({
  tab,
  setTab,
}: {
  tab: "files" | "clipboard";
  setTab: (tab: "files" | "clipboard") => void;
}) {
  return (
    <div className="flex shrink-0 gap-0.5" role="tablist" aria-label="Pocket sections">
      {/* Icon-only: at pill height there is no room for the words, and the two
          icons are unambiguous. The accessible name comes from aria-label. */}
      <TabButton
        active={tab === "files"}
        onClick={() => setTab("files")}
        icon={<FileText size={12.5} strokeWidth={2.2} />}
        label="Files"
      />
      <TabButton
        active={tab === "clipboard"}
        onClick={() => setTab("clipboard")}
        icon={<Clipboard size={12.5} strokeWidth={2.2} />}
        label="Clipboard"
      />
    </div>
  );
}

function TabButton({
  active,
  onClick,
  icon,
  label,
}: {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
}) {
  return (
    <button
      type="button"
      role="tab"
      onClick={onClick}
      aria-label={label}
      aria-selected={active}
      title={label}
      className={cn(
        "relative inline-flex h-[22px] w-[22px] items-center justify-center rounded-full transition-colors duration-150",
        active ? "text-white" : "text-ink-muted hover:text-white/90",
      )}
    >
      {active && (
        <span
          aria-hidden="true"
          className="absolute inset-0 rounded-full bg-white/[0.1] shadow-[inset_0_1px_0_rgba(255,255,255,0.14)]"
        />
      )}
      <span className="relative flex items-center justify-center">{icon}</span>
    </button>
  );
}

/* -------------------------------------------------------------------------- */
/* Files                                                                      */
/* -------------------------------------------------------------------------- */

function FilesTab() {
  const shelf = usePlayer((s) => s.shelf);
  const clearShelf = usePlayer((s) => s.clearShelf);
  // Search is not offered at pill height: the list is short enough to scan and a
  // text field would consume a third of the page. `query` stays in the filter so
  // a future expanded surface can drive it without a rewrite.
  const [query] = useState("");
  const [pinnedFirst, setPinnedFirst] = useState(true);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    const filtered = q ? shelf.filter((e) => e.name.toLowerCase().includes(q)) : shelf.slice();
    // Pinned first, then newest. The sort is stable, so pinned items keep their
    // own relative order rather than shuffling on every keystroke.
    filtered.sort((a, b) => {
      if (pinnedFirst && a.pinned !== b.pinned) return a.pinned ? -1 : 1;
      return b.addedAt - a.addedAt;
    });
    return filtered;
  }, [shelf, query, pinnedFirst]);

  return (
    <>
      {/* At pill height there is no room for a search field above the list, so
          the list starts immediately and the tools move into the list's own top
          edge as a single compact strip. Search lives in the expanded Settings
          surface; the shelf is short enough that scanning it beats typing. */}
      <div className="flex items-center gap-1 px-3 pb-1">
        <button
          type="button"
          onClick={() => setPinnedFirst((v) => !v)}
          aria-pressed={pinnedFirst}
          aria-label="Sort pinned first"
          title="Pinned first"
          className={cn("ctl h-[22px] w-[22px]", pinnedFirst && "ctl-active")}
        >
          <ArrowDownUp size={12} strokeWidth={2.2} />
        </button>
        {shelf.length > 0 && (
          <button
            type="button"
            onClick={() => void clearShelf()}
            aria-label="Clear shelf"
            title="Clear shelf"
            className="ctl h-[22px] w-[22px]"
          >
            <Trash2 size={12} strokeWidth={2.2} />
          </button>
        )}
      </div>

      <div className="scroll-slim min-h-0 flex-1 overflow-y-auto px-2.5 pb-2">
        {visible.length === 0 ? (
          <EmptyState
            icon={<FolderOpen size={22} strokeWidth={1.7} />}
            title={shelf.length === 0 ? "No files yet" : "No matches"}
            body={
              shelf.length === 0
                ? "Drag files from Explorer onto Sonic to keep them here."
                : `Nothing matches “${query}”.`
            }
          />
        ) : (
          <ul className="flex flex-col gap-0.5">
            {visible.map((entry) => (
              <ShelfRow key={entry.id} entry={entry} />
            ))}
          </ul>
        )}
      </div>
    </>
  );
}

function ShelfRow({ entry }: { entry: ShelfEntry }) {
  const remove = usePlayer((s) => s.removeShelfItem);
  const togglePin = usePlayer((s) => s.toggleShelfPinned);
  const open = usePlayer((s) => s.openShelfItem);
  const reveal = usePlayer((s) => s.revealShelfItem);
  const dragOut = usePlayer((s) => s.dragShelfOut);
  const [confirming, setConfirming] = useState(false);
  const timer = useRef<number | null>(null);

  useEffect(() => {
    return () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    };
  }, []);

  const askRemove = () => {
    if (!confirming) {
      setConfirming(true);
      // Two-step delete: the first click arms, the second commits. The window is
      // short and a stray click next to a row is easy, so an instant delete would
      // lose a file the user did not mean to touch.
      timer.current = window.setTimeout(() => setConfirming(false), 3200);
      return;
    }
    setConfirming(false);
    void remove(entry.id);
  };

  return (
    <li
      className={cn(
        "pocket-row group flex h-[30px] items-center gap-2 rounded-[10px] px-2 transition-colors duration-150",
      )}
      // Native drag-out. `draggable` alone is not enough in a Tauri webview: the
      // pointerdown claims the gesture for Rust so the OLE drag starts from the
      // real cursor position rather than a synthesized DOM event.
      draggable
      onDragStart={(e) => {
        // The ghost is optional; without one Windows draws its generic icon.
        e.dataTransfer.effectAllowed = "copy";
        e.dataTransfer.setData("application/x-sonic-shelf", entry.id);
        void dragOut([entry.path]);
      }}
    >
      <span
        aria-hidden="true"
        className="grid h-[22px] w-[22px] shrink-0 place-items-center rounded-[7px] bg-white/[0.07] text-ink-muted"
      >
        <ShelfIcon entry={entry} />
      </span>

      <button
        type="button"
        onClick={() => void open(entry.path)}
        className="min-w-0 flex-1 text-left"
        title="Open"
      >
        <span className="flex items-center gap-1">
          {entry.pinned && <Pin size={9.5} strokeWidth={2.4} className="shrink-0 text-ink-muted" />}
          <span className="clamp-1 text-[11px] font-medium leading-tight text-white">{entry.name}</span>
          <span className="clamp-1 shrink-0 text-[9.5px] leading-tight text-ink-faint">
            {formatBytes(entry.size)}
          </span>
        </span>
      </button>

      <div className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity duration-150 focus-within:opacity-100 group-hover:opacity-100">
        <button
          type="button"
          onClick={() => void reveal(entry.path)}
          aria-label="Show in Explorer"
          title="Show in Explorer"
          className="ctl h-6 w-6"
        >
          <FolderOpen size={12} strokeWidth={2.1} />
        </button>
        <button
          type="button"
          onClick={() => void togglePin(entry.id, !entry.pinned)}
          aria-label={entry.pinned ? "Unpin" : "Pin"}
          title={entry.pinned ? "Unpin" : "Pin"}
          className="ctl h-6 w-6"
        >
          {entry.pinned ? <PinOff size={12} strokeWidth={2.1} /> : <Pin size={12} strokeWidth={2.1} />}
        </button>
        <button
          type="button"
          onClick={askRemove}
          aria-label={confirming ? "Confirm remove" : "Remove"}
          title={confirming ? "Click again to remove" : "Remove"}
          className={cn("ctl h-6 w-6", confirming && "text-red-300")}
        >
          {confirming ? <Check size={12} strokeWidth={2.4} /> : <Trash2 size={12} strokeWidth={2.1} />}
        </button>
      </div>
    </li>
  );
}

function ShelfIcon({ entry }: { entry: ShelfEntry }) {
  const props = { size: 13, strokeWidth: 2.1 } as const;
  switch (entry.kind) {
    case "image":
      return <ImageIcon {...props} />;
    case "audio":
      return <Music {...props} />;
    case "archive":
      return <Upload {...props} />;
    default:
      return <FileText {...props} />;
  }
}

/* -------------------------------------------------------------------------- */
/* Clipboard                                                                  */
/* -------------------------------------------------------------------------- */

function ClipboardTab() {
  const vault = usePlayer((s) => s.vault);
  const restore = usePlayer((s) => s.restoreClipboardItem);
  const clear = usePlayer((s) => s.clearVault);
  const capture = usePlayer((s) => s.captureClipboardNow);
  const settings = usePlayer((s) => s.settings);
  const [query] = useState("");

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    const filtered = q
      ? vault.filter((e) => (e.text ?? e.id).toLowerCase().includes(q))
      : vault.slice();
    filtered.sort((a, b) => {
      if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
      return b.capturedAt - a.capturedAt;
    });
    return filtered;
  }, [vault, query]);

  const enabled = settings?.clipboardEnabled ?? false;

  return (
    <>
      <div className="flex items-center gap-1 px-3 pb-1">
        <button
          type="button"
          onClick={() => void capture()}
          aria-label="Capture now"
          title="Capture now"
          className="ctl h-[22px] w-[22px]"
        >
          <Clipboard size={12} strokeWidth={2.2} />
        </button>
        {vault.length > 0 && (
          <button
            type="button"
            onClick={() => void clear()}
            aria-label="Clear clipboard history"
            title="Clear history"
            className="ctl h-[22px] w-[22px]"
          >
            <Trash2 size={12} strokeWidth={2.2} />
          </button>
        )}
      </div>

      <div className="scroll-slim min-h-0 flex-1 overflow-y-auto px-2.5 pb-2">
        {!enabled ? (
          <EmptyState
            icon={<Clipboard size={22} strokeWidth={1.7} />}
            title="Clipboard capture is off"
            body="Sonic only records what you copy when you turn this on. Everything stays on this PC."
          />
        ) : visible.length === 0 ? (
          <EmptyState
            icon={<Clipboard size={22} strokeWidth={1.7} />}
            title={vault.length === 0 ? "Nothing captured yet" : "No matches"}
            body={
              vault.length === 0
                ? "Copy something, then press the clipboard button to check."
                : `Nothing matches “${query}”.`
            }
          />
        ) : (
          <ul className="flex flex-col gap-0.5">
            {visible.map((entry) => (
              <VaultRow key={entry.id} entry={entry} onRestore={() => void restore(entry.id)} />
            ))}
          </ul>
        )}
      </div>
    </>
  );
}

function VaultRow({ entry, onRestore }: { entry: ClipboardEntry; onRestore: () => void }) {
  const remove = usePlayer((s) => s.removeVaultItem);
  const togglePin = usePlayer((s) => s.toggleVaultPinned);
  const preview = (entry.text ?? "").replace(/\s+/g, " ").trim();

  return (
    <li className="pocket-row group flex h-[30px] items-center gap-2 rounded-[10px] px-2 transition-colors duration-150">
      <button
        type="button"
        onClick={onRestore}
        title="Copy back to clipboard"
        className="flex min-w-0 flex-1 items-center gap-1 text-left"
      >
        {entry.pinned && <Pin size={9.5} strokeWidth={2.4} className="shrink-0 text-ink-muted" />}
        <span className="clamp-1 text-[11px] leading-tight text-white">
          {preview || (entry.kind === "image" ? "Image" : "(empty)")}
        </span>
        <span className="clamp-1 shrink-0 text-[9.5px] leading-tight text-ink-faint">
          {entry.source || "app"}
        </span>
      </button>

      <div className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity duration-150 focus-within:opacity-100 group-hover:opacity-100">
        <button
          type="button"
          onClick={() => void togglePin(entry.id, !entry.pinned)}
          aria-label={entry.pinned ? "Unpin" : "Pin"}
          title={entry.pinned ? "Unpin" : "Pin"}
          className="ctl h-6 w-6"
        >
          {entry.pinned ? <PinOff size={12} strokeWidth={2.1} /> : <Pin size={12} strokeWidth={2.1} />}
        </button>
        <button
          type="button"
          onClick={() => void remove(entry.id)}
          aria-label="Remove"
          title="Remove"
          className="ctl h-6 w-6"
        >
          <Trash2 size={12} strokeWidth={2.1} />
        </button>
      </div>
    </li>
  );
}

function EmptyState({ icon, title, body }: { icon: React.ReactNode; title: string; body: string }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-1 px-5 pb-1 text-center">
      <span aria-hidden="true" className="scale-[0.82] text-ink-faint/70">
        {icon}
      </span>
      <p className="text-[11px] font-semibold leading-tight text-white/90">{title}</p>
      <p className="mx-auto line-clamp-1 max-w-[280px] text-[9.5px] leading-snug text-ink-faint">{body}</p>
    </div>
  );
}