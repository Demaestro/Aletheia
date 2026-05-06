/**
 * Unit tests for parseScriptureReference()
 *
 * These tests also document the expected behaviour of the Rust-side
 * parse_reference() function they mirror.
 */

import { describe, it, expect } from "vitest";
import { parseScriptureReference, BOOK_MAP } from "./scriptureReference";

// ---------------------------------------------------------------------------
// Happy paths
// ---------------------------------------------------------------------------

describe("parseScriptureReference — happy paths", () => {
  it("parses a standard Book Chapter:Verse reference", () => {
    const result = parseScriptureReference("Romans 8:28");
    expect(result).not.toBeNull();
    expect(result!.bookCode).toBe("ROM");
    expect(result!.chapter).toBe(8);
    expect(result!.verse).toBe(28);
    expect(result!.apiBibleId).toBe("ROM.8.28");
  });

  it("parses the canonical example — John 3:16", () => {
    const result = parseScriptureReference("John 3:16");
    expect(result!.apiBibleId).toBe("JHN.3.16");
  });

  it("parses a minor-prophet reference — Habakkuk 3:17", () => {
    const result = parseScriptureReference("Habakkuk 3:17");
    expect(result!.apiBibleId).toBe("HAB.3.17");
  });

  it("parses a Psalm reference (full name)", () => {
    const result = parseScriptureReference("Psalms 23:1");
    expect(result!.bookCode).toBe("PSA");
    expect(result!.apiBibleId).toBe("PSA.23.1");
  });

  it("parses psalm with singular form", () => {
    const result = parseScriptureReference("Psalm 119:11");
    expect(result!.apiBibleId).toBe("PSA.119.11");
  });

  it("parses a multi-word book — 1 Corinthians 13:4", () => {
    const result = parseScriptureReference("1 Corinthians 13:4");
    expect(result!.bookCode).toBe("1CO");
    expect(result!.apiBibleId).toBe("1CO.13.4");
  });

  it("parses Song of Solomon", () => {
    const result = parseScriptureReference("Song of Solomon 2:4");
    expect(result!.bookCode).toBe("SNG");
  });

  it("parses Song of Songs", () => {
    const result = parseScriptureReference("Song of Songs 2:4");
    expect(result!.bookCode).toBe("SNG");
  });

  it("parses a 3-word numbered book — 1 Chronicles 29:11", () => {
    const result = parseScriptureReference("1 Chronicles 29:11");
    expect(result!.apiBibleId).toBe("1CH.29.11");
  });

  it("accepts dot as verse delimiter", () => {
    const result = parseScriptureReference("Romans 8.28");
    expect(result!.apiBibleId).toBe("ROM.8.28");
  });

  it("accepts comma as verse delimiter", () => {
    const result = parseScriptureReference("Romans 8,28");
    expect(result!.apiBibleId).toBe("ROM.8.28");
  });

  it("accepts Nigerian space-separated chapter and verse", () => {
    expect(parseScriptureReference("Matthew 11 12")!.apiBibleId).toBe("MAT.11.12");
    expect(parseScriptureReference("Acts 12 14")!.apiBibleId).toBe("ACT.12.14");
    expect(parseScriptureReference("Daniel 3 10")!.apiBibleId).toBe("DAN.3.10");
    expect(parseScriptureReference("Acts 11 8")!.apiBibleId).toBe("ACT.11.8");
  });

  it("accepts STT-joined chapter and verse", () => {
    expect(parseScriptureReference("John 316")!.apiBibleId).toBe("JHN.3.16");
    expect(parseScriptureReference("Matthew 1112")!.apiBibleId).toBe("MAT.11.12");
    expect(parseScriptureReference("Acts 1214")!.apiBibleId).toBe("ACT.12.14");
    expect(parseScriptureReference("Acts 126")!.apiBibleId).toBe("ACT.12.6");
    expect(parseScriptureReference("1 Timothy 52")!.apiBibleId).toBe("1TI.5.2");
    expect(parseScriptureReference("Daniel 310")!.apiBibleId).toBe("DAN.3.10");
    expect(parseScriptureReference("Acts 118")!.apiBibleId).toBe("ACT.11.8");
  });

  it("is case-insensitive for the book name", () => {
    expect(parseScriptureReference("JOHN 3:16")).not.toBeNull();
    expect(parseScriptureReference("john 3:16")).not.toBeNull();
    expect(parseScriptureReference("JoHn 3:16")).not.toBeNull();
  });

  it("trims leading/trailing whitespace", () => {
    const result = parseScriptureReference("  Ephesians 2:8  ");
    expect(result!.bookCode).toBe("EPH");
  });
});

// ---------------------------------------------------------------------------
// Abbreviation handling
// ---------------------------------------------------------------------------

describe("parseScriptureReference — abbreviations", () => {
  const cases: Array<[string, string]> = [
    ["Hab 3:17",    "HAB.3.17"],
    ["Ps 23:1",     "PSA.23.1"],
    ["Isa 40:31",   "ISA.40.31"],
    ["Jer 29:11",   "JER.29.11"],
    ["Matt 5:16",   "MAT.5.16"],
    ["Mk 16:15",    "MRK.16.15"],
    ["Lk 4:18",     "LUK.4.18"],
    ["Jn 14:6",     "JHN.14.6"],
    ["Rom 8:28",    "ROM.8.28"],
    ["1 Cor 13:4",  "1CO.13.4"],
    ["2 Cor 12:9",  "2CO.12.9"],
    ["Eph 2:8",     "EPH.2.8"],
    ["Phil 4:13",    "PHP.4.13"],
    ["1 Pet 5:7",   "1PE.5.7"],
    ["Rev 22:20",   "REV.22.20"],
    ["Gen 1:1",     "GEN.1.1"],
  ];

  it.each(cases)("parses abbreviation %s → %s", (input, expected) => {
    expect(parseScriptureReference(input)?.apiBibleId).toBe(expected);
  });
});

// ---------------------------------------------------------------------------
// Null / invalid inputs
// ---------------------------------------------------------------------------

describe("parseScriptureReference — rejects invalid input", () => {
  it("returns null for a bare book name", () => {
    expect(parseScriptureReference("John")).toBeNull();
  });

  it("returns null for a book without a verse", () => {
    expect(parseScriptureReference("John 3")).toBeNull();
  });

  it("returns null for an unknown book name", () => {
    expect(parseScriptureReference("Esdras 1:1")).toBeNull();
  });

  it("returns null for an empty string", () => {
    expect(parseScriptureReference("")).toBeNull();
  });

  it("returns null for plain text with no reference", () => {
    expect(parseScriptureReference("God is good all the time")).toBeNull();
  });

  it("returns null for chapter 0 (invalid)", () => {
    // Even if the regex matches, chapter 0 should be rejected
    expect(parseScriptureReference("John 0:1")).toBeNull();
  });

  it("returns null for verse 0 (invalid)", () => {
    expect(parseScriptureReference("John 3:0")).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// BOOK_MAP coverage sanity
// ---------------------------------------------------------------------------

describe("BOOK_MAP completeness", () => {
  it("covers all 66 canonical books (at least one key per book)", () => {
    const expectedCodes = new Set([
      "GEN","EXO","LEV","NUM","DEU","JOS","JDG","RUT","1SA","2SA",
      "1KI","2KI","1CH","2CH","EZR","NEH","EST","JOB","PSA","PRO",
      "ECC","SNG","ISA","JER","LAM","EZK","DAN","HOS","JOL","AMO",
      "OBA","JON","MIC","NAM","HAB","ZEP","HAG","ZEC","MAL",
      "MAT","MRK","LUK","JHN","ACT","ROM","1CO","2CO","GAL","EPH",
      "PHP","COL","1TH","2TH","1TI","2TI","TIT","PHM","HEB","JAS",
      "1PE","2PE","1JN","2JN","3JN","JUD","REV",
    ]);

    const coveredCodes = new Set(Object.values(BOOK_MAP));
    // Every expected code must be reachable via at least one key
    for (const code of expectedCodes) {
      expect(coveredCodes, `Missing coverage for ${code}`).toContain(code);
    }
  });
});
