import { describe, it, expect } from 'vitest';
import { phaseAt, type EpisodeLike } from '../episodePhase';

const ep = (
  phase: string,
  started_at: string,
  ended_at: string,
  episode_id?: string,
): EpisodeLike => ({ phase, started_at, ended_at, episode_id });

describe('phaseAt', () => {
  it('returns the phase of the single covering episode', () => {
    const eps = [ep('action', '2026-05-31T10:00:00Z', '2026-05-31T10:30:00Z', 'a')];
    expect(phaseAt(eps, '2026-05-31T10:15:00Z')).toBe('action');
  });

  it('on OVERLAP picks the narrowest, not the first-listed/widest', () => {
    // Wide `action` listed FIRST (the stale episode); narrow `exploration` second.
    // A first-match impl returns `action`; narrowest-match must return `exploration`.
    const eps = [
      ep('action', '2026-05-31T10:00:00Z', '2026-05-31T10:30:00Z', 'wide'),
      ep('exploration', '2026-05-31T10:10:00Z', '2026-05-31T10:11:00Z', 'narrow'),
    ];
    expect(phaseAt(eps, '2026-05-31T10:10:30Z')).toBe('exploration');
  });

  it('returns null when no episode covers the instant', () => {
    const eps = [ep('action', '2026-05-31T10:00:00Z', '2026-05-31T10:30:00Z', 'a')];
    expect(phaseAt(eps, '2026-05-31T11:00:00Z')).toBeNull();
    expect(phaseAt([], '2026-05-31T10:00:00Z')).toBeNull();
  });

  it('includes episodes at their exact boundaries (inclusive containment)', () => {
    const eps = [ep('verification', '2026-05-31T10:00:00Z', '2026-05-31T10:30:00Z', 'v')];
    expect(phaseAt(eps, '2026-05-31T10:00:00Z')).toBe('verification');
    expect(phaseAt(eps, '2026-05-31T10:30:00Z')).toBe('verification');
  });

  it('breaks a duration tie by LATEST started_at', () => {
    // Two equal-duration (60s) covering episodes; later-starting one wins.
    const eps = [
      ep('early', '2026-05-31T10:00:00Z', '2026-05-31T10:01:00Z', 'e'),
      ep('late', '2026-05-31T10:00:30Z', '2026-05-31T10:01:30Z', 'l'),
    ];
    expect(phaseAt(eps, '2026-05-31T10:00:45Z')).toBe('late');
  });

  it('breaks a duration+started_at tie deterministically by episode_id', () => {
    // Identical windows → fall through to lexicographic episode_id ('aaa' < 'bbb').
    const eps = [
      ep('beta', '2026-05-31T10:00:00Z', '2026-05-31T10:01:00Z', 'bbb'),
      ep('alpha', '2026-05-31T10:00:00Z', '2026-05-31T10:01:00Z', 'aaa'),
    ];
    expect(phaseAt(eps, '2026-05-31T10:00:30Z')).toBe('alpha');
  });
});
