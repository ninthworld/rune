/**
 * Test setup: Pixi's `Text` constructor calls `canvas.getContext('2d')`, which
 * jsdom leaves unimplemented (returning null and logging a warning). The card
 * factory never renders or measures text (see FONT.charWidthRatio in tokens), so
 * a minimal no-op 2D context stub is enough to keep construction quiet. This is
 * deliberately NOT a real canvas — there is no GPU/GL in CI.
 */
const noop = (): void => {};

const stub2d = {
  measureText: (text: string) => ({ width: text.length * 6 }),
  fillText: noop,
  clearRect: noop,
  fillRect: noop,
  save: noop,
  restore: noop,
  scale: noop,
  translate: noop,
  font: '',
  textBaseline: '',
  fillStyle: '',
};

HTMLCanvasElement.prototype.getContext = (() =>
  stub2d) as unknown as HTMLCanvasElement['getContext'];
