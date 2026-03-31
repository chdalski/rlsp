import { describe, expect, it } from 'vitest';
import { statusBarLabel, type ServerState } from './status-label.js';

const ALL_STATES: ServerState[] = ['starting', 'running', 'stopped', 'error'];

describe('statusBarLabel', () => {
  describe('text content per state', () => {
    it('starting state includes a spinner icon', () => {
      const result = statusBarLabel('starting');
      expect(result.text).toBe('$(sync~spin) rlsp-yaml');
    });

    it('running state includes a check icon', () => {
      const result = statusBarLabel('running');
      expect(result.text).toBe('$(check) rlsp-yaml');
    });

    it('stopped state includes an x icon', () => {
      const result = statusBarLabel('stopped');
      expect(result.text).toBe('$(x) rlsp-yaml');
    });

    it('error state includes a warning icon', () => {
      const result = statusBarLabel('error');
      expect(result.text).toBe('$(warning) rlsp-yaml');
    });
  });

  describe('label presence', () => {
    it('all states include rlsp-yaml in the text', () => {
      for (const state of ALL_STATES) {
        expect(statusBarLabel(state).text).toContain('rlsp-yaml');
      }
    });
  });

  describe('tooltip presence', () => {
    it('all states return a non-empty tooltip', () => {
      for (const state of ALL_STATES) {
        const { tooltip } = statusBarLabel(state);
        expect(typeof tooltip).toBe('string');
        expect(tooltip.length).toBeGreaterThan(0);
      }
    });
  });

  describe('exhaustiveness', () => {
    it('handles every defined state without throwing', () => {
      for (const state of ALL_STATES) {
        const result = statusBarLabel(state);
        expect(typeof result.text).toBe('string');
        expect(typeof result.tooltip).toBe('string');
      }
    });
  });
});
