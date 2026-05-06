import React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const stream = {} as MediaStream;

vi.mock("../contexts/AudioStreamContext", () => ({
  useAudioStream: () => ({ stream }),
}));

import { VuMeter } from "./VuMeter";

class FakeAnalyser {
  fftSize = 256;
  smoothingTimeConstant = 0.6;
  frequencyBinCount = 128;

  getByteTimeDomainData(data: Uint8Array) {
    data.fill(136);
  }
}

class FakeAudioContext {
  analyser = new FakeAnalyser();
  closed = false;

  createMediaStreamSource() {
    return { connect: vi.fn() };
  }

  createAnalyser() {
    return this.analyser;
  }

  close() {
    this.closed = true;
    return Promise.resolve();
  }
}

describe("VuMeter", () => {
  let container: HTMLDivElement;
  let root: Root;
  let frameCallbacks: FrameRequestCallback[];
  let cancelledFrames: number[];
  let audioContext: FakeAudioContext;

  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    frameCallbacks = [];
    cancelledFrames = [];
    audioContext = new FakeAudioContext();

    Object.defineProperty(navigator, "mediaDevices", {
      value: { getUserMedia: vi.fn() },
      configurable: true,
    });

    vi.stubGlobal("AudioContext", class {
      constructor() {
        return audioContext;
      }
    });
    vi.stubGlobal("requestAnimationFrame", vi.fn((callback: FrameRequestCallback) => {
      frameCallbacks.push(callback);
      return frameCallbacks.length;
    }));
    vi.stubGlobal("cancelAnimationFrame", vi.fn((id: number) => {
      cancelledFrames.push(id);
    }));
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    delete (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT;
  });

  it("uses the shared stream and never opens a second microphone stream", () => {
    act(() => {
      root.render(<VuMeter active deviceLabel="Booth mic" />);
    });

    expect(navigator.mediaDevices.getUserMedia).not.toHaveBeenCalled();
    expect(audioContext.closed).toBe(false);
  });

  it("cancels animation and closes audio context on unmount", () => {
    act(() => {
      root.render(<VuMeter active deviceLabel="Booth mic" />);
    });

    act(() => {
      frameCallbacks[0]?.(16);
    });

    act(() => root.unmount());

    expect(cancelledFrames.length).toBeGreaterThan(0);
    expect(audioContext.closed).toBe(true);
  });

  it("renders fresh backend level without waiting for browser audio analysis", () => {
    act(() => {
      root.render(
        <VuMeter
          active
          deviceLabel="Backend"
          backendLevel={{
            rms: 0.5,
            peak: 0.7,
            level: 64,
            peakLevel: 70,
            speechDetected: true,
            checkedAtMs: Date.now(),
          }}
        />,
      );
    });

    expect(container.textContent).toContain("64%");
    expect(
      (container.querySelector("[aria-label='Microphone level indicator']") as HTMLElement)
        .style
        .getPropertyValue("--vu-peak"),
    ).toBe("70%");
  });
});
