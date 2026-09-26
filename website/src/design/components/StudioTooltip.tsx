import React, { useState, useRef, useEffect, useCallback } from 'react';
import { createPortal } from 'react-dom';
import { HelpCircle, ShieldAlert, Terminal, X, Copy, Check } from 'lucide-react';

export interface StudioTooltipProps {
  title: string;
  description: string;
  quadlet?: string;
  security?: string;
  section?: string;
  align?: 'left' | 'right' | 'center';
}

export function StudioTooltip({
  title,
  description,
  quadlet,
  security,
  section,
  align = 'left',
}: StudioTooltipProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const [coords, setCoords] = useState<{
    top: number;
    left: number;
    placement: 'top' | 'bottom';
  }>({
    top: 0,
    left: 0,
    placement: 'bottom',
  });

  const triggerRef = useRef<HTMLButtonElement>(null);
  const tooltipRef = useRef<HTMLDivElement>(null);
  const closeTimeoutRef = useRef<number | null>(null);

  const calculatePosition = useCallback(() => {
    if (!triggerRef.current) return;
    const rect = triggerRef.current.getBoundingClientRect();

    // Check if trigger is currently visible inside the viewport
    if (rect.bottom < 0 || rect.top > window.innerHeight) {
      setIsOpen(false);
      return;
    }

    const tooltipWidth = Math.min(320, window.innerWidth - 24);
    const spaceBelow = window.innerHeight - rect.bottom;
    const spaceAbove = rect.top;

    // Prefer bottom, switch to top if space below is limited
    let placement: 'top' | 'bottom' = 'bottom';
    let top = rect.bottom + 6;

    if (spaceBelow < 240 && spaceAbove > spaceBelow) {
      placement = 'top';
      top = rect.top - 6;
    }

    // Horizontal calculation with alignment bias
    let left = rect.left - 8;
    if (align === 'right') {
      left = rect.right - tooltipWidth + 8;
    } else if (align === 'center') {
      left = rect.left + rect.width / 2 - tooltipWidth / 2;
    }

    // Clamp strictly within viewport margins
    left = Math.max(12, Math.min(left, window.innerWidth - tooltipWidth - 12));

    setCoords({ top, left, placement });
  }, [align]);

  const handleOpen = () => {
    if (closeTimeoutRef.current) {
      window.clearTimeout(closeTimeoutRef.current);
      closeTimeoutRef.current = null;
    }
    calculatePosition();
    setIsOpen(true);
  };

  const handleClose = (immediate = false) => {
    if (immediate) {
      if (closeTimeoutRef.current) {
        window.clearTimeout(closeTimeoutRef.current);
        closeTimeoutRef.current = null;
      }
      setIsOpen(false);
    } else {
      closeTimeoutRef.current = window.setTimeout(() => {
        setIsOpen(false);
      }, 160);
    }
  };

  // Keep position accurate on scroll & resize
  useEffect(() => {
    if (!isOpen) return;

    calculatePosition();

    const handleScroll = () => {
      calculatePosition();
    };

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setIsOpen(false);
      }
    };

    window.addEventListener('scroll', handleScroll, true);
    window.addEventListener('resize', handleScroll);
    window.addEventListener('keydown', handleKeyDown);

    return () => {
      window.removeEventListener('scroll', handleScroll, true);
      window.removeEventListener('resize', handleScroll);
      window.removeEventListener('keydown', handleKeyDown);
    };
  }, [isOpen, calculatePosition]);

  // Click outside listener
  useEffect(() => {
    if (!isOpen) return;

    const handleDocumentClick = (e: MouseEvent) => {
      const target = e.target as Node;
      if (
        triggerRef.current?.contains(target) ||
        tooltipRef.current?.contains(target)
      ) {
        return;
      }
      setIsOpen(false);
    };

    document.addEventListener('mousedown', handleDocumentClick);
    return () => {
      document.removeEventListener('mousedown', handleDocumentClick);
    };
  }, [isOpen]);

  const handleCopyQuadlet = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!quadlet) return;
    navigator.clipboard.writeText(quadlet);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <div className="inline-flex items-center ml-1.5 align-middle select-none">
      <button
        ref={triggerRef}
        type="button"
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          if (isOpen) {
            handleClose(true);
          } else {
            handleOpen();
          }
        }}
        onMouseDown={(e) => {
          // Prevent activating parent labels/checkboxes
          e.preventDefault();
          e.stopPropagation();
        }}
        onMouseEnter={handleOpen}
        onMouseLeave={() => handleClose(false)}
        aria-label={`Info: ${title}`}
        className={`p-0.5 rounded-full transition-all focus:outline-none focus:ring-1 focus:ring-[var(--accent-mauve)]/50 cursor-pointer ${
          isOpen
            ? 'text-[var(--accent-mauve)] bg-[var(--accent-mauve)]/10'
            : 'text-[var(--text-muted)] hover:text-[var(--accent-mauve)] hover:bg-[var(--bg-surface0)]'
        }`}
      >
        <HelpCircle className="w-3.5 h-3.5" />
      </button>

      {isOpen &&
        createPortal(
          <div
            ref={tooltipRef}
            style={{
              position: 'fixed',
              top: `${coords.top}px`,
              left: `${coords.left}px`,
              transform: coords.placement === 'top' ? 'translateY(-100%)' : 'none',
              maxWidth: 'calc(100vw - 24px)',
              width: '320px',
              zIndex: 99999,
            }}
            onMouseEnter={handleOpen}
            onMouseLeave={() => handleClose(false)}
            onClick={(e) => e.stopPropagation()}
            role="tooltip"
            className="p-3.5 rounded-[4px] bg-[var(--bg-mantle)]/95 border border-[var(--border-focus)] shadow-2xl text-left pointer-events-auto backdrop-blur-md animate-in fade-in zoom-in-95 duration-100 ring-1 ring-black/40"
          >
            {/* Header */}
            <div className="flex items-start justify-between gap-2 pb-2 mb-2 border-b border-[var(--border)]">
              <div className="flex items-center gap-1.5 flex-wrap">
                {section && (
                  <span className="text-[10px] font-mono px-1.5 py-0.5 rounded-[2px] bg-[var(--bg-surface0)] text-[var(--accent-mauve)] font-medium">
                    {section}
                  </span>
                )}
                <span className="text-xs font-bold text-[var(--text-primary)] font-mono">
                  {title}
                </span>
              </div>
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  handleClose(true);
                }}
                className="text-[var(--text-muted)] hover:text-[var(--text-primary)] p-0.5 rounded transition-colors"
                aria-label="Close tooltip"
              >
                <X className="w-3.5 h-3.5" />
              </button>
            </div>

            {/* Description */}
            <p className="text-xs text-[var(--text-subtext)] leading-relaxed mb-2.5 font-sans">
              {description}
            </p>

            {/* Quadlet mapping */}
            {quadlet && (
              <div className="mt-2 pt-2 border-t border-[var(--border)]/60 text-[11px]">
                <div className="flex items-center justify-between text-[var(--accent-peach)] font-medium mb-1">
                  <div className="flex items-center gap-1">
                    <Terminal className="w-3 h-3 shrink-0" />
                    <span>Quadlet directive:</span>
                  </div>
                  <button
                    type="button"
                    onClick={handleCopyQuadlet}
                    className="flex items-center gap-1 text-[10px] text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors px-1 py-0.5 rounded bg-[var(--bg-surface0)] cursor-pointer"
                    title="Copy Quadlet directive"
                  >
                    {copied ? (
                      <>
                        <Check className="w-2.5 h-2.5 text-[var(--accent-green)]" />
                        <span className="text-[var(--accent-green)]">Copied</span>
                      </>
                    ) : (
                      <>
                        <Copy className="w-2.5 h-2.5" />
                        <span>Copy</span>
                      </>
                    )}
                  </button>
                </div>
                <pre className="p-1.5 rounded-[3px] bg-[var(--bg-crust)] border border-[var(--border)] text-[10.5px] font-mono text-[var(--accent-teal)] overflow-x-auto whitespace-pre-wrap break-all select-all">
                  {quadlet}
                </pre>
              </div>
            )}

            {/* Security note */}
            {security && (
              <div className="mt-2 pt-2 border-t border-[var(--border)]/60 flex items-start gap-1.5 text-[11px] text-[var(--accent-yellow)]">
                <ShieldAlert className="w-3.5 h-3.5 shrink-0 mt-0.5" />
                <span className="leading-snug">{security}</span>
              </div>
            )}
          </div>,
          document.body
        )}
    </div>
  );
}
