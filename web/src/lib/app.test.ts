import { describe, expect, it } from "vitest";
import { appName } from "./app";

describe("appName", () => {
  it("is the product name", () => {
    expect(appName).toBe("Discombobulator");
  });
});
