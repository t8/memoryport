import { useState } from "react";
import { Search } from "lucide-react";
import { retrieve, type SearchResult } from "../lib/api";

interface SearchBarProps {
  onResults?: (results: SearchResult[], query: string) => void;
}

export default function SearchBar({ onResults }: SearchBarProps) {
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(false);

  const handleSearch = async () => {
    if (!query.trim()) return;
    setLoading(true);
    try {
      const { results } = await retrieve(query, 50);
      onResults?.(results, query.trim());
    } catch (err) {
      console.error("Search failed:", err);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="relative">
      <Search
        size={16}
        className="absolute left-3 top-1/2 -translate-y-1/2 text-cream-dim"
      />
      <input
        type="text"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && handleSearch()}
        placeholder="Search your memory..."
        className="w-full pl-9 pr-4 py-2 bg-bg border border-border text-sm text-cream placeholder:text-cream-dim focus:outline-none focus:border-border-hover transition-colors"
      />
      {loading && (
        <div className="absolute right-3 top-1/2 -translate-y-1/2">
          <div className="w-4 h-4 border-2 border-cream-dim border-t-cream rounded-full animate-spin" />
        </div>
      )}
    </div>
  );
}
