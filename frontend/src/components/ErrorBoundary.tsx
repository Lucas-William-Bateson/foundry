import { Component, type ErrorInfo, type ReactNode } from "react";

interface Props {
  children: ReactNode;
  fallback?: (error: Error, reset: () => void) => ReactNode;
  section?: string;
}

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(
      `ErrorBoundary caught error${this.props.section ? ` in ${this.props.section}` : ""}:`,
      error,
      info,
    );
  }

  reset = () => {
    this.setState({ error: null });
  };

  render() {
    if (this.state.error) {
      if (this.props.fallback) {
        return this.props.fallback(this.state.error, this.reset);
      }
      return (
        <div
          style={{
            borderRadius: "6px",
            border: "1px solid rgba(212, 75, 75, 0.2)",
            backgroundColor: "rgba(212, 75, 75, 0.06)",
            padding: "1rem",
            fontSize: "0.8125rem",
          }}
        >
          <p style={{ fontWeight: 500, color: "#D44B4B" }}>
            Something went wrong
            {this.props.section ? ` in ${this.props.section}` : ""}
          </p>
          <p style={{ marginTop: "0.25rem", color: "#D44B4B", opacity: 0.8 }}>
            {this.state.error.message}
          </p>
          <button
            onClick={this.reset}
            style={{
              marginTop: "0.5rem",
              fontSize: "0.75rem",
              color: "#D44B4B",
              textDecoration: "underline",
              background: "none",
              border: "none",
              cursor: "pointer",
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
