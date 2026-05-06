import { describe, expect, it } from "vitest";
import { shouldDispatchInterimCommand } from "./useBrowserSpeechCommandLane";

describe("shouldDispatchInterimCommand", () => {
  it.each([
    ["Romans 8", false],
    ["Romans chapter 8", false],
    ["Romans 8 28", true],
    ["open Genesis 1 1", true],
    ["Malachi 2 verse 3", true],
    ["next verse", true],
    ["continue", true],
    ["keep going", true],
    ["go to verse 20", true],
    ["the Lord is my shepherd", false],
  ])("returns %s for %s", (input, expected) => {
    expect(shouldDispatchInterimCommand(input)).toBe(expected);
  });
});
