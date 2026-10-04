import React, { createElement, useId, useState } from 'react';
import { ChevronDown, Plus, X } from 'lucide-react';

/**
 * Shared class for the hand-rolled text fields panels build (mount rows,
 * host-exec pairs, RUN commands) so they focus and hover exactly like
 * `StudioInput`. Focus paints a shadow rather than a ring+border combo, which
 * keeps the box from resizing as you tab through the form.
 */
export const STUDIO_FIELD =
  'px-2.5 py-1.5 text-xs font-mono rounded-[2px] bg-[var(--bg-mantle)] border text-[var(--text-primary)] ' +
  'placeholder-[var(--text-muted)] outline-none transition-[border-color,box-shadow] duration-150 ease-out ' +
  'border-[var(--border)] hover:border-[var(--border-focus)] focus:border-[var(--accent-mauve)] ' +
  'focus:shadow-[0_0_0_2px_rgba(203,166,247,0.22)]';

/* -------------------------------------------------------------------------- */
/* Switch Toggle                                                              */
/* -------------------------------------------------------------------------- */
interface StudioSwitchProps {
  id?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: React.ReactNode;
  description?: string;
  disabled?: boolean;
}

export function StudioSwitch({
  id,
  checked,
  onChange,
  label,
  description,
  disabled = false,
}: StudioSwitchProps) {
  return (
    <label
      htmlFor={id}
      className={`group flex items-start justify-between gap-3 p-2.5 rounded-[3px] border transition-[border-color,background-color] duration-150 ease-out cursor-pointer select-none ${
        checked
          ? 'bg-[var(--accent-mauve)]/5 border-[var(--accent-mauve)]/30'
          : 'bg-[var(--bg-mantle)] border-[var(--border)] hover:border-[var(--border-focus)]'
      } ${disabled ? 'opacity-50 cursor-not-allowed' : ''}`}
    >
      <div className="flex-1 min-w-0 pr-1">
        <div className="text-xs font-medium text-[var(--text-primary)] flex items-center flex-wrap gap-1">
          {label}
        </div>
        {description && (
          <p className="text-[11px] text-[var(--text-muted)] mt-0.5 leading-snug">
            {description}
          </p>
        )}
      </div>

      <div className="relative inline-flex items-center shrink-0 mt-0.5">
        <input
          type="checkbox"
          id={id}
          checked={checked}
          disabled={disabled}
          onChange={(e) => onChange(e.target.checked)}
          className="sr-only"
        />
        <div
          className={`w-9 h-5 rounded-full transition-colors duration-150 relative flex items-center p-0.5 ${
            checked
              ? 'bg-[var(--accent-mauve)] shadow-[0_0_8px_rgba(203,166,247,0.35)]'
              : 'bg-[var(--bg-surface1)]'
          }`}
        >
          <div
            className={`w-4 h-4 rounded-full bg-white dark:bg-[var(--bg-crust)] shadow-sm transform transition-transform duration-200 ease-in-out ${
              checked ? 'translate-x-4' : 'translate-x-0'
            }`}
          />
        </div>
      </div>
    </label>
  );
}

/* -------------------------------------------------------------------------- */
/* Text Input                                                                 */
/* -------------------------------------------------------------------------- */
interface StudioInputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  label?: React.ReactNode;
  prefixIcon?: React.ReactNode;
  helperText?: string;
  isMono?: boolean;
  error?: string;
}

export function StudioInput({
  label,
  prefixIcon,
  helperText,
  isMono = true,
  error,
  className = '',
  ...props
}: StudioInputProps) {
  return (
    <div className="space-y-1.5 w-full">
      {label && (
        <div className="text-xs font-medium text-[var(--text-subtext)] flex items-center flex-wrap gap-1">
          {label}
        </div>
      )}
      <div className="relative flex items-center">
        {prefixIcon && (
          <div className="absolute left-2.5 text-[var(--text-muted)] pointer-events-none">
            {prefixIcon}
          </div>
        )}
        <input
          {...props}
          spellCheck={props.spellCheck ?? false}
          autoComplete={props.autoComplete ?? 'off'}
          autoCorrect={props.autoCorrect ?? 'off'}
          autoCapitalize={props.autoCapitalize ?? 'off'}
          className={`w-full px-3 py-1.5 text-xs rounded-[2px] bg-[var(--bg-mantle)] border text-[var(--text-primary)] placeholder-[var(--text-muted)] outline-none transition-[border-color,box-shadow] duration-150 ease-out ${
            error
              ? 'border-[var(--accent-red)] focus:shadow-[0_0_0_2px_rgba(243,139,168,0.25)]'
              : 'border-[var(--border)] hover:border-[var(--border-focus)] focus:border-[var(--accent-mauve)] focus:shadow-[0_0_0_2px_rgba(203,166,247,0.22)]'
          } ${
            isMono ? 'font-mono' : 'font-sans'
          } ${prefixIcon ? 'pl-8' : ''} ${className}`}
        />
      </div>
      {error ? (
        <p className="text-[11px] text-[var(--accent-red)] font-mono mt-1">
          {error}
        </p>
      ) : helperText && (
        <p className="text-[11px] text-[var(--text-muted)] leading-tight">
          {helperText}
        </p>
      )}
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/* Select Dropdown                                                            */
/* -------------------------------------------------------------------------- */
interface StudioSelectOption {
  value: string;
  label: string;
  sublabel?: string;
  group?: string;
}

interface StudioSelectProps {
  label?: React.ReactNode;
  value: string;
  onChange: (value: string) => void;
  options: StudioSelectOption[];
  helperText?: string;
  error?: string;
}

export function StudioSelect({
  label,
  value,
  onChange,
  options,
  helperText,
  error,
}: StudioSelectProps) {
  // Option content is real markup so the closed button can show just the name
  // while the popup shows the sublabel too (see `.sel-*` in global.css).
  // Browsers without `appearance: base-select` flatten this to plain text and
  // concatenate the spans with nothing between them, so the separator is a
  // real character — Firefox ignores ::before on <option>.
  const renderOption = (opt: StudioSelectOption, key: string) => (
    <option key={key} value={opt.value}>
      <span className="sel-name">{opt.label}</span>
      {opt.sublabel && (
        <span className="sel-sub">
          {' '}
          {opt.sublabel}
        </span>
      )}
    </option>
  );

  // Grouped options render as <optgroup> so a long preset list stays
  // navigable instead of one flat wall.
  const groups = [...new Set(options.map((o) => o.group).filter(Boolean))] as string[];
  const ungrouped = options.filter((o) => !o.group);
  const body: React.ReactNode =
    groups.length === 0
      ? options.map((o) => renderOption(o, o.value))
      : [
          ...groups.map((group) => (
            <optgroup key={group} label={group}>
              {options.filter((o) => o.group === group).map((o) => renderOption(o, o.value))}
            </optgroup>
          )),
          ...(ungrouped.length > 0
            ? [
                <optgroup key="other" label="Other">
                  {ungrouped.map((o) => renderOption(o, o.value))}
                </optgroup>,
              ]
            : []),
        ];

  return (
    <div className="space-y-1.5 w-full">
      {label && (
        <div className="text-xs font-medium text-[var(--text-subtext)] flex items-center flex-wrap gap-1">
          {label}
        </div>
      )}
      <div className="relative">
        <select
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className={`studio-select w-full px-3 py-1.5 pr-8 text-xs font-mono rounded-[2px] bg-[var(--bg-mantle)] border text-[var(--text-primary)] outline-none transition-[border-color,box-shadow] duration-150 ease-out cursor-pointer ${
            error
              ? 'border-[var(--accent-red)] focus:shadow-[0_0_0_2px_rgba(243,139,168,0.25)]'
              : 'border-[var(--border)] hover:border-[var(--border-focus)] focus:border-[var(--accent-mauve)] focus:shadow-[0_0_0_2px_rgba(203,166,247,0.22)]'
          }`}
        >
          {createElement('button', null, createElement('selectedcontent'))}
          {body}
        </select>
        {/* Fallback arrow. `appearance: base-select` draws its own via
            ::picker-icon, so this is hidden wherever that is supported. */}
        <ChevronDown className="studio-select-chevron w-3.5 h-3.5 text-[var(--text-muted)] absolute right-2.5 top-1/2 -translate-y-1/2 pointer-events-none" />
      </div>
      {error ? (
        <p className="text-[11px] text-[var(--accent-red)] font-mono mt-1">
          {error}
        </p>
      ) : helperText && (
        <p className="text-[11px] text-[var(--text-muted)] leading-tight">
          {helperText}
        </p>
      )}
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/* Chip / Tag Array Input (Packages, Ports)                                   */
/* -------------------------------------------------------------------------- */
interface StudioTagInputProps {
  label?: React.ReactNode;
  tags: string[];
  onChange: (tags: string[]) => void;
  placeholder?: string;
  helperText?: string;
  error?: string;
  /** Values offered as native autocomplete; free text is always accepted. */
  suggestions?: string[];
}

export function StudioTagInput({
  label,
  tags,
  onChange,
  placeholder = 'Add item and press Enter...',
  helperText,
  error,
  suggestions,
}: StudioTagInputProps) {
  const [inputVal, setInputVal] = useState('');
  const [dupe, setDupe] = useState<string | null>(null);
  const listId = useId();

  // Split on the separators people actually paste with, so dropping a
  // comma-separated list into the field does the obvious thing.
  const splitInput = (raw: string) =>
    raw
      .split(/[\s,]+/)
      .map((s) => s.trim())
      .filter(Boolean);

  const addAll = (candidates: string[]) => {
    const fresh = candidates.filter((c) => !tags.includes(c));
    if (fresh.length === 0 && candidates.length > 0) {
      setDupe(candidates[0]);
      window.setTimeout(() => setDupe(null), 1400);
      return;
    }
    setDupe(null);
    if (fresh.length > 0) onChange([...tags, ...fresh]);
  };

  const handleAdd = () => {
    const parts = splitInput(inputVal);
    if (parts.length === 0) return;
    addAll(parts);
    setInputVal('');
  };

  const handlePaste = (e: React.ClipboardEvent<HTMLInputElement>) => {
    const pasted = splitInput(e.clipboardData.getData('text'));
    if (pasted.length === 0) return;
    e.preventDefault();
    addAll(pasted);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter' || e.key === ',') {
      e.preventDefault();
      handleAdd();
    } else if (e.key === 'Backspace' && !inputVal && tags.length > 0) {
      onChange(tags.slice(0, -1));
    }
  };

  const handleRemove = (index: number) => {
    onChange(tags.filter((_, i) => i !== index));
  };

  const matches = suggestions
    ? suggestions.filter((s) => !tags.includes(s) && s.includes(inputVal.toLowerCase())).slice(0, 8)
    : [];

  return (
    <div className="space-y-1.5 w-full">
      {label && (
        <div className="text-xs font-medium text-[var(--text-subtext)] flex items-center flex-wrap gap-1">
          {label}
        </div>
      )}
      <div className={`p-2 rounded-[2px] bg-[var(--bg-mantle)] border transition-all flex flex-wrap gap-1.5 items-center min-h-[36px] ${
        error
          ? 'border-[var(--accent-red)] ring-1 ring-[var(--accent-red)]/30'
          : 'border-[var(--border)] focus-within:border-[var(--accent-mauve)] focus-within:ring-1 focus-within:ring-[var(--accent-mauve)]/30'
      }`}>
        {tags.map((tag, idx) => (
          <span
            key={`${tag}-${idx}`}
            className="inline-flex items-center gap-1 px-2 py-0.5 rounded-[2px] bg-[var(--bg-surface0)] text-[var(--accent-mauve)] text-[11px] font-mono border border-[var(--border)] animate-fadeIn"
          >
            <span>{tag}</span>
            <button
              type="button"
              onClick={() => handleRemove(idx)}
              className="text-[var(--text-muted)] hover:text-[var(--accent-red)] p-0.5 rounded-full transition-colors cursor-pointer"
              title={`Remove ${tag}`}
            >
              <X className="w-2.5 h-2.5" />
            </button>
          </span>
        ))}
        <div className="flex-1 min-w-[120px] flex items-center gap-1">
          <input
            type="text"
            value={inputVal}
            list={suggestions ? listId : undefined}
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => setInputVal(e.target.value)}
            onPaste={handlePaste}
            onKeyDown={handleKeyDown}
            placeholder={tags.length === 0 ? placeholder : 'Add more...'}
            className="w-full bg-transparent text-xs font-mono text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none py-0.5"
          />
          {inputVal.trim() && (
            <button
              type="button"
              onClick={handleAdd}
              className="text-[var(--accent-green)] hover:text-white p-0.5 cursor-pointer"
              title="Add item"
            >
              <Plus className="w-3.5 h-3.5" />
            </button>
          )}
        </div>
      </div>
      {suggestions && (
        <datalist id={listId}>
          {(matches.length > 0 ? matches : suggestions.filter((s) => !tags.includes(s))).map((s) => (
            <option key={s} value={s} />
          ))}
        </datalist>
      )}
      {error ? (
        <p className="text-[11px] text-[var(--accent-red)] font-mono mt-1">
          {error}
        </p>
      ) : dupe ? (
        <p className="text-[11px] text-[var(--accent-yellow)] font-mono mt-1">
          '{dupe}' is already in the list
        </p>
      ) : helperText ? (
        <p className="text-[11px] text-[var(--text-muted)] leading-tight">
          {helperText}
        </p>
      ) : null}
    </div>
  );
}
