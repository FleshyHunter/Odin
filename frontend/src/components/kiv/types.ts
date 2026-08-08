// Mirrors journey_concepts.status's real 'kiv' value + foundation_gap
// column (migrations/0001_initial_schema.sql) — PRD.md's KIV Review
// section locks foundation_gap as the MORE serious of the two (failed a
// BASIC question, vs. kiv's failed-an-ADVANCED-question-and-moved-on),
// so they're kept as a real distinction here, not one generic "flagged"
// state.
export type KivSeverity = 'foundation_gap' | 'kiv';

export interface KivItem {
  id: string;
  conceptTitle: string;
  severity: KivSeverity;
  // 0-1, mirrors mastery_bank.mastery_score — mixed-session ordering
  // (PRD.md) weights by distance below 0.75, so this is real data the
  // real component will need, not just display flavor.
  masteryScore: number;
}

export interface KivData {
  trackTitle: string;
  items: KivItem[];
}
