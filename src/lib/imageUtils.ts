/**
 * Image processing utilities for Claude API compatibility.
 *
 * Claude API constraints:
 * - Max 3.75 MB per image
 * - Max 8000x8000 px dimensions
 *
 * We use conservative limits to ensure images always pass validation.
 */

const MAX_SIZE_BYTES = 3.5 * 1024 * 1024; // 3.5MB (under 3.75MB limit)
const MAX_DIMENSION = 4096; // Conservative limit under 8000px

/**
 * Resize an image if it exceeds size or dimension limits.
 * Uses canvas for resizing and quality reduction.
 */
export async function resizeIfNeeded(file: File): Promise<Blob> {
  // Load image to check dimensions
  const img = await loadImage(file);

  // Check if resizing is needed
  const needsDimensionResize = img.width > MAX_DIMENSION || img.height > MAX_DIMENSION;
  const needsSizeResize = file.size > MAX_SIZE_BYTES;

  if (!needsDimensionResize && !needsSizeResize) {
    return file;
  }

  // Calculate new dimensions maintaining aspect ratio
  let newWidth = img.width;
  let newHeight = img.height;

  if (needsDimensionResize) {
    const scale = Math.min(MAX_DIMENSION / img.width, MAX_DIMENSION / img.height);
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
  while (blob.size > MAX_SIZE_BYTES && quality > 0.1) {
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
