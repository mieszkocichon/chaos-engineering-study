// Shared helpers for golden generators. Files starting with "_" are skipped by scripts/golden.sh.
import { createHash } from 'crypto';

export const bits = (x) => {
  if (x === null || x === undefined) return null;
  const b = Buffer.alloc(8);
  b.writeDoubleLE(x);
  return b.readBigUInt64LE().toString(16);
};
export const sha256 = (s) => createHash('sha256').update(s).digest('hex');
export const ROW_FIELDS = ['t', 'goodput', 'offered', 'fresh', 'retry', 'wasted', 'shed', 'timeout', 'queueLen', 'inService'];
export const rowsDigest = (rows) =>
  sha256(rows.map((r) => ROW_FIELDS.map((f) => bits(r[f])).join(',')).join('\n'));
