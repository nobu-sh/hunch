import { describe, expect, it } from 'vitest';
import { connectTarget } from './launch.ts';

describe('connectTarget', () => {
  it('turns a bare port into a local DevTools URL', () => {
    expect(connectTarget('9222')).toEqual({ browserURL: 'http://127.0.0.1:9222' });
  });

  it('passes http(s) URLs and ws(s) endpoints through', () => {
    expect(connectTarget('http://host:9222')).toEqual({ browserURL: 'http://host:9222' });
    expect(connectTarget('ws://127.0.0.1:1/devtools/browser/x')).toEqual({
      browserWSEndpoint: 'ws://127.0.0.1:1/devtools/browser/x',
    });
  });

  it('rejects anything else', () => {
    expect(() => connectTarget('localhost')).toThrow('Can\'t connect to "localhost"');
  });
});
