-- deferred.md #2d — the confirm-first branch-into-new-journey flow: the
-- tutor offers to start a separate track for a `branch_gap` classification
-- (deferred.md #2b) on the SAME turn it's detected, then the student's
-- NEXT message is read as answering that offer, not as a normal turn.
-- This column is what makes the next turn recognizable as "answering an
-- open offer" rather than an ordinary message — cleared as soon as it's
-- resolved (confirmed or declined), same lifecycle as pre_tangent_concept_id
-- already has for Flow 3.
ALTER TABLE study_threads ADD COLUMN pending_branch_topic TEXT;
