import { act, renderHook } from "@testing-library/react";
import { MockBackend } from "./mockBackend";
import { useAppModel } from "./useAppModel";
import type { OperationRequest, OperationResult } from "./types";

it("queues read-only heartbeats only for connected agent routes and stops on unmount", async () => {
  class Backend extends MockBackend {
    requests: OperationRequest[] = [];
    async listDevices() {
      const devices = await super.listDevices();
      return devices.map((d) => ({ ...d, id: "adbian-synthetic" }));
    }
    async startOperation(_id: string, request: OperationRequest) {
      this.requests.push(request);
      return "heartbeat";
    }
    async awaitOperation(): Promise<OperationResult> {
      return {
        operation_id: "heartbeat",
        device_id: "adbian-synthetic",
        operation: "diagnostics.read",
        origin: "mock",
        outcome: "success",
        data: null,
        error: null,
        uncertain: false,
        duration_ms: 1,
        evidence: [],
      };
    }
  }
  vi.useFakeTimers();
  try {
    const backend = new Backend();
    await backend.connectDevice("mock-e72");
    const { unmount } = renderHook(() => useAppModel(backend));
    await act(async () => {
      await Promise.resolve();
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(15000);
    });
    expect(backend.requests).toEqual([{ type: "agent.ping" }]);
    unmount();
    await vi.advanceTimersByTimeAsync(15000);
    expect(backend.requests).toHaveLength(1);
  } finally {
    vi.useRealTimers();
  }
});
