import type { ReactNode } from "react";
import type { Preferences } from "../../lib/tauri";

export type SettingsSectionId =
  | "general"
  | "appearance"
  | "library"
  | "ai"
  | "privacy"
  | "storage"
  | "performance"
  | "shortcuts"
  | "importExport"
  | "updates"
  | "about";

export const SETTINGS_NAV: { id: SettingsSectionId; label: string }[] = [
  { id: "general", label: "General" },
  { id: "appearance", label: "Appearance" },
  { id: "library", label: "Library" },
  { id: "ai", label: "AI Features" },
  { id: "privacy", label: "Privacy & Security" },
  { id: "storage", label: "Storage" },
  { id: "performance", label: "Performance" },
  { id: "shortcuts", label: "Keyboard Shortcuts" },
  { id: "importExport", label: "Import & Export" },
  { id: "updates", label: "Updates" },
  { id: "about", label: "About" },
];

export function SettingsBlock({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <section className="settings-block">
      <h3>{title}</h3>
      <div className="settings-block-body">{children}</div>
    </section>
  );
}

export function ToggleRow({
  label,
  description,
  checked,
  onChange,
  disabled,
  soon,
}: {
  label: string;
  description?: string;
  checked: boolean;
  onChange: (next: boolean) => void;
  disabled?: boolean;
  soon?: boolean;
}) {
  return (
    <label className={`settings-row settings-toggle ${disabled ? "is-disabled" : ""}`}>
      <span className="settings-row-copy">
        <span className="settings-row-label">
          {label}
          {soon && <span className="nav-soon-pill">Soon</span>}
        </span>
        {description && <span className="muted">{description}</span>}
      </span>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
    </label>
  );
}

export function ChoiceRow({
  label,
  description,
  value,
  options,
  onChange,
  disabled,
}: {
  label: string;
  description?: string;
  value: string;
  options: { value: string; label: string; soon?: boolean }[];
  onChange: (next: string) => void;
  disabled?: boolean;
}) {
  return (
    <div className={`settings-row settings-choice ${disabled ? "is-disabled" : ""}`}>
      <span className="settings-row-copy">
        <span className="settings-row-label">{label}</span>
        {description && <span className="muted">{description}</span>}
      </span>
      <div className="settings-choice-options" role="radiogroup" aria-label={label}>
        {options.map((opt) => (
          <button
            key={opt.value}
            type="button"
            className={value === opt.value ? "is-active" : ""}
            disabled={disabled || opt.soon}
            onClick={() => onChange(opt.value)}
            title={opt.soon ? "Coming soon" : undefined}
          >
            {opt.label}
            {opt.soon ? " · Soon" : ""}
          </button>
        ))}
      </div>
    </div>
  );
}

export function SelectRow({
  label,
  description,
  value,
  options,
  onChange,
  disabled,
}: {
  label: string;
  description?: string;
  value: string;
  options: { value: string; label: string }[];
  onChange: (next: string) => void;
  disabled?: boolean;
}) {
  return (
    <label className={`settings-row ${disabled ? "is-disabled" : ""}`}>
      <span className="settings-row-copy">
        <span className="settings-row-label">{label}</span>
        {description && <span className="muted">{description}</span>}
      </span>
      <select
        value={value}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
      >
        {options.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
    </label>
  );
}

export function SliderRow({
  label,
  description,
  value,
  min,
  max,
  step,
  onChange,
  format,
}: {
  label: string;
  description?: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (next: number) => void;
  format?: (n: number) => string;
}) {
  return (
    <div className="settings-row settings-slider">
      <span className="settings-row-copy">
        <span className="settings-row-label">
          {label}
          <span className="muted">{format ? format(value) : value}</span>
        </span>
        {description && <span className="muted">{description}</span>}
      </span>
      <input
        type="range"
        min={min}
        max={max}
        step={step ?? 1}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
      />
    </div>
  );
}

export type PrefsUpdater = (
  mutator: (current: Preferences) => Preferences,
) => Promise<void>;
