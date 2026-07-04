/** 지시문 diff 렌더용 라인 diff — LCS 기반, 결정론. */
import { describe, expect, it } from 'vitest';
import { lineDiff } from '../lineDiff';

describe('lineDiff', () => {
  it('추가·삭제·동일 라인을 구분한다', () => {
    const d = lineDiff('a\nb\nc\n', 'a\nx\nc\n');
    expect(d).toEqual([
      { type: 'same', text: 'a' },
      { type: 'del', text: 'b' },
      { type: 'add', text: 'x' },
      { type: 'same', text: 'c' },
    ]);
  });
  it('빈 쪽은 전부 add/del', () => {
    expect(lineDiff('', 'a\n')).toEqual([{ type: 'add', text: 'a' }]);
    expect(lineDiff('a\n', '')).toEqual([{ type: 'del', text: 'a' }]);
  });
});
