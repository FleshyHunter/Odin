interface MasteryBarProps {
  conceptTitle: string;
  masteryScore: number; // 0-1, mastery_bank.mastery_score
  requiredStreak?: number;
}

// ARCHITECTURE_LOCK.md's Concept Completion Rule requires
// advanced_correct_streak >= 3 (env var ADVANCED_STREAK_REQUIRED=3), not 2 —
// odin_session.html's footnote copy ("Two correct answers in a row")
// conflicts with this LOCKED numeric threshold. Per the handoff's ground
// rule that the architecture doc wins on LOCKED conflicts, the copy below
// is corrected to the actual required streak rather than the prototype's
// literal text. Flagged here as a deviation, not silently picked.
const DEFAULT_REQUIRED_STREAK = 3;

export function MasteryBar({ conceptTitle, masteryScore, requiredStreak = DEFAULT_REQUIRED_STREAK }: MasteryBarProps) {
  const percentage = Math.round(masteryScore * 100);

  return (
    <>
      <div className="mastery-block">
        <div className="mastery-label">
          <span>{conceptTitle}</span>
          <span>{percentage}%</span>
        </div>
        <div className="mastery-bar">
          <div className="mastery-fill" style={{ width: `${percentage}%` }} />
        </div>
      </div>

      <p className="panel-footnote">
        {requiredStreak} correct answers in a row on an advanced question will mark this concept complete.
      </p>
    </>
  );
}
