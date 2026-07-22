import { useEffect, useId, useState, type FormEvent } from 'react';
import { Button } from '../../ui/Button/Button';
import { Modal } from '../../ui/Modal/Modal';

interface TrackModalProps {
  open: boolean;
  onClose: () => void;
  onCreate: (title: string) => Promise<unknown>;
}

export function TrackModal({ open, onClose, onCreate }: TrackModalProps) {
  const [title, setTitle] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const titleId = useId();

  useEffect(() => {
    if (!open) return;
    setTitle('');
    setError(null);
    setIsSubmitting(false);
  }, [open]);

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const trimmedTitle = title.trim();
    if (!trimmedTitle || isSubmitting) return;

    setIsSubmitting(true);
    setError(null);
    try {
      await onCreate(trimmedTitle);
      onClose();
    } catch {
      setError('Track creation failed. Please try again.');
      setIsSubmitting(false);
    }
  };

  return (
    <Modal open={open} title="Create a track" onClose={onClose}>
      <form className="modal-form" onSubmit={handleSubmit}>
        <label className="modal-field" htmlFor={titleId}>
          <span className="modal-label">What do you want to learn?</span>
          <input
            id={titleId}
            className="modal-input"
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            placeholder="Name your learning track"
            autoFocus
            required
          />
        </label>

        <p className="modal-hint">
          A track is a focused learning journey with its own conversation, roadmap, exercises, and progress.
        </p>

        {error && <p className="modal-error">{error}</p>}

        <div className="modal-actions">
          <Button variant="secondary" onClick={onClose} disabled={isSubmitting}>
            Cancel
          </Button>
          <Button type="submit" disabled={!title.trim() || isSubmitting}>
            {isSubmitting ? 'Creating...' : 'Create track'}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
