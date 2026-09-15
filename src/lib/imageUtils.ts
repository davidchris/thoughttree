/**
 * Client-side processing for pasted/dropped inline images.
 *
 * The authoritative attachment limits live in the core crate
 * (`thoughttree_core::vault::files::limits`: IMAGE_MAX_BYTES, IMAGE_MAX_SIDE)
 * and are enforced by the backend before any ACP call. The values below are
 * NOT limits: they are the targets we resize pasted images down to. They sit
 * deliberately well below the core limits so a resized image always passes
 * validation with margin (base64 inflation, provider-side re-encoding, and
 * the fact that inline images are persisted as base64 in the Project file).
 */

const RESIZE_TARGET_BYTES = 3.5 * 1024 * 1024; // resize target, below IMAGE_MAX_BYTES (5 MB)
const RESIZE_TARGET_SIDE = 4096; // resize target, below IMAGE_MAX_SIDE (8000 px)

/**
 * Resize an image if it exceeds size or dimension limits.
 * Uses canvas for resizing and quality reduction.
 */
export async function resizeIfNeeded(file: File): Promise<Blob> {
  // Load image to check dimensions
  const img = await loadImage(file);

  // Check if resizing is needed
  const needsDimensionResize = img.width > RESIZE_TARGET_SIDE || img.height > RESIZE_TARGET_SIDE;
  const needsSizeResize = file.size > RESIZE_TARGET_BYTES;

  if (!needsDimensionResize && !needsSizeResize) {
    return file;
  }

  // Calculate new dimensions maintaining aspect ratio
  let newWidth = img.width;
  let newHeight = img.height;

  if (needsDimensionResize) {
    const scale = Math.min(RESIZE_TARGET_SIDE / img.width, RESIZE_TARGET_SIDE / img.height);
    newWidth = Math.floor(img.width * scale);
    newHeight = Math.floor(img.height * scale);
  }

  // Resize using canvas
  const canvas = document.createElement('canvas');
  canvas.width = newWidth;
  canvas.height = newHeight;

  const ctx = canvas.getContext('2d');
  if (!ctx) {
    throw new Error('Failed to get canvas context');
  }

  ctx.drawImage(img, 0, 0, newWidth, newHeight);

  // Try different quality levels to get under size limit
  const mimeType = file.type === 'image/png' ? 'image/png' : 'image/jpeg';
  let quality = 0.92;
  let blob = await canvasToBlob(canvas, mimeType, quality);

  // If still too large, reduce quality iteratively
  while (blob.size > RESIZE_TARGET_BYTES && quality > 0.1) {
    quality -= 0.1;
    blob = await canvasToBlob(canvas, 'image/jpeg', quality);
  }

  return blob;
}

/**
 * Convert a Blob to base64 string (without data: prefix).
 */
export async function fileToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result as string;
      // Remove the "data:image/...;base64," prefix
      const base64 = result.split(',')[1];
      resolve(base64);
    };
    reader.onerror = reject;
    reader.readAsDataURL(blob);
  });
}

/**
 * Load an image file into an HTMLImageElement.
 */
function loadImage(file: File): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = reject;
    img.src = URL.createObjectURL(file);
  });
}

/**
 * Convert canvas to Blob with specified quality.
 */
function canvasToBlob(canvas: HTMLCanvasElement, mimeType: string, quality: number): Promise<Blob> {
  return new Promise((resolve, reject) => {
    canvas.toBlob(
      (blob) => {
        if (blob) {
          resolve(blob);
        } else {
          reject(new Error('Failed to create blob from canvas'));
        }
      },
      mimeType,
      quality
    );
  });
}
