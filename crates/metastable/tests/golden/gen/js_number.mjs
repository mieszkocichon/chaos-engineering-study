// Golden vectors for js::js_number / js_to_fixed (research R7). Run via scripts/golden.sh.
import { writeFileSync } from 'fs';
const vals = [
  0, -0, 1, -1, 50, 0.1, 0.2, 0.1 + 0.2, 1 / 3, 2 / 3, 123.456, -123.456, 1e21, 1e20, 123456789012345680000,
  1e-6, 1e-7, 1.5e-7, 0.000001234, 5e-324, Number.MAX_VALUE, Number.MIN_VALUE, 2 ** 53, 2 ** 53 + 2,
  0.125, 0.375, 1.005, 2.5, 3.5, -2.5, 0.5, 1.45, 8.345, 10.235, 1e15, 1e16, 1.7976931348623157e308,
  99.99, 0.0001, 0.00001, 4.35, 0.615, 1234.5678, 0.04999999999999999, 3000, 0.9999999999999999,
  6.02214076e23, 1.602176634e-19, 299792458, 0.3, 0.7, 0.05, -0.0004, 12.5, 0.4567, 1e-10,
];
const out = vals.map(v => ({
  bits: Buffer.from(new Float64Array([v]).buffer).readBigUInt64LE().toString(16),
  toString: String(v),
  toFixed0: v.toFixed(0), toFixed1: v.toFixed(1), toFixed2: v.toFixed(2), toFixed4: v.toFixed(4),
}));
writeFileSync(new URL('../js_number.json', import.meta.url), JSON.stringify(out, null, 2) + '\n');
