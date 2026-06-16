// l10n — catalog parity. The Messages type already forces ko.ts to match en.ts
// at compile time; this is the runtime belt-and-suspenders that fails loudly if
// the two ever drift (e.g. a key added to en but not ko during migration).
import { describe, expect, it } from 'vitest';
import { en } from '../catalog/en';
import { ko } from '../catalog/ko';

describe('catalog parity', () => {
  it('ko has exactly the same keys as en', () => {
    expect(Object.keys(ko).sort()).toEqual(Object.keys(en).sort());
  });

  it('no message value is undefined in either catalog', () => {
    for (const [key, value] of Object.entries(en)) {
      expect(value, `en[${key}]`).toBeDefined();
    }
    for (const [key, value] of Object.entries(ko)) {
      expect(value, `ko[${key}]`).toBeDefined();
    }
  });
});
