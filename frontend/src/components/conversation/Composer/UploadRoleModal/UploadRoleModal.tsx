import { Modal } from '../../../ui/Modal/Modal';
import type { UploadRole } from '../../../../types';

interface UploadRoleModalProps {
  // The file currently being asked about — null means nothing pending,
  // so the modal stays closed. One file at a time: when a batch is
  // dropped/selected together, the caller (Composer) advances through
  // pendingFiles sequentially, re-opening this for the next one.
  file: File | null;
  onSelectRole: (role: UploadRole) => void;
  onClose: () => void;
}

// PRD.md / Combined_Architecture_File.md, "Upload System [LOCKED] — Roles
// (asked at upload time)". Only the two ingestion-tier roles are offered
// here — 'ephemeral' ("Just help with this") is intentionally not wired
// yet (see plan doc: no message-level field exists to carry one-turn-only
// context through POST /memoryless/messages).
export function UploadRoleModal({ file, onSelectRole, onClose }: UploadRoleModalProps) {
  return (
    <Modal open={file !== null} title="How do you want to use this document?" onClose={onClose}>
      {file && (
        <>
          <p className="modal-hint">{file.name}</p>
          <div className="modal-option-list">
            <button type="button" className="modal-option" onClick={() => onSelectRole('prompt_upload')}>
              <span>Teach me from this only</span>
              <span className="modal-option-current">This thread only</span>
            </button>
            <button type="button" className="modal-option" onClick={() => onSelectRole('material_upload')}>
              <span>Add as one source among many</span>
              <span className="modal-option-current">Available later too</span>
            </button>
          </div>
        </>
      )}
    </Modal>
  );
}
