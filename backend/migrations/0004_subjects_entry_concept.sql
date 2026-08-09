-- deferred.md #4 audit finding: the existing-subject diagnostic-reuse path
-- picked an entry concept via `ORDER BY order_index LIMIT 1` — order_index
-- is just raw array position from the original Dify response, never tied
-- to which concept generate_dag() actually named entry_concept. Only the
-- first student to create a subject got the right one; every later
-- student reusing that subject silently got whatever concept the LLM
-- happened to list first. This column stores the real answer durably.
ALTER TABLE subjects ADD COLUMN entry_concept_id UUID REFERENCES canonical_concepts(concept_id);
