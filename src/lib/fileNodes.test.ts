import { describe, expect, it } from 'vitest';
import { isRasterImage } from './fileNodes';

describe('isRasterImage', () => {
  // Must match thoughttree_core::vault::files::is_raster_image exactly: only
  // these four are read as bytes and inlined; everything else is a pointer.
  it.each(['image/png', 'image/jpeg', 'image/gif', 'image/webp'])('inlines %s', (mime) => {
    expect(isRasterImage(mime)).toBe(true);
  });

  it.each(['image/svg+xml', 'image/bmp', 'image/tiff', 'image/heic', 'text/markdown', 'application/octet-stream'])(
    'treats %s as a pointer file',
    (mime) => {
      expect(isRasterImage(mime)).toBe(false);
    }
  );
});
