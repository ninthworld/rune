import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import './chrome/tokens.css';
import './chrome/base.css';
import { App } from './App';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
