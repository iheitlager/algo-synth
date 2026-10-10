// Keep the caret where it was when an editor's text is replaced from outside
// (#458): autocommit swaps the typed text for the engine's printed one
// (ADR-0027), and a textarea given a new value puts its caret at the end.

/**
 * Where `caret` in `before` lands in `after`: the same place counted from the
 * start when it is in the text both share at the start, the same place counted
 * from the end when it is in the text they share at the end, else the end of
 * the part that changed.
 */
export function keepCaret(before: string, after: string, caret: number): number {
  const most = Math.min(before.length, after.length)
  let head = 0
  while (head < most && before[head] === after[head]) head++
  let tail = 0
  while (tail < most - head && before[before.length - 1 - tail] === after[after.length - 1 - tail]) tail++
  if (caret <= head) return caret
  if (caret >= before.length - tail) return after.length - (before.length - caret)
  return after.length - tail
}
