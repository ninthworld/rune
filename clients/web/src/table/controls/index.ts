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
export { FACE_TOKEN_NAMES, faceChamfer, facePointDepth, faceTipInset } from './plaqueGeometry';
export { derivePrimary, RESPOND_LABEL, RESPOND_ACCESSIBLE_NAME } from './controlPrimary';
export type {
  ControlSession,
  PrimaryRule,
  PrimaryForm,
  PrimaryInput,
  PrimaryDerivation,
} from './controlPrimary';
export { PhasePlaque } from './PhasePlaque';
export type { PhasePlaqueProps } from './PhasePlaque';
export { PHASE_GROUPS, STEP_NAME, pipStates } from './phaseSteps';
export type { PhaseGroup, PipState } from './phaseSteps';
export { ControlCluster } from './ControlCluster';
export type { ControlClusterProps } from './ControlCluster';
export { ManaReservoir } from './ManaReservoir';
export type { ManaReservoirProps } from './ManaReservoir';
