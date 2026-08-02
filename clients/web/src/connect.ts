/**
 * Where this client connects, and who it says it is when it gets there.
 *
 * Both are device facts rather than game state, and both are remembered here in the manner of
 * ADR 0012's art preference: written to this device's own storage, never sent anywhere, and
 * absent or unreadable is always a working answer. A player who has connected once presses one
 * key the next time; a player on a fresh device is asked once, in setup, instead of finding an
 * input box in the header of every screen asking who they are.
 *
 * **The server list is client-side configuration.** The protocol has no server directory and
 * this module is not inventing one: `PUBLIC_SERVERS` is a table in this file, and the custom
 * entry is what keeps a table in a file from being a limitation. Each predefined entry carries
 * the region it runs in, because "which of these is near me" is the only question a player has
 * about a list of addresses.
 *
 * **The address precedence in `socket.ts` is extended, not replaced.** A `?server=` parameter is
 * explicit configuration for this page load and still wins over everything, including a
 * remembered choice — it is what points one built bundle at a staging server or a stub. Below it
 * comes what this device chose last, and below that the build-time default, the page's own
 * origin, and the local address, exactly as before.
 */
import { defaultServerUrl, serverUrlOverride } from './socket'

/** One address a player can connect to, and where the machine behind it is. */
export interface ServerEntry {
  id: string
  label: string
  /** The region it runs in. Absent only for the entry whose address the player types. */
  region?: string
  /** The WebSocket address. Empty for the custom entry, which has no address until one is typed. */
  url: string
}

/**
 * The published servers, each with its region.
 *
 * Empty, because SAGE runs no public server yet. That is the honest state of the world rather
 * than a gap: this is the table an address goes in the day one exists, and until then the list a
 * player sees is the two addresses that really are reachable plus one they can type. A row
 * pointing at a host that answers nothing would be worse than no row.
 */
export const PUBLIC_SERVERS: readonly ServerEntry[] = []

/** The address a local `sage-server` binds, which is how everybody plays today. */
export const LOCAL_URL = 'ws://127.0.0.1:9000'

/** The id of the entry that has no address of its own until the player types one. */
export const CUSTOM = 'custom'

/**
 * The list to choose from: what this page is configured for, this device, the published servers,
 * and an address you type.
 *
 * The configured entry is `defaultServerUrl()` — the query parameter, the build-time value, or
 * the page's own origin — so choosing it is choosing exactly what an unconfigured client would
 * have done. It is dropped when it is the same address as the local entry, because two rows
 * carrying one address is a choice that is not one.
 */
export function serverChoices(): readonly ServerEntry[] {
  const configured = defaultServerUrl()
  const entries: ServerEntry[] = []
  if (configured !== LOCAL_URL) {
    entries.push({
      id: 'configured',
      label: hostOf(configured),
      region: 'Where this page came from',
      url: configured,
    })
  }
  entries.push({ id: 'local', label: 'Localhost', region: 'This device', url: LOCAL_URL })
  entries.push(...PUBLIC_SERVERS)
  entries.push({ id: CUSTOM, label: 'Another address', url: '' })
  return entries
}

/** The host and port of a `ws://` address, for a row that should read as a place. */
export function hostOf(url: string): string {
  const match = /^wss?:\/\/([^/?#]+)/i.exec(url)
  return match?.[1] ?? url
}

/**
 * Which entry an address is, or the custom one.
 *
 * A remembered address that no longer matches any published entry is not an error: it is a
 * custom address, which is what it was the moment the list it came from changed.
 */
export const entryFor = (url: string, choices: readonly ServerEntry[]): ServerEntry =>
  choices.find((entry) => entry.url === url && entry.id !== CUSTOM) ??
  choices.find((entry) => entry.id === CUSTOM)!

/** What this device remembers about connecting. Both halves are optional and both may be wrong. */
export interface ConnectionPreference {
  name: string
  /** The address last connected to, if any. */
  server?: string
}

const KEY = 'sage.connect.v1'

/** Read what this device last used, or nothing. Every failure lands on nothing. */
export function readConnection(storage: Storage | undefined): ConnectionPreference {
  if (!storage) return { name: '' }
  try {
    const raw: unknown = JSON.parse(storage.getItem(KEY) ?? 'null')
    if (typeof raw !== 'object' || raw === null) return { name: '' }
    const stored = raw as Partial<ConnectionPreference>
    return {
      name: typeof stored.name === 'string' ? stored.name.slice(0, 32) : '',
      ...(typeof stored.server === 'string' && stored.server.length > 0
        ? { server: stored.server }
        : {}),
    }
  } catch {
    return { name: '' }
  }
}

/** Remember it, or carry on without. A device that cannot remember still connects. */
export function writeConnection(
  storage: Storage | undefined,
  preference: ConnectionPreference,
): void {
  try {
    storage?.setItem(KEY, JSON.stringify(preference))
  } catch {
    // Full, disabled, or denied. The choice still applies to this session.
  }
}

/**
 * The address to open with: explicit configuration, then what this device chose, then the
 * default `socket.ts` resolves.
 */
export const initialAddress = (storage: Storage | undefined): string =>
  serverUrlOverride() ?? readConnection(storage).server ?? defaultServerUrl()
