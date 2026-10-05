import React from "react";
import { api, type Shot } from "../lib/api";

type Props = { shot: Shot; selected: boolean; active: boolean; onSelect: (shot: Shot) => void; onFocus: () => void };

export function ScreenshotCard({ shot, selected, active, onSelect, onFocus }: Props) {
  const [thumbnail, setThumbnail] = React.useState<string | null>(null);
  const [failed, setFailed] = React.useState(false);
  const container = React.useRef<HTMLButtonElement>(null);

  React.useEffect(() => {
    const element = container.current;
    if (!element) return;
    let cancelled = false;
    const load = () => {
      void api.getShotThumbnail(shot.id).then((image) => {
        if (!cancelled) setThumbnail(image.dataUrl);
      }).catch(() => {
        if (!cancelled) setFailed(true);
      });
    };
    if (typeof IntersectionObserver === "undefined") {
      load();
      return () => { cancelled = true; };
    }
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) {
        observer.disconnect();
        load();
      }
    }, { rootMargin: "120px" });
    observer.observe(element);
    return () => { cancelled = true; observer.disconnect(); };
  }, [shot.id]);

  return <button ref={container} className={`shot-card${selected ? " selected" : ""}`} tabIndex={active ? 0 : -1} aria-pressed={selected} onFocus={onFocus} onClick={() => onSelect(shot)}>
    <span className="card-image">{thumbnail ? <img src={thumbnail} alt="" /> : <span className="thumbnail-placeholder" aria-hidden="true">{failed ? "Image unavailable" : `${shot.width} × ${shot.height}`}</span>}</span>
    <span className="card-content">
      <span className="card-date">{new Date(shot.mtime * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })}</span>
      <span className="card-snippet">{shot.ocrSnippet || "No OCR text available"}</span>
      <span className="card-dimensions">{shot.width} × {shot.height}</span>
    </span>
  </button>;
}
