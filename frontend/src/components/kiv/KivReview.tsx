import type { ConceptStatus, RoadmapNode } from '../roadmap/types';
import './kiv.css';

interface KivReviewProps {
  items: RoadmapNode[];
  onReview: (id: string, title: string, status: ConceptStatus, unmetPrerequisiteTitles: string[]) => void;
}

const REVIEW_ICON = (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
    <path d="M5 12h14" />
    <path d="M13 6l6 6-6 6" />
  </svg>
);

// deferred.md #38 — real data now, straight off the same GET .../roadmap
// fetch ActivePanel already makes for the Map tab (see RoadmapData.
// kivItems's own doc comment for why there's no separate list endpoint).
// "Review" reuses NodeDetail's exact tier/attempt flow (same as clicking
// a node on the Map) — kiv_flagged_at clears itself implicitly once a
// concept actually completes (get_roadmap's own kiv_flagged is read live
// each fetch; the completion rule flips status to 'complete'/'mastered',
// which drops it out of buildRoadmapData's kivItems filter). No manual
// "mark done": PRD.md's locked rule is explicit — completion is always
// data-driven, never a dismiss click (deferred.md #68, the old mock's
// real spec violation).
//
// MVP scope, per explicit direction (2026-08-27): PRD.md's "Mixed
// session" (weighted, cross-concept serving) is deferred — items are
// reviewed one at a time via the tier flow below, current-concept style.
export function KivReview({ items, onReview }: KivReviewProps) {
  return (
    <div className="kiv-review">
      {items.length === 0 ? (
        <p className="panel-footnote">Nothing flagged right now — nice work.</p>
      ) : (
        <>
          <div className="kiv-prompt">
            <p>
              You have <strong>{items.length} flagged concept{items.length === 1 ? '' : 's'}</strong>. Review one
              below to work on it.
            </p>
          </div>

          <p className="kiv-section-label">Flagged concepts</p>
          <div className="kiv-list">
            {items.map((item) => (
              <div key={item.id} className="kiv-row">
                <div className="kiv-row-main">
                  <div className="kiv-row-top">
                    <span className="kiv-title">{item.title}</span>
                    <span className={`kiv-tag kiv-tag-${item.foundationGap ? 'foundation_gap' : 'kiv'}`}>
                      {item.foundationGap ? 'Foundation gap' : 'KIV'}
                    </span>
                  </div>
                  <span className="kiv-meta">
                    {item.foundationGap ? 'Needs foundational work' : 'Failed advanced, moved on'}
                    {item.masteryScore !== undefined ? ` — mastery ${item.masteryScore.toFixed(2)}` : ''}
                  </span>
                  {item.masteryScore !== undefined && (
                    <div className="kiv-mastery-bar">
                      <div
                        className={`kiv-mastery-fill kiv-mastery-fill-${item.foundationGap ? 'foundation_gap' : 'kiv'}`}
                        style={{ width: `${item.masteryScore * 100}%` }}
                      />
                    </div>
                  )}
                </div>
                <button
                  type="button"
                  className="kiv-review-btn"
                  onClick={() => onReview(item.id, item.title, item.status, item.unmetPrerequisiteTitles ?? [])}
                >
                  {REVIEW_ICON}
                  Review
                </button>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
