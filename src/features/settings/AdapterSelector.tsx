// Recorder adapter selector. M1 only supports "sqlite"; future adapters are
// shown as disabled to preview the UI without allowing invalid selection.

interface AdapterSelectorProps {
  value: string;
  onChange?: (name: string) => void;
  disabled?: boolean;
}

const ADAPTERS = [
  { name: "sqlite", label: "ローカル (SQLite)", available: true },
  { name: "obsidian", label: "Obsidian (M2)", available: false },
  { name: "notion", label: "Notion (M3)", available: false },
];

export function AdapterSelector({ value, onChange, disabled = false }: AdapterSelectorProps) {
  return (
    <div className="rhanis-adapter-selector">
      <label htmlFor="rhanis-adapter-select" className="rhanis-label">
        保存先アダプター
      </label>
      <select
        id="rhanis-adapter-select"
        value={value}
        onChange={(e) => onChange?.(e.target.value)}
        disabled={disabled}
        className="rhanis-select"
      >
        {ADAPTERS.map((a) => (
          <option key={a.name} value={a.name} disabled={!a.available}>
            {a.label}
          </option>
        ))}
      </select>
    </div>
  );
}
