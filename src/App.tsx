import { useState } from "react";

// Base-layer styles (theme tokens + shared primitives like `.koe-btn`) must load
// BEFORE any feature stylesheet, so feature VARIANTS (.koe-btn-approve /
// .koe-btn-primary …) win the cascade over the base at equal specificity. Hence
// this import precedes the feature-component imports below (koe-iyr).
import "./App.css";

import { ActivityLog } from "./features/activity/ActivityLog";
import { ApprovalModal } from "./features/activity/ApprovalModal";
import { DevMockEmitter } from "./features/activity/DevMockEmitter";
import { useActivityEvents } from "./features/activity/useActivityEvents";
import { useCostEvents } from "./features/activity/useCostEvents";
import { VoiceButton } from "./features/session/VoiceButton";
import { useSessionEvents } from "./features/session/useSessionEvents";
import { OnboardingGate } from "./features/settings/OnboardingGate";
import { SettingsPanel } from "./features/settings/SettingsPanel";

function ActivityConsole() {
  // Subscribe to the backend tool-event / approval / status streams for the
  // app's lifetime.
  useActivityEvents();
  // Subscribe to the backend session-status stream; drives sessionStore.
  useSessionEvents();
  // Pull + subscribe to the live monthly cost snapshot; drives costStore (koe-9xi).
  useCostEvents();

  const [showSettings, setShowSettings] = useState(false);

  return (
    <main className="koe-app">
      <div className="koe-app-header">
        <h1 className="koe-app-title">koe — activity</h1>
        <button
          type="button"
          className="koe-btn koe-btn-icon"
          onClick={() => setShowSettings((v) => !v)}
          aria-label="設定を開く"
        >
          設定
        </button>
      </div>
      {showSettings && <SettingsPanel onClose={() => setShowSettings(false)} />}
      {/* Primary MVP control — start / stop the Realtime session. */}
      <VoiceButton />
      <ActivityLog />
      {import.meta.env.DEV && <DevMockEmitter />}
      <ApprovalModal />
    </main>
  );
}

function App() {
  return (
    <OnboardingGate>
      <ActivityConsole />
    </OnboardingGate>
  );
}

export default App;
