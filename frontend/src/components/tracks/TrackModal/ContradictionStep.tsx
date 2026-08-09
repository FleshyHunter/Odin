interface ContradictionStepProps {
  level: string;
  hasBackup: boolean;
  onConfirmDowngrade: () => void;
  onTryBackup: () => void;
  isSubmitting: boolean;
  error: string | null;
}

// PRD.md Onboarding Diagnostic Step 3's required disclosure: "tell the
// student directly, don't silently reassign." Wording matches the
// locked copy as closely as a real UI can (the doc's own
// [foundational/advanced] bracket is a placeholder for whichever
// direction the mismatch went — this app's diagnostic only ever
// contradicts downward, so "foundational" is the only real case today).
export function ContradictionStep({
  level,
  hasBackup,
  onConfirmDowngrade,
  onTryBackup,
  isSubmitting,
  error,
}: ContradictionStepProps) {
  return (
    <div className="modal-form">
      <p className="exercise-q">
        This looks like it might be a bit more foundational than {level.toLowerCase()} — want to confirm, or try the
        backup question?
      </p>

      {error && <p className="modal-error">{error}</p>}

      <div className="modal-actions">
        <button type="button" className="btn-primary" onClick={onConfirmDowngrade} disabled={isSubmitting}>
          {isSubmitting ? 'Confirming...' : 'Confirm'}
        </button>
        {hasBackup && (
          <button type="button" className="btn-secondary" onClick={onTryBackup} disabled={isSubmitting}>
            Try backup question
          </button>
        )}
      </div>
    </div>
  );
}
