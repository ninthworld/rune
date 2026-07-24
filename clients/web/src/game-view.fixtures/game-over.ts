/**
 * Terminal (game-over) wire frames: a shared builder for the game-over shape plus the
 * win / loss / draw fixtures it produces. Raw wire JSON — `result` present is the
 * game-over signal the server elides while a game is live.
 */

/**
 * Build a terminal server→client `GameView` frame (issue #141): the game is over,
 * so `result` is present and `valid_actions` is empty (CR 104.2a). The `you` seat
 * lets the client phrase the verdict from the receiver's perspective. Mirrors the
 * wire shape the server elides while live — `result` present is the game-over signal.
 */
export function gameOverViewJson(
  you: string,
  result: { winner?: string; losers: string[]; reason: 'life_zero' | 'decked' | 'concede' },
): string {
  return JSON.stringify({
    you,
    opponents: [{ player_id: you === 'p1' ? 'p2' : 'p1', hand_size: 3, life: 0, library_size: 40 }],
    phase: 'end',
    valid_actions: [],
    result,
  });
}

/** Terminal view where the receiver (`p1`) won by their opponent decking out. */
export const GAME_OVER_WIN_JSON = gameOverViewJson('p1', {
  winner: 'p1',
  losers: ['p2'],
  reason: 'decked',
});

/** Terminal view where the receiver (`p1`) lost to lethal damage (opponent won). */
export const GAME_OVER_LOSS_JSON = gameOverViewJson('p1', {
  winner: 'p2',
  losers: ['p1'],
  reason: 'life_zero',
});

/** Terminal view of a draw — no winner, every remaining player lost at once. */
export const GAME_OVER_DRAW_JSON = gameOverViewJson('p1', {
  losers: ['p1', 'p2'],
  reason: 'life_zero',
});
