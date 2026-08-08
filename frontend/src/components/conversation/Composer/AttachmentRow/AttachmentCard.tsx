import { useEffect, useState } from 'react';
import type { Attachment } from '../../../../types';

interface AttachmentCardProps {
  attachment: Attachment;
  onRemove: () => void;
  onRetry: () => void;
}

const TEXT_LIKE_EXTENSIONS = /\.(md|markdown|txt)$/i;

function isTextLike(file: File): boolean {
  return file.type.startsWith('text/') || TEXT_LIKE_EXTENSIONS.test(file.name);
}

function fileTypeBadge(filename: string): string {
  const match = /\.([a-z0-9]+)$/i.exec(filename);
  return match ? match[1].toUpperCase() : 'FILE';
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

// Single attached-file card, shown above the composer. Branches on file
// type per the reference design: images get a real visual thumbnail,
// everything else gets a filename/detail/type-badge card.
export function AttachmentCard({ attachment, onRemove, onRetry }: AttachmentCardProps) {
  const { file, status, role, errorMessage, chunkCount } = attachment;
  const isImage = file.type.startsWith('image/');
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  const [lineCount, setLineCount] = useState<number | null>(null);

  useEffect(() => {
    if (!isImage) return;
    const url = URL.createObjectURL(file);
    setImageUrl(url);
    return () => URL.revokeObjectURL(url);
  }, [isImage, file]);

  useEffect(() => {
    if (isImage || !isTextLike(file)) return;
    let cancelled = false;
    file.text().then((text) => {
      if (!cancelled) setLineCount(text.length === 0 ? 0 : text.split('\n').length);
    });
    return () => {
      cancelled = true;
    };
  }, [isImage, file]);

  // Once the real chunk_count comes back, prefer it over the pre-upload
  // line-count guess — it's the server's own account of what actually got
  // ingested, not just a client-side approximation of the raw file.
  const detail =
    chunkCount !== undefined
      ? `${chunkCount} chunk${chunkCount === 1 ? '' : 's'}`
      : lineCount !== null
        ? `${lineCount} line${lineCount === 1 ? '' : 's'}`
        : formatFileSize(file.size);

  const cardClassName = `attachment-card${status === 'error' ? ' attachment-card-error' : ''}${
    isImage ? ' attachment-card-image' : ''
  }`;

  return (
    <div className={cardClassName}>
      {isImage ? (
        <div className="attachment-thumb">
          {imageUrl && <img src={imageUrl} alt={file.name} />}
          {status === 'uploading' && <div className="attachment-thumb-overlay" aria-hidden="true" />}
        </div>
      ) : (
        <div className="attachment-icon" aria-hidden="true">
          {fileTypeBadge(file.name)}
        </div>
      )}

      <div className="attachment-info">
        <span className="attachment-name" title={file.name}>
          {file.name}
        </span>
        {status === 'error' ? (
          <span className="attachment-error-message">{errorMessage ?? 'Upload failed.'}</span>
        ) : (
          <span className="attachment-detail">
            {status === 'uploading' ? 'Uploading…' : detail}
            {status === 'ready' && attachment.deduped && ' · already added'}
            {status === 'ready' && role === 'material_upload' && !attachment.deduped && ' · source'}
            {status === 'ready' && role === 'prompt_upload' && !attachment.deduped && ' · this track only'}
          </span>
        )}
      </div>

      {status === 'error' && (
        <button type="button" className="attachment-retry" onClick={onRetry}>
          Retry
        </button>
      )}
      <button
        type="button"
        className="attachment-remove"
        aria-label={`Remove ${file.name}`}
        onClick={onRemove}
        disabled={status === 'uploading'}
      >
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.2}>
          <line x1="18" y1="6" x2="6" y2="18" />
          <line x1="6" y1="6" x2="18" y2="18" />
        </svg>
      </button>
    </div>
  );
}
