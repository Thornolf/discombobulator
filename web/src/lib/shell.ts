export type Shell = "desktop" | "phone";

export function pickShell(search: string, coarsePointer: boolean, width: number): Shell {
  const requested = new URLSearchParams(search).get("ui");
  if (requested === "phone" || requested === "desktop") {
    return requested;
  }
  return coarsePointer && width < 1100 ? "phone" : "desktop";
}
