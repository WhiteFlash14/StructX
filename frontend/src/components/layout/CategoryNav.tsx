"use client";

import { CATEGORY_TABS, type CategoryTab } from "@/lib/strategyCatalog";

type Props = {
  active: CategoryTab;
  onChange: (tab: CategoryTab) => void;
  counts: Partial<Record<CategoryTab, number>>;
};

export function CategoryNav({ active, onChange, counts }: Props) {
  return (
    <nav className="category-nav" aria-label="Strategy categories">
      <div className="category-nav-inner">
        {CATEGORY_TABS.map((tab) => {
          const count = counts[tab.id];
          const isActive = active === tab.id;
          return (
            <button
              key={tab.id}
              type="button"
              className={`category-tab ${isActive ? "active" : ""}`}
              onClick={() => onChange(tab.id)}
              aria-pressed={isActive}
            >
              <span>{tab.label}</span>
              {typeof count === "number" && count > 0 && (
                <span className="category-count">{count}</span>
              )}
            </button>
          );
        })}
      </div>
    </nav>
  );
}
