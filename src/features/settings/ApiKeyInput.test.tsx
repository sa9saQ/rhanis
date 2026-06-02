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
    hasProviderApiKey.mockResolvedValue(true);
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
    hasProviderApiKey.mockResolvedValue(true);
    render(<ApiKeyInput provider="xai" label="XAI (Grok) APIキー" placeholder="xai-…" />);
    const input = document.querySelector("input") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "xai-secret" } });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /保存|save/i }));
    });

    expect(setProviderApiKey).toHaveBeenCalledWith("xai", "xai-secret");
    expect(hasProviderApiKey).toHaveBeenCalledWith("xai");
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

  it("confirms key presence via hasProviderApiKey after save", async () => {
    hasProviderApiKey.mockResolvedValue(true);
    render(<ApiKeyInput />);
    const input = document.querySelector("input") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "sk-test-key" } });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /保存|save/i }));
    });

    expect(hasProviderApiKey).toHaveBeenCalledWith("openai");
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
    hasProviderApiKey.mockResolvedValue(true);
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
