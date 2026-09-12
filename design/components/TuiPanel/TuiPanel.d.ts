import * as React from 'react';

export interface TuiPanelProps {
  /** Title inset into the top rule, e.g. "Known Hosts". */
  title?: string;
  /** Right-aligned annotation in the top rule, e.g. "12 records". */
  right?: string;
  /** Content rows, one string per character row. */
  rows?: string[];
  /** Frame width in character cells. Default 64. */
  width?: number;
  /** Focused panels draw an orange border — only one panel at a time. */
  focus?: boolean;
  /** Use the ╔═╗ double border, reserved for blocking modals. */
  double?: boolean;
  /** Pad with blank rows up to this count so the frame never jumps. */
  minRows?: number;
  /** Rendering size of one cell in px. Default 15. */
  fontSize?: number;
  style?: React.CSSProperties;
  /** Custom rows rendered between `rows` and the bottom border. */
  children?: React.ReactNode;
}

/** A titled box-drawing frame — the universal container of the cfdns TUI system. */
export declare function TuiPanel(props: TuiPanelProps): React.JSX.Element;
