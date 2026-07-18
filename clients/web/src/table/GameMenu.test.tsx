/**
 * The game menu: session-level actions at the shell's top-right. Concede lives
 * here behind a confirm step — never in the action tray beside Pass priority —
 * and remains a server-offered action (the menu invents nothing).
 */
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ValidAction } from '../protocol';
import { GameMenu } from './GameMenu';

const CONCEDE: ValidAction = { id: 'a9', type: 'concede', label: 'Concede' };

describe('GameMenu', () => {
  afterEach(cleanup);

  it('opens a drawer with the shortcut reference item', () => {
    const onShowShortcuts = vi.fn();
    render(<GameMenu onChoose={() => {}} onShowShortcuts={onShowShortcuts} />);
    expect(screen.queryByTestId('game-menu')).toBeNull();
    fireEvent.click(screen.getByTestId('game-menu-button'));
    fireEvent.click(screen.getByTestId('menu-shortcuts'));
    expect(onShowShortcuts).toHaveBeenCalledOnce();
    // Choosing an item closes the drawer.
    expect(screen.queryByTestId('game-menu')).toBeNull();
  });

  it('offers concede only when the server offers it', () => {
    render(<GameMenu onChoose={() => {}} onShowShortcuts={() => {}} />);
    fireEvent.click(screen.getByTestId('game-menu-button'));
    expect(screen.queryByTestId('menu-concede')).toBeNull();
  });

  it('requires a confirm step before submitting concede', () => {
    const onChoose = vi.fn();
    render(<GameMenu concede={CONCEDE} onChoose={onChoose} onShowShortcuts={() => {}} />);
    fireEvent.click(screen.getByTestId('game-menu-button'));
    fireEvent.click(screen.getByTestId('menu-concede'));
    // Not yet submitted — the confirm step is showing.
    expect(onChoose).not.toHaveBeenCalled();
    expect(screen.getByTestId('menu-concede-confirm')).toBeDefined();
    fireEvent.click(screen.getByTestId('menu-concede-yes'));
    expect(onChoose).toHaveBeenCalledWith(CONCEDE);
    expect(screen.queryByTestId('game-menu')).toBeNull();
  });

  it('backs out of the confirm step without submitting', () => {
    const onChoose = vi.fn();
    render(<GameMenu concede={CONCEDE} onChoose={onChoose} onShowShortcuts={() => {}} />);
    fireEvent.click(screen.getByTestId('game-menu-button'));
    fireEvent.click(screen.getByTestId('menu-concede'));
    fireEvent.click(screen.getByTestId('menu-concede-no'));
    expect(onChoose).not.toHaveBeenCalled();
    // The drawer stays open with the plain item restored.
    expect(screen.getByTestId('menu-concede')).toBeDefined();
  });

  it('closes on the click-away scrim and disarms any pending confirm', () => {
    render(<GameMenu concede={CONCEDE} onChoose={() => {}} onShowShortcuts={() => {}} />);
    fireEvent.click(screen.getByTestId('game-menu-button'));
    fireEvent.click(screen.getByTestId('menu-concede'));
    fireEvent.click(screen.getByTestId('game-menu-scrim'));
    expect(screen.queryByTestId('game-menu')).toBeNull();
    // Reopening starts from the plain item, not the armed confirm.
    fireEvent.click(screen.getByTestId('game-menu-button'));
    expect(screen.getByTestId('menu-concede')).toBeDefined();
    expect(screen.queryByTestId('menu-concede-confirm')).toBeNull();
  });

  it('closes on Escape', () => {
    render(<GameMenu concede={CONCEDE} onChoose={() => {}} onShowShortcuts={() => {}} />);
    fireEvent.click(screen.getByTestId('game-menu-button'));
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByTestId('game-menu')).toBeNull();
  });
});
