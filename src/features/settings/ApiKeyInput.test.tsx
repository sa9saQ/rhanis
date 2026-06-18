// TDD tests for the (now provider-generic) ApiKeyInput component.
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const setProviderApiKey = vi.fn();
const hasProviderApiKey = vi.fn();
const deleteProviderApiKey = vi.fn();

vi.mock("../../lib/tauri/ipc", () => ({
  setProviderApiKey: (...args: unknown[]) => setProviderApiKey(...args),
  hasProviderApiKey: (...args: unknown[]) => hasProviderApiKey(...args),
  deleteProviderApiKey: (...args: unknown[]) => deleteProviderApiKey(...args),
  // Unused-by-this-component exports, present so the mock matches the module shape.
  getAppSettings: vi.fn(),
  completeOnboarding: vi.fn(),
  saveBudgetConfig: vi.fn(),
  setRecorderAdapter: vi.fn(),
  setVoiceProvider: vi.fn(),
  setToolProviderEnabled: vi.fn(),
}));

import { ApiKeyInput } from "./ApiKeyInput";

beforeEach(() => {
  setProviderApiKey.mockReset();
  hasProviderApiKey.mockReset();
  deleteProviderApiKey.mockReset();
  setProviderApiKey.mockResolvedValue(undefined);
  hasProviderApiKey.mockResolvedValue(false);
  deleteProviderApiKey.mockResolvedValue(undefined);
});

describe("ApiKeyInput", () => {
  it("renders a password input by default", () => {
    render(<ApiKeyInput />);
    const passwordInput = document.querySelector('input[type="password"]');
    expect(passwordInput).not.toBeNull();
  });

  it("toggles input type between password and text on show/hide click", () => {
    render(<ApiKeyInput />);
    const toggle = screen.getByRole("button", { name: /表示|非表示|show|hide/i });
    const input = document.querySelector("input") as HTMLInputElement;
    expect(input.type).toBe("password");

    fireEvent.click(toggle);
    expect(input.type).toBe("text");

    fireEvent.click(toggle);
    expect(input.type).toBe("password");
  });

  it("saves the key for the default 'openai' provider and clears the input", async () => {
    render(<ApiKeyInput />);
    const input = document.querySelector("input") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "sk-test-key" } });

    const saveBtn = screen.getByRole("button", { name: /保存|save/i });
    await act(async () => {
      fireEvent.click(saveBtn);
    });

    // Default provider is "openai" → call carries the provider id explicitly.
    expect(setProviderApiKey).toHaveBeenCalledWith("openai", "sk-test-key");
    expect(input.value).toBe("");
  });

  it("routes to the given provider (xai), NOT openai", async () => {
    render(<ApiKeyInput provider="xai" label="XAI (Grok) APIキー" placeholder="xai-…" />);
    const input = document.querySelector("input") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "xai-secret" } });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /保存|save/i }));
    });

    expect(setProviderApiKey).toHaveBeenCalledWith("xai", "xai-secret");
    expect(setProviderApiKey).not.toHaveBeenCalledWith("openai", expect.anything());
  });

  it("shows a fixed JP error message on IPC failure, does not leak raw error", async () => {
    setProviderApiKey.mockRejectedValue(new Error("internal/path/leaked sk-secret"));
    render(<ApiKeyInput />);
    const input = document.querySelector("input") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "sk-test-key" } });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /保存|save/i }));
    });

    const alert = screen.getByRole("alert");
    expect(alert).not.toHaveTextContent("sk-secret");
    expect(alert).not.toHaveTextContent("/path");
    expect(alert.textContent!.length).toBeGreaterThan(0);
  });

  it("reports the key present optimistically after save (no presence-confirm round-trip)", async () => {
    setProviderApiKey.mockResolvedValue(undefined); // save OK
    const onKeyStatusChange = vi.fn();
    render(<ApiKeyInput onKeyStatusChange={onKeyStatusChange} />);
    const input = document.querySelector("input") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "sk-x" } });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /保存|save/i }));
    });

    // rhanis-nt2: the save's Ok is the authoritative proof of storage, so the
    // key is reported present optimistically — no redundant has() round-trip
    // (which would pay another snapshot decrypt). No save-failure alert, and the
    // input is cleared.
    expect(hasProviderApiKey).not.toHaveBeenCalled();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(onKeyStatusChange).toHaveBeenCalledWith(true);
    expect(input.value).toBe("");
  });

  it("does NOT call hasProviderApiKey after a successful save (redundant scrypt removed)", async () => {
    render(<ApiKeyInput />);
    const input = document.querySelector("input") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "sk-test-key" } });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /保存|save/i }));
    });

    expect(setProviderApiKey).toHaveBeenCalledWith("openai", "sk-test-key");
    // rhanis-nt2: the post-save presence confirm is removed — save Ok already
    // proves storage, so has() would only pay another snapshot decrypt.
    expect(hasProviderApiKey).not.toHaveBeenCalled();
  });

  it("calls deleteProviderApiKey with the provider when delete is clicked", async () => {
    render(<ApiKeyInput hasKey={true} />);
    const deleteBtn = screen.getByRole("button", { name: /削除|delete/i });
    await act(async () => {
      fireEvent.click(deleteBtn);
    });
    expect(deleteProviderApiKey).toHaveBeenCalledWith("openai");
  });

  it("uses the onDelete override instead of deleteProviderApiKey when provided", async () => {
    const onDelete = vi.fn().mockResolvedValue(undefined);
    render(<ApiKeyInput provider="xai" hasKey={true} onDelete={onDelete} />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /削除|delete/i }));
    });
    expect(onDelete).toHaveBeenCalled();
    expect(deleteProviderApiKey).not.toHaveBeenCalled();
  });

  it("does not show the saved key value in the DOM after save (key must not linger)", async () => {
    render(<ApiKeyInput />);
    const input = document.querySelector("input") as HTMLInputElement;
    const secretKey = "sk-super-secret-12345";
    fireEvent.change(input, { target: { value: secretKey } });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /保存|save/i }));
    });

    await waitFor(() => {
      expect(document.body.innerHTML).not.toContain(secretKey);
    });
  });
});
