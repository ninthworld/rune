/**
 * The action list, and the choices an action needs before it can be submitted.
 *
 * Every button here comes from `valid_actions`. The client offers nothing the server did not
 * list, disables nothing on its own reasoning, and computes no legality — the only thing
 * gating the submit button is whether each owed slot holds the number of ids the server itself
 * advertised (`submission.ts`).
 */
import { useState } from 'react'

import type { ClientMessage, Prompt, ValidAction } from './../protocol'
import {
  advertisedCount,
  buildChooseAction,
  isSubmittable,
  requiredSlots,
  toggleSelection,
  type Draft,
} from './../submission'

interface Props {
  actions: readonly ValidAction[]
  /** Names an entity id for display; the server's labels are preferred where it gives them. */
  labelFor(id: string): string
  send(message: ClientMessage): void
}

/** An action needing no choices is a single click; anything else opens the choice panel. */
const needsChoices = (action: ValidAction): boolean =>
  (action.requirements?.length ?? 0) > 0 || (action.prompts?.length ?? 0) > 0

let submissionCounter = 0
const nextSubmissionId = (): string => `s:${++submissionCounter}`

export function ActionPanel({ actions, labelFor, send }: Props) {
  const [openId, setOpenId] = useState<string>()
  const [draft, setDraft] = useState<Draft>({})

  if (actions.length === 0) {
    return (
      <section aria-labelledby="actions-heading">
        <h2 id="actions-heading">Actions</h2>
        <p>Nothing to do right now.</p>
      </section>
    )
  }

  const open = actions.find((action) => action.id === openId)

  const submit = (action: ValidAction, answers: Draft) => {
    send(buildChooseAction(action, answers, nextSubmissionId()))
    setOpenId(undefined)
    setDraft({})
  }

  return (
    <section aria-labelledby="actions-heading">
      <h2 id="actions-heading">Actions</h2>
      <ul className="actions">
        {actions.map((action) => (
          <li key={action.id}>
            <button
              type="button"
              onClick={() => {
                if (!needsChoices(action)) {
                  submit(action, {})
                  return
                }
                setOpenId(action.id === openId ? undefined : action.id)
                setDraft({})
              }}
              aria-expanded={needsChoices(action) ? action.id === openId : undefined}
            >
              {action.label}
              {action.mana_ability ? ' ⟨mana⟩' : ''}
              {needsChoices(action) ? ' …' : ''}
            </button>
          </li>
        ))}
      </ul>

      {open && (
        <ChoicePanel
          action={open}
          draft={draft}
          labelFor={labelFor}
          onChange={setDraft}
          onSubmit={() => submit(open, draft)}
          onCancel={() => {
            setOpenId(undefined)
            setDraft({})
          }}
        />
      )}
    </section>
  )
}

interface ChoiceProps {
  action: ValidAction
  draft: Draft
  labelFor(id: string): string
  onChange(draft: Draft): void
  onSubmit(): void
  onCancel(): void
}

function ChoicePanel({ action, draft, labelFor, onChange, onSubmit, onCancel }: ChoiceProps) {
  const required = requiredSlots(action, draft)
  const ready = isSubmittable(action, draft)

  return (
    <div className="choices">
      <h3>{action.label}</h3>

      {(action.requirements ?? []).map((requirement) => (
        <fieldset key={requirement.slot}>
          <legend>{requirement.prompt}</legend>
          {/* No count is advertised for a target slot, so nothing here caps the selection —
              the server decides how many are legal. */}
          {(requirement.candidates ?? []).map((id) => (
            <Choice
              key={id}
              name={requirement.slot}
              label={labelFor(id)}
              selected={(draft[requirement.slot] ?? []).includes(id)}
              onToggle={() => onChange(toggleSelection(draft, requirement.slot, id, null))}
            />
          ))}
        </fieldset>
      ))}

      {(action.prompts ?? [])
        .filter((prompt) => required.has(prompt.slot))
        .map((prompt) => (
          <PromptField
            key={prompt.slot}
            prompt={prompt}
            draft={draft}
            labelFor={labelFor}
            onChange={onChange}
          />
        ))}

      <p>
        <button type="button" onClick={onSubmit} disabled={!ready}>
          Submit
        </button>{' '}
        <button type="button" onClick={onCancel}>
          Cancel
        </button>
      </p>
    </div>
  )
}

function PromptField({
  prompt,
  draft,
  labelFor,
  onChange,
}: {
  prompt: Prompt
  draft: Draft
  labelFor(id: string): string
  onChange(draft: Draft): void
}) {
  const limit = advertisedCount(prompt)
  const chosen = draft[prompt.slot] ?? []

  if (prompt.kind === 'number') {
    // The bounds are the server's, computed from mana, the source's text, and the state. The
    // control offers exactly that range and works out no affordability of its own.
    return (
      <fieldset>
        <legend>{prompt.prompt}</legend>
        <label>
          {prompt.min}–{prompt.max}{' '}
          <input
            type="number"
            min={prompt.min}
            max={prompt.max}
            value={chosen[0] ?? ''}
            onChange={(event) =>
              onChange({ ...draft, [prompt.slot]: event.target.value ? [event.target.value] : [] })
            }
          />
        </label>
      </fieldset>
    )
  }

  if (prompt.kind === 'order') {
    // A permutation, built by clicking items in the order wanted.
    const items = prompt.items ?? []
    return (
      <fieldset>
        <legend>
          {prompt.prompt} — click in order ({chosen.length}/{items.length})
        </legend>
        {items.map((id) => {
          const position = chosen.indexOf(id)
          return (
            <Choice
              key={id}
              name={prompt.slot}
              label={`${position >= 0 ? `${position + 1}. ` : ''}${labelFor(id)}`}
              selected={position >= 0}
              onToggle={() => onChange(toggleSelection(draft, prompt.slot, id, items.length))}
            />
          )
        })}
      </fieldset>
    )
  }

  const options =
    prompt.kind === 'option'
      ? (prompt.options ?? []).map((option) => ({ id: option.id, label: option.label }))
      : (prompt.candidates ?? []).map((id) => ({ id, label: labelFor(id) }))

  return (
    <fieldset>
      <legend>
        {prompt.prompt}
        {limit !== null && limit > 1 ? ` (${chosen.length}/${limit})` : ''}
      </legend>
      {options.map((option) => (
        <Choice
          key={option.id}
          name={prompt.slot}
          label={option.label}
          selected={chosen.includes(option.id)}
          onToggle={() => onChange(toggleSelection(draft, prompt.slot, option.id, limit))}
        />
      ))}
    </fieldset>
  )
}

function Choice({
  name,
  label,
  selected,
  onToggle,
}: {
  name: string
  label: string
  selected: boolean
  onToggle(): void
}) {
  return (
    <label className="choice">
      <input type="checkbox" name={name} checked={selected} onChange={onToggle} />
      {label}
    </label>
  )
}
