import { describe, expect, it } from "vitest";
import {
  actionFor,
  bindingFor,
  changedFromDefault,
  defaultKeymap,
  describe as describeBinding,
  rebind,
  restoreKeymap,
} from "./keymap";

const key = (
  key: string,
  code: string,
  extra: Partial<{ ctrlKey: boolean; altKey: boolean; shiftKey: boolean }> = {},
) => ({
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

  it("stores only changed actions so new default keys reach remapped keymaps", () => {
    const remapped = rebind(defaultKeymap, "approve", { key: "q" });
    const stored = changedFromDefault(remapped);
    expect(Object.keys(stored)).toEqual(["approve"]);
    expect(restoreKeymap(stored).sceneNext).toEqual(defaultKeymap.sceneNext);
    expect(restoreKeymap({ ...defaultKeymap, sceneNext: [{ key: "PageDown" }] }).sceneNext).toEqual(
      defaultKeymap.sceneNext,
    );
    expect(restoreKeymap({ sceneNext: [{ key: "j" }] }).sceneNext).toEqual([{ key: "j" }]);
  });

  it("gives every default key to exactly one action", () => {
    for (const [action, bindings] of Object.entries(defaultKeymap)) {
      for (const binding of bindings) {
        const event = binding.code
          ? key("", binding.code, { ctrlKey: Boolean(binding.ctrl), shiftKey: Boolean(binding.shift) })
          : key(binding.shift ? (binding.key ?? "").toUpperCase() : (binding.key ?? ""), "", {
              ctrlKey: Boolean(binding.ctrl),
              shiftKey: Boolean(binding.shift),
            });
        expect(actionFor(defaultKeymap, event), describeBinding(binding)).toBe(action);
      }
    }
  });

  it("tells Shift+G from G", () => {
    expect(actionFor(defaultKeymap, key("g", "KeyG"))).toBe("histogram");
    expect(actionFor(defaultKeymap, key("G", "KeyG", { shiftKey: true }))).toBe("histogramMode");
    expect(actionFor(defaultKeymap, key("A", "KeyQ", { shiftKey: true }))).toBe("approve");
    expect(describeBinding({ key: "g", shift: true })).toBe("Shift+G");
    expect(bindingFor(key("G", "KeyG", { shiftKey: true }))).toEqual({ key: "g", ctrl: false, shift: true });
    expect(bindingFor(key("?", "Comma", { shiftKey: true }))).toEqual({ key: "?", ctrl: false });
  });

  it("keeps a custom key over a new default that uses the same key", () => {
    const restored = restoreKeymap({ approve: [{ key: "p" }] });
    expect(restored.approve).toEqual([{ key: "p" }]);
    expect(restored.focusPoint).toEqual([]);
    expect(actionFor(restored, key("p", "KeyP"))).toBe("approve");
    expect(restoreKeymap(null).focusPoint).toEqual([{ key: "p" }]);
  });

  it("moves between scenes with Ctrl and the vertical arrows", () => {
    expect(actionFor(defaultKeymap, key("ArrowUp", "ArrowUp", { ctrlKey: true }))).toBe("scenePrev");
    expect(actionFor(defaultKeymap, key("ArrowDown", "ArrowDown", { ctrlKey: true }))).toBe("sceneNext");
    expect(actionFor(defaultKeymap, key("ArrowDown", "ArrowDown"))).toBe("stackNext");
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
