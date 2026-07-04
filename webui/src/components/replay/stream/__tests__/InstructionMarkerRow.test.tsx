/** B-12 마커 행 — 시각 표기는 스트림 가터와 같은 로컬 시계(clockLabel)를 쓴다.
 *  (라이브 스모크에서 가터 21:04 vs 마커 12:04(UTC 슬라이스) 불일치 발견.) */
import { screen } from '@testing-library/react';
import { renderWithI18n as render } from '../../../../test/i18nRender';
import { describe, expect, it } from 'vitest';
import { InstructionMarkerRow } from '../InstructionMarkerRow';
import { clockLabel } from '../../../../lib/format';
import type { InstructionMarkerItem } from '../streamModel';

const item: InstructionMarkerItem = {
  type: 'instruction-marker',
  id: 'im_1',
  observedAt: '2026-07-04T12:04:15.194378+00:00',
  source: 'project',
  beforeHash: '108f3537aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
  afterHash: 'dbc53211bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
};

describe('InstructionMarkerRow', () => {
  it('마커 시각 = 가터와 동일 포맷터(clockLabel, 로컬 시계)', () => {
    render(<InstructionMarkerRow item={item} />);
    const expected = clockLabel(Date.parse(item.observedAt));
    expect(screen.getByRole('button', { name: new RegExp(expected) })).toBeInTheDocument();
  });
});
