import { render } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, expect, it } from 'vitest';
import { CoverageBar } from '../CoverageBar';

describe('CoverageBar', () => {
  it('커버 비율만큼 초록, 나머지 앰버 폭', () => {
    render(<CoverageBar covered={3} total={4} />);
    const bar = document.querySelector('[data-coverage-bar]') as HTMLElement;
    const [green, amber] = Array.from(bar.querySelectorAll('i')) as HTMLElement[];
    expect(green.style.width).toBe('75%');
    expect(amber.style.width).toBe('25%');
    expect(green.style.background).toBe('rgb(65, 194, 133)'); // #41c285
    expect(amber.style.background).toBe('rgb(240, 180, 41)'); // #f0b429
  });
});
