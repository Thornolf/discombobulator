import { describe, expect, it } from "vitest";
import { actionFor, bindingFor, defaultKeymap, describe as describeBinding, rebind } from "./keymap";

const key = (key: string, code: string, extra: Partial<{ ctrlKey: boolean; altKey: boolean }> = {}) => ({
  key,
  code,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  ...extra,
});

describe("keymap", () => {
  it("matches letters by the produced character on AZERTY and QWERTY", () => {
    expect(actionFor(defaultKeymap, key("a", "KeyQ"))).toBe("approve");
    expect(actionFor(defaultKeymap, key("A", "KeyQ"))).toBe("approve");
    expect(actionFor(defaultKeymap, key("q", "KeyA"))).toBeNull();
  });

  it("matches digits by physical key so AZERTY needs no Shift", () => {
    expect(actionFor(defaultKeymap, key("&", "Digit1"))).toBe("rate1");
    expect(actionFor(defaultKeymap, key("é", "Digit2"))).toBe("rate2");
    expect(actionFor(defaultKeymap, key("5", "Numpad5"))).toBe("rate5");
    expect(actionFor(defaultKeymap, key("à", "Digit0"))).toBe("rate0");
  });

  it("ignores AltGr combinations and requires Ctrl for undo", () => {
    expect(actionFor(defaultKeymap, key("@", "Digit0", { ctrlKey: true, altKey: true }))).toBeNull();
    expect(actionFor(defaultKeymap, key("z", "KeyW", { ctrlKey: true }))).toBe("undo");
    expect(actionFor(defaultKeymap, key("z", "KeyW"))).toBeNull();
  });

  it("default bindings never use AltGr characters", () => {
    const altGr = new Set(["[", "]", "{", "}", "\\", "@", "#"]);
    const keys = Object.values(defaultKeymap).flatMap((bindings) => bindings.map((b) => b.key ?? ""));
    expect(keys.filter((k) => altGr.has(k))).toEqual([]);
  });

  it("rebinding moves a key from its old action", () => {
    const next = rebind(defaultKeymap, "keep", bindingFor(key("x", "KeyX")));
    expect(actionFor(next, key("x", "KeyX"))).toBe("keep");
    expect(next.delete).toEqual([{ key: "Delete", ctrl: false }].map(({ key }) => ({ key })));
  });

  it("describes bindings for humans", () => {
    expect(describeBinding({ key: " " })).toBe("Space");
    expect(describeBinding({ code: "Digit3" })).toBe("3");
    expect(describeBinding({ key: "z", ctrl: true })).toBe("Ctrl+Z");
  });
});
