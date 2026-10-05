import type { ShotDetail, ShotImage } from "../lib/api";
import { EmptyState, ErrorState, LoadingState } from "./States";

type Props = { detail: ShotDetail | null; image: ShotImage | null; loading: boolean; error: string | null; onClose: () => void; onRetry: () => void };

export function ScreenshotDetail({ detail, image, loading, error, onClose, onRetry }: Props) {
  return <aside className="detail-panel" aria-label="Screenshot details">
    <div className="detail-heading"><div><span className="eyebrow">Selected evidence</span><h2>Screenshot</h2></div><button className="icon-button" aria-label="Close details" onClick={onClose}>×</button></div>
    {loading ? <LoadingState message="Loading screenshot…" /> : error ? <ErrorState message={error} action={onRetry} /> : !detail ? <EmptyState message="Select a screenshot to view its details." /> : <>
      <section className="detail-section"><h3>Image</h3><div className="detail-image">{image ? <img src={image.dataUrl} alt={`Screenshot captured ${new Date(detail.shot.mtime * 1000).toLocaleString()}`} /> : <LoadingState message="Loading image…" />}</div></section>
      <section className="detail-section"><h3>OCR text</h3>{detail.ocrLines.length ? <ol className="ocr-lines">{detail.ocrLines.map((line, index) => <li key={`${index}-${line}`}>{line}</li>)}</ol> : <p className="muted">No OCR text was found in this screenshot.</p>}</section>
      <section className="detail-section metadata"><h3>Metadata</h3><dl><div><dt>Date</dt><dd>{new Date(detail.shot.mtime * 1000).toLocaleString()}</dd></div><div><dt>Dimensions</dt><dd>{detail.shot.width} × {detail.shot.height}</dd></div><div><dt>Index ID</dt><dd>{detail.shot.id}</dd></div></dl></section>
    </>}
  </aside>;
}
