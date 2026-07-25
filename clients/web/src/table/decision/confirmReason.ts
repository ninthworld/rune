/**
 * The one disablement the control language permits, and its phrasing
 * (`docs/design/control-language.md` §3.2, D14, GAP-4; §9 storyboard 9 — issue
 * #534).
 *
 * §3.2 is blunt: **disabled is rare by construction.** An action the server has
 * not offered is not rendered at all, so a greyed control is normally a bug. The
 * single exception is a reason the *server* states — today a `PromptOption.requires`
 * whose slot is not yet satisfied, which storyboard 9 states as "CONFIRM is
 * disabled until exactly `count` ids are chosen". `ControlButton` enforces the
 * rest: its `disabledReason` is a string, not a boolean, so nothing can render
 * disabled without printing the server's words.
 *
 * This function does **not** decide whether the count is met — that is
 * `multiSelect.allSlotsSatisfied`, over the server's own `count`, and duplicating
 * it here would be the client computing cardinality twice. It only carries the
 * server's prompt into the phrasing the button requires, so the wording lives in
 * one place rather than being re-invented at each call site.
 *
 * It lives in its own module rather than beside {@link DecisionPlaque} so that
 * file exports components only — the react-refresh boundary the client's lint
 * config draws.
 *
 * Consumed in production by `LiveMatchTable`, building the plaque's
 * `confirm.disabledReason`.
 */
export function confirmDisabledReason(
  satisfied: boolean,
  slotPrompt: string | undefined,
): string | undefined {
  if (satisfied) return undefined;
  // With no slot prompt there is no server-stated reason to print — and without
  // one the control may not render disabled at all (D14), so it stays enabled
  // and the server rejects a premature answer. That is the correct failure: a
  // client-invented reason would be the client stating legality, which GAP-4
  // records as having no protocol representation.
  return slotPrompt === undefined || slotPrompt === '' ? undefined : `needs: ${slotPrompt}`;
}
