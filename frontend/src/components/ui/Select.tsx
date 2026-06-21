"use client";

// Theme-matched dropdown that replaces the native <select> everywhere in the
// app. The native control can't be styled consistently across browsers (the
// OS paints its own menu), so this renders a button trigger plus a custom
// listbox popover. It keeps full keyboard support: arrows to move, Enter or
// Space to choose, Escape to close, Home/End to jump, and type-ahead.

import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";

export type SelectOption = {
  value: string;
  label: string;
  hint?: string;
};

type SelectProps = {
  value: string;
  onChange: (value: string) => void;
  options: SelectOption[];
  id?: string;
  ariaLabel?: string;
  className?: string;
  size?: "sm" | "md";
  align?: "start" | "end";
  fullWidth?: boolean;
  disabled?: boolean;
};

export function Select({
  value,
  onChange,
  options,
  id,
  ariaLabel,
  className,
  size = "md",
  align = "start",
  fullWidth = false,
  disabled = false,
}: SelectProps) {
  const autoId = useId();
  const rootId = id ?? `sx-select-${autoId}`;
  const listId = `${rootId}-list`;

  const [open, setOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);

  const rootRef = useRef<HTMLDivElement | null>(null);
  const buttonRef = useRef<HTMLButtonElement | null>(null);
  const listRef = useRef<HTMLUListElement | null>(null);
  const typeahead = useRef<{ buffer: string; at: number }>({ buffer: "", at: 0 });

  const selectedIndex = useMemo(() => {
    const i = options.findIndex((o) => o.value === value);
    return i < 0 ? 0 : i;
  }, [options, value]);

  const selected = options[selectedIndex];

  const close = useCallback((focusTrigger = true) => {
    setOpen(false);
    if (focusTrigger) buttonRef.current?.focus();
  }, []);

  const openMenu = useCallback(() => {
    if (disabled) return;
    setActiveIndex(selectedIndex);
    setOpen(true);
  }, [disabled, selectedIndex]);

  const choose = useCallback(
    (index: number) => {
      const opt = options[index];
      if (!opt) return;
      onChange(opt.value);
      close();
    },
    [options, onChange, close],
  );

  // Close on outside pointer and on scroll/resize so the popover never floats
  // away from its trigger.
  useEffect(() => {
    if (!open) return;
    const onPointer = (e: PointerEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    const onScroll = () => setOpen(false);
    document.addEventListener("pointerdown", onPointer, true);
    window.addEventListener("resize", onScroll);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      document.removeEventListener("pointerdown", onPointer, true);
      window.removeEventListener("resize", onScroll);
      window.removeEventListener("scroll", onScroll, true);
    };
  }, [open]);

  // Keep the active option scrolled into view while navigating.
  useEffect(() => {
    if (!open) return;
    const el = listRef.current?.querySelector<HTMLElement>(
      `[data-index="${activeIndex}"]`,
    );
    el?.scrollIntoView({ block: "nearest" });
  }, [open, activeIndex]);

  const moveActive = useCallback(
    (delta: number) => {
      setActiveIndex((i) => {
        const next = i + delta;
        if (next < 0) return 0;
        if (next > options.length - 1) return options.length - 1;
        return next;
      });
    },
    [options.length],
  );

  const onTriggerKeyDown = (e: React.KeyboardEvent) => {
    if (disabled) return;
    if (!open) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp" || e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        openMenu();
      }
      return;
    }
    onMenuKeyDown(e);
  };

  const onMenuKeyDown = (e: React.KeyboardEvent) => {
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        moveActive(1);
        break;
      case "ArrowUp":
        e.preventDefault();
        moveActive(-1);
        break;
      case "Home":
        e.preventDefault();
        setActiveIndex(0);
        break;
      case "End":
        e.preventDefault();
        setActiveIndex(options.length - 1);
        break;
      case "Enter":
      case " ":
        e.preventDefault();
        choose(activeIndex);
        break;
      case "Escape":
        e.preventDefault();
        close();
        break;
      case "Tab":
        setOpen(false);
        break;
      default: {
        // Type-ahead: jump to the next option whose label starts with the
        // typed run of characters.
        if (e.key.length === 1 && /\S/.test(e.key)) {
          const now = Date.now();
          const t = typeahead.current;
          t.buffer = now - t.at > 600 ? e.key : t.buffer + e.key;
          t.at = now;
          const q = t.buffer.toLowerCase();
          const start = t.buffer.length === 1 ? activeIndex + 1 : activeIndex;
          for (let k = 0; k < options.length; k++) {
            const idx = (start + k) % options.length;
            if (options[idx].label.toLowerCase().startsWith(q)) {
              setActiveIndex(idx);
              break;
            }
          }
        }
      }
    }
  };

  return (
    <div
      ref={rootRef}
      className={[
        "sx-select",
        size === "sm" ? "sx-select-sm" : "",
        fullWidth ? "sx-select-full" : "",
        open ? "is-open" : "",
        className ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
    >
      <button
        ref={buttonRef}
        type="button"
        id={rootId}
        className="sx-select-trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={ariaLabel}
        disabled={disabled}
        onClick={() => (open ? setOpen(false) : openMenu())}
        onKeyDown={onTriggerKeyDown}
      >
        <span className="sx-select-value">{selected?.label ?? ""}</span>
        <svg
          className="sx-select-caret"
          width="13"
          height="13"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2.2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden
        >
          <path d="M6 9l6 6 6-6" />
        </svg>
      </button>

      {open && (
        <ul
          ref={listRef}
          id={listId}
          role="listbox"
          tabIndex={-1}
          aria-activedescendant={`${rootId}-opt-${activeIndex}`}
          className={`sx-select-menu sx-select-menu-${align}`}
          onKeyDown={onMenuKeyDown}
        >
          {options.map((opt, index) => {
            const isSelected = opt.value === value;
            const isActive = index === activeIndex;
            return (
              <li
                key={opt.value}
                id={`${rootId}-opt-${index}`}
                role="option"
                data-index={index}
                aria-selected={isSelected}
                className={[
                  "sx-select-option",
                  isActive ? "is-active" : "",
                  isSelected ? "is-selected" : "",
                ]
                  .filter(Boolean)
                  .join(" ")}
                onMouseEnter={() => setActiveIndex(index)}
                onClick={() => choose(index)}
              >
                <span className="sx-select-option-main">
                  <span className="sx-select-option-label">{opt.label}</span>
                  {opt.hint && (
                    <span className="sx-select-option-hint">{opt.hint}</span>
                  )}
                </span>
                <svg
                  className="sx-select-check"
                  width="14"
                  height="14"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2.4"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  aria-hidden
                >
                  <path d="M5 12l5 5 9-10" />
                </svg>
              </li>
            );
          })}
        </ul>
      )}

      {/* Scoped styles. React 19 hoists + dedupes a <style> by href, so this
          ships once no matter how many Selects render. */}
      <style href="sx-ui-select" precedence="default">{SELECT_CSS}</style>
    </div>
  );
}

const SELECT_CSS = `
.sx-select {
  position: relative;
  display: inline-flex;
  font-family: var(--font-sans), "Inter", ui-sans-serif, system-ui, sans-serif;
}
.sx-select-full { display: flex; width: 100%; }
.sx-select-full .sx-select-trigger { width: 100%; }

.sx-select-trigger {
  display: inline-flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  height: 38px;
  padding: 0 12px 0 15px;
  min-width: 132px;
  border: 1px solid var(--sx-border);
  border-radius: 12px;
  background: var(--sx-surface);
  color: var(--sx-navy);
  font-size: 13.5px;
  font-weight: 500;
  letter-spacing: -0.005em;
  cursor: pointer;
  outline: none;
  transition: border-color 0.15s ease, background 0.15s ease, box-shadow 0.15s ease;
}
.sx-select-sm .sx-select-trigger {
  height: 34px;
  min-width: 120px;
  font-size: 13px;
  border-radius: 11px;
}
.sx-select-trigger:hover:not(:disabled) {
  border-color: var(--sx-border-strong);
}
.sx-select.is-open .sx-select-trigger,
.sx-select-trigger:focus-visible {
  border-color: var(--sx-navy);
  box-shadow: 0 0 0 4px rgba(16, 40, 74, 0.07);
}
.sx-select-trigger:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
.sx-select-value {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.sx-select-caret {
  flex: 0 0 auto;
  color: var(--sx-navy-muted);
  transition: transform 0.18s cubic-bezier(0.4, 0, 0.2, 1), color 0.15s ease;
}
.sx-select.is-open .sx-select-caret {
  transform: rotate(180deg);
  color: var(--sx-navy);
}

.sx-select-menu {
  position: absolute;
  top: calc(100% + 8px);
  z-index: 60;
  min-width: 100%;
  max-height: 280px;
  overflow-y: auto;
  margin: 0;
  padding: 6px;
  list-style: none;
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 14px;
  box-shadow:
    0 2px 4px rgba(16, 40, 74, 0.04),
    0 18px 44px rgba(16, 40, 74, 0.16);
  transition: opacity 0.16s ease, transform 0.16s cubic-bezier(0.4, 0, 0.2, 1);
}
.sx-select-menu-start { left: 0; }
.sx-select-menu-end { right: 0; }
@starting-style {
  .sx-select-menu {
    opacity: 0;
    transform: translateY(-6px) scale(0.985);
  }
}

.sx-select-option {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 8px 10px;
  border-radius: 9px;
  color: var(--sx-navy-muted);
  cursor: pointer;
  user-select: none;
  white-space: nowrap;
  transition: background 0.12s ease, color 0.12s ease;
}
.sx-select-option.is-active {
  background: var(--sx-surface-soft);
  color: var(--sx-navy);
}
.sx-select-option.is-selected {
  color: var(--sx-navy);
  font-weight: 600;
}
.sx-select-option-main {
  display: inline-flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.sx-select-option-label {
  font-size: 13.5px;
  letter-spacing: -0.005em;
}
.sx-select-option-hint {
  font-size: 11.5px;
  color: var(--sx-muted);
  font-weight: 500;
}
.sx-select-check {
  flex: 0 0 auto;
  color: var(--sx-teal-dark);
  opacity: 0;
  transform: scale(0.7);
  transition: opacity 0.12s ease, transform 0.12s ease;
}
.sx-select-option.is-selected .sx-select-check {
  opacity: 1;
  transform: scale(1);
}
`;