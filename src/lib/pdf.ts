import * as pdfjs from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import "pdfjs-dist/web/pdf_viewer.css";

pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

/** Loads a PDF from bytes; runtime assets are served from /pdfjs/. */
export function loadPdf(data: ArrayBuffer) {
  return pdfjs.getDocument({
    data: new Uint8Array(data),
    wasmUrl: "/pdfjs/wasm/",
    cMapUrl: "/pdfjs/cmaps/",
    cMapPacked: true,
    standardFontDataUrl: "/pdfjs/standard_fonts/",
    iccUrl: "/pdfjs/iccs/",
  }).promise;
}

export { pdfjs };
export type { PDFDocumentProxy, PDFPageProxy, RenderTask } from "pdfjs-dist";
