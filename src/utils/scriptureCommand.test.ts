import { describe, expect, it } from "vitest";
import { classifyScriptureCommand, parseExplicitScriptureCommand } from "./scriptureCommand";

describe("parseExplicitScriptureCommand", () => {
  it.each([
    ["Genesis 1 1", "Genesis", 1, 1],
    ["open Genesis chapter 1 verse 1", "Genesis", 1, 1],
    ["open Malachi 2 verse 3", "Malachi", 2, 3],
    ["Daniel 3 10", "Daniel", 3, 10],
    ["Acts 11 8", "Acts", 11, 8],
    ["Matthew 11 12", "Matthew", 11, 12],
    ["1 Timothy 5 2", "1 Timothy", 5, 2],
    ["first Timothy chapter five verse two", "1 Timothy", 5, 2],
    ["Romans 8", "Romans", 8, 1],
    ["Deutronomy 6 4", "Deuteronomy", 6, 4],
    ["Habbakuk 2 4", "Habakkuk", 2, 4],
    ["pastor said John 3 16", "John", 3, 16],
    ["please open Malachi 2 verse 3", "Malachi", 2, 3],
    ["we were in Ezekiel but now open Matthew chapter 3 verse 4", "Matthew", 3, 4],
    ["Mathew chapter 3 verse 4", "Matthew", 3, 4],
    ["Eziekiel chapter 1 verse 2", "Ezekiel", 1, 2],
  ])("parses %s", (input, book, chapter, verse) => {
    expect(parseExplicitScriptureCommand(input)).toMatchObject({ book, chapter, verse });
  });

  it.each([
    ["Romans 8 28", "explicitReference"],
    ["Matthew chapter 3 verse 4", "explicitReference"],
    ["Ezekiel chapter 1 verse 2", "explicitReference"],
    ["Romans 8", "chapterReference"],
    ["next verse", "contextualFollowUp"],
    ["continue", "contextualFollowUp"],
    ["go to verse 20", "contextualFollowUp"],
    ["the sermon is about mercy", "none"],
  ] as const)("classifies %s", (input, kind) => {
    expect(classifyScriptureCommand(input)).toBe(kind);
  });
});
