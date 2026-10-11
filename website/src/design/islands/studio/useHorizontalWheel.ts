import { useEffect, type RefObject } from 'react';

/**
 * Let a vertical wheel scroll an overflowing horizontal strip.
 *
 * Both tab strips hide their scrollbar, so plain wheel does nothing there.
 * A native (non-passive) listener translates vertical deltas into
 * `scrollLeft`; trackpad horizontal gestures and shift+wheel stay native,
 * and once the strip hits either end the event is left alone so the page
 * keeps scrolling.
 */
export function useHorizontalWheel(ref: RefObject<HTMLElement | null>) {
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (Math.abs(e.deltaX) >= Math.abs(e.deltaY)) return;
      const max = el.scrollWidth - el.clientWidth;
      if (max <= 0) return;
      // Clamp rather than bail on an overshooting tick: bailing meant the last
      // few pixels were unreachable, so the last tab could never be scrolled
      // into view. At a real edge we still let the event through so the page
      // scrolls underneath.
      const next = Math.min(max, Math.max(0, el.scrollLeft + e.deltaY));
      if (next === el.scrollLeft) return;
      e.preventDefault();
      el.scrollLeft = next;
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [ref]);
}
