// PR-3 §3a — DeltaChip 공용화. 대시보드 HeadlineStats에서 추출한 계약을 잠근다:
// null → noCompare 텍스트, |v|<0.05 → ▬, 방향×betterUp → 초록/앰버.
import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { DeltaChip, trim1 } from '../DeltaChip';

describe('DeltaChip', () => {
  it('v가 null이면 noCompare 텍스트만 렌더한다', () => {
    render(<DeltaChip v={null} unit="%p" betterUp noCompare="비교 없음" />);
    expect(screen.getByText('비교 없음')).toBeInTheDocument();
  });

  it('|v| < 0.05는 ▬(무변화)로 렌더한다', () => {
    render(<DeltaChip v={0.01} unit="%p" betterUp noCompare="-" />);
    expect(screen.getByText(/▬/)).toBeInTheDocument();
  });

  it('상승이 좋은 지표(betterUp)의 +delta는 초록 계열 클래스를 얻는다', () => {
    render(<DeltaChip v={2.4} unit="%p" betterUp noCompare="-" />);
    const chip = screen.getByText(/▲ 2.4%p/);
    expect(chip.className).toContain('text-[#41c285]');
  });

  it('상승이 나쁜 지표(betterUp=false)의 +delta는 앰버 계열 클래스를 얻는다', () => {
    render(<DeltaChip v={1.2} unit="$" betterUp={false} noCompare="-" />);
    const chip = screen.getByText(/▲ 1.2\$/);
    expect(chip.className).toContain('text-[#f0b429]');
  });
});

describe('trim1', () => {
  it('소수 1자리로 반올림한다', () => {
    expect(trim1(1.26)).toBe('1.3');
    expect(trim1(2)).toBe('2');
  });
});
