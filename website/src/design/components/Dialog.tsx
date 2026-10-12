import React, { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { X } from 'lucide-react';

// Layout timing on the client, passive on the server (where `document` and
// `useLayoutEffect` both don't exist). Avoids the SSR warning without
// giving up pre-paint modal promotion.
const useIsomorphicLayoutEffect = typeof document !== 'undefined' ? useLayoutEffect : useEffect;

interface DialogProps {
  /** Controlled visibility. The element opens with `showModal`, so Esc,
      backdrop click, and focus all behave like a native modal. */
  open: boolean;
  /** Fires on Esc, backdrop click, the ✕ button, or programmatic close. */
  onClose: () => void;
  /** Heading shown in the title bar. */
  title: string;
  /** Body content. */
  children: React.ReactNode;
  /** Footer buttons, right-aligned. Absent when there are none. */
  actions?: React.ReactNode;
  /** Roomier frame for forms and editors. Defaults to a compact confirm. */
  size?: 'sm' | 'md';
}

/**
 * App-wide modal primitive on top of the native `<dialog>` element.
 *
 * Declarative (`open` boolean in, `onClose` out) so callers never touch the
 * imperative `showModal`/`close` API. The backdrop is part of the element,
 * so there is no extra overlay div to keep in sync, and Esc plus initial
 * focus come free. For a yes/no question, prefer `ConfirmDialog` below.
 */
export function Dialog({ open, onClose, title, children, actions, size = 'sm' }: DialogProps) {
  const ref = useRef<HTMLDialogElement>(null);
  const [mounted, setMounted] = useState(false);

  useIsomorphicLayoutEffect(() => {
    setMounted(true);
  }, []);

  // Layout effect, not passive: it runs before paint, so there is never a
  // frame where the dialog sits in-flow inside its mount point and shoves
  // the page down. The explicit fixed positioning is the second half of
  // that guarantee — out of flow by construction, top layer or not.
  useIsomorphicLayoutEffect(() => {
    if (!mounted) return;
    const el = ref.current;
    if (!el) return;
    // `showModal` throws when already open (double effects, fast toggles).
    if (open && !el.open) el.showModal();
    else if (!open && el.open) el.close();
  }, [open, mounted ]);

  // Portaled to <body>: a modal must never be a layout sibling. The Studio
  // header uses `space-y-3`, whose v4 rule puts margin on every non-last
  // child — mounting a dialog beside the header row moved the whole page
  // down 12px, top layer or not. SSR renders nothing (no `document` there);
  // hydration agrees on first paint, then the portal lands.
  if (!mounted || typeof document === 'undefined') return null;
  return createPortal(
    <dialog
      ref={ref}
      onClose={onClose}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      aria-label={title}
      // Fixed + capped: the frame can never exceed the viewport, so
      // `showModal`'s focus step has no reason to scroll the page behind it.
      className={`fixed inset-0 m-auto p-0 h-fit max-h-[calc(100dvh-2rem)] bg-transparent backdrop:bg-black/60 backdrop:backdrop-blur-[2px] ${
        size === 'md' ? 'w-full max-w-xl' : 'w-full max-w-md'
      }`}
    >
      <div className="max-h-[calc(100dvh-2rem)] overflow-y-auto overscroll-contain scrollbar-slim rounded-[3px] bg-[var(--bg-mantle)] border border-[var(--border)] shadow-2xl">
        <div className="flex items-center justify-between gap-3 px-4 py-3 border-b border-[var(--border)]">
          <div className="text-sm font-bold text-[var(--text-primary)]">{title}</div>
          <button
            onClick={onClose}
            type="button"
            className="p-1 text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors cursor-pointer"
            aria-label={`Close ${title}`}
          >
            <X className="w-4 h-4" />
          </button>
        </div>
        <div className="px-4 py-3">{children}</div>
        {actions && (
          <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-[var(--border)]">
            {actions}
          </div>
        )}
      </div>
    </dialog>,
    document.body,
  );
}

interface ConfirmDialogProps {
  open: boolean;
  onClose: () => void;
  /** Runs the action, then closes. */
  onConfirm: () => void;
  title: string;
  /** One or two sentences: what goes away, and whether it comes back. */
  body: string;
  confirmLabel?: string;
  cancelLabel?: string;
  /** Red confirm button for destructive actions. Defaults to mauve. */
  danger?: boolean;
}

/**
 * A yes/no question with one safe default: focus lands on Cancel, so an
 * accidental Enter never confirms a destructive action.
 */
export function ConfirmDialog({
  open,
  onClose,
  onConfirm,
  title,
  body,
  confirmLabel = 'Confirm',
  cancelLabel = 'Cancel',
  danger = false,
}: ConfirmDialogProps) {
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={title}
      actions={
        <>
          <button
            // eslint-disable-next-line jsx-a11y/no-autofocus
            autoFocus
            onClick={onClose}
            type="button"
            className="px-3 py-1.5 rounded-[2px] bg-[var(--bg-crust)] border border-[var(--border)] text-xs text-[var(--text-subtext)] hover:text-[var(--text-primary)] transition-colors cursor-pointer"
          >
            {cancelLabel}
          </button>
          <button
            onClick={() => {
              onConfirm();
              onClose();
            }}
            type="button"
            className={`px-3 py-1.5 rounded-[2px] font-semibold text-xs transition-colors cursor-pointer ${
              danger
                ? 'bg-[var(--accent-red)]/10 border border-[var(--accent-red)]/50 text-[var(--accent-red)] hover:bg-[var(--accent-red)]/20'
                : 'bg-[var(--accent-mauve)] text-[var(--bg-crust)] hover:opacity-95'
            }`}
          >
            {confirmLabel}
          </button>
        </>
      }
    >
      <p className="text-xs text-[var(--text-subtext)] leading-relaxed">{body}</p>
    </Dialog>
  );
}
