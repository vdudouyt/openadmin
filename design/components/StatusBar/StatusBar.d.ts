import * as React from 'react';

export interface StatusBarProps {
  /** Left slot — where you are, e.g. "10 hosts · 5 mounted". */
  left?: React.ReactNode;
  /** Middle slot — a hover/context hint printed on mouse motion. */
  hint?: React.ReactNode;
  /** Right slot — transient status text. */
  message?: React.ReactNode;
  /** Tone of `message`. */
  tone?: 'ok' | 'err' | 'warn' | 'info' | '';
  /** Rendering size in px. Default 13. */
  fontSize?: number;
  style?: React.CSSProperties;
}

/** The context / status row above the function bar. */
export declare function StatusBar(props: StatusBarProps): React.JSX.Element;
