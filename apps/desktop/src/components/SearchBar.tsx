import type React from "react";

type Props = { value: string; onChange: (value: string) => void; onSubmit: (event: React.FormEvent<HTMLFormElement>) => void; inputRef: React.RefObject<HTMLInputElement | null> };

export function SearchBar({ value, onChange, onSubmit, inputRef }: Props) {
  return <form className="search-bar" role="search" onSubmit={onSubmit}>
    <span className="search-icon" aria-hidden="true">⌕</span>
    <input ref={inputRef} aria-label="Search screenshot text" placeholder="Search screenshots…" value={value} onChange={(event) => onChange(event.target.value)} />
    {value && <button className="clear-search" type="button" aria-label="Clear search" onClick={() => onChange("")}>×</button>}
    <kbd>⌘ K</kbd>
  </form>;
}
