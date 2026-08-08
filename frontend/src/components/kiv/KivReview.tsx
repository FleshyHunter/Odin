import { useState } from 'react';
import { sampleKiv } from './sampleData';
import type { KivData, KivSeverity } from './types';
import './kiv.css';

interface KivReviewProps {
  data?: KivData;
}

const SEVERITY_LABEL: Record<KivSeverity, string> = {
  foundation_gap: 'Foundation gap',
  kiv: 'KIV',
};

// PRD.md, KIV Review: "foundation_gap concepts (failed a BASIC question)
// are more serious than kiv concepts (failed an ADVANCED question and
// moved on)" — reflected in the meta line, not just the tag color.
const SEVERITY_META: Record<KivSeverity, string> = {
  foundation_gap: 'Skipped',
  kiv: 'Failed advanced, moved on',
};

// deferred.md #38 — no journeys:: module exists yet to fetch this from
// (real endpoints/seeded rows are waiting on that, per explicit
// direction), so this renders standalone sample data by default, same
// pattern as Roadmap before it had real wiring. "Mark done" is real,
// local interaction (removes the item from view) — nothing here is a
// static screenshot.
export function KivReview({ data = sampleKiv }: KivReviewProps) {
  const [items, setItems] = useState(data.items);

  const handleMarkDone = (id: string) => {
    setItems((prev) => prev.filter((item) => item.id !== id));
  };

  return (
    <div className="kiv-review">
      {items.length > 0 && (
        <div className="kiv-prompt">
          <p>
            You have <strong>{items.length} flagged concept{items.length === 1 ? '' : 's'}</strong>. Review:
          </p>
          <div className="kiv-prompt-actions">
            {/* No defined behavior yet — starting a real review session
                needs a real exercise-serving loop that doesn't exist
                until journeys:: does (same "placeholder affordance
                only" pattern as Composer.tsx's own attachment icon). */}
            <button className="kiv-btn kiv-btn-primary">Current concept only</button>
            <button className="kiv-btn">Mixed session, all flagged</button>
          </div>
        </div>
      )}

      <p className="kiv-section-label">Flagged concepts</p>

      {items.length === 0 ? (
        <p className="panel-footnote">Nothing flagged right now — nice work.</p>
      ) : (
        <div className="kiv-list">
          {items.map((item) => (
            <div key={item.id} className="kiv-row">
              <div className="kiv-row-main">
                <div className="kiv-row-top">
                  <span className="kiv-title">{item.conceptTitle}</span>
                  <span className={`kiv-tag kiv-tag-${item.severity}`}>{SEVERITY_LABEL[item.severity]}</span>
                </div>
                <span className="kiv-meta">
                  {SEVERITY_META[item.severity]} — mastery {item.masteryScore.toFixed(2)}
                </span>
                <div className="kiv-mastery-bar">
                  <div
                    className={`kiv-mastery-fill kiv-mastery-fill-${item.severity}`}
                    style={{ width: `${item.masteryScore * 100}%` }}
                  />
                </div>
              </div>
              <button className="kiv-mark-done" onClick={() => handleMarkDone(item.id)}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
                  <polyline points="20 6 9 17 4 12" />
                </svg>
                Mark done
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
