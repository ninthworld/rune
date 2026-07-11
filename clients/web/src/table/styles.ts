/**
 * Inline style objects for the React DOM layer of the table (ADR 0003: prompts,
 * action bar, player tiles, and the interactive overlay are DOM, not canvas).
 *
 * Card colors and sizes always come from `src/tokens.ts`; the values here are UI
 * chrome (bars, tiles, spacing) — never card characteristics. Where the DOM must
 * agree with a card token (the selection ring, the board backdrop) it reads the
 * shared token so both renderers stay in lockstep.
 */
import type { CSSProperties } from 'react';
import { SURFACES } from '../tokens';
import type { Rect } from './scene';

/** Minimum touch target per AGENTS.md (44px), applied to every affordance. */
const TOUCH = 44;

export const main: CSSProperties = {
  minHeight: '100vh',
  background: SURFACES.board,
  color: SURFACES.nameText,
  fontFamily: 'system-ui, sans-serif',
  display: 'flex',
  flexDirection: 'column',
  gap: 8,
  padding: 8,
  boxSizing: 'border-box',
};

export const banner: CSSProperties = {
  display: 'flex',
  flexWrap: 'wrap',
  gap: 16,
  alignItems: 'center',
  padding: '10px 14px',
  borderRadius: 8,
  background: '#1E2126',
  fontSize: 14,
};

export const bannerAccent: CSSProperties = { color: SURFACES.selection, fontWeight: 600 };

export const tiles: CSSProperties = {
  display: 'flex',
  flexWrap: 'wrap',
  gap: 8,
};

export const tile: CSSProperties = {
  minWidth: 140,
  padding: '8px 12px',
  borderRadius: 8,
  background: '#1E2126',
  border: '1px solid #2C313A',
  fontSize: 13,
  lineHeight: 1.5,
};

export const localTile: CSSProperties = {
  borderColor: SURFACES.selection,
};

export const tileName: CSSProperties = {
  fontWeight: 600,
  fontSize: 14,
  marginBottom: 2,
};

export function boardWrap(width: number, height: number): CSSProperties {
  return {
    position: 'relative',
    width,
    height,
    maxWidth: '100%',
    overflowX: 'auto',
    alignSelf: 'center',
  };
}

export function overlay(width: number, height: number): CSSProperties {
  return {
    position: 'absolute',
    inset: 0,
    width,
    height,
    // The overlay itself passes pointer events through; only its buttons catch
    // them, so empty board space never intercepts clicks.
    pointerEvents: 'none',
  };
}

export function hotspot(rect: Rect, selected: boolean): CSSProperties {
  return {
    position: 'absolute',
    left: rect.x,
    top: rect.y,
    width: Math.max(rect.w, TOUCH),
    height: Math.max(rect.h, TOUCH),
    minWidth: TOUCH,
    minHeight: TOUCH,
    padding: 0,
    background: 'transparent',
    border: selected ? `2px solid ${SURFACES.selection}` : '2px solid transparent',
    borderRadius: 10,
    cursor: 'pointer',
    pointerEvents: 'auto',
  };
}

export function entityActions(rect: Rect): CSSProperties {
  return {
    position: 'absolute',
    left: rect.x,
    top: rect.y + rect.h + 4,
    display: 'flex',
    flexWrap: 'wrap',
    gap: 4,
    pointerEvents: 'auto',
    zIndex: 2,
  };
}

export const bar: CSSProperties = {
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  gap: 8,
  padding: '8px 12px',
  borderRadius: 8,
  background: '#1E2126',
  minHeight: TOUCH + 12,
};

export const button: CSSProperties = {
  minHeight: TOUCH,
  minWidth: TOUCH,
  padding: '0 16px',
  borderRadius: 8,
  border: '1px solid #3A4049',
  background: '#2A2F37',
  color: SURFACES.nameText,
  fontSize: 14,
  fontWeight: 600,
  cursor: 'pointer',
};

export const chip: CSSProperties = {
  minHeight: TOUCH,
  padding: '0 12px',
  borderRadius: 8,
  border: `1px solid ${SURFACES.selection}`,
  background: '#2A2F37',
  color: SURFACES.nameText,
  fontSize: 13,
  fontWeight: 600,
  cursor: 'pointer',
  boxShadow: '0 2px 6px rgba(0,0,0,0.4)',
};

export const echo: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  flexWrap: 'wrap',
  gap: 8,
  marginLeft: 'auto',
  paddingLeft: 12,
  borderLeft: '1px solid #3A4049',
};

export const echoLabel: CSSProperties = {
  fontSize: 13,
  color: SURFACES.typeText,
};

export const muted: CSSProperties = {
  fontSize: 13,
  color: SURFACES.typeText,
};
