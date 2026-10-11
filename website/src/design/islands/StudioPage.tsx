import React, { useEffect, useMemo, useState } from 'react';
import { X } from 'lucide-react';
import { SettingsPalette } from './studio/SettingsPalette';
import { useStudioState } from './studio/useStudioState';
import { useWasm } from './studio/useWasm';
import { generateStudioToml } from './studio/generateToml';
import { generateStudioQuadlet } from './studio/generateQuadlet';
import { tomlFieldLines } from './studio/tomlLines';
import { tomlToPatch } from './studio/parseImport';
import { decodeShare, markSharedLoadPending, shareFromHash } from './studio/share';
import { STUDIO_DEFAULTS } from './studio/schema';
import { defaultPatch, presetPatch } from './studio/presets';
import { StudioHeader } from './studio/StudioHeader';
import { CategoryTabs } from './studio/CategoryTabs';
import { ImagePanel } from './studio/panels/ImagePanel';
import { ContainerPanel } from './studio/panels/ContainerPanel';
import { DotfilesPanel } from './studio/panels/DotfilesPanel';
import { StoragePanel } from './studio/panels/StoragePanel';
import { SecurityPanel } from './studio/panels/SecurityPanel';
import { NetworkPanel } from './studio/panels/NetworkPanel';
import { IntegrationPanel } from './studio/panels/IntegrationPanel';
import { LifecyclePanel } from './studio/panels/LifecyclePanel';
import { ProxiesPanel } from './studio/panels/ProxiesPanel';
import { CodePreview } from './studio/CodePreview';

interface StudioPageProps {
  // Navigation is plain MPA links; no callback props cross the Astro boundary.
}

export function StudioPage(_props: StudioPageProps) {
  const st = useStudioState();
  const { isFullscreen, activeCategory } = st;
  const toml = generateStudioToml(st.values);

  // Rust engine (wasm): exact CLI validation + Quadlet codegen. TS template
  // stays as the fallback until wasm initializes (or if it fails to load).
  const { ready: wasmReady, validate, compileQuadlet, parseToml, fieldLines } = useWasm();

  // Shared link (`#c=…`). Decoding is pure JS — no wasm — so it starts on
  // mount and finishes long before the engine loads. The parse needs the
  // engine (migrations + defaults), so that waits for it. `useStudioState`
  // sees the hash and skips its localStorage restore, so nothing here can
  // be overwritten by a saved session.
  const [shared, setShared] = useState<
    { kind: 'none' } | { kind: 'pending' } | { kind: 'ok'; toml: string } | { kind: 'bad' }
  >({ kind: 'none' });
  const [shareNotice, setShareNotice] = useState('');

  useEffect(() => {
    const payload = shareFromHash(window.location.hash);
    if (payload == null) return;
    setShared({ kind: 'pending' });
    markSharedLoadPending(true);
    // The counter is released exactly once: by the decode landing, or by the
    // cleanup if the island unmounts first (StrictMode's double-mount in dev
    // does exactly that). State is only written while still mounted.
    let released = false;
    let live = true;
    const release = () => {
      if (released) return;
      released = true;
      markSharedLoadPending(false);
    };
    void decodeShare(payload).then((toml) => {
      release();
      if (!live) return;
      setShared(toml == null ? { kind: 'bad' } : { kind: 'ok', toml });
    });
    return () => {
      live = false;
      release();
    };
  }, []);

  useEffect(() => {
    if (shared.kind === 'none' || shared.kind === 'pending') return;
    if (shared.kind === 'bad') {
      // Unreadable link: drop it and reload, which takes the normal
      // saved-session path. Better than silently showing defaults.
      window.location.replace(window.location.pathname);
      return;
    }
    if (!wasmReady) return;
    const parsed = parseToml(shared.toml);
    if (parsed && typeof parsed === 'object') {
      st.applyPatch(tomlToPatch(parsed as Record<string, any>));
      setShareNotice('Loaded shared configuration');
    } else {
      window.location.replace(window.location.pathname);
    }
    // Depends on `shared`/`wasmReady`, both signals for "apply now".
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shared, wasmReady]);
  // First config change after a shared load clears the notice.
  useEffect(() => {
    if (!shareNotice) return;
    setShareNotice('');
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [toml]);

  // Cmd+K belongs to the settings palette while Studio is on screen. The
  // listener is capture-phase so it runs before the site-wide palette's
  // bubble listener; stopPropagation keeps that second palette closed
  // instead of stacking two dialogs.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        e.stopPropagation();
        setPaletteOpen((open) => !open);
      }
    };
    window.addEventListener('keydown', onKeyDown, { capture: true });
    return () => window.removeEventListener('keydown', onKeyDown, { capture: true });
  }, []);

  const validation = validate(toml);
  const compiled = compileQuadlet(toml);
  const quadlet = compiled?.container ?? generateStudioQuadlet(st.values);
  const containerfile = compiled?.containerfile ?? null;
  const hostService = compiled?.host_service ?? null;
  const proxyService = compiled?.dbus_proxy_service ?? null;
  const compositorService = compiled?.compositor_service ?? null;
  const warnings = [...(validation?.warnings ?? []), ...(compiled?.warnings ?? [])];
  const engine = compiled ? 'rust' : 'ts';
  const errorMap = validation?.errorMap ?? {};
  // Advisories keep their tab badge but never the red banner or field border.
  const errors = validation?.errors ?? [];

  // TOML source map: wasm when ready, TS mirror before that. Inverted for
  // click-to-edit (line -> field).
  const fieldMap = useMemo(
    () => fieldLines(toml) ?? tomlFieldLines(toml),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [toml, wasmReady],
  );
  const lineFields = useMemo(() => {
    const inv: Record<number, string> = {};
    for (const [field, line] of Object.entries(fieldMap)) inv[line] ??= field;
    return inv;
  }, [fieldMap]);
  const errorLines = useMemo(() => {
    const out: Record<number, string> = {};
    for (const issue of errors) {
      if (issue.line != null) out[issue.line] ??= issue.message;
      else if (issue.field && fieldMap[issue.field] != null) out[fieldMap[issue.field]] ??= issue.message;
    }
    return out;
  }, [errors, fieldMap]);

  // Banner click scrolls the TOML view; form hover/focus highlights it.
  const [focusLine, setFocusLine] = useState<number | null>(null);
  const [hoverLine, setHoverLine] = useState<number | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  // Orphans own the banner: anything with a field or line already renders
  // inline (tab badge, input message, code squiggle).
  const orphanErrors = errors.filter((issue) => {
    const line = issue.line ?? (issue.field ? fieldMap[issue.field] : undefined);
    return !issue.field && line == null;
  });

  // `studio-input-*` ids encode the dotted field with `-` separators
  // (`studio-input-container-memory`, `studio-input-provision-0-name`).
  const fieldFromInputId = (id: string): string | null => {
    if (!id.startsWith('studio-input-')) return null;
    const parts = id.slice('studio-input-'.length).split('-').filter((p) => !/^\d+$/.test(p));
    return parts.length > 0 ? parts.join('.') : null;
  };
  const onFormHover = (e: React.MouseEvent | React.FocusEvent) => {
    const el = (e.target as HTMLElement).closest?.('[id^="studio-input-"]');
    const field = el?.id ? fieldFromInputId(el.id) : null;
    setHoverLine(field && fieldMap[field] != null ? fieldMap[field] : null);
  };

  // Banner click-to-focus: jump to the tab owning the field, then focus it.
  // Issues with a TOML line also scroll the code view there.
  const focusField = (field: string | undefined, line?: number) => {
    if (line != null) {
      st.setActiveView('toml');
      setFocusLine(line);
    }
    if (!field) return;
    // `[[provision]]` steps live in the dotfiles panel; their engine fields
    // look like `provision[0].name` and their inputs are id'd per step.
    const provisionMatch = field.match(/^provision\[(\d+)\]\.(.+)$/);
    const targetId = provisionMatch
      ? `studio-input-provision-${provisionMatch[1]}-${provisionMatch[2]}`
      : `studio-input-${field.replaceAll('.', '-')}`;
    // Tabs currently rendered; fields from other sections skip the switch
    // (their banner entry still shows) and go straight to the focus attempt.
    const known = ['image', 'container', 'dotfiles', 'storage', 'security', 'network', 'integration', 'lifecycle', 'proxies'];
    const category = provisionMatch ? 'dotfiles' : (field.split('.')[0] === 'dbus' || field.split('.')[0] === 'wayland' ? 'proxies' : field.split('.')[0]);
    if (known.includes(category) && category !== st.activeCategory) {
      st.setActiveCategory(category as typeof st.activeCategory);
    }
    // Panels render after the category switch; wait a tick before focusing.
    // A step carrying the error opens itself (see ProvisionSteps), but the
    // state flip needs a beat to land before the element exists.
    window.setTimeout(() => {
      document.getElementById(targetId)?.focus();
    }, 120);
  };

  return (
<div
  data-page-palette
  className={`w-full font-sans text-[var(--text-primary)] transition-all ${
    st.isFullscreen
      ? 'fixed inset-0 z-50 bg-[var(--bg-base)] flex flex-col p-1 sm:p-1.5 h-screen max-h-screen overflow-hidden'
      : 'max-w-7xl mx-auto px-4 sm:px-6 py-4 sm:py-6 lg:h-[calc(100vh-4.5rem)] lg:flex lg:flex-col lg:overflow-hidden'
  }`}
>
  {/* -------------------------------------------------------------------- */}
      <StudioHeader st={st} toml={toml} quadlet={quadlet} containerfile={containerfile} />

{/* Shared-link notice. One line, self-clearing: it confirms the load that
    the hash triggered and says nothing once you start editing. */}
{shareNotice && (
  <div className={`${isFullscreen ? 'mb-2' : 'mb-4'}`}>
    <div className="px-3 py-1.5 rounded-[2px] bg-[var(--accent-green)]/10 border border-[var(--accent-green)]/30 text-[var(--accent-green)] font-mono text-xs flex items-center gap-2">
      <span>{shareNotice}</span>
      <button
        type="button"
        onClick={() => {
          window.history.replaceState(null, '', window.location.pathname);
          setShareNotice('');
        }}
        className="text-[var(--text-muted)] hover:text-[var(--text-primary)] cursor-pointer ml-auto"
        title="Clear the link from the address bar"
      >
        <X className="w-3 h-3" />
      </button>
    </div>
  </div>
)}

{/* Live diagnostics from the Rust engine (validation errors + codegen warnings).
    Field errors render inline (tab badge, input message, code squiggle), so
    the banner only keeps orphans with neither field nor line. */}
{wasmReady && validation && (orphanErrors.length > 0 || warnings.length > 0) && (
  <div className={`${isFullscreen ? 'mb-2' : 'mb-4'} space-y-1.5 font-mono text-xs`}>
    {orphanErrors.map((issue, idx) => (
      <button
        key={`e-${idx}`}
        type="button"
        onClick={() => focusField(issue.field, issue.line)}
        className="w-full text-left px-3 py-2 rounded-[2px] bg-[var(--accent-red)]/10 border border-[var(--accent-red)]/40 text-[var(--accent-red)] hover:bg-[var(--accent-red)]/15 transition-colors cursor-pointer"
      >
        ✘ {issue.message}
      </button>
    ))}
    {warnings.map((msg, idx) => (
      <div key={`w-${idx}`} className="px-3 py-2 rounded-[2px] bg-[var(--accent-peach)]/10 border border-[var(--accent-peach)]/40 text-[var(--accent-peach)]">
        ⚠ {msg}
      </div>
    ))}
  </div>
)}

{/* 2. MAIN WORKSPACE (SETTINGS + LIVE PREVIEW)                           */}
{/* -------------------------------------------------------------------- */}
<div className={`grid grid-cols-1 lg:grid-cols-12 lg:items-stretch ${st.isFullscreen ? 'gap-1 sm:gap-1.5 flex-1 min-h-0 overflow-hidden' : 'gap-4 lg:flex-1 lg:min-h-0 lg:overflow-hidden'}`}>
  {/* LEFT COLUMN: Categories & Settings Form (7 cols) */}
  <div className="lg:col-span-7 flex flex-col min-h-0 h-full overflow-hidden rounded-[3px] border border-[var(--border)] bg-[var(--bg-mantle)]/40 shadow-sm">
          <CategoryTabs st={st} errorMap={errorMap} advisoryMap={validation?.advisoryMap ?? {}} />

{/* Form Content Body - Scrollable */}
<div
  className={`flex-1 min-h-0 overflow-y-auto space-y-6 scrollbar-slim bg-[var(--bg-base)] ${isFullscreen ? 'p-3' : 'p-4 sm:p-5'}`}
  onMouseOver={onFormHover}
  onMouseOut={(e) => {
    if (!(e.relatedTarget as HTMLElement)?.closest?.('[id^="studio-input-"]')) setHoverLine(null);
  }}
  onFocus={onFormHover}
  onBlur={() => setHoverLine(null)}
>
            {activeCategory === 'image' && <ImagePanel st={st} errorMap={errorMap} />}
            {activeCategory === 'container' && <ContainerPanel st={st} errorMap={errorMap} />}
            {activeCategory === 'dotfiles' && <DotfilesPanel st={st} errorMap={errorMap} />}
            {activeCategory === 'storage' && <StoragePanel st={st} />}
            {activeCategory === 'security' && <SecurityPanel st={st} />}
            {activeCategory === 'network' && <NetworkPanel st={st} errorMap={errorMap} />}
            {activeCategory === 'integration' && <IntegrationPanel st={st} errorMap={errorMap} />}
            {activeCategory === 'lifecycle' && <LifecyclePanel st={st} errorMap={errorMap} />}
            {activeCategory === 'proxies' && <ProxiesPanel st={st} />}
          </div>
        </div>

        {/* "What changed" baseline: the active preset (or blank defaults for
            custom) rendered through the same generator, so the diff shows
            real drift instead of formatting noise. */}
        {(() => {
          const baselineValues = { ...STUDIO_DEFAULTS, ...(st.values.activePreset !== 'custom' ? presetPatch(st.values.activePreset as Parameters<typeof presetPatch>[0]) : defaultPatch()) } as typeof st.values;
          const baselineToml = generateStudioToml(baselineValues);
          const baselineLabel = st.values.activePreset !== 'custom' ? st.values.activePreset : 'defaults';
          return (
            <CodePreview
              st={st}
              toml={toml}
              quadlet={quadlet}
              containerfile={containerfile}
              hostService={hostService}
              proxyService={proxyService}
              compositorService={compositorService}
              engine={engine}
              errorLines={errorLines}
              lineFields={lineFields}
              hoverLine={hoverLine}
              focusLine={focusLine}
              onFocusLineConsumed={() => setFocusLine(null)}
              onLineClick={(field) => focusField(field, fieldMap[field])}
              baselineToml={baselineToml}
              baselineLabel={baselineLabel}
              onDraftValidate={(draft) => validate(draft)?.errors[0]?.message ?? null}
              onApplyToml={(draft) => {
                const parsed = parseToml(draft);
                if (!parsed || typeof parsed !== 'object') return false;
                st.applyPatch(tomlToPatch(parsed as Record<string, any>));
                return true;
              }}
            />
          );
        })()}
      </div>

      {paletteOpen && (
        <SettingsPalette
          onClose={() => setPaletteOpen(false)}
          onPick={(field) => focusField(field, fieldMap[field])}
          onCategory={(category) => st.setActiveCategory(category)}
        />
      )}
    </div>
  );
}
