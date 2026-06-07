import { beforeEach, describe, expect, it, vi } from "vitest";

// Mock the Tauri API surface so the wrappers can be exercised without a runtime.
const listen = vi.fn();
const invoke = vi.fn();

vi.mock("@tauri-apps/api/event", () => ({
  listen: (...args: unknown[]) => listen(...args),
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));

import {
  COMMAND,
  EVENT,
  completeOnboarding,
  deleteOpenaiApiKey,
  deleteProviderApiKey,
  deleteToolProviderKey,
  getAppSettings,
  getCostSnapshot,
  hasOpenaiApiKey,
  hasProviderApiKey,
  onApprovalRequired,
  onCostUpdate,
  onSessionStatus,
  onThinkingEvent,
  onToolEvent,
  resolveToolApproval,
  saveBudgetConfig,
  setOpenaiApiKey,
  setProviderApiKey,
  setRecorderAdapter,
  setToolProviderEnabled,
  setVoiceProvider,
} from "./ipc";
import type {
  ApprovalRequest,
  CostSnapshot,
  SessionStatusEvent,
  ThinkingEvent,
  ToolEvent,
} from "../../features/activity/types";

beforeEach(() => {
  listen.mockReset();
  invoke.mockReset();
  listen.mockResolvedValue(() => {});
  invoke.mockResolvedValue(undefined);
});

describe("ipc event subscriptions", () => {
  it("onToolEvent listens on the tool-event channel and unwraps the payload", async () => {
    let captured: ToolEvent | undefined;
    await onToolEvent((e) => {
      captured = e;
    });
    expect(listen).toHaveBeenCalledTimes(1);
    const [channel, cb] = listen.mock.calls[0] as [string, (e: { payload: ToolEvent }) => void];
    expect(channel).toBe(EVENT.toolEvent);

    const payload = { eventId: "e1" } as ToolEvent;
    cb({ payload });
    expect(captured).toBe(payload);
  });

  it("onThinkingEvent listens on the thinking-event channel and unwraps the payload", async () => {
    let captured: ThinkingEvent | undefined;
    await onThinkingEvent((e) => {
      captured = e;
    });
    const [channel, cb] = listen.mock.calls[0] as [
      string,
      (e: { payload: ThinkingEvent }) => void,
    ];
    expect(channel).toBe(EVENT.thinkingEvent);
    const payload = { eventId: "t1" } as ThinkingEvent;
    cb({ payload });
    expect(captured).toBe(payload);
  });

  it("onApprovalRequired listens on the tool-approval-required channel", async () => {
    let captured: ApprovalRequest | undefined;
    await onApprovalRequired((r) => {
      captured = r;
    });
    const [channel, cb] = listen.mock.calls[0] as [
      string,
      (e: { payload: ApprovalRequest }) => void,
    ];
    expect(channel).toBe(EVENT.approvalRequired);
    const payload = { approvalId: "a1" } as ApprovalRequest;
    cb({ payload });
    expect(captured).toBe(payload);
  });

  it("onSessionStatus listens on the session-status channel", async () => {
    await onSessionStatus(() => {});
    expect(listen.mock.calls[0]?.[0]).toBe(EVENT.sessionStatus);
  });

  it("onCostUpdate listens on the cost-update channel and unwraps the payload", async () => {
    let captured: CostSnapshot | undefined;
    await onCostUpdate((s) => {
      captured = s;
    });
    const [channel, cb] = listen.mock.calls[0] as [
      string,
      (e: { payload: CostSnapshot }) => void,
    ];
    expect(channel).toBe(EVENT.costUpdate);
    const payload = { month: 202606, over_budget: false } as CostSnapshot;
    cb({ payload });
    expect(captured).toBe(payload);
  });

  it("forwards the unlisten function from listen", async () => {
    const unlisten = vi.fn();
    listen.mockResolvedValue(unlisten);
    const result = await onSessionStatus(() => {});
    expect(result).toBe(unlisten);
  });
});

describe("resolveToolApproval", () => {
  it("invokes resolve_tool_approval with approvalId + decision", async () => {
    await resolveToolApproval("abc-123", "approve");
    expect(invoke).toHaveBeenCalledWith(COMMAND.resolveToolApproval, {
      approvalId: "abc-123",
      decision: "approve",
    });
  });

  it("passes the deny decision through unchanged", async () => {
    await resolveToolApproval("xyz", "deny");
    expect(invoke).toHaveBeenCalledWith(COMMAND.resolveToolApproval, {
      approvalId: "xyz",
      decision: "deny",
    });
  });
});

// Guard the SessionStatusEvent shape is referenced (compile-time contract).
const _shape: SessionStatusEvent = { state: "idle", sequence: 0 };
void _shape;

// ---------------------------------------------------------------------------
// Settings + secret-store command wrappers
// ---------------------------------------------------------------------------

describe("getAppSettings", () => {
  it("invokes get_app_settings with no args and returns the result", async () => {
    const fakeSettings = { onboarding_completed: true, budget: { enabled: false, monthly_limit_nanodollars: 0 }, recorder_adapter: "sqlite" };
    invoke.mockResolvedValueOnce(fakeSettings);
    const result = await getAppSettings();
    expect(invoke).toHaveBeenCalledWith(COMMAND.getAppSettings);
    expect(result).toEqual(fakeSettings);
  });
});

describe("completeOnboarding", () => {
  it("invokes complete_onboarding with camelCase args", async () => {
    await completeOnboarding(true, 10.0, "sqlite");
    expect(invoke).toHaveBeenCalledWith(COMMAND.completeOnboarding, {
      enabled: true,
      monthlyLimitUsd: 10.0,
      recorderAdapter: "sqlite",
    });
  });

  it("passes null monthlyLimitUsd for unlimited", async () => {
    await completeOnboarding(false, null, "sqlite");
    expect(invoke).toHaveBeenCalledWith(COMMAND.completeOnboarding, {
      enabled: false,
      monthlyLimitUsd: null,
      recorderAdapter: "sqlite",
    });
  });
});

describe("saveBudgetConfig", () => {
  it("invokes save_budget_config with enabled + monthlyLimitUsd", async () => {
    await saveBudgetConfig(true, 20.0);
    expect(invoke).toHaveBeenCalledWith(COMMAND.saveBudgetConfig, {
      enabled: true,
      monthlyLimitUsd: 20.0,
    });
  });
});

describe("setRecorderAdapter", () => {
  it("invokes set_recorder_adapter with {name}", async () => {
    await setRecorderAdapter("sqlite");
    expect(invoke).toHaveBeenCalledWith(COMMAND.setRecorderAdapter, { name: "sqlite" });
  });
});

describe("setOpenaiApiKey", () => {
  it("invokes set_openai_api_key with {key}", async () => {
    await setOpenaiApiKey("sk-abc123");
    expect(invoke).toHaveBeenCalledWith(COMMAND.setOpenaiApiKey, { key: "sk-abc123" });
  });
});

describe("hasOpenaiApiKey", () => {
  it("invokes has_openai_api_key and returns the boolean", async () => {
    invoke.mockResolvedValueOnce(true);
    const result = await hasOpenaiApiKey();
    expect(invoke).toHaveBeenCalledWith(COMMAND.hasOpenaiApiKey);
    expect(result).toBe(true);
  });
});

describe("deleteOpenaiApiKey", () => {
  it("invokes delete_openai_api_key with no args", async () => {
    await deleteOpenaiApiKey();
    expect(invoke).toHaveBeenCalledWith(COMMAND.deleteOpenaiApiKey);
  });
});

describe("multi-provider key + voice/tool commands (koe-31u)", () => {
  it("setVoiceProvider invokes set_voice_provider with {value}", async () => {
    await setVoiceProvider("openai/gpt-realtime-2");
    expect(invoke).toHaveBeenCalledWith(COMMAND.setVoiceProvider, {
      value: "openai/gpt-realtime-2",
    });
  });

  it("setToolProviderEnabled invokes set_tool_provider_enabled with {provider, enabled}", async () => {
    await setToolProviderEnabled("xai", true);
    expect(invoke).toHaveBeenCalledWith(COMMAND.setToolProviderEnabled, {
      provider: "xai",
      enabled: true,
    });
  });

  it("setProviderApiKey invokes set_provider_api_key with {provider, key}", async () => {
    await setProviderApiKey("xai", "xai-secret");
    expect(invoke).toHaveBeenCalledWith(COMMAND.setProviderApiKey, {
      provider: "xai",
      key: "xai-secret",
    });
  });

  it("hasProviderApiKey invokes has_provider_api_key with {provider} and returns the boolean", async () => {
    invoke.mockResolvedValueOnce(true);
    const result = await hasProviderApiKey("search");
    expect(invoke).toHaveBeenCalledWith(COMMAND.hasProviderApiKey, { provider: "search" });
    expect(result).toBe(true);
  });

  it("deleteProviderApiKey invokes delete_provider_api_key with {provider}", async () => {
    await deleteProviderApiKey("x");
    expect(invoke).toHaveBeenCalledWith(COMMAND.deleteProviderApiKey, { provider: "x" });
  });

  it("deleteToolProviderKey invokes delete_tool_provider_key with {provider}", async () => {
    await deleteToolProviderKey("xai");
    expect(invoke).toHaveBeenCalledWith(COMMAND.deleteToolProviderKey, { provider: "xai" });
  });
});

describe("getCostSnapshot", () => {
  it("invokes get_cost_snapshot with no args and returns the snapshot", async () => {
    const snap: CostSnapshot = {
      month: 202606,
      used_nanodollars: 16_000_000_000,
      limit_nanodollars: 32_000_000_000,
      enabled: true,
      over_budget: false,
      sequence: 3,
      used_usd: 16,
      remaining_usd: 16,
    };
    invoke.mockResolvedValueOnce(snap);
    const result = await getCostSnapshot();
    expect(invoke).toHaveBeenCalledWith(COMMAND.getCostSnapshot);
    expect(result).toEqual(snap);
  });
});
