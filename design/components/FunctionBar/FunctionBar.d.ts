import * as React from 'react';

export interface FunctionBarItem {
  /** The key cap text, e.g. "F2", "Ins", "^R". */
  keyLabel: string;
  /** Single Title-Case word, e.g. "Add", "Delete". */
  label: string;
  /** Inverts the cap to a solid orange fill. */
  active?: boolean;
  /** Tints the label red for destructive actions. */
  danger?: boolean;
  disabled?: boolean;
  title?: string;
  onClick?: () => void;
}

export interface FunctionBarProps {
  items?: FunctionBarItem[];
  /** Rendering size in px. Default 13. */
  fontSize?: number;
  style?: React.CSSProperties;
}

/** The always-visible bottom F-key action strip. */
export declare function FunctionBar(props: FunctionBarProps): React.JSX.Element;
