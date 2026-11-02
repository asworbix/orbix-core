import { Component, type ReactNode } from 'react';

interface Props {
  children: ReactNode;
  fallbackLabel?: string;
}

interface State {
  hasError: boolean;
  error: string;
}

export default class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false, error: '' };

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error: error.message };
  }

  componentDidCatch(error: Error) {
    console.error('[ErrorBoundary]', this.props.fallbackLabel || 'Panel', error);
  }

  render() {
    if (this.state.hasError) {
      return (
        <div style={{
          flex: 1, display: 'flex', flexDirection: 'column',
          alignItems: 'center', justifyContent: 'center', gap: 12,
          padding: 32, color: 'var(--text-muted)', fontSize: 13,
          textAlign: 'center',
        }}>
          <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" style={{ opacity: 0.5 }}>
            <circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/>
          </svg>
          <div style={{ fontWeight: 600, color: 'var(--text-secondary)' }}>
            {this.props.fallbackLabel || 'This panel'} ran into an issue
          </div>
          <div style={{ fontSize: 11, maxWidth: 300 }}>{this.state.error}</div>
          <button
            onClick={() => this.setState({ hasError: false, error: '' })}
            style={{
              marginTop: 8, padding: '6px 16px', borderRadius: 20,
              fontSize: 12, fontWeight: 600, cursor: 'pointer',
              border: '1px solid var(--border)', color: 'var(--text-secondary)',
              background: 'var(--bg-surface)',
            }}
          >
            Try again
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
