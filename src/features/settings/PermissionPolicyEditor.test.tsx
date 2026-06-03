// Tests for PermissionPolicyEditor (koe-351). The folder picker (pickFolder) and
// the persistence IPC are mocked; the editor's CRUD logic + the exact policy it
// persists are what we verify here. Real folder-dialog launch is a Windows E2E
// follow-up (koe-ef8 family) and is intentionally out of scope for this test.
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const getAppSettings = vi.fn();
const setPermissionPolicy = vi.fn();
const pickFolder = vi.fn();

vi.mock("../../lib/tauri/ipc", () => ({
  getAppSettings: (...a: unknown[]) => getAppSettings(...a),
  setPermissionPolicy: (...a: unknown[]) => setPermissionPolicy(...a),
  pickFolder: (...a: unknown[]) => pickFolder(...a),
  // Other ipc fns the store imports — present so the module mock is complete.
  completeOnboarding: vi.fn(),
  saveBudgetConfig: vi.fn(),
  setVoiceProvider: vi.fn(),
  setToolProviderEnabled: vi.fn(),
  deleteToolProviderKey: vi.fn(),
}));

import { PermissionPolicyEditor } from "./PermissionPolicyEditor";
import { useSettingsStore } from "./settingsStore";
import { EMPTY_PERMISSION_POLICY, type AppSettings, type PermissionPolicy } from "./types";

function settingsWith(policy: PermissionPolicy): AppSettings {
  return {
    onboarding_completed: true,
    budget: { enabled: false, monthly_limit_nanodollars: 0 },
    recorder_adapter: "sqlite",
    voice_provider_model: "openai/gpt-realtime-2",
    tool_providers: { xai: false, x: false, search: false },
    permission_policy: policy,
  };
}

function seedPolicy(policy: PermissionPolicy) {
  useSettingsStore.setState({ settings: settingsWith(policy), loaded: true, loadError: null });
  // The store re-fetches after each save; echo the same settings back so the
  // component re-renders without exploding.
  getAppSettings.mockResolvedValue(settingsWith(policy));
}

/** The single policy object passed to the most recent setPermissionPolicy call. */
function lastSavedPolicy(): PermissionPolicy {
  const calls = setPermissionPolicy.mock.calls;
  return calls[calls.length - 1][0] as PermissionPolicy;
}

beforeEach(() => {
  getAppSettings.mockReset();
  setPermissionPolicy.mockReset();
  pickFolder.mockReset();
  setPermissionPolicy.mockResolvedValue(undefined);
  seedPolicy(EMPTY_PERMISSION_POLICY);
});

describe("PermissionPolicyEditor", () => {
  it("renders empty-state placeholders for every list", () => {
    render(<PermissionPolicyEditor />);
    expect(screen.getAllByText("まだ登録されていません。").length).toBe(4);
  });

  it("adds an allowed folder typed into the text input (allow_danger defaults false)", async () => {
    render(<PermissionPolicyEditor />);
    fireEvent.change(screen.getByLabelText("許可フォルダのパス"), {
      target: { value: "/home/u/work" },
    });
    fireEvent.click(screen.getByRole("button", { name: "許可フォルダを追加" }));
    await waitFor(() => expect(setPermissionPolicy).toHaveBeenCalledTimes(1));
    expect(lastSavedPolicy().allowed_folders).toEqual([{ path: "/home/u/work", allow_danger: false }]);
  });

  it("adds an allowed folder chosen from the native picker", async () => {
    pickFolder.mockResolvedValue("/picked/folder");
    render(<PermissionPolicyEditor />);
    fireEvent.click(screen.getByRole("button", { name: "許可フォルダをダイアログで選択" }));
    await waitFor(() => expect(setPermissionPolicy).toHaveBeenCalled());
    expect(lastSavedPolicy().allowed_folders).toEqual([{ path: "/picked/folder", allow_danger: false }]);
  });

  it("does not persist anything when the picker is cancelled (null)", async () => {
    pickFolder.mockResolvedValue(null);
    render(<PermissionPolicyEditor />);
    fireEvent.click(screen.getByRole("button", { name: "許可フォルダをダイアログで選択" }));
    await waitFor(() => expect(pickFolder).toHaveBeenCalled());
    expect(setPermissionPolicy).not.toHaveBeenCalled();
  });

  it("toggles the per-folder DANGER opt-in", async () => {
    seedPolicy({ ...EMPTY_PERMISSION_POLICY, allowed_folders: [{ path: "/home/u/work", allow_danger: false }] });
    render(<PermissionPolicyEditor />);
    fireEvent.click(screen.getByRole("checkbox", { name: "強い操作も自動" }));
    await waitFor(() => expect(setPermissionPolicy).toHaveBeenCalled());
    expect(lastSavedPolicy().allowed_folders).toEqual([{ path: "/home/u/work", allow_danger: true }]);
  });

  it("removes an allowed folder", async () => {
    seedPolicy({ ...EMPTY_PERMISSION_POLICY, allowed_folders: [{ path: "/home/u/work", allow_danger: false }] });
    render(<PermissionPolicyEditor />);
    fireEvent.click(screen.getByRole("button", { name: "許可フォルダを削除: /home/u/work" }));
    await waitFor(() => expect(setPermissionPolicy).toHaveBeenCalled());
    expect(lastSavedPolicy().allowed_folders).toEqual([]);
  });

  it("adds a denied folder", async () => {
    render(<PermissionPolicyEditor />);
    fireEvent.change(screen.getByLabelText("禁止する場所のパス"), { target: { value: "/home/u/secret" } });
    fireEvent.click(screen.getByRole("button", { name: "禁止の場所を追加" }));
    await waitFor(() => expect(setPermissionPolicy).toHaveBeenCalled());
    expect(lastSavedPolicy().denied_folders).toEqual(["/home/u/secret"]);
  });

  it("adds a normalized allowed URL host", async () => {
    render(<PermissionPolicyEditor />);
    fireEvent.change(screen.getByLabelText("許可ドメイン"), { target: { value: " OpenAI.com. " } });
    fireEvent.click(screen.getByRole("button", { name: "許可ドメインを追加" }));
    await waitFor(() => expect(setPermissionPolicy).toHaveBeenCalled());
    expect(lastSavedPolicy().allowed_url_hosts).toEqual(["openai.com"]);
  });

  it("rejects a malformed host without persisting", async () => {
    render(<PermissionPolicyEditor />);
    fireEvent.change(screen.getByLabelText("許可ドメイン"), {
      target: { value: "https://openai.com/path" },
    });
    fireEvent.click(screen.getByRole("button", { name: "許可ドメインを追加" }));
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(setPermissionPolicy).not.toHaveBeenCalled();
  });

  it("adds a denied URL host", async () => {
    render(<PermissionPolicyEditor />);
    fireEvent.change(screen.getByLabelText("禁止ドメイン"), { target: { value: "evil.com" } });
    fireEvent.click(screen.getByRole("button", { name: "禁止ドメインを追加" }));
    await waitFor(() => expect(setPermissionPolicy).toHaveBeenCalled());
    expect(lastSavedPolicy().denied_url_hosts).toEqual(["evil.com"]);
  });

  it("toggles allow-all-urls and shows the exfiltration warning", async () => {
    render(<PermissionPolicyEditor />);
    // The warning copy is always present so the user sees the risk before opting in.
    expect(screen.getByText(/機密が URL に含まれて外部に送られる/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox", { name: /すべての URL を許可/ }));
    await waitFor(() => expect(setPermissionPolicy).toHaveBeenCalled());
    expect(lastSavedPolicy().allow_all_urls).toBe(true);
  });

  it("surfaces a save failure as an alert and does not throw", async () => {
    setPermissionPolicy.mockRejectedValue(new Error("permission policy host is invalid"));
    render(<PermissionPolicyEditor />);
    fireEvent.change(screen.getByLabelText("禁止ドメイン"), { target: { value: "evil.com" } });
    fireEvent.click(screen.getByRole("button", { name: "禁止ドメインを追加" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("ポリシーの保存に失敗");
  });

  it("clears the input on a successful save but PRESERVES it on failure (no silent drop)", async () => {
    // Success → input cleared.
    render(<PermissionPolicyEditor />);
    const okInput = screen.getByLabelText("禁止ドメイン") as HTMLInputElement;
    fireEvent.change(okInput, { target: { value: "evil.com" } });
    fireEvent.click(screen.getByRole("button", { name: "禁止ドメインを追加" }));
    await waitFor(() => expect(setPermissionPolicy).toHaveBeenCalled());
    await waitFor(() => expect(okInput.value).toBe(""));

    // Failure → the typed deny entry must NOT be silently lost (input preserved).
    setPermissionPolicy.mockReset();
    setPermissionPolicy.mockRejectedValue(new Error("save failed"));
    const failInput = screen.getByLabelText("禁止ドメイン") as HTMLInputElement;
    fireEvent.change(failInput, { target: { value: "still-here.com" } });
    fireEvent.click(screen.getByRole("button", { name: "禁止ドメインを追加" }));
    await screen.findByRole("alert");
    expect(failInput.value).toBe("still-here.com");
  });

  it("does not revert a concurrent save when the folder picker resolves (stale-closure race)", async () => {
    // Stateful backend: setPermissionPolicy persists, getAppSettings echoes it,
    // so the store's permission_policy reflects each save (what latestPolicy reads).
    let backend: PermissionPolicy = EMPTY_PERMISSION_POLICY;
    setPermissionPolicy.mockImplementation(async (p: PermissionPolicy) => {
      backend = p;
    });
    getAppSettings.mockImplementation(async () => settingsWith(backend));
    useSettingsStore.setState({ settings: settingsWith(backend), loaded: true, loadError: null });

    // The picker stays pending until we resolve it by hand.
    let resolvePick!: (v: string | null) => void;
    pickFolder.mockReturnValue(
      new Promise<string | null>((res) => {
        resolvePick = res;
      }),
    );

    render(<PermissionPolicyEditor />);
    // 1) open the folder picker — its await is now in flight
    fireEvent.click(screen.getByRole("button", { name: "許可フォルダをダイアログで選択" }));
    // 2) while it is open, save a DIFFERENT change (a deny host)
    fireEvent.change(screen.getByLabelText("禁止ドメイン"), { target: { value: "evil.com" } });
    fireEvent.click(screen.getByRole("button", { name: "禁止ドメインを追加" }));
    await waitFor(() =>
      expect(
        useSettingsStore.getState().settings?.permission_policy.denied_url_hosts,
      ).toEqual(["evil.com"]),
    );
    // 3) resolve the picker → it commits the allowed folder from the LATEST policy
    resolvePick("/picked");
    await waitFor(() =>
      expect(backend.allowed_folders).toEqual([{ path: "/picked", allow_danger: false }]),
    );
    // The deny host saved in step 2 must STILL be present (not reverted by the
    // picker's deferred commit building off a stale render snapshot).
    expect(backend.denied_url_hosts).toEqual(["evil.com"]);
  });
});
