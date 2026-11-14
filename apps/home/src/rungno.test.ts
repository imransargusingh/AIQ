// Control for DC-077: a rung's number in brackets beside counts ("Assisted (1)" over "286 of 287 tasks") reads as
// a task count. Home shows the number as a badge (`RungNo`); no source may write the bracketed form again.
import { describe, expect, it } from "vitest";

const RUNG_IN_BRACKETS = /\b(Assisted|Directed|Delegated|Orchestrated|Factory) \(\s*[1-5]\s*\)|\.name\} \(\$\{|\} \(\{(r|i \+ 1|rung)\}\)/;
const files = import.meta.glob(["./**/*.{ts,tsx,css}", "!./**/*.test.ts"], { query: "?raw", import: "default", eager: true }) as Record<string, string>;

describe("rung numbers", () => {
  it("never appear in brackets after a rung name in Home's source", () => {
    expect(Object.keys(files).length).toBeGreaterThan(20);
    const hits = Object.entries(files).flatMap(([p, src]) =>
      src
        .split("\n")
        .flatMap((l, i) => (RUNG_IN_BRACKETS.test(l) ? [`${p}:${i + 1}: ${l.trim()}`] : [])),
    );
    expect(hits).toEqual([]);
  });
});
