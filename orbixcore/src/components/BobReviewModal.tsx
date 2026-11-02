import { useState } from 'react';
import { marked } from 'marked';
import type { BobReviewPayload } from '../hooks/api';
import { bobReviewRespond } from '../hooks/api';
import './BobReviewModal.css';

marked.setOptions({ breaks: true, gfm: true });
const renderMarkdown = (text: string) => marked.parse(text, { async: false }) as string;

interface Props {
  review: BobReviewPayload;
  onClose: () => void;
}

const TYPE_LABELS: Record<string, { label: string; color: string }> = {
  created: { label: 'Created', color: '#34d399' },
  updated: { label: 'Updated', color: '#6C8FFF' },
  executed: { label: 'Executed', color: '#F59E0B' },
  generated: { label: 'Generated', color: '#A78BFA' },
};

export default function BobReviewModal({ review, onClose }: Props) {
  const [editing, setEditing] = useState(false);
  const [editContent, setEditContent] = useState(review.content);
  const [responding, setResponding] = useState(false);

  const typeInfo = TYPE_LABELS[review.type] || TYPE_LABELS.generated;

  const handleAccept = async () => {
    setResponding(true);
    await bobReviewRespond(review.id, 'accepted').catch(() => {});
    onClose();
  };

  const handleDecline = async () => {
    setResponding(true);
    await bobReviewRespond(review.id, 'declined').catch(() => {});
    onClose();
  };

  const handleSaveEdits = async () => {
    setResponding(true);
    await bobReviewRespond(review.id, 'edited', editContent).catch(() => {});
    onClose();
  };

  return (
    <div className="bob-review-overlay">
      <div className="bob-review-backdrop" onClick={onClose} />
      <div className="bob-review-modal">
        {/* Accent bar */}
        <div className="bob-review-accent" style={{ background: `linear-gradient(90deg, ${typeInfo.color}, ${typeInfo.color}33)` }} />

        {/* Header */}
        <div className="bob-review-header">
          <div className="bob-review-header-left">
            <span className="bob-review-badge" style={{ color: typeInfo.color, background: `${typeInfo.color}18` }}>
              {typeInfo.label}
            </span>
            <span className="bob-review-title">{review.title}</span>
          </div>
          <button className="bob-review-close" onClick={onClose}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>

        {/* Summary */}
        {review.summary && (
          <div className="bob-review-summary">{review.summary}</div>
        )}

        {/* Content */}
        <div className="bob-review-body">
          {editing ? (
            <textarea
              className="bob-review-editor"
              value={editContent}
              onChange={e => setEditContent(e.target.value)}
              autoFocus
            />
          ) : (
            <div
              className="bob-review-content"
              dangerouslySetInnerHTML={{ __html: renderMarkdown(review.content) }}
            />
          )}
        </div>

        {/* Files changed */}
        {review.files_changed && review.files_changed.length > 0 && (
          <div className="bob-review-files">
            <span className="bob-review-files-label">Files:</span>
            {review.files_changed.map((f, i) => (
              <span key={i} className="bob-review-file">{f}</span>
            ))}
          </div>
        )}

        {/* Actions */}
        <div className="bob-review-actions">
          <button
            className="bob-review-btn edit"
            onClick={() => setEditing(!editing)}
            disabled={responding}
          >
            {editing ? 'Preview' : 'Edit'}
          </button>
          <div className="bob-review-actions-right">
            <button
              className="bob-review-btn decline"
              onClick={handleDecline}
              disabled={responding}
            >
              Decline
            </button>
            <button
              className="bob-review-btn accept"
              onClick={editing ? handleSaveEdits : handleAccept}
              disabled={responding}
            >
              {editing ? 'Save & Accept' : 'Accept'}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
