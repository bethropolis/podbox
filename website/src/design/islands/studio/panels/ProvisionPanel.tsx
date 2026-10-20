import React, { useEffect, useState } from 'react';
import {
  Terminal,
  Plus,
  Trash2,
  ChevronDown,
} from 'lucide-react';
import {
  STUDIO_FIELD,
  StudioInput,
  StudioSelect,
} from '../../../components/StudioControls';
import { StudioTooltip } from '../../../components/StudioTooltip';
import { provisionOnFailureOptions, provisionTimeout } from '../schema';
import type { EnvVarItem, ProvisionItem } from '../types';
import type { StudioState } from '../useStudioState';

type ProvisionStepsProps = Pick<StudioState, 'provisionSteps' | 'setProvisionSteps'>;

const blankStep = (): ProvisionItem => ({
  name: '',
  run: '',
  timeout: provisionTimeout(),
  onFailure: 'warn',
  envList: [],
  root: false,
});

export function ProvisionSteps({
  st,
  errorMap,
}: {
  st: ProvisionStepsProps;
  errorMap?: Record<string, string>;
}) {
  const { provisionSteps, setProvisionSteps } = st;

  // Collapse state, keyed by step name and persisted across sessions.
  // `true` = open. Names never seen default to collapsed; untitled rows
  // default to open so a fresh step never shuts while being named. A
  // corrupted payload is ignored outright.
  const STORAGE_KEY = 'podbox-studio-provision-v1';

  const loadOpen = (): Record<string, boolean> => {
    try {
      if (typeof localStorage === 'undefined') return {};
      const raw = localStorage.getItem(STORAGE_KEY);
      if (!raw) return {};
      const parsed: unknown = JSON.parse(raw);
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return {};
      const clean: Record<string, boolean> = {};
      for (const [k, v] of Object.entries(parsed as Record<string, unknown>)) {
        if (typeof v === 'boolean') clean[k] = v;
      }
      return clean;
    } catch {
      return {};
    }
  };

  const [openMap, setOpenMap] = useState<Record<string, boolean>>(loadOpen);

  // Persist on change; private mode / quota failures just don't persist.
  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(openMap));
    } catch {
      // Session simply won't persist.
    }
  }, [openMap]);

  const isUntitled = (name: string) => name.trim() === '';
  const isOpen = (step: ProvisionItem) =>
    isUntitled(step.name) ? (openMap[''] ?? true) : (openMap[step.name] ?? false);
  const toggle = (idx: number) => {
    const step = provisionSteps[idx];
    setOpenMap((prev) => ({ ...prev, [step.name]: !isOpen(step) }));
  };

  // A step carrying an engine error must be visible: open it so the
  // red message under the field can actually be seen.
  useEffect(() => {
    if (!errorMap) return;
    setOpenMap((prev) => {
      const next = { ...prev };
      let changed = false;
      provisionSteps.forEach((step, idx) => {
        if (isUntitled(step.name)) return; // already open
        const hasError = Object.keys(errorMap).some((f) => f.startsWith(`provision[${idx}]`));
        if (hasError && next[step.name] !== true) {
          next[step.name] = true;
          changed = true;
        }
      });
      return changed ? next : prev;
    });
    // Only the error keys matter here, not the step contents, so the
    // effect depends on errorMap alone.
  }, [errorMap]);

  const patch = (idx: number, changes: Partial<ProvisionItem>) => {
    const step = provisionSteps[idx];
    if (changes.name !== undefined && changes.name !== step.name) {
      const from = step.name;
      const to = changes.name;
      setOpenMap((prev) => {
        const next = { ...prev };
        if (from in next) {
          // Carry the row's state across the rename — including '' → name,
          // which keeps a fresh step open while it is being named.
          if (to !== from) {
            next[to] = next[from];
            delete next[from];
          }
        } else if (isUntitled(from) && !isUntitled(to)) {
          // Fresh step named for the first time: stay open.
          next[to] = true;
        }
        return next;
      });
    }
    setProvisionSteps(
      provisionSteps.map((s, i) => (i === idx ? { ...s, ...changes } : s)),
    );
  };

  const addStep = () => {
    const idx = provisionSteps.length;
    // Fresh rows open: drop any '' entry back to the untitled default.
    setOpenMap((prev) => {
      if (!('' in prev)) return prev;
      const next = { ...prev };
      delete next[''];
      return next;
    });
    setProvisionSteps([...provisionSteps, blankStep()]);
    // Focus lands in the new step's name field once it renders.
    window.setTimeout(() => {
      document.getElementById(`studio-input-provision-${idx}-name`)?.focus();
    }, 50);
  };
  const removeStep = (idx: number) => {
    const remaining = provisionSteps.filter((_, i) => i !== idx);
    // Prune names that no longer exist so the stored object can't grow stale.
    const names = new Set(remaining.map((s) => s.name));
    setOpenMap((prev) => {
      const next: Record<string, boolean> = {};
      for (const [k, v] of Object.entries(prev)) {
        if (names.has(k)) next[k] = v;
      }
      return next;
    });
    setProvisionSteps(remaining);
  };

  const addEnv = (idx: number) =>
    patch(idx, { envList: [...provisionSteps[idx].envList, { key: '', value: '' }] });
  const patchEnv = (idx: number, eidx: number, changes: Partial<EnvVarItem>) =>
    patch(idx, {
      envList: provisionSteps[idx].envList.map((e, i) => (i === eidx ? { ...e, ...changes } : e)),
    });
  const removeEnv = (idx: number, eidx: number) =>
    patch(idx, { envList: provisionSteps[idx].envList.filter((_, i) => i !== eidx) });

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2">
        <Terminal className="w-4 h-4 text-[var(--accent-mauve)]" />
        <h3 className="text-sm font-bold text-[var(--text-primary)] font-mono">
          [[provision]] steps
        </h3>
        <div className="flex-1" />
        {provisionSteps.length > 1 && (
          <div className="flex items-center gap-1 text-[11px] font-mono">
            <button
              type="button"
              onClick={() => {
                const all: Record<string, boolean> = {};
                for (const s of provisionSteps) all[s.name] = false;
                setOpenMap(all);
              }}
              className="px-1.5 py-0.5 rounded-[2px] text-[var(--text-subtext)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-base)] transition-colors cursor-pointer"
            >
              collapse all
            </button>
            <span className="text-[var(--border)]">/</span>
            <button
              type="button"
              onClick={() => {
                const all: Record<string, boolean> = {};
                for (const s of provisionSteps) all[s.name] = true;
                setOpenMap(all);
              }}
              className="px-1.5 py-0.5 rounded-[2px] text-[var(--text-subtext)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-base)] transition-colors cursor-pointer"
            >
              expand all
            </button>
          </div>
        )}
      </div>
      <p className="text-xs text-[var(--text-subtext)] font-sans">
        Commands that run once inside the container, in order, after your dotfiles land.
        Each one is stamped when it succeeds, so a re-run only repeats what changed.
        Run them with <code className="font-mono">podbox provision sync</code>.
      </p>

      {provisionSteps.length === 0 ? (
        <button
          type="button"
          onClick={addStep}
          className="w-full flex items-center justify-center gap-2 px-3 py-3 rounded-[2px] border border-dashed border-[var(--border)] text-xs font-mono text-[var(--text-subtext)] hover:text-[var(--text-primary)] hover:border-[var(--accent-mauve)]/50 transition-colors cursor-pointer"
        >
          <Plus className="w-3.5 h-3.5" />
          Add first step
        </button>
      ) : (
        <div className="space-y-3">
          {provisionSteps.map((step, idx) => {
            const open = isOpen(step);
            const title = step.name.trim() || 'Untitled step';
            const runPreview = step.run.trim();
            const badges: string[] = [];
            if (step.onFailure === 'abort') badges.push('abort');
            if (step.root) badges.push('root');
            if (step.timeout.trim() && step.timeout.trim() !== provisionTimeout()) {
              badges.push(step.timeout.trim());
            }
            if (step.envList.some((e) => e.key.trim())) {
              const n = step.envList.filter((e) => e.key.trim()).length;
              badges.push(`env×${n}`);
            }
            return (
              <div
                key={idx}
                className="rounded-[3px] border border-[var(--border)] bg-[var(--bg-mantle)]/30 overflow-hidden"
              >
                {/* Collapsible header: number · name · run preview · badges */}
                <div className="flex items-center gap-1.5 px-3 py-2">
                  <button
                    type="button"
                    onClick={() => toggle(idx)}
                    aria-expanded={open}
                    title={open ? 'Collapse step' : 'Expand step'}
                    className="flex-1 min-w-0 flex items-center gap-2 text-left cursor-pointer group"
                  >
                    <ChevronDown
                      className={`w-3.5 h-3.5 shrink-0 text-[var(--text-subtext)] transition-transform duration-150 ${open ? '' : '-rotate-90'}`}
                    />
                    <span className="text-xs font-mono text-[var(--accent-mauve)] font-bold shrink-0">
                      {idx + 1}
                    </span>
                    <span className="text-xs font-mono text-[var(--text-primary)] truncate">
                      {title}
                    </span>
                    {!open && runPreview && (
                      <span className="hidden sm:inline text-[11px] font-mono text-[var(--text-subtext)] truncate">
                        — {runPreview}
                      </span>
                    )}
                    {badges.length > 0 && (
                      <span className="hidden md:inline-flex items-center gap-1 shrink-0">
                        {badges.map((b) => (
                          <span
                            key={b}
                            className="px-1 py-px rounded-[2px] text-[10px] font-mono bg-[var(--bg-base)] border border-[var(--border)] text-[var(--text-subtext)]"
                          >
                            {b}
                          </span>
                        ))}
                      </span>
                    )}
                  </button>
                  <button
                    type="button"
                    onClick={() => removeStep(idx)}
                    title="Remove this step"
                    className="p-1.5 rounded-[2px] text-[var(--text-subtext)] hover:text-[var(--accent-red)] hover:bg-[var(--accent-red)]/10 transition-colors cursor-pointer shrink-0"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>

                {open && (
                  <div className="px-4 pb-4 pt-1 space-y-4 border-t border-[var(--border)]/60">
                    <div className="grid grid-cols-1 sm:grid-cols-[1fr_120px] gap-4 pt-3">
                      <StudioInput
                        label={
                          <div className="flex items-center">
                            <span>Name</span>
                            <StudioTooltip
                              section="[[provision]]"
                              title={`name = "${step.name || 'my-step'}"`}
                              description="Unique per container. Used as the stamp key, so renaming a step re-runs it."
                            />
                          </div>
                        }
                        value={step.name}
                        onChange={(e) =>
                          patch(idx, {
                            name: e.target.value
                              .toLowerCase()
                              .replace(/[^a-z0-9_-]/g, '')
                              .slice(0, 64),
                          })
                        }
                        placeholder="rust-toolchain"
                        id={`studio-input-provision-${idx}-name`}
                        error={errorMap?.[`provision[${idx}].name`]}
                      />

                      <StudioInput
                        label={
                          <div className="flex items-center">
                            <span>Timeout</span>
                            <StudioTooltip
                              section="[[provision]]"
                              title={`timeout = "${step.timeout}"`}
                              description="Max runtime before the step counts as failed. Leave at 5m unless it is genuinely slower."
                            />
                          </div>
                        }
                        value={step.timeout}
                        onChange={(e) => patch(idx, { timeout: e.target.value })}
                        placeholder="5m"
                        id={`studio-input-provision-${idx}-timeout`}
                        error={errorMap?.[`provision[${idx}].timeout`]}
                      />
                    </div>

                    <div className="space-y-1.5 w-full">
                      <div className="text-xs font-medium text-[var(--text-subtext)] flex items-center flex-wrap gap-1">
                        <div className="flex items-center">
                          <span>Run</span>
                          <StudioTooltip
                            section="[[provision]]"
                            title='run = "mise use -g rust@stable"'
                            description="One command or a small script, run with sh -c from your container home. Shell profiles are not sourced — use absolute paths or bash -lc."
                          />
                        </div>
                      </div>
                      <textarea
                        value={step.run}
                        onChange={(e) => patch(idx, { run: e.target.value })}
                        placeholder={"set -e\nmise use -g rust@stable"}
                        id={`studio-input-provision-${idx}-run`}
                        rows={3}
                        spellCheck={false}
                        autoComplete="off"
                        autoCorrect="off"
                        autoCapitalize="off"
                        className={`${STUDIO_FIELD} w-full font-mono text-xs resize-y min-h-[52px] leading-relaxed ${
                          errorMap?.[`provision[${idx}].run`]
                            ? '!border-[var(--accent-red)]'
                            : ''
                        }`}
                      />
                      {errorMap?.[`provision[${idx}].run`] && (
                        <p className="text-[11px] text-[var(--accent-red)] font-mono mt-1">
                          {errorMap[`provision[${idx}].run`]}
                        </p>
                      )}
                    </div>

                    <div className="flex flex-col sm:flex-row gap-4">
                      <div className="flex-1">
                        <StudioSelect
                          label={
                            <div className="flex items-center">
                              <span>On Failure</span>
                              <StudioTooltip
                                section="[[provision]]"
                                title={`on_failure = "${step.onFailure}"`}
                                description="warn keeps going with the next step; abort stops the whole run."
                              />
                            </div>
                          }
                          value={step.onFailure}
                          onChange={(v) => patch(idx, { onFailure: v as ProvisionItem['onFailure'] })}
                          options={provisionOnFailureOptions()}
                        />
                      </div>

                      {/* Inline root flag: same pill language as StudioSwitch
                          but no bordered card, so it sits naturally beside the
                          select. The tooltip is a sibling of the button — never
                          nested inside a label — so it can't swallow clicks. */}
                      <div className="flex items-end pb-2">
                        <div className="flex items-center gap-2">
                          <button
                            type="button"
                            role="switch"
                            aria-checked={step.root}
                            aria-label="Run as root"
                            onClick={() => patch(idx, { root: !step.root })}
                            className={`w-9 h-5 rounded-full transition-colors duration-150 relative flex items-center p-0.5 shrink-0 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-mauve)]/60 ${
                              step.root
                                ? 'bg-[var(--accent-mauve)] shadow-[0_0_8px_rgba(203,166,247,0.35)]'
                                : 'bg-[var(--bg-surface1)]'
                            }`}
                          >
                            <div
                              className={`w-4 h-4 rounded-full bg-white dark:bg-[var(--bg-crust)] shadow-sm transform transition-transform duration-200 ease-in-out ${
                                step.root ? 'translate-x-4' : 'translate-x-0'
                              }`}
                            />
                          </button>
                          <span className="text-xs font-medium text-[var(--text-primary)]">
                            Run as root
                          </span>
                          <StudioTooltip
                            section="[[provision]]"
                            title={step.root ? 'root = true' : 'root = false'}
                            description="Only for system packages or system-wide config. Otherwise it leaves root-owned files in your home."
                          />
                        </div>
                      </div>
                    </div>

                    {/* Per-step environment */}
                    <div className="space-y-2.5">
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-mono text-[var(--text-subtext)]">env</span>
                        <div className="flex-1 border-t border-[var(--border)]" />
                        <button
                          type="button"
                          onClick={() => addEnv(idx)}
                          className="flex items-center gap-1 px-2 py-1 rounded-[2px] text-xs font-mono text-[var(--text-subtext)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-base)] transition-colors cursor-pointer"
                        >
                          <Plus className="w-3 h-3" />
                          add
                        </button>
                      </div>
                      {step.envList.map((e, eidx) => (
                        <div key={eidx} className="flex items-center gap-2">
                          <input
                            value={e.key}
                            onChange={(ev) => patchEnv(idx, eidx, { key: ev.target.value })}
                            placeholder="KEY"
                            className="flex-1 min-w-0 px-2.5 py-1.5 rounded-[2px] border border-[var(--border)] bg-[var(--bg-base)] text-xs font-mono text-[var(--text-primary)] placeholder:text-[var(--text-subtext)]/50 focus:outline-none focus:border-[var(--accent-mauve)]/60 transition-colors"
                          />
                          <span className="text-xs font-mono text-[var(--text-subtext)]">=</span>
                          <input
                            value={e.value}
                            onChange={(ev) => patchEnv(idx, eidx, { value: ev.target.value })}
                            placeholder="value"
                            className="flex-1 min-w-0 px-2.5 py-1.5 rounded-[2px] border border-[var(--border)] bg-[var(--bg-base)] text-xs font-mono text-[var(--text-primary)] placeholder:text-[var(--text-subtext)]/50 focus:outline-none focus:border-[var(--accent-mauve)]/60 transition-colors"
                          />
                          <button
                            type="button"
                            onClick={() => removeEnv(idx, eidx)}
                            title="Remove this variable"
                            className="p-1.5 rounded-[2px] text-[var(--text-subtext)] hover:text-[var(--accent-red)] hover:bg-[var(--accent-red)]/10 transition-colors cursor-pointer"
                          >
                            <Trash2 className="w-3 h-3" />
                          </button>
                        </div>
                      ))}
                      {step.envList.length === 0 && (
                        <p className="text-xs text-[var(--text-subtext)] font-sans">
                          Extra variables for this step only. All values are plaintext — never put
                          a secret here.
                        </p>
                      )}
                    </div>
                  </div>
                )}
              </div>
            );
          })}

          <button
            type="button"
            onClick={addStep}
            className="w-full flex items-center justify-center gap-2 px-3 py-2 rounded-[2px] border border-dashed border-[var(--border)] text-xs font-mono text-[var(--text-subtext)] hover:text-[var(--text-primary)] hover:border-[var(--accent-mauve)]/50 transition-colors cursor-pointer"
          >
            <Plus className="w-3.5 h-3.5" />
            Add step
          </button>
        </div>
      )}
    </div>
  );
}
