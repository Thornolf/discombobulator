function popcount(value: number): number {
  let x = value - ((value >>> 1) & 0x55555555);
  x = (x & 0x33333333) + ((x >>> 2) & 0x33333333);
  return (Math.imul((x + (x >>> 4)) & 0x0f0f0f0f, 0x01010101) >>> 24) & 0xff;
}

export function hamming(a: string | null, b: string | null): number | null {
  if (!a || !b || a.length !== 16 || b.length !== 16) {
    return null;
  }
  const high = parseInt(a.slice(0, 8), 16) ^ parseInt(b.slice(0, 8), 16);
  const low = parseInt(a.slice(8), 16) ^ parseInt(b.slice(8), 16);
  if (Number.isNaN(high) || Number.isNaN(low)) {
    return null;
  }
  return popcount(high >>> 0) + popcount(low >>> 0);
}
