/**
 * The match control language's public surface (issue #534).
 *
 * Everything the contextual shell composes from — the button family, the
 * geometry mirror, the primary derivation, the phase plaque, and the cluster —
 * is re-exported here, so a consumer imports `table/controls` rather than
 * reaching into the directory's internals.
 */
export { ControlButton, IconButton } from './ControlButton';
export type { ControlVariant, ControlButtonProps, IconButtonProps } from './ControlButton';
export { CONTROL, CONTROL_TOKEN_NAMES, PIP_COUNT, pipRowWidth } from './controlTokens';
