import { describe, expect, it } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { InfoTip } from '../InfoTip';

describe('InfoTip', () => {
  it('is closed by default — no tooltip in the DOM', () => {
    render(<InfoTip label="cache" text="cache-hit explanation" />);
    expect(screen.queryByRole('tooltip')).toBeNull();
  });

  it('opens on hover and shows the explanatory text', () => {
    render(<InfoTip label="cache" text="cache-hit explanation" />);
    fireEvent.mouseEnter(screen.getByTestId('infotip-trigger'));
    expect(screen.getByRole('tooltip')).toHaveTextContent('cache-hit explanation');
  });

  it('closes again on mouse leave when not click-pinned', () => {
    render(<InfoTip label="cache" text="explain" />);
    const t = screen.getByTestId('infotip-trigger');
    fireEvent.mouseEnter(t);
    fireEvent.mouseLeave(t);
    expect(screen.queryByRole('tooltip')).toBeNull();
  });

  it('click pins it open; mouse leave then keeps it open; second click closes it', () => {
    render(<InfoTip label="cache" text="explain" />);
    const t = screen.getByTestId('infotip-trigger');
    fireEvent.click(t);
    expect(screen.getByRole('tooltip')).toBeInTheDocument();
    fireEvent.mouseLeave(t);
    expect(screen.getByRole('tooltip')).toBeInTheDocument(); // pinned
    fireEvent.click(t);
    expect(screen.queryByRole('tooltip')).toBeNull(); // unpinned + not hovered
  });

  it('the trigger does not bubble its click to an enclosing handler', () => {
    let outer = 0;
    render(
      <div onClick={() => { outer += 1; }}>
        <InfoTip label="cache" text="explain" />
      </div>,
    );
    fireEvent.click(screen.getByTestId('infotip-trigger'));
    expect(outer).toBe(0); // stopPropagation so opening the tip never expands the card
  });
});
