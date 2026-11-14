import { describe, expect, it } from "vitest";
import { pickShell } from "./shell";

describe("pickShell", () => {
  it("honours an explicit choice", () => {
    expect(pickShell("?ui=phone", false, 2000)).toBe("phone");
    expect(pickShell("?ui=desktop", true, 400)).toBe("desktop");
  });

  it("uses the phone shell on touch screens narrower than a laptop", () => {
    expect(pickShell("", true, 390)).toBe("phone");
    expect(pickShell("", false, 390)).toBe("desktop");
    expect(pickShell("", true, 1400)).toBe("desktop");
  });
});
