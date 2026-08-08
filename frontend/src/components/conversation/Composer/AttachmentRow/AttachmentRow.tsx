import type { Attachment } from '../../../../types';
import { AttachmentCard } from './AttachmentCard';
import './attachmentRow.css';

interface AttachmentRowProps {
  attachments: Attachment[];
  onRemove: (attachmentId: string) => void;
  onRetry: (attachmentId: string) => void;
}

export function AttachmentRow({ attachments, onRemove, onRetry }: AttachmentRowProps) {
  if (attachments.length === 0) return null;

  return (
    <div className="attachment-row">
      {attachments.map((attachment) => (
        <AttachmentCard
          key={attachment.id}
          attachment={attachment}
          onRemove={() => onRemove(attachment.id)}
          onRetry={() => onRetry(attachment.id)}
        />
      ))}
    </div>
  );
}
