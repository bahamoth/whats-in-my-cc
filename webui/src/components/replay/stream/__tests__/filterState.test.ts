import { describe, expect, it } from 'vitest';
import {
  EMPTY_FILTER, filterFromSearch, filterKey, filterToSearch,
  isFilterActive, toEventFilterParams, type FilterState,
} from '../filterState';

const sample: FilterState = {
  ...EMPTY_FILTER,
  kinds: ['tool_call', 'tool_result'],
  origins: ['human'],
  error: true,
  q: 'panic',
};

describe('filterState', () => {
  it('EMPTY_FILTER is inactive; any axis activates', () => {
    expect(isFilterActive(EMPTY_FILTER)).toBe(false);
    expect(isFilterActive(sample)).toBe(true);
    expect(isFilterActive({ ...EMPTY_FILTER, signal: true })).toBe(true);
  });

  it('toEventFilterParams emits spec §1.2 param names, omitting inactive axes', () => {
    expect(toEventFilterParams(sample)).toEqual({
      kind: 'tool_call,tool_result', origin: 'human', error: 'true', q: 'panic',
    });
    expect(toEventFilterParams(EMPTY_FILTER)).toEqual({});
  });

  it('URL round-trip is lossless and prunes stale f_* keys', () => {
    const sp = new URLSearchParams('selected=EV1&f_model=claude-fable-5');
    filterToSearch(sample, sp);
    expect(sp.get('selected')).toBe('EV1');       // 비필터 키 보존
    expect(sp.get('f_model')).toBeNull();          // 비활성 축 제거
    expect(filterFromSearch(sp)).toEqual(sample);
  });

  it('filterKey changes iff filter content changes', () => {
    expect(filterKey(sample)).not.toBe(filterKey(EMPTY_FILTER));
    expect(filterKey({ ...sample })).toBe(filterKey(sample));
  });

  it('filterKey is order-insensitive within an axis (no spurious window reset)', () => {
    const a: FilterState = { ...EMPTY_FILTER, kinds: ['tool_call', 'tool_result'] };
    const b: FilterState = { ...EMPTY_FILTER, kinds: ['tool_result', 'tool_call'] };
    expect(filterKey(a)).toBe(filterKey(b));
  });
});
