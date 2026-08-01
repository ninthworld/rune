import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import { App } from './App'
import { ArtProvider } from './ui/art'
import './index.css'

const root = document.getElementById('root')
if (!root) throw new Error('missing #root')

createRoot(root).render(
  <StrictMode>
    {/* Outside the app rather than inside a screen: the art preference and its cache belong to
        the device, and the lobby, the deck builder, and the table all draw the same card. It
        holds nothing about the game and survives no message. */}
    <ArtProvider>
      <App />
    </ArtProvider>
  </StrictMode>,
)
