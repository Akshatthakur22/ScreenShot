import React from "react";

import { api, errorMessage, type Shot, type ShotDetail, type ShotImage, type Stats } from "../lib/api";
import { SearchBar } from "../components/SearchBar";
import { ScreenshotCard } from "../components/ScreenshotCard";
import { ScreenshotDetail } from "../components/ScreenshotDetail";
import { EmptyState, ErrorState, LoadingState } from "../components/States";

type View = "all" | "recent";

export function Library() {
  const [view, setView] = React.useState<View>("all");
  const [query, setQuery] = React.useState("");
  const [appliedQuery, setAppliedQuery] = React.useState("");
  const [shots, setShots] = React.useState<Shot[]>([]);
  const [stats, setStats] = React.useState<Stats | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);
  const [selected, setSelected] = React.useState<number | null>(null);
  const [detail, setDetail] = React.useState<ShotDetail | null>(null);
  const [image, setImage] = React.useState<ShotImage | null>(null);
  const [detailLoading, setDetailLoading] = React.useState(false);
  const [detailError, setDetailError] = React.useState<string | null>(null);
  const [activeIndex, setActiveIndex] = React.useState(0);
  const input = React.useRef<HTMLInputElement>(null);
  const list = React.useRef<HTMLElement>(null);

  const loadLibrary = React.useCallback(async (currentView: View = view) => {
    setLoading(true);
    setError(null);
    setAppliedQuery("");
    try {
      const [nextStats, nextShots] = await Promise.all([
        api.getStats(),
        api.getRecentShots(currentView === "recent" ? 12 : 100),
      ]);
      setStats(nextStats);
      setShots(nextShots);
      setSelected(null);
      setDetail(null);
      setImage(null);
      setActiveIndex(0);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setLoading(false);
    }
  }, [view]);

  React.useEffect(() => { void loadLibrary("all"); }, []);

  async function submitSearch(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const text = query.trim();
    if (!text) {
      setView("all");
      await loadLibrary("all");
      return;
    }
    setLoading(true);
    setError(null);
    setSelected(null);
    setDetail(null);
    setImage(null);
    setAppliedQuery(text);
    try {
      setShots(await api.searchShots(text));
      setView("all");
      setActiveIndex(0);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setLoading(false);
    }
  }

  async function retryCurrent() {
    if (!appliedQuery) {
      await loadLibrary();
      return;
    }
    setLoading(true);
    setError(null);
    try {
      setShots(await api.searchShots(appliedQuery));
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setLoading(false);
    }
  }

  async function openShot(shot: Shot) {
    setSelected(shot.id);
    setDetail(null);
    setImage(null);
    setDetailError(null);
    setDetailLoading(true);
    try {
      const [nextDetail, nextImage] = await Promise.all([
        api.getShotDetail(shot.id),
        api.getShotImage(shot.id),
      ]);
      setDetail(nextDetail);
      setImage(nextImage);
    } catch (cause) {
      setDetailError(errorMessage(cause));
    } finally {
      setDetailLoading(false);
    }
  }

  React.useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        input.current?.focus();
        return;
      }
      const target = event.target as HTMLElement | null;
      if (event.key === "Escape" && selected !== null) {
        setSelected(null);
        setDetail(null);
        setImage(null);
        return;
      }
      if (event.key === "Escape" && target?.tagName === "INPUT") {
        if (query || appliedQuery) clearSearch();
        return;
      }
      if (target?.tagName === "INPUT" || target?.tagName === "TEXTAREA") return;
      if (!shots.length) return;
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        const next = Math.max(0, Math.min(shots.length - 1, activeIndex + (event.key === "ArrowDown" ? 1 : -1)));
        setActiveIndex(next);
        list.current?.querySelectorAll<HTMLButtonElement>(".shot-card")[next]?.focus();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [activeIndex, selected, shots]);

  function clearSearch() {
    setQuery("");
    setView("all");
    void loadLibrary("all");
  }

  const title = appliedQuery ? "Search results" : view === "recent" ? "Recent screenshots" : "All screenshots";

  return <main className="app-shell">
    <aside className="sidebar">
      <div className="brand"><span className="brand-mark" aria-hidden="true">A</span><span>Akshat</span></div>
      <p className="nav-label">Library</p>
      <nav aria-label="Screenshot library">
        <button className={`nav-item${view === "all" && !appliedQuery ? " active" : ""}`} aria-current={view === "all" && !appliedQuery ? "page" : undefined} onClick={() => { setView("all"); setQuery(""); void loadLibrary("all"); }}><span aria-hidden="true">▦</span>All</button>
        <button className={`nav-item${view === "recent" && !appliedQuery ? " active" : ""}`} aria-current={view === "recent" && !appliedQuery ? "page" : undefined} onClick={() => { setView("recent"); setQuery(""); void loadLibrary("recent"); }}><span aria-hidden="true">◷</span>Recent</button>
      </nav>
      <div className="sidebar-foot">Stored on this Mac</div>
    </aside>

    <section className="main-column">
      <header className="workspace-header">
        <div className="search-wrap"><SearchBar value={query} onChange={setQuery} onSubmit={submitSearch} inputRef={input} /></div>
        <div className="workspace-heading"><div><p className="eyebrow">Library</p><h1>{title}</h1></div><span className="count-label">{stats ? `${stats.screenshotCount} indexed` : "Local index"}</span></div>
      </header>
      <section className="results-area" aria-label={title} ref={list}>
        {error ? <ErrorState message={error} action={() => void retryCurrent()} /> : loading ? <LoadingState /> : shots.length === 0 ? <EmptyState message={appliedQuery ? "No screenshots found. Try another search." : "No screenshots indexed yet. Once screenshots are indexed, they’ll appear here."} action={appliedQuery ? clearSearch : undefined} actionLabel={appliedQuery ? "Clear search" : undefined} /> : <>
          {appliedQuery && <p className="evidence-note">Results matched OCR text for <strong>“{appliedQuery}”</strong>.</p>}
          <div className="results-list">
            {shots.map((shot, index) => <ScreenshotCard key={shot.id} shot={shot} selected={selected === shot.id} active={index === activeIndex} onSelect={openShot} onFocus={() => setActiveIndex(index)} />)}
          </div>
        </>}
      </section>
    </section>

    <ScreenshotDetail detail={detail} image={image} loading={detailLoading} error={detailError} onClose={() => { setSelected(null); setDetail(null); setImage(null); }} onRetry={() => { const shot = shots.find((item) => item.id === selected); if (shot) void openShot(shot); }} />
  </main>;
}
