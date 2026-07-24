/** Dev-only Phase 1 2.5D fixture battlefield (issue #483). */
export { FixtureBattlefield, type FixtureHarnessHook } from './FixtureBattlefield';
export {
  FIXTURE_SCENARIOS,
  fixtureScenario,
  normalizeFixture,
  type FixtureFrame,
  type FixtureScenario,
} from './scenarios';
export {
  FrameBudgetSampler,
  fixtureBudget,
  fixtureBudgetReport,
  frameSummaryPasses,
  summarizeFrameDeltas,
  type FixtureBudget,
  type FixtureBudgetReport,
  type FrameMode,
  type FrameSummary,
} from './metrics';
