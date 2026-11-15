export type Action =
  | "approve"
  | "delete"
  | "unsure"
  | "favorite"
  | "clear"
  | "undo"
  | "rate0"
  | "rate1"
  | "rate2"
  | "rate3"
  | "rate4"
  | "rate5"
  | "focusPrev"
  | "focusNext"
  | "stackPrev"
  | "stackNext"
  | "scenePrev"
  | "sceneNext"
  | "solo"
  | "back"
  | "nextUndecided"
  | "apply"
  | "select"
  | "keep"
  | "duplicates"
  | "open"
  | "help"
  | "info"
  | "focusPoint"
  | "histogram"
  | "histogramMode"
  | "clipping";

export interface Binding {
  key?: string;
  code?: string;
  ctrl?: boolean;
  shift?: boolean;
}

export type Keymap = Record<Action, Binding[]>;

const digit = (n: number): Binding[] => [{ code: `Digit${n}` }, { code: `Numpad${n}` }];

export const defaultKeymap: Keymap = {
  approve: [{ key: "a" }, { key: " " }, { key: "l" }],
  delete: [{ key: "x" }, { key: "Delete" }],
  unsure: [{ key: "u" }],
  favorite: [{ key: "f" }],
  clear: [{ key: "Backspace" }],
  undo: [{ key: "z", ctrl: true }],
  rate0: digit(0),
  rate1: digit(1),
  rate2: digit(2),
  rate3: digit(3),
  rate4: digit(4),
  rate5: digit(5),
  focusPrev: [{ key: "ArrowLeft" }],
  focusNext: [{ key: "ArrowRight" }],
  stackPrev: [{ key: "ArrowUp" }],
  stackNext: [{ key: "ArrowDown" }],
  scenePrev: [{ key: "PageUp" }, { key: "ArrowUp", ctrl: true }],
  sceneNext: [{ key: "PageDown" }, { key: "ArrowDown", ctrl: true }],
  solo: [{ key: "Enter" }],
  back: [{ key: "Escape" }],
  nextUndecided: [{ key: "n" }],
  apply: [{ key: "e" }],
  select: [{ key: "s" }],
  keep: [{ key: "k" }],
  duplicates: [{ key: "d" }],
  open: [{ key: "o" }],
  help: [{ key: "?" }, { key: "h" }],
  info: [{ key: "i" }],
  focusPoint: [{ key: "p" }],
  histogram: [{ key: "g" }],
  histogramMode: [{ key: "g", shift: true }],
  clipping: [{ key: "c" }],
};

export const actionLabels: Record<Action, string> = {
  approve: "LR approved",
  delete: "Delete",
  unsure: "Unsure",
  favorite: "Favorite",
  clear: "Clear mark",
  undo: "Undo",
  rate0: "Clear rating",
  rate1: "1 star",
  rate2: "2 stars",
  rate3: "3 stars",
  rate4: "4 stars",
  rate5: "5 stars",
  focusPrev: "Previous frame",
  focusNext: "Next frame",
  stackPrev: "Previous stack",
  stackNext: "Next stack",
  scenePrev: "Previous scene",
  sceneNext: "Next scene",
  solo: "Solo view",
  back: "Leave solo / close",
  nextUndecided: "Next undecided stack",
  apply: "Apply",
  select: "Select frame",
  keep: "Keep selected, delete undecided",
  duplicates: "Duplicates",
  open: "Open folder",
  help: "Help",
  info: "Show or hide shot info",
  focusPoint: "Show or hide the focus point",
  histogram: "Show or hide the histogram",
  histogramMode: "Histogram: colors or brightness",
  clipping: "Show or hide clipped highlights and shadows",
};

export interface KeyInput {
  key: string;
  code: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey?: boolean;
}

export function matches(binding: Binding, event: KeyInput): boolean {
  if (event.altKey) {
    return false;
  }
  const ctrl = event.ctrlKey || event.metaKey;
  if (Boolean(binding.ctrl) !== ctrl) {
    return false;
  }
  if (binding.shift && !event.shiftKey) {
    return false;
  }
  if (binding.code) {
    return binding.code === event.code;
  }
  if (!binding.key) {
    return false;
  }
  return event.key.length === 1 ? event.key.toLowerCase() === binding.key : event.key === binding.key;
}

export function actionFor(keymap: Keymap, event: KeyInput): Action | null {
  const entries = Object.entries(keymap) as [Action, Binding[]][];
  for (const withShift of [true, false]) {
    for (const [action, bindings] of entries) {
      if (bindings.some((binding) => Boolean(binding.shift) === withShift && matches(binding, event))) {
        return action;
      }
    }
  }
  return null;
}

function shiftMatters(event: KeyInput): boolean {
  return Boolean(event.shiftKey) && (event.key.length > 1 || /^\p{L}$/u.test(event.key));
}

export function bindingFor(event: KeyInput): Binding {
  const ctrl = event.ctrlKey || event.metaKey;
  if (/^(Digit|Numpad)[0-9]$/.test(event.code)) {
    return { code: event.code, ctrl };
  }
  const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
  return shiftMatters(event) ? { key, ctrl, shift: true } : { key, ctrl };
}

export function describe(binding: Binding): string {
  const names: Record<string, string> = {
    " ": "Space",
    ArrowLeft: "←",
    ArrowRight: "→",
    ArrowUp: "↑",
    ArrowDown: "↓",
    Backspace: "⌫",
    Delete: "Del",
    Escape: "Esc",
    PageUp: "PgUp",
    PageDown: "PgDn",
  };
  const base = binding.code
    ? binding.code.replace(/^Digit/, "").replace(/^Numpad/, "Num ")
    : (names[binding.key ?? ""] ?? (binding.key ?? "").toUpperCase());
  return `${binding.ctrl ? "Ctrl+" : ""}${binding.shift ? "Shift+" : ""}${base}`;
}

const storageKey = "discombobulator.keymap";

const replacedDefaults: Partial<Record<Action, string>> = {
  scenePrev: JSON.stringify([{ key: "PageUp" }]),
  sceneNext: JSON.stringify([{ key: "PageDown" }]),
};

export function sameBinding(one: Binding, other: Binding): boolean {
  return (
    one.key === other.key &&
    one.code === other.code &&
    Boolean(one.ctrl) === Boolean(other.ctrl) &&
    Boolean(one.shift) === Boolean(other.shift)
  );
}

export function restoreKeymap(stored: Partial<Keymap> | null): Keymap {
  const keymap = { ...defaultKeymap };
  const custom = new Set<Action>();
  for (const [action, bindings] of Object.entries(stored ?? {}) as [Action, Binding[]][]) {
    if (action in defaultKeymap && JSON.stringify(bindings) !== replacedDefaults[action]) {
      keymap[action] = bindings;
      custom.add(action);
    }
  }
  const claimed = [...custom].flatMap((action) => keymap[action]);
  for (const action of Object.keys(keymap) as Action[]) {
    if (!custom.has(action)) {
      keymap[action] = keymap[action].filter(
        (binding) => !claimed.some((taken) => sameBinding(taken, binding)),
      );
    }
  }
  return keymap;
}

export function changedFromDefault(keymap: Keymap): Partial<Keymap> {
  return Object.fromEntries(
    (Object.entries(keymap) as [Action, Binding[]][]).filter(
      ([action, bindings]) => JSON.stringify(bindings) !== JSON.stringify(defaultKeymap[action]),
    ),
  );
}

export function loadKeymap(): Keymap {
  try {
    return restoreKeymap(JSON.parse(localStorage.getItem(storageKey) ?? "null") as Partial<Keymap> | null);
  } catch {
    return { ...defaultKeymap };
  }
}

export function saveKeymap(keymap: Keymap): void {
  try {
    localStorage.setItem(storageKey, JSON.stringify(changedFromDefault(keymap)));
  } catch {
    return;
  }
}

export function rebind(keymap: Keymap, action: Action, binding: Binding): Keymap {
  const same = (other: Binding) => sameBinding(other, binding);
  const next = Object.fromEntries(
    Object.entries(keymap).map(([name, bindings]) => [name, bindings.filter((other) => !same(other))]),
  ) as Keymap;
  next[action] = [binding];
  return next;
}
