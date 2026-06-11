// Base-layer styles (theme tokens + shared primitives like `.koe-btn`) must load
// BEFORE any feature stylesheet, so feature VARIANTS (.koe-btn-approve /
// .koe-btn-primary …) win the cascade over the base at equal specificity. Hence
// this import precedes the feature-component imports below (koe-iyr).
import "./App.css";

import { ConsoleLayout } from "./features/console/ConsoleLayout";
import { OnboardingGate } from "./features/settings/OnboardingGate";

function App() {
  return (
    <OnboardingGate>
      {/* The glass-box console shell (koe-ios.1): sidebar + greeting +
          live activity panel + voice orb. Owns the backend event wiring. */}
      <ConsoleLayout />
    </OnboardingGate>
  );
}

export default App;
