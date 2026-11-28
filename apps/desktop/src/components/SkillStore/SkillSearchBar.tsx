// ============================================================================
// SkillSearchBar - Search input for skills.sh
// ============================================================================

import { useRef } from "react";
import { Search, X, Loader } from "lucide-react";
import { Button, Input } from "@skill-studio/ui";
import { useDelayedCall } from "../../hooks/useDelayedCall";

interface SkillSearchBarProps {
  value: string;
  onChange: (value: string) => void;
  onSearch: (query: string) => void;
  isLoading: boolean;
  placeholder?: string;
}

export function SkillSearchBar({
  value,
  onChange,
  onSearch,
  isLoading,
  placeholder = "Search skills.sh…",
}: SkillSearchBarProps) {
  const lastSearchedValue = useRef(value);
  const inputRef = useRef<HTMLInputElement>(null);
  const debounce = useDelayedCall();

  const change = (next: string) => {
    onChange(next);
    debounce.schedule(() => {
      if (lastSearchedValue.current === next) return;
      lastSearchedValue.current = next;
      onSearch(next);
    }, 300);
  };

  const handleClear = () => {
    change("");
    inputRef.current?.focus();
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      handleClear();
    }
  };

  return (
    <div className="relative flex max-w-[400px] flex-1 items-center">
      <div className="pointer-events-none absolute left-3 flex items-center text-text-tertiary">
        {isLoading ? <Loader size={16} className="animate-spin" /> : <Search size={16} />}
      </div>
      <Input
        ref={inputRef}
        type="text"
        value={value}
        onChange={(e) => change(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={placeholder}
        aria-label="Search skills.sh"
        className="h-(--control-height) pr-8.5 pl-8.5"
      />
      {value && (
        <Button
          variant="ghost"
          size="icon-xs"
          className="absolute right-2 rounded-full bg-bg-tertiary text-text-tertiary"
          onClick={handleClear}
          aria-label="Clear search"
        >
          <X size={14} />
        </Button>
      )}
    </div>
  );
}
