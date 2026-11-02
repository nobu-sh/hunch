import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import '@hunch/ui/overlay.css';
import './app.css';
import { App } from './App';

const container = document.getElementById('app');
if (!container) throw new Error('Hunch desktop page is missing #app');
createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
