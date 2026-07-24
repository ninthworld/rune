import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import './chrome/tokens.css';
import './chrome/base.css';
import { App } from './App';

const root = createRoot(document.getElementById('root')!);
const fixtureRoute = window.location.pathname === '/fixtures/2.5d';
const fixtureEnabled = import.meta.env.DEV || import.meta.env.VITE_RUNE_FIXTURE_HARNESS === 'true';

if (fixtureRoute && fixtureEnabled) {
  void import('./fixture').then(({ FixtureBattlefield }) => {
    root.render(
      <StrictMode>
        <FixtureBattlefield />
      </StrictMode>,
    );
  });
} else {
  root.render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
