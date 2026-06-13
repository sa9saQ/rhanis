// Voice provider/model selector (rhanis-31u). OpenAI is active for M1; Google is a
// disabled preview until rhanis-zv3 wires the Gemini Live connection. The chosen
// value is a single "provider/model" string the backend persists; rhanis-zv3 acts
// on it (parses provider + model). Mirrors AdapterSelector's pattern.

interface VoiceProviderSelectorProps {
  value: string;
  onChange?: (value: string) => void;
  disabled?: boolean;
}

// These value strings MUST stay in sync with the Rust allowlist
// KNOWN_VOICE_PROVIDER_MODELS (settings_store.rs) — they are duplicated literals
// with no generated single source, so edit both together. Google is a disabled
// preview until rhanis-zv3 wires the Gemini Live connection.
const VOICE_PROVIDERS = [
  { value: "openai/gpt-realtime-2", label: "OpenAI (GPT Realtime)", available: true },
  { value: "google/gemini-2.5-flash-live", label: "Google (Gemini Live, 準備中)", available: false },
];

export function VoiceProviderSelector({
  value,
  onChange,
  disabled = false,
}: VoiceProviderSelectorProps) {
  return (
    <div className="rhanis-voice-provider-selector">
      <label htmlFor="rhanis-voice-provider-select" className="rhanis-label">
        声のプロバイダ
      </label>
      <select
        id="rhanis-voice-provider-select"
        value={value}
        onChange={(e) => onChange?.(e.target.value)}
        disabled={disabled}
        className="rhanis-select"
      >
        {VOICE_PROVIDERS.map((p) => (
          <option key={p.value} value={p.value} disabled={!p.available}>
            {p.label}
          </option>
        ))}
      </select>
    </div>
  );
}
