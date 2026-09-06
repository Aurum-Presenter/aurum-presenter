/**
 * Rendering a sheet to images, for the places that cannot host a live canvas — the print pack
 * above all.
 *
 * It runs off the cached file, so it works with the network off, and it is the reason the print
 * dialog checks what is downloaded before it starts rather than after.
 */
export async function renderPdfToImages(file: Blob, maxWidth = 1400): Promise<string[]> {
  if (file.type !== 'application/pdf') {
    return [URL.createObjectURL(file)];
  }

  const pdfjs = await import('pdfjs-dist');

  pdfjs.GlobalWorkerOptions.workerSrc = new URL('pdfjs-dist/build/pdf.worker.min.mjs', import.meta.url).toString();

  const document = await pdfjs.getDocument({ data: await file.arrayBuffer() }).promise;
  const images: string[] = [];

  for (let number = 1; number <= document.numPages; number++) {
    const page = await document.getPage(number);
    const unscaled = page.getViewport({ scale: 1 });
    const viewport = page.getViewport({ scale: Math.min(2, maxWidth / unscaled.width) });

    const canvas = window.document.createElement('canvas');
    canvas.width = viewport.width;
    canvas.height = viewport.height;

    const context = canvas.getContext('2d');

    if (context === null) {
      break;
    }

    await page.render({ canvasContext: context, viewport }).promise;
    images.push(canvas.toDataURL('image/jpeg', 0.85));
  }

  await document.destroy();

  return images;
}
