import { Component, type ReactNode } from 'react';

/**
 * The last thing between a render error and a congregation.
 *
 * An output window must never show a browser error page or a white rectangle: whatever went
 * wrong, the screen falls back to the theme's background and asks the control surface for the
 * state again, which is enough to recover from a slide that could not be rendered. Failing that,
 * a dark screen is the correct thing to be showing.
 */
export class OutputBoundary extends Component<
  { background: string; children: ReactNode; onRetry: () => void },
  { failed: boolean }
> {
  state = { failed: false };

  static getDerivedStateFromError(): { failed: boolean } {
    return { failed: true };
  }

  componentDidCatch(): void {
    // One attempt, on the next frame: the state that arrives may well be renderable, and a loop
    // of failing renders in front of a room is worse than a blank background.
    setTimeout(() => {
      this.setState({ failed: false });
      this.props.onRetry();
    }, 500);
  }

  render(): ReactNode {
    return this.state.failed
      ? <div className="h-full w-full" style={{ background: this.props.background }} />
      : this.props.children;
  }
}
