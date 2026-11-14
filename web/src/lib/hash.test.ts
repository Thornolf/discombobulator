import { describe, expect, it } from "vitest";
import { hamming } from "./hash";

describe("hamming", () => {
  it("counts differing bits across both halves", () => {
    expect(hamming("0000000000000000", "0000000000000000")).toBe(0);
    expect(hamming("ffffffffffffffff", "0000000000000000")).toBe(64);
    expect(hamming("8000000000000001", "0000000000000000")).toBe(2);
  });

  it("is null when a hash is missing or malformed", () => {
    expect(hamming(null, "0000000000000000")).toBeNull();
    expect(hamming("zz", "0000000000000000")).toBeNull();
  });
});
