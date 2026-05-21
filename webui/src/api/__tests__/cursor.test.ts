import { describe, it, expect, beforeEach } from 'vitest';
import { readCursor, writeCursor, clearCursor } from '../cursor';

describe('cursor', () => {
  beforeEach(() => sessionStorage.clear());

  it('readCursor returns null when nothing stored', () => {
    expect(readCursor('global')).toBeNull();
    expect(readCursor('sess-1')).toBeNull();
  });

  it('writeCursor + readCursor roundtrips', () => {
    writeCursor('sess-1', '01HZZZ000000000000000000A');
    expect(readCursor('sess-1')).toBe('01HZZZ000000000000000000A');
  });

  it('cursors are scoped independently', () => {
    writeCursor('global', 'A');
    writeCursor('sess-1', 'B');
    expect(readCursor('global')).toBe('A');
    expect(readCursor('sess-1')).toBe('B');
  });

  it('clearCursor removes only the scoped key', () => {
    writeCursor('global', 'A');
    writeCursor('sess-1', 'B');
    clearCursor('sess-1');
    expect(readCursor('global')).toBe('A');
    expect(readCursor('sess-1')).toBeNull();
  });
});
