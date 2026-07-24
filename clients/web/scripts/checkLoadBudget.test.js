import { describe, expect, it } from 'vitest';
import { resolveDistDir } from './checkLoadBudget.js';

const CWD = '/home/user/rune/clients/web';
const PACKAGE_ROOT = '/opt/rune/clients/web/';

describe('resolveDistDir', () => {
  it('measures the package dist when no directory is given — the path CI takes', () => {
    expect(resolveDistDir(undefined, CWD, PACKAGE_ROOT)).toBe('/opt/rune/clients/web/dist');
  });

  it('resolves a relative directory against the working directory', () => {
    expect(resolveDistDir('dist', CWD, PACKAGE_ROOT)).toBe(`${CWD}/dist`);
    expect(resolveDistDir('../../build/dist', CWD, PACKAGE_ROOT)).toBe(
      '/home/user/rune/build/dist',
    );
  });

  it('leaves an absolute directory alone rather than hanging it off the cwd', () => {
    expect(resolveDistDir('/tmp/some/dist', CWD, PACKAGE_ROOT)).toBe('/tmp/some/dist');
  });
});
