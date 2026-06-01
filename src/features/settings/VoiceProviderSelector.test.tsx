// Tests for VoiceProviderSelector (koe-31u). Mirrors AdapterSelector.test.tsx.
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { VoiceProviderSelector } from "./VoiceProviderSelector";

describe("VoiceProviderSelector", () => {
  it("renders the OpenAI option as selected by default value", () => {
    render(<VoiceProviderSelector value="openai/gpt-realtime-2" />);
    const select = screen.getByRole("combobox") as HTMLSelectElement;
    expect(select.value).toBe("openai/gpt-realtime-2");
  });

  it("shows the Google (M2) option as disabled preview", () => {
    render(<VoiceProviderSelector value="openai/gpt-realtime-2" />);
    const options = screen.getAllByRole("option") as HTMLOptionElement[];
    const disabled = options.filter((o) => o.disabled);
    expect(disabled.length).toBeGreaterThan(0);
    // The OpenAI option must NOT be disabled (it's the active one).
    const openai = options.find((o) => o.value === "openai/gpt-realtime-2");
    expect(openai?.disabled).toBe(false);
  });

  it("calls onChange with the selected provider/model string", () => {
    const onChange = vi.fn();
    render(<VoiceProviderSelector value="openai/gpt-realtime-2" onChange={onChange} />);
    const select = screen.getByRole("combobox");
    fireEvent.change(select, { target: { value: "openai/gpt-realtime-2" } });
    expect(onChange).toHaveBeenCalledWith("openai/gpt-realtime-2");
  });

  it("is disabled when the disabled prop is true", () => {
    render(<VoiceProviderSelector value="openai/gpt-realtime-2" disabled={true} />);
    const select = screen.getByRole("combobox") as HTMLSelectElement;
    expect(select.disabled).toBe(true);
  });
});
