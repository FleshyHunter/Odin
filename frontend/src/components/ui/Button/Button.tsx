import type { ReactNode } from 'react';
import './button.css';

interface ButtonProps {
  type?: 'button' | 'submit';
  onClick?: () => void;
  disabled?: boolean;
  variant?: 'primary' | 'secondary';
  children: ReactNode;
}

// Shared form action button. Primary is the gold CTA used throughout the
// app; secondary is the transparent outlined action used for modal cancel.
export function Button({ type = 'button', onClick, disabled, variant = 'primary', children }: ButtonProps) {
  return (
    <button type={type} className={`btn-${variant}`} onClick={onClick} disabled={disabled}>
      {children}
    </button>
  );
}
