import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import { DevMockEmitter } from "./DevMockEmitter";
import { selectActiveActions, useActivityStore } from "./activityStore";

beforeEach(() => {
  useActivityStore.getState().reset();
});

describe("DevMockEmitter", () => {
  it("emits a tool run that appears as an active action", () => {
    render(<DevMockEmitter />);
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: /tool 実行/ }));
    });
    expect(selectActiveActions(useActivityStore.getState()).length).toBeGreaterThan(0);
  });

  it("discloses thinking before the tool runs (glass-box M1)", () => {
    render(<DevMockEmitter />);
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: /思考→tool/ }));
    });
    // The disclosure is emitted synchronously; the tool start is deferred, so at
    // this instant only the thinking disclosure is present — i.e. think-before-act.
    const thinking = useActivityStore.getState().thinking;
    expect(thinking).toHaveLength(1);
    expect(thinking[0].tool).toBe("web_search");
    expect(thinking[0].plan).toBe("ウェブを検索しています");
    expect(selectActiveActions(useActivityStore.getState())).toHaveLength(0);
  });

  it("enqueues a DANGER approval", () => {
    render(<DevMockEmitter />);
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: /承認要求 \(DANGER\)/ }));
    });
    const queue = useActivityStore.getState().approvalQueue;
    expect(queue).toHaveLength(1);
    expect(queue[0].risk).toBe("DANGER");
  });

  it("enqueues a CAUTION approval", () => {
    render(<DevMockEmitter />);
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: /承認要求 \(CAUTION\)/ }));
    });
    expect(useActivityStore.getState().approvalQueue[0].risk).toBe("CAUTION");
  });
});
