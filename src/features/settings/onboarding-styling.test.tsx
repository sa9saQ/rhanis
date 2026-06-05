// Styling contract for the first-run onboarding + settings panel (koe-iyr).
//
// jsdom does NOT resolve computed/pixel styles or `var()`, so these tests assert
// the things that ACTUALLY caused the "raw HTML form" / "white box on a dark
// desk" regression at a structural level instead of comparing pixels:
//   1. the onboarding CSS is on the component's import (load) path;
//   2. every koe-* class the onboarding AND settings components use is DEFINED on
//      that path (the real root cause was undefined .koe-onboarding-* classes);
//   3. settings.css no longer ships a global :root override (the dark/light mix);
//   4. the onboarding components actually mount the layout classes (used ⟺ defined).
//
// Selector lookups run on COMMENT-STRIPPED css, so a `.koe-foo` mentioned only in
// a comment is never mistaken for a real definition (R-C[MEDIUM]).
//
// The final pixel-level look is validated on Windows E2E (koe-ef8); see the PR.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { BudgetOnboarding } from "./BudgetOnboarding";

const here = dirname(fileURLToPath(import.meta.url)); // .../src/features/settings
const SRC = resolve(here, "../.."); // .../src

function read(rel: string): string {
  return readFileSync(resolve(SRC, rel), "utf8");
}
function readOrEmpty(rel: string): string {
  // Tolerant read so a missing onboarding.css surfaces as a clean contract
  // failure (undefined classes / missing import) rather than a module-load crash.
  try {
    return readFileSync(resolve(SRC, rel), "utf8");
  } catch {
    return "";
  }
}
/** Drop /* … *​/ comments so commented-out selectors don't count as defined. */
function stripComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

const appCss = read("App.css");
const settingsCss = read("features/settings/settings.css");
const onboardingCss = readOrEmpty("features/settings/onboarding.css");

// Reachable on the ONBOARDING render path: App.css (always) + settings.css +
// onboarding.css (both imported by OnboardingGate).
const onboardingPathCss = stripComments([appCss, settingsCss, onboardingCss].join("\n"));
// Reachable in the SETTINGS panel: App.css (.koe-btn base) + settings.css.
const settingsPathCss = stripComments([appCss, settingsCss].join("\n"));
const settingsCssCode = stripComments(settingsCss);

const onboardingGateSrc = read("features/settings/OnboardingGate.tsx");
const budgetSrc = read("features/settings/BudgetOnboarding.tsx");
const apiKeySrc = read("features/settings/ApiKeyInput.tsx");
const settingsPanelSrc = read("features/settings/SettingsPanel.tsx");
const voiceSelectorSrc = read("features/settings/VoiceProviderSelector.tsx");
const policyEditorSrc = read("features/settings/PermissionPolicyEditor.tsx");

/** koe-* tokens from `className="..."` string literals only — `id="..."`
 *  attributes (e.g. koe-budget-amount-input) are intentionally excluded. */
function usedClasses(...srcs: string[]): string[] {
  const out = new Set<string>();
  const re = /className="([^"]*)"/g;
  for (const src of srcs) {
    let m: RegExpExecArray | null;
    while ((m = re.exec(src)) !== null) {
      for (const tok of m[1].split(/\s+/)) {
        if (tok.startsWith("koe-")) out.add(tok);
      }
    }
  }
  return [...out];
}

/** True if `cls` is defined as a selector in `css`. The negative lookahead stops
 *  `.koe-btn` from matching inside `.koe-btn-primary`. */
function isDefined(cls: string, css: string): boolean {
  return new RegExp(`\\.${cls}(?![\\w-])`).test(css);
}
function undefinedAmong(srcs: string[], css: string): string[] {
  return usedClasses(...srcs).filter((c) => !isDefined(c, css));
}

describe("onboarding styling — load path & class contract (koe-iyr)", () => {
  it("OnboardingGate imports both the shared settings styles and the onboarding layout", () => {
    expect(onboardingGateSrc).toMatch(/import\s+["']\.\/settings\.css["']/);
    expect(onboardingGateSrc).toMatch(/import\s+["']\.\/onboarding\.css["']/);
  });

  it("every koe-* class used by the onboarding flow is defined on the onboarding CSS path", () => {
    expect(undefinedAmong([onboardingGateSrc, budgetSrc, apiKeySrc], onboardingPathCss)).toEqual([]);
  });

  it("every koe-* class used by the settings panel is defined on the settings CSS path", () => {
    expect(
      undefinedAmong([settingsPanelSrc, voiceSelectorSrc, policyEditorSrc], settingsPathCss),
    ).toEqual([]);
  });

  it("settings.css no longer ships a global :root override (theme clash, root cause #2)", () => {
    expect(settingsCssCode).not.toMatch(/:root\b/);
  });

  it("settings.css carries no white surface (#fff / #ffffff) — the theme stays dark", () => {
    expect(settingsCssCode.toLowerCase()).not.toContain("#fff");
  });

  it("the settings/onboarding sub-theme tokens are scoped to the component roots", () => {
    // --koe-surface stays defined (dark, aliased to the app palette) but under
    // the component-root selector list, never a bare global override.
    expect(settingsCssCode).toMatch(/--koe-surface\s*:/);
    expect(settingsCssCode).toMatch(/\.koe-onboarding-wizard[^{]*\{[^}]*--koe-surface/s);
  });
});

describe("onboarding components render their layout classes (koe-iyr)", () => {
  it("BudgetOnboarding mounts its onboarding layout classes", () => {
    const { container } = render(<BudgetOnboarding onBudgetChosen={() => {}} />);
    expect(container.querySelector(".koe-budget-onboarding")).not.toBeNull();
    expect(container.querySelector(".koe-onboarding-title")).not.toBeNull();
    expect(container.querySelector(".koe-onboarding-desc")).not.toBeNull();
    expect(container.querySelector(".koe-budget-fieldset")).not.toBeNull();
  });
});
