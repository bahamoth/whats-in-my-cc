/** InfoTip 마크업 렌더러 — 굵게/색/코드 토큰이 실제 요소로 변환되는지 잠근다. */
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { describe, expect, it, afterEach } from 'vitest';
import { InfoTip } from '../InfoTip';
import { I18nProvider } from '../../../../i18n';

afterEach(cleanup);

describe('InfoTip 마크업', () => {
  it('**굵게**·[green]색[/green]·`코드`를 요소로 렌더한다', () => {
    render(
      <I18nProvider initialLocale="ko">
        <InfoTip label="테스트" text={'수식 **A ÷ B** 그리고 [green]통과[/green] 와 `exit code`'} />
      </I18nProvider>,
    );
    fireEvent.click(screen.getByTestId('infotip-trigger'));
    const tip = screen.getByRole('tooltip');
    const bolds = tip.querySelectorAll('b');
    expect(bolds.length).toBe(2);
    expect(bolds[0].textContent).toBe('A ÷ B');
    expect(bolds[1].textContent).toBe('통과');
    expect((bolds[1] as HTMLElement).style.color).toBeTruthy();
    expect(tip.querySelector('code')?.textContent).toBe('exit code');
  });
});
