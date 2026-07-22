import { useEffect, useId, useState, type FormEvent } from 'react';
import { Button } from '../../ui/Button/Button';
import { Modal } from '../../ui/Modal/Modal';

interface ProjectModalProps {
  open: boolean;
  onClose: () => void;
  onCreate: (title: string, description: string | null) => Promise<unknown>;
}

export function ProjectModal({ open, onClose, onCreate }: ProjectModalProps) {
  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const titleId = useId();
  const descriptionId = useId();

  useEffect(() => {
    if (!open) return;
    setTitle('');
    setDescription('');
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
      await onCreate(trimmedTitle, description.trim() || null);
      onClose();
    } catch {
      setError('Project creation failed. Please try again.');
      setIsSubmitting(false);
    }
  };

  return (
    <Modal open={open} title="Create a project" onClose={onClose}>
      <form className="modal-form" onSubmit={handleSubmit}>
        <label className="modal-field" htmlFor={titleId}>
          <span className="modal-label">What are you working on?</span>
          <input
            id={titleId}
            className="modal-input"
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            placeholder="Name your project"
            autoFocus
            required
          />
        </label>

        <label className="modal-field" htmlFor={descriptionId}>
          <span className="modal-label">
            What are you trying to achieve?
            <span className="modal-optional">Optional</span>
          </span>
          <textarea
            id={descriptionId}
            className="modal-textarea"
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            placeholder="Describe your goals, subject, or intended outcome"
          />
        </label>

        <p className="modal-hint">Projects keep related learning tracks and their goals together.</p>

        {error && <p className="modal-error">{error}</p>}

        <div className="modal-actions">
          <Button variant="secondary" onClick={onClose} disabled={isSubmitting}>
            Cancel
          </Button>
          <Button type="submit" disabled={!title.trim() || isSubmitting}>
            {isSubmitting ? 'Creating...' : 'Create project'}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
